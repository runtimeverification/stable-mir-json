{ allocs:
    ( [ .allocs[] ]
# sort allocs by their ID
        | sort_by(.[0])
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