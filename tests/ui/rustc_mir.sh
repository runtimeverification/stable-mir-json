#!/bin/bash
set -eu # fail on non-zero retcode or undefined variable
if [ $# -lt 2 ]; then
  printf "usage $0: <inputfile> <outputfile> [extra_rustc_opt...]\n\n"
  printf "alias for:\n\nrustc --emit mir -o <outputfile> [rustc_opt...] [extra_rustc_opt...] -- <inputfile>\n\n"
  printf "where:\n\n[rustc_opt...] is from <inputfile> comments of form: '//@ compile-flags: [rustc_opt...]'\n\n"
  printf "set environment variables:\n"
  printf "  RUSTC_MIR_VERBOSE    - to see raw rustc invocation and output\n"
  printf "  RUSTC_MIR_NOFILEOPTS - to ignore //@ compile-flags: ...\n"
  exit 1
fi

# grab primary arguments and extra options
infile=$1
outfile=$2
shift 2
extraopts=("$@")

# if RUSTC_MIR_NOFILEOPTS unset, extract options embedded in input file as follows:
# 1. ignoring return codes:
#    - use `grep -s`  to extract 0+ lines with comment //@ compile-flags: ...
#    - use `grep -os` to extract fragment of comment starting from colon (:)
# 2. use `${varname:1}` to trim first character from string, i.e., the colon (:)
# 3. use `read -a <varname> <<< "word"` to perform word splitting and store words in array variable
fileopts=()
if [ -z "${RUSTC_MIR_NOFILEOPTS+x}" ]; then
  set +e
  fileopts=$(grep -s '^[[:space:]]*//@[[:space:]]*compile-flags:.*$' "$infile" | grep -os ':.*$')
  set -e
  fileopts=${fileopts:1}
  read -a fileopts <<< "$fileopts"
fi

# invoke rustc with options and optionally ignore output
rustc_cmd=(rustc --emit mir -o "$outfile" "${fileopts[@]}" "${extraopts[@]}" -- "$infile")
if [ -n "${RUSTC_MIR_VERBOSE:-}" ]; then
  printf "%s\n" "${rustc_cmd[*]}"
  "${rustc_cmd[@]}"
else
  "${rustc_cmd[@]}" &>/dev/null
fi
