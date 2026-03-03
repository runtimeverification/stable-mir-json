//! Mono item collection and symbol naming.
//!
//! Wraps `tcx.collect_and_partition_mono_items()`, `item.symbol_name()`,
//! and the `rustc_internal` stable/internal conversions needed for naming.

use super::middle;
use super::rustc_internal;
use super::stable_mir;
use super::TyCtxt;
use stable_mir::mir::mono::MonoItem;

/// Collect all monomorphized items from the compiler.
pub fn mono_collect(tcx: TyCtxt<'_>) -> Vec<MonoItem> {
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

/// Get the symbol name for a mono item (the mangled linker name).
pub fn mono_item_name(tcx: TyCtxt<'_>, item: &MonoItem) -> String {
    if let MonoItem::GlobalAsm(data) = item {
        crate::printer::hash(data).to_string()
    } else {
        mono_item_name_int(tcx, &rustc_internal::internal(tcx, item))
    }
}

/// Get the symbol name for an internal (non-stable) mono item.
pub fn mono_item_name_int<'a>(tcx: TyCtxt<'a>, item: &middle::mir::mono::MonoItem<'a>) -> String {
    item.symbol_name(tcx).name.into()
}
