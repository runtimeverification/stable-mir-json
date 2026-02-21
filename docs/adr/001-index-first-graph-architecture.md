# ADR-001: Index-first graph architecture for MIR visualization

**Status:** Accepted
**Date:** 2026-01-13
**PR:** [#111](https://github.com/runtimeverification/stable-mir-json/pull/111) (addresses [#83](https://github.com/runtimeverification/stable-mir-json/issues/83))

## Context

The original `mk_graph.rs` was a single 300-line file that rendered MIR as Graphviz DOT. It traversed function bodies and emitted graph nodes inline, with no pre-processing of the data. So constants showed up as opaque labels like `const ?_i32` because the renderer had no way to look up what an `AllocId` or type actually contained. Adding richer labels (allocation provenance, type layouts, decoded string literals) would have meant threading lookup logic through every rendering path; not practical in a single-pass traversal.

The architecture was also tightly coupled to `dot_writer`, so adding a second output format would have meant duplicating most of the traversal logic.

## Decision

Restructure `mk_graph` into a module with an index-first architecture: build lookup indices upfront, then traverse bodies with full context available.

Since `SmirJson` already contains all the data (allocs, types, functions), it just needs to be indexed before traversal rather than looked up ad-hoc during rendering.

The module splits into:

```console
src/mk_graph
├── context.rs
├── index.rs
├── mod.rs
├── output          # renderers go here; one file per format
│   ├── d2.rs
│   ├── dot.rs
│   └── mod.rs
└── util.rs
```

- **`index.rs`**: `AllocIndex` and `TypeIndex`, built from the serialized `SmirJson` data. `AllocIndex` decodes `GlobalAlloc` entries into human-readable `AllocEntry` structs (distinguishing memory/static/vtable/function, decoding byte slices as ASCII strings or integers). `TypeIndex` processes `TypeMetadata` into `TypeEntry` with layout information, field details, and variant info for enums.

- **`context.rs`**: `GraphContext` holds both indices plus a function-type-to-name map. Provides rendering methods (`render_const`, `render_operand`, `render_stmt`, `render_type_with_layout`) that produce context-aware labels. So a constant with provenance now renders as `const [alloc0: Int(I32) = 42]` instead of `const ?_i32`.

- **`util.rs`**: Pure helper functions for string formatting, name shortening, and label construction. No state, no indices; just string manipulation.

- **`output/dot.rs`** and **`output/d2.rs`**: Format-specific renderers that consume `GraphContext`. Each format implements its own traversal but shares the same index-backed label rendering. Adding a new output format means writing a new file in `output/` without touching the indexing or label logic.

## Consequences

**What this enables:**
- Constants show provenance references with decoded values (strings as ASCII, integers as numeric values)
- ALLOCS and TYPES legend nodes give a global overview of the program's allocations and composite types
- Type labels include layout information (size, alignment, field offsets)
- Adding a new output format is isolated to `output/`; no changes needed in `context.rs` or `index.rs`. D2 (`output/d2.rs`) was added alongside DOT as a demonstration of this: same rendering fidelity, no duplicated traversal logic

**What this adds:**
- `AllocIndex`, `AllocEntry`, `AllocKind`: allocation lookup and decoding
- `TypeIndex`, `TypeEntry`, `TypeKind`, `LayoutInfo`, `FieldInfo`, `VariantInfo`: type lookup with layout
- `GraphContext` with rendering methods for constants, operands, statements, and types
- `output/d2.rs`: D2 format renderer
- Make targets: `make dot`, `make d2`, `make svg`, `make png`

**Trade-offs:**
- The indices duplicate some data from `SmirJson` into more accessible structures. This is intentional; the rendering code is much simpler when it can ask "describe this alloc id" rather than manually searching the alloc list. The graph data is small relative to the MIR itself, so the memory cost is negligible.
- `SmirJson` gains a few `pub` fields that were previously private, since `GraphContext::from_smir` needs read access. This slightly widens the internal API surface, but `SmirJson` is already a serialization boundary so the fields were effectively public anyway (they show up in JSON output).
