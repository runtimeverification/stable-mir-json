use std::fs::File;
use std::io;
use std::iter::Iterator;
use std::vec::Vec;
use std::str;
extern crate rustc_data_structures;
extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_session;
extern crate rustc_span;
extern crate rustc_smir;
extern crate stable_mir;
// use rustc_hir::{def::DefKind, definitions::DefPath};
use tracing::{debug, debug_span, trace};
use rustc_data_structures::fx::FxHashSet;
use rustc_middle::ty::{TyCtxt, Ty, TyKind, EarlyBinder, FnSig, GenericArgs, TypeFoldable, ParamEnv, VtblEntry}; // Binder Generics, GenericPredicates
use rustc_session::config::{OutFileName, OutputType};
use rustc_span::{def_id::DefId, symbol}; // symbol::sym::test;
use rustc_smir::rustc_internal;
use stable_mir::{CrateDef,ItemKind,to_json,mir::Body,ty::ForeignItemKind, mir::mono::{Instance, InstanceKind, MonoItem}}; // Symbol
use stable_mir::ty::{Allocation, RigidTy, Ty as TyStable, TyKind as TyKindStable};
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
    id: stable_mir::DefId,
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
struct InstanceData {
    internal_id: String,
    instance: Instance,
}
#[derive(Serialize)]
struct CrateData {
    name: String,
    items: Vec<Item>,
    foreign_modules: Vec<ForeignModule>,
    upstream_monomorphizations: String,
    upstream_monomorphizations_resolved: Vec<InstanceData>,
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
  // From kani compiler_interface.rs
  // From kani reachability.rs
  let main_instance:Option<Instance> = stable_mir::entry_fn().map(|main_fn| Instance::try_from(main_fn).unwrap());
  let initial_mono_items: Vec<MonoItem> = filter_crate_items(tcx, |_, instance| {
    let def_id = rustc_internal::internal(tcx, instance.def.def_id());
    Some(instance) == main_instance || tcx.is_reachable_non_generic(def_id)
  })
    .into_iter()
    .map(MonoItem::Fn)
    .collect();
  let _all_mono_items = collect_all_mono_items(tcx, &initial_mono_items);
  let items: Vec<Item> = stable_mir::all_local_items().iter().map(|item| {
    let body = item.body();
    let id = rustc_internal::internal(tcx,item.def_id());
    Item {
      name: item.name(),
      id:   item.def_id(),
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
  let mono_map_str = format!("{:?}", tcx.upstream_monomorphizations(()));
  let mono_map: Vec<InstanceData> = tcx.with_stable_hashing_context(|ref hcx| {
     tcx.upstream_monomorphizations(()).to_sorted(hcx, false).into_iter().flat_map(|(id, monos)| {
      monos.to_sorted(hcx, false).into_iter().map(|(args, _crate_num)| {
          let inst = rustc_internal::stable(rustc_middle::ty::Instance::resolve(tcx, ParamEnv::reveal_all(), *id, args).ok().flatten());
          if let Some(inst) = inst {
            Some(InstanceData {
                internal_id: format!("{:?}", id.clone()),
                instance: inst
            })
          } else {
            None
          }
      })
    }).flatten().collect()
  });
  let crate_data = CrateData { name: local_crate.name,
                               items: items,
                               foreign_modules: foreign_modules,
                               upstream_monomorphizations: mono_map_str,
                               upstream_monomorphizations_resolved: mono_map,
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

fn collect_all_mono_items(tcx: TyCtxt, initial_mono_items: &[MonoItem]) -> Vec<MonoItem> {
  let mut collector = MonoItemsCollector::new(tcx);
  for item in initial_mono_items {
    collector.collect(item.clone());
  }
  vec![]
}

struct MonoItemsCollector<'tcx> {
  /// The compiler context.
  tcx: TyCtxt<'tcx>,
  /// Set of collected items used to avoid entering recursion loops.
  collected: FxHashSet<MonoItem>,
  /// Items enqueued for visiting.
  queue: Vec<MonoItem>,
}

impl<'tcx> MonoItemsCollector<'tcx> {
  pub fn new(tcx: TyCtxt<'tcx>) -> Self {
    MonoItemsCollector {
      tcx,
      collected: FxHashSet::default(),
      queue: vec![],
    }
  }

  pub fn collect(&mut self, root: MonoItem) {
    self.queue.push(root);
    self.reachable_items();
  }

  fn reachable_items(&mut self) {
    while let Some(to_visit) = self.queue.pop() {
      if !self.collected.contains(&to_visit) {
        self.collected.insert(to_visit.clone());
        let next_items = match &to_visit {
          MonoItem::Fn(instance) => self.visit_fn(*instance),
          MonoItem::Static(static_def) => todo!(),
          MonoItem::GlobalAsm(_) => {
            vec![]
          },
        };

        self.queue
          .extend(next_items.into_iter().filter(|item| !self.collected.contains(item)));
      }
    }
  }

  fn visit_fn(&mut self, instance: Instance) -> Vec<MonoItem> {
    let _guard = debug_span!("visit_fn", function=?instance).entered();
    let body = instance.body();
    let mut collector = MonoItemsFnCollector { tcx: self.tcx, collected: FxHashSet::default(), body: &body.unwrap() };
    vec![]
  }
}

struct MonoItemsFnCollector<'a, 'tcx> {
  tcx: TyCtxt<'tcx>,
  collected: FxHashSet<MonoItem>,
  body: &'a Body,
}

// impl<'a, 'tcx> MonoItemsFnCollector<'a, 'tcx> {
//   /// Collect the implementation of all trait methods and its supertrait methods for the given
//   /// concrete type.
//   fn collect_vtable_methods(&mut self, concrete_ty: TyStable, trait_ty: TyStable) {
//       trace!(?concrete_ty, ?trait_ty, "collect_vtable_methods");
//       let concrete_kind = concrete_ty.kind();
//       let trait_kind = trait_ty.kind();

//       assert!(!concrete_kind.is_trait(), "expected a concrete type, but found `{concrete_ty:?}`");
//       assert!(trait_kind.is_trait(), "expected a trait `{trait_ty:?}`");
//       if let Some(principal) = trait_kind.trait_principal() {
//           // A trait object type can have multiple trait bounds but up to one non-auto-trait
//           // bound. This non-auto-trait, named principal, is the only one that can have methods.
//           // https://doc.rust-lang.org/reference/special-types-and-traits.html#auto-traits
//           let poly_trait_ref = principal.with_self_ty(concrete_ty);

//           // Walk all methods of the trait, including those of its supertraits
//           let entries =
//               self.tcx.vtable_entries(rustc_internal::internal(self.tcx, &poly_trait_ref));
//           let methods = entries.iter().filter_map(|entry| match entry {
//               VtblEntry::MetadataAlign
//               | VtblEntry::MetadataDropInPlace
//               | VtblEntry::MetadataSize
//               | VtblEntry::Vacant => None,
//               VtblEntry::TraitVPtr(_) => {
//                   // all super trait items already covered, so skip them.
//                   None
//               }
//               VtblEntry::Method(instance) => {
//                   let instance = rustc_internal::stable(instance);
//                   should_codegen_locally(&instance).then_some(MonoItem::Fn(instance))
//               }
//           });
//           trace!(methods=?methods.clone().collect::<Vec<_>>(), "collect_vtable_methods");
//           self.collected.extend(methods);
//       }

//       // Add the destructor for the concrete type.
//       let instance = Instance::resolve_drop_in_place(concrete_ty);
//       self.collect_instance(instance, false);
//   }

//   /// Collect an instance depending on how it is used (invoked directly or via fn_ptr).
//   fn collect_instance(&mut self, instance: Instance, is_direct_call: bool) {
//       let should_collect = match instance.kind {
//           InstanceKind::Virtual { .. } => {
//               // Instance definition has no body.
//               assert!(is_direct_call, "Expected direct call {instance:?}");
//               false
//           }
//           InstanceKind::Intrinsic => {
//               // Intrinsics may have a fallback body.
//               assert!(is_direct_call, "Expected direct call {instance:?}");
//               let TyKindStable::RigidTy(RigidTy::FnDef(def, _)) = instance.ty().kind() else {
//                   unreachable!("Expected function type for intrinsic: {instance:?}")
//               };
//               // The compiler is currently transitioning how to handle intrinsic fallback body.
//               // Until https://github.com/rust-lang/project-stable-mir/issues/79 is implemented
//               // we have to check `must_be_overridden` and `has_body`.
//               !def.as_intrinsic().unwrap().must_be_overridden() && instance.has_body()
//           }
//           InstanceKind::Shim | InstanceKind::Item => true,
//       };
//       if should_collect && should_codegen_locally(&instance) {
//           trace!(?instance, "collect_instance");
//           self.collected.insert(instance.into());
//       }
//   }

//   /// Collect constant values represented by static variables.
//   fn collect_allocation(&mut self, alloc: &Allocation) {
//       debug!(?alloc, "collect_allocation");
//       for (_, id) in &alloc.provenance.ptrs {
//           self.collected.extend(collect_alloc_items(id.0).into_iter())
//       }
//   }
// }

/// Collect all (top-level) items in the crate that matches the given predicate.
/// An item can only be a root if they are a non-generic function.
pub fn filter_crate_items<F>(tcx: TyCtxt, predicate: F) -> Vec<Instance>
where
    F: Fn(TyCtxt, Instance) -> bool,
{
    let crate_items = stable_mir::all_local_items();
    // Filter regular items.
    crate_items
        .iter()
        .filter_map(|item| {
            // Only collect monomorphic items.
            // TODO: Remove the def_kind check once https://github.com/rust-lang/rust/pull/119135 has been released.
            let def_id = rustc_internal::internal(tcx, item.def_id());
            (matches!(tcx.def_kind(def_id), rustc_hir::def::DefKind::Ctor(..))
                || matches!(item.kind(), ItemKind::Fn))
            .then(|| {
                Instance::try_from(*item)
                    .ok()
                    .and_then(|instance| predicate(tcx, instance).then_some(instance))
            })
            .flatten()
        })
        .collect::<Vec<_>>()
}