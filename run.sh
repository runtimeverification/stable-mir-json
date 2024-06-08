#!/bin/bash
set -eu
BIN=smir_pretty
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
RUST_DIR=${SCRIPT_DIR}/deps/rust
ARCH=$("${SCRIPT_DIR}/rustc_arch.sh")
STAGE=$(cat "${RUST_DIR}/stage")
RUST_BUILD_DIR=${RUST_DIR}/src/build/${ARCH}
SEP=""
[ -n "${LD_LIBRARY_PATH:-}" ] && SEP=":"
export LD_LIBRARY_PATH="${RUST_BUILD_DIR}/stage${STAGE}/lib/rustlib/x86_64-unknown-linux-gnu/lib:$SEP${LD_LIBRARY_PATH:-}"
if [ -x "$SCRIPT_DIR/target/debug/$BIN" ]; then
  "$SCRIPT_DIR/target/debug/$BIN"   "$@"
elif [ -x "$SCRIPT_DIR/target/release/$BIN" ]; then
  "$SCRIPT_DIR/target/release/$BIN" "$@"
else
  echo "Could not find smir_pretty executable; is it built?"
  exit 1
fi
