//! MIR graph generation module.
//!
//! This module provides functionality to generate graph visualizations
//! of Rust's MIR in various formats (DOT, D2).

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::{self, Write};

use crate::compat::middle::ty::TyCtxt;
use crate::compat::output::{mir_output_path, OutputDest};
use crate::compat::stable_mir::mir::{ConstOperand, Operand, TerminatorKind};
use crate::printer::{collect_smir, Item, MonoItemKind};

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
// Item Filtering
// =============================================================================

/// A predicate that identifies items to exclude from graph output.
/// Each variant corresponds to an environment variable that enables it.
pub(crate) enum ItemFilter {
    /// Exclude `std::rt::lang_start` and items only reachable through it.
    /// Enabled by `SKIP_LANG_START=1`.
    LangStart,
}

impl ItemFilter {
    /// Return the set of filters currently enabled via environment variables.
    pub fn enabled() -> Vec<ItemFilter> {
        let mut filters = Vec::new();
        if std::env::var("SKIP_LANG_START").is_ok() {
            filters.push(ItemFilter::LangStart);
        }
        filters
    }

    /// Compute the set of symbol names this filter wants to exclude.
    pub fn compute_exclusions(&self, items: &[Item], ctx: &GraphContext) -> HashSet<String> {
        match self {
            ItemFilter::LangStart => compute_lang_start_exclusions(items, ctx),
        }
    }

    /// Apply all enabled filters: collect exclusions, then prune both
    /// `items` and `ctx.functions` in one pass.
    ///
    /// After this call, `ctx.resolve_call_target()` returns `None` for any
    /// excluded function, so renderers don't need a separate exclusion set.
    pub fn apply_all(items: &mut Vec<Item>, ctx: &mut GraphContext) {
        let filters = Self::enabled();
        if filters.is_empty() {
            return;
        }
        let mut excluded = HashSet::new();
        for filter in &filters {
            excluded.extend(filter.compute_exclusions(items, ctx));
        }
        items.retain(|i| !excluded.contains(&i.symbol_name));
        ctx.functions.retain(|_, name| !excluded.contains(name));
    }
}

/// Compute the set of symbol names to exclude from graph rendering.
/// Excludes `std::rt::lang_start` items and items uniquely downstream
/// of them (i.e., only reachable through `lang_start` in the call graph).
///
/// The algorithm:
/// 1. Build a call graph from Call terminators
/// 2. Identify `std::rt::lang_start` seed items (via demangled name of MonoItemFn)
/// 3. Find entry-point items (not called by any other item)
/// 4. BFS from non-seed entry points, not entering seed nodes
/// 5. Everything not reachable gets excluded
fn compute_lang_start_exclusions(items: &[Item], ctx: &GraphContext) -> HashSet<String> {
    // Build forward call graph: symbol_name -> list of callee names
    let mut call_graph: HashMap<&str, Vec<&str>> = HashMap::new();
    for item in items {
        if let MonoItemKind::MonoItemFn {
            body: Some(body), ..
        } = &item.mono_item_kind
        {
            let callees: Vec<&str> = body
                .blocks
                .iter()
                .filter_map(|block| {
                    if let TerminatorKind::Call {
                        func: Operand::Constant(ConstOperand { const_, .. }),
                        ..
                    } = &block.terminator.kind
                    {
                        return ctx.functions.get(&const_.ty()).map(|s| s.as_str());
                    }
                    None
                })
                .collect();
            call_graph.insert(&item.symbol_name, callees);
        }
    }

    // Identify seed items via the demangled MonoItemFn name containing "std::rt::lang_start".
    let seed_names: HashSet<&str> = items
        .iter()
        .filter(|item| is_std_rt_lang_start(&item.mono_item_kind))
        .map(|item| item.symbol_name.as_str())
        .collect();

    // Retrieve all items that were called via a Call terminator
    let has_callers: HashSet<&str> = call_graph.values().flatten().copied().collect();

    // BFS from non-seed entry points (items with no callers)
    let mut reachable: HashSet<&str> = HashSet::new();
    let mut queue: VecDeque<&str> = VecDeque::new();

    for item in items {
        let name = item.symbol_name.as_str();
        let is_entry = !has_callers.contains(name);
        if is_entry && !seed_names.contains(name) {
            reachable.insert(name);
            queue.push_back(name);
        }
    }

    while let Some(name) = queue.pop_front() {
        if let Some(callees) = call_graph.get(name) {
            for &callee in callees {
                if !reachable.contains(callee) && !seed_names.contains(callee) {
                    reachable.insert(callee);
                    queue.push_back(callee);
                }
            }
        }
    }

    // Everything NOT reachable should be excluded
    let all_names: HashSet<&str> = items
        .iter()
        .map(|i| i.symbol_name.as_str())
        .chain(ctx.functions.values().map(|s| s.as_str()))
        .collect();

    all_names
        .difference(&reachable)
        .map(|s| s.to_string())
        .collect()
}

/// Check the demangled MonoItemFn name for `std::rt::lang_start`.
/// This catches:
/// - `std::rt::lang_start::<()>` (the runtime entry point)
/// - `std::rt::lang_start::<()>::{closure#0}` (its closure)
/// - `<{closure@std::rt::lang_start<()>::{closure#0}} as ...>::call_once` (trait impls referencing it)
/// - `std::ptr::drop_in_place::<{closure@std::rt::lang_start<()>::{closure#0}}>` (drop glue)
///
/// But not a user-defined `lang_start` e.g. `crate1::something::lang_start`.
fn is_std_rt_lang_start(kind: &MonoItemKind) -> bool {
    match kind {
        MonoItemKind::MonoItemFn { name, .. } => name.contains("std::rt::lang_start"),
        _ => false,
    }
}

// =============================================================================
// Entry Points
// =============================================================================

/// Entry point to write the DOT file
pub fn emit_dotfile(tcx: TyCtxt<'_>) {
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
