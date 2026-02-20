//! Recursive type visitor that collects all reachable types and their layouts.
//!
//! Implements the Stable MIR [`Visitor`] trait to walk type trees, resolving
//! function ABIs for `FnDef`/`FnPtr`/`Closure` types and explicitly visiting
//! ADT field types (which the default visitor does not descend into).

extern crate rustc_middle;
extern crate rustc_smir;
extern crate stable_mir;

use std::collections::HashSet;
use std::ops::ControlFlow;

use rustc_middle::ty::{List, TyCtxt, TypingEnv};
use rustc_smir::rustc_internal::{self, internal};
use stable_mir::mir::mono::Instance;
use stable_mir::ty::{RigidTy, TyKind};
use stable_mir::visitor::{Visitable, Visitor};

use super::schema::TyMap;

pub(super) struct TyCollector<'tcx> {
    tcx: TyCtxt<'tcx>,
    pub types: TyMap,
    resolved: HashSet<stable_mir::ty::Ty>,
}

impl TyCollector<'_> {
    pub fn new(tcx: TyCtxt<'_>) -> TyCollector {
        TyCollector {
            tcx,
            types: std::collections::HashMap::new(),
            resolved: HashSet::new(),
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
            return ControlFlow::Continue(());
        }

        match ty.kind() {
            TyKind::RigidTy(RigidTy::Closure(def, ref args)) => {
                self.resolved.insert(*ty);
                let instance =
                    Instance::resolve_closure(def, args, stable_mir::ty::ClosureKind::Fn).unwrap();
                self.visit_instance(instance)
            }
            // Break on CoroutineWitnesses, because they aren't expected when getting the layout
            TyKind::RigidTy(RigidTy::CoroutineWitness(..)) => ControlFlow::Break(()),
            TyKind::RigidTy(RigidTy::FnDef(def, ref args)) => {
                self.resolved.insert(*ty);
                let instance = Instance::resolve(def, args).unwrap();
                self.visit_instance(instance)
            }
            TyKind::RigidTy(RigidTy::FnPtr(binder_stable)) => {
                self.resolved.insert(*ty);
                let binder_internal = internal(self.tcx, binder_stable);
                let sig_stable = rustc_internal::stable(
                    self.tcx
                        .fn_abi_of_fn_ptr(
                            TypingEnv::fully_monomorphized()
                                .as_query_input((binder_internal, List::empty())),
                        )
                        .unwrap(),
                );
                let mut inputs_outputs: Vec<stable_mir::ty::Ty> =
                    sig_stable.args.iter().map(|arg_abi| arg_abi.ty).collect();
                inputs_outputs.push(sig_stable.ret.ty);
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
                    let maybe_layout_shape = ty.layout().ok().map(|layout| layout.shape());
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
                        let maybe_layout_shape = ty.layout().ok().map(|layout| layout.shape());
                        self.types.insert(*ty, (ty.kind(), maybe_layout_shape));
                        control
                    }
                    _ => control,
                }
            }
        }
    }
}
