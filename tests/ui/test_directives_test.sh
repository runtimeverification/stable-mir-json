#!/usr/bin/env bash
# test_directives_test.sh: unit tests for parse_test_directives.awk
#
# Each test creates a tiny synthetic .rs file with //@ directives, runs
# the awk script against it, and checks the output.
#
# Tests that exercise non-obvious boundary behavior are annotated with
# notes explaining the design decision. These notes are collected and
# printed in a "Boundary notes" report after the pass/fail summary, so
# someone reading the output can understand (and challenge) the choices.
#
# Usage: bash tests/ui/test_directives_test.sh

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
AWK="$SCRIPT_DIR/parse_test_directives.awk"
TMPFILE=$(mktemp /tmp/directive_test.XXXXXX.rs)
trap 'rm -f "$TMPFILE"' EXIT

passed=0
failed=0
boundary_notes=()

# run_awk <host_os> <host_arch> <host_bits> <file_content>
# Prints the awk output line.
run_awk() {
  local os="$1" arch="$2" bits="$3"
  shift 3
  printf '%s\n' "$@" > "$TMPFILE"
  awk -v host_os="$os" -v host_arch="$arch" -v host_bits="$bits" \
      -f "$AWK" "$TMPFILE"
}

# assert_output <test_name> <expected> <actual>
assert_output() {
  local name="$1" expected="$2" actual="$3"
  if [[ "$actual" == "$expected" ]]; then
    (( ++passed ))
  else
    printf 'FAIL: %s\n  expected: %s\n  actual:   %s\n' "$name" "$expected" "$actual"
    (( ++failed ))
  fi
}

# Shorthand: assert that running awk produces the expected output.
# check <test_name> <os> <arch> <bits> <expected_output> <lines...>
check() {
  local name="$1" os="$2" arch="$3" bits="$4" expected="$5"
  shift 5
  local actual
  actual=$(run_awk "$os" "$arch" "$bits" "$@")
  assert_output "$name" "$expected" "$actual"
}

# Register a boundary note. These are printed in a report section after
# the pass/fail summary to document non-obvious design decisions.
# boundary <test_name> <note>
boundary() {
  boundary_notes+=("$(printf '  %-52s %s' "$1:" "$2")")
}

# =========================================================================
# Skip directives: only-<os>
# =========================================================================

check "only-linux skips on macos" \
  macos aarch64 64 "SKIP	only-linux" \
  "//@ only-linux"

check "only-linux passes on linux" \
  linux x86_64 64 "FLAGS	" \
  "//@ only-linux"

check "only-windows skips on linux" \
  linux x86_64 64 "SKIP	only-windows" \
  "//@ only-windows"

check "only-macos passes on macos" \
  macos aarch64 64 "FLAGS	" \
  "//@ only-macos"

check "only-macos skips on linux" \
  linux x86_64 64 "SKIP	only-macos" \
  "//@ only-macos"

check "only-unix passes on linux" \
  linux x86_64 64 "FLAGS	" \
  "//@ only-unix"

check "only-unix passes on macos" \
  macos aarch64 64 "FLAGS	" \
  "//@ only-unix"
boundary "only-unix passes on macos" \
  "macos is unix (unix = linux|macos|freebsd|openbsd|netbsd|dragonfly|solaris|illumos|android)"

check "only-unix skips on windows" \
  windows x86_64 64 "SKIP	only-unix" \
  "//@ only-unix"

check "only-apple passes on macos" \
  macos aarch64 64 "FLAGS	" \
  "//@ only-apple"
boundary "only-apple passes on macos" \
  "apple currently = macos only; if iOS/tvOS targets arise, expand is_apple in the awk script"

check "only-apple skips on linux" \
  linux x86_64 64 "SKIP	only-apple" \
  "//@ only-apple"

check "only-msvc skips on linux" \
  linux x86_64 64 "SKIP	only-msvc" \
  "//@ only-msvc"
boundary "only-msvc skips on linux" \
  "only-msvc is treated as only-windows (we don't distinguish MSVC vs GNU toolchains)"

# =========================================================================
# Skip directives: only-<arch>
# =========================================================================

check "only-x86_64 passes on x86_64" \
  linux x86_64 64 "FLAGS	" \
  "//@ only-x86_64"

check "only-x86_64 skips on aarch64" \
  linux aarch64 64 "SKIP	only-x86_64" \
  "//@ only-x86_64"

check "only-aarch64 passes on aarch64" \
  macos aarch64 64 "FLAGS	" \
  "//@ only-aarch64"

check "only-aarch64 skips on x86_64" \
  linux x86_64 64 "SKIP	only-aarch64" \
  "//@ only-aarch64"

check "only-x86 (bare) skips on aarch64" \
  linux aarch64 64 "SKIP	only-x86" \
  "//@ only-x86"

check "only-x86 (bare) passes on x86_64" \
  linux x86_64 64 "FLAGS	" \
  "//@ only-x86"
boundary "only-x86 (bare) passes on x86_64" \
  "only-x86 is a family match: covers both x86_64 and i686 (rustc convention)"

# =========================================================================
# Skip directives: only-<bits>
# =========================================================================

check "only-32bit skips on 64-bit" \
  linux x86_64 64 "SKIP	only-32bit" \
  "//@ only-32bit"

check "only-32bit passes on 32-bit" \
  linux i686 32 "FLAGS	" \
  "//@ only-32bit"

check "only-64bit passes on 64-bit" \
  linux x86_64 64 "FLAGS	" \
  "//@ only-64bit"

check "only-64bit skips on 32-bit" \
  linux i686 32 "SKIP	only-64bit" \
  "//@ only-64bit"

# =========================================================================
# Skip directives: ignore-<os>
# =========================================================================

check "ignore-linux skips on linux" \
  linux x86_64 64 "SKIP	ignore-linux" \
  "//@ ignore-linux"

check "ignore-linux passes on macos" \
  macos aarch64 64 "FLAGS	" \
  "//@ ignore-linux"

check "ignore-macos skips on macos" \
  macos aarch64 64 "SKIP	ignore-macos" \
  "//@ ignore-macos"

check "ignore-apple skips on macos" \
  macos aarch64 64 "SKIP	ignore-apple" \
  "//@ ignore-apple"

check "ignore-apple passes on linux" \
  linux x86_64 64 "FLAGS	" \
  "//@ ignore-apple"

check "ignore-windows passes on linux" \
  linux x86_64 64 "FLAGS	" \
  "//@ ignore-windows"

check "ignore-unix skips on linux" \
  linux x86_64 64 "SKIP	ignore-unix" \
  "//@ ignore-unix"

check "ignore-unix skips on macos" \
  macos aarch64 64 "SKIP	ignore-unix" \
  "//@ ignore-unix"
boundary "ignore-unix skips on macos" \
  "mirrors only-unix: macos is unix, so ignore-unix skips on macos"

# =========================================================================
# Skip directives: ignore-<arch>
# =========================================================================

check "ignore-aarch64 skips on aarch64" \
  macos aarch64 64 "SKIP	ignore-aarch64" \
  "//@ ignore-aarch64"

check "ignore-aarch64 passes on x86_64" \
  linux x86_64 64 "FLAGS	" \
  "//@ ignore-aarch64"

check "ignore-x86_64 skips on x86_64" \
  linux x86_64 64 "SKIP	ignore-x86_64" \
  "//@ ignore-x86_64"

# =========================================================================
# Skip directives: needs-sanitizer
# =========================================================================

check "needs-sanitizer-cfi skips everywhere" \
  linux x86_64 64 "SKIP	needs-sanitizer" \
  "//@ needs-sanitizer-cfi"

check "needs-sanitizer-address skips everywhere" \
  macos aarch64 64 "SKIP	needs-sanitizer" \
  "//@ needs-sanitizer-address"
boundary "needs-sanitizer-address skips everywhere" \
  "all needs-sanitizer-* skip unconditionally; our test harness has no sanitizer support"

# Skip directives: needs-subprocess

check "needs-subprocess skips (no binary with -Zno-codegen)" \
  linux x86_64 64 "SKIP	needs-subprocess" \
  "//@ run-pass" \
  "//@ needs-subprocess"
boundary "needs-subprocess skips (no binary with -Zno-codegen)" \
  "we run with -Zno-codegen so there is no binary to fork/exec"

# =========================================================================
# Revision-gated skip directives
# =========================================================================

check "revision-gated skips on aarch64 (last reason wins)" \
  macos aarch64 64 "SKIP	only-x86" \
  "//@ revisions: x64 x32" \
  "//@ [x64]only-x86_64" \
  "//@ [x32]only-x86"
boundary "revision-gated skips on aarch64 (last reason wins)" \
  "both [x64] and [x32] trigger skip on aarch64; last processed reason overwrites (here: only-x86)"

check "revision-gated passes on x86_64" \
  linux x86_64 64 "FLAGS	" \
  "//@ revisions: x64 x32" \
  "//@ [x64]only-x86_64" \
  "//@ [x32]only-x86"
boundary "revision-gated passes on x86_64" \
  "conservative: revision-gated skip directives are stripped and applied globally; x86_64 satisfies both only-x86_64 and only-x86"

# =========================================================================
# Flag extraction: compile-flags
# =========================================================================

check "compile-flags extracted" \
  linux x86_64 64 "FLAGS	-C opt-level=3 -Zvalidate-mir" \
  "//@ compile-flags: -C opt-level=3 -Zvalidate-mir"

check "multiple compile-flags lines concatenated" \
  linux x86_64 64 "FLAGS	-C opt-level=3 -Zvalidate-mir" \
  "//@ compile-flags: -C opt-level=3" \
  "//@ compile-flags: -Zvalidate-mir"

# =========================================================================
# Flag extraction: edition
# =========================================================================

check "edition extracted" \
  linux x86_64 64 "FLAGS	--edition 2021" \
  "//@ edition: 2021"

check "compile-flags and edition combined" \
  linux x86_64 64 "FLAGS	-C opt-level=3 --edition 2021" \
  "//@ compile-flags: -C opt-level=3" \
  "//@ edition: 2021"

check "range edition uses earliest (dotdot)" \
  linux x86_64 64 "FLAGS	--edition 2015" \
  "//@ edition:2015..2021"

check "range edition uses earliest (dotdoteq)" \
  linux x86_64 64 "FLAGS	--edition 2021" \
  "//@ edition:2021..=2024"

# =========================================================================
# Flag extraction: rustc-env
# =========================================================================

check "rustc-env extracted" \
  linux x86_64 64 "FLAGS	--env-set MY_VAR=hello -Zunstable-options" \
  "//@ rustc-env:MY_VAR=hello"

# =========================================================================
# Revision-gated flags: must NOT be extracted
# =========================================================================

check "revision-gated compile-flags not extracted" \
  linux x86_64 64 "FLAGS	" \
  "//@ revisions: a b" \
  "//@ [a]compile-flags: --edition 2018" \
  "//@ [b]compile-flags: --edition 2021"
boundary "revision-gated compile-flags not extracted" \
  "applying both --edition 2018 and --edition 2021 would conflict; all revision-gated flags are dropped"

check "revision-gated edition not extracted" \
  linux x86_64 64 "FLAGS	" \
  "//@ [a]edition: 2018"
boundary "revision-gated edition not extracted" \
  "same rationale: we don't know which revision to run, so gated flags are unsafe to extract"

check "mixed: non-gated flags kept, gated flags dropped" \
  linux x86_64 64 "FLAGS	-C opt-level=3" \
  "//@ compile-flags: -C opt-level=3" \
  "//@ [x32]compile-flags: -Ctarget-feature=+sse2"
boundary "mixed: non-gated flags kept, gated flags dropped" \
  "non-gated -C opt-level=3 is safe to extract; gated -Ctarget-feature=+sse2 is dropped"

# =========================================================================
# No directives at all
# =========================================================================

check "no directives: empty flags" \
  linux x86_64 64 "FLAGS	" \
  "fn main() {}"

# =========================================================================
# Skip takes precedence over flags
# =========================================================================

check "skip takes precedence over extracted flags" \
  macos aarch64 64 "SKIP	only-linux" \
  "//@ only-linux" \
  "//@ compile-flags: -C opt-level=3"
boundary "skip takes precedence over extracted flags" \
  "flags are still parsed but never emitted; the SKIP line is output instead of FLAGS"

# =========================================================================
# Summary
# =========================================================================

total=$(( passed + failed ))
printf '\n—— Directive parser tests ——\n'
printf 'Passed: %d / %d\n' "$passed" "$total"
if (( failed > 0 )); then
  printf 'FAILED: %d\n' "$failed"
fi

if (( ${#boundary_notes[@]} > 0 )); then
  printf '\n—— Boundary notes (%d) ——\n' "${#boundary_notes[@]}"
  printf 'Tests below exercise non-obvious behavior. If a test fails, check\n'
  printf 'whether the expectation or the awk script needs updating.\n\n'
  for note in "${boundary_notes[@]}"; do
    printf '%s\n' "$note"
  done
fi

if (( failed > 0 )); then
  exit 1
else
  printf '\nAll tests passed.\n'
fi
