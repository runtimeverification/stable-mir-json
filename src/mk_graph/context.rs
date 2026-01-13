//! Graph context for rendering MIR with type and allocation information.

use std::collections::HashMap;

extern crate stable_mir;
use stable_mir::mir::{
    BorrowKind, ConstOperand, Mutability, NonDivergingIntrinsic, Operand, Rvalue, Statement,
    StatementKind, Terminator, TerminatorKind,
};
use stable_mir::ty::{ConstantKind, IndexedVal, MirConst, Ty};

use crate::printer::SmirJson;

use super::index::{AllocIndex, LayoutInfo, TypeEntry, TypeIndex, TypeKind};
use super::util::{function_string, short_fn_name, GraphLabelString};

// =============================================================================
// GraphContext
// =============================================================================

/// Context for rendering graph labels with access to indices
pub struct GraphContext {
    pub allocs: AllocIndex,
    pub types: TypeIndex,
    pub functions: HashMap<Ty, String>,
}

impl GraphContext {
    pub fn from_smir(smir: &SmirJson) -> Self {
        let types = TypeIndex::from_types(&smir.types);
        let allocs = AllocIndex::from_alloc_infos(&smir.allocs, &types);
        let functions: HashMap<Ty, String> = smir
            .functions
            .iter()
            .map(|(k, v)| (k.0, function_string(v.clone())))
            .collect();

        Self {
            allocs,
            types,
            functions,
        }
    }

    /// Render a constant operand with alloc information
    pub fn render_const(&self, const_: &MirConst) -> String {
        let ty = const_.ty();
        let ty_name = self.types.get_name(ty);

        match const_.kind() {
            ConstantKind::Allocated(alloc) => {
                // Check if this constant references any allocs via provenance
                if !alloc.provenance.ptrs.is_empty() {
                    let alloc_refs: Vec<String> = alloc
                        .provenance
                        .ptrs
                        .iter()
                        .map(|(_offset, prov)| self.allocs.describe(prov.0.to_index() as u64))
                        .collect();
                    format!("const [{}]", alloc_refs.join(", "))
                } else {
                    // Inline constant - try to show value
                    let bytes = &alloc.bytes;
                    // Convert Option<u8> to concrete bytes
                    let concrete_bytes: Vec<u8> = bytes.iter().filter_map(|&b| b).collect();
                    if concrete_bytes.len() <= 8 && !concrete_bytes.is_empty() {
                        format!(
                            "const {}_{}",
                            super::util::bytes_to_u64_le(&concrete_bytes),
                            ty_name
                        )
                    } else {
                        format!("const {}", ty_name)
                    }
                }
            }
            ConstantKind::ZeroSized => {
                // Function pointers, unit type, etc.
                if ty.kind().is_fn() {
                    if let Some(name) = self.functions.get(&ty) {
                        format!("const fn {}", short_fn_name(name))
                    } else {
                        format!("const {}", ty_name)
                    }
                } else {
                    format!("const {}", ty_name)
                }
            }
            ConstantKind::Ty(_) => format!("const {}", ty_name),
            ConstantKind::Unevaluated(_) => format!("const unevaluated {}", ty_name),
            ConstantKind::Param(_) => format!("const param {}", ty_name),
        }
    }

    /// Render an operand with context
    pub fn render_operand(&self, op: &Operand) -> String {
        match op {
            Operand::Constant(ConstOperand { const_, .. }) => self.render_const(const_),
            Operand::Copy(place) => format!("cp({})", place.label()),
            Operand::Move(place) => format!("mv({})", place.label()),
        }
    }

    /// Generate the allocs legend as lines for display
    pub fn allocs_legend_lines(&self) -> Vec<String> {
        let mut lines = vec!["ALLOCS".to_string()];
        let mut entries: Vec<_> = self.allocs.iter().collect();
        entries.sort_by_key(|e| e.alloc_id);
        for entry in entries {
            lines.push(entry.short_description());
        }
        lines
    }

    /// Resolve a call target to a function name if it's a constant function pointer
    pub fn resolve_call_target(&self, func: &Operand) -> Option<String> {
        match func {
            Operand::Constant(ConstOperand { const_, .. }) => {
                let ty = const_.ty();
                if ty.kind().is_fn() {
                    self.functions.get(&ty).cloned()
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Render statement with context for alloc/type information
    pub fn render_stmt(&self, s: &Statement) -> String {
        use StatementKind::*;
        match &s.kind {
            Assign(p, v) => format!("{} <- {}", p.label(), self.render_rvalue(v)),
            FakeRead(_cause, p) => format!("Fake-Read {}", p.label()),
            SetDiscriminant {
                place,
                variant_index,
            } => format!(
                "set discriminant {}({})",
                place.label(),
                variant_index.to_index()
            ),
            Deinit(p) => format!("Deinit {}", p.label()),
            StorageLive(l) => format!("Storage Live _{}", &l),
            StorageDead(l) => format!("Storage Dead _{}", &l),
            Retag(_retag_kind, p) => format!("Retag {}", p.label()),
            PlaceMention(p) => format!("Mention {}", p.label()),
            AscribeUserType {
                place,
                projections,
                variance: _,
            } => format!("Ascribe {}.{}", place.label(), projections.base),
            Coverage(_) => "Coverage".to_string(),
            Intrinsic(intr) => format!("Intr: {}", self.render_intrinsic(intr)),
            ConstEvalCounter {} => "ConstEvalCounter".to_string(),
            Nop {} => "Nop".to_string(),
        }
    }

    /// Render rvalue with context
    pub fn render_rvalue(&self, v: &Rvalue) -> String {
        use Rvalue::*;
        match v {
            AddressOf(mutability, p) => match mutability {
                Mutability::Not => format!("&raw {}", p.label()),
                Mutability::Mut => format!("&raw mut {}", p.label()),
            },
            Aggregate(kind, operands) => {
                let os: Vec<String> = operands.iter().map(|op| self.render_operand(op)).collect();
                format!("{} ({})", kind.label(), os.join(", "))
            }
            BinaryOp(binop, op1, op2) => format!(
                "{:?}({}, {})",
                binop,
                self.render_operand(op1),
                self.render_operand(op2)
            ),
            Cast(kind, op, _ty) => format!("Cast-{:?} {}", kind, self.render_operand(op)),
            CheckedBinaryOp(binop, op1, op2) => {
                format!(
                    "chkd-{:?}({}, {})",
                    binop,
                    self.render_operand(op1),
                    self.render_operand(op2)
                )
            }
            CopyForDeref(p) => format!("CopyForDeref({})", p.label()),
            Discriminant(p) => format!("Discriminant({})", p.label()),
            Len(p) => format!("Len({})", p.label()),
            Ref(_region, borrowkind, p) => {
                format!(
                    "&{} {}",
                    match borrowkind {
                        BorrowKind::Mut { kind: _ } => "mut",
                        _other => "",
                    },
                    p.label()
                )
            }
            Repeat(op, _ty_const) => format!("Repeat {}", self.render_operand(op)),
            ShallowInitBox(op, _ty) => format!("ShallowInitBox({})", self.render_operand(op)),
            ThreadLocalRef(_item) => "ThreadLocalRef".to_string(),
            NullaryOp(nullop, ty) => format!("{} :: {}", nullop.label(), ty),
            UnaryOp(unop, op) => format!("{:?}({})", unop, self.render_operand(op)),
            Use(op) => format!("Use({})", self.render_operand(op)),
        }
    }

    /// Render intrinsic with context
    pub fn render_intrinsic(&self, intr: &NonDivergingIntrinsic) -> String {
        use NonDivergingIntrinsic::*;
        match intr {
            Assume(op) => format!("Assume {}", self.render_operand(op)),
            CopyNonOverlapping(c) => format!(
                "CopyNonOverlapping: {} <- {}({})",
                c.dst.label(),
                c.src.label(),
                self.render_operand(&c.count)
            ),
        }
    }

    /// Render terminator with context for alloc/type information
    pub fn render_terminator(&self, term: &Terminator) -> String {
        use TerminatorKind::*;
        match &term.kind {
            Goto { .. } => "Goto".to_string(),
            SwitchInt { discr, .. } => format!("SwitchInt {}", self.render_operand(discr)),
            Resume {} => "Resume".to_string(),
            Abort {} => "Abort".to_string(),
            Return {} => "Return".to_string(),
            Unreachable {} => "Unreachable".to_string(),
            Drop { place, .. } => format!("Drop {}", place.label()),
            Call {
                func,
                args,
                destination,
                ..
            } => {
                let fn_name = self
                    .resolve_call_target(func)
                    .map(|n| short_fn_name(&n))
                    .unwrap_or_else(|| "?".to_string());
                let arg_str = args
                    .iter()
                    .map(|op| self.render_operand(op))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{} = {}({})", destination.label(), fn_name, arg_str)
            }
            Assert { cond, expected, .. } => {
                format!("Assert {} == {}", self.render_operand(cond), expected)
            }
            InlineAsm { .. } => "InlineAsm".to_string(),
        }
    }

    // =========================================================================
    // Type and Layout Rendering
    // =========================================================================

    /// Get detailed type information for a type
    pub fn get_type_entry(&self, ty: Ty) -> Option<&TypeEntry> {
        self.types.get(ty)
    }

    /// Get layout information for a type
    pub fn get_layout(&self, ty: Ty) -> Option<&LayoutInfo> {
        self.types.get_layout(ty)
    }

    /// Render a type with its size and alignment
    pub fn render_type_with_layout(&self, ty: Ty) -> String {
        let name = self.types.get_name(ty);
        match self.types.get_layout(ty) {
            Some(layout) => format!("{} ({} bytes, align {})", name, layout.size, layout.align),
            None => name,
        }
    }

    /// Render a type with detailed field layout (for structs/unions)
    pub fn render_type_detailed(&self, ty: Ty) -> String {
        match self.types.get(ty) {
            Some(entry) => entry.detailed_description(&self.types),
            None => format!("{}", ty),
        }
    }

    /// Generate lines describing a type's memory layout
    pub fn render_type_layout_lines(&self, ty: Ty) -> Vec<String> {
        let mut lines = Vec::new();
        let entry = match self.types.get(ty) {
            Some(e) => e,
            None => {
                lines.push(format!("{}", ty));
                return lines;
            }
        };

        // Header with size/align
        let header = match &entry.layout {
            Some(layout) => format!(
                "{} ({} bytes, align {})",
                entry.name, layout.size, layout.align
            ),
            None => entry.name.clone(),
        };
        lines.push(header);

        // Field details for composite types
        match &entry.kind {
            TypeKind::Struct { fields } => {
                for (i, field) in fields.iter().enumerate() {
                    let field_ty_name = self.types.get_name(field.ty);
                    let field_layout = self.types.get_layout(field.ty);
                    let size_str = field_layout
                        .map(|l| format!(" ({} bytes)", l.size))
                        .unwrap_or_default();
                    match field.offset {
                        Some(off) => lines.push(format!(
                            "  @{:3}: field{}: {}{}",
                            off, i, field_ty_name, size_str
                        )),
                        None => lines.push(format!("  field{}: {}{}", i, field_ty_name, size_str)),
                    }
                }
            }
            TypeKind::Union { fields } => {
                lines.push("  (all fields at offset 0)".to_string());
                for (i, field) in fields.iter().enumerate() {
                    let field_ty_name = self.types.get_name(field.ty);
                    let field_layout = self.types.get_layout(field.ty);
                    let size_str = field_layout
                        .map(|l| format!(" ({} bytes)", l.size))
                        .unwrap_or_default();
                    lines.push(format!("  field{}: {}{}", i, field_ty_name, size_str));
                }
            }
            TypeKind::Enum { variants } => {
                for (i, variant) in variants.iter().enumerate() {
                    lines.push(format!(
                        "  variant {} (discriminant {}):",
                        i, variant.discriminant
                    ));
                    for (j, field) in variant.fields.iter().enumerate() {
                        let field_ty_name = self.types.get_name(field.ty);
                        lines.push(format!("    field{}: {}", j, field_ty_name));
                    }
                }
            }
            TypeKind::Tuple { fields } => {
                for (i, &field_ty) in fields.iter().enumerate() {
                    let field_ty_name = self.types.get_name(field_ty);
                    let offset = entry.layout.as_ref().and_then(|l| l.field_offset(i));
                    match offset {
                        Some(off) => lines.push(format!("  @{:3}: .{}: {}", off, i, field_ty_name)),
                        None => lines.push(format!("  .{}: {}", i, field_ty_name)),
                    }
                }
            }
            TypeKind::Array { elem_ty, len } => {
                let elem_name = self.types.get_name(*elem_ty);
                let len_str = len.map(|l| l.to_string()).unwrap_or("?".to_string());
                lines.push(format!("  [{}; {}]", elem_name, len_str));
            }
            _ => {}
        }

        lines
    }

    /// Generate the types legend as lines for display (types with layout info)
    pub fn types_legend_lines(&self) -> Vec<String> {
        let mut lines = vec!["TYPES".to_string()];

        // Collect and sort types that have interesting layout info
        let mut entries: Vec<_> = self
            .types
            .iter()
            .filter(|(_, entry)| {
                // Only include composite types with layout
                matches!(
                    entry.kind,
                    TypeKind::Struct { .. }
                        | TypeKind::Union { .. }
                        | TypeKind::Enum { .. }
                        | TypeKind::Tuple { .. }
                )
            })
            .collect();
        entries.sort_by(|a, b| a.1.name.cmp(&b.1.name));

        for (_ty_id, entry) in entries {
            let layout_str = entry
                .layout
                .as_ref()
                .map(|l| format!(" ({} bytes)", l.size))
                .unwrap_or_default();
            lines.push(format!("{}{}", entry.name, layout_str));
        }

        lines
    }
}
