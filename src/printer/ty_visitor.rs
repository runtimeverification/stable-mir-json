//! Recursive type visitor for collecting reachable types with layout info.
//!
//! [`TyCollector`] implements `stable_mir::visitor::Visitor` and traverses
//! type trees, recording each relevant type along with its `TyKind` and
//! `LayoutShape`. These collected types are later transformed into
//! [`TypeMetadata`](super::schema::TypeMetadata) entries in the final output.
//! Note that some special kinds (function definitions/pointers and coroutine
//! witnesses) are traversed only to gather the types they reference and are
//! not themselves stored as entries in the type map.

use crate::compat::middle::ty::TyCtxt;
use crate::compat::stable_mir;

use std::collections::{HashMap, HashSet};
use std::ops::ControlFlow;

use stable_mir::mir::mono::Instance;
use stable_mir::ty::{RigidTy, TyKind};
use stable_mir::visitor::{Visitable, Visitor};

use super::schema::TyMap;
use super::tracer::{ty_kind_tag, TraceEvent};

pub(super) struct TyCollector<'tcx> {
    tcx: TyCtxt<'tcx>,
    pub types: TyMap,
    resolved: HashSet<stable_mir::ty::Ty>,
    /// When tracing is enabled, newly collected types are buffered here.
    /// The caller (BodyAnalyzer::visit_ty) drains this buffer after each
    /// ty.visit() call and copies events into the main Tracer with the
    /// correct item context. This sidesteps the double-&mut problem.
    pub trace_buffer: Option<Vec<TraceEvent>>,
}

impl TyCollector<'_> {
    pub fn new(tcx: TyCtxt<'_>, trace: bool) -> TyCollector {
        TyCollector {
            tcx,
            types: HashMap::new(),
            resolved: HashSet::new(),
            trace_buffer: if trace { Some(Vec::new()) } else { None },
        }
    }
}

impl TyCollector<'_> {
    #[inline(always)]
    fn visit_instance(&mut self, instance: Instance) -> ControlFlow<<Self as Visitor>::Break> {
        let fn_abi = instance.fn_abi().unwrap();
        let mut inputs_outputs: Vec<stable_mir::ty::Ty> =
            fn_abi.args.iter().map(|arg_abi| arg_abi.ty).collect();
        inputs_outputs.push(fn_abi.ret.ty);
        inputs_outputs.super_visit(self)
    }
}

impl Visitor for TyCollector<'_> {
    type Break = ();

    fn visit_ty(&mut self, ty: &stable_mir::ty::Ty) -> ControlFlow<Self::Break> {
        if self.types.contains_key(ty) || self.resolved.contains(ty) {
            // The type is already collected; skip recursion (which also
            // serves as a cycle breaker for self-referential types). We
            // still emit a trace event so that *every* body that references
            // a type gets credited, not just whichever body happened to be
            // walked first.
            //
            // Note: skipping recursion means *nested* types (e.g. the
            // Foreign pointee inside a RawPtr) are only traced from the
            // first body that visits the outer type. Combined with
            // non-deterministic body walk order (HashMap iteration in
            // collect_and_analyze_items), this makes provenance attribution
            // for nested types non-deterministic. The trace report's
            // "stdlib only" / "user code" annotation for such types may
            // vary between runs.
            if let Some(buf) = &mut self.trace_buffer {
                buf.push(TraceEvent::TypeCollected {
                    item: String::new(),
                    location: None,
                    ty_kind: ty_kind_tag(&ty.kind()).to_string(),
                });
            }
            return ControlFlow::Continue(());
        }

        match ty.kind() {
            TyKind::RigidTy(RigidTy::Closure(def, ref args)) => {
                self.resolved.insert(*ty);
                let instance =
                    Instance::resolve_closure(def, args, stable_mir::ty::ClosureKind::Fn).unwrap();
                let control = self.visit_instance(instance);
                // Mirror other branches: record closure Ty only when traversal succeeds.
                if matches!(control, ControlFlow::Continue(_)) {
                    let kind = ty.kind();
                    let maybe_layout_shape = ty.layout().ok().map(|layout| layout.shape());
                    if let Some(buf) = &mut self.trace_buffer {
                        buf.push(TraceEvent::TypeCollected {
                            item: String::new(),
                            location: None,
                            ty_kind: ty_kind_tag(&kind).to_string(),
                        });
                    }
                    self.types.insert(*ty, (kind, maybe_layout_shape));
                }
                control
            }
            // Break on CoroutineWitnesses, because they aren't expected when getting the layout
            TyKind::RigidTy(RigidTy::CoroutineWitness(..)) => {
                debug_log_println!("DEBUG: TyCollector skipping CoroutineWitness: {:?}", ty);
                ControlFlow::Break(())
            }
            TyKind::RigidTy(RigidTy::FnDef(def, ref args)) => {
                self.resolved.insert(*ty);
                if let Some(buf) = &mut self.trace_buffer {
                    buf.push(TraceEvent::TypeCollected {
                        item: String::new(),
                        location: None,
                        ty_kind: "FnDef".to_string(),
                    });
                }
                let instance = Instance::resolve(def, args).unwrap();
                self.visit_instance(instance)
            }
            TyKind::RigidTy(RigidTy::FnPtr(binder_stable)) => {
                self.resolved.insert(*ty);
                if let Some(buf) = &mut self.trace_buffer {
                    buf.push(TraceEvent::TypeCollected {
                        item: String::new(),
                        location: None,
                        ty_kind: "FnPtr".to_string(),
                    });
                }
                let fn_abi = crate::compat::types::fn_ptr_abi(self.tcx, binder_stable);
                let mut inputs_outputs: Vec<stable_mir::ty::Ty> =
                    fn_abi.args.iter().map(|arg_abi| arg_abi.ty).collect();
                inputs_outputs.push(fn_abi.ret.ty);
                inputs_outputs.super_visit(self)
            }
            // The visitor won't collect field types for ADTs, therefore doing it explicitly
            TyKind::RigidTy(RigidTy::Adt(adt_def, args)) => {
                let fields = adt_def
                    .variants()
                    .iter()
                    .flat_map(|v| v.fields())
                    .map(|f| f.ty_with_args(&args))
                    .collect::<Vec<_>>();

                let control = ty.super_visit(self);
                if matches!(control, ControlFlow::Continue(_)) {
                    let kind = ty.kind();
                    let maybe_layout_shape = ty.layout().ok().map(|layout| layout.shape());
                    if let Some(buf) = &mut self.trace_buffer {
                        buf.push(TraceEvent::TypeCollected {
                            item: String::new(), // filled by caller
                            location: None,      // filled by caller
                            ty_kind: ty_kind_tag(&kind).to_string(),
                        });
                    }
                    self.types.insert(*ty, (kind, maybe_layout_shape));
                    fields.super_visit(self)
                } else {
                    control
                }
            }
            _ => {
                let control = ty.super_visit(self);
                match control {
                    ControlFlow::Continue(_) => {
                        let kind = ty.kind();
                        let maybe_layout_shape = ty.layout().ok().map(|layout| layout.shape());
                        if let Some(buf) = &mut self.trace_buffer {
                            buf.push(TraceEvent::TypeCollected {
                                item: String::new(), // filled by caller
                                location: None,      // filled by caller
                                ty_kind: ty_kind_tag(&kind).to_string(),
                            });
                        }
                        self.types.insert(*ty, (kind, maybe_layout_shape));
                        control
                    }
                    _ => control,
                }
            }
        }
    }
}
