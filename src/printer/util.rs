//! Small utility functions shared across the printer submodules.

extern crate rustc_middle;
extern crate rustc_smir;
extern crate rustc_span;
extern crate stable_mir;

use rustc_middle::ty::TyCtxt;
use rustc_smir::rustc_internal;
use rustc_span::symbol;
use stable_mir::mir::mono::{Instance, MonoItem};

pub fn hash<T: std::hash::Hash>(obj: T) -> u64 {
    use std::hash::Hasher;
    let mut hasher = std::hash::DefaultHasher::new();
    obj.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn take_any<K: Clone + std::hash::Hash + std::cmp::Eq, V>(
    map: &mut std::collections::HashMap<K, V>,
) -> Option<(K, V)> {
    let key = map.keys().next()?.clone();
    map.remove(&key).map(|val| (key, val))
}

pub(super) fn mono_item_name(tcx: TyCtxt<'_>, item: &MonoItem) -> String {
    if let MonoItem::GlobalAsm(data) = item {
        hash(data).to_string()
    } else {
        mono_item_name_int(tcx, &rustc_internal::internal(tcx, item))
    }
}

pub(super) fn mono_item_name_int<'a>(
    tcx: TyCtxt<'a>,
    item: &rustc_middle::mir::mono::MonoItem<'a>,
) -> String {
    item.symbol_name(tcx).name.into()
}

/// Check whether a crate item carries a given attribute (e.g., `sym::test`).
pub fn has_attr(tcx: TyCtxt<'_>, item: &stable_mir::CrateItem, attr: symbol::Symbol) -> bool {
    tcx.has_attr(rustc_internal::internal(tcx, item), attr)
}

pub(super) fn fn_inst_for_ty(ty: stable_mir::ty::Ty, direct_call: bool) -> Option<Instance> {
    ty.kind().fn_def().and_then(|(fn_def, args)| {
        if direct_call {
            Instance::resolve(fn_def, args)
        } else {
            Instance::resolve_for_fn_ptr(fn_def, args)
        }
        .ok()
    })
}

pub(super) fn def_id_to_inst(tcx: TyCtxt<'_>, id: stable_mir::DefId) -> Instance {
    let internal_id = rustc_internal::internal(tcx, id);
    let internal_inst = rustc_middle::ty::Instance::mono(tcx, internal_id);
    rustc_internal::stable(internal_inst)
}
