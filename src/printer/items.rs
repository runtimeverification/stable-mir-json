//! Construction of [`Item`] values from monomorphized compiler items.
//!
//! Handles the mapping from `MonoItem` (function, static, or global asm) to the
//! serializable [`Item`] structure, including optional debug-level details
//! (instance kind, body pretty-print, generic parameters, internal type info)
//! and foreign module enumeration.

extern crate rustc_middle;
extern crate rustc_smir;
extern crate rustc_span;
extern crate serde;
extern crate stable_mir;

use rustc_middle as middle;
use rustc_middle::ty::{EarlyBinder, FnSig, GenericArgs, Ty, TyCtxt, TypeFoldable};
use rustc_smir::rustc_internal;
use rustc_span::def_id::DefId;
use serde::Serialize;
use stable_mir::mir::mono::{Instance, MonoItem};
use stable_mir::mir::Body;
use stable_mir::ty::Allocation;
use stable_mir::{CrateDef, CrateItem};

use super::schema::{BodyDetails, ForeignItem, ForeignModule, GenericData, Item, ItemDetails};
use super::util::def_id_to_inst;

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

fn generic_data(tcx: TyCtxt<'_>, id: DefId) -> GenericData {
    let mut v = Vec::new();
    let mut next_id = Some(id);
    while let Some(curr_id) = next_id {
        let params = tcx.generics_of(curr_id);
        let preds = tcx.predicates_of(curr_id);
        if params.parent != preds.parent {
            panic!("Generics and predicates parent ids are distinct");
        }
        v.push((format!("{:#?}", params), format!("{:#?}", preds)));
        next_id = params.parent;
    }
    v.reverse();
    GenericData(v)
}

// unwrap early binder in a default manner; panic on error
fn default_unwrap_early_binder<'tcx, T>(tcx: TyCtxt<'tcx>, id: DefId, v: EarlyBinder<'tcx, T>) -> T
where
    T: TypeFoldable<TyCtxt<'tcx>>,
{
    let v_copy = v.clone();
    let body = tcx.optimized_mir(id);
    match tcx.try_instantiate_and_normalize_erasing_regions(
        GenericArgs::identity_for_item(tcx, id),
        body.typing_env(tcx),
        v,
    ) {
        Ok(res) => res,
        Err(err) => {
            eprintln!("{:?}", err);
            v_copy.skip_binder()
        }
    }
}

fn print_type<'tcx>(tcx: TyCtxt<'tcx>, id: DefId, ty: EarlyBinder<'tcx, Ty<'tcx>>) -> String {
    // lookup type kind in order to perform case analysis
    let kind: &middle::ty::TyKind = ty.skip_binder().kind();
    if let middle::ty::TyKind::FnDef(fun_id, args) = kind {
        // since FnDef doesn't contain signature, lookup actual function type
        // via getting fn signature with parameters and resolving those parameters
        let sig0 = tcx.fn_sig(fun_id);
        let body = tcx.optimized_mir(id);
        let sig1 = match tcx.try_instantiate_and_normalize_erasing_regions(
            args,
            body.typing_env(tcx),
            sig0,
        ) {
            Ok(res) => res,
            Err(err) => {
                eprintln!("{:?}", err);
                sig0.skip_binder()
            }
        };
        let sig2: FnSig<'_> = tcx.instantiate_bound_regions_with_erased(sig1);
        format!("\nTyKind(FnDef): {:#?}", sig2)
    } else {
        let kind = default_unwrap_early_binder(tcx, id, ty);
        format!("\nTyKind: {:#?}", kind)
    }
}

fn get_item_details(
    tcx: TyCtxt<'_>,
    id: DefId,
    fn_inst: Option<Instance>,
    fn_body: Option<&Body>,
) -> Option<ItemDetails> {
    if super::debug_enabled() {
        Some(ItemDetails {
            fn_instance_kind: fn_inst.map(|i| i.kind),
            fn_item_kind: fn_inst
                .and_then(|i| CrateItem::try_from(i).ok())
                .map(|i| i.kind()),
            fn_body_details: fn_body.map(get_body_details),
            internal_kind: format!("{:#?}", tcx.def_kind(id)),
            path: tcx.def_path_str(id), // NOTE: underlying data from tcx.def_path(id);
            internal_ty: print_type(tcx, id, tcx.type_of(id)),
            generic_data: generic_data(tcx, id),
            // TODO: let layout = tcx.layout_of(id);
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
            let internal_id = rustc_internal::internal(tcx, id);
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
            let internal_id = rustc_internal::internal(tcx, static_def.def_id());
            let alloc = match static_def.eval_initializer() {
                Ok(alloc) => Some(alloc),
                err => {
                    eprintln!(
                        "StaticDef({:#?}).eval_initializer() failed with: {:#?}",
                        static_def, err
                    );
                    None
                }
            };
            let inst = def_id_to_inst(tcx, static_def.def_id());
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
            let asm_str = format!("{:#?}", asm);
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
