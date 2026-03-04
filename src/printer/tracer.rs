//! Pipeline trace instrumentation.
//!
//! When `TRACE=1` is set, the pipeline emits a `*.smir.trace.json` file
//! capturing the complete story of how each entry ended up in the output:
//! which item's body was being analyzed, which callback fired, what it
//! received, and what it produced.
//!
//! The trace is a flat, chronological list of [`TraceEvent`] values, one per
//! observable pipeline step. Events carry domain-language payloads (item
//! names, type tags, symbol kinds) rather than raw compiler ids, so they're
//! readable without knowing stable MIR internals.

use crate::compat::middle::ty::TyCtxt;
use crate::compat::serde;
use crate::compat::stable_mir;

use serde::Serialize;

use stable_mir::ty::TyKind;

use super::schema::FnSymType;

/// Source range of the MIR statement that triggered a trace event.
///
/// Carries both endpoints so a visualization tool (e.g. presenterm) can
/// highlight the exact source region associated with each event.
#[derive(Serialize, Clone)]
pub(super) struct SourceLocation {
    pub file: String,
    pub line: usize,
    pub col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

/// Resolve a MIR visitor `Location` to a `SourceLocation` via the compiler's source map.
pub(super) fn resolve_location(
    tcx: TyCtxt<'_>,
    loc: &stable_mir::mir::visit::Location,
) -> SourceLocation {
    let (file, line, col, end_line, end_col) = crate::compat::spans::resolve_span(tcx, &loc.span());
    SourceLocation {
        file,
        line,
        col,
        end_line,
        end_col,
    }
}

/// Snapshot of collection sizes, used for before/after deltas.
#[derive(Clone, Serialize)]
pub(super) struct CollectionSnapshot {
    pub link_map_count: usize,
    pub alloc_count: usize,
    pub type_count: usize,
    pub span_count: usize,
}

/// A single observable pipeline step.
#[derive(Serialize)]
#[serde(tag = "event")]
pub(super) enum TraceEvent {
    /// Phase 1: a monomorphized item was discovered.
    ItemDiscovered { name: String, source: &'static str },

    /// Phase 2 bookend: beginning body analysis for an item.
    BodyWalkStarted {
        item: String,
        before: CollectionSnapshot,
    },

    /// Phase 2 bookend: finished body analysis for an item.
    BodyWalkFinished {
        item: String,
        after: CollectionSnapshot,
    },

    /// visit_terminator: resolved a Call terminator to a function instance.
    FunctionCallResolved {
        item: String,
        location: SourceLocation,
        callee_ty: String,
        sym_kind: &'static str,
        sym_name: String,
    },

    /// visit_terminator: resolved a Drop terminator to drop glue.
    DropGlueResolved {
        item: String,
        location: SourceLocation,
        drop_ty: String,
        sym_kind: &'static str,
        sym_name: String,
    },

    /// visit_rvalue: detected a ReifyFnPointer cast.
    ReifyFnPointerResolved {
        item: String,
        location: SourceLocation,
        fn_ty: String,
        sym_kind: &'static str,
        sym_name: String,
    },

    /// visit_mir_const: walked an Allocated constant's provenance.
    AllocationCollected {
        item: String,
        location: SourceLocation,
        alloc_ty: String,
        allocs_before: usize,
        allocs_after: usize,
    },

    /// visit_mir_const: a ZeroSized FnDef used as a value.
    FnDefAsValue {
        item: String,
        location: SourceLocation,
        fn_ty: String,
    },

    /// visit_mir_const: an unevaluated constant was discovered.
    UnevaluatedConstDiscovered {
        item: String,
        location: SourceLocation,
        const_name: String,
    },

    /// visit_ty (via TyCollector): a type was collected.
    TypeCollected {
        item: String,
        location: Option<SourceLocation>,
        ty_kind: String,
    },

    /// visit_span: a new span was resolved.
    SpanResolved {
        item: String,
        file: String,
        line: usize,
    },

    /// Phase 3: assembly started (summary).
    AssemblyStarted {
        total_items: usize,
        total_functions: usize,
        total_allocs: usize,
        total_types: usize,
        total_spans: usize,
    },
}

/// Accumulates trace events during the pipeline.
pub(super) struct Tracer {
    pub events: Vec<TraceEvent>,
    pub current_item: Option<String>,
}

impl Tracer {
    pub fn new() -> Self {
        Tracer {
            events: Vec::new(),
            current_item: None,
        }
    }

    pub fn push(&mut self, event: TraceEvent) {
        self.events.push(event);
    }

    /// The current item name, or a fallback for events outside a body walk.
    pub fn item_name(&self) -> String {
        self.current_item
            .clone()
            .unwrap_or_else(|| "<unknown>".to_string())
    }
}

/// Short tag for a TyKind (e.g. "Adt", "FnDef", "Ref").
pub(super) fn ty_kind_tag(kind: &TyKind) -> &'static str {
    match kind {
        TyKind::RigidTy(rigid) => rigid_ty_tag(rigid),
        TyKind::Alias(..) => "Alias",
        TyKind::Param(..) => "Param",
        TyKind::Bound(..) => "Bound",
    }
}

fn rigid_ty_tag(ty: &stable_mir::ty::RigidTy) -> &'static str {
    use stable_mir::ty::RigidTy::*;
    match ty {
        Bool => "Bool",
        Char => "Char",
        Int(_) => "Int",
        Uint(_) => "Uint",
        Float(_) => "Float",
        Adt(..) => "Adt",
        Foreign(_) => "Foreign",
        Str => "Str",
        Array(..) => "Array",
        Slice(_) => "Slice",
        RawPtr(..) => "RawPtr",
        Ref(..) => "Ref",
        FnDef(..) => "FnDef",
        FnPtr(..) => "FnPtr",
        Closure(..) => "Closure",
        Coroutine(..) => "Coroutine",
        Dynamic(..) => "Dynamic",
        Never => "Never",
        Tuple(_) => "Tuple",
        CoroutineWitness(..) => "CoroutineWitness",
        Pat(..) => "Pat",
    }
}

/// Classify a FnSymType into a short tag.
pub(super) fn sym_kind_str(sym: &FnSymType) -> &'static str {
    match sym {
        FnSymType::NoOpSym(_) => "no_op",
        FnSymType::IntrinsicSym(_) => "intrinsic",
        FnSymType::NormalSym(_) => "normal",
    }
}

/// Extract the name string from a FnSymType.
pub(super) fn sym_name(sym: &FnSymType) -> &str {
    match sym {
        FnSymType::NoOpSym(s) | FnSymType::IntrinsicSym(s) | FnSymType::NormalSym(s) => s,
    }
}
