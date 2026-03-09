//! Three-phase collection pipeline.
//!
//! - [`collect_items`]: phase 1, enumerates monomorphized items from rustc
//! - [`collect_and_analyze_items`]: phase 2, walks bodies with [`BodyAnalyzer`],
//!   discovering transitive items through unevaluated constants
//! - [`assemble_smir`]: phase 3, pure data transformation into [`SmirJson`]
//!
//! The phase boundary between 2 and 3 is enforced structurally: [`Item`] does
//! not carry a `MonoItem`, so phase 3 code cannot call `inst.body()` or
//! otherwise re-enter rustc. `MonoItem` values live only in the phase 1+2
//! maps and are dropped before phase 3 begins.

use crate::compat::middle::ty::TyCtxt;
use crate::compat::mono_collect::mono_collect;
use crate::compat::stable_mir;

use std::collections::{HashMap, HashSet};

use crate::compat::indexed_val::to_index;
use stable_mir::mir::alloc::GlobalAlloc;
use stable_mir::mir::mono::MonoItem;
use stable_mir::mir::visit::MirVisitor;
use stable_mir::CrateDef;

use super::items::{get_foreign_module_details, mk_item};
use super::mir_visitor::{maybe_add_to_link_map, BodyAnalyzer, UnevalConstInfo};
use super::schema::{
    AllocInfo, AllocMap, CollectedCrate, DerivedInfo, Item, LinkMap, SmirJson, SmirJsonDebugInfo,
    SpanMap,
};
use super::ty_visitor::TyCollector;
use super::types::mk_type_metadata;
use super::util::take_any;

use crate::compat::mono_collect::mono_item_name;

/// Log a warning when a body was expected but missing.
fn warn_missing_body(mono_item: &MonoItem) {
    match mono_item {
        MonoItem::Fn(inst) => {
            eprintln!(
                "Failed to retrieve body for Instance of MonoItem::Fn {}",
                inst.name()
            );
        }
        MonoItem::Static(def) => {
            eprintln!(
                "Failed to retrieve body for Instance of MonoItem::Static {}",
                def.name()
            );
        }
        MonoItem::GlobalAsm(_) => {}
    }
}

fn collect_items(tcx: TyCtxt<'_>) -> HashMap<String, (MonoItem, Item)> {
    // get initial set of mono_items
    let items = mono_collect(tcx);
    items
        .iter()
        .map(|item| {
            let name = mono_item_name(tcx, item);
            let (mono_item, built_item) = mk_item(tcx, item.clone(), name.clone());
            (name, (mono_item, built_item))
        })
        .collect::<HashMap<_, _>>()
}

/// Enqueue newly discovered unevaluated-const items into the fixpoint work queue.
/// Each new entry calls `mk_item` (which calls `inst.body()` exactly once),
/// producing a `(MonoItem, Item)` pair for the pending map.
fn enqueue_unevaluated_consts(
    tcx: TyCtxt<'_>,
    discovered: Vec<UnevalConstInfo>,
    known_names: &mut HashSet<String>,
    pending: &mut HashMap<String, (MonoItem, Item)>,
    unevaluated_consts: &mut HashMap<stable_mir::ty::ConstDef, String>,
) {
    for info in discovered {
        if known_names.contains(&info.item_name) || pending.contains_key(&info.item_name) {
            continue;
        }
        debug_log_println!("Adding unevaluated const body for: {}", info.item_name);
        unevaluated_consts.insert(info.const_def, info.item_name.clone());
        let new_entry = mk_item(tcx, info.mono_item, info.item_name.clone());
        pending.insert(info.item_name.clone(), new_entry);
        known_names.insert(info.item_name);
    }
}

/// Collect all mono items and analyze their bodies in a single pass per body.
///
/// Each body is walked exactly once. The fixpoint loop handles transitive
/// discovery of items through unevaluated constants: when a body references an
/// unknown unevaluated const, `mk_item` produces a new `(MonoItem, Item)` pair
/// (calling `inst.body()` exactly once) and adds it to the work queue. The
/// `MonoItem` half is used for link-map registration and diagnostics during
/// this phase, then dropped; only the `Item` survives into `CollectedCrate`.
fn collect_and_analyze_items(
    tcx: TyCtxt<'_>,
    initial_items: HashMap<String, (MonoItem, Item)>,
) -> (CollectedCrate, DerivedInfo) {
    let mut calls_map: LinkMap = HashMap::new();
    let mut visited_allocs = AllocMap::new();
    let mut ty_visitor = TyCollector::new(tcx);
    let mut span_map: SpanMap = HashMap::new();
    let mut unevaluated_consts: HashMap<stable_mir::ty::ConstDef, String> = HashMap::new();

    let mut known_names: HashSet<String> = initial_items.keys().cloned().collect();
    let mut pending: HashMap<String, (MonoItem, Item)> = initial_items;
    let mut all_items: Vec<Item> = Vec::new();

    while let Some((_name, (mono_item, item))) = take_any(&mut pending) {
        maybe_add_to_link_map(tcx, &mono_item, &mut calls_map);

        let Some((body, locals)) = item.body_and_locals() else {
            warn_missing_body(&mono_item);
            all_items.push(item);
            continue;
        };

        let mut new_unevaluated = Vec::new();
        BodyAnalyzer {
            tcx,
            locals,
            link_map: &mut calls_map,
            visited_allocs: &mut visited_allocs,
            ty_visitor: &mut ty_visitor,
            spans: &mut span_map,
            new_unevaluated: &mut new_unevaluated,
        }
        .visit_body(body);

        enqueue_unevaluated_consts(
            tcx,
            new_unevaluated,
            &mut known_names,
            &mut pending,
            &mut unevaluated_consts,
        );

        all_items.push(item);
    }

    if !ty_visitor.layout_panics.is_empty() {
        eprintln!(
            "warning: {} type layout(s) could not be computed (rustc panicked):",
            ty_visitor.layout_panics.len()
        );
        for panic in &ty_visitor.layout_panics {
            eprintln!("  type {:?}: {}", panic.ty, panic.message);
        }
    }

    (
        CollectedCrate {
            items: all_items,
            unevaluated_consts,
        },
        DerivedInfo {
            calls: calls_map,
            allocs: visited_allocs,
            types: ty_visitor.types,
            spans: span_map,
        },
    )
}

/// Allocations are sorted by a three-tier key for deterministic output:
///
/// 1. **Variant tag** (`alloc_sort_tag`): a `&'static str` with a numeric
///    prefix that groups allocations by `GlobalAlloc` variant (Memory < Static
///    < VTable < Function).
///
/// 2. **Content key** (`alloc_content_key`): the name or Display string that
///    disambiguates entries within Static, VTable, and Function variants.
///    Empty for Memory (handled entirely by tier 3).
///
/// 3. **Byte content** (`alloc_bytes`): direct `&[Option<u8>]` slice
///    comparison that breaks ties between Memory allocations with identical
///    length (e.g. two 5-byte string literals "hello" vs "world"). Empty
///    for non-Memory variants (already resolved by tier 2).
fn alloc_sort_tag(info: &AllocInfo) -> &'static str {
    match info.global_alloc() {
        GlobalAlloc::Memory(_) => "0_Memory",
        GlobalAlloc::Static(_) => "1_Static",
        GlobalAlloc::VTable(..) => "2_VTable",
        GlobalAlloc::Function(_) => "3_Function",
    }
}

fn alloc_content_key(info: &AllocInfo) -> String {
    match info.global_alloc() {
        GlobalAlloc::Memory(_) => String::new(),
        GlobalAlloc::Static(def) => def.name(),
        GlobalAlloc::VTable(ty, _) => format!("{ty}"),
        GlobalAlloc::Function(inst) => inst.name(),
    }
}

fn alloc_bytes(info: &AllocInfo) -> &[Option<u8>] {
    match info.global_alloc() {
        GlobalAlloc::Memory(alloc) => &alloc.bytes,
        _ => &[],
    }
}

/// Phase 3: Assemble the final SmirJson from collected and derived data.
/// This is a pure data transformation with no inst.body() calls.
fn assemble_smir(tcx: TyCtxt<'_>, collected: CollectedCrate, derived: DerivedInfo) -> SmirJson {
    let local_crate = stable_mir::local_crate();
    let CollectedCrate {
        mut items,
        unevaluated_consts,
    } = collected;
    let DerivedInfo {
        calls,
        allocs: visited_allocs,
        types: visited_tys,
        spans: span_map,
    } = derived;

    // Verify alloc coherence: no duplicate AllocIds, and every AllocId
    // referenced in a stored body was actually collected.
    #[cfg(debug_assertions)]
    visited_allocs.verify_coherence(&items);

    let debug: Option<SmirJsonDebugInfo> = if super::debug_enabled() {
        let fn_sources = calls
            .iter()
            .map(|(k, (source, _))| (k.clone(), source.clone()))
            .collect::<Vec<_>>();
        Some(SmirJsonDebugInfo {
            fn_sources,
            types: visited_tys.clone(),
            foreign_modules: get_foreign_module_details(),
        })
    } else {
        None
    };

    let mut functions = calls
        .into_iter()
        .map(|(k, (_, name))| (k, name))
        .collect::<Vec<_>>();
    let mut allocs = visited_allocs
        .into_entries()
        .map(|(alloc_id, (ty, global_alloc))| AllocInfo::new(alloc_id, ty, global_alloc))
        .collect::<Vec<_>>();
    let crate_id = crate::compat::types::local_crate_id(tcx);

    let mut types = visited_tys
        .into_iter()
        .filter_map(|(k, (t, l))| mk_type_metadata(tcx, k, t, l))
        .collect::<Vec<_>>();

    let mut spans = span_map.into_iter().collect::<Vec<_>>();

    // sort output vectors by content-derived keys for deterministic output.
    // Ty's Display impl (ty_pretty) should be injective for monomorphized types,
    // but we use the interned index as a tiebreaker just in case two distinct
    // types produce the same display string.
    allocs.sort_by(|a, b| {
        alloc_sort_tag(a)
            .cmp(alloc_sort_tag(b))
            .then_with(|| alloc_content_key(a).cmp(&alloc_content_key(b)))
            .then_with(|| alloc_bytes(a).cmp(alloc_bytes(b)))
    });
    functions.sort_by(|a, b| {
        format!("{}", a.0 .0)
            .cmp(&format!("{}", b.0 .0))
            .then_with(|| {
                let a_kind = a.0 .1.as_ref().map(|k| format!("{k}"));
                let b_kind = b.0 .1.as_ref().map(|k| format!("{k}"));
                a_kind.cmp(&b_kind)
            })
            .then_with(|| to_index(&a.0 .0).cmp(&to_index(&b.0 .0)))
    });
    items.sort();
    types.sort_by(|a, b| {
        format!("{}", a.0)
            .cmp(&format!("{}", b.0))
            .then_with(|| to_index(&a.0).cmp(&to_index(&b.0)))
    });
    spans.sort_by(|a, b| a.1.cmp(&b.1));

    let mut uneval_consts: Vec<_> = unevaluated_consts.into_iter().collect();
    uneval_consts.sort_by(|a, b| a.1.cmp(&b.1));

    SmirJson {
        name: local_crate.name,
        crate_id,
        allocs,
        functions,
        uneval_consts,
        items,
        types,
        spans,
        debug,
        machine: stable_mir::target::MachineInfo::target(),
    }
}

pub fn collect_smir(tcx: TyCtxt<'_>) -> SmirJson {
    // Phase 1+2: Collect all mono items from rustc and analyze their bodies
    // in a single pass. Each body is walked exactly once. Transitive item
    // discovery (unevaluated constants) is handled by a fixpoint loop.
    let initial_items = collect_items(tcx);
    let (collected, derived) = collect_and_analyze_items(tcx, initial_items);

    // Phase 3: Assemble the final output (pure data transformation)
    assemble_smir(tcx, collected, derived)
}
