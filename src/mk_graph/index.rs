//! Index structures for looking up allocations and types.

use std::collections::HashMap;

extern crate stable_mir;
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
    by_id: HashMap<u64, String>,
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
                let is_str = ty_name.contains("str") || ty_name.contains("&str");

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
            let name = Self::type_name_from_metadata(metadata, *ty);
            index.by_id.insert(ty.to_index() as u64, name);
        }
        index
    }

    fn type_name_from_metadata(metadata: &TypeMetadata, ty: Ty) -> String {
        match metadata {
            TypeMetadata::PrimitiveType(rigid) => format!("{:?}", rigid),
            TypeMetadata::EnumType { name, .. } => name.clone(),
            TypeMetadata::StructType { name, .. } => name.clone(),
            TypeMetadata::UnionType { name, .. } => name.clone(),
            TypeMetadata::ArrayType { .. } => format!("{}", ty),
            TypeMetadata::PtrType { .. } => format!("{}", ty),
            TypeMetadata::RefType { .. } => format!("{}", ty),
            TypeMetadata::TupleType { .. } => format!("{}", ty),
            TypeMetadata::FunType(name) => name.clone(),
            TypeMetadata::VoidType => "()".to_string(),
        }
    }

    pub fn get_name(&self, ty: Ty) -> String {
        self.by_id
            .get(&(ty.to_index() as u64))
            .cloned()
            .unwrap_or_else(|| format!("{}", ty))
    }
}
