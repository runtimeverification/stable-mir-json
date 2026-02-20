//! Span-to-source-location resolution.
//!
//! Wraps the `source_map().span_to_location_info()` internal API
//! so that callers don't need to touch `rustc_span` directly.

use super::internal;
use super::rustc_span;
use super::stable_mir;
use super::TyCtxt;
use stable_mir::ty::Span;

pub type SourceData = (String, usize, usize, usize, usize);

/// Resolve a stable MIR span to a (file, lo_line, lo_col, hi_line, hi_col) tuple.
pub fn resolve_span(tcx: TyCtxt<'_>, span: &Span) -> SourceData {
    let span_internal = internal(tcx, span);
    let (source_file, lo_line, lo_col, hi_line, hi_col) =
        tcx.sess.source_map().span_to_location_info(span_internal);
    let file_name = match source_file {
        Some(sf) => sf
            .name
            .display(rustc_span::FileNameDisplayPreference::Remapped)
            .to_string(),
        None => "no-location".to_string(),
    };
    (file_name, lo_line, lo_col, hi_line, hi_col)
}
