extern crate rustc_middle;
extern crate rustc_smir;
extern crate rustc_span;
extern crate stable_mir;

use std::collections::{HashMap, HashSet};

use rustc_middle::ty::TyCtxt;
use rustc_smir::rustc_internal;
use rustc_span::def_id::LOCAL_CRATE;
use stable_mir::mir::mono::MonoItem;
use stable_mir::mir::visit::MirVisitor;
use stable_mir::ty::IndexedVal;

use super::items::{get_foreign_module_details, mk_item};
use super::mir_visitor::{maybe_add_to_link_map, BodyAnalyzer, UnevalConstInfo};
use super::schema::{
    AllocInfo, AllocMap, CollectedCrate, DerivedInfo, Item, LinkMap, SmirJson, SmirJsonDebugInfo,
    SpanMap,
};
use super::ty_visitor::TyCollector;
use super::types::mk_type_metadata;
use super::util::{mono_item_name, take_any};

fn mono_collect(tcx: TyCtxt<'_>) -> Vec<MonoItem> {
    let units = tcx.collect_and_partition_mono_items(()).1;
    units
        .iter()
        .flat_map(|unit| {
            unit.items_in_deterministic_order(tcx)
                .iter()
                .map(|(internal_item, _)| rustc_internal::stable(internal_item))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn collect_items(tcx: TyCtxt<'_>) -> HashMap<String, Item> {
    // get initial set of mono_items
    let items = mono_collect(tcx);
    items
        .iter()
        .map(|item| {
            let name = mono_item_name(tcx, item);
            (name.clone(), mk_item(tcx, item.clone(), name))
        })
        .collect::<HashMap<_, _>>()
}

/// Enqueue newly discovered unevaluated-const items into the fixpoint work queue.
/// Each new item calls mk_item (which calls inst.body() exactly once).
fn enqueue_unevaluated_consts(
    tcx: TyCtxt<'_>,
    discovered: Vec<UnevalConstInfo>,
    known_names: &mut HashSet<String>,
    pending: &mut HashMap<String, Item>,
    unevaluated_consts: &mut HashMap<stable_mir::ty::ConstDef, String>,
) {
    for info in discovered {
        if known_names.contains(&info.item_name) || pending.contains_key(&info.item_name) {
            continue;
        }
        debug_log_println!("Adding unevaluated const body for: {}", info.item_name);
        unevaluated_consts.insert(info.const_def, info.item_name.clone());
        let new_item = mk_item(tcx, info.mono_item, info.item_name.clone());
        pending.insert(info.item_name.clone(), new_item);
        known_names.insert(info.item_name);
    }
}

/// Collect all mono items and analyze their bodies in a single pass per body.
///
/// Each body is walked exactly once. The fixpoint loop handles transitive
/// discovery of items through unevaluated constants: when a body references an
/// unknown unevaluated const, a new Item is created (calling inst.body() once)
/// and added to the work queue.
fn collect_and_analyze_items<'tcx>(
    tcx: TyCtxt<'tcx>,
    initial_items: HashMap<String, Item>,
) -> (CollectedCrate, DerivedInfo<'tcx>) {
    let mut calls_map: LinkMap<'tcx> = HashMap::new();
    let mut visited_allocs = AllocMap::new();
    let mut ty_visitor = TyCollector::new(tcx);
    let mut span_map: SpanMap = HashMap::new();
    let mut unevaluated_consts: HashMap<stable_mir::ty::ConstDef, String> = HashMap::new();

    let mut known_names: HashSet<String> = initial_items.keys().cloned().collect();
    let mut pending: HashMap<String, Item> = initial_items;
    let mut all_items: Vec<Item> = Vec::new();

    while let Some((_name, item)) = take_any(&mut pending) {
        maybe_add_to_link_map(tcx, &item, &mut calls_map);

        let Some((body, locals)) = item.body_and_locals() else {
            item.warn_missing_body();
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

/// Phase 3: Assemble the final SmirJson from collected and derived data.
/// This is a pure data transformation with no inst.body() calls.
fn assemble_smir<'tcx>(
    tcx: TyCtxt<'tcx>,
    collected: CollectedCrate,
    derived: DerivedInfo<'tcx>,
) -> SmirJson<'tcx> {
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
    let crate_id = tcx.stable_crate_id(LOCAL_CRATE).as_u64();

    let mut types = visited_tys
        .into_iter()
        .filter_map(|(k, (t, l))| mk_type_metadata(tcx, k, t, l))
        .collect::<Vec<_>>();

    let mut spans = span_map.into_iter().collect::<Vec<_>>();

    // sort output vectors to stabilise output (a bit)
    allocs.sort_by_key(|a| a.alloc_id().to_index());
    functions.sort_by(|a, b| a.0 .0.to_index().cmp(&b.0 .0.to_index()));
    items.sort();
    types.sort_by(|a, b| a.0.to_index().cmp(&b.0.to_index()));
    spans.sort();

    SmirJson {
        name: local_crate.name,
        crate_id,
        allocs,
        functions,
        uneval_consts: unevaluated_consts.into_iter().collect(),
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
