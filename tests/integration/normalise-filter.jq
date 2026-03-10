# normalise-filter.jq
#
# Normalizes *.smir.json output for golden-file comparison.  Interned
# indices (Ty, Span, DefId, etc.) are non-deterministic across platforms
# and runs; we strip or zero them so that cross-platform diffs are clean.
#
# This filter reads a companion receipts file ($receipts) that declares
# which JSON paths carry interned indices.  See ADR-004 for the design.
#
# The receipts cover the Body tree (where the whack-a-mole problem lives);
# top-level array transformations below are structural and stay explicit.

# ── Hash stripping ──────────────────────────────────────────────────────
#
# Strip platform-specific crate hashes from mangled symbol names.
# Legacy mangling (_ZN...): hash is always the trailing 17 chars (16 hex + "h" prefix).
# v0 mangling (_R...): crate disambiguators appear as C<base62>_ throughout the symbol;
# replace each with "C_" to normalize across platforms.
def strip_hashes:
    if startswith("_R") then gsub("C[a-zA-Z0-9]+_(?=[0-9])"; "C_")
    else .[:-17] end;

# ── Top-level structural transforms ─────────────────────────────────────
#
# These operate on the SmirJson arrays directly and are unrelated to the
# walk-based interned-index normalization that the receipts drive.

.functions = ( [ .functions[] | if .[1].NormalSym then .[1].NormalSym = (.[1].NormalSym | strip_hashes) else .  end ] )
    | .items = ( [ .items[] | if .symbol_name then .symbol_name = (.symbol_name | strip_hashes) else .  end ] )
# delete unstable alloc, function, and type IDs
    | .allocs = ( .allocs | map(del(.alloc_id)) | map(del(.ty)) )
    | .functions = ( [ .functions[] ] | map(del(.[0])) )
    | .types     =  ( [ .types[] ] | map(del(.[0])) )
# remove "Never" type
    | .types = ( [ .types[] ] | map(select(.[0] != "VoidType")) )
    |

# ── Restructure and sort ────────────────────────────────────────────────

{ allocs:    ( .allocs | sort ),
  functions: (.functions | sort ),
  items:     (.items | sort_by(.symbol_name + "|" + (.mono_item_kind.MonoItemFn.name // "")) ),
  types: ( [
# sort by constructors and remove unstable IDs within each
    ( .types | map(select(.[0].PrimitiveType)) | sort ),
  # delete unstable adt_ref IDs and struct field Ty IDs
    ( .types | map(select(.[0].EnumType) | del(.[0].EnumType.adt_def) | .[0].EnumType.fields = "elided") | sort_by(.[0].EnumType.name) ),
    ( .types | map(select(.[0].StructType) | del(.[0].StructType.adt_def) | .[0].StructType.fields = "elided" ) | sort_by(.[0].StructType.name) ),
    ( .types | map(select(.[0].UnionType) | del(.[0].UnionType.adt_def)) | sort_by(.[0].UnionType.name) ),
  # delete unstable Ty IDs for arrays and tuples
    ( .types | map(select(.[0].ArrayType) | del(.[0].ArrayType.elem_type) | del(.[0].ArrayType.size.id) | del(.[0].ArrayType.size.kind.Value[0])) | sort ),
    ( .types | map(select(.[0].TupleType) | .[0].TupleType.types = "elided") ),
  # replace unstable Ty IDs for references by zero
    ( .types | map(select(.[0].PtrType) | .[0].PtrType.pointee_type = "elided") | sort ),
    ( .types | map(select(.[0].RefType) | .[0].RefType.pointee_type = "elided") | sort ),
  # keep function type strings
    ( .types | map(select(.[0].FunType) | sort) )
  ] | flatten(1) )
}

# ── Receipt-driven interned-index normalization ─────────────────────────
#
# Instead of hardcoding per-field rules, we read the receipts and apply
# three generic passes:
#
#   1. interned_keys:      delete object fields whose values are interned
#   2. interned_newtypes:  zero enum-variant wrappers around bare integers
#   3. interned_positions: zero known tuple positions carrying interned indices

| walk(if type == "object" then
    # 1. Strip interned key fields (e.g. "span", "ty", "def_id", "id")
    reduce ($receipts[0].interned_keys[]) as $k (.; del(.[$k]))
    # 2. Zero interned newtype wrappers (e.g. {"Type": 42} → {"Type": 0})
  | reduce ($receipts[0].interned_newtypes[]) as $n (.;
      if .[$n] and (.[$n] | type) == "number" then .[$n] = 0 else . end)
    # 3. Zero interned positions in tuple variants (e.g. Cast[2], Field[1])
  | reduce ($receipts[0].interned_positions | to_entries[]) as $e (.;
      if .[$e.key] and (.[$e.key] | type) == "array" then
        reduce ($e.value[]) as $p (.;
          if .[$e.key][$p] then .[$e.key][$p] = 0 else . end)
      else . end)
  else . end)
