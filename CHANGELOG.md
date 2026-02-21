# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Note: this changelog was introduced at 0.2.0. The 0.1.0 section is a
retroactive best-effort summary; earlier changes were not formally tracked.

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

### Changed
- Refactored serialization pipeline into declarative collect/analyze/assemble phases
- Switched from compiler build to `rustup`-managed toolchain ([#33])
- Removed forked rust dependency ([#19])

### Fixed
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

## [0.1.0] - 2024-11-29

Initial release.

### Added
- Compiler driver hooking into rustc's `after_analysis` phase
- JSON serialization of Stable MIR: monomorphized items, allocations, interned values
- `--json` flag (default) for JSON output, `--dot` flag for graphviz DOT output
- Function link map tracking monomorphized function symbols
- Integration test harness with `.smir.json.expected` golden files and jq normalization
