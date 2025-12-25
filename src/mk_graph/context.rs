//! Graph context for rendering MIR with type and allocation information.

use std::collections::HashMap;

extern crate stable_mir;
use stable_mir::mir::{
    BorrowKind, ConstOperand, Mutability, NonDivergingIntrinsic, Operand, Rvalue, Statement,
    StatementKind, Terminator, TerminatorKind,
};
use stable_mir::ty::{ConstantKind, IndexedVal, MirConst, Ty};

use crate::printer::SmirJson;

use super::index::{AllocIndex, TypeIndex};
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
}
