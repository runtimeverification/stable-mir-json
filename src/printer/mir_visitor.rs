//! MIR body traversal for collecting calls, allocations, types, and spans.
//!
//! [`BodyAnalyzer`] implements `MirVisitor` and walks each function body exactly
//! once, collecting:
//! - function calls and drop glue into the link map
//! - global allocations (memory, statics, vtables, function pointers) with
//!   provenance type resolution via [`get_prov_ty`]
//! - reachable types via the type visitor
//! - source spans
//!
//! [`get_prov_ty`] recursively resolves the type of a pointer at a given byte
//! offset within a struct or tuple, walking down through nested fields until it
//! reaches the actual pointer type.

extern crate rustc_middle;
extern crate rustc_smir;
extern crate rustc_span;
extern crate stable_mir;

use rustc_middle::ty::{TyCtxt, TypingEnv};
use rustc_smir::rustc_internal::{self, internal};
use stable_mir::abi::{FieldsShape, LayoutShape};
use stable_mir::mir::alloc::GlobalAlloc;
use stable_mir::mir::mono::Instance;
use stable_mir::mir::visit::MirVisitor;
use stable_mir::mir::{LocalDecl, Rvalue, Terminator, TerminatorKind};
use stable_mir::ty::{ConstDef, IndexedVal};
use stable_mir::visitor::Visitable;
use stable_mir::CrateDef;

use super::link_map::{fn_inst_sym, update_link_map};
use super::schema::{AllocMap, ItemSource, LinkMap, SpanMap, FPTR, ITEM, TERM};
use super::ty_visitor::TyCollector;
use super::util::{fn_inst_for_ty, mono_item_name_int};

/// Single-pass body visitor that collects all derived information from a MIR body:
/// link map entries (calls, drops, fn pointers), allocations, types, spans,
/// and unevaluated constant references (for transitive item discovery).
///
/// By combining what was previously two separate visitors (BodyAnalyzer
/// and UnevaluatedConstCollector), each body is walked exactly once.
pub(super) struct BodyAnalyzer<'tcx, 'local> {
    pub tcx: TyCtxt<'tcx>,
    pub locals: &'local [LocalDecl],
    pub link_map: &'local mut LinkMap<'tcx>,
    pub visited_allocs: &'local mut AllocMap,
    pub ty_visitor: &'local mut TyCollector<'tcx>,
    pub spans: &'local mut SpanMap,
    /// Unevaluated constants discovered during this body walk.
    /// The outer fixpoint loop uses these to discover and create new Items.
    pub new_unevaluated: &'local mut Vec<UnevalConstInfo>,
}

/// Information about an unevaluated constant discovered during body analysis.
/// The outer fixpoint loop in collect_and_analyze_items uses this to create
/// new Items for transitively discovered mono items.
pub(super) struct UnevalConstInfo {
    pub const_def: ConstDef,
    pub item_name: String,
    pub mono_item: stable_mir::mir::mono::MonoItem,
}

/// Register a `MonoItem::Fn` in the link map (when `LINK_ITEMS` is enabled).
pub(super) fn maybe_add_to_link_map<'tcx>(
    tcx: TyCtxt<'tcx>,
    mono_item: &stable_mir::mir::mono::MonoItem,
    link_map: &mut LinkMap<'tcx>,
) {
    if !super::link_items_enabled() {
        return;
    }
    if let stable_mir::mir::mono::MonoItem::Fn(inst) = mono_item {
        update_link_map(
            link_map,
            fn_inst_sym(tcx, None, Some(inst)),
            ItemSource(ITEM),
        );
    }
}

/// Returns the field index (source order) for a given offset and layout if
/// the layout contains fields (shared between all variants), otherwise None.
/// NB No search for fields within variants (needs recursive call).
fn field_for_offset(l: &LayoutShape, offset: usize) -> Option<usize> {
    match &l.fields {
        FieldsShape::Primitive | FieldsShape::Union(_) | FieldsShape::Array { .. } => None,
        FieldsShape::Arbitrary { offsets } => offsets
            .iter()
            .enumerate()
            .find(|(_, o)| o.bytes() == offset)
            .map(|(i, _)| i),
    }
}

/// Find the field whose byte range contains the given offset by scanning for
/// the field with the largest start offset that doesn't exceed the target.
/// Returns `(field_index, field_start_byte_offset)`. Single linear pass; no
/// allocation or sorting needed since we only track the running best.
fn field_containing_offset(l: &LayoutShape, offset: usize) -> Option<(usize, usize)> {
    match &l.fields {
        FieldsShape::Arbitrary { offsets } => {
            let mut best: Option<(usize, usize)> = None;
            for (i, o) in offsets.iter().enumerate() {
                let start = o.bytes();
                if start <= offset {
                    match best {
                        None => best = Some((i, start)),
                        Some((_, best_start)) if start > best_start => {
                            best = Some((i, start));
                        }
                        _ => {}
                    }
                }
            }
            best
        }
        _ => None,
    }
}

fn opaque_placeholder_ty() -> stable_mir::ty::Ty {
    stable_mir::ty::Ty::to_val(0)
}

fn get_prov_ty(ty: stable_mir::ty::Ty, offset: &usize) -> Option<stable_mir::ty::Ty> {
    use stable_mir::ty::RigidTy;
    let ty_kind = ty.kind();
    debug_log_println!("get_prov_ty: {:?} offset={}", ty_kind, offset);
    // if ty is a pointer, box, or Ref, expect no offset and dereference
    if let Some(derefed) = ty_kind.builtin_deref(true) {
        if *offset != 0 {
            eprintln!(
                "get_prov_ty: unexpected non-zero offset {} for builtin_deref type {:?}",
                offset, ty_kind
            );
            return None;
        }
        debug_log_println!("get_prov_ty: resolved -> pointee {:?}", derefed.ty.kind());
        return Some(derefed.ty);
    }

    // Otherwise the allocation is a reference within another kind of data.
    // Decompose this outer data type to determine the reference type
    let layout = match ty.layout().map(|l| l.shape()) {
        Ok(l) => l,
        Err(_) => {
            eprintln!("get_prov_ty: unable to get layout for {:?}", ty_kind);
            return None;
        }
    };
    let rigid = match ty_kind.rigid() {
        Some(r) => r,
        None => {
            eprintln!(
                "get_prov_ty: non-rigid type in allocation: {:?} (offset={})",
                ty_kind, offset
            );
            return None;
        }
    };
    let ref_ty = match rigid {
        // homogenous, so no choice. Could check alignment of the offset...
        RigidTy::Array(ty, _) | RigidTy::Slice(ty) => Some(*ty),
        // cases covered above
        RigidTy::Ref(_, _, _) | RigidTy::RawPtr(_, _) => {
            unreachable!("Covered by builtin_deref above")
        }
        RigidTy::Adt(def, _) if def.is_box() => {
            unreachable!("Covered by builtin_deref above")
        }
        // For structs, find the field containing this offset and recurse.
        // The provenance offset may point into a nested struct field, so we
        // walk down through the field hierarchy until we reach the pointer.
        RigidTy::Adt(adt_def, args) if ty_kind.is_struct() => {
            let (field_idx, field_start) = field_containing_offset(&layout, *offset)?;
            // NB struct, single variant
            let fields = adt_def.variants().pop().map(|v| v.fields())?;
            let field_ty = fields.get(field_idx)?.ty_with_args(args);
            let relative_offset = *offset - field_start;
            debug_log_println!(
                "get_prov_ty: struct {:?} offset={} -> field {} (start={}) type {:?}, relative_offset={}",
                adt_def, offset, field_idx, field_start, field_ty.kind(), relative_offset
            );
            return get_prov_ty(field_ty, &relative_offset);
        }
        RigidTy::Adt(_adt_def, _args) if ty_kind.is_enum() => {
            // we have to figure out which variant we are dealing with (requires the data)
            match field_for_offset(&layout, *offset) {
                // FIXME we'd have to figure out which variant we are dealing with (requires the data)
                None => None,
                // FIXME we'd have to figure out where that shared field is in the source ordering
                Some(_idx) => None,
            }
        }
        // Same as structs: find containing field and recurse.
        RigidTy::Tuple(fields) => {
            let (field_idx, field_start) = field_containing_offset(&layout, *offset)?;
            let field_ty = *fields.get(field_idx)?;
            let relative_offset = *offset - field_start;
            debug_log_println!(
                "get_prov_ty: tuple offset={} -> field {} (start={}) type {:?}, relative_offset={}",
                offset,
                field_idx,
                field_start,
                field_ty.kind(),
                relative_offset
            );
            return get_prov_ty(field_ty, &relative_offset);
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
    offset: usize,
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
            let pointed_ty = get_prov_ty(ty, &offset);
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
                        collect_alloc(val_collector, p_ty, *prov_offset, prov.0);
                    });
            } else {
                val_collector
                    .visited_allocs
                    .insert(val, (opaque_placeholder_ty(), global_alloc.clone()));
            }
        }
        GlobalAlloc::Static(_) => {
            // Keep builtin-deref behavior; recover only non-builtin-deref cases.
            if kind.clone().builtin_deref(true).is_none() {
                let prov_ty = get_prov_ty(ty, &offset);
                debug_log_println!(
                    "DEBUG: GlobalAlloc::Static with non-builtin-deref type; alloc_id={:?}, ty={:?}, offset={}, kind={:?}, recovered_prov_ty={:?}",
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
                    // Recovery failed: do not treat outer container `ty` as pointee.
                    val_collector
                        .visited_allocs
                        .insert(val, (opaque_placeholder_ty(), global_alloc.clone()));
                }
            } else {
                val_collector
                    .visited_allocs
                    .insert(val, (ty, global_alloc.clone()));
            }
        }
        GlobalAlloc::VTable(_, _) => {
            // Same policy as Static: keep builtin-deref, recover non-builtin-deref.
            if kind.clone().builtin_deref(true).is_none() {
                let prov_ty = get_prov_ty(ty, &offset);
                debug_log_println!(
                    "DEBUG: GlobalAlloc::VTable with non-builtin-deref type; alloc_id={:?}, ty={:?}, offset={}, kind={:?}, recovered_prov_ty={:?}",
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
                    // Unknown is safer than wrong pointee type.
                    val_collector
                        .visited_allocs
                        .insert(val, (opaque_placeholder_ty(), global_alloc.clone()));
                }
            } else {
                val_collector
                    .visited_allocs
                    .insert(val, (ty, global_alloc.clone()));
            }
        }
        GlobalAlloc::Function(_) => {
            if !kind.is_fn_ptr() {
                let prov_ty = get_prov_ty(ty, &offset);
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
                    val_collector
                        .visited_allocs
                        .insert(val, (opaque_placeholder_ty(), global_alloc.clone()));
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
                    None
                } else {
                    let inst = fn_inst_for_ty(cnst.ty(), true)
                        .expect("Direct calls to functions must resolve to an instance");
                    fn_inst_sym(self.tcx, Some(cnst.ty()), Some(&inst))
                }
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
                    .for_each(|(offset, prov)| collect_alloc(self, constant.ty(), *offset, prov.0));
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
