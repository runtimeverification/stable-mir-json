//! MIR body traversal for collecting interned values.
//!
//! Walks all function and static bodies to populate:
//! - The **link map** (function calls and fn-pointer casts),
//! - The **allocation map** (global allocations reachable from constants),
//! - The **type map** (all types encountered in MIR), and
//! - The **span map** (source locations for all MIR spans).

extern crate rustc_middle;
extern crate rustc_smir;
extern crate rustc_span;
extern crate stable_mir;

use std::collections::HashMap;

use rustc_middle::ty::TyCtxt;
use rustc_smir::rustc_internal::internal;
use stable_mir::abi::{FieldsShape, LayoutShape};
use stable_mir::mir::alloc::GlobalAlloc;
use stable_mir::mir::mono::{Instance, MonoItem};
use stable_mir::mir::visit::MirVisitor;
use stable_mir::mir::{LocalDecl, Rvalue, Terminator, TerminatorKind};
use stable_mir::ty::IndexedVal;
use stable_mir::visitor::Visitable;
use stable_mir::CrateDef;

use super::link_map::{fn_inst_sym, update_link_map};
use super::schema::{
    AllocMap, InternedValues, Item, ItemSource, LinkMap, MonoItemKind, SpanMap, FPTR, ITEM, TERM,
};
use super::ty_visitor::TyCollector;
use super::util::{def_id_to_inst, fn_inst_for_ty};

struct InternedValueCollector<'tcx, 'local> {
    tcx: TyCtxt<'tcx>,
    _sym: String,
    locals: &'local [LocalDecl],
    link_map: &'local mut LinkMap<'tcx>,
    visited_allocs: &'local mut AllocMap,
    ty_visitor: &'local mut TyCollector<'tcx>,
    spans: &'local mut SpanMap,
}

/// Returns the field index (source order) for a given offset and layout if
/// the layout contains fields (shared between all variants), otherwise None.
/// NB No search for fields within variants (needs recursive call).
fn field_for_offset(l: LayoutShape, offset: usize) -> Option<usize> {
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
    val_collector: &mut InternedValueCollector,
    ty: stable_mir::ty::Ty,
    offset: &usize,
    val: stable_mir::mir::alloc::AllocId,
) {
    let entry = val_collector.visited_allocs.entry(val);
    if matches!(entry, std::collections::hash_map::Entry::Occupied(_)) {
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
                entry.or_insert((p_ty, global_alloc.clone()));
                alloc
                    .provenance
                    .ptrs
                    .iter()
                    .for_each(|(prov_offset, prov)| {
                        collect_alloc(val_collector, p_ty, prov_offset, prov.0);
                    });
            } else {
                entry.or_insert((stable_mir::ty::Ty::to_val(0), global_alloc.clone()));
            }
        }
        GlobalAlloc::Static(_) => {
            assert!(
                kind.clone().builtin_deref(true).is_some(),
                "Allocated pointer is not a built-in pointer type: {:?}",
                kind
            );
            entry.or_insert((ty, global_alloc.clone()));
        }
        GlobalAlloc::VTable(_, _) => {
            assert!(
                kind.clone().builtin_deref(true).is_some(),
                "Allocated pointer is not a built-in pointer type: {:?}",
                kind
            );
            entry.or_insert((ty, global_alloc.clone()));
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
                    entry.or_insert((p_ty, global_alloc.clone()));
                } else {
                    // Could not recover a precise pointee type; use an opaque 0-valued Ty
                    // as a conservative placeholder.
                    entry.or_insert((stable_mir::ty::Ty::to_val(0), global_alloc.clone()));
                }
            } else {
                entry.or_insert((ty, global_alloc.clone()));
            }
        }
    };
}

impl MirVisitor for InternedValueCollector<'_, '_> {
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
        use stable_mir::ty::{ConstantKind, TyConstKind};
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
            ConstantKind::Unevaluated(_) | ConstantKind::Param(_) => {}
        }
        self.super_mir_const(constant, loc);
    }

    fn visit_ty(&mut self, ty: &stable_mir::ty::Ty, _location: stable_mir::mir::visit::Location) {
        ty.visit(self.ty_visitor);
        self.super_ty(ty);
    }
}

pub(super) fn collect_interned_values<'tcx>(
    tcx: TyCtxt<'tcx>,
    items: &[Item],
) -> InternedValues<'tcx> {
    let mut calls_map = HashMap::new();
    let mut visited_allocs = HashMap::new();
    let mut ty_visitor = TyCollector::new(tcx);
    let mut span_map = HashMap::new();
    if super::link_items_enabled() {
        for item in items.iter() {
            if let MonoItem::Fn(inst) = &item.mono_item {
                update_link_map(
                    &mut calls_map,
                    fn_inst_sym(tcx, None, Some(inst)),
                    ItemSource(ITEM),
                )
            }
        }
    }
    for item in items.iter() {
        match &item.mono_item {
            MonoItem::Fn(inst) => {
                if let MonoItemKind::MonoItemFn {
                    body: Some(body), ..
                } = &item.mono_item_kind
                {
                    InternedValueCollector {
                        tcx,
                        _sym: inst.mangled_name(),
                        locals: body.locals(),
                        link_map: &mut calls_map,
                        visited_allocs: &mut visited_allocs,
                        ty_visitor: &mut ty_visitor,
                        spans: &mut span_map,
                    }
                    .visit_body(body)
                } else {
                    eprintln!(
                        "Failed to retrive body for Instance of MonoItem::Fn {}",
                        inst.name()
                    )
                }
            }
            MonoItem::Static(def) => {
                let inst = def_id_to_inst(tcx, def.def_id());
                if let Some(body) = inst.body() {
                    InternedValueCollector {
                        tcx,
                        _sym: inst.mangled_name(),
                        locals: &[],
                        link_map: &mut calls_map,
                        visited_allocs: &mut visited_allocs,
                        ty_visitor: &mut ty_visitor,
                        spans: &mut span_map,
                    }
                    .visit_body(&body)
                } else {
                    eprintln!(
                        "Failed to retrive body for Instance of MonoItem::Static {}",
                        inst.name()
                    )
                }
            }
            MonoItem::GlobalAsm(_) => {}
        }
    }
    (calls_map, visited_allocs, ty_visitor.types, span_map)
}
