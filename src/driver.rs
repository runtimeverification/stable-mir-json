//! This module provides a compiler driver such that:
//!
//! 1.  the rustc compiler context is available
//! 2.  the rustc `stable_mir` APIs are available
//!
//! It exports a single function:
//!
//! ```rust,ignore
//! stable_mir_driver(args: &Vec<String>, callback_fn: fn (TyCtxt) -> () )
//! ```
//!
//! Calling this function is essentially equivalent to the following macro call:
//!
//! ```rust,ignore
//! rustc_internal::run_with_tcx!( args, callback_fn );
//! ```
//!
//! However, we prefer a non-macro version for clarity and build simplicity.

extern crate rustc_driver;
extern crate rustc_interface;
extern crate rustc_middle;
#[cfg(smir_crate_renamed)]
extern crate rustc_public_bridge as rustc_smir;
extern crate rustc_session;
#[cfg(not(smir_crate_renamed))]
extern crate rustc_smir;
use rustc_driver::Compilation;
use rustc_interface::interface::Compiler;
use rustc_middle::ty::TyCtxt;
// In nightlies >= 2025-07-08, rustc_internal moved from rustc_smir to stable_mir.
// Both paths are re-exported through compat/mod.rs; driver.rs is the exception
// that imports rustc crates directly, so we cfg-gate the source crate here too.
// See build.rs BREAKPOINTS table.
#[cfg(smir_rustc_internal_moved)]
use crate::compat::rustc_internal;
#[cfg(not(smir_rustc_internal_moved))]
use rustc_smir::rustc_internal;

struct StableMirCallbacks {
    callback_fn: fn(TyCtxt) -> (),
}

impl rustc_driver::Callbacks for StableMirCallbacks {
    fn after_analysis(&mut self, _compiler: &Compiler, tcx: TyCtxt) -> Compilation {
        let _ = rustc_internal::run(tcx, || (self.callback_fn)(tcx));

        Compilation::Continue
    }
}

pub fn stable_mir_driver(args_outer: &[String], callback_fn: fn(TyCtxt) -> ()) {
    let mut callbacks = StableMirCallbacks { callback_fn };
    let early_dcx =
        rustc_session::EarlyDiagCtxt::new(rustc_session::config::ErrorOutputType::default());
    rustc_driver::init_rustc_env_logger(&early_dcx);
    // In nightlies >= 2025-01-24, RunCompiler was replaced with a free
    // function run_compiler(). See build.rs BREAKPOINTS table.
    #[cfg(not(smir_has_run_compiler_fn))]
    let _ = rustc_driver::RunCompiler::new(args_outer, &mut callbacks).run();
    #[cfg(smir_has_run_compiler_fn)]
    rustc_driver::run_compiler(args_outer, &mut callbacks);
}
