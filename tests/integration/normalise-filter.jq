# Remove the hashes at the end of mangled names
.functions = ( [ .functions[] | if .[1].NormalSym then .[1].NormalSym = .[1].NormalSym[:-17] else .  end ] )
    | .items = ( [ .items[] | if .symbol_name then .symbol_name = .symbol_name[:-17] else .  end ] )
    |
# Apply the normalisation filter
{ allocs:
    ( [ .allocs[] ]
# sort allocs by their ID
        | sort_by(.[0])
        | map(del(.[0]))
    ),
  functions:
    ( [ .functions[] ]
# sort functions by their ID (int, first in list)
        | sort_by(.[0])
        | map(del(.[0]))
    ),
  items:
    ( [ .items[] ]
# sort items by symbol name they refer to and by the function name for functions
        | sort_by(.symbol_name, .mono_item_kind.MonoItemFn.name)
    )
}