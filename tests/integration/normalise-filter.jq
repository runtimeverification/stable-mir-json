# Remove the hashes at the end of mangled names
.functions = ( [ .functions[] | if .[1].NormalSym then .[1].NormalSym = .[1].NormalSym[:-17] else .  end ] )
    | .items = ( [ .items[] | if .symbol_name then .symbol_name = .symbol_name[:-17] else .  end ] )
# delete unstable alloc, function, and type IDs
    | .allocs = ( .allocs | map(del(.alloc_id)) | map(del(.ty)) )
    | .functions = ( [ .functions[] ] | map(del(.[0])) )
    | .types     =  ( [ .types[] ] | map(del(.[0])) )
# remove "Never" type
    | .types = ( [ .types[] ] | map(select(.[0] != "VoidType")) )
    |
# Apply the normalisation filter
{ allocs:    ( .allocs | sort ),
  functions: (.functions | sort ),
  items:     (.items | map(walk(if type == "object" then del(.ty) else . end)) | sort ),
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
# Strip def_id fields globally. These are interned compiler indices (the
# underlying ID inside AdtDef) that are consistent within a single rustc
# invocation but not stable across runs; the same non-determinism that
# affects alloc_id, Ty indices, and adt_def (see lines 5-6, 18-21 above).
# Downstream consumers use adt_def/def_id as cross-reference keys to join
# AggregateKind::Adt in MIR bodies with type metadata entries, so the
# values can't be dropped from the output itself; we only strip them here
# for golden-file comparison.
| walk(if type == "object" then del(.def_id) else . end)
