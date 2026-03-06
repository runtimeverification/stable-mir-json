# ADR-004: Generative JSON Schema for smir.json output

**Author:** cds-amal
**Status:** Proposed
**Date:** 2026-03-06

## Context

The `*.smir.json` output is, for all practical purposes, the contract between stable-mir-json and the outside world. KMIR consumes it; other tools may follow. But right now, that contract is entirely implicit: it's "whatever the `Serialize` impls happen to produce." There's no spec a consumer can validate against, no way to detect that a field disappeared until your parser falls over, and no systematic way to answer the question "what changed between this version and the last one?"

To understand why this is harder than it sounds, it helps to look at where the JSON shape actually comes from. There are two layers:

1. **Our types.** These live in `src/printer/schema.rs` and `src/printer/items.rs`: `SmirJson`, `Item`, `MonoItemKind`, `TypeMetadata`, `AllocInfo`, `LinkMapKey`, `FnSymType`, `ItemSource`, `SourceData`, plus the debug-only types (`SmirJsonDebugInfo`, `ItemDetails`, `BodyDetails`, `GenericData`, `ForeignModule`, `ForeignItem`). We own these; we control how they serialize; we can put `#[derive(Whatever)]` on them.

2. **Compiler types.** These come from `stable_mir` (routed through `src/compat/`): `Body`, `GlobalAlloc`, `LayoutShape`, `TyKind`, `Ty`, `AdtDef`, `Allocation`, `MachineInfo`, `InstanceKind`, `Mutability`, `TyConst`, `RigidTy`, `ForeignItemKind`, and others. Their serialization is defined by derives inside the compiler itself. We don't control these shapes, and they can change on any nightly bump.

The second layer is the tricky part. ADR-003's compat layer absorbs API-level breakage (function signatures changing, crates getting renamed), but *serialization* shape is a different axis entirely. A compiler type can keep the same Rust API while its `Serialize` impl gains a field, drops a variant, or restructures an enum representation. That change flows straight through to JSON output, and downstream consumers discover it by breaking.

A natural question: will this get worse? Probably, yes. If stable-mir-json eventually targets multiple rustc versions (different projects pinned to different nightlies), downstream consumers will need to say "I understand the schema for toolchain X" and get a concrete spec for exactly that version.

## Decision

### Approach: schemars with manual impls for compiler types

We looked at four options:

| Approach | Verdict | Reason |
|----------|---------|--------|
| `schemars` derive + manual impls | **Chosen** | Derives on our types; manual `JsonSchema` impls for compiler types; produces JSON Schema directly |
| `serde_reflection` | Rejected | Requires runtime tracing to infer the schema; no JSON Schema output (it has its own format); unclear how it interacts with vendored serde |
| Schema inference from JSON output | Rejected as primary | Useful as a cross-check (more on this below), but inference from examples fundamentally can't distinguish "this field is optional" from "this field just wasn't present in my test cases" |
| Custom schema generator | Rejected | A lot of work to build something worse than schemars |

**Why schemars works here.** `schemars` reads `#[serde(...)]` attributes at compile time to produce JSON Schema; it doesn't link against serde at runtime. This matters because stable-mir-json uses rustc's vendored serde (to avoid version conflicts with compiler internals), and schemars is compatible with that setup. For our own types, slapping `#[derive(JsonSchema)]` next to the existing `#[derive(Serialize)]` just works.

For compiler-internal types, things are less automatic. We write manual `impl JsonSchema` blocks for roughly 10 types (`Body`, `GlobalAlloc`, `LayoutShape`, `TyKind`, `Ty`, `AdtDef`, `Allocation`, `MachineInfo`, `InstanceKind`, `RigidTy`). These impls describe the serialization shape as we observe it for the current toolchain. Some of them (particularly `TyKind` and `RigidTy`) have a lot of variants, so "manual" is doing real work here; this isn't trivial. But when the toolchain bumps and a compiler type's serialization changes, the manual impl must be updated, and that's the point: we *want* that change to be visible and deliberate, not silent.

### Schema versioning: one schema per (toolchain, commit) pair

Here's the key design principle, and it's worth stating precisely: **the schema is a function of two inputs: the rustc nightly version and stable-mir-json's own code at a given commit.** The rustc version determines how compiler-internal types serialize (layer 2); stable-mir-json's own evolution determines the shape of custom types like `SmirJson`, `TypeMetadata`, and `MonoItemKind` (layer 1). Either axis can change the schema independently. A toolchain bump can alter compiler type serialization without touching a single line of our code; a stable-mir-json PR can restructure custom types without bumping the toolchain.

Neither axis is something we can abstract away, and we shouldn't pretend otherwise. The schema captures a snapshot of both inputs. It's tagged with provenance via JSON Schema `x-` extension properties:

- `x-toolchain`: the nightly version from `rust-toolchain.toml` (e.g., `nightly-2024-11-29`)
- `x-git-ref`: the git commit or tag of stable-mir-json that produced the schema

The schema gets committed to the repository as a golden file (e.g., `schema/smir.schema.json`). CI regenerates it and diffs against the committed version; a mismatch fails the build. So:

- Toolchain bump PRs include the schema diff, making serialization changes visible in review
- Any code change that alters the output shape shows up as a schema diff
- Downstream consumers can pin to a specific schema version

For multi-rustc support (future work, out of scope for the initial implementation): the `schema/` directory grows a file per supported toolchain, e.g., `schema/nightly-2024-11-29.schema.json`. The `x-toolchain` tag is what downstream consumers use to pick the right schema.

### Understanding changes between versions: diffing, not abstraction

So, when a toolchain bump changes the schema, how do downstream consumers figure out what changed and adapt? The answer is deliberately simple: diff the schemas.

A JSON Schema diff between the old and new `smir.schema.json` tells you exactly which types gained or lost fields, which enum variants changed, which structural shapes shifted. This is the same information that would otherwise surface as "my parser broke at 2am"; the schema just makes it legible *before* anything breaks.

A natural question: how far should stable-mir-json go in supporting this? We stop short of building migration tooling, and that's a conscious choice:

- **What we provide:** the schema files themselves, committed to the repo, one per toolchain version. A `git diff` (or `diff` between two schema files) is the primary tool. For structured diffing, standard JSON Schema diff tools (e.g., `json-schema-diff`, `openapi-diff` repurposed for JSON Schema) work out of the box.
- **What we don't provide:** automatic migration scripts, compatibility shims that translate old-format JSON to new-format, or semver-style "this is a breaking change" annotations. The schema changes are driven by compiler internals; we can make them visible, but we can't meaningfully promise backward compatibility on types we don't own.
- **What downstream consumers should do:** treat the schema as a per-version contract. When upgrading the toolchain (or pulling a new stable-mir-json commit), diff the schemas, identify changes relevant to your consumer, and update your parser. The schema diff is a checklist, not an obstacle.

Why not go further? Building migration tooling would require understanding the *semantic intent* behind every compiler type change. Is a new `TyKind` variant something KMIR needs to handle, or is it irrelevant? That's domain knowledge that belongs in the consumer, not in the schema generator. We'd be building an abstraction layer over something we don't understand well enough to abstract.

### Inference-based validation as a cross-check

One thing the "schema inference from JSON output" approach *is* good for: catching drift in the manual `JsonSchema` impls. Generate a schema from actual test suite output (using something like `genson` or a custom jq-based inferrer), then check that it's a subset of the schemars-generated schema. If the inferred schema has fields the generative schema doesn't know about, a manual impl has gone stale. This is a CI enhancement, not a replacement for the primary generative approach.

## Consequences

**What this enables:**

- Downstream consumers get an explicit, machine-readable contract for the JSON output
- Breaking changes to the output shape are visible in PR diffs as schema changes
- Toolchain bumps that silently alter compiler type serialization now produce a concrete, reviewable diff
- Future multi-rustc support has a natural versioning mechanism

**What this costs:**

- One-time effort to write manual `JsonSchema` impls for compiler-internal types. Roughly 10 types, though some (particularly `TyKind` and `RigidTy`) have many variants and will require careful enumeration. This isn't a weekend task.
- On each toolchain bump, the manual impls need updating if compiler type serialization changed. But this is exactly the cost we're trying to make visible; previously it was invisible breakage for downstream consumers, which is strictly worse.
- `schemars` becomes a build dependency (though only for schema generation, not for the main compiler driver path).

**What this does not cover:**

- Semantic versioning of the schema (semver-style "is this a breaking change?"). The schema diff makes changes visible; interpreting whether a given change is breaking is left to human review.
- Schema evolution guarantees (e.g., "we will only add fields, never remove them"). The output shape is driven by compiler internals we don't control; making compatibility promises beyond "here is the schema for this (toolchain, commit) pair" would be dishonest.
- Migration tooling or compatibility shims between schema versions. The schema is a diagnostic tool ("what changed?"), not an abstraction layer ("pretend nothing changed").
