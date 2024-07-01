use std::{fs::File,io,iter::Iterator,vec::Vec,str};
extern crate rustc_middle;
extern crate rustc_monomorphize;
extern crate rustc_session;
extern crate rustc_span;
extern crate rustc_smir;
extern crate stable_mir;
// HACK: typically, we would source serde/serde_json separately from the compiler
//       However, due to issues matching crate versions when we have our own serde
//       in addition to the rustc serde, we force ourselves to use rustc serde
extern crate serde;
extern crate serde_json;
use rustc_middle::ty::{TyCtxt, Ty, TyKind, EarlyBinder, FnSig, GenericArgs, TypeFoldable, ParamEnv}; // Binder, Generics, GenericPredicates
use rustc_session::config::{OutFileName, OutputType};
use rustc_span::{def_id::DefId, symbol}; // symbol::sym::test;
use rustc_smir::rustc_internal;
use stable_mir::{CrateItem,CrateDef,ItemKind,mir::Body,ty::ForeignItemKind,mir::mono::{MonoItem,Instance,InstanceKind},visited_tys,visited_alloc_ids}; // Symbol
use tracing::enabled;
use serde::Serialize;
use crate::kani_collector::{filter_crate_items, collect_all_mono_items};

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
enum MonoItemKind {
    MonoItemFn,
    MonoItemStatic,
    MonoItemGlobalAsm
}
#[derive(Serialize)]
struct Item {
    name: String,
    mono_item_kind: MonoItemKind,
    id: Option<stable_mir::DefId>,
    instance_kind: Option<InstanceKind>,
    item_kind: Option<ItemKind>,
    body: Option<MirBody>,
    promoted: Option<Vec<MirBody>>,
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
fn default_unwrap_early_binder<'tcx, T>(tcx: TyCtxt<'tcx>, id: DefId, v: EarlyBinder<'tcx, T>) -> T
  where T: TypeFoldable<TyCtxt<'tcx>>
{
  let v_copy = v.clone();
  match tcx.try_instantiate_and_normalize_erasing_regions(GenericArgs::identity_for_item(tcx, id), tcx.param_env(id), v) {
      Ok(res) => return res,
      Err(err) => { println!("{:?}", err); v_copy.skip_binder() }
  }
}

fn print_type<'tcx>(tcx: TyCtxt<'tcx>, id: DefId, ty: EarlyBinder<'tcx, Ty<'tcx>>) -> String {
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

fn mk_item(tcx: TyCtxt<'_>, item: MonoItem) -> Item {
  match item {
    MonoItem::Fn(item) => {
      let body = item.body();
      let id = item.def.def_id();
      let name = item.name();
      let internal_id = rustc_internal::internal(tcx,id);
      Item {
        name: name.clone(),
        mono_item_kind: MonoItemKind::MonoItemFn,
        id:   Some(id),
        instance_kind: Some(item.kind),
        item_kind: CrateItem::try_from(item).map_or(None, |item| Some(item.kind())),
        body: body.map_or(None, |body| { Some(mk_mir_body(body, Some(&name))) }),
        promoted: if item.has_body() { Some(tcx.promoted_mir(internal_id).into_iter().map(|body| mk_mir_body(rustc_internal::stable(body), None)).collect()) } else { None },
        details: get_item_details(tcx, internal_id),
      }
    },
    MonoItem::Static(static_def) => {
      let internal_id = rustc_internal::internal(tcx,static_def.def_id());
      Item {
        name: static_def.name(),
        mono_item_kind: MonoItemKind::MonoItemStatic,
        id:   Some(static_def.def_id()),
        instance_kind: None,
        item_kind: None,
        body: None,
        promoted: None,
        details: get_item_details(tcx, internal_id),
      }
    },
    MonoItem::GlobalAsm(asm) => {
      Item {
        name: format!("{:#?}", asm),
        mono_item_kind: MonoItemKind::MonoItemGlobalAsm,
        id:   None,
        instance_kind: None,
        item_kind: None,
        body: None,
        promoted: None,
        details: None,
      }
    }
  }
}

fn kani_collect(tcx: TyCtxt<'_>, opts: String) -> Vec<Item> {
  let collect_all = opts == "ALL";
  let main_instance = stable_mir::entry_fn().map(|main_fn| Instance::try_from(main_fn).ok()).flatten();
  let initial_mono_items: Vec<MonoItem> = filter_crate_items(tcx, |_, instance| {
    let def_id = rustc_internal::internal(tcx, instance.def.def_id());
    Some(instance) == main_instance || (collect_all && tcx.is_reachable_non_generic(def_id))
  })
    .into_iter()
    .map(MonoItem::Fn)
    .collect();
  collect_all_mono_items(tcx, &initial_mono_items).iter().map(|item| mk_item(tcx, item.clone())).collect()
}

fn mono_collect(tcx: TyCtxt<'_>) -> Vec<Item> {
  let units = tcx.collect_and_partition_mono_items(()).1;
  units.iter().flat_map(|unit| {
    unit.items_in_deterministic_order(tcx).iter().map(|(internal_item,_)| {
      mk_item(tcx, rustc_internal::stable(internal_item))
    }).collect::<Vec<_>>()
  }).collect()
}

fn emit_smir_internal(tcx: TyCtxt<'_>, writer: &mut dyn io::Write) {
  let local_crate = stable_mir::local_crate();
  let items = if let Ok(opts) = std::env::var("USE_KANI_PORT") {
    kani_collect(tcx, opts)
  } else {
    mono_collect(tcx)
  };
  write!(writer, "{{\"name\": {}, \"items\": {}, \"allocs\": {}, \"types\": {}}}",
    serde_json::to_string(&local_crate.name).expect("serde_json string failed"),
    serde_json::to_string(&items).expect("serde_json mono items failed"),
    serde_json::to_string(&visited_alloc_ids()).expect("serde_json global allocs failed"),
    serde_json::to_string(&visited_tys()).expect("serde_json tys failed")
  ).expect("Failed to write JSON to file");
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
