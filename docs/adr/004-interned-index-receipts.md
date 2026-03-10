# ADR-004: Interned index receipts for golden-file normalization

**Status:** Proposed
**Date:** 2026-03-10

## Context

Integration test golden files compare serialized Stable MIR JSON across platforms
(macOS vs. Linux CI). Several values in the output are "interned indices":
compile-session-specific integers assigned by rustc's internal interning machinery
for types like `Ty`, `Span`, `AllocId`, `DefId`, and `AdtDef`. These indices are
consistent within a single rustc invocation but differ across platforms and across
runs; a `Ty` that interns as index 42 on macOS might be index 37 on Linux.

The normalise filter (`normalise-filter.jq`) strips or zeroes these indices before
comparison. The trouble is that the filter has to independently know the JSON
schema: every time Stable MIR adds a new field or array position that carries an
interned index, the filter needs a corresponding rule. We've been discovering
these gaps exclusively through CI failures on the other platform; the feedback
loop is slow and the pattern is pure whack-a-mole. In the span of three commits,
we hit three distinct categories of missed interned index (bare `span` fields,
`{"Type": N}` newtype wrappers, and positional indices inside `Cast[2]` and
`Closure[0]` arrays).

The root cause: the schema knowledge (which values are interned) lives in the
Rust type definitions, but the normalization rules live in a separate jq script
that has to reverse-engineer that knowledge from examples. There's no contract
between the producer (the printer) and the consumer (the normaliser) that says
"these are the interned paths."

## Decision

The printer emits a companion "receipts" file (`*.smir.receipts.json`) alongside
each `*.smir.json` output. The receipts file declares which JSON key names,
newtype wrappers, and array positions carry interned indices. The normalise filter
reads the receipts and applies them generically, rather than hardcoding per-field
rules.

The receipts are generated dynamically by observing actual serde serialization
calls, not by a static list. This is the key property: if upstream adds a new `Ty`
field somewhere inside `Body` (which we don't control), the receipt generator
automatically detects it because serde's derive-generated code calls
`serialize_newtype_struct("Ty", ...)` for the new field, and our serialization
observer records it.

### Receipt format

```json
{
  "interned_keys": ["span", "ty", "def_id", "id", "alloc_id", "adt_def"],
  "interned_newtypes": ["Type"],
  "interned_positions": {
    "Cast": [2],
    "Closure": [0],
    "VTable": [0],
    "Adt": [0],
    "Field": [1]
  }
}
```

Three categories, mapping directly to the three normalization patterns the jq
filter needs:

| Category | Meaning | jq action |
|----------|---------|-----------|
| `interned_keys` | Object field names whose values are interned indices | `del(.[$key])` or zero the value |
| `interned_newtypes` | Enum variant names that wrap a bare interned integer (e.g. `{"Type": 42}`) | `.[$name] = 0` when value is a number |
| `interned_positions` | Parent array name to list of positions carrying interned indices | `.[$name][$pos] = 0` |

### How it works: the spy serializer

The mechanism is a "spy" `serde::Serializer` implementation that mirrors the
structure of a real serializer but produces no output; it only tracks context
(which struct field, which array position, which enum variant we're currently
inside) and records findings.

When the spy encounters a `serialize_newtype_struct` call whose type name matches
a known interned type (`Ty`, `Span`, `AllocId`, `DefId`, `AdtDef`, `CrateNum`,
`VariantIdx`), it examines the current context to classify the finding:

- Inside a struct field named `"ty"` → `interned_keys` gets `"ty"`
- Inside an enum newtype variant named `"Type"` → `interned_newtypes` gets `"Type"`
- Inside a tuple variant named `"Cast"` at position 2 → `interned_positions["Cast"]` gets `2`

The spy serializer runs as a separate pass before the real `serde_json` serialization.
This means we serialize twice, which is acceptable: the spy pass is cheap (no I/O,
no string formatting, just context tracking) and the SmirJson structure is
typically modest in size. The two passes are:

1. `value.serialize(&mut SpySerializer::new(...))` — collect receipts
2. `serde_json::to_string(&value)` — produce the actual JSON

### How the normaliser consumes receipts

The normalise filter receives the receipts file via jq's `--slurpfile` mechanism:

```shell
jq -S -e --slurpfile receipts input.smir.receipts.json \
   -f normalise-filter.jq input.smir.json
```

The items walk simplifies from a list of hardcoded rules to a generic application
of the receipt:

```jq
# Before (hardcoded):
walk(if type == "object" then del(.ty) | del(.span) | del(.def_id) | del(.id)
     | if .Field then .Field[1] = 0 else . end
     | if .Type and (.Type | type) == "number" then .Type = 0 else . end
     # ... more rules added with each CI failure ...
     else . end)

# After (receipt-driven):
walk(if type == "object" then
       reduce ($receipts[0].interned_keys[]) as $k (.; del(.[$k]))
     | reduce ($receipts[0].interned_newtypes[]) as $n (.;
         if .[$n] and (.[$n] | type) == "number" then .[$n] = 0 else . end)
     | reduce ($receipts[0].interned_positions | to_entries[]) as $e (.;
         if .[$e.key] then
           reduce ($e.value[]) as $p (.; .[$e.key][$p] = 0)
         else . end)
     else . end)
```

The normalise filter no longer needs to know about individual field names or
array positions. A new interned field upstream is automatically captured by the
receipts; the filter handles it without any change.

## Consequences

**What improves:**

- The schema knowledge moves from the jq filter to the Rust code, right next to
  the type definitions. The jq filter becomes a generic consumer.
- New interned fields in Body (which comes from stable_mir and whose structure we
  don't control) are automatically detected via the spy serializer observing
  serde's derive-generated code.
- The receipts file is itself a useful diagnostic artifact: it tells you exactly
  which parts of the output carry non-deterministic values.

**What stays the same:**

- The top-level array handling in the normalise filter (stripping alloc_id from
  the allocs array, removing the Ty key from the types array, etc.) is
  structurally different from the walk-based normalization and remains as
  explicit jq code. The receipts cover the Body tree where the whack-a-mole
  problem lives; the top-level arrays are stable and few.
- Golden files still need regeneration when the normalise filter changes. The
  receipts reduce how often the filter needs to change, but they don't eliminate
  golden file churn entirely.

**What to watch for:**

- The spy serializer depends on stable_mir types using `#[derive(Serialize)]` with
  standard newtype struct serialization. If a type switches to a custom Serialize
  impl that doesn't call `serialize_newtype_struct`, the spy won't detect it. In
  practice this is unlikely for the interned index types (they're all simple
  newtypes around `usize`), but worth noting.
- The `INTERNED_TYPES` list (the set of type names the spy recognizes) is
  maintained in Rust. If stable_mir adds a new interned newtype with a name not
  in the list, it won't be detected. This is a small, infrequently-changing list
  (currently 7 entries) and is trivial to update; it's also easy to validate by
  comparing receipts across platforms.
- The receipts file adds one more output artifact per compilation. The Makefile
  and test harness need to account for it (passing the receipts to jq, cleaning
  up receipts files alongside JSON files).
