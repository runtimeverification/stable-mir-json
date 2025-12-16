# Remove the hashes at the end of mangled names
.functions = ( [ .functions[] | if .[1].NormalSym then .[1].NormalSym = .[1].NormalSym[:-17] else .  end ] )
    | .items = ( [ .items[] | if .symbol_name then .symbol_name = .symbol_name[:-17] else .  end ] | map(walk(if type == "object" then del(.ty) else . end)) )
# delete unstable alloc, function, and type IDs
    | .allocs = ( .allocs | map(del(.alloc_id)) | map(del(.ty)) )
    | .functions = ( [ .functions[] ] | map(del(.[0])) | map(walk(if type == "object" then del(.ty) else . end)) )
    | .types     =  ( [ .types[] ] | map(del(.[0])) )
# remove "Never" type
    | .types = ( [ .types[] ] | map(select(.[0] != "VoidType")) )
    |
# Apply the normalisation filter
{ allocs:    ( .allocs | sort ),
  functions: (.functions | sort ),
  items:     (.items | sort ),
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
