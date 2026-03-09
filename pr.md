## The Curious Case of stdlib

Can stable-mir-json emit `smir.json` for Rust's standard library? Yes. But getting there required catching a rustc bug, simplifying the toolchain management story, stress-testing the compat layer against a 15-month nightly jump, and then building the infrastructure to make multi-nightly support sustainable rather than a one-off heroic effort.

This PR breaks down into several largely independent pieces: a rustc panic workaround, a `make stdlib-smir` target, single-source-of-truth toolchain management, a `build.rs`-based cfg detection system for multi-nightly support, a base+delta UI test list system, per-nightly integration test golden files, and the compat layer work needed to push the supported range from a single nightly all the way to an 8-month window. The nightly pin moves from `2024-11-29` to `2025-07-11`, with verified support through `2025-07-14`.


### 1. The rustc layout panic

`ty.layout()` returns `Result<Layout, Error>`; a deliberate API contract that says "layout computation can fail, and we'll tell you about it via Err." We were already handling that correctly with `.ok()`. Turns out that's not enough: rustc's implementation panics before it ever gets a chance to construct the Err.

The call chain (against nightly-2024-11-29, rustc commit a2545fd6fc):

```console
layout_of_uncached
  needs: is this field type Sized?
  calls: type_known_to_meet_bound_modulo_regions(ty, Sized)
    constructs: TraitRef::new(tcx, sized_def_id, [ty])
    calls: UpcastFrom<TraitRef> for Predicate
      calls: ty::Binder::dummy(from)
        asserts: !value.has_escaping_bound_vars()   <-- PANIC
```

The assert in `Binder::dummy` is legitimate: wrapping a value with escaping bound vars in a dummy binder would be semantically wrong (it would capture vars that belong to an outer binder). The bug is upstream: `type_known_to_meet_bound_modulo_regions` blindly constructs a TraitRef for `<ty as Sized>` without checking whether ty has escaping bound vars. When `layout_of_uncached` asks "is this field Sized?" for `dyn fmt::Write` (still under a lifetime binder from `Formatter<'a>`), it feeds a type with escaping bound vars into a code path that assumes they've already been substituted away.

The workaround has three parts:

1. `try_layout_shape` wraps `ty.layout()` in `catch_unwind` with
   `AssertUnwindSafe`, returning `Result<Option<LayoutShape>, String>`.
   On panic, the payload is downcast to extract the message.

2. The default panic hook is swapped out (`take_hook`/`set_hook`) for the
   duration of the `catch_unwind` call, suppressing the noisy backtrace
   that rustc's hook would otherwise print to stderr. The previous
   hook is restored immediately after.

3. `TyCollector` gains a `layout_panics: Vec<LayoutPanic>` field.
   `layout_shape_or_record` delegates to `try_layout_shape` and pushes
   any `Err` into the vec; the type is still recorded with `layout: None`.
   At the end of `collect_and_analyze_items`, accumulated panics are
   reported as a single summary warning with the type and message.

Against nightly-2024-11-29, one layout panic is observed:

```
warning: 1 type layout(s) could not be computed (rustc panicked):
  type Ty { id: 20228, kind: RigidTy(Adt(AdtDef(DefId { id: 5070,
  name: "core::fmt::Formatter" }), ...)) }:
  `<dyn core::fmt::Write as core::marker::Sized>` has escaping
  bound vars, so it cannot be wrapped in a dummy binder.
```


### 2. `make stdlib-smir`

A single command that builds stable-mir-json, creates a throwaway crate in a temp directory, runs `-Zbuild-std` through the driver, and collects the resulting artifacts into `tests/stdlib-artifacts/`. Against nightly-2024-11-29 this produces 21 artifacts (~100MB total), including `std` (44MB, 11,950 items), `core` (16MB), `alloc`, `proc_macro`, and their transitive dependencies.

The target points `RUSTC` directly at `target/debug/stable_mir_json` with the library path set inline via `rustc --print sysroot`; no wrapper script or `~/.stable-mir-json/` install step needed.


### 3. Single source of truth for toolchain version

The `[metadata] rustc-commit` field in `rust-toolchain.toml` was a manual cache of the backing commit. If you bumped the nightly and forgot to update it, UI tests would silently run against the wrong compiler source. It's been removed; everything is now derived from `rust-toolchain.toml`'s `channel` field:

| Derived value | How |
|---------------|-----|
| Compiler binary + rustc-dev libs | rustup resolves `channel` |
| stdlib source for `-Zbuild-std` | `rust-src` component bundled with toolchain |
| Library path for wrapper scripts | `rustc --print sysroot` |
| rustc commit for UI test checkout | `rustc -vV \| grep commit-hash` |

CI (`test.yml`) now reads the channel via `yq` from `rust-toolchain.toml` in all three jobs, replacing hardcoded nightly versions. `cargo_stable_mir_json.rs` derives the library path dynamically via `rustc --print sysroot` instead of hardcoding a toolchain path. `ensure_rustc_commit.sh` fetches the commit if needed before creating worktrees (handles bare repos).


### 4. Multi-nightly support via build.rs cfg detection

This is the centrepiece. A single branch that compiles against a range of nightlies, without maintaining per-nightly forks or feature-flag spaghetti.

**The problem**: stable MIR's public API evolves across nightlies. New enum variants appear, old ones disappear, function signatures change. Each change breaks an exhaustive `match` somewhere. Before this PR, bumping the nightly was a manual editing session; you couldn't even keep the old nightly working while adding support for the new one.

**The mechanism**: `build.rs` runs `rustc -vV`, extracts the `commit-date`, and compares it against a table of known API breakpoints. For each breakpoint whose date is at or before the active compiler's commit-date, it emits a `cargo:rustc-cfg` flag. The rest of the crate gates code on these flags with `#[cfg(smir_has_*)]` / `#[cfg(not(smir_has_*))]`.

```
build.rs                          rustc -vV
  |                                  |
  |  BREAKPOINTS table               |  commit-date: 2025-07-10
  |  +--------------------------+    |
  |  | 2024-12-14  coroutine_cl |    |
  |  | 2025-01-24  run_compiler |    |  all seven dates <= 2025-07-10
  |  | 2025-01-27  named_mono   |    |  so all seven cfgs are emitted
  |  | 2025-01-28  raw_ptr_kind |    |
  |  | 2025-07-04  no_indexed   |    |
  |  | 2025-07-07  rustc_int_mv |    |
  |  | 2025-07-10  typeid_alloc |    |
  |  +--------------------------+    |
  |                                  |
  v                                  v
cargo:rustc-cfg=smir_has_coroutine_closure
cargo:rustc-cfg=smir_has_run_compiler_fn
cargo:rustc-cfg=smir_has_named_mono_item_partitions
cargo:rustc-cfg=smir_has_raw_ptr_kind
cargo:rustc-cfg=smir_no_indexed_val
cargo:rustc-cfg=smir_rustc_internal_moved
cargo:rustc-cfg=smir_has_global_alloc_typeid
```

Each breakpoint corresponds to a real API change discovered by binary-searching (overkill?) across nightlies:

| Date | cfg flag | What changed | Where gated |
|------|----------|-------------|-------------|
| 2024-12-14 | `smir_has_coroutine_closure` | `AggregateKind::CoroutineClosure` variant added | `mk_graph/util.rs` |
| 2025-01-24 | `smir_has_run_compiler_fn` | `RunCompiler` struct replaced by `run_compiler()` free fn | `driver.rs` |
| 2025-01-27 | `smir_has_named_mono_item_partitions` | `collect_and_partition_mono_items` return changed from tuple to named fields | `compat/mono_collect.rs` |
| 2025-01-28 | `smir_has_raw_ptr_kind` | `Rvalue::AddressOf` first field changed from `Mutability` to `RawPtrKind` | `mk_graph/util.rs`, `mk_graph/context.rs` |
| 2025-07-04 | `smir_no_indexed_val` | `IndexedVal` trait became `pub(crate)` | `compat/indexed_val.rs` (adapter module); all `mk_graph/` and `printer/` call sites use shim |
| 2025-07-07 | `smir_rustc_internal_moved` | `rustc_internal::{internal,stable,run}` moved from `rustc_smir` to `stable_mir` | `compat/mod.rs` (cfg-gated re-export), `driver.rs` (cfg-gated import) |
| 2025-07-10 | `smir_has_global_alloc_typeid` | `GlobalAlloc::TypeId { ty }` variant added | `mk_graph/index.rs`, `printer/collect.rs`, `printer/mir_visitor.rs` (conditional match arms) |

The cfg names are declared unconditionally via `cargo:rustc-check-cfg`, so rustc never warns about `unexpected_cfgs` regardless of which nightly is active. Diagnostics go through `eprintln!` (visible only with `cargo build -vv`); a `make build-info` target wraps this for quick inspection.

**Exhaustiveness is preserved on every supported nightly**: without the flag, the variant doesn't exist in the enum, so the gated arm is excluded and the match remains exhaustive. With the flag, the arm is included to cover the new variant. No `#[allow(unreachable_patterns)]` needed.

The last two breakpoints are worth calling out because they demonstrate the full range of shim patterns:

**`smir_no_indexed_val` (pattern 3: adapter module)**: `IndexedVal` provided `to_index()` and `to_val()` on opaque newtype wrappers like `Ty`, `Span`, `AllocId`, and `VariantIdx`. When the trait became `pub(crate)`, those methods vanished from external code. The adapter in `compat/indexed_val.rs` provides free functions with two cfg-gated implementations: old nightlies delegate to the trait methods directly; new nightlies use a minimal serde `Serializer` that intercepts the `serialize_newtype_struct` chain to extract the inner `usize` (and `transmute_copy` with a compile-time size assertion for the reverse direction). All call sites across `mk_graph/` and `printer/` import the free functions instead of using trait methods.

**`smir_rustc_internal_moved` (pattern 2: mutually exclusive blocks)**: entirely absorbed by the compat layer re-exports in `compat/mod.rs`; consumer code (`bridge.rs`, `mono_collect.rs`, `types.rs`) imports via `super::rustc_internal` and needed zero changes. `driver.rs` is the one exception (it imports rustc crates directly), so it gets its own cfg-gated import.


### 5. Per-nightly UI test lists via base+delta

The UI test suite runs ~2,900 of rustc's own tests through our driver. When we bump the nightly, upstream may have added, removed, renamed, or rewritten tests. Rather than maintaining full copies of the test lists per nightly (they're 99.5% identical), `diff_test_lists.sh` computes the delta:

```
base passing.tsv (2880 tests, against nightly-2024-11-29)
  |
  |  git diff <base-commit>..<target-commit> -- tests/ui/
  |    - 2 files deleted (dropped from list)
  |    - 0 files renamed
  |    - 47 files modified (content changed, but same path)
  |
  |  manual overrides (tests/ui/overrides/nightly-2025-03-01.tsv)
  |    - 1 test skipped (repetitions.rs: rewritten to use syntax we don't handle)
  |
  v
effective passing.tsv (2878 tests, for nightly-2025-03-01)
```

The script has three modes:

| Mode | Description |
|------|-------------|
| `--report` | Human-readable summary: deletions, renames, modifications, effective list size |
| `--chain` | Incremental diffs between consecutive breakpoint nightlies |
| `--emit` | Write effective `passing.tsv` and `failing.tsv` to `overrides/<nightly>/` |

`run_ui_tests.sh` auto-detects the active nightly via `rustup show active-toolchain` and uses the effective list from `overrides/<nightly>/` if it exists, falling back to the base list otherwise. No manual flag-passing needed.


### 6. Per-nightly integration test golden files

MIR output differs structurally across compiler versions: span indices shift, local variable ordering changes, lowering decisions evolve. Normalisation (via `normalise-filter.jq`) handles non-determinism within a single nightly, but cross-nightly differences are real semantic changes in the compiler's output, not noise to be papered over.

Golden files now live in per-nightly directories under `tests/integration/expected/<nightly>/`. The Makefile auto-detects the active nightly from `rustc -vV` (commit-date + 1 day) and selects the matching directory, falling back to the pinned nightly's set if no directory exists for the active one:

```
tests/integration/expected/
  nightly-2025-03-01/   (29 files)
  nightly-2025-07-05/   (29 files; all 29 differ from 2025-03-01)
  nightly-2025-07-08/   (29 files; only slice.smir.json.expected differs from 2025-07-05)
```

Adding golden files for a new nightly is just `RUSTUP_TOOLCHAIN=nightly-YYYY-MM-DD make golden`.


### 7. Integration test normalisation hardening

The nightly bump exposed cross-platform non-determinism in the integration test golden files that previously went unnoticed (we were only running on one nightly, on one platform). Three fixes to `normalise-filter.jq`:

1. **Field projection Ty IDs**: `{"Field": [field_idx, ty_id]}` embeds an
   interned Ty index that varies across platforms. The existing `walk`
   stripped `.ty` from objects but missed these array-encoded IDs. Now
   zeroed out.

2. **Item sort stability**: after hash truncation, two monomorphised
   `Debug::fmt` impls can share the same truncated `symbol_name`; bare
   `sort` couldn't break the tie deterministically. Now using
   `sort_by(symbol_name + "|" + name)`, mirroring the Rust-side `Ord`
   key structure.

3. **Interned `.id` fields**: `MonoItemFn.id` and `const_.id` are interned
   indices that vary across platforms. Now stripped alongside `.def_id` in
   the global walk pass.


### 8. `make fmt` and `make clippy` targets

The `style-check` target has been split into independently callable pieces: `make fmt` (alias for `make format`; formats Rust + Nix) and `make clippy` (runs `cargo clippy -- -Dwarnings`). `make style-check` delegates to both. This came out of a CI failure: a newer nightly's clippy enforces `uninlined_format_args` (e.g., `format!("{}", x)` must be `format!("{x}")`), and having a standalone `make clippy` makes it easy to catch and fix these locally.


### Nightly compatibility

The pinned nightly moves from `2024-11-29` to `2025-07-11`, with verified support through `2025-07-14`. With the cfg gates in place, everything in this range compiles from the same source:

```
  oldest tested                                              pinned (CI)   newest tested
       v                                                        v              v
  2024-11-29  ----------------------------------------------  2025-07-11  --  2025-07-14
       |                                                                       |
       |  coroutine  run_compiler  named_mono  raw_ptr  indexed  rustc  typeid |
       |  closure    fn            partitions  kind     val      int_mv alloc  |
       |     |          |             |          |        |        |      |    |
       v     v          v             v          v        v        v      v    v
  ----[------+----------+-------------+----------+--------+--------+------+---]-->
             12-14      01-24         01-27      01-28    07-04    07-07  07-10
                                 breakpoint dates
```

Beyond the supported range, nightly-2025-07-15 (commit-date 2025-07-14) is an **epoch boundary**: `stable_mir` was renamed to `rustc_public`, `rustc_smir` to `rustc_public_bridge`, and tuple variants across `Rvalue`, `TerminatorKind`, `StatementKind`, `AggregateKind`, and `TyKind` were converted to struct variants. This produces 96 errors and is too large for conditional compilation (every match arm would need a dual form). Migration to the `rustc_public` API is future work on a separate branch; the current branch is complete for the pre-`rustc_public` era.

The compat layer cleanly separates rustc internal changes (absorbed in `compat/`) from stable MIR public API evolution (requires cfg-gated code in `printer/` and `mk_graph/`). Extending the supported range within the pre-epoch window means adding new rows to the `BREAKPOINTS` table in `build.rs` and gating the corresponding match arms. A comprehensive nightly compatibility guide lives at `docs/nightly-compat.md`, covering the strategy, all three shim patterns with worked examples, the epoch boundary, and a step-by-step playbook for finding and fixing new breakpoints.


### Todo
- [ ] challenge the assumptions and approaches here! Let's discuss!

### Tests
- `make integration-test` passes (29/29) with per-nightly golden files for four nightlies
- `make test-ui` passes (2878/2878 against nightly-2025-03-01)
- `make stdlib-smir` produces 21 valid artifacts
- Backward compatibility verified: `RUSTUP_TOOLCHAIN=nightly-2024-11-29 cargo build` succeeds
- Forward compatibility verified: `RUSTUP_TOOLCHAIN=nightly-2025-07-14 cargo build` succeeds
