# Remove the hashes at the end of mangled names
.functions = ( [ .functions[] | if .[1].NormalSym then .[1].NormalSym = .[1].NormalSym[:-17] else .  end ] )
    | .items = ( [ .items[] | if .symbol_name then .symbol_name = .symbol_name[:-17] else .  end ] )
# delete unstable alloc, function, and type IDs
    | .allocs    = ( [ .allocs[]    ] | map(del(.[0])) | map(del(.[0].[0])) )
    | .functions = ( [ .functions[] ] | map(del(.[0])) )
    | .types     =  ( [ .types[] ] | map(del(.[0])) )
    |
# Apply the normalisation filter
{ allocs:    .allocs,
  functions: .functions,
  items:     .items,
  types: ( [
# sort by constructors and remove unstable IDs within each
    ( .types | map(select(.[0].PrimitiveType)) | sort ),
  # delete unstable adt_ref IDs and struct field Ty IDs
    ( .types | map(select(.[0].EnumType) | del(.[0].EnumType.adt_def)) | sort ),
    ( .types | map(select(.[0].StructType) | del(.[0].StructType.adt_def) | .[0].StructType.fields = "elided" ) | sort ),
    ( .types | map(select(.[0].UnionType) | del(.[0].UnionType.adt_def)) | sort ),
  # delete unstable Ty IDs for arrays and tuples
    ( .types | map(select(.[0].ArrayType) | del(.[0].ArrayType[0]) | del(.[0].ArrayType[0].id)) | sort ),
    ( .types | map(select(.[0].TupleType) | .[0].TupleType.types = "elided") ),
  # replace unstable Ty IDs for references by zero
    ( .types | map(select(.[0].PtrType) | .[0].PtrType = "elided") ),
    ( .types | map(select(.[0].RefType) | .[0].RefType = "elided") ),
  # keep function type strings
    ( .types | map(select(.[0].FunType) | .[0].FunType = "elided") )
  ] | flatten(1) )
}
