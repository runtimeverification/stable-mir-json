#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: run_ui_tests.sh [--verbose] [--save-generated-output] [--save-debug-output] RUST_DIR_ROOT

Options:
  --verbose                Print passing and skipped tests.
  --save-generated-output   Do not delete generated *.smir.json files.
  --save-debug-output      On failure, print stderr snippet inline and save
                           full stderr to <UI_DIR>/debug/<test_name>.stderr.
  --help, -h               Show this help.

Environment:
  RUN_SMIR                 Optional override for the runner command.
                           Example: RUN_SMIR=./target/debug/stable_mir_json
                           (If you include flags, they will be split on spaces.)
EOF
}

die() {
  printf 'Error: %s\n' "$*" >&2
  exit 1
}

log() {
  printf '%s\n' "$*"
}

# -------------------------
# Extract test directives
# -------------------------

# Extract //@ compile-flags:, //@ edition:, and //@ rustc-env: directives
# from a test file. Prints space-separated flags to stdout.
extract_test_flags() {
    local file="$1"
    local flags=""

    # Extract //@ compile-flags: directives (everything after "compile-flags:")
    local compile_flags
    compile_flags=$(grep -s '^[[:space:]]*//@[[:space:]]*compile-flags:' "$file" \
                    | sed 's/^.*compile-flags:[[:space:]]*//' || true)
    if [ -n "$compile_flags" ]; then
        flags="$compile_flags"
    fi

    # Extract //@ edition: directive (e.g., "//@ edition: 2021")
    local edition
    edition=$(grep -s '^[[:space:]]*//@[[:space:]]*edition:' "$file" \
              | sed 's/^.*edition:[[:space:]]*//' | head -1 || true)
    if [ -n "$edition" ]; then
        flags="$flags --edition $edition"
    fi

    # Extract //@ rustc-env: directives (e.g., "//@ rustc-env:MY_VAR=value")
    # These set environment variables for the rustc process via --env-set.
    local rustc_envs
    rustc_envs=$(grep -s '^[[:space:]]*//@[[:space:]]*rustc-env:' "$file" \
                 | sed 's/^.*rustc-env:[[:space:]]*//' || true)
    if [ -n "$rustc_envs" ]; then
        while IFS= read -r env_pair; do
            flags="$flags --env-set $env_pair -Zunstable-options"
        done <<< "$rustc_envs"
    fi

    echo "$flags"
}

# -------------------------
# Arg parsing
# -------------------------
VERBOSE=0
SAVE_GENERATED_OUTPUT=0
SAVE_DEBUG_OUTPUT=0
RUST_DIR_ROOT=""

while (( $# > 0 )); do
  case "$1" in
    --verbose)
      VERBOSE=1
      shift
      ;;
    --save-generated-output)
      SAVE_GENERATED_OUTPUT=1
      shift
      ;;
    --save-debug-output)
      SAVE_DEBUG_OUTPUT=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    --*)
      die "unknown option: $1"
      ;;
    *)
      if [[ -z "$RUST_DIR_ROOT" ]]; then
        RUST_DIR_ROOT=$1
      else
        die "unexpected argument: $1"
      fi
      shift
      ;;
  esac
done

[[ -n "$RUST_DIR_ROOT" ]] || { usage; exit 1; }
[[ -d "$RUST_DIR_ROOT" ]] || die "RUST_DIR_ROOT is not a directory: $RUST_DIR_ROOT"

# -------------------------
# Paths
# -------------------------
UI_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
PASSING_TSV="${UI_DIR}/passing.tsv"
[[ -f "$PASSING_TSV" ]] || die "Missing TSV file: $PASSING_TSV"

DEBUG_DIR="${UI_DIR}/debug"
if (( SAVE_DEBUG_OUTPUT )); then
  rm -rf "$DEBUG_DIR"
  mkdir -p "$DEBUG_DIR"
fi

# -------------------------
# Runner setup
# -------------------------
declare -a RUN_SMIR_CMD

if [[ -z "${RUN_SMIR:-}" ]]; then
  # Build once upfront, then invoke the binary directly to avoid cargo's
  # per-invocation freshness check (~2-3s overhead * thousands of tests).
  log "RUN_SMIR unset; building and using binary directly"
  cargo build

  SMIR_BIN="./target/debug/stable_mir_json"
  [[ -x "$SMIR_BIN" ]] || die "failed to build: $SMIR_BIN"

  SYSROOT_LIB="$(rustc --print sysroot)/lib"
  DYLD_LIBRARY_PATH="${SYSROOT_LIB}${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
  LD_LIBRARY_PATH="${SYSROOT_LIB}${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  export DYLD_LIBRARY_PATH LD_LIBRARY_PATH

  RUN_SMIR_CMD=( "$SMIR_BIN" -Zno-codegen )
else
  # Allow RUN_SMIR to be either a path or a simple "path flags..." string.
  # Note: this splits on spaces (no shell quoting).
  read -r -a RUN_SMIR_CMD <<<"$RUN_SMIR"
  [[ -n "${RUN_SMIR_CMD[0]:-}" ]] || die "RUN_SMIR is set but empty"
  [[ -x "${RUN_SMIR_CMD[0]}" ]] || die "RUN_SMIR binary is not executable: ${RUN_SMIR_CMD[0]}"
  RUN_SMIR_CMD+=( -Zno-codegen )
fi

# -------------------------
# Arch filtering
# -------------------------
HOST_ARCH="$(uname -m)"
case "$HOST_ARCH" in
  arm64|aarch64)
    SKIP_ARCH_RE='/(x86_64|x86|i686|i386|s390x|powerpc|mips|riscv|loongarch|sparc|hexagon|bpf|avr|msp430|nvptx)/'
    HOST_ONLY_RE='^//@ (\[.*\])?only-(aarch64|arm)'
    ;;
  x86_64)
    SKIP_ARCH_RE='/(aarch64|arm|s390x|powerpc|mips|riscv|loongarch|sparc|hexagon|bpf|avr|msp430|nvptx)/'
    HOST_ONLY_RE='^//@ (\[.*\])?only-(x86_64|x86)'
    ;;
  *)
    SKIP_ARCH_RE='^$'       # no filtering on unknown arch
    HOST_ONLY_RE=''         # no directive filtering either
    ;;
esac

ANY_ARCH_ONLY_RE='^//@ (\[.*\])?only-(x86_64|x86|i686|aarch64|arm|s390x|riscv|mips|powerpc|loongarch|sparc)'

log "Running regression tests for passing UI cases (host: $HOST_ARCH)..."

start_time=$SECONDS
failed=0
passed=0
skipped=0
total=0

while IFS= read -r test; do
  [[ -n "$test" ]] || continue

  test_path="${RUST_DIR_ROOT}/${test}"
  test_name="$(basename "$test" .rs)"
  json_file="${PWD}/${test_name}.smir.json"

  (( ++total ))

  # Skip tests gated on a different architecture.
  # Check 1: path contains a foreign-arch directory name.
  # Check 2: file contains a rustc `//@ only-<arch>` directive for a different arch.
  skip_test=0
  if grep -qE "$SKIP_ARCH_RE" <<<"$test"; then
    skip_test=1
  elif [[ -f "$test_path" ]] && grep -qE "$ANY_ARCH_ONLY_RE" "$test_path"; then
    if [[ -n "${HOST_ONLY_RE:-}" ]] && ! grep -qE "$HOST_ONLY_RE" "$test_path"; then
      skip_test=1
    fi
  fi

  if (( skip_test )); then
    (( ++skipped ))
    if (( VERBOSE )); then
      log "⏭️  SKIPPED (arch): $test_path"
    fi
    continue
  fi

  # Extract //@ compile-flags:, //@ edition:, and //@ rustc-env: directives.
  test_flags=$(extract_test_flags "$test_path")

  # shellcheck disable=SC2086 # intentional word-splitting of $test_flags
  if (( SAVE_DEBUG_OUTPUT )); then
    test_stderr=$(mktemp)
    "${RUN_SMIR_CMD[@]}" ${test_flags} "$test_path" >/dev/null 2>"$test_stderr" && rc=0 || rc=$?
  else
    "${RUN_SMIR_CMD[@]}" ${test_flags} "$test_path" >/dev/null 2>&1 && rc=0 || rc=$?
  fi

  if (( rc == 0 )); then
    (( ++passed ))
    if (( VERBOSE )); then
      log "✅ PASSING: $test_path"
    fi
  else
    log "❌ FAILED: $test_path (exit $rc)"
    if (( SAVE_DEBUG_OUTPUT )) && [[ -s "$test_stderr" ]]; then
      tail -4 "$test_stderr" | sed 's/^/   /'
      cp -- "$test_stderr" "${DEBUG_DIR}/${test_name}.stderr"
    fi
    (( ++failed ))
  fi
  (( SAVE_DEBUG_OUTPUT )) && rm -f -- "$test_stderr"

  # Clean up generated JSON
  if (( ! SAVE_GENERATED_OUTPUT )) && [[ -f "$json_file" ]]; then
    rm -f -- "$json_file"
  fi
done <"$PASSING_TSV"

elapsed=$(( SECONDS - start_time ))
printf '—— Summary ——\n'
printf 'Total tests : %d\n' "$total"
printf 'Skipped     : %d\n' "$skipped"
printf 'Passed      : %d\n' "$passed"
printf 'Failed      : %d\n' "$failed"
printf 'Elapsed     : %dm%02ds\n' $(( elapsed / 60 )) $(( elapsed % 60 ))

run=$(( total - skipped ))
if (( run > 0 )); then
  ratio_passed="$(awk "BEGIN { printf \"%.2f\", $passed/$run }")"
  ratio_failed="$(awk "BEGIN { printf \"%.2f\", $failed/$run }")"
  printf '\nPassing ratio : %s\nFailing ratio : %s\n' "$ratio_passed" "$ratio_failed"
fi

if (( SAVE_DEBUG_OUTPUT && failed > 0 )); then
  printf '\nDebug output saved to: %s\n' "$DEBUG_DIR"
fi

(( failed == 0 )) || exit 1
