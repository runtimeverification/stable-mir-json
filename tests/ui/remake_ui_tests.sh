#!/usr/bin/env bash

set -u

USAGE="Usage: $0 RUST_DIR_ROOT [y|n]\n
'RUST_DIR_ROOT' is the Rust directory to take ui tests from. Optional arg 'y|n' is whether to keep *.smir.json and source files for analysis (default 'n')."

if [ $# -lt 1 ]; then
    echo -e "$USAGE"
    exit 1
elif [ $# -lt 2 ]; then
    KEEP_OUTPUT= # Default to not saving output
else
    case "$2" in
        y) KEEP_OUTPUT=1 ;;
        n) KEEP_OUTPUT= ;;
        *) exit 2 ;;
    esac
fi

RUST_DIR="$1"
UI_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
UI_SOURCES="${UI_DIR}/ui_sources.txt"
FAILING_TSV="${UI_DIR}/failing.tsv"
PASSING_TSV="${UI_DIR}/passing.tsv"
FAILING_DIR="${UI_DIR}/failing"
PASSING_DIR="${UI_DIR}/passing"

echo "Resetting UI test directories and TSVs..."
rm -f "$FAILING_TSV" "$PASSING_TSV"
touch "$FAILING_TSV" "$PASSING_TSV"

if [ -n "${KEEP_OUTPUT}" ]; then
    rm -rf "$FAILING_DIR" "$PASSING_DIR"
    mkdir -p "$FAILING_DIR" "$PASSING_DIR"
fi

echo "Running UI tests..."
while read -r test; do
    full_path="$RUST_DIR/$test"

    if [ ! -f "$full_path" ]; then
        echo "Error: Test file '$full_path' not found."
        exit 3 # The test files should always be there
    fi

    echo "Running test: $test"
    cargo run -- -Zno-codegen "$full_path" > tmp.stdout 2> tmp.stderr
    status=$?
    base_test="$(basename "$test")"
    json_file="${PWD}/$(basename "$test" .rs).smir.json"

    if [ "$status" -ne 0 ]; then
        echo "Test $test FAILED with exit code $status"
        echo -e "$test\t$status" >> "$FAILING_TSV"
        if [ -n "${KEEP_OUTPUT}" ]; then
            cp "$full_path" "$FAILING_DIR/$base_test"
            cp tmp.stdout "$FAILING_DIR/$base_test.stdout"
            cp tmp.stderr "$FAILING_DIR/$base_test.stderr"
        else
            rm -f tmp.stdout tmp.stderr
        fi
    else
        echo "Test $test PASSED"
        echo "$test" >> "$PASSING_TSV"
        if [ -n "${KEEP_OUTPUT}" ]; then
            cp "$full_path" "$PASSING_DIR/$base_test"
            if [ -f "$json_file" ]; then
                mv "$json_file" "$PASSING_DIR/$(basename "$json_file")"
            fi
        else
            rm -f "$json_file" tmp.stderr tmp.stdout
        fi
    fi
done < "$UI_SOURCES"

echo "Sorting TSV files..."
[ -s "$FAILING_TSV" ] && LC_ALL=C sort "$FAILING_TSV" -o "$FAILING_TSV"
[ -s "$PASSING_TSV" ] && LC_ALL=C sort "$PASSING_TSV" -o "$PASSING_TSV"

echo "UI tests remade."
