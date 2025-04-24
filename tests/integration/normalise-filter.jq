# Remove the hashes at the end of mangled names
.functions = ( [ .functions[] | if .[1].NormalSym then .[1].NormalSym = .[1].NormalSym[:-17] else .  end ] )
    | .items = ( [ .items[] | if .symbol_name then .symbol_name = .symbol_name[:-17] else .  end ] )
# delete unstable alloc, function, and type IDs
    | .allocs    = ( [ .allocs[]    ] | map(del(.[0])) )
    | .functions = ( [ .functions[] ] | map(del(.[0])) )
    | .types     =  ( [ .types[] ] | map(del(.[0])) )
    |
# Apply the normalisation filter
{ allocs:    .allocs,
  functions: .functions,
  items:     .items,
  types: ( [
# sort by constructors and remove unstable IDs within each
    ( .types | map(select(.[0].PrimitiveType)) ),
  # delete unstable adt_ref IDs
    ( .types | map(select(.[0].EnumType) | del(.[0].EnumType.adt_def)) ),
    ( .types | map(select(.[0].StructType) | del(.[0].StructType.adt_def)) ),
    ( .types | map(select(.[0].UnionType) | del(.[0].UnionType.adt_def)) ),
  # delete unstable Ty IDs for arrays and tuples
    ( .types | map(select(.[0].ArrayType) | del(.[0].ArrayType[0])) ),
    ( .types | map(select(.[0].TupleType) | .[0].TupleType = "elided") ),
  # replace unstable Ty IDs for references by zero
    ( .types | map(select(.[0].PtrType) | .[0].PtrType = "elided") ),
    ( .types | map(select(.[0].RefType) | .[0].RefType = "elided") ),
  # keep function type strings
    ( .types | map(select(.[0].FunType) | .[0].FunType = "elided") )
  ] | flatten(1) )
}
