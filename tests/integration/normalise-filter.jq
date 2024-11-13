{ allocs:
    ( [ .allocs[] ]
# sort allocs by their ID
        | sort_by(.[0])
# TODO this should be removed
        | map ( select( .[1] | has("Static") | not ) )
    ),
  functions:
    ( [ .functions[] ]
# sort functions by their ID (int, first in list)
        | sort_by(.[0])
    ),
  items:
    ( [ .items[] ]
# sort items by symbol name they refer to
        | sort_by(.symbol_name)
    )
}