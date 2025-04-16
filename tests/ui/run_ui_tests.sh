#!/usr/bin/env bash

set -u

USAGE="Usage: $0 RUST_DIR_ROOT\n\n'RUST_DIR_ROOT' is the Rust directory to take ui tests from."

if [ $# -lt 1 ]; then
    echo -e "$USAGE"
    exit 1
fi

RUST_DIR_ROOT="$1"
UI_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
PASSING_TSV="${UI_DIR}/passing.tsv"

echo "Running regression tests for passing UI cases..."
failed=0

while read -r test; do
    test_path="${RUST_DIR_ROOT}/${test}"
    test_name="$(basename "$test" .rs)"
    json_file="${PWD}/${test_name}.smir.json"

    cargo run -- -Zno-codegen "$test_path" > /dev/null 2>&1
    status=$?

    if [ $status -ne 0 ]; then
        echo "❌ FAILED: $test_path (exit $status)"
        failed=1
    fi

    # Clean up generated JSON
    [ -f "$json_file" ] && rm -f "$json_file"
done < "$PASSING_TSV"

if [ $failed -ne 0 ]; then
    echo "Some regression tests FAILED."
    exit 1
else
    echo "All regression tests passed."
fi
