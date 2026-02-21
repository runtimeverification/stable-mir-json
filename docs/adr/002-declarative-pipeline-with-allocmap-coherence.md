# ADR-002: Declarative collect/analyze/assemble pipeline with AllocMap coherence

**Status:** Accepted
**Date:** 2026-02-21

## Context

The original `printer.rs` pipeline had three separate phases: `mk_item`, `collect_unevaluated_constant_items`, and `collect_interned_values`. Each phase had full access to `TyCtxt` and was free to call `inst.body()` or any other side-effecting rustc query whenever it felt like it. Nothing in the types prevented that.

This caused a real bug (#120): one phase called `inst.body()` a second time, rustc minted fresh AllocIds (because that's what rustc does), and suddenly the alloc map had ids that didn't correspond to anything in the stored bodies. The downstream effect was that KMIR's proof engine couldn't decode allocations properly; they came out as opaque thunks instead of concrete values.

The fix in #120 was correct (carry collected items forward instead of re-fetching), but it left the underlying structure intact. The question was: how do we make that class of bug structurally impossible rather than just fixed for the one case we caught?

## Decision

Split the pipeline into phases whose type signatures enforce the boundary:

```rust
collect_and_analyze_items(HashMap<String, Item>)
  -> (CollectedCrate, DerivedInfo)

assemble_smir(CollectedCrate, DerivedInfo) -> SmirJson
```

`CollectedCrate` holds items and unevaluated consts (the output of talking to rustc). `DerivedInfo` holds calls, allocs, types, and spans (the output of walking bodies). `assemble_smir` takes both by value and does pure data transformation; it structurally cannot call `inst.body()` because it has no `MonoItem` or `Instance` to call it on. If you can't reach the query, you can't accidentally call it.

The two body-walking visitors (`InternedValueCollector` and `UnevaluatedConstCollector`) are merged into a single `BodyAnalyzer` that walks each body exactly once. The fixpoint loop for transitive unevaluated constant discovery is integrated: when `BodyAnalyzer` finds an unevaluated const, it records it; the outer loop creates the new `Item` (the one place `inst.body()` is called) and enqueues it.

### AllocMap coherence verification

The existing integration tests normalize away `alloc_id`s (via the jq filter), so they literally cannot catch this class of bug. The golden files don't contain alloc ids at all; you could scramble every id and the tests would still pass.

`AllocMap` replaces the bare `HashMap<AllocId, ...>` with a newtype that, under `#[cfg(debug_assertions)]`, tracks every insertion and flags duplicates. After the collect/analyze phase completes, `verify_coherence` walks every stored `Item` body with an `AllocIdCollector` visitor and asserts that each referenced `AllocId` exists in the map. This catches both "walked a stale body" (missing ids) and "walked the same body twice" (duplicate insertions) at dev time; zero cost in release builds.

## Consequences

**What's enforced by types:**
- `assemble_smir` cannot call `inst.body()` because it receives `CollectedCrate` and `DerivedInfo`, neither of which contains `Instance` or `MonoItem` handles
- Each body is walked exactly once in `collect_and_analyze_items`; the single `BodyAnalyzer` pass replaces two separate visitor passes

**What's enforced at dev-time (debug builds only):**
- Duplicate `AllocId` insertions are flagged (indicates a body was walked more than once)
- Missing `AllocId`s in the map (referenced in stored bodies but not collected) are flagged (indicates the analysis walked a different body than what's stored)

**What got deleted:**
`InternedValueCollector`, `UnevaluatedConstCollector`, `collect_interned_values`, `collect_unevaluated_constant_items`, the `InternedValues` type alias, and `items_clone`. The `items_clone` is particularly worth noting: it was a full `HashMap` clone that existed only so the static fixup pass could check "was this item in the original collection?" That's now a `HashSet<String>` of original item names.

**Other things that fell out of this:**
- Static items now store their body in `MonoItemKind::MonoItemStatic` (collected once in `mk_item`), so the analysis phase never goes back to rustc for static bodies
- `get_item_details` takes the pre-collected body as a parameter instead of calling `inst.body()` independently
- The `Item` type gains `body_and_locals()` and `warn_missing_body()` helpers that centralize the body-access pattern

**Downstream impact:**
The tighter allocs representation has already shown positive effects in KMIR: the proof engine can now decode allocations inline (resolving to concrete values like `StringVal("123")`) instead of deferring them as opaque thunks.
