//! Top-level collection logic that assembles the final [`SmirJson`] output.
//!
//! Gathers monomorphized items from the compiler, discovers additional items
//! via unevaluated constants, traverses MIR bodies to collect interned values
//! (calls, allocations, types, spans), and produces the sorted, deterministic
//! output structure.

extern crate rustc_middle;
extern crate rustc_smir;
extern crate rustc_span;
extern crate stable_mir;

use std::collections::HashMap;

use rustc_middle::ty::TyCtxt;
use rustc_smir::rustc_internal;
use rustc_span::def_id::LOCAL_CRATE;
use stable_mir::mir::alloc::GlobalAlloc;
use stable_mir::mir::mono::MonoItem;
use stable_mir::ty::IndexedVal;

use super::items::{get_foreign_module_details, mk_item};
use super::mir_visitor::collect_interned_values;
use super::schema::{AllocInfo, Item, SmirJson, SmirJsonDebugInfo};
use super::types::mk_type_metadata;
use super::uneval::collect_unevaluated_constant_items;
use super::util::mono_item_name;

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

/// Collect all Stable MIR data for the current crate into a [`SmirJson`] value.
///
/// This is the primary collection entry point. It:
/// 1. Gathers all monomorphized items via `rustc_monomorphize`.
/// 2. Discovers additional items reachable through unevaluated constants.
/// 3. Traverses MIR bodies to build the function link map, allocation map,
///    type map, and span map.
/// 4. Constructs type metadata for each collected type.
/// 5. Sorts all output vectors for deterministic serialization.
///
/// The returned value is ready to be serialized with `serde_json`.
pub fn collect_smir(tcx: TyCtxt<'_>) -> SmirJson {
    let local_crate = stable_mir::local_crate();
    let items = collect_items(tcx);
    let items_clone = items.clone();
    let (unevaluated_consts, mut items) = collect_unevaluated_constant_items(tcx, items);
    let (calls_map, visited_allocs, visited_tys, span_map) = collect_interned_values(tcx, &items);

    // FIXME: We dump extra static items here --- this should be handled better
    for (_, alloc) in visited_allocs.iter() {
        if let (_, GlobalAlloc::Static(def)) = alloc {
            let mono_item =
                stable_mir::mir::mono::MonoItem::Fn(stable_mir::mir::mono::Instance::from(*def));
            let item_name = &mono_item_name(tcx, &mono_item);
            if !items_clone.contains_key(item_name) {
                println!(
                    "Items missing static with id {:?} and name {:?}",
                    def, item_name
                );
                items.push(mk_item(tcx, mono_item, item_name.clone()));
            }
        }
    }

    let debug: Option<SmirJsonDebugInfo> = if super::debug_enabled() {
        let fn_sources = calls_map
            .clone()
            .into_iter()
            .map(|(k, (source, _))| (k, source))
            .collect::<Vec<_>>();
        Some(SmirJsonDebugInfo {
            fn_sources,
            types: visited_tys.clone(),
            foreign_modules: get_foreign_module_details(),
        })
    } else {
        None
    };

    let mut functions = calls_map
        .into_iter()
        .map(|(k, (_, name))| (k, name))
        .collect::<Vec<_>>();
    let mut allocs = visited_allocs
        .into_iter()
        .map(|(alloc_id, (ty, global_alloc))| AllocInfo::new(alloc_id, ty, global_alloc))
        .collect::<Vec<_>>();
    let crate_id = tcx.stable_crate_id(LOCAL_CRATE).as_u64();

    let mut types = visited_tys
        .into_iter()
        .filter_map(|(k, (t, l))| mk_type_metadata(tcx, k, t, l))
        .collect::<Vec<_>>();

    let mut spans = span_map.into_iter().collect::<Vec<_>>();

    // sort output vectors to stabilise output (a bit)
    allocs.sort_by(|a, b| a.alloc_id().to_index().cmp(&b.alloc_id().to_index()));
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
