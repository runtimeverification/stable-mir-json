# ADR-001: Compatibility layer for rustc internal APIs

**Status:** Accepted
**Date:** 2025-02-21

## Context

stable-mir-json hooks into rustc's internal APIs (`rustc_middle`, `rustc_smir`, `rustc_span`, etc.) to extract MIR data. These APIs are unstable; they change regularly across nightly releases, and the crate names themselves get renamed (the `stable_mir` crate became `rustc_public`, `rustc_smir` became `rustc_public_bridge`, etc.). Before this decision, rustc internals were used directly throughout the codebase: `printer.rs`, `mk_graph/`, `driver.rs`, and various helpers all had their own `extern crate` declarations and direct imports. So a toolchain bump meant hunting through every file that touched a changed API; not fun, and easy to miss things.

## Decision

Route all rustc internal API usage through a single `src/compat/` module. The module re-exports crate names (so a rename like `stable_mir` to `rustc_public` is a one-line alias change in `compat/mod.rs`) and wraps unstable functions behind stable signatures (so a changed calling convention is absorbed in one place).

The compat layer does *not* try to abstract over stable MIR's own public API. When `stable_mir` (the public, downstream-facing API) changes its types, any consumer has to adapt; that's by design. The boundary is: if it's a rustc implementation detail, it goes through compat; if it's the stable MIR contract, it flows through directly.

`src/driver.rs` is the one exception; it uses `rustc_driver` and `rustc_interface` directly because it *is* the rustc integration point. Everything else goes through compat.

## Consequences

**What the compat layer absorbs (rustc internals):**

| Change | Absorbed in |
|--------|-------------|
| `collect_and_partition_mono_items` API changes | `compat/mono_collect.rs` |
| `RunCompiler::new().run()` becoming `run_compiler()` | `driver.rs` |
| `stable_mir` renamed to `rustc_public` | `compat/mod.rs` (re-exported as alias) |
| `rustc_smir` renamed to `rustc_public_bridge` | `compat/mod.rs`, `driver.rs` |
| `IndexedVal` trait moving between crates | `compat/mod.rs` (re-exported) |
| `FileNameDisplayPreference` variants changing | `compat/spans.rs` |

None of these changes leaked into `printer.rs` or `mk_graph/`. The abstraction worked as designed.

**What still propagates (stable MIR public API evolution):**

- `Rvalue::AddressOf` changed from `Mutability` to `RawPtrKind`
- `StatementKind::Deinit` and `Rvalue::NullaryOp` removed
- `AggregateKind::CoroutineClosure` added
- `Coroutine` and `Dynamic` field count changes
- `Ty::visit()` return type changed from `()` to `ControlFlow<T>`

These affect `printer.rs` and `mk_graph/` regardless of the compat layer. Any consumer of stable MIR would need to handle them; there's nothing we can (or should) do about that.

**The mk_graph gap (now fixed).** Turns out the `mk_graph/` files originally declared their own `extern crate stable_mir`, bypassing the abstraction entirely. This was introduced in commit `e9395d9` (PR #111) before the compat layer existed; it wasn't an oversight so much as a timing issue. The 13-month toolchain bump exposed the cost: when `stable_mir` was renamed to `rustc_public`, all 5 mk_graph files needed updating, while `printer.rs` needed zero import changes because it already went through compat. Commit `307dcb8` closed this gap by routing all mk_graph imports through `use crate::compat::stable_mir`.

## Validation

We stress-tested the abstraction against two toolchain bumps to see if it actually holds up in practice:

- **6-month jump** (nightly-2024-11-29 to nightly-2025-06-01, rustc 1.85 to 1.89): all internal API changes contained in `compat/` and `driver.rs`
- **13-month jump** (nightly-2024-11-29 to nightly-2026-01-15, rustc 1.85 to 1.94): same containment, plus the major `stable_mir` to `rustc_public` crate rename absorbed by a single alias in `compat/mod.rs`

Both branches compile and are available for reference: `spike/toolchain-2025-06` and `spike/toolchain-2026-01`, each with a detailed `rustc-<version>.md` breakdown.
