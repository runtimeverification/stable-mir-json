//! Utility functions and traits for graph generation.

use std::hash::{DefaultHasher, Hash, Hasher};

extern crate stable_mir;
use stable_mir::mir::{
    AggregateKind, BorrowKind, ConstOperand, Mutability, NonDivergingIntrinsic, NullOp, Operand,
    Place, ProjectionElem, Rvalue, Terminator, TerminatorKind, UnwindAction,
};
use stable_mir::ty::{IndexedVal, RigidTy};

use crate::printer::FnSymType;

// =============================================================================
// GraphLabelString Trait
// =============================================================================

/// Rendering things as part of graph node labels
pub trait GraphLabelString {
    fn label(&self) -> String;
}

impl GraphLabelString for Place {
    fn label(&self) -> String {
        project(self.local.to_string(), &self.projection)
    }
}

impl GraphLabelString for Operand {
    fn label(&self) -> String {
        match &self {
            Operand::Constant(ConstOperand { const_, .. }) => {
                let ty = const_.ty();
                match &ty.kind() {
                    stable_mir::ty::TyKind::RigidTy(RigidTy::Int(_))
                    | stable_mir::ty::TyKind::RigidTy(RigidTy::Uint(_)) => {
                        format!("const ?_{}", const_.ty())
                    }
                    _ => format!("const {}", const_.ty()),
                }
            }
            Operand::Copy(place) => format!("cp({})", place.label()),
            Operand::Move(place) => format!("mv({})", place.label()),
        }
    }
}

impl GraphLabelString for AggregateKind {
    fn label(&self) -> String {
        use AggregateKind::*;
        match &self {
            Array(_ty) => "Array".to_string(),
            Tuple {} => "Tuple".to_string(),
            Adt(_, idx, _, _, _) => format!("Adt{{{}}}", idx.to_index()),
            Closure(_, _) => "Closure".to_string(),
            Coroutine(_, _, _) => "Coroutine".to_string(),
            RawPtr(ty, Mutability::Mut) => format!("*mut ({})", ty),
            RawPtr(ty, Mutability::Not) => format!("*({})", ty),
        }
    }
}

impl GraphLabelString for Rvalue {
    fn label(&self) -> String {
        use Rvalue::*;
        match &self {
            AddressOf(mutability, p) => match mutability {
                Mutability::Not => format!("&raw {}", p.label()),
                Mutability::Mut => format!("&raw mut {}", p.label()),
            },
            Aggregate(kind, operands) => {
                let os: Vec<String> = operands.iter().map(|op| op.label()).collect();
                format!("{} ({})", kind.label(), os.join(", "))
            }
            BinaryOp(binop, op1, op2) => format!("{:?}({}, {})", binop, op1.label(), op2.label()),
            Cast(kind, op, _ty) => format!("Cast-{:?} {}", kind, op.label()),
            CheckedBinaryOp(binop, op1, op2) => {
                format!("chkd-{:?}({}, {})", binop, op1.label(), op2.label())
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
            Repeat(op, _ty_const) => format!("Repeat {}", op.label()),
            ShallowInitBox(op, _ty) => format!("ShallowInitBox({})", op.label()),
            ThreadLocalRef(_item) => "ThreadLocalRef".to_string(),
            NullaryOp(nullop, ty) => format!("{} :: {}", nullop.label(), ty),
            UnaryOp(unop, op) => format!("{:?}({})", unop, op.label()),
            Use(op) => format!("Use({})", op.label()),
        }
    }
}

impl GraphLabelString for NullOp {
    fn label(&self) -> String {
        match &self {
            NullOp::OffsetOf(_vec) => "OffsetOf(..)".to_string(),
            other => format!("{:?}", other),
        }
    }
}

impl GraphLabelString for NonDivergingIntrinsic {
    fn label(&self) -> String {
        use NonDivergingIntrinsic::*;
        match &self {
            Assume(op) => format!("Assume {}", op.label()),
            CopyNonOverlapping(c) => format!(
                "CopyNonOverlapping: {} <- {}({}))",
                c.dst.label(),
                c.src.label(),
                c.count.label()
            ),
        }
    }
}

// =============================================================================
// Projection Helpers
// =============================================================================

fn project(local: String, ps: &[ProjectionElem]) -> String {
    ps.iter().fold(local, decorate)
}

fn decorate(thing: String, p: &ProjectionElem) -> String {
    match p {
        ProjectionElem::Deref => format!("(*{})", thing),
        ProjectionElem::Field(i, _) => format!("{thing}.{i}"),
        ProjectionElem::Index(local) => format!("{thing}[_{local}]"),
        ProjectionElem::ConstantIndex {
            offset,
            min_length: _,
            from_end,
        } => format!("{thing}[{}{}]", if *from_end { "-" } else { "" }, offset),
        ProjectionElem::Subslice { from, to, from_end } => {
            format!(
                "{thing}[{}..{}{}]",
                from,
                if *from_end { "-" } else { "" },
                to
            )
        }
        ProjectionElem::Downcast(i) => format!("({thing} as variant {})", i.to_index()),
        ProjectionElem::OpaqueCast(ty) => format!("{thing} as type {ty}"),
        ProjectionElem::Subtype(i) => format!("{thing} :> {i}"),
    }
}

// =============================================================================
// Name Helpers
// =============================================================================

/// Shorten a function name for display
pub fn short_fn_name(name: &str) -> String {
    name.rsplit("::").next().unwrap_or(name).to_string()
}

/// Check if a name is unqualified (no :: separators)
pub fn is_unqualified(name: &str) -> bool {
    !name.contains("::")
}

/// Convert FnSymType to a display string
pub fn function_string(f: FnSymType) -> String {
    match f {
        FnSymType::NormalSym(name) => name,
        FnSymType::NoOpSym(name) => format!("NoOp: {name}"),
        FnSymType::IntrinsicSym(name) => format!("Intr: {name}"),
    }
}

/// Format a name with line breaks for display
pub fn name_lines(name: &str) -> String {
    name.split_inclusive(" ")
        .flat_map(|s| s.as_bytes().chunks(25))
        .map(|bs| core::str::from_utf8(bs).unwrap().to_string())
        .collect::<Vec<String>>()
        .join("\\n")
}

/// Generate a consistent short name (hash-based) for a function
pub fn short_name(function_name: &str) -> String {
    let mut h = DefaultHasher::new();
    function_name.hash(&mut h);
    format!("X{:x}", h.finish())
}

/// Generate a consistent block name within a function
pub fn block_name(function_name: &str, id: usize) -> String {
    let mut h = DefaultHasher::new();
    function_name.hash(&mut h);
    format!("X{:x}_{}", h.finish(), id)
}

// =============================================================================
// Escape Helpers
// =============================================================================

/// Escape special characters for D2 string labels
pub fn escape_d2(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
}

// =============================================================================
// Byte Helpers
// =============================================================================

/// Convert byte slice to u64, little-endian (least significant byte first)
pub fn bytes_to_u64_le(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .enumerate()
        .fold(0u64, |acc, (i, &b)| acc | ((b as u64) << (i * 8)))
}

// =============================================================================
// Terminator Helpers
// =============================================================================

/// Get target block indices from a terminator
pub fn terminator_targets(term: &Terminator) -> Vec<usize> {
    use TerminatorKind::*;
    match &term.kind {
        Goto { target } => vec![*target],
        SwitchInt { targets, .. } => {
            let mut result: Vec<usize> = targets.branches().map(|(_, t)| t).collect();
            result.push(targets.otherwise());
            result
        }
        Resume {} | Abort {} | Return {} | Unreachable {} => vec![],
        Drop { target, unwind, .. } => {
            let mut result = vec![*target];
            if let UnwindAction::Cleanup(t) = unwind {
                result.push(*t);
            }
            result
        }
        Call { target, unwind, .. } => {
            let mut result = vec![];
            if let Some(t) = target {
                result.push(*t);
            }
            if let UnwindAction::Cleanup(t) = unwind {
                result.push(*t);
            }
            result
        }
        Assert { target, unwind, .. } => {
            let mut result = vec![*target];
            if let UnwindAction::Cleanup(t) = unwind {
                result.push(*t);
            }
            result
        }
        InlineAsm {
            destination,
            unwind,
            ..
        } => {
            let mut result = vec![];
            if let Some(t) = destination {
                result.push(*t);
            }
            if let UnwindAction::Cleanup(t) = unwind {
                result.push(*t);
            }
            result
        }
    }
}
