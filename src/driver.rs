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

extern crate rustc_middle;
extern crate rustc_driver;
extern crate rustc_interface;
extern crate rustc_smir;
extern crate rustc_session;
use rustc_middle::ty::TyCtxt;
use rustc_driver::Compilation;
use rustc_interface::{interface::Compiler, Queries};
use rustc_smir::rustc_internal;

struct StableMirCallbacks {
  callback_fn: fn (TyCtxt) -> (),
}

impl rustc_driver::Callbacks for StableMirCallbacks {
    fn after_analysis<'tcx>(
        &mut self,
        _compiler: &Compiler,
        queries: &'tcx Queries<'tcx>,
    ) -> Compilation {

        let _q = queries
            .global_ctxt()
            .unwrap()
            .get_mut()
            .enter(|tcx| {
                let _ = rustc_internal::run(tcx, || (self.callback_fn)(tcx));
            });

        Compilation::Continue
    }
}

pub fn stable_mir_driver(args_outer: &Vec<String>, callback_fn: fn (TyCtxt) -> ()) {
    let mut callbacks = StableMirCallbacks { callback_fn };
    let early_dcx = rustc_session::EarlyDiagCtxt::new(rustc_session::config::ErrorOutputType::default());
    rustc_driver::init_rustc_env_logger(&early_dcx);
    let _ = rustc_driver::RunCompiler::new(args_outer, &mut callbacks).run();
}
