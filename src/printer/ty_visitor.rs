//! Recursive type visitor for collecting reachable types with layout info.
//!
//! [`TyCollector`] implements `stable_mir::visitor::Visitor` and traverses
//! type trees, recording each relevant type along with its `TyKind` and
//! `LayoutShape`. These collected types are later transformed into
//! [`TypeMetadata`](super::schema::TypeMetadata) entries in the final output.
//! Note that some special kinds (function definitions/pointers and coroutine
//! witnesses) are traversed only to gather the types they reference and are
//! not themselves stored as entries in the type map.
//!
//! # Dedup, cycle breaking, and trace provenance
//!
//! Types can be self-referential (e.g. a linked list whose field is
//! `Option<Box<Self>>`), so `visit_ty` maintains a dedup guard: once a type
//! appears in `types` or `resolved`, we skip recursion to break cycles.
//! This is correct for the types map (the same type always produces the same
//! `TyKind` and `LayoutShape`), but it creates a provenance problem for
//! tracing.
//!
//! The pipeline walks mono item bodies in HashMap iteration order (see
//! `collect_and_analyze_items`), which is non-deterministic. Without special
//! handling, a nested type like `Foreign` (the pointee inside `*const Opaque`)
//! would only be traced from whichever body *first* visited the outer
//! `RawPtr`; if a stdlib body happens to win, the trace report would label
//! the type "stdlib only" even though user code references it too.
//!
//! To get deterministic provenance the visitor uses a **descendant replay**
//! strategy:
//!
//! 1. **First traversal**: before recursing into a type's children, snapshot
//!    the trace buffer length. After recursion, everything appended past that
//!    point is a descendant. Stash those `ty_kind` tags in a `descendants`
//!    map keyed by the outer type.
//!
//! 2. **Subsequent dedup hits**: instead of just emitting one event for the
//!    outer type, replay the full set of descendant tags too. Every body that
//!    references `*const Opaque` therefore also gets credited for `Foreign`,
//!    regardless of walk order.
//!
//! The `descendants` map is only populated when `TRACE=1`, so there is no
//! overhead in normal operation. The types map and recursion/cycle-breaking
//! logic are completely unaffected.

use crate::compat::middle::ty::TyCtxt;
use crate::compat::stable_mir;

use std::collections::{HashMap, HashSet};
use std::ops::ControlFlow;

use stable_mir::mir::mono::Instance;
use stable_mir::ty::{RigidTy, TyKind};
use stable_mir::visitor::{Visitable, Visitor};

use super::schema::TyMap;
use super::tracer::{ty_kind_tag, TraceEvent};

pub(super) struct TyCollector<'tcx> {
    tcx: TyCtxt<'tcx>,
    /// The collected types map: `Ty -> (TyKind, Option<LayoutShape>)`.
    /// Serves as the dedup guard for types that get stored (everything
    /// except FnDef, FnPtr, and CoroutineWitness).
    pub types: TyMap,
    /// Dedup guard for traversal-only types (FnDef, FnPtr, Closure before
    /// its successful insertion). Together with `types`, prevents infinite
    /// recursion on self-referential type trees.
    resolved: HashSet<stable_mir::ty::Ty>,
    /// When tracing is enabled (`TRACE=1`), newly collected types are
    /// buffered here. The caller (`BodyAnalyzer::visit_ty`) drains this
    /// buffer after each `ty.visit()` call and stamps each event with
    /// the current item context and source location. This sidesteps
    /// the double-`&mut` problem (BodyAnalyzer borrows both the tracer
    /// and the TyCollector).
    pub trace_buffer: Option<Vec<TraceEvent>>,
    /// Descendant replay map for trace provenance (see module-level docs).
    ///
    /// During a type's first traversal, we snapshot the trace buffer before
    /// recursion and record every `ty_kind` tag appended during recursion as
    /// a "descendant" of that type. On subsequent dedup hits we replay these
    /// tags so the current body gets provenance credit for nested types it
    /// never directly visits (because the dedup guard skips recursion).
    ///
    /// Only populated when `trace_buffer` is `Some`.
    descendants: HashMap<stable_mir::ty::Ty, Vec<String>>,
}

impl TyCollector<'_> {
    pub fn new(tcx: TyCtxt<'_>, trace: bool) -> TyCollector {
        TyCollector {
            tcx,
            types: HashMap::new(),
            resolved: HashSet::new(),
            trace_buffer: if trace { Some(Vec::new()) } else { None },
            descendants: HashMap::new(),
        }
    }
}

// ── Trace helpers ────────────────────────────────────────────────────────
//
// These methods implement the snapshot/record/replay protocol described in
// the module-level docs. They are no-ops when tracing is disabled.

impl TyCollector<'_> {
    /// Push a `TypeCollected` trace event into the buffer.
    /// No-op when tracing is disabled.
    fn trace_type(&mut self, ty_kind: &str) {
        if let Some(buf) = &mut self.trace_buffer {
            buf.push(TraceEvent::TypeCollected {
                item: String::new(),
                location: None,
                ty_kind: ty_kind.to_string(),
            });
        }
    }

    /// Capture the current trace buffer length so we can later identify
    /// which events were appended during recursion into a type's children.
    ///
    /// Call this *before* `super_visit` / `visit_instance`. After recursion,
    /// pass the returned value to [`record_descendants`] to stash the
    /// descendant ty_kind tags.
    ///
    /// Returns 0 when tracing is disabled (the value is never used in that
    /// case, but returning 0 avoids an `Option` at every call site).
    #[inline(always)]
    fn trace_snapshot(&self) -> usize {
        match &self.trace_buffer {
            Some(buf) => buf.len(),
            None => 0,
        }
    }

    /// Scan the trace buffer from `snapshot` to the current end, extract
    /// every `TypeCollected` ty_kind tag, and stash them in the
    /// `descendants` map keyed by `ty`.
    ///
    /// Must be called *after* recursion completes (i.e. after `super_visit`
    /// or `visit_instance`), and only once per type (first traversal).
    /// The stashed tags are later replayed by [`replay_with_descendants`]
    /// on dedup hits.
    fn record_descendants(&mut self, ty: stable_mir::ty::Ty, snapshot: usize) {
        let Some(buf) = &self.trace_buffer else {
            return;
        };
        let descs: Vec<String> = buf[snapshot..]
            .iter()
            .filter_map(|ev| match ev {
                TraceEvent::TypeCollected { ty_kind, .. } => Some(ty_kind.clone()),
                _ => None,
            })
            .collect();
        if !descs.is_empty() {
            self.descendants.insert(ty, descs);
        }
    }

    /// Emit a `TypeCollected` trace event for `ty` itself, then replay
    /// all descendant ty_kind tags recorded during its first traversal.
    ///
    /// Called on dedup hits (when `types` or `resolved` already contain the
    /// type). This is the key to deterministic provenance: without replay,
    /// the dedup guard would skip recursion and nested types would only be
    /// attributed to whichever body happened to be walked first.
    ///
    /// Example: `*const Opaque` (RawPtr) contains `Opaque` (Foreign).
    /// On first traversal, `record_descendants` stashes `["Foreign"]` for
    /// the RawPtr. When a second body later visits `*const Opaque` and hits
    /// the dedup guard, we replay the `Foreign` tag so that body also gets
    /// credit for the nested type.
    fn replay_with_descendants(&mut self, ty: &stable_mir::ty::Ty) {
        let Some(buf) = &mut self.trace_buffer else {
            return;
        };

        buf.push(TraceEvent::TypeCollected {
            item: String::new(),
            location: None,
            ty_kind: ty_kind_tag(&ty.kind()).to_string(),
        });

        // Clone to avoid borrow conflict: self.descendants is read while
        // self.trace_buffer is mutably borrowed above.
        let Some(descs) = self.descendants.get(ty).cloned() else {
            return;
        };
        for ty_kind in descs {
            buf.push(TraceEvent::TypeCollected {
                item: String::new(),
                location: None,
                ty_kind,
            });
        }
    }
}

// ── Instance visitor ─────────────────────────────────────────────────────

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

// ── Visitor impl ─────────────────────────────────────────────────────────

impl Visitor for TyCollector<'_> {
    type Break = ();

    fn visit_ty(&mut self, ty: &stable_mir::ty::Ty) -> ControlFlow<Self::Break> {
        // ── Dedup guard / cycle breaker ──────────────────────────────
        // If we've already collected or resolved this type, don't recurse
        // (avoids infinite loops on self-referential types like linked
        // lists). Instead, replay the full descendant trace so the
        // *current* body gets provenance credit for nested types too.
        if self.types.contains_key(ty) || self.resolved.contains(ty) {
            self.replay_with_descendants(ty);
            return ControlFlow::Continue(());
        }

        // ── First traversal ──────────────────────────────────────────
        // Each branch follows the same trace protocol:
        //   1. trace_snapshot()       -- mark the buffer position
        //   2. recurse (super_visit / visit_instance)
        //   3. record_descendants()   -- stash child ty_kinds for replay
        //   4. trace_type()           -- push own TypeCollected event
        //   5. insert into types / resolved
        match ty.kind() {
            TyKind::RigidTy(RigidTy::Closure(def, ref args)) => {
                self.resolved.insert(*ty);
                let snap = self.trace_snapshot();
                let instance =
                    Instance::resolve_closure(def, args, stable_mir::ty::ClosureKind::Fn).unwrap();
                let control = self.visit_instance(instance);
                if !matches!(control, ControlFlow::Continue(_)) {
                    return control;
                }
                let kind = ty.kind();
                let maybe_layout_shape = ty.layout().ok().map(|layout| layout.shape());
                self.record_descendants(*ty, snap);
                self.trace_type(ty_kind_tag(&kind));
                self.types.insert(*ty, (kind, maybe_layout_shape));
                control
            }
            TyKind::RigidTy(RigidTy::CoroutineWitness(..)) => {
                debug_log_println!("DEBUG: TyCollector skipping CoroutineWitness: {:?}", ty);
                ControlFlow::Break(())
            }
            TyKind::RigidTy(RigidTy::FnDef(def, ref args)) => {
                self.resolved.insert(*ty);
                let snap = self.trace_snapshot();
                self.trace_type("FnDef");
                let instance = Instance::resolve(def, args).unwrap();
                let control = self.visit_instance(instance);
                self.record_descendants(*ty, snap);
                control
            }
            TyKind::RigidTy(RigidTy::FnPtr(binder_stable)) => {
                self.resolved.insert(*ty);
                let snap = self.trace_snapshot();
                self.trace_type("FnPtr");
                let fn_abi = crate::compat::types::fn_ptr_abi(self.tcx, binder_stable);
                let mut inputs_outputs: Vec<stable_mir::ty::Ty> =
                    fn_abi.args.iter().map(|arg_abi| arg_abi.ty).collect();
                inputs_outputs.push(fn_abi.ret.ty);
                let control = inputs_outputs.super_visit(self);
                self.record_descendants(*ty, snap);
                control
            }
            // The visitor won't collect field types for ADTs, therefore doing it explicitly.
            TyKind::RigidTy(RigidTy::Adt(adt_def, args)) => {
                let fields = adt_def
                    .variants()
                    .iter()
                    .flat_map(|v| v.fields())
                    .map(|f| f.ty_with_args(&args))
                    .collect::<Vec<_>>();

                let snap = self.trace_snapshot();
                let control = ty.super_visit(self);
                if !matches!(control, ControlFlow::Continue(_)) {
                    return control;
                }
                let kind = ty.kind();
                let maybe_layout_shape = ty.layout().ok().map(|layout| layout.shape());
                self.trace_type(ty_kind_tag(&kind));
                self.types.insert(*ty, (kind, maybe_layout_shape));
                let control = fields.super_visit(self);
                self.record_descendants(*ty, snap);
                control
            }
            _ => {
                let snap = self.trace_snapshot();
                let control = ty.super_visit(self);
                if !matches!(control, ControlFlow::Continue(_)) {
                    return control;
                }
                let kind = ty.kind();
                let maybe_layout_shape = ty.layout().ok().map(|layout| layout.shape());
                self.trace_type(ty_kind_tag(&kind));
                self.types.insert(*ty, (kind, maybe_layout_shape));
                self.record_descendants(*ty, snap);
                control
            }
        }
    }
}
