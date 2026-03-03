//! Output filename resolution.
//!
//! Wraps `tcx.output_filenames().path(OutputType::Mir)` so that callers
//! don't need to import `rustc_session` directly.

use std::path::PathBuf;

use super::rustc_session::config::{OutFileName, OutputType};
use super::TyCtxt;

/// Resolved output destination for MIR-derived files.
pub enum OutputDest {
    Stdout,
    File(PathBuf),
}

/// Resolve the MIR output path from the compiler session, replacing
/// the extension with the given one.
pub fn mir_output_path(tcx: TyCtxt<'_>, extension: &str) -> OutputDest {
    match tcx.output_filenames(()).path(OutputType::Mir) {
        OutFileName::Stdout => OutputDest::Stdout,
        OutFileName::Real(path) => OutputDest::File(path.with_extension(extension)),
    }
}
