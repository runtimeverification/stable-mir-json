//! Small helpers: name resolution, attribute queries, and collection utilities.

use crate::compat::middle::ty::TyCtxt;
use crate::compat::rustc_span;
use crate::compat::stable_mir;

use std::collections::HashMap;

use rustc_span::symbol;
use stable_mir::mir::mono::{Instance, MonoItem};

pub(crate) fn hash<T: std::hash::Hash>(obj: T) -> u64 {
    use std::hash::Hasher;
    let mut hasher = std::hash::DefaultHasher::new();
    obj.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn take_any<K: Clone + std::hash::Hash + std::cmp::Eq, V>(
    map: &mut HashMap<K, V>,
) -> Option<(K, V)> {
    let key = map.keys().next()?.clone();
    map.remove(&key).map(|val| (key, val))
}

pub(super) fn mono_item_name(tcx: TyCtxt<'_>, item: &MonoItem) -> String {
    crate::compat::mono_collect::mono_item_name(tcx, item)
}

// Possible input: sym::test
pub fn has_attr(tcx: TyCtxt<'_>, item: &stable_mir::CrateItem, attr: symbol::Symbol) -> bool {
    crate::compat::types::has_attr(tcx, item, attr)
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
    crate::compat::bridge::mono_instance(tcx, id)
}
