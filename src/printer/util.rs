//! Small helpers: name resolution, attribute queries, and collection utilities.

use crate::compat::stable_mir;

use std::collections::HashMap;

use stable_mir::mir::mono::Instance;

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

