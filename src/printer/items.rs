//! Construction of [`Item`] values from monomorphized compiler items.
//!
//! Handles the mapping from `MonoItem` (function, static, or global asm) to the
//! serializable [`Item`] structure, including optional debug-level details
//! (instance kind, body pretty-print, generic parameters, internal type info)
//! and foreign module enumeration.

extern crate rustc_middle;
extern crate rustc_smir;
extern crate rustc_span;
extern crate stable_mir;

use std::str;
use std::vec::Vec;

use rustc_middle as middle;
use rustc_middle::ty::{EarlyBinder, FnSig, GenericArgs, Ty, TyCtxt, TypeFoldable};
use rustc_smir::rustc_internal;
use rustc_span::def_id::DefId;
use stable_mir::mir::mono::{Instance, MonoItem};
use stable_mir::CrateDef;

use super::schema::{
    BodyDetails, ForeignItem, ForeignModule, GenericData, Item, ItemDetails, MonoItemKind,
};

pub(super) fn mk_item(tcx: TyCtxt<'_>, item: MonoItem, sym_name: String) -> Item {
    match item {
        MonoItem::Fn(inst) => {
            let id = inst.def.def_id();
            let name = inst.name();
            let internal_id = rustc_internal::internal(tcx, id);
            Item::new(
                item,
                sym_name.clone(),
                MonoItemKind::MonoItemFn {
                    name: name.clone(),
                    id,
                    body: inst.body(),
                },
                get_item_details(tcx, internal_id, Some(inst)),
            )
        }
        MonoItem::Static(static_def) => {
            let internal_id = rustc_internal::internal(tcx, static_def.def_id());
            let alloc = match static_def.eval_initializer() {
                Ok(alloc) => Some(alloc),
                err => {
                    println!(
                        "StaticDef({:#?}).eval_initializer() failed with: {:#?}",
                        static_def, err
                    );
                    None
                }
            };
            Item::new(
                item,
                sym_name,
                MonoItemKind::MonoItemStatic {
                    name: static_def.name(),
                    id: static_def.def_id(),
                    allocation: alloc,
                },
                get_item_details(tcx, internal_id, None),
            )
        }
        MonoItem::GlobalAsm(ref asm) => {
            let asm = format!("{:#?}", asm);
            Item::new(
                item,
                sym_name,
                MonoItemKind::MonoItemGlobalAsm { asm },
                None,
            )
        }
    }
}

fn get_body_details(body: &stable_mir::mir::Body) -> BodyDetails {
    let mut v = Vec::new();
    let _ = body.dump(&mut v, "<omitted>");
    BodyDetails {
        pp: str::from_utf8(&v).unwrap().into(),
    }
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

pub(super) fn get_item_details(
    tcx: TyCtxt<'_>,
    id: DefId,
    fn_inst: Option<Instance>,
) -> Option<ItemDetails> {
    if super::debug_enabled() {
        Some(ItemDetails {
            fn_instance_kind: fn_inst.map(|i| i.kind),
            fn_item_kind: fn_inst
                .and_then(|i| stable_mir::CrateItem::try_from(i).ok())
                .map(|i| i.kind()),
            fn_body_details: if let Some(fn_inst) = fn_inst {
                fn_inst.body().map(|body| get_body_details(&body))
            } else {
                None
            },
            internal_kind: format!("{:#?}", tcx.def_kind(id)),
            path: tcx.def_path_str(id),
            internal_ty: print_type(tcx, id, tcx.type_of(id)),
            generic_data: generic_data(tcx, id),
        })
    } else {
        None
    }
}

pub(super) fn default_unwrap_early_binder<'tcx, T>(
    tcx: TyCtxt<'tcx>,
    id: DefId,
    v: EarlyBinder<'tcx, T>,
) -> T
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
            println!("{:?}", err);
            v_copy.skip_binder()
        }
    }
}

pub(super) fn print_type<'tcx>(
    tcx: TyCtxt<'tcx>,
    id: DefId,
    ty: EarlyBinder<'tcx, Ty<'tcx>>,
) -> String {
    let kind: &middle::ty::TyKind = ty.skip_binder().kind();
    if let middle::ty::TyKind::FnDef(fun_id, args) = kind {
        let sig0 = tcx.fn_sig(fun_id);
        let body = tcx.optimized_mir(id);
        let sig1 = match tcx.try_instantiate_and_normalize_erasing_regions(
            args,
            body.typing_env(tcx),
            sig0,
        ) {
            Ok(res) => res,
            Err(err) => {
                println!("{:?}", err);
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
