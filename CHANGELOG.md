# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Note: this changelog was introduced at 0.2.0. The 0.1.0 section is a
retroactive best-effort summary; earlier changes were not formally tracked.

## [Unreleased]

### Added
- `src/compat/` module isolating all rustc internal API usage behind a stable boundary; printer/ now has zero `extern crate rustc_*` declarations and zero direct `tcx.query()` calls
- `OpaqueInstanceKind` owned type replacing `middle::ty::InstanceKind<'tcx>`, eliminating the `'tcx` lifetime parameter from `SmirJson`, `LinkMapKey`, `FnSymInfo`, `LinkMap`, `DerivedInfo`, and `SmirJsonDebugInfo`
- `ensure_rustc_commit.sh` helper that derives the rustc commit from `rustc -vV` and ensures the rust checkout (regular or bare+worktree) is at that commit
- ADR-003 documenting compat layer design decisions and validation results from two toolchain bump stress tests (6-month and 13-month jumps)

### Changed
- Routed `mk_graph/` stable_mir imports through the compat module
- Eliminated thin compat wrappers in printer/ (`mono_collect`, `mono_item_name`, `has_attr`, `def_id_to_inst`, `GenericData` newtype, `SourceData` alias); callers now go through the compat boundary directly
- UI test scripts (`run_ui_tests.sh`, `remake_ui_tests.sh`) now source `ensure_rustc_commit.sh` and use `RUST_SRC_DIR` instead of using the raw directory argument directly

## [0.2.0] - 2026-02-21

### Added
- D2 and Mermaid graph renderers alongside existing DOT, with index-first architecture for richer allocation/type lookup in rendered output ([#111])
- `--d2` flag for D2 diagram output
- `MachineInfo` in JSON output: pointer width, endianness ([#80])
- `Span` information in JSON output ([#74])
- Type visitor collecting full type metadata: ADT fields, discriminants, layouts ([#68], [#69], [#82], [#93], [#94])
- `TyKind` in `GlobalAlloc` entries ([#84])
- Extract discriminant information for enums ([#62])
- ADT def and field type metadata for structs and enums ([#64], [#82], [#105])
- UI test infrastructure ([#66])
- `cargo_stable_mir_json` helper binary for cargo integration ([#47])
- Nix derivation for reproducible builds ([#96])
- macOS support ([#97])
- Mutability field on `PtrType` and `RefType` in `TypeMetadata`, needed to distinguish `PtrToPtr` casts that change mutability from those that change pointee type ([#127])
- ADR-001: index-first graph architecture for MIR visualization ([#124])

### Changed
- Restructured `printer.rs` into a declarative 3-phase pipeline: `collect_items` -> `collect_and_analyze_items` -> `assemble_smir`, with `CollectedCrate`/`DerivedInfo` interface types enforcing the boundary between collection and assembly ([#121])
- Added `AllocMap` with debug-mode coherence checking (`verify_coherence`): asserts that every `AllocId` in stored bodies exists in the alloc map and that no body is walked twice; zero cost in release builds ([#121])
- Removed dead static-item fixup from `assemble_smir` (violated the phase boundary, misclassified statics as functions; never triggered in integration tests) ([#121])
- Rewrote `run_ui_tests.sh`: flag-based CLI (`--verbose`, `--save-generated-output`, `--save-debug-output`), build-once-then-invoke (eliminates per-test cargo overhead), arch-aware skip logic for cross-platform test lists ([#126])
- UI test runners now extract and pass `//@ compile-flags:`, `//@ edition:`, and `//@ rustc-env:` directives from test files (previously silently ignored) ([#126])
- Switched from compiler build to `rustup`-managed toolchain ([#33])
- Removed forked rust dependency ([#19])

### Fixed
- Fixed `get_prov_ty` to recursively walk nested struct/tuple fields when resolving provenance types; previously used exact byte-offset matching which panicked on pointers inside nested structs (e.g., `&str` at offset 56 inside `TestDesc` inside `TestDescAndFn`) ([#126])
- Removed incorrect `builtin_deref` assertions from VTable and Static allocation collection that rejected valid non-pointer types (raw `*const ()` vtable pointers, non-pointer statics) ([#126])
- Replaced panicking `unwrap`/`assert` calls in `get_prov_ty` with graceful fallbacks for layout failures, non-rigid types, and unexpected offsets ([#126])
- Fixed early `return` in `BodyAnalyzer::visit_terminator` that skipped `super_terminator()`, causing alloc/type/span collection to miss everything inside `Call` terminators with non-`ZeroSized` function operands (const-evaluated function pointers); bug present since [`aff2dd0`](https://github.com/runtimeverification/stable-mir-json/commit/aff2dd0) ([#126])
- Avoided duplicate `inst.body()` calls that were reallocating `AllocId`s ([#120])
- Prevented svg/png generation when `dot` is unavailable ([#117])
- Removed unreachable early return in D2 legend rendering ([#118])
- Included ZeroSized FnDef consts in functions map ([#112])
- Support `GlobalAlloc::Function` with non-fn-ptr type ([#102])
- Emitted correct `Alloc` ty for each allocation ([#100])
- Normalized field types before emitting them ([#95])
- Fixed monomorphisation bug for FnDef and ClosureDef ([#53])

[#19]: https://github.com/runtimeverification/stable-mir-json/pull/19
[#33]: https://github.com/runtimeverification/stable-mir-json/pull/33
[#47]: https://github.com/runtimeverification/stable-mir-json/pull/47
[#53]: https://github.com/runtimeverification/stable-mir-json/pull/53
[#62]: https://github.com/runtimeverification/stable-mir-json/pull/62
[#64]: https://github.com/runtimeverification/stable-mir-json/pull/64
[#66]: https://github.com/runtimeverification/stable-mir-json/pull/66
[#68]: https://github.com/runtimeverification/stable-mir-json/pull/68
[#69]: https://github.com/runtimeverification/stable-mir-json/pull/69
[#74]: https://github.com/runtimeverification/stable-mir-json/pull/74
[#80]: https://github.com/runtimeverification/stable-mir-json/pull/80
[#82]: https://github.com/runtimeverification/stable-mir-json/pull/82
[#84]: https://github.com/runtimeverification/stable-mir-json/pull/84
[#93]: https://github.com/runtimeverification/stable-mir-json/pull/93
[#94]: https://github.com/runtimeverification/stable-mir-json/pull/94
[#95]: https://github.com/runtimeverification/stable-mir-json/pull/95
[#96]: https://github.com/runtimeverification/stable-mir-json/pull/96
[#97]: https://github.com/runtimeverification/stable-mir-json/pull/97
[#100]: https://github.com/runtimeverification/stable-mir-json/pull/100
[#102]: https://github.com/runtimeverification/stable-mir-json/pull/102
[#105]: https://github.com/runtimeverification/stable-mir-json/pull/105
[#111]: https://github.com/runtimeverification/stable-mir-json/pull/111
[#112]: https://github.com/runtimeverification/stable-mir-json/pull/112
[#117]: https://github.com/runtimeverification/stable-mir-json/pull/117
[#118]: https://github.com/runtimeverification/stable-mir-json/pull/118
[#120]: https://github.com/runtimeverification/stable-mir-json/pull/120
[#121]: https://github.com/runtimeverification/stable-mir-json/pull/121
[#124]: https://github.com/runtimeverification/stable-mir-json/pull/124
[#126]: https://github.com/runtimeverification/stable-mir-json/pull/126
[#127]: https://github.com/runtimeverification/stable-mir-json/pull/127

## [0.1.0] - 2024-11-29

Initial release.

### Added
- Compiler driver hooking into rustc's `after_analysis` phase
- JSON serialization of Stable MIR: monomorphized items, allocations, interned values
- `--json` flag (default) for JSON output, `--dot` flag for graphviz DOT output
- Function link map tracking monomorphized function symbols
- Integration test harness with `.smir.json.expected` golden files and jq normalization
