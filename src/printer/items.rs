//! Construction of [`Item`] values from monomorphized compiler items.
//!
//! [`mk_item`] takes a `MonoItem` and returns `(MonoItem, Item)`: the caller
//! gets back the original `MonoItem` (needed during phase 2 for link-map
//! registration and diagnostics) alongside the serializable [`Item`]. This
//! split is what keeps `MonoItem` out of [`Item`] while still making it
//! available during collection.
//!
//! Also handles optional debug-level details (instance kind, body pretty-print,
//! generic parameters, internal type info) and foreign module enumeration.

use crate::compat::middle::ty::TyCtxt;
use crate::compat::serde;
use crate::compat::stable_mir;

use crate::compat::DefId;
use serde::Serialize;
use stable_mir::mir::mono::{Instance, MonoItem};
use stable_mir::mir::Body;
use stable_mir::ty::Allocation;
use stable_mir::{CrateDef, CrateItem};

use crate::compat::bridge::mono_instance;

use super::schema::{BodyDetails, ForeignItem, ForeignModule, GenericData, Item, ItemDetails};

#[derive(Serialize, Clone)]
pub enum MonoItemKind {
    MonoItemFn {
        name: String,
        id: stable_mir::DefId,
        body: Option<Body>,
    },
    MonoItemStatic {
        name: String,
        id: stable_mir::DefId,
        allocation: Option<Allocation>,
        #[serde(skip)]
        body: Option<Body>,
    },
    MonoItemGlobalAsm {
        asm: String,
    },
}

fn get_body_details(body: &Body) -> BodyDetails {
    let mut v = Vec::new();
    let _ = body.dump(&mut v, "<omitted>");
    BodyDetails::new(std::str::from_utf8(&v).unwrap().into())
}

fn get_item_details(
    tcx: TyCtxt<'_>,
    id: DefId,
    fn_inst: Option<Instance>,
    fn_body: Option<&Body>,
) -> Option<ItemDetails> {
    if super::debug_enabled() {
        let (internal_kind, path, internal_ty) = crate::compat::types::get_def_info(tcx, id);
        Some(ItemDetails {
            fn_instance_kind: fn_inst.map(|i| i.kind),
            fn_item_kind: fn_inst
                .and_then(|i| CrateItem::try_from(i).ok())
                .map(|i| i.kind()),
            fn_body_details: fn_body.map(get_body_details),
            internal_kind,
            path,
            internal_ty,
            generic_data: GenericData(crate::compat::types::generic_data(tcx, id)),
        })
    } else {
        None
    }
}

pub(super) fn mk_item(tcx: TyCtxt<'_>, item: MonoItem, sym_name: String) -> (MonoItem, Item) {
    match item {
        MonoItem::Fn(inst) => {
            let id = inst.def.def_id();
            let name = inst.name();
            let internal_id = crate::compat::types::internal_def_id(tcx, id);
            let body = inst.body();
            let details = get_item_details(tcx, internal_id, Some(inst), body.as_ref());
            let mono_item = MonoItem::Fn(inst);
            (
                mono_item,
                Item::new(
                    sym_name.clone(),
                    MonoItemKind::MonoItemFn {
                        name: name.clone(),
                        id,
                        body,
                    },
                    details,
                ),
            )
        }
        MonoItem::Static(static_def) => {
            let internal_id = crate::compat::types::internal_def_id(tcx, static_def.def_id());
            let alloc = match static_def.eval_initializer() {
                Ok(alloc) => Some(alloc),
                err => {
                    eprintln!(
                        "StaticDef({static_def:#?}).eval_initializer() failed with: {err:#?}"
                    );
                    None
                }
            };
            let inst = mono_instance(tcx, static_def.def_id());
            let body = inst.body();
            let mono_item = MonoItem::Static(static_def);
            (
                mono_item,
                Item::new(
                    sym_name,
                    MonoItemKind::MonoItemStatic {
                        name: static_def.name(),
                        id: static_def.def_id(),
                        allocation: alloc,
                        body,
                    },
                    get_item_details(tcx, internal_id, None, None),
                ),
            )
        }
        MonoItem::GlobalAsm(ref asm) => {
            let asm_str = format!("{asm:#?}");
            (
                item,
                Item::new(
                    sym_name,
                    MonoItemKind::MonoItemGlobalAsm { asm: asm_str },
                    None,
                ),
            )
        }
    }
}

pub(super) fn get_foreign_module_details() -> Vec<(String, Vec<ForeignModule>)> {
    let mut crates = vec![stable_mir::local_crate()];
    crates.append(&mut stable_mir::external_crates());
    crates
        .into_iter()
        .map(|krate| {
            (
                krate.name.clone(),
                krate
                    .foreign_modules()
                    .into_iter()
                    .map(|mod_def| {
                        let fmod = mod_def.module();
                        ForeignModule {
                            name: mod_def.name(),
                            items: fmod
                                .items()
                                .into_iter()
                                .map(|def| ForeignItem {
                                    name: def.name(),
                                    kind: def.kind(),
                                })
                                .collect(),
                        }
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}
