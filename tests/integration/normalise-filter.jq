# Remove the hashes at the end of mangled names
.functions = ( [ .functions[] | if .[1].NormalSym then .[1].NormalSym = .[1].NormalSym[:-17] else .  end ] )
    | .items = ( [ .items[] | if .symbol_name then .symbol_name = .symbol_name[:-17] else .  end ] )
    |
# Apply the normalisation filter
{ allocs:
    ( [ .allocs[] ]
# delete unstable alloc ID
        | map(del(.[0]))
    ),
  functions:
    ( [ .functions[] ]
# delete unstable function ID
        | map(del(.[0]))
    ),
  items:
    ( [ .items[] ]
    ),
  types:
    ( [ .types[] ]
# delete unstable Ty ID (int, first in list)
        | map(del(.[0]))
# delete unstable adt_def from Struct and Enum
        | map(del(.[0].StructType.adt_def))
        | map(del(.[0].EnumType.adt_def))
    )
}
