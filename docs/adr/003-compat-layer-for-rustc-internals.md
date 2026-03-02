# ADR-003: Compatibility layer for rustc internal APIs

**Status:** Accepted
**Date:** 2026-03-02

## Context

stable-mir-json hooks into rustc's internal APIs (`rustc_middle`, `rustc_smir`, `rustc_span`, etc.) to extract MIR data. These APIs are unstable; they change regularly across nightly releases, and the crate names themselves get renamed (the `stable_mir` crate became `rustc_public`, `rustc_smir` became `rustc_public_bridge`, etc.). Before this decision, rustc internals were used directly throughout the codebase: `printer.rs`, `mk_graph/`, `driver.rs`, and various helpers all had their own `extern crate` declarations and direct imports. So a toolchain bump meant hunting through every file that touched a changed API; not fun, and easy to miss things.

## Decision

Route all rustc internal API usage through a single `src/compat/` module. The module re-exports crate names (so a rename like `stable_mir` to `rustc_public` is a one-line alias change in `compat/mod.rs`) and wraps unstable functions behind stable signatures (so a changed calling convention is absorbed in one place).

The compat layer does *not* try to abstract over stable MIR's own public API. When `stable_mir` (the public, downstream-facing API) changes its types, any consumer has to adapt; that's by design. The boundary is: if it's a rustc implementation detail, it goes through compat; if it's the stable MIR contract, it flows through directly.

`src/driver.rs` is the one exception; it uses `rustc_driver` and `rustc_interface` directly because it *is* the rustc integration point. Everything else goes through compat.

## Consequences

**What the compat layer absorbs (rustc internals):**

The table below shows changes observed during validation (see the Validation section) and where each was contained. Note that `driver.rs` changes are listed here because they stay within the rustc integration boundary, even though `driver.rs` sits outside `compat/` itself.

| Change | Absorbed in |
|--------|-------------|
| `collect_and_partition_mono_items` tuple to `MonoItemPartitions` struct | `compat/mono_collect.rs` |
| `RunCompiler::new().run()` becoming `run_compiler()` | `driver.rs` |
| `stable_mir` renamed to `rustc_public` | `compat/mod.rs` (re-exported as alias) |
| `rustc_smir` renamed to `rustc_public_bridge` | `compat/mod.rs`, `driver.rs` |
| `FileNameDisplayPreference` variants changing | `compat/spans.rs` |

None of these changes leaked into `printer/` or `mk_graph/`. The abstraction worked as designed.

**What still propagates (stable MIR public API evolution):**

- `Rvalue::AddressOf` changed from `Mutability` to `RawPtrKind`
- `StatementKind::Deinit` and `Rvalue::NullaryOp` removed
- `AggregateKind::CoroutineClosure` added
- `Coroutine` and `Dynamic` field count changes
- `Ty::visit()` return type changed from `()` to `ControlFlow<T>`

These affect `printer/` and `mk_graph/` regardless of the compat layer. Any consumer of stable MIR would need to handle them; there's nothing we can (or should) do about that.

**The mk_graph gap (now fixed).** The `mk_graph/` files originally declared their own `extern crate stable_mir`, bypassing the abstraction entirely. This was introduced in commit `e9395d9` (PR #111) before the compat layer existed; it wasn't an oversight so much as a timing issue. The 13-month toolchain bump exposed the cost: when `stable_mir` was renamed to `rustc_public`, all 5 mk_graph files needed updating, while `printer/` needed zero import path changes because it already went through compat. This branch closes the gap by routing all mk_graph imports through `use crate::compat::stable_mir`.

## Validation

We stress-tested the abstraction against two toolchain bumps on ephemeral branches (branched off `spike/hex-rustc`, since deleted) to see if it actually holds up in practice:

- **6-month jump** (nightly-2024-11-29 to nightly-2025-06-01, rustc 1.85 to 1.89): all internal API changes contained in `compat/` and `driver.rs`
- **13-month jump** (nightly-2024-11-29 to nightly-2026-01-15, rustc 1.85 to 1.94): same containment, plus the major `stable_mir` to `rustc_public` crate rename absorbed by a single alias in `compat/mod.rs`

The validation branches were disposable spike work and have been removed. Detailed findings are recorded in the PR description for this branch.
