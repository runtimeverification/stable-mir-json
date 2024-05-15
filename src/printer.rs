use std::io;
extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;
extern crate rustc_smir;
extern crate stable_mir;
use rustc_hir::{def::DefKind, definitions::DefPath};
use rustc_middle::ty::{TyCtxt, Ty, TyKind, EarlyBinder, Binder, FnSig, GenericArgs, TypeFoldable};
use rustc_span::{def_id::DefId, symbol::sym};
use rustc_smir::rustc_internal;
use stable_mir::{CrateDef,Symbol,serde_json};
use super::pretty::function_body;

pub fn print_generics_chain(tcx: TyCtxt<'_>, opt_id: Option<DefId>) -> String {
  if let Some(id) = opt_id {
     let params = tcx.generics_of(id);
     let preds  = tcx.predicates_of(id);
     if params.parent != preds.parent { panic!("Generics and predicates parent ids are distinct"); }
     let parent_chain = print_generics_chain(tcx, params.parent);
     // skip printing empty predicate structs
     let preds_string = if preds.predicates.len() == 0 {
       "".into()
     } else {
       format!("\nPreds: {:#?}", preds.predicates)
     };
     return format!("\nParams: {:#?}{preds_string}{parent_chain}", params);
  } else {
    return "".into()
  }
}

pub fn print_item(tcx: TyCtxt<'_>, item: &stable_mir::CrateItem, out: &mut io::Stdout) {
  // function_body(out, &item.body(), &item.name());
  item.emit_mir(out);
  // println!("{:#?}", item.body());
  println!("{}",serde_json::to_string(&item.body()).unwrap());
  for (idx, promoted) in tcx.promoted_mir(rustc_internal::internal(tcx,item.def_id())).into_iter().enumerate() {
    let promoted_body = rustc_internal::stable(promoted);
    promoted_body.dump(out,format!("promoted[{}:{}]", item.name(), idx).as_str());
    // function_body(out, &promoted_body, format!("promoted[{}:{}]", item.name(), idx).as_str());
    println!("{:#?}", promoted_body);
  }
}

// unwrap early binder in a default manner; panic on error
fn default_unwrap_early_binder<'tcx, T>(tcx: TyCtxt<'tcx>, id: DefId, v: EarlyBinder<T>) -> T
  where T: TypeFoldable<TyCtxt<'tcx>>
{
  tcx.instantiate_and_normalize_erasing_regions(GenericArgs::identity_for_item(tcx, id), tcx.param_env(id), v)
}

pub fn print_type<'tcx>(tcx: TyCtxt<'tcx>, id: DefId, ty: EarlyBinder<Ty<'tcx>>) -> String {
  // lookup type kind in order to perform case analysis
  let kind: &TyKind = ty.skip_binder().kind();
  if let TyKind::FnDef(fun_id, args) = kind {
    // since FnDef doesn't contain signature, lookup actual function type
    // via getting fn signature with parameters and resolving those parameters
    let sig0 = tcx.fn_sig(fun_id);
    let sig1 = tcx.instantiate_and_normalize_erasing_regions(args, tcx.param_env(fun_id), sig0);
    let sig2: FnSig<'_> = tcx.instantiate_bound_regions_with_erased(sig1);
    format!("\nTyKind(FnDef): {:#?}", sig2)
  } else {
    let kind = default_unwrap_early_binder(tcx, id, ty);
    format!("\nTyKind: {:#?}", kind)
  }
}

pub fn print_item_details(tcx: TyCtxt<'_>, id: DefId, item: &stable_mir::CrateItem) {
  // Internal Details
  //
  // get DefKind for item
  let internal_kind: DefKind = tcx.def_kind(id);
  // get DefPath for item
  let path: DefPath = tcx.def_path(id);
  // get string version of DefPath
  let path_str: String = tcx.def_path_str(id);
  // get type, generic parameters, required predicates, layout
  let ty_str = print_type(tcx, id, tcx.type_of(id));
  let generics_chain = print_generics_chain(tcx, Some(id));
  // let layout = tcx.layout_of(id);
  println!("===Internal===\nDefId: {:#?}\nDefKind: {:#?}\nDefPath: {:#?}\nDefPathStr: {}{ty_str}{generics_chain}",
           id, internal_kind, path, path_str);

  // Stable Details
  //
  // get stable MIR kind
  let kind = item.kind();
  // get MIR Symbol
  let name: Symbol = item.name();
  println!("===SMIR===\nkind: {:#?}\nname: {:#?}",
           kind, name);
}

pub fn print_all_items(tcx: TyCtxt<'_>) {
  let mut out = io::stdout();
  for item in stable_mir::all_local_items().iter() {
    print_item(tcx, item, &mut out);
  }
}

pub fn has_attr(tcx: TyCtxt<'_>, item: &stable_mir::CrateItem, attr: Symbol) -> bool {
   tcx.has_attr(rustc_internal::internal(tcx,item), sym::test)
}

pub fn print_all_items_verbose(tcx: TyCtxt<'_>) {
  let mut out = io::stdout();
  // find entrypoints and constants
  for item in stable_mir::all_local_items().iter() { // .filter(|item| has_attr(item, sym::test) or matches!(item.kind, ItemKind::Const | ItemKind::Static | ItemKind::Fn))  {
    let id = rustc_internal::internal(tcx, item);
    print_item_details(tcx, id, item);
    print_item(tcx, item, &mut out);
  }
}
