#!/usr/bin/env bash
# Run TRACE=1 across integration tests and aggregate event coverage.
#
# Usage: trace-report.sh <testdir> <smir_cmd...>
# Example: trace-report.sh tests/integration/programs cargo run -- -Zno-codegen

set -euo pipefail

testdir="$1"; shift
smir=("$@")

tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT

for rust in "$testdir"/*.rs; do
	name=$(basename "$rust" .rs)
	echo "Tracing $name..."
	TRACE=1 "${smir[@]}" --out-dir "$tmpdir" "$rust" 2>/dev/null
done

script_dir="$(cd "$(dirname "$0")" && pwd)"
python3 "$script_dir/trace-report.py" "$tmpdir" "$testdir"
