//! Data model for the serialized SMIR JSON output.
//!
//! Contains the top-level [`SmirJson`] structure and all supporting types that
//! appear in the JSON, plus internal type aliases used across the printer submodules.

extern crate rustc_middle;
extern crate serde;
extern crate stable_mir;

use std::collections::HashMap;

use rustc_middle as middle;
use serde::{Serialize, Serializer};
use stable_mir::abi::LayoutShape;
use stable_mir::mir::alloc::{AllocId, GlobalAlloc};
use stable_mir::mir::mono::MonoItem;
use stable_mir::mir::Body;
use stable_mir::ty::{AdtDef, Allocation, ConstDef, ForeignItemKind, RigidTy};

// Type aliases
pub(super) type LinkMap<'tcx> = HashMap<LinkMapKey<'tcx>, (ItemSource, FnSymType)>;
pub(super) type AllocMap =
    HashMap<stable_mir::mir::alloc::AllocId, (stable_mir::ty::Ty, GlobalAlloc)>;
pub(super) type TyMap =
    HashMap<stable_mir::ty::Ty, (stable_mir::ty::TyKind, Option<stable_mir::abi::LayoutShape>)>;
/// Map from span index to its source location data.
pub(super) type SpanMap = HashMap<usize, SourceData>;

/// Collected interned values from MIR body traversal.
///
/// Aggregates the four maps populated by [`super::mir_visitor::collect_interned_values`]:
/// function call targets, global allocations, reachable types, and source spans.
pub(super) struct InternedValues<'tcx> {
    /// Function call and fn-pointer resolution map.
    pub link_map: LinkMap<'tcx>,
    /// Global allocations reachable from constants.
    pub alloc_map: AllocMap,
    /// All types encountered during traversal, with optional layouts.
    pub ty_map: TyMap,
    /// Source location information for each MIR span.
    pub span_map: SpanMap,
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
/// - A no-op shim (empty body),
/// - A compiler intrinsic, or
/// - A normal function with a mangled symbol name.
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
pub struct LinkMapKey<'tcx>(
    pub stable_mir::ty::Ty,
    pub(super) Option<middle::ty::InstanceKind<'tcx>>,
);

impl Serialize for LinkMapKey<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeTuple;
        if super::link_instance_enabled() {
            let mut tup = serializer.serialize_tuple(2)?;
            tup.serialize_element(&self.0)?;
            tup.serialize_element(&format!("{:?}", self.1).as_str())?;
            tup.end()
        } else {
            <stable_mir::ty::Ty as Serialize>::serialize(&self.0, serializer)
        }
    }
}

/// The kind-specific payload of a collected monomorphized item.
///
/// Each variant carries the item's human-readable name, definition id,
/// and—depending on kind—either a MIR body, a static allocation, or
/// the textual global assembly.
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
    },
    MonoItemGlobalAsm {
        asm: String,
    },
}

// Item details (debug info)
#[derive(Serialize, Clone)]
pub(super) struct BodyDetails {
    pub pp: String,
}

#[derive(Serialize, Clone)]
pub(super) struct GenericData(pub Vec<(String, String)>);

#[derive(Serialize, Clone)]
pub(super) struct ItemDetails {
    pub fn_instance_kind: Option<stable_mir::mir::mono::InstanceKind>,
    pub fn_item_kind: Option<stable_mir::ItemKind>,
    pub fn_body_details: Option<BodyDetails>,
    pub internal_kind: String,
    pub path: String,
    pub internal_ty: String,
    pub generic_data: GenericData,
}

/// A single monomorphized item (function, static, or global asm) collected from the crate.
///
/// Items are sorted by `symbol_name` for deterministic output. The `mono_item`
/// field is skipped during serialization but retained for internal comparisons.
#[derive(Serialize, Clone)]
pub struct Item {
    #[serde(skip)]
    pub(super) mono_item: MonoItem,
    pub symbol_name: String,
    pub mono_item_kind: MonoItemKind,
    details: Option<ItemDetails>,
}

impl Item {
    pub(super) fn new(
        mono_item: MonoItem,
        symbol_name: String,
        mono_item_kind: MonoItemKind,
        details: Option<ItemDetails>,
    ) -> Self {
        Item {
            mono_item,
            symbol_name,
            mono_item_kind,
            details,
        }
    }

    pub(super) fn mono_item(&self) -> &MonoItem {
        &self.mono_item
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
                    MonoItemFn {
                        name,
                        id: _,
                        body: _,
                    } => name,
                    MonoItemStatic {
                        name,
                        id: _,
                        allocation: _,
                    } => name,
                    MonoItemGlobalAsm { asm } => asm,
                }
            )
        };
        sort_key(self).cmp(&sort_key(other))
    }
}

// Foreign items
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
    /// A scalar primitive (integer, float, bool, char).
    PrimitiveType(RigidTy),
    /// An enum with discriminant-to-variant mapping and per-variant field types.
    EnumType {
        name: String,
        adt_def: AdtDef,
        discriminants: Vec<u128>,
        fields: Vec<Vec<stable_mir::ty::Ty>>,
        layout: Option<LayoutShape>,
    },
    /// A struct with its field types in declaration order.
    StructType {
        name: String,
        adt_def: AdtDef,
        fields: Vec<stable_mir::ty::Ty>,
        layout: Option<LayoutShape>,
    },
    /// A union with its field types.
    UnionType {
        name: String,
        adt_def: AdtDef,
        fields: Vec<stable_mir::ty::Ty>,
        layout: Option<LayoutShape>,
    },
    /// An array or slice, with element type and optional compile-time size.
    ArrayType {
        elem_type: stable_mir::ty::Ty,
        size: Option<stable_mir::ty::TyConst>,
        layout: Option<LayoutShape>,
    },
    /// A raw pointer (`*const T` / `*mut T`).
    PtrType {
        pointee_type: stable_mir::ty::Ty,
        layout: Option<LayoutShape>,
    },
    /// A reference (`&T` / `&mut T`).
    RefType {
        pointee_type: stable_mir::ty::Ty,
        layout: Option<LayoutShape>,
    },
    /// A tuple with its element types.
    TupleType {
        types: Vec<stable_mir::ty::Ty>,
        layout: Option<LayoutShape>,
    },
    /// An opaque function type (FnDef, FnPtr, or Closure); carries the display name.
    FunType(String),
    /// The never type (`!`).
    VoidType,
}

/// Span location data: `(filename, start_line, start_col, end_line, end_col)`.
pub type SourceData = (String, usize, usize, usize, usize);

/// Top-level output structure serialized as the `*.smir.json` file.
///
/// Contains all information extracted from the crate's Stable MIR:
/// monomorphized items with bodies, the link-time function map, type metadata,
/// global allocations, source spans, and optionally debug information.
///
/// Fields are sorted for deterministic output across runs.
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

/// Extra debug information included when the `DEBUG` environment variable is set.
///
/// Contains the provenance of each function map entry (whether it came from
/// an item, a terminator call, or a function pointer cast), the raw type map,
/// and foreign module details.
#[derive(Serialize)]
pub struct SmirJsonDebugInfo<'t> {
    pub(super) fn_sources: Vec<(LinkMapKey<'t>, ItemSource)>,
    pub(super) types: TyMap,
    pub(super) foreign_modules: Vec<(String, Vec<ForeignModule>)>,
}
