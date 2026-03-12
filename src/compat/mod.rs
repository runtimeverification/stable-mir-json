//! Compatibility layer for rustc internal APIs.
//!
//! Every direct `rustc_*` import and raw `TyCtxt` query lives inside this
//! module (or one of its submodules). Code outside `compat` (and `driver.rs`)
//! should never touch rustc internals directly; it should go through the
//! types and functions re-exported here instead.
//!
//! The payoff: when a nightly toolchain upgrade moves or renames an internal
//! API, the fix stays inside `compat/` and nothing else needs to change.
//!
//! # Submodules
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`bridge`] | Stable-to-internal conversions (`Instance`, `InstanceKind`, unevaluated consts) |
//! | [`mono_collect`] | Monomorphization collection and symbol naming |
//! | [`output`] | Output filename resolution from the compiler session |
//! | [`spans`] | Span-to-source-location resolution |
//! | [`types`] | Type queries: generics, signatures, discriminants, attributes |
//!
//! # Re-exports
//!
//! The crate-level re-exports below give callers access to the handful of
//! rustc types that inevitably appear in public signatures (`TyCtxt`,
//! `DefId`, etc.) without requiring them to know which rustc crate the
//! type actually lives in.

pub extern crate rustc_demangle;
pub extern crate rustc_middle;
pub extern crate rustc_monomorphize;
pub extern crate rustc_session;
pub extern crate rustc_smir;
pub extern crate rustc_span;
pub extern crate stable_mir;

// We use rustc's vendored serde rather than pulling in our own copy.
// Having two serde versions causes version-mismatch errors when
// serializing types that come from the compiler.
pub extern crate serde;
pub extern crate serde_json;

/// Alias for `rustc_middle`; keeps import paths shorter.
pub use rustc_middle as middle;
/// The compiler's typing context; threaded through most compat functions.
pub use rustc_middle::ty::TyCtxt;
/// Bridge between stable MIR types and rustc internals.
pub use rustc_smir::rustc_internal;
/// Convenience re-export: converts a stable MIR value to its internal rustc
/// counterpart.
pub use rustc_smir::rustc_internal::internal;
/// Rustc's definition identifier. Re-exported so callers outside `compat`
/// don't need to depend on `rustc_span` directly.
pub use rustc_span::def_id::DefId;

pub mod bridge;
pub mod mono_collect;
pub mod output;
pub mod spans;
pub mod types;
