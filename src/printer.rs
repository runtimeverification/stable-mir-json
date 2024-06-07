use std::io;
use std::iter::Iterator;
use std::vec::Vec;
use std::str;
extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;
extern crate rustc_smir;
extern crate stable_mir;
use rustc_hir::{def::DefKind, definitions::DefPath};
use rustc_middle::ty::{TyCtxt, Ty, TyKind, EarlyBinder, Binder, FnSig, GenericArgs, TypeFoldable, Generics, GenericPredicates};
use rustc_span::{def_id::DefId, symbol::sym};
use rustc_smir::rustc_internal;
use stable_mir::{CrateDef,ItemKind,Symbol,to_json,mir::Body};
#[macro_use]
use tracing::enabled;
use serde::Serialize;

// TODO: consider using underlying structs struct GenericData<'a>(Vec<(&'a Generics,GenericPredicates<'a>)>);
#[derive(Serialize)]
struct GenericData(Vec<(String,String)>);
#[derive(Serialize)]
struct ItemDetails {
    internal_kind: String,
    path: String,
    internal_ty: String,
    generic_data: GenericData,
}
#[derive(Serialize)]
struct BodyDetails {
    pp: String,
}

#[derive(Serialize)]
struct MirBody(Body, Option<BodyDetails>);
#[derive(Serialize)]
struct Item {
    name: String,
    kind: ItemKind,
    body: MirBody,
    promoted: Vec<MirBody>,
    details: Option<ItemDetails>
}

pub fn generic_data(tcx: TyCtxt<'_>, id: DefId) -> GenericData {
     let mut v = Vec::new();
     let mut next_id = Some(id);
     while let Some(curr_id) = next_id {
        let params = tcx.generics_of(id);
        let preds  = tcx.predicates_of(id);
        if params.parent != preds.parent { panic!("Generics and predicates parent ids are distinct"); }
        v.push((format!("{:#?}", params), format!("{:#?}", preds)));
        next_id = params.parent;
     }
     v.reverse();
     return GenericData(v);
}

pub fn get_body_details(body: &Body, name: Option<&String>) -> Option<BodyDetails> {
  if enabled!(tracing::Level::DEBUG) {
    let mut v = Vec::new();
    let name = if let Some(name) = name { name } else { "<promoted>" };
    body.dump(&mut v, name);
    Some(BodyDetails {
      pp: str::from_utf8(&v).unwrap().into(),
    })
  } else {
    None
  }
}

// unwrap early binder in a default manner; panic on error
fn default_unwrap_early_binder<'tcx, T>(tcx: TyCtxt<'tcx>, id: DefId, v: EarlyBinder<T>) -> T
  where T: TypeFoldable<TyCtxt<'tcx>>
{
  let v_copy = v.clone();
  match tcx.try_instantiate_and_normalize_erasing_regions(GenericArgs::identity_for_item(tcx, id), tcx.param_env(id), v) {
      Ok(res) => return res,
      Err(err) => { println!("{:?}", err); v_copy.skip_binder() }
  }
}

pub fn print_type<'tcx>(tcx: TyCtxt<'tcx>, id: DefId, ty: EarlyBinder<Ty<'tcx>>) -> String {
  // lookup type kind in order to perform case analysis
  let kind: &TyKind = ty.skip_binder().kind();
  if let TyKind::FnDef(fun_id, args) = kind {
    // since FnDef doesn't contain signature, lookup actual function type
    // via getting fn signature with parameters and resolving those parameters
    let sig0 = tcx.fn_sig(fun_id);
    let sig0_copy = sig0.clone();
    let sig1 = match tcx.try_instantiate_and_normalize_erasing_regions(args, tcx.param_env(fun_id), sig0) {
      Ok(res) => res,
      Err(err) => { println!("{:?}", err); sig0.skip_binder() }
    };
    let sig2: FnSig<'_> = tcx.instantiate_bound_regions_with_erased(sig1);
    format!("\nTyKind(FnDef): {:#?}", sig2)
  } else {
    let kind = default_unwrap_early_binder(tcx, id, ty);
    format!("\nTyKind: {:#?}", kind)
  }
}

pub fn get_item_details(tcx: TyCtxt<'_>, id: DefId) -> Option<ItemDetails> {
  if enabled!(tracing::Level::DEBUG) {
    Some(ItemDetails {
      internal_kind: format!("{:#?}", tcx.def_kind(id)),
      path: tcx.def_path_str(id),  // NOTE: underlying data from tcx.def_path(id);
      internal_ty: print_type(tcx, id, tcx.type_of(id)),
      generic_data: generic_data(tcx, id),
      // TODO: let layout = tcx.layout_of(id);
    })
  } else {
    None
  }
}

pub fn has_attr(tcx: TyCtxt<'_>, item: &stable_mir::CrateItem, attr: Symbol) -> bool {
   tcx.has_attr(rustc_internal::internal(tcx,item), sym::test)
}

fn mkMirBody(body: Body, name: Option<&String>) -> MirBody {
  let details = get_body_details(&body, name);
  MirBody(body, details)
}

// TODO: Should we filter any incoming items?
//       Example: .filter(|item| has_attr(item, sym::test) or matches!(item.kind, ItemKind::Const | ItemKind::Static | ItemKind::Fn))
pub fn print_all_items(tcx: TyCtxt<'_>) {
  let mut out = io::stdout();
  let items: Vec<Item> = stable_mir::all_local_items().iter().map(|item| {
    let name = format!("{:?}", item.name());
    let body = item.body();
    let id = rustc_internal::internal(tcx,item.def_id());
    Item {
      name: name.clone(),
      kind: item.kind(),
      body: mkMirBody(body, Some(&name)),
      promoted: tcx.promoted_mir(id).into_iter().map(|body| mkMirBody(rustc_internal::stable(body), None)).collect(),
      details: get_item_details(tcx, id),
    }
  }).collect();
  println!("{}", to_json(items).expect("serde_json failed"));
}
