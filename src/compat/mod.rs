//! Compatibility layer for rustc internal APIs.
//!
//! All `extern crate rustc_*` declarations and direct `TyCtxt` queries live
//! here so that toolchain upgrades only need to touch this module (plus
//! `driver.rs`).

pub extern crate rustc_middle;
pub extern crate rustc_monomorphize;
pub extern crate rustc_session;
pub extern crate rustc_smir;
pub extern crate rustc_span;
pub extern crate stable_mir;

// HACK: typically, we would source serde/serde_json separately from the compiler.
//       However, due to issues matching crate versions when we have our own serde
//       in addition to the rustc serde, we force ourselves to use rustc serde.
pub extern crate serde;
pub extern crate serde_json;

pub use rustc_middle as middle;
pub use rustc_middle::ty::TyCtxt;
pub use rustc_smir::rustc_internal;
pub use rustc_smir::rustc_internal::internal;

pub mod bridge;
pub mod mono_collect;
pub mod output;
pub mod spans;
pub mod types;
