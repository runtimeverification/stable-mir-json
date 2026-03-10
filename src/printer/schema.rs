//! Data model types for the `*.smir.json` output.
//!
//! Contains the top-level [`SmirJson`] structure and all supporting types:
//! [`Item`], [`AllocMap`], [`AllocInfo`], [`TypeMetadata`], [`LinkMapKey`],
//! [`FnSymType`], and serialization helpers.

use crate::compat::bridge::OpaqueInstanceKind;
use crate::compat::serde;
use crate::compat::stable_mir;

use std::collections::{HashMap, HashSet};

use super::items::MonoItemKind;
use serde::{Serialize, Serializer};
use stable_mir::abi::LayoutShape;
use stable_mir::mir::alloc::{AllocId, GlobalAlloc};
use stable_mir::mir::visit::MirVisitor;
use stable_mir::mir::Body;
use stable_mir::ty::{AdtDef, ConstDef, ForeignItemKind, RigidTy};

// Type aliases
pub(super) type LinkMap = HashMap<LinkMapKey, (ItemSource, FnSymType)>;
pub(super) type TyMap =
    HashMap<stable_mir::ty::Ty, (stable_mir::ty::TyKind, Option<stable_mir::abi::LayoutShape>)>;
pub(super) type SpanMap = HashMap<usize, SourceData>;

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
pub(super) struct AllocMap {
    inner: HashMap<stable_mir::mir::alloc::AllocId, (stable_mir::ty::Ty, GlobalAlloc)>,
    #[cfg(debug_assertions)]
    insert_count: usize,
    #[cfg(debug_assertions)]
    duplicate_ids: Vec<stable_mir::mir::alloc::AllocId>,
}

impl AllocMap {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
            #[cfg(debug_assertions)]
            insert_count: 0,
            #[cfg(debug_assertions)]
            duplicate_ids: Vec::new(),
        }
    }

    pub fn contains_key(&self, key: &stable_mir::mir::alloc::AllocId) -> bool {
        self.inner.contains_key(key)
    }

    pub fn insert(
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

    pub fn into_entries(
        self,
    ) -> impl Iterator<
        Item = (
            stable_mir::mir::alloc::AllocId,
            (stable_mir::ty::Ty, GlobalAlloc),
        ),
    > {
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
    pub fn verify_coherence(&self, items: &[Item]) {
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
                AllocIdCollector { ids: &mut body_ids }.visit_body(body);
            }
        }

        let map_ids: HashSet<_> = self.inner.keys().copied().collect();
        let missing_from_map: Vec<_> = body_ids.difference(&map_ids).collect();

        assert!(
            missing_from_map.is_empty(),
            "Alloc-id coherence violation: AllocIds {missing_from_map:?} are referenced in \
             stored Item bodies but missing from the alloc map. This means \
             the analysis phase collected allocations from a different body \
             than what is stored in the Items."
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

// Item source constants and type
pub(super) const ITEM: u8 = 1 << 0;
pub(super) const TERM: u8 = 1 << 1;
pub(super) const FPTR: u8 = 1 << 2;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct ItemSource(pub u8);

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

/// Classification of a function symbol's resolution.
///
/// Each function encountered during MIR traversal is categorized as one of:
/// a no-op shim (empty body), a compiler intrinsic, or a normal function
/// with a mangled symbol name.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum FnSymType {
    /// An empty shim (no-op); the string is unused.
    NoOpSym(String),
    /// A compiler intrinsic; carries the intrinsic name.
    IntrinsicSym(String),
    /// A regular function; carries the mangled symbol name.
    NormalSym(String),
}

/// Key into the link-time function resolution map.
///
/// Pairs a Stable MIR type (always an `FnDef`) with an optional internal
/// `InstanceKind` for disambiguation. When serialized, the representation
/// depends on the `LINK_INST` environment variable: with it set, both
/// components are emitted as a 2-tuple; without it, only the type index
/// is written.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct LinkMapKey(
    pub stable_mir::ty::Ty,
    pub(super) Option<OpaqueInstanceKind>,
);

impl Serialize for LinkMapKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeTuple;
        if super::link_instance_enabled() {
            let mut tup = serializer.serialize_tuple(2)?;
            tup.serialize_element(&self.0)?;
            tup.serialize_element(&format!("{:?}", self.1))?;
            tup.end()
        } else {
            <stable_mir::ty::Ty as Serialize>::serialize(&self.0, serializer)
        }
    }
}

// Item details (debug info)
#[derive(Serialize, Clone)]
pub(super) struct BodyDetails {
    pp: String,
}

impl BodyDetails {
    pub fn new(pp: String) -> Self {
        BodyDetails { pp }
    }
}

#[derive(Serialize, Clone)]
pub(super) struct GenericData(pub Vec<(String, String)>);

#[derive(Serialize, Clone)]
pub(super) struct ItemDetails {
    // these fields only defined for fn items
    pub fn_instance_kind: Option<stable_mir::mir::mono::InstanceKind>,
    pub fn_item_kind: Option<stable_mir::ItemKind>,
    pub fn_body_details: Option<BodyDetails>,
    // these fields defined for all items
    pub internal_kind: String,
    pub path: String,
    pub internal_ty: String,
    pub generic_data: GenericData,
}

#[derive(Serialize)]
pub(super) struct ForeignItem {
    pub name: String,
    pub kind: ForeignItemKind,
}

#[derive(Serialize)]
pub(super) struct ForeignModule {
    pub name: String,
    pub items: Vec<ForeignItem>,
}

/// A single monomorphized item (function, static, or global asm) collected from the crate.
///
/// Deliberately does not carry a `MonoItem`: by the time an `Item` reaches
/// phase 3 (`assemble_smir`), no handle to re-enter rustc should exist.
/// The `MonoItem` lives alongside the `Item` in the phase 1+2 work queue
/// and is dropped before assembly begins.
#[derive(Serialize, Clone)]
pub struct Item {
    pub symbol_name: String,
    pub mono_item_kind: MonoItemKind,
    details: Option<ItemDetails>,
}

impl Item {
    pub(super) fn new(
        symbol_name: String,
        mono_item_kind: MonoItemKind,
        details: Option<ItemDetails>,
    ) -> Self {
        Item {
            symbol_name,
            mono_item_kind,
            details,
        }
    }

    /// Returns the pre-collected body and appropriate locals slice, if available.
    /// For functions, locals come from the body; for statics, locals are empty.
    pub(super) fn body_and_locals(&self) -> Option<(&Body, &[stable_mir::mir::LocalDecl])> {
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
}

impl PartialEq for Item {
    fn eq(&self, other: &Item) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
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

/// A recorded global allocation encountered during MIR traversal.
///
/// Captures the allocation id, the pointee type (as best as can be determined
/// from provenance analysis), and the underlying [`GlobalAlloc`] data
/// (memory contents, static reference, vtable, or function pointer).
#[derive(Serialize)]
pub struct AllocInfo {
    alloc_id: AllocId,
    ty: stable_mir::ty::Ty,
    global_alloc: GlobalAlloc,
}

impl AllocInfo {
    pub(super) fn new(
        alloc_id: AllocId,
        ty: stable_mir::ty::Ty,
        global_alloc: GlobalAlloc,
    ) -> Self {
        AllocInfo {
            alloc_id,
            ty,
            global_alloc,
        }
    }

    /// The unique allocation identifier within the crate.
    pub fn alloc_id(&self) -> AllocId {
        self.alloc_id
    }

    /// The pointee type of this allocation, as determined by provenance analysis.
    pub fn ty(&self) -> stable_mir::ty::Ty {
        self.ty
    }

    /// The underlying global allocation data (memory, static, vtable, or function).
    pub fn global_alloc(&self) -> &GlobalAlloc {
        &self.global_alloc
    }
}

/// Structured metadata about a Rust type, suitable for execution or verification.
///
/// Each variant captures the information a consumer needs to interpret values
/// of that type: field types and layouts for aggregates, element types for
/// arrays/slices, pointee types for pointers/references, and discriminant
/// mappings for enums.
#[derive(Serialize)]
pub enum TypeMetadata {
    PrimitiveType(RigidTy),
    EnumType {
        name: String,
        // adt_def serializes as a non-deterministic interned index (DefId), but
        // downstream consumers need it to cross-reference AggregateKind::Adt in
        // MIR bodies with the type metadata here. We can't stabilize it without
        // also controlling AggregateKind serialization (which comes from stable_mir).
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
        mutability: stable_mir::mir::Mutability,
    },
    RefType {
        pointee_type: stable_mir::ty::Ty,
        layout: Option<LayoutShape>,
        mutability: stable_mir::mir::Mutability,
    },
    TupleType {
        types: Vec<stable_mir::ty::Ty>,
        layout: Option<LayoutShape>,
    },
    DynType {
        name: String,
        layout: Option<LayoutShape>,
    },
    FunType(String),
    VoidType,
}

/// Span location data: `(filename, start_line, start_col, end_line, end_col)`.
pub type SourceData = crate::compat::spans::SourceData;

/// Top-level output structure serialized as the `*.smir.json` file.
///
/// Contains all information extracted from the crate's Stable MIR:
/// monomorphized items with bodies, the link-time function map, type metadata,
/// global allocations, source spans, and optionally debug information.
///
/// Collection fields (`allocs`, `functions`, `items`, `types`, `spans`) are
/// sorted where applicable to improve output determinism across runs.
#[derive(Serialize)]
pub struct SmirJson {
    pub name: String,
    pub crate_id: u64,
    pub allocs: Vec<AllocInfo>,
    pub functions: Vec<(LinkMapKey, FnSymType)>,
    pub uneval_consts: Vec<(ConstDef, String)>,
    pub items: Vec<Item>,
    pub types: Vec<(stable_mir::ty::Ty, TypeMetadata)>,
    pub spans: Vec<(usize, SourceData)>,
    pub debug: Option<SmirJsonDebugInfo>,
    pub machine: stable_mir::target::MachineInfo,
}

/// Extra debug information included when the `DEBUG` environment variable is set.
///
/// Contains the provenance of each function map entry (whether it came from
/// an item, a terminator call, or a function pointer cast), the raw type map,
/// and foreign module details.
#[derive(Serialize)]
pub struct SmirJsonDebugInfo {
    pub(super) fn_sources: Vec<(LinkMapKey, ItemSource)>,
    pub(super) types: TyMap,
    pub(super) foreign_modules: Vec<(String, Vec<ForeignModule>)>,
}

/// Result of collecting all mono items and analyzing their bodies in a single pass.
///
/// Contains only [`Item`] values (no `MonoItem`), so code that operates on a
/// `CollectedCrate` structurally cannot call `inst.body()` or re-enter rustc.
pub(super) struct CollectedCrate {
    pub items: Vec<Item>,
    pub unevaluated_consts: HashMap<stable_mir::ty::ConstDef, String>,
}

pub(super) struct DerivedInfo {
    pub calls: LinkMap,
    pub allocs: AllocMap,
    pub types: TyMap,
    pub spans: SpanMap,
}
