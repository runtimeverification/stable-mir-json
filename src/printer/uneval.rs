//! Discovery of items reachable through unevaluated constants.
//!
//! Some function bodies reference other functions only via unevaluated
//! `MirConst` values (e.g., generic const expressions). This module
//! iterates to a fixed point, resolving each unevaluated constant to
//! its underlying `Instance` and adding the corresponding item to the
//! collection if it wasn't already present.

extern crate rustc_middle;
extern crate rustc_smir;
extern crate stable_mir;

use std::collections::HashMap;

use rustc_middle::ty::{TyCtxt, TypingEnv};
use rustc_smir::rustc_internal;
use stable_mir::mir::visit::MirVisitor;
use stable_mir::CrateDef;

use super::items::mk_item;
use super::schema::{Item, MonoItemKind};
use super::util::{hash, mono_item_name_int, take_any};

pub(super) struct UnevaluatedConstCollector<'tcx, 'local> {
    tcx: TyCtxt<'tcx>,
    unevaluated_consts: &'local mut HashMap<stable_mir::ty::ConstDef, String>,
    processed_items: &'local mut HashMap<String, Item>,
    pending_items: &'local mut HashMap<String, Item>,
    current_item: u64,
}

impl MirVisitor for UnevaluatedConstCollector<'_, '_> {
    fn visit_mir_const(
        &mut self,
        constant: &stable_mir::ty::MirConst,
        _location: stable_mir::mir::visit::Location,
    ) {
        if let stable_mir::ty::ConstantKind::Unevaluated(uconst) = constant.kind() {
            let internal_def = rustc_internal::internal(self.tcx, uconst.def.def_id());
            let internal_args = rustc_internal::internal(self.tcx, uconst.args.clone());
            let maybe_inst = rustc_middle::ty::Instance::try_resolve(
                self.tcx,
                TypingEnv::post_analysis(self.tcx, internal_def),
                internal_def,
                internal_args,
            );
            let inst = maybe_inst
                .ok()
                .flatten()
                .unwrap_or_else(|| panic!("Failed to resolve mono item for {:?}", uconst));
            let internal_mono_item = rustc_middle::mir::mono::MonoItem::Fn(inst);
            let item_name = mono_item_name_int(self.tcx, &internal_mono_item);
            if !(self.processed_items.contains_key(&item_name)
                || self.pending_items.contains_key(&item_name)
                || self.current_item == hash(&item_name))
            {
                debug_log_println!("Adding unevaluated const body for: {}", item_name);
                self.unevaluated_consts
                    .insert(uconst.def, item_name.clone());
                self.pending_items.insert(
                    item_name.clone(),
                    mk_item(
                        self.tcx,
                        rustc_internal::stable(internal_mono_item),
                        item_name,
                    ),
                );
            }
        }
    }
}

pub(super) fn collect_unevaluated_constant_items(
    tcx: TyCtxt<'_>,
    items: HashMap<String, Item>,
) -> (HashMap<stable_mir::ty::ConstDef, String>, Vec<Item>) {
    // setup collector prerequisites
    let mut unevaluated_consts = HashMap::new();
    let mut processed_items = HashMap::new();
    let mut pending_items = items;

    while let Some((curr_name, value)) = take_any(&mut pending_items) {
        // skip item if it isn't a function
        let body = match value.mono_item_kind {
            MonoItemKind::MonoItemFn { ref body, .. } => body,
            _ => continue,
        };

        // create new collector for function body
        let mut collector = UnevaluatedConstCollector {
            tcx,
            unevaluated_consts: &mut unevaluated_consts,
            processed_items: &mut processed_items,
            pending_items: &mut pending_items,
            current_item: hash(&curr_name),
        };

        if let Some(body) = body {
            collector.visit_body(body);
        }

        // move processed item into seen items
        processed_items.insert(curr_name.to_string(), value);
    }

    (
        unevaluated_consts,
        processed_items.drain().map(|(_name, item)| item).collect(),
    )
}
