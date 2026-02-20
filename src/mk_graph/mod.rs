//! MIR graph generation module.
//!
//! This module provides functionality to generate graph visualizations
//! of Rust's MIR in various formats (DOT, D2).

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::{self, Write};

extern crate rustc_middle;
use rustc_middle::ty::TyCtxt;

extern crate rustc_session;
use rustc_session::config::{OutFileName, OutputType};

extern crate stable_mir;
use stable_mir::mir::{ConstOperand, Operand, TerminatorKind};

use crate::printer::{collect_smir, Item};
use crate::MonoItemKind;

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
pub(crate) fn compute_lang_start_exclusions(items: &[Item], ctx: &GraphContext) -> HashSet<String> {
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
            // some items call other items
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
        .chain(ctx.functions.values().map(|s| s.as_str())) // chain external functions too
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

    match tcx.output_filenames(()).path(OutputType::Mir) {
        OutFileName::Stdout => {
            write!(io::stdout(), "{}", smir_dot).expect("Failed to write smir.dot");
        }
        OutFileName::Real(path) => {
            let out_path = path.with_extension("smir.dot");
            let mut b = io::BufWriter::new(
                File::create(&out_path)
                    .unwrap_or_else(|e| panic!("Failed to create {}: {}", out_path.display(), e)),
            );
            write!(b, "{}", smir_dot).expect("Failed to write smir.dot");
        }
    }
}

/// Entry point to write the D2 file
pub fn emit_d2file(tcx: TyCtxt<'_>) {
    let smir_d2 = collect_smir(tcx).to_d2_file();

    match tcx.output_filenames(()).path(OutputType::Mir) {
        OutFileName::Stdout => {
            write!(io::stdout(), "{}", smir_d2).expect("Failed to write smir.d2");
        }
        OutFileName::Real(path) => {
            let out_path = path.with_extension("smir.d2");
            let mut b = io::BufWriter::new(
                File::create(&out_path)
                    .unwrap_or_else(|e| panic!("Failed to create {}: {}", out_path.display(), e)),
            );
            write!(b, "{}", smir_d2).expect("Failed to write smir.d2");
        }
    }
}
