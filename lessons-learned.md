# Lessons Learned: Toolchain Bump Stress Test

Two experimental branches were created off `spike/hex-rustc` to test whether the `src/compat/` abstraction actually contains rustc internal API churn:

- `spike/toolchain-2025-06`: nightly-2024-11-29 → nightly-2025-06-01 (6 months, rustc 1.85 → 1.89)
- `spike/toolchain-2026-01`: nightly-2024-11-29 → nightly-2026-01-15 (13 months, rustc 1.85 → 1.94)

Each branch has a detailed `rustc-<version>.md` with the full diff-by-diff breakdown.

## What the compat layer caught

All rustc internal API changes were fully contained in `src/compat/` and `src/driver.rs`:

| Change | Where it was absorbed |
|--------|----------------------|
| `collect_and_partition_mono_items` tuple → `MonoItemPartitions` struct | `compat/mono_collect.rs` |
| `RunCompiler::new().run()` → `run_compiler()` | `driver.rs` |
| `after_analysis` lifetime annotation | `driver.rs` |
| `stable_mir` → `rustc_public` crate rename | `compat/mod.rs` (re-exported as alias) |
| `rustc_smir` → `rustc_public_bridge` crate rename | `compat/mod.rs`, `driver.rs` |
| `IndexedVal` moved to `rustc_public_bridge` | `compat/mod.rs` (re-exported) |
| `FileNameDisplayPreference::Remapped` removed | `compat/spans.rs` |

None of these changes leaked into `printer.rs` or `mk_graph/`. The abstraction worked as designed.

## What leaked out (and why it's fine)

Changes to `printer.rs` and `mk_graph/` were exclusively stable MIR API evolution: the public `stable_mir` (now `rustc_public`) crate changing its own types. The compat layer isn't designed to absorb these; any consumer of stable MIR would need to handle them. Examples:

- `Rvalue::AddressOf` changed from `Mutability` to `RawPtrKind` (gained `FakeForPtrMetadata` for pointer metadata extraction)
- `StatementKind::Deinit` and `Rvalue::NullaryOp` removed from stable MIR
- `AggregateKind::CoroutineClosure` added (async closures)
- `Coroutine` and `Dynamic` lost a field each (movability and `DynKind` respectively)
- `PointerCoercion::ReifyFnPointer` gained a `Safety` parameter
- `GlobalAlloc::TypeId` added
- `Ty::visit()` return type changed from `()` to `ControlFlow<T>`

## One abstraction gap: `mk_graph/` extern crate declarations

The `mk_graph/` files (`context.rs`, `index.rs`, `util.rs`, `output/d2.rs`, `output/dot.rs`) each declare their own `extern crate stable_mir`. This was introduced in commit `e9395d9` ("Pr/83 (#111)", authored by cds-amal) when the mk_graph module was created; the compat layer didn't exist yet at that point.

When the compat refactor landed in `b88325c`, it moved `mk_graph/mod.rs` to import `TyCtxt` and output path utilities through compat (those were rustc internals), but deliberately left the `extern crate stable_mir` declarations in the other mk_graph files alone. The reasoning was sound: those files only use the stable API, not rustc internals, so there was no pressing reason to route them through compat.

The 13-month bump (`spike/toolchain-2026-01`) exposed the cost of this decision. When `stable_mir` was renamed to `rustc_public`, all 5 mk_graph files needed `extern crate rustc_public as stable_mir` (the alias keeps all downstream `use stable_mir::` paths working). Meanwhile, `printer.rs` needed zero import path changes because it already goes through `use crate::compat::stable_mir`, and `compat/mod.rs` absorbed the rename with `pub use rustc_public as stable_mir`.

If the mk_graph files had imported `stable_mir` through compat instead of declaring their own extern crate, the rename would have been a single-file change (`compat/mod.rs`). That's 5 fewer files touched for a crate rename, which is exactly the kind of churn the compat layer was built to prevent.

### Recommendation

Route the mk_graph files through compat:

```rust
// instead of:
extern crate stable_mir;

// use:
use crate::compat::stable_mir;
```

This is a small change (5 files, one line each) that would close the gap. The `IndexedVal` import already goes through compat in the 13-month branch, so the pattern is established.
