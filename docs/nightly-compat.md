# Nightly Compatibility Guide

This document covers how stable-mir-json compiles against multiple rustc nightly
toolchains from a single branch: the strategy, the machinery, and (perhaps most
importantly) the playbook for when upstream breaks something new.

## The problem, stated precisely

stable-mir-json depends on two layers of unstable API, and it's worth being
explicit about which is which, because the mitigation strategy differs:

1. **Rustc internals** (`rustc_middle`, `rustc_smir`, `rustc_span`, etc.):
   `#[rustc_private]` crates whose signatures, module paths, and even crate
   names change across nightlies. The `src/compat/` layer absorbs these entirely
   (see [ADR-003](adr/003-compat-layer-for-rustc-internals.md)). A function
   moves? Fix it in one compat submodule. Done.

2. **Stable MIR public API** (`stable_mir`): despite the reassuring name, this
   API evolves too. New enum variants appear, old ones vanish, trait visibility
   changes, function signatures shift. The trouble is that these changes affect
   exhaustive `match` expressions scattered across `src/mk_graph/` and
   `src/printer/`. You can't centralize a match arm behind a wrapper without
   re-inventing the enum; the consumers have to adapt in place.

So the compat layer handles category 1, but category 2 needs a different trick.

## Strategy: build-time cfg detection

The trick turns out to be surprisingly simple. `build.rs` runs `rustc -vV`,
extracts the `commit-date` field, and compares it against a table of known API
breakpoints. For each breakpoint whose date is `<=` the detected commit-date, it
emits a `cargo:rustc-cfg` flag. ISO date strings sort lexicographically, so the
`>=` comparison just works without any date parsing:

```
         build.rs                            consumer code
    +-----------------+              +---------------------------+
    | rustc -vV       |              | #[cfg(smir_has_foo)]      |
    | commit-date:    |  -- emits -> | FooVariant => { ... }     |
    |   2025-03-01    |   cfg flags  |                           |
    | >= 2024-12-14?  |              | #[cfg(not(smir_has_foo))] |
    |   yes -> emit   |              | // variant doesn't exist  |
    +-----------------+              +---------------------------+
```

Every flag is also declared with `cargo:rustc-check-cfg`, which tells rustc
"this cfg name is legitimate even if it's not active right now." Without that,
you'd get `unexpected_cfgs` warnings on every nightly where the flag isn't set.

### Naming convention

| Pattern | Meaning | Example |
|---------|---------|---------|
| `smir_has_<thing>` | A type, variant, or function was **added** on this date | `smir_has_coroutine_closure` |
| `smir_no_<thing>` | A type, trait, or function was **removed** (or made private) | `smir_no_indexed_val` |

The `smir_` prefix scopes these to stable MIR public API changes. Rustc
internal changes don't get cfg flags; the compat layer eats those.

## Breakpoints matrix

This table is duplicated from `build.rs` for readability, but `build.rs` is
canonical. If they ever diverge, trust `build.rs`.

| Date | Cfg flag | What changed | Where the fix lives |
|------|----------|--------------|---------------------|
| 2024-12-14 | `smir_has_coroutine_closure` | `AggregateKind::CoroutineClosure` variant added | `mk_graph/util.rs` (conditional match arm) |
| 2025-01-24 | `smir_has_run_compiler_fn` | `RunCompiler` struct replaced by `run_compiler()` free fn | `driver.rs` (mutually exclusive blocks) |
| 2025-01-27 | `smir_has_named_mono_item_partitions` | `MonoItemPartitions` tuple became named-field struct | `compat/mono_collect.rs` (mutually exclusive blocks) |
| 2025-01-28 | `smir_has_raw_ptr_kind` | `Rvalue::AddressOf` first field: `Mutability` to `RawPtrKind` | `mk_graph/util.rs`, `mk_graph/context.rs` (mutually exclusive arms) |
| 2025-07-04 | `smir_no_indexed_val` | `IndexedVal` trait became `pub(crate)` | `compat/indexed_val.rs` (adapter module); all `mk_graph/` and `printer/` call sites use the shim |
| 2025-07-07 | `smir_rustc_internal_moved` | `rustc_internal::{internal,stable,run}` moved from `rustc_smir` to `stable_mir` | `compat/mod.rs` (cfg-gated re-export), `driver.rs` (cfg-gated import) |
| 2025-07-10 | `smir_has_global_alloc_typeid` | `GlobalAlloc::TypeId { ty }` variant added | `mk_graph/index.rs`, `printer/collect.rs`, `printer/mir_visitor.rs` (conditional match arms) |

### Supported range

```
  oldest tested                   pinned (CI)   newest tested
       v                              v              v
  2024-11-29  ------------------  2025-07-05  --  2025-07-14
       |                                              |
       +-- all 7 breakpoints covered ----------------+
```

The pinned nightly (the one CI actually runs) is whatever `rust-toolchain.toml`
says. Everything between the oldest tested nightly and the newest tested one
should compile; anything past the newest tested nightly is uncharted territory
and may have uncatalogued breakpoints.

Integration test golden files are stored per-nightly under
`tests/integration/expected/<nightly>/`. The Makefile auto-detects the active
nightly and selects the matching directory (falling back to the pinned nightly's
set). `make golden` writes into the detected nightly's directory, so adding
golden files for a new nightly is just
`RUSTUP_TOOLCHAIN=nightly-YYYY-MM-DD make golden`.

## Shim patterns

Three patterns keep recurring. Recognizing which one applies to a new upstream
change is most of the work; the implementation follows mechanically.

### Pattern 1: Conditional match arm

This is the common case when stable MIR adds (or removes) an enum variant.
The key insight: Rust evaluates `#[cfg]` *before* exhaustiveness checking. So
a gated arm is invisible to the compiler on nightlies where the variant doesn't
exist, and the match stays exhaustive on both sides:

```rust
match &aggregate_kind {
    Array(_) => "Array".to_string(),
    Tuple => "Tuple".to_string(),
    // ...other arms...

    // Added in nightlies >= 2024-12-14; see build.rs BREAKPOINTS table.
    #[cfg(smir_has_coroutine_closure)]
    CoroutineClosure(_, _) => "CoroutineClosure".to_string(),
}
```

**When to use:** enum variant additions or removals. Grep for every exhaustive
match on the affected enum; each one needs a gated arm.

### Pattern 2: Mutually exclusive code blocks

When a function signature or struct layout changes, you need two complete
implementations. Neither one is a subset of the other; they're just different:

```rust
#[cfg(not(smir_has_run_compiler_fn))]
let _ = rustc_driver::RunCompiler::new(args, &mut cb).run();

#[cfg(smir_has_run_compiler_fn)]
rustc_driver::run_compiler(args, &mut cb);
```

**When to use:** function renames, parameter changes, struct field renames.
Typically a 2-5 line block per side. Straightforward.

### Pattern 3: Adapter module

This is the heavy-duty pattern, and it's worth understanding why it exists
separately from pattern 2. When an entire trait (or set of methods) becomes
inaccessible, you can't just swap one call for another; you need a fundamentally
different mechanism to achieve the same result.

The `indexed_val.rs` shim is the canonical example. `IndexedVal` provided
`to_index()` and `to_val()` on opaque newtype wrappers like `Ty`, `Span`,
`AllocId`, and `VariantIdx`. When the trait became `pub(crate)`, those methods
vanished from external code. The adapter module provides free functions
`to_index(&val)` and `to_val::<T>(idx)` with two cfg-gated implementations:

- **Old nightlies:** delegate straight to the trait methods (the trait is still
  public, so this is a thin wrapper).
- **New nightlies:** the types are all `#[derive(Serialize)]` newtypes around
  `usize`, so `to_index` runs a minimal serde `Serializer` that intercepts the
  `serialize_newtype_struct` to `serialize_u64` chain to extract the inner
  value. `to_val` goes the other direction via `transmute_copy` with a
  compile-time size assertion (safe because a single-field newtype has the same
  layout as its field).

All call sites import the free functions instead of using trait methods:

```rust
// before (trait method; breaks on new nightlies):
let id = ty.to_index();

// after (adapter function; works everywhere):
use crate::compat::indexed_val::to_index;
let id = to_index(&ty);
```

**When to use:** trait visibility changes, removed inherent methods, or any
situation where the old and new implementations are structurally different enough
that they can't coexist as inline alternatives.

## Finding new breakpoints

A natural question: what happens when you bump the pinned nightly past the
current supported range? You'll hit compile errors. Here's the playbook.

### Step 1: Attempt the build

```shell
RUSTUP_TOOLCHAIN=nightly-YYYY-MM-DD cargo build 2>&1
```

The errors tell you what changed. Missing variant, unknown method, wrong field
count; the compiler is fairly specific.

### Step 2: Find the exact commit date

Use git pickaxe in a local `rust-lang/rust` checkout to find when the change
landed:

```shell
git log -S'CoroutineClosure' --format="%h author:%ad commit:%cd %s" --date=short -- compiler/
```

N.B.: the *author date* can be weeks or months before the *commit date*. A PR
might be authored in March and merged in July. The commit date determines when a
change appears in a nightly, so always check `commit:%cd` in the output. This
distinction has caused confusion before; it's worth internalizing.

To map a specific nightly date to a rustc commit, look up the manifest:

```
https://static.rust-lang.org/dist/YYYY-MM-DD/channel-rust-nightly.toml
```

The `[pkg.rust.git_commit_hash]` field gives the exact backing commit. Then
verify ancestry:

```shell
git merge-base --is-ancestor <suspect_commit> <nightly_commit> && echo "included"
```

### Step 3: Add the breakpoint

Add an entry to the `BREAKPOINTS` table in `build.rs` (keep it sorted by date):

```rust
Breakpoint {
    date: "YYYY-MM-DD",       // commit-date, not nightly date
    cfg: "smir_has_foo",       // or smir_no_foo for removals
    description: "What changed, briefly",
},
```

### Step 4: Apply the appropriate shim pattern

Refer to the three patterns above. Most changes are pattern 1 or pattern 2.
Pattern 3 is rare but you'll know it when you see it: the error isn't "missing
arm" or "wrong arguments," it's "trait `Foo` is private" or "no method named
`bar` found."

### Step 5: Verify both directions

This is the step that catches cfg-gating mistakes. Always build against at
least two nightlies: one where the flag is inactive and one where it's active.
If you only test one side, the other side's code may not even parse correctly,
and you won't find out until CI (or the next bump) breaks:

```shell
cargo build                                           # pinned nightly
RUSTUP_TOOLCHAIN=nightly-YYYY-MM-DD cargo build       # new nightly
```

## The compounding payoff

There's a dynamic here that's easy to miss if you're only looking at the current
state of the table: the upfront cost of cataloguing breakpoints is front-loaded,
but the ongoing cost drops dramatically once you catch up to the present.

Right now, the table covers roughly 8 months of upstream evolution (2024-11-29
through 2025-07-05). Building that catalogue meant triaging months of
accumulated changes after the fact: archaeological work, sifting through rustc
commit history to figure out when each break landed. That's the expensive part.

But once the table reflects the current nightly, future maintenance becomes
incremental. Each new upstream break surfaces as a single compile error the
moment you try a fresh nightly. You add one breakpoint entry, apply one shim
pattern, verify both directions, done. The triage cost drops from "hunt through
6 months of commits" to "diagnose one new error." The playbook above (steps 1
through 5) takes minutes, not hours.

The backward compatibility is arguably the more interesting benefit, though. The
entire supported range remains compilable indefinitely; old breakpoints don't
rot. So if you need to analyze Rust code that was compiled with a specific older
nightly (forensic analysis, reproducing a user's environment, validating MIR
output against a known compiler version), you can just set
`RUSTUP_TOOLCHAIN=nightly-YYYY-MM-DD` and build. No branch checkout, no
patching, no "which version of stable-mir-json do I need for this compiler?"
questions. The answer is always: this branch, this commit, any nightly in the
supported range.

That property gets more valuable over time as the range widens. Each new
breakpoint extends the upper bound without shrinking the lower bound.

## Trade-offs

Worth being honest about what this buys us and what it costs.

**Pros:**

- Single branch compiles against a range of nightlies. No per-nightly branches,
  no CI matrix.
- The supported range only grows: each new breakpoint extends the upper bound
  without shrinking the lower. Any nightly in the range works with any commit
  on the branch, which makes reproducing results against older compiler versions
  trivial (`RUSTUP_TOOLCHAIN=nightly-YYYY-MM-DD cargo build`; no branch
  archaeology needed).
- Maintenance cost is front-loaded: catching up to the present means triaging
  accumulated changes, but once the table is current, each new upstream break
  is a single compile error with a minutes-long fix cycle. The steady-state
  cost is low.
- The breakpoints table doubles as a readable changelog of upstream API
  evolution; useful context even if you never touch the cfg machinery.
- cfg flags are zero-cost at runtime: the compiler strips inactive code paths
  entirely.
- `cargo:rustc-check-cfg` means no spurious "unexpected cfg" warnings on any
  nightly in the supported range.
- New breakpoints are isolated: adding one never touches unrelated code.

**Cons:**

- The initial catch-up is real work. If the table has fallen behind by months,
  bumping to a current nightly may surface multiple unlisted breaks at once,
  and you have to triage them one by one. (The upstream changelog doesn't flag
  stable MIR API changes explicitly, so there's no shortcut.) This cost
  diminishes once you're keeping pace with upstream, but it's worth knowing
  about if the project goes dormant for a while.
- Date-based detection is coarse. Two breaking changes on the same commit-date
  either share one flag (fine if they always co-occur) or need separate flags
  (fine, but you have to know both changes exist).
- Conditional compilation reduces local readability. Your editor greys out the
  inactive path, which is helpful for knowing what's live but makes reviewing
  both paths harder. `make build-info` helps by showing which flags are active.
- The serde/transmute adapter pattern (`indexed_val.rs`) is admittedly clever in
  a way that warrants caution. It works because the affected types are
  `#[derive(Serialize)]` newtypes around `usize`. If upstream changes their
  representation, the transmute size assertion catches layout changes at
  compile time, but a semantic change (e.g., the inner value no longer being a
  plain index) would be silent. This is an acceptable risk for now; these types
  have been stable newtypes for years, and the serde path only activates when
  the trait-based path is unavailable.

## Quick reference

| Task | Command |
|------|---------|
| See which cfg flags are active | `make build-info` |
| Build against a different nightly | `RUSTUP_TOOLCHAIN=nightly-YYYY-MM-DD cargo build` |
| Run clippy lint checks | `make clippy` |
| Format code (Rust + Nix) | `make fmt` |
| Format + clippy combined | `make style-check` |
| Run integration tests | `make integration-test` |
| Run tests against a specific nightly | `RUSTUP_TOOLCHAIN=nightly-YYYY-MM-DD make integration-test` |
| Regenerate golden files (active nightly) | `make golden` |
| Generate golden files for a new nightly | `RUSTUP_TOOLCHAIN=nightly-YYYY-MM-DD make golden` |
| Check the pinned nightly | `cat rust-toolchain.toml` |
| Find the breakpoints table | `build.rs`, search for `BREAKPOINTS` |
| List available golden file sets | `ls tests/integration/expected/` |
