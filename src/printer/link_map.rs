//! Link-time function resolution map.
//!
//! Maintains a mapping from function types (optionally qualified by instance kind)
//! to their resolved symbol names. Entries are added from three sources:
//! - `ITEM`: the function appears as a monomorphized item,
//! - `TERM`: the function is called in a `Call` or `Drop` terminator,
//! - `FPTR`: the function is referenced via a `ReifyFnPointer` cast or a
//!   zero-sized FnDef constant.

extern crate rustc_middle;
extern crate rustc_smir;
extern crate stable_mir;

use rustc_middle as middle;
use rustc_middle::ty::TyCtxt;
use rustc_smir::rustc_internal;
use stable_mir::mir::mono::Instance;

use super::schema::{FnSymType, ItemSource, LinkMap, LinkMapKey};

pub(super) type FnSymInfo<'tcx> = (
    stable_mir::ty::Ty,
    middle::ty::InstanceKind<'tcx>,
    FnSymType,
);

pub(super) fn fn_inst_sym<'tcx>(
    tcx: TyCtxt<'tcx>,
    ty: Option<stable_mir::ty::Ty>,
    inst: Option<&Instance>,
) -> Option<FnSymInfo<'tcx>> {
    use FnSymType::*;
    inst.and_then(|inst| {
        let ty = ty.unwrap_or_else(|| inst.ty());
        let kind = ty.kind();
        if kind.fn_def().is_some() {
            let internal_inst = rustc_internal::internal(tcx, inst);
            let sym_type = if inst.is_empty_shim() {
                NoOpSym(String::from(""))
            } else if let Some(intrinsic_name) = inst.intrinsic_name() {
                IntrinsicSym(intrinsic_name)
            } else {
                NormalSym(inst.mangled_name())
            };
            Some((ty, internal_inst.def, sym_type))
        } else {
            None
        }
    })
}

pub(super) fn is_reify_shim(kind: &middle::ty::InstanceKind<'_>) -> bool {
    matches!(kind, middle::ty::InstanceKind::ReifyShim(..))
}

pub(super) fn update_link_map<'tcx>(
    link_map: &mut LinkMap<'tcx>,
    fn_sym: Option<FnSymInfo<'tcx>>,
    source: ItemSource,
) {
    let Some((ty, kind, name)) = fn_sym else {
        return;
    };
    let new_val = (source, name.clone());
    let key = if super::link_instance_enabled() {
        LinkMapKey(ty, Some(kind))
    } else {
        LinkMapKey(ty, None)
    };
    if let Some(curr_val) = link_map.get_mut(&key) {
        if curr_val.1 != new_val.1 {
            if !super::link_instance_enabled() {
                // When LINK_INST is disabled, prefer Item over ReifyShim.
                // ReifyShim has no body in items, so Item is more useful.
                if is_reify_shim(&kind) {
                    // New entry is ReifyShim, existing is Item → skip
                    return;
                }
                // New entry is Item, existing is ReifyShim → replace
                curr_val.1 = name;
                curr_val.0 .0 |= new_val.0 .0;
                return;
            }
            panic!(
                "Added inconsistent entries into link map! {:?} -> {:?}, {:?}",
                (ty, ty.kind().fn_def(), &kind),
                curr_val.1,
                new_val.1
            );
        }
        curr_val.0 .0 |= new_val.0 .0;
        debug_log_println!(
            "Regenerated link map entry: {:?}:{:?} -> {:?}",
            &key,
            key.0.kind().fn_def(),
            new_val
        );
    } else {
        debug_log_println!(
            "Generated link map entry from call: {:?}:{:?} -> {:?}",
            &key,
            key.0.kind().fn_def(),
            new_val
        );
        link_map.insert(key, new_val);
    }
}
