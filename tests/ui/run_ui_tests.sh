#!/usr/bin/env bash

set -u

USAGE="Usage: $0 RUST_DIR_ROOT [VERBOSE]\n
'RUST_DIR_ROOT' is the Rust directory to take ui tests from. Optional 'VERBOSE' can be set to '1' for verbose output."

if [ $# -lt 1 ]; then
    echo -e "$USAGE"
    exit 1
else
    VERBOSE="$2"
fi

RUST_DIR="$1"
UI_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

# Ensure the rust checkout is at the expected commit (handles bare repos)
source "$UI_DIR/ensure_rustc_commit.sh"

PASSING_TSV="${UI_DIR}/passing.tsv"

KEEP_FILES=${KEEP_FILES:-""}

if [ -z "${RUN_SMIR:-""}" ]; then
    echo "RUN_SMIR unset, using cargo to run the tests"
    RUN_SMIR="cargo run -- -Zno-codegen"
fi

echo "Running regression tests for passing UI cases..."
failed=0
passed=0
total=0

while read -r test; do
    test_path="${RUST_SRC_DIR}/${test}"
    test_name="$(basename "$test" .rs)"
    json_file="${PWD}/${test_name}.smir.json"

    ${RUN_SMIR} "$test_path" > /dev/null 2>&1
    status=$?

    total=$((total + 1))

    if [ $status -ne 0 ]; then
        echo "❌ FAILED: $test_path (exit $status)"
        failed=$((failed + 1))
    else
        if [ "$VERBOSE" -eq "1" ]; then
            echo "✅ PASSING: $test_path"
        fi
        passed=$((passed + 1))
    fi

    # Clean up generated JSON
    [ -z "$KEEP_FILES" ] && [ -f "$json_file" ] && rm -f "$json_file"
done < "$PASSING_TSV"

echo "—— Summary ——"
echo "Total tests : $total"
echo "Passed      : $passed"
echo "Failed      : $failed"

if [ $total -gt 0 ]; then
    # Calculate ratios as decimal fractions (e.g. 0.75)
    ratio_passed=$(awk "BEGIN { printf \"%.2f\", $passed/$total }")
    ratio_failed=$(awk "BEGIN { printf \"%.2f\", $failed/$total }")

    echo
    echo "Passing ratio : $ratio_passed"
    echo "Failing ratio : $ratio_failed"
fi

if [ $failed -gt 0 ]; then
    exit 1
fi
