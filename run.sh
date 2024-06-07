#!/bin/bash
set -eu
BIN=smir_pretty
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
SEP=""
[ -n "${LD_LIBRARY_PATH:-}" ] && SEP=":"
export LD_LIBRARY_PATH="$SCRIPT_DIR/deps/rust/src/build/x86_64-unknown-linux-gnu/stage2/lib$SEP${LD_LIBRARY_PATH:-}"
if [ -x "$SCRIPT_DIR/target/debug/$BIN" ]; then
  "$SCRIPT_DIR/target/debug/$BIN"   "$@"
elif [ -x "$SCRIPT_DIR/target/release/$BIN" ]; then
  "$SCRIPT_DIR/target/release/$BIN" "$@"
else
  echo "Could not find smir_pretty executable; is it built?"
  exit 1
fi
