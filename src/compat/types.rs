//! Type queries (generics, fn sigs, discriminants, attrs).
//!
//! Wraps `tcx.generics_of()`, `tcx.predicates_of()`, `tcx.fn_sig()`,
//! `tcx.optimized_mir()`, `tcx.def_kind()`, `tcx.type_of()`,
//! `tcx.has_attr()`, `adt.discriminants(tcx)`, and `tcx.fn_abi_of_fn_ptr()`.

use super::middle;
use super::middle::ty::{EarlyBinder, FnSig, GenericArgs, List, Ty, TypeFoldable, TypingEnv};
use super::rustc_internal::{self, internal};
use super::rustc_span;
use super::stable_mir;
use super::TyCtxt;
use rustc_span::def_id::DefId;

/// Collect generics/predicates chain for a DefId, walking parent scopes.
pub fn generic_data(tcx: TyCtxt<'_>, id: DefId) -> Vec<(String, String)> {
    let mut v = Vec::new();
    let mut next_id = Some(id);
    while let Some(curr_id) = next_id {
        let params = tcx.generics_of(curr_id);
        let preds = tcx.predicates_of(curr_id);
        if params.parent != preds.parent {
            panic!("Generics and predicates parent ids are distinct");
        }
        v.push((format!("{:#?}", params), format!("{:#?}", preds)));
        next_id = params.parent;
    }
    v.reverse();
    v
}

/// Unwrap an `EarlyBinder` in a default manner; panic on error.
pub fn default_unwrap_early_binder<'tcx, T>(
    tcx: TyCtxt<'tcx>,
    id: DefId,
    v: EarlyBinder<'tcx, T>,
) -> T
where
    T: TypeFoldable<TyCtxt<'tcx>>,
{
    let v_copy = v.clone();
    let body = tcx.optimized_mir(id);
    match tcx.try_instantiate_and_normalize_erasing_regions(
        GenericArgs::identity_for_item(tcx, id),
        body.typing_env(tcx),
        v,
    ) {
        Ok(res) => res,
        Err(err) => {
            println!("{:?}", err);
            v_copy.skip_binder()
        }
    }
}

/// Pretty-print a type, resolving FnDef signatures via `tcx.fn_sig()`.
pub fn print_type<'tcx>(tcx: TyCtxt<'tcx>, id: DefId, ty: EarlyBinder<'tcx, Ty<'tcx>>) -> String {
    let kind: &middle::ty::TyKind = ty.skip_binder().kind();
    if let middle::ty::TyKind::FnDef(fun_id, args) = kind {
        let sig0 = tcx.fn_sig(fun_id);
        let body = tcx.optimized_mir(id);
        let sig1 = match tcx.try_instantiate_and_normalize_erasing_regions(
            args,
            body.typing_env(tcx),
            sig0,
        ) {
            Ok(res) => res,
            Err(err) => {
                println!("{:?}", err);
                sig0.skip_binder()
            }
        };
        let sig2: FnSig<'_> = tcx.instantiate_bound_regions_with_erased(sig1);
        format!("\nTyKind(FnDef): {:#?}", sig2)
    } else {
        let kind = default_unwrap_early_binder(tcx, id, ty);
        format!("\nTyKind: {:#?}", kind)
    }
}

/// Query the def_kind, def_path, and type_of for a DefId (debug info).
pub fn get_def_info(tcx: TyCtxt<'_>, id: DefId) -> (String, String, String) {
    (
        format!("{:#?}", tcx.def_kind(id)),
        tcx.def_path_str(id),
        print_type(tcx, id, tcx.type_of(id)),
    )
}

/// Check whether a CrateItem has a given attribute.
pub fn has_attr(
    tcx: TyCtxt<'_>,
    item: &stable_mir::CrateItem,
    attr: rustc_span::symbol::Symbol,
) -> bool {
    tcx.has_attr(rustc_internal::internal(tcx, item), attr)
}

/// Collect discriminant values for an ADT (enum) by going through internals.
pub fn adt_discriminants(tcx: TyCtxt<'_>, adt_def: stable_mir::ty::AdtDef) -> Vec<u128> {
    let adt_internal = rustc_internal::internal(tcx, adt_def);
    adt_internal
        .discriminants(tcx)
        .map(|(_, discr)| discr.val)
        .collect()
}

/// Resolve the ABI of a function pointer type (via `tcx.fn_abi_of_fn_ptr`).
pub fn fn_ptr_abi(
    tcx: TyCtxt<'_>,
    binder_stable: stable_mir::ty::PolyFnSig,
) -> stable_mir::abi::FnAbi {
    let binder_internal = internal(tcx, binder_stable);
    rustc_internal::stable(
        tcx.fn_abi_of_fn_ptr(
            TypingEnv::fully_monomorphized().as_query_input((binder_internal, List::empty())),
        )
        .unwrap(),
    )
}

/// Convert a stable DefId to an internal DefId.
pub fn internal_def_id(tcx: TyCtxt<'_>, id: stable_mir::DefId) -> DefId {
    rustc_internal::internal(tcx, id)
}

/// Get the stable crate ID for the local crate.
pub fn local_crate_id(tcx: TyCtxt<'_>) -> u64 {
    tcx.stable_crate_id(rustc_span::def_id::LOCAL_CRATE)
        .as_u64()
}
