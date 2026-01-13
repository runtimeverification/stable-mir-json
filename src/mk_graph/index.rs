//! Index structures for looking up allocations and types.

use std::collections::HashMap;

extern crate stable_mir;
use stable_mir::abi::{FieldsShape, LayoutShape};
use stable_mir::mir::alloc::GlobalAlloc;
use stable_mir::ty::{IndexedVal, Ty};
use stable_mir::CrateDef;

use crate::printer::{AllocInfo, TypeMetadata};

// =============================================================================
// Index Structures
// =============================================================================

/// Index for looking up allocation information by AllocId
pub struct AllocIndex {
    pub by_id: HashMap<u64, AllocEntry>,
}

/// Processed allocation entry with human-readable description
pub struct AllocEntry {
    pub alloc_id: u64,
    pub ty: Ty,
    pub kind: AllocKind,
    pub description: String,
}

/// Simplified allocation kind for display
pub enum AllocKind {
    Memory { bytes_len: usize, is_str: bool },
    Static { name: String },
    VTable { ty_desc: String },
    Function { name: String },
}

/// Index for looking up type information
pub struct TypeIndex {
    by_id: HashMap<u64, TypeEntry>,
}

/// Detailed type information for rendering
pub struct TypeEntry {
    pub name: String,
    pub kind: TypeKind,
    pub layout: Option<LayoutInfo>,
}

/// Simplified type kind for display
#[derive(Clone)]
pub enum TypeKind {
    Primitive,
    Struct { fields: Vec<FieldInfo> },
    Enum { variants: Vec<VariantInfo> },
    Union { fields: Vec<FieldInfo> },
    Array { elem_ty: Ty, len: Option<u64> },
    Tuple { fields: Vec<Ty> },
    Ptr { pointee: Ty },
    Ref { pointee: Ty },
    Function,
    Void,
}

/// Field information for structs/unions
#[derive(Clone)]
pub struct FieldInfo {
    pub ty: Ty,
    pub offset: Option<usize>,
}

/// Variant information for enums
#[derive(Clone)]
pub struct VariantInfo {
    pub discriminant: u128,
    pub fields: Vec<FieldInfo>,
}

/// Layout information extracted from LayoutShape
#[derive(Clone)]
pub struct LayoutInfo {
    pub size: usize,
    pub align: usize,
    pub field_offsets: Vec<usize>,
}

// =============================================================================
// AllocIndex Implementation
// =============================================================================

impl Default for AllocIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl AllocIndex {
    pub fn new() -> Self {
        Self {
            by_id: HashMap::new(),
        }
    }

    pub fn from_alloc_infos(allocs: &[AllocInfo], type_index: &TypeIndex) -> Self {
        let mut index = Self::new();
        for info in allocs {
            let entry = AllocEntry::from_alloc_info(info, type_index);
            index.by_id.insert(entry.alloc_id, entry);
        }
        index
    }

    pub fn get(&self, id: u64) -> Option<&AllocEntry> {
        self.by_id.get(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &AllocEntry> {
        self.by_id.values()
    }

    /// Describe an alloc by its ID for use in labels
    pub fn describe(&self, id: u64) -> String {
        match self.get(id) {
            Some(entry) => entry.short_description(),
            None => format!("alloc{}", id),
        }
    }
}

// =============================================================================
// AllocEntry Implementation
// =============================================================================

impl AllocEntry {
    pub fn from_alloc_info(info: &AllocInfo, type_index: &TypeIndex) -> Self {
        let alloc_id = info.alloc_id().to_index() as u64;
        let ty = info.ty();
        let ty_name = type_index.get_name(ty);

        let (kind, description) = match info.global_alloc() {
            GlobalAlloc::Memory(alloc) => {
                let bytes = &alloc.bytes;
                let is_str = ty_name.contains("str");

                // Convert Option<u8> bytes to actual bytes for display
                let concrete_bytes: Vec<u8> = bytes.iter().filter_map(|&b| b).collect();

                let desc = if is_str && concrete_bytes.iter().all(|b| b.is_ascii()) {
                    let s: String = concrete_bytes
                        .iter()
                        .take(20)
                        .map(|&b| b as char)
                        .collect::<String>()
                        .escape_default()
                        .to_string();
                    if concrete_bytes.len() > 20 {
                        format!("\"{}...\" ({} bytes)", s, concrete_bytes.len())
                    } else {
                        format!("\"{}\"", s)
                    }
                } else if concrete_bytes.len() <= 8 && !concrete_bytes.is_empty() {
                    format!(
                        "{} = {}",
                        ty_name,
                        super::util::bytes_to_u64_le(&concrete_bytes)
                    )
                } else {
                    format!("{} ({} bytes)", ty_name, bytes.len())
                };

                (
                    AllocKind::Memory {
                        bytes_len: bytes.len(),
                        is_str,
                    },
                    desc,
                )
            }
            GlobalAlloc::Static(def) => {
                let name = def.name();
                (
                    AllocKind::Static { name: name.clone() },
                    format!("static {}", name),
                )
            }
            GlobalAlloc::VTable(vty, _trait_ref) => {
                let desc = format!("{}", vty);
                (
                    AllocKind::VTable {
                        ty_desc: desc.clone(),
                    },
                    format!("vtable<{}>", desc),
                )
            }
            GlobalAlloc::Function(instance) => {
                let name = instance.name();
                (
                    AllocKind::Function { name: name.clone() },
                    format!("fn {}", name),
                )
            }
        };

        Self {
            alloc_id,
            ty,
            kind,
            description,
        }
    }

    pub fn short_description(&self) -> String {
        format!("alloc{}: {}", self.alloc_id, self.description)
    }
}

// =============================================================================
// TypeIndex Implementation
// =============================================================================

impl Default for TypeIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeIndex {
    pub fn new() -> Self {
        Self {
            by_id: HashMap::new(),
        }
    }

    pub fn from_types(types: &[(Ty, TypeMetadata)]) -> Self {
        let mut index = Self::new();
        for (ty, metadata) in types {
            let entry = TypeEntry::from_metadata(metadata, *ty);
            index.by_id.insert(ty.to_index() as u64, entry);
        }
        index
    }

    pub fn get(&self, ty: Ty) -> Option<&TypeEntry> {
        self.by_id.get(&(ty.to_index() as u64))
    }

    pub fn get_name(&self, ty: Ty) -> String {
        self.by_id
            .get(&(ty.to_index() as u64))
            .map(|e| e.name.clone())
            .unwrap_or_else(|| format!("{}", ty))
    }

    pub fn get_layout(&self, ty: Ty) -> Option<&LayoutInfo> {
        self.by_id
            .get(&(ty.to_index() as u64))
            .and_then(|e| e.layout.as_ref())
    }

    /// Iterate over all type entries
    pub fn iter(&self) -> impl Iterator<Item = (u64, &TypeEntry)> {
        self.by_id.iter().map(|(&id, entry)| (id, entry))
    }
}

// =============================================================================
// TypeEntry Implementation
// =============================================================================

impl TypeEntry {
    pub fn from_metadata(metadata: &TypeMetadata, ty: Ty) -> Self {
        let (name, kind, layout) = match metadata {
            TypeMetadata::PrimitiveType(rigid) => {
                (format!("{:?}", rigid), TypeKind::Primitive, None)
            }
            TypeMetadata::StructType {
                name,
                fields,
                layout,
                ..
            } => {
                let layout_info = layout.as_ref().map(LayoutInfo::from_shape);
                let field_infos = Self::make_field_infos(fields, layout_info.as_ref());
                (
                    name.clone(),
                    TypeKind::Struct {
                        fields: field_infos,
                    },
                    layout_info,
                )
            }
            TypeMetadata::EnumType {
                name,
                fields,
                discriminants,
                layout,
                ..
            } => {
                let layout_info = layout.as_ref().map(LayoutInfo::from_shape);
                let variants = discriminants
                    .iter()
                    .zip(fields.iter())
                    .map(|(&discr, variant_fields)| VariantInfo {
                        discriminant: discr,
                        fields: variant_fields
                            .iter()
                            .map(|&t| FieldInfo {
                                ty: t,
                                offset: None, // Enum variant offsets require variant-specific layout
                            })
                            .collect(),
                    })
                    .collect();
                (name.clone(), TypeKind::Enum { variants }, layout_info)
            }
            TypeMetadata::UnionType {
                name,
                fields,
                layout,
                ..
            } => {
                let layout_info = layout.as_ref().map(LayoutInfo::from_shape);
                // Union fields all start at offset 0
                let field_infos: Vec<FieldInfo> = fields
                    .iter()
                    .map(|&t| FieldInfo {
                        ty: t,
                        offset: Some(0),
                    })
                    .collect();
                (
                    name.clone(),
                    TypeKind::Union {
                        fields: field_infos,
                    },
                    layout_info,
                )
            }
            TypeMetadata::ArrayType {
                elem_type,
                size,
                layout,
            } => {
                let layout_info = layout.as_ref().map(LayoutInfo::from_shape);
                let len = size.as_ref().and_then(|s| s.eval_target_usize().ok());
                (
                    format!("{}", ty),
                    TypeKind::Array {
                        elem_ty: *elem_type,
                        len,
                    },
                    layout_info,
                )
            }
            TypeMetadata::TupleType { types, layout } => {
                let layout_info = layout.as_ref().map(LayoutInfo::from_shape);
                (
                    format!("{}", ty),
                    TypeKind::Tuple {
                        fields: types.clone(),
                    },
                    layout_info,
                )
            }
            TypeMetadata::PtrType {
                pointee_type,
                layout,
            } => {
                let layout_info = layout.as_ref().map(LayoutInfo::from_shape);
                (
                    format!("{}", ty),
                    TypeKind::Ptr {
                        pointee: *pointee_type,
                    },
                    layout_info,
                )
            }
            TypeMetadata::RefType {
                pointee_type,
                layout,
            } => {
                let layout_info = layout.as_ref().map(LayoutInfo::from_shape);
                (
                    format!("{}", ty),
                    TypeKind::Ref {
                        pointee: *pointee_type,
                    },
                    layout_info,
                )
            }
            TypeMetadata::FunType(name) => (name.clone(), TypeKind::Function, None),
            TypeMetadata::VoidType => ("()".to_string(), TypeKind::Void, None),
        };

        Self { name, kind, layout }
    }

    fn make_field_infos(fields: &[Ty], layout: Option<&LayoutInfo>) -> Vec<FieldInfo> {
        fields
            .iter()
            .enumerate()
            .map(|(i, &ty)| FieldInfo {
                ty,
                offset: layout.and_then(|l| l.field_offsets.get(i).copied()),
            })
            .collect()
    }

    /// Get a detailed description of this type including layout
    pub fn detailed_description(&self, type_index: &TypeIndex) -> String {
        let mut desc = self.name.clone();
        if let Some(layout) = &self.layout {
            desc.push_str(&format!(" ({} bytes, align {})", layout.size, layout.align));
        }
        match &self.kind {
            TypeKind::Struct { fields } | TypeKind::Union { fields } => {
                if !fields.is_empty() {
                    desc.push_str(" { ");
                    let field_strs: Vec<String> = fields
                        .iter()
                        .map(|f| {
                            let ty_name = type_index.get_name(f.ty);
                            match f.offset {
                                Some(off) => format!("@{}: {}", off, ty_name),
                                None => ty_name,
                            }
                        })
                        .collect();
                    desc.push_str(&field_strs.join(", "));
                    desc.push_str(" }");
                }
            }
            _ => {}
        }
        desc
    }
}

// =============================================================================
// LayoutInfo Implementation
// =============================================================================

impl LayoutInfo {
    pub fn from_shape(shape: &LayoutShape) -> Self {
        let field_offsets = match &shape.fields {
            FieldsShape::Primitive => vec![],
            FieldsShape::Union(_) => vec![0], // All fields at offset 0
            FieldsShape::Array { stride, count } => {
                // Generate offsets for each element
                (0..*count).map(|i| (i as usize) * stride.bytes()).collect()
            }
            FieldsShape::Arbitrary { offsets } => offsets.iter().map(|o| o.bytes()).collect(),
        };

        Self {
            size: shape.size.bytes(),
            align: shape.abi_align as usize,
            field_offsets,
        }
    }

    /// Get the offset for a specific field index
    pub fn field_offset(&self, index: usize) -> Option<usize> {
        self.field_offsets.get(index).copied()
    }
}
