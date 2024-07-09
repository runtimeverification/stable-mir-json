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
use rustc_middle as middle;
use rustc_middle::ty::{TyCtxt, Ty, TyKind, EarlyBinder, FnSig, GenericArgs, TypeFoldable, ParamEnv}; // Binder, Generics, GenericPredicates
use rustc_session::config::{OutFileName, OutputType};
use rustc_span::{def_id::DefId, symbol, DUMMY_SP}; // symbol::sym::test;
use rustc_smir::rustc_internal;
use stable_mir::{CrateItem,CrateDef,ItemKind,mir::{Body,TerminatorKind,Operand},ty::{Allocation,ForeignItemKind,FnDef},mir::mono::{MonoItem,Instance,InstanceKind},visited_tys,visited_alloc_ids}; // Symbol
use tracing::enabled;
use serde::Serialize;
use crate::kani_lib::kani_collector::{filter_crate_items, collect_all_mono_items};

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
    MonoItemFn {
      name: String,
      id: stable_mir::DefId,
      instance_kind: InstanceKind,
      item_kind: Option<ItemKind>,
      body: Option<MirBody>,
      promoted: Vec<MirBody>,
    },
    MonoItemStatic {
      name: String,
      id: stable_mir::DefId,
      allocation: Option<Allocation>,
    },
    MonoItemGlobalAsm {
      asm: String,
    },
}
#[derive(Serialize)]
struct Item {
    symbol_name: String,
    mono_item_kind: MonoItemKind,
    details: Option<ItemDetails>,
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
enum FnSym {
    NoOpSym(FnDef, stable_mir::ty::GenericArgs),              // this function type corresponds to a no-op, so call can be optimized away
    IntrinsicSym(FnDef, stable_mir::ty::GenericArgs, String), // this function type corresponds to an intrinsic with the given name, so it has a built-in meaning
    NormalSym(FnDef, stable_mir::ty::GenericArgs, String),    // this function type corresponds to a linkable function, which we must look up in memory
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

fn get_promoted(tcx: TyCtxt<'_>, inst: &Instance) -> Vec<Body> {
  let id = rustc_internal::internal(tcx, inst.def.def_id());
  if inst.has_body() { tcx.promoted_mir(id).into_iter().map(rustc_internal::stable).collect() } else { vec![] }
}

fn mk_item(tcx: TyCtxt<'_>, item: &MonoItem, sym_name: String) -> Item {
  match item {
    MonoItem::Fn(item) => {
      let body = item.body();
      let id = item.def.def_id();
      let name = item.name();
      let internal_id = rustc_internal::internal(tcx,id);
      Item {
        symbol_name: sym_name,
        mono_item_kind: MonoItemKind::MonoItemFn {
          name: name.clone(),
          id: id,
          instance_kind: item.kind,
          item_kind: CrateItem::try_from(item.clone()).map_or(None, |item| Some(item.kind())),
          body: body.map_or(None, |body| { Some(mk_mir_body(body, Some(&name))) }),
          promoted: get_promoted(tcx, &item).into_iter().map(|body| mk_mir_body(body, None)).collect(),
        },
        details: get_item_details(tcx, internal_id),
      }
    },
    MonoItem::Static(static_def) => {
      let internal_id = rustc_internal::internal(tcx,static_def.def_id());
      let alloc = match static_def.eval_initializer() {
          Ok(alloc) => Some(alloc),
          err       => { println!("StaticDef({:#?}).eval_initializer() failed with: {:#?}", static_def, err); None }
      };
      Item {
        symbol_name: sym_name,
        mono_item_kind: MonoItemKind::MonoItemStatic {
          name: static_def.name(),
          id: static_def.def_id(),
          allocation: alloc,
        },
        details: get_item_details(tcx, internal_id),
      }
    },
    MonoItem::GlobalAsm(asm) => {
      Item {
        symbol_name: sym_name,
        mono_item_kind: MonoItemKind::MonoItemGlobalAsm { asm: format!("{:#?}", asm) },
        details: None,
      }
    }
  }
}

fn kani_collect(tcx: TyCtxt<'_>, opts: String) -> Vec<MonoItem> {
  let collect_all = opts == "ALL";
  let main_instance = stable_mir::entry_fn().map(|main_fn| Instance::try_from(main_fn).ok()).flatten();
  let initial_mono_items: Vec<MonoItem> = filter_crate_items(tcx, |_, instance| {
    let def_id = rustc_internal::internal(tcx, instance.def.def_id());
    Some(instance) == main_instance || (collect_all && tcx.is_reachable_non_generic(def_id))
  })
    .into_iter()
    .map(MonoItem::Fn)
    .collect();
  collect_all_mono_items(tcx, &initial_mono_items)
}

fn mono_collect(tcx: TyCtxt<'_>) -> Vec<MonoItem> {
  let units = tcx.collect_and_partition_mono_items(()).1;
  units.iter().flat_map(|unit| {
    unit.items_in_deterministic_order(tcx).iter().map(|(internal_item, _)| rustc_internal::stable(internal_item)).collect::<Vec<_>>()
  }).collect()
}

// fn handle_intrinsic(tcx: TyCtxt<'_>, inst: middle::ty::Instance) -> FnSym {
//   let _ = tcx;
//   let _ = inst;
// }

fn fn_def_ty_sym(tcx: TyCtxt<'_>, ty: &stable_mir::ty::Ty) -> Option<FnSym> {
  use middle::ty::InstanceKind::*;
  use stable_mir::ty::{TyKind::RigidTy, RigidTy::FnDef};
  match ty.kind() {
    RigidTy(ref fn_def_ty @ FnDef(fn_def, ref args)) => {
      let (def, int_args) = match rustc_internal::internal(tcx, fn_def_ty) {
        middle::ty::TyKind::FnDef(def, int_args) => (def, int_args),
        _ => panic!("rustc_internal(FnDef) did not return FnDef")
      };
      let inst = middle::ty::Instance::expect_resolve(tcx, ParamEnv::reveal_all(), def, int_args, DUMMY_SP).polymorphize(tcx);
      let fn_sym = match inst.def {
        DropGlue(_, None) | AsyncDropGlueCtorShim(_, None) => FnSym::NoOpSym(fn_def, args.clone()),
        Virtual(_, _) | _ => FnSym::NormalSym(fn_def, args.clone(), tcx.symbol_name(inst).name.into()),
        // Intrinsic(_) => handle_intrinsic(tcx, inst),
      };
      Some(fn_sym)
    }
    _ => None
  }
}

fn collect_fn_calls_inner(tcx: TyCtxt<'_>, body: &Body, add_fn: &mut impl FnMut(FnSym)) {
  use stable_mir::mir::{TerminatorKind::{Call, Drop}, Operand::Constant, ConstOperand};
  use stable_mir::ty::MirConst;
  use middle::ty::InstanceKind::{DropGlue, AsyncDropGlueCtorShim};
  for block in body.blocks.iter() {
    let fn_sym = match &block.terminator.kind {
      Call { func: Constant(ConstOperand { const_: cnst, .. }), args: _, .. } => {
        if *cnst.kind() != stable_mir::ty::ConstantKind::ZeroSized { return }
        Some(fn_def_ty_sym(tcx, &cnst.ty()).expect("Direct calls to functions must return a function name"))
      }
      Drop { place, .. } => {
        let drop_ty = place.ty(body.locals()).unwrap();
        let inst = rustc_internal::stable(middle::ty::Instance::resolve_drop_in_place(tcx, rustc_internal::internal(tcx, drop_ty)));
        fn_def_ty_sym(tcx, &inst.ty())
      }
      _ => None
    };
    if let Some(fn_sym) = fn_sym { add_fn(fn_sym) };
  }
}

fn update_link_map(link_map: &mut std::collections::HashMap<(FnDef, u64), String>, fn_sym: FnSym, check_collision: bool) {
  use std::hash::{Hash, Hasher};
  let (fn_def, args, name) = match fn_sym {
    FnSym::NoOpSym(fn_def, args) => (fn_def, args, "".into()),
    FnSym::IntrinsicSym(fn_def, args, name) => (fn_def, args, name),
    FnSym::NormalSym(fn_def, args, name) => (fn_def, args, name),
  };
  let mut hasher = std::hash::DefaultHasher::new();
  for arg in args.0.iter() {
    format!("{:?}", arg).hash(&mut hasher);
  }
  if let Some(old_name) = link_map.insert((fn_def, hasher.finish()), name.clone()) {
    if old_name != name {
      panic!("Checking collisions: {}, Added inconsistent entries into link map! {:?} -> {}, {}", check_collision, (fn_def, &args.0), old_name, name);
    }
  }
  if check_collision {
    println!("Regenerated link map entry: {:?} -> {}", (fn_def, &args.0), name);
  }
}

fn collect_fn_calls(tcx: TyCtxt<'_>, items: Vec<MonoItem>) -> Vec<((stable_mir::ty::FnDef, u64), String)> {
  use std::collections::HashMap;
  use MonoItemKind::*;
  let mut hash_map = HashMap::new();
  let ref mut add_to_hash_map = |fn_sym| { update_link_map(&mut hash_map, fn_sym, false) };
  for item in items.iter() {
    if let MonoItem::Fn ( inst ) = item {
       if let Some(fn_sym) = fn_def_ty_sym(tcx, &inst.ty()) { add_to_hash_map(fn_sym) }
    }
  }
  let ref mut add_to_hash_map = |fn_sym| { update_link_map(&mut hash_map, fn_sym, true) };
  for item in items.iter() {
    match &item {
      MonoItem::Fn( inst ) => {
        if let Some(ref body) = inst.body() {
          collect_fn_calls_inner(tcx, body, add_to_hash_map);
          get_promoted(tcx, inst).iter().for_each(|body| collect_fn_calls_inner(tcx, body, add_to_hash_map));
        }
      }
      kind @ MonoItem::Static { .. } => {}
      kind @ MonoItem::GlobalAsm { .. } => {}
    }
  }
  let calls: Vec<_> = hash_map.into_iter().collect();
  // calls.sort_by(|fst,snd| rustc_internal::internal(tcx, fst.0.def_id()).cmp(rustc_internal::internal(tcx, snd.0.def_id())));
  calls
}

fn emit_smir_internal(tcx: TyCtxt<'_>, writer: &mut dyn io::Write) {
  let local_crate = stable_mir::local_crate();
  let mono_items = if let Ok(opts) = std::env::var("USE_KANI_PORT") {
    kani_collect(tcx, opts)
  } else {
    mono_collect(tcx)
  };
  let called_functions = collect_fn_calls(tcx, mono_items.clone());
  let items = mono_items.iter().map(|item|
     mk_item(tcx, item, rustc_internal::internal(tcx, item).symbol_name(tcx).name.into())
  ).collect::<Vec<_>>();
  let mut crates = vec![local_crate.clone()];
  crates.append(&mut stable_mir::external_crates());
  let foreign_modules: Vec<_> = crates.into_iter().map(|krate| {
      ( krate.name.clone(),
        krate.foreign_modules().into_iter().map(|mod_def| {
          let fmod = mod_def.module();
          ForeignModule { name: mod_def.name(), items: fmod.items().into_iter().map(|def| ForeignItem { name: def.name(), kind: def.kind() }).collect() }
        }).collect::<Vec<_>>()
      )
  }).collect();
  write!(writer, "{{\"name\": {}, \"items\": {}, \"allocs\": {}, \"types\": {}, \"functions\": {}, \"foreign_modules\": {}}}",
    serde_json::to_string(&local_crate.name).expect("serde_json string failed"),
    serde_json::to_string(&items).expect("serde_json mono items failed"),
    serde_json::to_string(&visited_alloc_ids()).expect("serde_json global allocs failed"),
    serde_json::to_string(&visited_tys()).expect("serde_json tys failed"),
    serde_json::to_string(&called_functions).expect("serde_json functions failed"),
    serde_json::to_string(&foreign_modules).expect("foreign_module serialization failed"),
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
