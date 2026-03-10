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
use std::panic::{catch_unwind, set_hook, take_hook, AssertUnwindSafe};

use stable_mir::mir::mono::Instance;
use stable_mir::ty::{RigidTy, TyKind};
use stable_mir::visitor::{Visitable, Visitor};

use super::schema::TyMap;

/// A layout computation that panicked inside rustc.
pub(super) struct LayoutPanic {
    pub ty: stable_mir::ty::Ty,
    pub message: String,
}

/// Attempt to get a type's layout, catching any rustc-internal panics.
///
/// Some types (e.g., those involving `dyn Trait` in certain positions) cause
/// rustc's layout computation to panic rather than returning an error. We
/// catch those panics here so the visitor can continue; the caller gets
/// `Ok(Some(shape))` on success, `Ok(None)` when layout returns `Err`, or
/// `Err(message)` when rustc panicked.
fn try_layout_shape(
    ty: &stable_mir::ty::Ty,
) -> Result<Option<stable_mir::abi::LayoutShape>, String> {
    // Temporarily suppress the default panic hook so caught panics don't
    // spray backtraces to stderr; we report them in our own summary.
    let prev_hook = take_hook();
    set_hook(Box::new(|_| {}));
    let result = catch_unwind(AssertUnwindSafe(|| ty.layout().ok().map(|l| l.shape())));
    set_hook(prev_hook);

    match result {
        Ok(shape) => Ok(shape),
        Err(payload) => {
            let message = if let Some(s) = payload.downcast_ref::<&str>() {
                (*s).to_string()
            } else if let Some(s) = payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "(non-string panic payload)".to_string()
            };
            Err(message)
        }
    }
}

pub(super) struct TyCollector<'tcx> {
    tcx: TyCtxt<'tcx>,
    pub types: TyMap,
    pub layout_panics: Vec<LayoutPanic>,
    resolved: HashSet<stable_mir::ty::Ty>,
}

impl TyCollector<'_> {
    pub fn new(tcx: TyCtxt<'_>) -> TyCollector<'_> {
        TyCollector {
            tcx,
            types: HashMap::new(),
            layout_panics: Vec::new(),
            resolved: HashSet::new(),
        }
    }
}

impl TyCollector<'_> {
    /// Get layout for `ty`, recording a [`LayoutPanic`] if rustc panics.
    fn layout_shape_or_record(
        &mut self,
        ty: &stable_mir::ty::Ty,
    ) -> Option<stable_mir::abi::LayoutShape> {
        match try_layout_shape(ty) {
            Ok(shape) => shape,
            Err(message) => {
                self.layout_panics.push(LayoutPanic { ty: *ty, message });
                None
            }
        }
    }

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
                    let maybe_layout_shape = self.layout_shape_or_record(ty);
                    self.types.insert(*ty, (ty.kind(), maybe_layout_shape));
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
                let instance = Instance::resolve(def, args).unwrap();
                self.visit_instance(instance)
            }
            TyKind::RigidTy(RigidTy::FnPtr(binder_stable)) => {
                self.resolved.insert(*ty);
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
                    let maybe_layout_shape = self.layout_shape_or_record(ty);
                    self.types.insert(*ty, (ty.kind(), maybe_layout_shape));
                    fields.super_visit(self)
                } else {
                    control
                }
            }
            _ => {
                let control = ty.super_visit(self);
                match control {
                    ControlFlow::Continue(_) => {
                        let maybe_layout_shape = self.layout_shape_or_record(ty);
                        self.types.insert(*ty, (ty.kind(), maybe_layout_shape));
                        control
                    }
                    _ => control,
                }
            }
        }
    }
}
