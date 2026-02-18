//! MIR graph generation module.
//!
//! This module provides functionality to generate graph visualizations
//! of Rust's MIR in various formats (DOT, D2).

use std::fs::File;
use std::io::{self, Write};

use crate::compat::middle::ty::TyCtxt;
use crate::compat::output::{mir_output_path, OutputDest};
use crate::printer::collect_smir;

// Sub-modules
pub mod context;
pub mod index;
pub mod output;
pub mod util;

// Re-exports for convenience
pub use context::GraphContext;
pub use index::{AllocEntry, AllocIndex, AllocKind, TypeIndex};
pub use util::GraphLabelString;

// =============================================================================
// Lang Start Filtering
// =============================================================================

pub(crate) fn skip_lang_start() -> bool {
    use std::sync::OnceLock;
    static VAR: OnceLock<bool> = OnceLock::new();
    *VAR.get_or_init(|| std::env::var("SKIP_LANG_START").is_ok())
}

// =============================================================================
// Entry Points
// =============================================================================

/// Entry point to write the DOT file
pub fn emit_dotfile(tcx: TyCtxt<'_>) {
    if skip_lang_start() {
        println!("SKIP_LANG_START is set");
    }

    let smir_dot = collect_smir(tcx).to_dot_file();

    match mir_output_path(tcx, "smir.dot") {
        OutputDest::Stdout => {
            write!(io::stdout(), "{}", smir_dot).expect("Failed to write smir.dot");
        }
        OutputDest::File(path) => {
            let mut b = io::BufWriter::new(
                File::create(&path)
                    .unwrap_or_else(|e| panic!("Failed to create {}: {}", path.display(), e)),
            );
            write!(b, "{}", smir_dot).expect("Failed to write smir.dot");
        }
    }
}

/// Entry point to write the D2 file
pub fn emit_d2file(tcx: TyCtxt<'_>) {
    let smir_d2 = collect_smir(tcx).to_d2_file();

    match mir_output_path(tcx, "smir.d2") {
        OutputDest::Stdout => {
            write!(io::stdout(), "{}", smir_d2).expect("Failed to write smir.d2");
        }
        OutputDest::File(path) => {
            let mut b = io::BufWriter::new(
                File::create(&path)
                    .unwrap_or_else(|e| panic!("Failed to create {}: {}", path.display(), e)),
            );
            write!(b, "{}", smir_d2).expect("Failed to write smir.d2");
        }
    }
}
