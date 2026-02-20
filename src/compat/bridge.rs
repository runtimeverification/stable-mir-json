//! Stable<->internal conversions and OpaqueInstanceKind.
//!
//! This module wraps rustc-internal instance kind queries behind an owned,
//! lifetime-free representation so that the rest of the codebase doesn't
//! need to carry `'tcx` lifetimes for link map keys.

use std::hash::{Hash, Hasher};

use super::middle;
use super::rustc_internal;
use super::stable_mir;
use super::TyCtxt;
use stable_mir::mir::mono::Instance;

/// Owned, lifetime-free replacement for `middle::ty::InstanceKind<'tcx>`.
///
/// The actual `InstanceKind` usage is narrow:
/// 1. Serialized as `format!("{:?}", kind)` (a Debug string)
/// 2. Checked via `is_reify_shim()` (a single pattern match)
/// 3. Used for `Hash`/`Eq` in `LinkMapKey` (map keying)
///
/// This struct captures all three via owned data, eliminating the need
/// to propagate the `'tcx` lifetime through `LinkMapKey`, `FnSymInfo`,
/// `SmirJson`, and `SmirJsonDebugInfo`.
#[derive(Clone, Debug)]
pub struct OpaqueInstanceKind {
    debug_repr: String,
    pub is_reify_shim: bool,
}

impl PartialEq for OpaqueInstanceKind {
    fn eq(&self, other: &Self) -> bool {
        self.debug_repr == other.debug_repr
    }
}

impl Eq for OpaqueInstanceKind {}

impl Hash for OpaqueInstanceKind {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.debug_repr.hash(state);
    }
}

impl std::fmt::Display for OpaqueInstanceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.debug_repr)
    }
}

/// Create a monomorphized Instance from a stable DefId (wraps `Instance::mono`).
pub fn mono_instance(tcx: TyCtxt<'_>, id: stable_mir::DefId) -> Instance {
    let internal_id = rustc_internal::internal(tcx, id);
    let internal_inst = middle::ty::Instance::mono(tcx, internal_id);
    rustc_internal::stable(internal_inst)
}

/// Resolve an unevaluated constant into a (MonoItem, symbol_name) pair.
///
/// This wraps `middle::ty::Instance::try_resolve` and the internal mono item
/// symbol name resolution, keeping those internal APIs out of printer.rs.
pub fn resolve_unevaluated_const(
    tcx: TyCtxt<'_>,
    def_id: stable_mir::DefId,
    args: stable_mir::ty::GenericArgs,
) -> (stable_mir::mir::mono::MonoItem, String) {
    use super::middle::ty::TypingEnv;
    let internal_def = rustc_internal::internal(tcx, def_id);
    let internal_args = rustc_internal::internal(tcx, args);
    let maybe_inst = middle::ty::Instance::try_resolve(
        tcx,
        TypingEnv::post_analysis(tcx, internal_def),
        internal_def,
        internal_args,
    );
    let inst = maybe_inst
        .ok()
        .flatten()
        .unwrap_or_else(|| panic!("Failed to resolve mono item for def {:?}", def_id));
    let internal_mono_item = middle::mir::mono::MonoItem::Fn(inst);
    let item_name = crate::compat::mono_collect::mono_item_name_int(tcx, &internal_mono_item);
    (rustc_internal::stable(internal_mono_item), item_name)
}

/// Extract an `OpaqueInstanceKind` from a stable MIR `Instance` by
/// converting to the internal representation and capturing the debug
/// string and reify-shim flag.
pub fn instance_kind(tcx: TyCtxt<'_>, inst: &Instance) -> OpaqueInstanceKind {
    let internal_inst = rustc_internal::internal(tcx, inst);
    let kind = internal_inst.def;
    OpaqueInstanceKind {
        debug_repr: format!("{:?}", kind),
        is_reify_shim: matches!(kind, middle::ty::InstanceKind::ReifyShim(..)),
    }
}
