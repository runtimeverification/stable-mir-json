#!/bin/bash
set -eu # fail on non-zero retcode or undefined variable
print_usage() {
  printf "usage: $(basename $0) [-x] [-e] [-f] [-t] [-h]\n\n"
  printf "print rustc UI test sources which satisfy certain criteria; by default those that are:\n"
  printf "normal, passing, contain a non-empty main function, and compile in reasonable time\n\n"
  printf "options:\n"
  printf "x\tinclude fixed tests [may contain run-rustfix header]\n"
  printf "e\tinclude tests with empty/missing main function\n"
  printf "f\tinclude failing tests [may NOT contain run-pass header] (implies -t)\n"
  printf "t\tinclude tests that fail to generate MIR within 30s\n"
  printf "h\tprint this usage and exit\n\n"
  printf "if -t unset:\n"
  printf "  set environment variable RUSTC_MIR_VERBOSE to see raw rustc invocation and CLI output\n"
  printf "  set environment variable RUSTC_MIR_NOFILEOPTS to skip //@ compile-flags: ...\n\n"
  [ $# -ne 0 ] && echo "ERROR: $*"
  exit 1
}

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

# setup time mir filter
# NOTE: temp file is needed because rustc chokes when writing directly to /dev/null
# NOTE: this filter does not work natively on mac due to no timeout command
RUSTC_MIR=$SCRIPT_DIR/rustc_mir.sh
TMPMIR=$SCRIPT_DIR/temp.mir
RUSTOPT=('-C' 'overflow-checks=off')
TIMEMIR=('-execdir' 'timeout' '30s' "$RUSTC_MIR" '{}' "$TMPMIR" "${RUSTOPT[@]}" ';')

# setup grep filters
HAS_MATCH=$SCRIPT_DIR/has_match.sh
RUSTFIX=('-execdir' "$HAS_MATCH" 'n' '//@[[:space:]]*run-rustfix'                                                  '{}' ';')
RUNPASS=('-execdir' "$HAS_MATCH" 'y' '//@[[:space:]]*run-pass'                                                     '{}' ';')
HASMAIN=('-execdir' "$HAS_MATCH" 'y' 'fn[[:space:]]\{1,\}main[[:space:]]*([[:space:]]*)'                           '{}' ';')
EMPMAIN=('-execdir' "$HAS_MATCH" 'n' 'fn[[:space:]]\{1,\}main[[:space:]]*([[:space:]]*)[[:space:]]*{[[:space:]]*}' '{}' ';')

# parse opts
while getopts 'xfet' opt; do
  case "${opt}" in
    x)   RUSTFIX=()  ;;
    e)   HASMAIN=()
         EMPMAIN=()  ;;
    f)   RUNPASS=()
         TIMEMIR=()  ;;
    t)   TIMEMIR=()  ;;
    ?|h) print_usage "environment variable RUST_TOP must point to a valid rustc distribution" ;;
  esac
done

# check test dir
# NOTE: ${var:-default} expands to default if var unset
[ -z "${RUST_TOP:-}" ] && print_usage "set RUST_TOP environment variable to your local Rust compiler source directory before running this script"
RUST_TESTS=${RUST_TOP}/tests
[ ! -d "${RUST_TESTS}" ] && print_usage "RUST_TOP environment variable does not appear to point to a local Rust compiler source directory"

# clean temp mir file whenever timing mir, ignore errors
if [ -n "${TIMEMIR[*]}" ]; then
  trap "rm \"$TMPMIR\" &> /dev/null" SIGINT SIGTERM EXIT
fi

# find test source files that pass filters
( cd "${RUST_TOP}";
  find tests/ui           \
       -name '*.rs'       \
       "${RUSTFIX[@]}"    \
       "${RUNPASS[@]}"    \
       "${HASMAIN[@]}"    \
       "${EMPMAIN[@]}"    \
       "${TIMEMIR[@]}"    \
       -print )
