use std::fs::File;
use std::io;
use std::iter::Iterator;
use std::vec::Vec;
use std::str;
// extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_session;
extern crate rustc_span;
extern crate rustc_smir;
extern crate stable_mir;
// use rustc_hir::{def::DefKind, definitions::DefPath};
use rustc_middle::ty::{TyCtxt, Ty, TyKind, EarlyBinder, FnSig, GenericArgs, TypeFoldable}; // Binder Generics, GenericPredicates, ParamEnv
use rustc_session::config::{OutFileName, OutputType};
use rustc_span::{def_id::DefId, symbol}; // symbol::sym::test;
use rustc_smir::rustc_internal;
use stable_mir::{CrateDef,ItemKind,to_json,mir::Body,ty::ForeignItemKind}; // Symbol, mir::mono::Instance
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
#[derive(Serialize)]
struct ForeignItem {
    name: String,
    kind: ForeignItemKind,
}
#[derive(Serialize)]
struct ForeignModule {
    name: String,
    items: Vec<ForeignItem>,
}
#[derive(Serialize)]
struct CrateData {
    name: String,
    items: Vec<Item>,
    foreign_modules: Vec<ForeignModule>,
    upstream_monomorphizations: String // Vec<Instance>,
}

fn generic_data(tcx: TyCtxt<'_>, id: DefId) -> GenericData {
     let mut v = Vec::new();
     let mut next_id = Some(id);
     while let Some(_curr_id) = next_id {
        let params = tcx.generics_of(id);
        let preds  = tcx.predicates_of(id);
        if params.parent != preds.parent { panic!("Generics and predicates parent ids are distinct"); }
        v.push((format!("{:#?}", params), format!("{:#?}", preds)));
        next_id = params.parent;
     }
     v.reverse();
     return GenericData(v);
}

fn get_body_details(body: &Body, name: Option<&String>) -> Option<BodyDetails> {
  if enabled!(tracing::Level::DEBUG) {
    let mut v = Vec::new();
    let name = if let Some(name) = name { name } else { "<promoted>" };
    let _ = body.dump(&mut v, name);
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

fn print_type<'tcx>(tcx: TyCtxt<'tcx>, id: DefId, ty: EarlyBinder<Ty<'tcx>>) -> String {
  // lookup type kind in order to perform case analysis
  let kind: &TyKind = ty.skip_binder().kind();
  if let TyKind::FnDef(fun_id, args) = kind {
    // since FnDef doesn't contain signature, lookup actual function type
    // via getting fn signature with parameters and resolving those parameters
    let sig0 = tcx.fn_sig(fun_id);
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

fn get_item_details(tcx: TyCtxt<'_>, id: DefId) -> Option<ItemDetails> {
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

// Possible input: sym::test
pub fn has_attr(tcx: TyCtxt<'_>, item: &stable_mir::CrateItem, attr: symbol::Symbol) -> bool {
   tcx.has_attr(rustc_internal::internal(tcx,item), attr)
}

fn mk_mir_body(body: Body, name: Option<&String>) -> MirBody {
  let details = get_body_details(&body, name);
  MirBody(body, details)
}

// TODO: Should we filter any incoming items?
//       Example: .filter(|item| has_attr(item, sym::test) or matches!(item.kind, ItemKind::Const | ItemKind::Static | ItemKind::Fn))
fn emit_smir_internal(tcx: TyCtxt<'_>, writer: &mut dyn io::Write) {
  let local_crate = stable_mir::local_crate();
  let items: Vec<Item> = stable_mir::all_local_items().iter().map(|item| {
    let body = item.body();
    let id = rustc_internal::internal(tcx,item.def_id());
    Item {
      name: item.name(),
      kind: item.kind(),
      body: mk_mir_body(body, Some(&item.name())),
      promoted: tcx.promoted_mir(id).into_iter().map(|body| mk_mir_body(rustc_internal::stable(body), None)).collect(),
      details: get_item_details(tcx, id),
    }
  }).collect();
  let foreign_modules: Vec<ForeignModule> = local_crate.foreign_modules().into_iter().map(|module_def| {
      ForeignModule {
        name: module_def.name(),
        items: module_def.module().items().into_iter().map(|item| ForeignItem { name: item.name(), kind: item.kind() }).collect()
      }
  }).collect();
  // let mono_map: Vec<Instance> = tcx.with_stable_hashing_context(|ref hcx| {
  //    tcx.upstream_monomorphizations(()).to_sorted(hcx, false).into_iter().flat_map(|(id, monos)| {
  //     monos.to_sorted(hcx, false).into_iter().map(|(args, _crate_num)| {
  //         rustc_internal::stable(rustc_middle::ty::Instance::resolve(tcx, ParamEnv::reveal_all(), *id, args).ok().flatten())
  //     })
  //   }).flatten().collect()
  // });
  let mono_map = format!("{:?}", tcx.upstream_monomorphizations(()));
  let crate_data = CrateData { name: local_crate.name,
                               items: items,
                               foreign_modules: foreign_modules,
                               upstream_monomorphizations: mono_map,
                             };
  writer.write_all(to_json(crate_data).expect("serde_json failed").as_bytes()).expect("internal error: writing SMIR JSON failed");
}

pub fn emit_smir(tcx: TyCtxt<'_>) {
  match tcx.output_filenames(()).path(OutputType::Mir) {
    OutFileName::Stdout => {
        let mut f = io::stdout();
        emit_smir_internal(tcx, &mut f);
    }
    OutFileName::Real(path) => {
        let mut f = io::BufWriter::new(File::create(&path.with_extension("smir.json")).expect("Failed to create SMIR output file"));
        emit_smir_internal(tcx, &mut f);
    }
  }
}
