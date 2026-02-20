use std::hash::Hash;
use std::io::Write;
use std::ops::ControlFlow;
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io,
    iter::Iterator,
    str,
    vec::Vec,
};
extern crate rustc_middle;
extern crate rustc_monomorphize;
extern crate rustc_session;
extern crate rustc_smir;
extern crate rustc_span;
extern crate stable_mir;
// HACK: typically, we would source serde/serde_json separately from the compiler
//       However, due to issues matching crate versions when we have our own serde
//       in addition to the rustc serde, we force ourselves to use rustc serde
extern crate serde;
extern crate serde_json;
use rustc_middle as middle;
use rustc_middle::ty::{
    EarlyBinder, FnSig, GenericArgs, List, Ty, TyCtxt, TypeFoldable, TypingEnv,
};
use rustc_session::config::{OutFileName, OutputType};
use rustc_smir::rustc_internal::{self, internal};
use rustc_span::{
    def_id::{DefId, LOCAL_CRATE},
    symbol,
};
use serde::{Serialize, Serializer};
use stable_mir::{
    abi::{FieldsShape, LayoutShape},
    mir::mono::{Instance, InstanceKind, MonoItem},
    mir::{
        alloc::AllocId, alloc::GlobalAlloc, visit::MirVisitor, Body, LocalDecl, Rvalue, Terminator,
        TerminatorKind,
    },
    ty::{AdtDef, Allocation, ConstDef, ForeignItemKind, IndexedVal, RigidTy, TyKind},
    visitor::{Visitable, Visitor},
    CrateDef, CrateItem, ItemKind,
};

// Structs for serializing extra details about mono items
// ======================================================

#[derive(Serialize, Clone)]
struct BodyDetails {
    pp: String,
}

fn get_body_details(body: &Body) -> BodyDetails {
    let mut v = Vec::new();
    let _ = body.dump(&mut v, "<omitted>");
    BodyDetails {
        pp: str::from_utf8(&v).unwrap().into(),
    }
}

#[derive(Serialize, Clone)]
struct GenericData(Vec<(String, String)>); // Alternatively, GenericData<'a>(Vec<(&'a Generics,GenericPredicates<'a>)>);

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

#[derive(Serialize, Clone)]
struct ItemDetails {
    // these fields only defined for fn items
    fn_instance_kind: Option<InstanceKind>,
    fn_item_kind: Option<ItemKind>,
    fn_body_details: Option<BodyDetails>,
    // these fields defined for all items
    internal_kind: String,
    path: String,
    internal_ty: String,
    generic_data: GenericData,
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
            println!("{:?}", err);
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

fn get_item_details(
    tcx: TyCtxt<'_>,
    id: DefId,
    fn_inst: Option<Instance>,
    fn_body: Option<&Body>,
) -> Option<ItemDetails> {
    if debug_enabled() {
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

fn get_foreign_module_details() -> Vec<(String, Vec<ForeignModule>)> {
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

// Miscellaneous helper functions
// ==============================

macro_rules! def_env_var {
    ($fn_name:ident, $var_name:ident) => {
        fn $fn_name() -> bool {
            use std::sync::OnceLock;
            static VAR: OnceLock<bool> = OnceLock::new();
            *VAR.get_or_init(|| std::env::var(stringify!($var_name)).is_ok())
        }
    };
}

def_env_var!(debug_enabled, DEBUG);
def_env_var!(link_items_enabled, LINK_ITEMS);
def_env_var!(link_instance_enabled, LINK_INST);

macro_rules! debug_log_println {
    ($($args:tt)*) => {
        #[cfg(feature = "debug_log")]
        println!($($args)*);
    };
}

// Possible input: sym::test
pub fn has_attr(tcx: TyCtxt<'_>, item: &stable_mir::CrateItem, attr: symbol::Symbol) -> bool {
    tcx.has_attr(rustc_internal::internal(tcx, item), attr)
}

fn mono_item_name(tcx: TyCtxt<'_>, item: &MonoItem) -> String {
    if let MonoItem::GlobalAsm(data) = item {
        hash(data).to_string()
    } else {
        mono_item_name_int(tcx, &rustc_internal::internal(tcx, item))
    }
}

fn mono_item_name_int<'a>(tcx: TyCtxt<'a>, item: &rustc_middle::mir::mono::MonoItem<'a>) -> String {
    item.symbol_name(tcx).name.into()
}

fn fn_inst_for_ty(ty: stable_mir::ty::Ty, direct_call: bool) -> Option<Instance> {
    ty.kind().fn_def().and_then(|(fn_def, args)| {
        if direct_call {
            Instance::resolve(fn_def, args)
        } else {
            Instance::resolve_for_fn_ptr(fn_def, args)
        }
        .ok()
    })
}

fn def_id_to_inst(tcx: TyCtxt<'_>, id: stable_mir::DefId) -> Instance {
    let internal_id = rustc_internal::internal(tcx, id);
    let internal_inst = rustc_middle::ty::Instance::mono(tcx, internal_id);
    rustc_internal::stable(internal_inst)
}

fn take_any<K: Clone + std::hash::Hash + std::cmp::Eq, V>(
    map: &mut HashMap<K, V>,
) -> Option<(K, V)> {
    let key = map.keys().next()?.clone();
    map.remove(&key).map(|val| (key, val))
}

fn hash<T: std::hash::Hash>(obj: T) -> u64 {
    use std::hash::Hasher;
    let mut hasher = std::hash::DefaultHasher::new();
    obj.hash(&mut hasher);
    hasher.finish()
}

// Structs for serializing critical details about mono items
// =========================================================

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
#[derive(Serialize, Clone)]
pub struct Item {
    #[serde(skip)]
    mono_item: MonoItem,
    pub symbol_name: String,
    pub mono_item_kind: MonoItemKind,
    details: Option<ItemDetails>,
}

impl Item {
    /// Returns the pre-collected body and appropriate locals slice, if available.
    /// For functions, locals come from the body; for statics, locals are empty.
    fn body_and_locals(&self) -> Option<(&Body, &[LocalDecl])> {
        match &self.mono_item_kind {
            MonoItemKind::MonoItemFn {
                body: Some(body), ..
            } => Some((body, body.locals())),
            MonoItemKind::MonoItemStatic {
                body: Some(body), ..
            } => Some((body, &[])),
            _ => None,
        }
    }

    /// Log a warning when a body was expected but missing.
    fn warn_missing_body(&self) {
        match &self.mono_item {
            MonoItem::Fn(inst) => {
                eprintln!(
                    "Failed to retrieve body for Instance of MonoItem::Fn {}",
                    inst.name()
                );
            }
            MonoItem::Static(def) => {
                eprintln!(
                    "Failed to retrieve body for Instance of MonoItem::Static {}",
                    def.name()
                );
            }
            MonoItem::GlobalAsm(_) => {}
        }
    }
}

impl PartialEq for Item {
    fn eq(&self, other: &Item) -> bool {
        self.mono_item.eq(&other.mono_item)
    }
}
impl Eq for Item {}

impl PartialOrd for Item {
    fn partial_cmp(&self, other: &Item) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Item {
    fn cmp(&self, other: &Item) -> std::cmp::Ordering {
        use MonoItemKind::*;
        let sort_key = |i: &Item| {
            format!(
                "{}!{}",
                i.symbol_name,
                match &i.mono_item_kind {
                    MonoItemFn { name, .. } => name,
                    MonoItemStatic { name, .. } => name,
                    MonoItemGlobalAsm { asm } => asm,
                }
            )
        };
        sort_key(self).cmp(&sort_key(other))
    }
}

fn mk_item(tcx: TyCtxt<'_>, item: MonoItem, sym_name: String) -> Item {
    match item {
        MonoItem::Fn(inst) => {
            let id = inst.def.def_id();
            let name = inst.name();
            let internal_id = rustc_internal::internal(tcx, id);
            let body = inst.body();
            let details = get_item_details(tcx, internal_id, Some(inst), body.as_ref());
            Item {
                mono_item: item,
                symbol_name: sym_name.clone(),
                mono_item_kind: MonoItemKind::MonoItemFn {
                    name: name.clone(),
                    id,
                    body,
                },
                details,
            }
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
            let inst = def_id_to_inst(tcx, static_def.def_id());
            let body = inst.body();
            Item {
                mono_item: item,
                symbol_name: sym_name,
                mono_item_kind: MonoItemKind::MonoItemStatic {
                    name: static_def.name(),
                    id: static_def.def_id(),
                    allocation: alloc,
                    body,
                },
                details: get_item_details(tcx, internal_id, None, None),
            }
        }
        MonoItem::GlobalAsm(ref asm) => {
            let asm = format!("{:#?}", asm);
            Item {
                mono_item: item,
                symbol_name: sym_name,
                mono_item_kind: MonoItemKind::MonoItemGlobalAsm { asm },
                details: None,
            }
        }
    }
}

// Link-time resolution logic
// ==========================

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum FnSymType {
    NoOpSym(String),
    IntrinsicSym(String),
    NormalSym(String),
}

type FnSymInfo<'tcx> = (
    stable_mir::ty::Ty,
    middle::ty::InstanceKind<'tcx>,
    FnSymType,
);

fn fn_inst_sym<'tcx>(
    tcx: TyCtxt<'tcx>,
    ty: Option<stable_mir::ty::Ty>,
    inst: Option<&Instance>,
) -> Option<FnSymInfo<'tcx>> {
    use FnSymType::*;
    inst.and_then(|inst| {
        let ty = if let Some(ty) = ty { ty } else { inst.ty() };
        let kind = ty.kind();
        if kind.fn_def().is_some() {
            let internal_inst = rustc_internal::internal(tcx, inst);
            let sym_type = if inst.is_empty_shim() {
                NoOpSym(String::from(""))
            } else if let Some(intrinsic_name) = inst.intrinsic_name() {
                IntrinsicSym(intrinsic_name)
            } else {
                NormalSym(inst.mangled_name())
            };
            Some((ty, internal_inst.def, sym_type))
        } else {
            None
        }
    })
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct LinkMapKey<'tcx>(
    pub stable_mir::ty::Ty,
    Option<middle::ty::InstanceKind<'tcx>>,
);

impl Serialize for LinkMapKey<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeTuple;
        if link_instance_enabled() {
            let mut tup = serializer.serialize_tuple(2)?;
            tup.serialize_element(&self.0)?;
            tup.serialize_element(&format!("{:?}", self.1).as_str())?;
            tup.end()
        } else {
            <stable_mir::ty::Ty as Serialize>::serialize(&self.0, serializer)
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ItemSource(u8);
const ITEM: u8 = 1 << 0;
const TERM: u8 = 1 << 1;
const FPTR: u8 = 1 << 2;

impl Serialize for ItemSource {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(None)?;
        if self.0 & ITEM != 0u8 {
            seq.serialize_element(&"Item")?
        };
        if self.0 & TERM != 0u8 {
            seq.serialize_element(&"Term")?
        };
        if self.0 & FPTR != 0u8 {
            seq.serialize_element(&"Fptr")?
        };
        seq.end()
    }
}

type LinkMap<'tcx> = HashMap<LinkMapKey<'tcx>, (ItemSource, FnSymType)>;
/// Wrapper around the alloc-id-to-allocation map that tracks insertion
/// behavior in debug builds. This serves two purposes:
///
/// 1. Duplicate detection: if the same AllocId is inserted twice, that
///    indicates a body was walked more than once (a regression the
///    declarative pipeline is designed to prevent).
///
/// 2. Coherence verification (via `verify_coherence`): after collection,
///    checks that every AllocId in the stored Item bodies actually
///    exists in this map. A mismatch means the analysis walked a
///    different body than what's stored (the original alloc-id bug).
///
/// In release builds the tracking fields are compiled out, making this
/// a zero-cost wrapper.
struct AllocMap {
    inner: HashMap<stable_mir::mir::alloc::AllocId, (stable_mir::ty::Ty, GlobalAlloc)>,
    #[cfg(debug_assertions)]
    insert_count: usize,
    #[cfg(debug_assertions)]
    duplicate_ids: Vec<stable_mir::mir::alloc::AllocId>,
}

impl AllocMap {
    fn new() -> Self {
        Self {
            inner: HashMap::new(),
            #[cfg(debug_assertions)]
            insert_count: 0,
            #[cfg(debug_assertions)]
            duplicate_ids: Vec::new(),
        }
    }

    fn contains_key(&self, key: &stable_mir::mir::alloc::AllocId) -> bool {
        self.inner.contains_key(key)
    }

    fn insert(
        &mut self,
        key: stable_mir::mir::alloc::AllocId,
        value: (stable_mir::ty::Ty, GlobalAlloc),
    ) {
        #[cfg(debug_assertions)]
        {
            self.insert_count += 1;
            if self.inner.contains_key(&key) {
                self.duplicate_ids.push(key);
            }
        }
        self.inner.insert(key, value);
    }

    fn iter(
        &self,
    ) -> impl Iterator<
        Item = (
            &stable_mir::mir::alloc::AllocId,
            &(stable_mir::ty::Ty, GlobalAlloc),
        ),
    > {
        self.inner.iter()
    }

    fn into_entries(
        self,
    ) -> impl Iterator<Item = (stable_mir::mir::alloc::AllocId, (stable_mir::ty::Ty, GlobalAlloc))>
    {
        self.inner.into_iter()
    }

    /// Verify that alloc ids in the stored Item bodies match this map.
    ///
    /// Walks every stored body to extract AllocIds from provenance, then
    /// checks that each one exists in this map. A mismatch means the
    /// analysis phase walked a different body than what's stored in the
    /// Items (which is exactly the bug that the declarative pipeline
    /// restructuring was designed to prevent).
    #[cfg(debug_assertions)]
    fn verify_coherence(&self, items: &[Item]) {
        // Collect alloc ids referenced in stored bodies
        let mut body_ids: HashSet<stable_mir::mir::alloc::AllocId> = HashSet::new();
        for item in items {
            let body = match &item.mono_item_kind {
                MonoItemKind::MonoItemFn {
                    body: Some(body), ..
                } => Some(body),
                MonoItemKind::MonoItemStatic {
                    body: Some(body), ..
                } => Some(body),
                _ => None,
            };
            if let Some(body) = body {
                AllocIdCollector {
                    ids: &mut body_ids,
                }
                .visit_body(body);
            }
        }

        let map_ids: HashSet<_> = self.inner.keys().copied().collect();
        let missing_from_map: Vec<_> = body_ids.difference(&map_ids).collect();

        assert!(
            missing_from_map.is_empty(),
            "Alloc-id coherence violation: AllocIds {:?} are referenced in \
             stored Item bodies but missing from the alloc map. This means \
             the analysis phase collected allocations from a different body \
             than what is stored in the Items.",
            missing_from_map
        );

        assert!(
            self.duplicate_ids.is_empty(),
            "Alloc-id duplicate insertion: AllocIds {:?} were inserted into \
             the alloc map more than once, indicating a body was walked \
             multiple times.",
            self.duplicate_ids
        );
    }
}

/// MirVisitor that extracts AllocIds from provenance in Allocated constants.
/// Used by AllocMap::verify_coherence to find which alloc ids the stored
/// bodies actually reference.
#[cfg(debug_assertions)]
struct AllocIdCollector<'a> {
    ids: &'a mut HashSet<stable_mir::mir::alloc::AllocId>,
}

#[cfg(debug_assertions)]
impl MirVisitor for AllocIdCollector<'_> {
    fn visit_mir_const(
        &mut self,
        constant: &stable_mir::ty::MirConst,
        loc: stable_mir::mir::visit::Location,
    ) {
        if let stable_mir::ty::ConstantKind::Allocated(alloc) = constant.kind() {
            for (_, prov) in &alloc.provenance.ptrs {
                self.ids.insert(prov.0);
            }
        }
        self.super_mir_const(constant, loc);
    }
}
type TyMap =
    HashMap<stable_mir::ty::Ty, (stable_mir::ty::TyKind, Option<stable_mir::abi::LayoutShape>)>;
type SpanMap = HashMap<usize, (String, usize, usize, usize, usize)>;

struct TyCollector<'tcx> {
    tcx: TyCtxt<'tcx>,
    types: TyMap,
    resolved: HashSet<stable_mir::ty::Ty>,
}

impl TyCollector<'_> {
    fn new(tcx: TyCtxt<'_>) -> TyCollector {
        TyCollector {
            tcx,
            types: HashMap::new(),
            resolved: HashSet::new(),
        }
    }
}

impl TyCollector<'_> {
    #[inline(always)]
    fn visit_instance(&mut self, instance: Instance) -> ControlFlow<<Self as Visitor>::Break> {
        let fn_abi = instance.fn_abi().unwrap();
        let mut inputs_outputs: Vec<stable_mir::ty::Ty> =
            fn_abi.args.iter().map(|arg_abi| arg_abi.ty).collect();
        inputs_outputs.push(fn_abi.ret.ty);
        inputs_outputs.super_visit(self)
    }
}

impl Visitor for TyCollector<'_> {
    type Break = ();

    fn visit_ty(&mut self, ty: &stable_mir::ty::Ty) -> ControlFlow<Self::Break> {
        if self.types.contains_key(ty) || self.resolved.contains(ty) {
            return ControlFlow::Continue(());
        }

        match ty.kind() {
            TyKind::RigidTy(RigidTy::Closure(def, ref args)) => {
                self.resolved.insert(*ty);
                let instance =
                    Instance::resolve_closure(def, args, stable_mir::ty::ClosureKind::Fn).unwrap();
                self.visit_instance(instance)
            }
            // Break on CoroutineWitnesses, because they aren't expected when getting the layout
            TyKind::RigidTy(RigidTy::CoroutineWitness(..)) => ControlFlow::Break(()),
            TyKind::RigidTy(RigidTy::FnDef(def, ref args)) => {
                self.resolved.insert(*ty);
                let instance = Instance::resolve(def, args).unwrap();
                self.visit_instance(instance)
            }
            TyKind::RigidTy(RigidTy::FnPtr(binder_stable)) => {
                self.resolved.insert(*ty);
                let binder_internal = internal(self.tcx, binder_stable);
                let sig_stable = rustc_internal::stable(
                    self.tcx
                        .fn_abi_of_fn_ptr(
                            TypingEnv::fully_monomorphized()
                                .as_query_input((binder_internal, List::empty())),
                        )
                        .unwrap(),
                );
                let mut inputs_outputs: Vec<stable_mir::ty::Ty> =
                    sig_stable.args.iter().map(|arg_abi| arg_abi.ty).collect();
                inputs_outputs.push(sig_stable.ret.ty);
                inputs_outputs.super_visit(self)
            }
            // The visitor won't collect field types for ADTs, therefore doing it explicitly
            TyKind::RigidTy(RigidTy::Adt(adt_def, args)) => {
                let fields = adt_def
                    .variants()
                    .iter()
                    .flat_map(|v| v.fields())
                    .map(|f| f.ty_with_args(&args))
                    .collect::<Vec<_>>();

                let control = ty.super_visit(self);
                if matches!(control, ControlFlow::Continue(_)) {
                    let maybe_layout_shape = ty.layout().ok().map(|layout| layout.shape());
                    self.types.insert(*ty, (ty.kind(), maybe_layout_shape));
                    fields.super_visit(self)
                } else {
                    control
                }
            }
            _ => {
                let control = ty.super_visit(self);
                match control {
                    ControlFlow::Continue(_) => {
                        let maybe_layout_shape = ty.layout().ok().map(|layout| layout.shape());
                        self.types.insert(*ty, (ty.kind(), maybe_layout_shape));
                        control
                    }
                    _ => control,
                }
            }
        }
    }
}

/// Single-pass body visitor that collects all derived information from a MIR body:
/// link map entries (calls, drops, fn pointers), allocations, types, spans,
/// and unevaluated constant references (for transitive item discovery).
///
/// By combining what was previously two separate visitors (BodyAnalyzer
/// and UnevaluatedConstCollector), each body is walked exactly once.
struct BodyAnalyzer<'tcx, 'local> {
    tcx: TyCtxt<'tcx>,
    locals: &'local [LocalDecl],
    link_map: &'local mut LinkMap<'tcx>,
    visited_allocs: &'local mut AllocMap,
    ty_visitor: &'local mut TyCollector<'tcx>,
    spans: &'local mut SpanMap,
    /// Unevaluated constants discovered during this body walk.
    /// The outer fixpoint loop uses these to discover and create new Items.
    new_unevaluated: &'local mut Vec<UnevalConstInfo>,
}

/// Information about an unevaluated constant discovered during body analysis.
/// The outer fixpoint loop in collect_and_analyze_items uses this to create
/// new Items for transitively discovered mono items.
struct UnevalConstInfo {
    const_def: ConstDef,
    item_name: String,
    mono_item: MonoItem,
}

fn is_reify_shim(kind: &middle::ty::InstanceKind<'_>) -> bool {
    matches!(kind, middle::ty::InstanceKind::ReifyShim(..))
}

fn update_link_map<'tcx>(
    link_map: &mut LinkMap<'tcx>,
    fn_sym: Option<FnSymInfo<'tcx>>,
    source: ItemSource,
) {
    if fn_sym.is_none() {
        return;
    }
    let (ty, kind, name) = fn_sym.unwrap();
    let new_val = (source, name.clone());
    let key = if link_instance_enabled() {
        LinkMapKey(ty, Some(kind))
    } else {
        LinkMapKey(ty, None)
    };
    if let Some(curr_val) = link_map.get_mut(&key.clone()) {
        if curr_val.1 != new_val.1 {
            if !link_instance_enabled() {
                // When LINK_INST is disabled, prefer Item over ReifyShim.
                // ReifyShim has no body in items, so Item is more useful.
                if is_reify_shim(&kind) {
                    // New entry is ReifyShim, existing is Item → skip
                    return;
                }
                // New entry is Item, existing is ReifyShim → replace
                curr_val.1 = name;
                curr_val.0 .0 |= new_val.0 .0;
                return;
            }
            panic!(
                "Added inconsistent entries into link map! {:?} -> {:?}, {:?}",
                (ty, ty.kind().fn_def(), &kind),
                curr_val.1,
                new_val.1
            );
        }
        curr_val.0 .0 |= new_val.0 .0;
        debug_log_println!(
            "Regenerated link map entry: {:?}:{:?} -> {:?}",
            &key,
            key.0.kind().fn_def(),
            new_val
        );
    } else {
        debug_log_println!(
            "Generated link map entry from call: {:?}:{:?} -> {:?}",
            &key,
            key.0.kind().fn_def(),
            new_val
        );
        link_map.insert(key.clone(), new_val);
    }
}

/// Returns the field index (source order) for a given offset and layout if
/// the layout contains fields (shared between all variants), otherwise None.
/// NB No search for fields within variants (needs recursive call).
fn field_for_offset(l: LayoutShape, offset: usize) -> Option<usize> {
    // FieldIdx
    match l.fields {
        FieldsShape::Primitive | FieldsShape::Union(_) | FieldsShape::Array { .. } => None,
        FieldsShape::Arbitrary { offsets } => {
            let fields: Vec<usize> = offsets.into_iter().map(|o| o.bytes()).collect();
            fields
                .into_iter()
                .enumerate()
                .find(|(_, o)| *o == offset)
                .map(|(i, _)| i)
        }
    }
}

fn get_prov_ty(ty: stable_mir::ty::Ty, offset: &usize) -> Option<stable_mir::ty::Ty> {
    use stable_mir::ty::RigidTy;
    let ty_kind = ty.kind();
    // if ty is a pointer, box, or Ref, expect no offset and dereference
    if let Some(ty) = ty_kind.builtin_deref(true) {
        assert!(*offset == 0);
        return Some(ty.ty);
    }

    // Otherwise the allocation is a reference within another kind of data.
    // Decompose this outer data type to determine the reference type
    let layout = ty
        .layout()
        .map(|l| l.shape())
        .expect("Unable to get layout for {ty_kind:?}");
    let ref_ty = match ty_kind
        .rigid()
        .expect("Non-rigid-ty allocation found! {ty_kind:?}")
    {
        // homogenous, so no choice. Could check alignment of the offset...
        RigidTy::Array(ty, _) | RigidTy::Slice(ty) => Some(*ty),
        // cases covered above
        RigidTy::Ref(_, _, _) | RigidTy::RawPtr(_, _) => {
            unreachable!("Covered by builtin_deref above")
        }
        RigidTy::Adt(def, _) if def.is_box() => {
            unreachable!("Covered by builtin_deref above")
        }
        // For other structs, consult layout to determine field type
        RigidTy::Adt(adt_def, args) if ty_kind.is_struct() => {
            let field_idx = field_for_offset(layout, *offset).unwrap();
            // NB struct, single variant
            let fields = adt_def.variants().pop().map(|v| v.fields()).unwrap();
            fields.get(field_idx).map(|f| f.ty_with_args(args))
        }
        RigidTy::Adt(_adt_def, _args) if ty_kind.is_enum() => {
            // we have to figure out which variant we are dealing with (requires the data)
            match field_for_offset(layout, *offset) {
                None =>
                // FIXME we'd have to figure out which variant we are dealing with (requires the data)
                {
                    None
                }
                Some(_idx) =>
                // FIXME we'd have to figure out where that shared field is in the source ordering
                {
                    None
                }
            }
        }
        RigidTy::Tuple(fields) => {
            let field_idx = field_for_offset(layout, *offset)?;
            fields.get(field_idx).copied()
        }
        RigidTy::FnPtr(_) => None,
        _unimplemented => {
            debug_log_println!(
                "get_prov_type: Unimplemented RigidTy allocation: {:?}",
                _unimplemented
            );
            None
        }
    };
    match ref_ty {
        None => None,
        Some(ty) => get_prov_ty(ty, &0),
    }
}

fn collect_alloc(
    val_collector: &mut BodyAnalyzer,
    ty: stable_mir::ty::Ty,
    offset: &usize,
    val: stable_mir::mir::alloc::AllocId,
) {
    if val_collector.visited_allocs.contains_key(&val) {
        return;
    }
    let kind = ty.kind();
    let global_alloc = GlobalAlloc::from(val);
    debug_log_println!(
        "DEBUG: called collect_alloc: {:?}:{:?}:{:?}",
        val,
        ty,
        offset
    );
    match global_alloc {
        GlobalAlloc::Memory(ref alloc) => {
            let pointed_ty = get_prov_ty(ty, offset);
            debug_log_println!(
                "DEBUG: adding alloc: {:?}:{:?}: {:?}",
                val,
                pointed_ty,
                global_alloc
            );
            if let Some(p_ty) = pointed_ty {
                val_collector
                    .visited_allocs
                    .insert(val, (p_ty, global_alloc.clone()));
                alloc
                    .provenance
                    .ptrs
                    .iter()
                    .for_each(|(prov_offset, prov)| {
                        collect_alloc(val_collector, p_ty, prov_offset, prov.0);
                    });
            } else {
                val_collector.visited_allocs.insert(
                    val,
                    (stable_mir::ty::Ty::to_val(0), global_alloc.clone()),
                );
            }
        }
        GlobalAlloc::Static(_) => {
            assert!(
                kind.clone().builtin_deref(true).is_some(),
                "Allocated pointer is not a built-in pointer type: {:?}",
                kind
            );
            val_collector
                .visited_allocs
                .insert(val, (ty, global_alloc.clone()));
        }
        GlobalAlloc::VTable(_, _) => {
            assert!(
                kind.clone().builtin_deref(true).is_some(),
                "Allocated pointer is not a built-in pointer type: {:?}",
                kind
            );
            val_collector
                .visited_allocs
                .insert(val, (ty, global_alloc.clone()));
        }
        GlobalAlloc::Function(_) => {
            if !kind.is_fn_ptr() {
                let prov_ty = get_prov_ty(ty, offset);
                debug_log_println!(
                    "DEBUG: GlobalAlloc::Function with non-fn-ptr type; alloc_id={:?}, ty={:?}, offset={}, kind={:?}, recovered_prov_ty={:?}",
                    val,
                    ty,
                    offset,
                    kind,
                    prov_ty
                );
                if let Some(p_ty) = prov_ty {
                    val_collector
                        .visited_allocs
                        .insert(val, (p_ty, global_alloc.clone()));
                } else {
                    // Could not recover a precise pointee type; use an opaque 0-valued Ty
                    // as a conservative placeholder.
                    val_collector.visited_allocs.insert(
                        val,
                        (stable_mir::ty::Ty::to_val(0), global_alloc.clone()),
                    );
                }
            } else {
                val_collector
                    .visited_allocs
                    .insert(val, (ty, global_alloc.clone()));
            }
        }
    };
}

impl MirVisitor for BodyAnalyzer<'_, '_> {
    fn visit_span(&mut self, span: &stable_mir::ty::Span) {
        let span_internal = internal(self.tcx, span);
        let (source_file, lo_line, lo_col, hi_line, hi_col) = self
            .tcx
            .sess
            .source_map()
            .span_to_location_info(span_internal);
        let file_name = match source_file {
            Some(sf) => sf
                .name
                .display(rustc_span::FileNameDisplayPreference::Remapped)
                .to_string(),
            None => "no-location".to_string(),
        };
        self.spans.insert(
            span.to_index(),
            (file_name, lo_line, lo_col, hi_line, hi_col),
        );
    }

    fn visit_terminator(&mut self, term: &Terminator, loc: stable_mir::mir::visit::Location) {
        use stable_mir::mir::{ConstOperand, Operand::Constant};
        use TerminatorKind::*;
        let fn_sym = match &term.kind {
            Call {
                func: Constant(ConstOperand { const_: cnst, .. }),
                args: _,
                ..
            } => {
                if *cnst.kind() != stable_mir::ty::ConstantKind::ZeroSized {
                    return;
                }
                let inst = fn_inst_for_ty(cnst.ty(), true)
                    .expect("Direct calls to functions must resolve to an instance");
                fn_inst_sym(self.tcx, Some(cnst.ty()), Some(&inst))
            }
            Drop { place, .. } => {
                let drop_ty = place.ty(self.locals).unwrap();
                let inst = Instance::resolve_drop_in_place(drop_ty);
                fn_inst_sym(self.tcx, None, Some(&inst))
            }
            _ => None,
        };
        update_link_map(self.link_map, fn_sym, ItemSource(TERM));
        self.super_terminator(term, loc);
    }

    fn visit_rvalue(&mut self, rval: &Rvalue, loc: stable_mir::mir::visit::Location) {
        use stable_mir::mir::{CastKind, PointerCoercion};

        #[allow(clippy::single_match)] // TODO: Unsure if we need to fill these out
        match rval {
            Rvalue::Cast(CastKind::PointerCoercion(PointerCoercion::ReifyFnPointer), ref op, _) => {
                let inst = fn_inst_for_ty(op.ty(self.locals).unwrap(), false)
                    .expect("ReifyFnPointer Cast operand type does not resolve to an instance");
                let fn_sym = fn_inst_sym(self.tcx, None, Some(&inst));
                update_link_map(self.link_map, fn_sym, ItemSource(FPTR));
            }
            _ => {}
        };
        self.super_rvalue(rval, loc);
    }

    fn visit_mir_const(
        &mut self,
        constant: &stable_mir::ty::MirConst,
        loc: stable_mir::mir::visit::Location,
    ) {
        use stable_mir::ty::{ConstantKind, TyConstKind}; // TyConst
        match constant.kind() {
            ConstantKind::Allocated(alloc) => {
                debug_log_println!(
                    "visited_mir_const::Allocated({:?}) as {:?}",
                    alloc,
                    constant.ty().kind()
                );
                alloc
                    .provenance
                    .ptrs
                    .iter()
                    .for_each(|(offset, prov)| collect_alloc(self, constant.ty(), offset, prov.0));
            }
            ConstantKind::Ty(ty_const) => {
                if let TyConstKind::Value(..) = ty_const.kind() {
                    panic!("TyConstKind::Value");
                }
            }
            ConstantKind::ZeroSized => {
                // Zero-sized constants can represent function items (FnDef) used as values,
                // e.g. when passing a function pointer to a higher-order function.
                // Ensure such functions are included in the link map so they appear in the
                // `functions` array of the SMIR JSON.
                if constant.ty().kind().fn_def().is_some() {
                    if let Some(inst) = fn_inst_for_ty(constant.ty(), false)
                        .or_else(|| fn_inst_for_ty(constant.ty(), true))
                    {
                        let fn_sym = fn_inst_sym(self.tcx, Some(constant.ty()), Some(&inst));
                        if let Some((ty, kind, name)) = fn_sym {
                            update_link_map(
                                self.link_map,
                                Some((ty, kind, name)),
                                ItemSource(FPTR),
                            );
                        }
                    }
                }
            }
            ConstantKind::Unevaluated(uconst) => {
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
                self.new_unevaluated.push(UnevalConstInfo {
                    const_def: uconst.def,
                    item_name,
                    mono_item: rustc_internal::stable(internal_mono_item),
                });
            }
            ConstantKind::Param(_) => {}
        }
        self.super_mir_const(constant, loc);
    }

    fn visit_ty(&mut self, ty: &stable_mir::ty::Ty, _location: stable_mir::mir::visit::Location) {
        ty.visit(self.ty_visitor);
        self.super_ty(ty);
    }
}

/// Result of collecting all mono items and analyzing their bodies in a single pass.
/// After this phase completes, no more inst.body() calls should be needed.
struct CollectedCrate {
    items: Vec<Item>,
    original_item_names: HashSet<String>,
    unevaluated_consts: HashMap<stable_mir::ty::ConstDef, String>,
}

struct DerivedInfo<'tcx> {
    calls: LinkMap<'tcx>,
    allocs: AllocMap,
    types: TyMap,
    spans: SpanMap,
}

/// Enqueue newly discovered unevaluated-const items into the fixpoint work queue.
/// Each new item calls mk_item (which calls inst.body() exactly once).
fn enqueue_unevaluated_consts(
    tcx: TyCtxt<'_>,
    discovered: Vec<UnevalConstInfo>,
    known_names: &mut HashSet<String>,
    pending: &mut HashMap<String, Item>,
    unevaluated_consts: &mut HashMap<stable_mir::ty::ConstDef, String>,
) {
    for info in discovered {
        if known_names.contains(&info.item_name) || pending.contains_key(&info.item_name) {
            continue;
        }
        debug_log_println!("Adding unevaluated const body for: {}", info.item_name);
        unevaluated_consts.insert(info.const_def, info.item_name.clone());
        let new_item = mk_item(tcx, info.mono_item, info.item_name.clone());
        pending.insert(info.item_name.clone(), new_item);
        known_names.insert(info.item_name);
    }
}

/// Register a function item in the link map (when LINK_ITEMS is enabled).
fn maybe_add_to_link_map<'tcx>(tcx: TyCtxt<'tcx>, item: &Item, link_map: &mut LinkMap<'tcx>) {
    if !link_items_enabled() {
        return;
    }
    if let MonoItem::Fn(inst) = &item.mono_item {
        update_link_map(
            link_map,
            fn_inst_sym(tcx, None, Some(inst)),
            ItemSource(ITEM),
        );
    }
}

/// Collect all mono items and analyze their bodies in a single pass per body.
///
/// Each body is walked exactly once. The fixpoint loop handles transitive
/// discovery of items through unevaluated constants: when a body references an
/// unknown unevaluated const, a new Item is created (calling inst.body() once)
/// and added to the work queue.
fn collect_and_analyze_items<'tcx>(
    tcx: TyCtxt<'tcx>,
    initial_items: HashMap<String, Item>,
) -> (CollectedCrate, DerivedInfo<'tcx>) {
    let original_item_names: HashSet<String> = initial_items.keys().cloned().collect();

    let mut calls_map: LinkMap<'tcx> = HashMap::new();
    let mut visited_allocs = AllocMap::new();
    let mut ty_visitor = TyCollector::new(tcx);
    let mut span_map: SpanMap = HashMap::new();
    let mut unevaluated_consts: HashMap<stable_mir::ty::ConstDef, String> = HashMap::new();

    let mut pending: HashMap<String, Item> = initial_items;
    let mut known_names: HashSet<String> = original_item_names.clone();
    let mut all_items: Vec<Item> = Vec::new();

    while let Some((_name, item)) = take_any(&mut pending) {
        maybe_add_to_link_map(tcx, &item, &mut calls_map);

        let Some((body, locals)) = item.body_and_locals() else {
            item.warn_missing_body();
            all_items.push(item);
            continue;
        };

        let mut new_unevaluated = Vec::new();
        BodyAnalyzer {
            tcx,
            locals,
            link_map: &mut calls_map,
            visited_allocs: &mut visited_allocs,
            ty_visitor: &mut ty_visitor,
            spans: &mut span_map,
            new_unevaluated: &mut new_unevaluated,
        }
        .visit_body(body);

        enqueue_unevaluated_consts(
            tcx,
            new_unevaluated,
            &mut known_names,
            &mut pending,
            &mut unevaluated_consts,
        );

        all_items.push(item);
    }

    (
        CollectedCrate {
            items: all_items,
            original_item_names,
            unevaluated_consts,
        },
        DerivedInfo {
            calls: calls_map,
            allocs: visited_allocs,
            types: ty_visitor.types,
            spans: span_map,
        },
    )
}

// Core item collection logic
// ==========================

fn mono_collect(tcx: TyCtxt<'_>) -> Vec<MonoItem> {
    let units = tcx.collect_and_partition_mono_items(()).1;
    units
        .iter()
        .flat_map(|unit| {
            unit.items_in_deterministic_order(tcx)
                .iter()
                .map(|(internal_item, _)| rustc_internal::stable(internal_item))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn collect_items(tcx: TyCtxt<'_>) -> HashMap<String, Item> {
    // get initial set of mono_items
    let items = mono_collect(tcx);
    items
        .iter()
        .map(|item| {
            let name = mono_item_name(tcx, item);
            (name.clone(), mk_item(tcx, item.clone(), name))
        })
        .collect::<HashMap<_, _>>()
}

// Type metadata required for execution

#[derive(Serialize)]
pub enum TypeMetadata {
    PrimitiveType(RigidTy),
    EnumType {
        name: String,
        adt_def: AdtDef,
        discriminants: Vec<u128>,
        fields: Vec<Vec<stable_mir::ty::Ty>>,
        layout: Option<LayoutShape>,
    },
    StructType {
        name: String,
        adt_def: AdtDef,
        fields: Vec<stable_mir::ty::Ty>,
        layout: Option<LayoutShape>,
    },
    UnionType {
        name: String,
        adt_def: AdtDef,
        fields: Vec<stable_mir::ty::Ty>,
        layout: Option<LayoutShape>,
    },
    ArrayType {
        elem_type: stable_mir::ty::Ty,
        size: Option<stable_mir::ty::TyConst>,
        layout: Option<LayoutShape>,
    },
    PtrType {
        pointee_type: stable_mir::ty::Ty,
        layout: Option<LayoutShape>,
    },
    RefType {
        pointee_type: stable_mir::ty::Ty,
        layout: Option<LayoutShape>,
    },
    TupleType {
        types: Vec<stable_mir::ty::Ty>,
        layout: Option<LayoutShape>,
    },
    FunType(String),
    VoidType,
}

fn mk_type_metadata(
    tcx: TyCtxt<'_>,
    k: stable_mir::ty::Ty,
    t: TyKind,
    layout: Option<LayoutShape>,
) -> Option<(stable_mir::ty::Ty, TypeMetadata)> {
    use stable_mir::ty::RigidTy::*;
    use TyKind::RigidTy as T;
    use TypeMetadata::*;
    let name = format!("{k}"); // prints name with type parameters
    match t {
        T(prim_type) if t.is_primitive() => Some((k, PrimitiveType(prim_type))),
        // for enums, we need a mapping of variantIdx to discriminant
        // this requires access to the internals and is not provided as an interface function at the moment
        T(Adt(adt_def, args)) if t.is_enum() => {
            let adt_internal = rustc_internal::internal(tcx, adt_def);
            let discriminants = adt_internal
                .discriminants(tcx)
                .map(|(_, discr)| discr.val)
                .collect::<Vec<_>>();
            let fields = adt_def
                .variants()
                .iter()
                .map(|v| {
                    v.fields()
                        .iter()
                        .map(|f| f.ty_with_args(&args))
                        .collect::<Vec<stable_mir::ty::Ty>>()
                })
                .collect();
            Some((
                k,
                EnumType {
                    name,
                    adt_def,
                    discriminants,
                    fields,
                    layout,
                },
            ))
        }
        T(Adt(adt_def, args)) if t.is_struct() => {
            let fields = adt_def
                .variants()
                .pop() // NB struct, there should be a single variant
                .unwrap()
                .fields()
                .iter()
                .map(|f| f.ty_with_args(&args))
                .collect();
            Some((
                k,
                StructType {
                    name,
                    adt_def,
                    fields,
                    layout,
                },
            ))
        }
        T(Adt(adt_def, args)) if t.is_union() => {
            let fields = adt_def
                .variants()
                .pop() // TODO: Check union has single variant
                .unwrap()
                .fields()
                .iter()
                .map(|f| f.ty_with_args(&args))
                .collect();
            Some((
                k,
                UnionType {
                    name,
                    adt_def,
                    fields,
                    layout,
                },
            ))
        }
        // encode str together with primitive types
        T(Str) => Some((k, PrimitiveType(Str))),
        // for arrays and slices, record element type and optional size
        T(Array(elem_type, ty_const)) => {
            if matches!(
                ty_const.kind(),
                stable_mir::ty::TyConstKind::Unevaluated(_, _)
            ) {
                panic!("Unevaluated constant {ty_const:?} in type {k}");
            }
            Some((
                k,
                ArrayType {
                    elem_type,
                    size: Some(ty_const),
                    layout,
                },
            ))
        }

        T(Slice(elem_type)) => Some((
            k,
            ArrayType {
                elem_type,
                size: None,
                layout,
            },
        )),
        // for raw pointers and references store the pointee type
        T(RawPtr(pointee_type, _)) => Some((
            k,
            PtrType {
                pointee_type,
                layout,
            },
        )),
        T(Ref(_, pointee_type, _)) => Some((
            k,
            RefType {
                pointee_type,
                layout,
            },
        )),
        // for tuples the element types are provided
        T(Tuple(types)) => Some((k, TupleType { types, layout })),
        // opaque function types (fun ptrs, closures, FnDef) are only provided to avoid dangling ty references
        T(FnDef(_, _)) | T(FnPtr(_)) | T(Closure(_, _)) => Some((k, FunType(name))),
        // other types are not provided either
        T(Foreign(_))
        | T(Pat(_, _))
        | T(Coroutine(_, _, _))
        | T(Dynamic(_, _, _))
        | T(CoroutineWitness(_, _)) => {
            debug_log_println!(
                "\nDEBUG: Skipping unsupported ty {}: {:?}",
                k.to_index(),
                k.kind()
            );
            None
        }
        T(Never) => Some((k, VoidType)),
        TyKind::Alias(_, _) | TyKind::Param(_) | TyKind::Bound(_, _) => {
            debug_log_println!("\nSkipping undesired ty {}: {:?}", k.to_index(), k.kind());
            None
        }
        _ => {
            // redundant because of first 4 cases, but rustc does not understand that
            debug_log_println!("\nDEBUG: Funny other Ty {}: {:?}", k.to_index(), k.kind());
            None
        }
    }
}

type SourceData = (String, usize, usize, usize, usize);

/// the serialised data structure as a whole
#[derive(Serialize)]
pub struct SmirJson<'t> {
    pub name: String,
    pub crate_id: u64,
    pub allocs: Vec<AllocInfo>,
    pub functions: Vec<(LinkMapKey<'t>, FnSymType)>,
    pub uneval_consts: Vec<(ConstDef, String)>,
    pub items: Vec<Item>,
    pub types: Vec<(stable_mir::ty::Ty, TypeMetadata)>,
    pub spans: Vec<(usize, SourceData)>,
    pub debug: Option<SmirJsonDebugInfo<'t>>,
    pub machine: stable_mir::target::MachineInfo,
}

#[derive(Serialize)]
pub struct SmirJsonDebugInfo<'t> {
    fn_sources: Vec<(LinkMapKey<'t>, ItemSource)>,
    types: TyMap,
    foreign_modules: Vec<(String, Vec<ForeignModule>)>,
}

#[derive(Serialize)]
pub struct AllocInfo {
    alloc_id: AllocId,
    ty: stable_mir::ty::Ty,
    global_alloc: GlobalAlloc,
}

impl AllocInfo {
    pub fn alloc_id(&self) -> AllocId {
        self.alloc_id
    }

    pub fn ty(&self) -> stable_mir::ty::Ty {
        self.ty
    }

    pub fn global_alloc(&self) -> &GlobalAlloc {
        &self.global_alloc
    }
}

// Serialization Entrypoint
// ========================

/// Phase 3: Assemble the final SmirJson from collected and derived data.
/// This is a pure data transformation with no inst.body() calls.
fn assemble_smir<'tcx>(
    tcx: TyCtxt<'tcx>,
    collected: CollectedCrate,
    derived: DerivedInfo<'tcx>,
) -> SmirJson<'tcx> {
    let local_crate = stable_mir::local_crate();
    let CollectedCrate {
        mut items,
        original_item_names,
        unevaluated_consts,
    } = collected;
    let DerivedInfo {
        calls,
        allocs: visited_allocs,
        types: visited_tys,
        spans: span_map,
    } = derived;

    // Verify alloc coherence: no duplicate AllocIds, and every AllocId
    // referenced in a stored body was actually collected.
    #[cfg(debug_assertions)]
    visited_allocs.verify_coherence(&items);

    // FIXME: We dump extra static items here --- this should be handled better.
    // Statics discovered through allocation provenance that weren't in the
    // original mono item collection.
    for (_, alloc) in visited_allocs.iter() {
        if let (_, GlobalAlloc::Static(def)) = alloc {
            let mono_item =
                stable_mir::mir::mono::MonoItem::Fn(stable_mir::mir::mono::Instance::from(*def));
            let item_name = &mono_item_name(tcx, &mono_item);
            if !original_item_names.contains(item_name) {
                println!(
                    "Items missing static with id {:?} and name {:?}",
                    def, item_name
                );
                items.push(mk_item(tcx, mono_item, item_name.clone()));
            }
        }
    }

    let debug: Option<SmirJsonDebugInfo> = if debug_enabled() {
        let fn_sources = calls
            .iter()
            .map(|(k, (source, _))| (k.clone(), source.clone()))
            .collect::<Vec<_>>();
        Some(SmirJsonDebugInfo {
            fn_sources,
            types: visited_tys.clone(),
            foreign_modules: get_foreign_module_details(),
        })
    } else {
        None
    };

    let mut functions = calls
        .into_iter()
        .map(|(k, (_, name))| (k, name))
        .collect::<Vec<_>>();
    let mut allocs = visited_allocs
        .into_entries()
        .map(|(alloc_id, (ty, global_alloc))| AllocInfo {
            alloc_id,
            ty,
            global_alloc,
        })
        .collect::<Vec<_>>();
    let crate_id = tcx.stable_crate_id(LOCAL_CRATE).as_u64();

    let mut types = visited_tys
        .into_iter()
        .filter_map(|(k, (t, l))| mk_type_metadata(tcx, k, t, l))
        .collect::<Vec<_>>();

    let mut spans = span_map.into_iter().collect::<Vec<_>>();

    // sort output vectors to stabilise output (a bit)
    allocs.sort_by(|a, b| a.alloc_id.to_index().cmp(&b.alloc_id.to_index()));
    functions.sort_by(|a, b| a.0 .0.to_index().cmp(&b.0 .0.to_index()));
    items.sort();
    types.sort_by(|a, b| a.0.to_index().cmp(&b.0.to_index()));
    spans.sort();

    SmirJson {
        name: local_crate.name,
        crate_id,
        allocs,
        functions,
        uneval_consts: unevaluated_consts.into_iter().collect(),
        items,
        types,
        spans,
        debug,
        machine: stable_mir::target::MachineInfo::target(),
    }
}

pub fn collect_smir(tcx: TyCtxt<'_>) -> SmirJson {
    // Phase 1+2: Collect all mono items from rustc and analyze their bodies
    // in a single pass. Each body is walked exactly once. Transitive item
    // discovery (unevaluated constants) is handled by a fixpoint loop.
    let initial_items = collect_items(tcx);
    let (collected, derived) = collect_and_analyze_items(tcx, initial_items);

    // Verify alloc coherence: no duplicate AllocIds, and every AllocId
    // referenced in a stored body was actually collected.
    #[cfg(debug_assertions)]
    derived.allocs.verify_coherence(&collected.items);

    // Phase 3: Assemble the final output (pure data transformation)
    assemble_smir(tcx, collected, derived)
}

pub fn emit_smir(tcx: TyCtxt<'_>) {
    let smir_json =
        serde_json::to_string(&collect_smir(tcx)).expect("serde_json failed to write result");

    match tcx.output_filenames(()).path(OutputType::Mir) {
        OutFileName::Stdout => {
            write!(&io::stdout(), "{}", smir_json).expect("Failed to write smir.json");
        }
        OutFileName::Real(path) => {
            let mut b = io::BufWriter::new(
                File::create(path.with_extension("smir.json"))
                    .expect("Failed to create {path}.smir.json output file"),
            );
            write!(b, "{}", smir_json).expect("Failed to write smir.json");
        }
    }
}
