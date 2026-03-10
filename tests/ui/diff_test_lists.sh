#!/usr/bin/env bash
set -euo pipefail

# diff_test_lists.sh: generate effective UI test lists for a target nightly
# by diffing tests/ui/ in the rustc repo between the base commit and the
# target commit, then applying deletions, renames, and manual overrides
# to the base passing.tsv / failing.tsv.
#
# This script is the single source of truth for "which tests should we
# run against nightly X?" Its output is auditable and reproducible: given
# the same rust repo and commits, it produces the same result.
#
# Modes:
#   --report     Print a human-readable diff report (default)
#   --emit       Write effective passing.tsv and failing.tsv for the
#                target nightly to the overrides directory
#
# Usage:
#   ./tests/ui/diff_test_lists.sh RUST_DIR [OPTIONS] [NIGHTLY...]
#
# If no nightlies are specified, the script walks through the default
# breakpoint nightlies (those with installed toolchains).

usage() {
  cat <<'EOF'
Usage: diff_test_lists.sh [OPTIONS] RUST_DIR [NIGHTLY...]

  RUST_DIR    Path to a rust-lang/rust checkout (regular or bare)
  NIGHTLY     One or more nightly dates (e.g., nightly-2025-03-01).
              If omitted, walks through all breakpoint nightlies.

Options:
  --report    Print a human-readable report to stdout (default)
  --emit      Write effective test lists to tests/ui/overrides/<nightly>/
  --chain     Show incremental diffs between each consecutive nightly

Environment:
  BASE_COMMIT   Override the base commit (default: from base-nightly.txt)
EOF
  exit 1
}

die() { printf 'Error: %s\n' "$*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# Arg parsing
# ---------------------------------------------------------------------------
MODE="report"
RUST_DIR=""
NIGHTLY_ARGS=()

while (( $# > 0 )); do
  case "$1" in
    --report) MODE="report"; shift ;;
    --emit)   MODE="emit";   shift ;;
    --chain)  MODE="chain";  shift ;;
    --help|-h) usage ;;
    --*) die "unknown option: $1" ;;
    *)
      if [[ -z "$RUST_DIR" ]]; then
        RUST_DIR="$1"
      else
        NIGHTLY_ARGS+=("$1")
      fi
      shift ;;
  esac
done

[[ -n "$RUST_DIR" ]] || usage
[[ -d "$RUST_DIR" ]] || die "not a directory: $RUST_DIR"

UI_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
PASSING_TSV="${UI_DIR}/passing.tsv"
FAILING_TSV="${UI_DIR}/failing.tsv"
OVERRIDES_DIR="${UI_DIR}/overrides"

[[ -f "$PASSING_TSV" ]] || die "missing $PASSING_TSV"
[[ -f "$FAILING_TSV" ]] || die "missing $FAILING_TSV"

# ---------------------------------------------------------------------------
# Resolve base commit
# ---------------------------------------------------------------------------
if [[ -z "${BASE_COMMIT:-}" ]]; then
  if [[ -f "${UI_DIR}/base-nightly.txt" ]]; then
    BASE_NIGHTLY=$(head -1 "${UI_DIR}/base-nightly.txt" | tr -d '[:space:]')
    BASE_COMMIT=$(rustup run "$BASE_NIGHTLY" rustc -vV 2>/dev/null \
      | grep 'commit-hash' | cut -d' ' -f2 || true)
    [[ -n "$BASE_COMMIT" ]] || die "could not resolve commit for $BASE_NIGHTLY (is it installed?)"
  else
    BASE_COMMIT=$(rustc -vV | grep 'commit-hash' | cut -d' ' -f2)
    BASE_NIGHTLY="(current)"
  fi
else
  BASE_NIGHTLY="(override)"
fi

# ---------------------------------------------------------------------------
# Resolve nightly list -> (label, commit) pairs
# ---------------------------------------------------------------------------
DEFAULT_NIGHTLIES="nightly-2024-12-15 nightly-2025-01-25 nightly-2025-01-28 nightly-2025-01-29 nightly-2025-03-01 nightly-2025-07-11 nightly-2025-07-15 nightly-2025-07-26 nightly-2025-09-19 nightly-2025-10-03 nightly-2025-10-12 nightly-2025-11-19 nightly-2025-12-06 nightly-2025-12-14"

if (( ${#NIGHTLY_ARGS[@]} > 0 )); then
  NIGHTLY_LIST=("${NIGHTLY_ARGS[@]}")
else
  read -ra NIGHTLY_LIST <<< "$DEFAULT_NIGHTLIES"
fi

COMMITS=()
LABELS=()

for n in "${NIGHTLY_LIST[@]}"; do
  hash=$(rustup run "$n" rustc -vV 2>/dev/null \
    | grep 'commit-hash' | cut -d' ' -f2 || true)
  if [[ -z "$hash" ]]; then
    echo "# warning: $n not installed, skipping" >&2
    continue
  fi
  git -C "$RUST_DIR" cat-file -e "$hash" 2>/dev/null || {
    echo "# warning: commit for $n ($hash) not in $RUST_DIR, skipping" >&2
    continue
  }
  COMMITS+=("$hash")
  LABELS+=("$n")
done

(( ${#COMMITS[@]} > 0 )) || die "no target commits resolved"

# ---------------------------------------------------------------------------
# Load base test lists
# ---------------------------------------------------------------------------
declare -A BASE_PASSING
while IFS= read -r line; do
  [[ -n "$line" ]] && BASE_PASSING["$line"]=1
done < "$PASSING_TSV"

declare -A BASE_FAILING
while IFS=$'\t' read -r path code; do
  [[ -n "$path" ]] && BASE_FAILING["$path"]="${code:-1}"
done < "$FAILING_TSV"

# ---------------------------------------------------------------------------
# Compute cumulative diff from base to a target commit
#
# Sets these arrays in the caller's scope:
#   GIT_DELETED   - files deleted upstream
#   GIT_ADDED     - files added upstream
#   GIT_RENAMED   - tab-separated old<TAB>new pairs
#   GIT_MODIFIED  - files with content changes
# ---------------------------------------------------------------------------
compute_diff() {
  local from="$1" to="$2"

  # Use a high rename limit to avoid truncated rename detection on large diffs.
  # Without this, git silently falls back to treating renames as delete+add,
  # which shrinks the effective test list by dropping renamed tests.
  local -a diff_cmd=(git -C "$RUST_DIR" -c diff.renameLimit=5000 diff)

  mapfile -t GIT_DELETED < <(
    "${diff_cmd[@]}" --diff-filter=D --name-only "$from..$to" -- tests/ui/ \
    | grep '\.rs$' || true
  )

  mapfile -t GIT_ADDED < <(
    "${diff_cmd[@]}" --diff-filter=A --name-only "$from..$to" -- tests/ui/ \
    | grep '\.rs$' || true
  )

  mapfile -t GIT_RENAMED < <(
    "${diff_cmd[@]}" --diff-filter=R --name-status "$from..$to" -- tests/ui/ \
    | grep '\.rs' \
    | awk '{print $2 "\t" $3}' || true
  )

  mapfile -t GIT_MODIFIED < <(
    "${diff_cmd[@]}" --diff-filter=M --name-only "$from..$to" -- tests/ui/ \
    | grep '\.rs$' || true
  )
}

# ---------------------------------------------------------------------------
# Build effective test list for a target commit
#
# Starts from BASE_PASSING, applies git deletions and renames, then
# applies manual overrides from overrides/<nightly>.tsv if it exists.
#
# Outputs the effective passing list (one path per line, sorted) to stdout.
# ---------------------------------------------------------------------------
build_effective_passing() {
  local target_commit="$1" label="$2"

  compute_diff "$BASE_COMMIT" "$target_commit"

  # Start with a copy of the base passing list
  declare -A effective
  for path in "${!BASE_PASSING[@]}"; do
    effective["$path"]=1
  done

  # Remove deleted files.
  # N.B.: we mark entries as empty rather than using `unset`, because bash
  # evaluates the subscript in `unset "arr[$key]"` and chokes on filenames
  # containing $ (e.g., need-crate-arg-ignore-tidy$x.rs).
  for f in "${GIT_DELETED[@]}"; do
    effective["$f"]=""
  done

  # Handle renames: remove old path, add new path
  for entry in "${GIT_RENAMED[@]}"; do
    local old="${entry%%	*}" new="${entry##*	}"
    if [[ -n "${effective[$old]:-}" ]]; then
      effective["$old"]=""
      effective["$new"]=1
    fi
  done

  # Apply manual overrides if they exist
  local override_file="${OVERRIDES_DIR}/${label}.tsv"
  if [[ -f "$override_file" ]]; then
    while IFS=$'\t' read -r action path _rest; do
      [[ -z "$action" || "$action" == \#* ]] && continue
      case "$action" in
        -)    effective["$path"]="" ;;
        +)    effective["$path"]=1 ;;
        skip) effective["$path"]="" ;;
      esac
    done < "$override_file"
  fi

  # Output sorted (skip empty-value entries, which were deleted/skipped).
  # The `|| true` prevents a false-last-iteration from triggering set -e.
  for path in "${!effective[@]}"; do
    [[ -n "${effective[$path]}" ]] && printf '%s\n' "$path" || true
  done | sort
}

# ---------------------------------------------------------------------------
# Build effective failing list for a target commit
# ---------------------------------------------------------------------------
build_effective_failing() {
  local target_commit="$1" label="$2"

  # Start with base failing list
  declare -A effective
  for path in "${!BASE_FAILING[@]}"; do
    effective["$path"]="${BASE_FAILING[$path]}"
  done

  # Remove deleted files (mark empty; see note in build_effective_passing)
  for f in "${GIT_DELETED[@]}"; do
    effective["$f"]=""
  done

  # Handle renames
  for entry in "${GIT_RENAMED[@]}"; do
    local old="${entry%%	*}" new="${entry##*	}"
    if [[ -n "${effective[$old]:-}" ]]; then
      local code="${effective[$old]}"
      effective["$old"]=""
      effective["$new"]="$code"
    fi
  done

  # Apply manual overrides
  local override_file="${OVERRIDES_DIR}/${label}.tsv"
  if [[ -f "$override_file" ]]; then
    while IFS=$'\t' read -r action path rest; do
      [[ -z "$action" || "$action" == \#* ]] && continue
      case "$action" in
        -)    effective["$path"]="" ;;
        fail) effective["$path"]="${rest:-1}" ;;
        pass) effective["$path"]="" ;;
      esac
    done < "$override_file"
  fi

  # Output sorted (path<TAB>exit_code), skipping empty-value entries
  for path in "${!effective[@]}"; do
    [[ -n "${effective[$path]}" ]] && printf '%s\t%s\n' "$path" "${effective[$path]}" || true
  done | sort
}

# ---------------------------------------------------------------------------
# Report mode: human-readable diff report
# ---------------------------------------------------------------------------
print_report() {
  local target_commit="$1" label="$2"

  compute_diff "$BASE_COMMIT" "$target_commit"

  echo "=========================================="
  echo "## ${BASE_NIGHTLY} -> ${label}"
  echo "##   ${BASE_COMMIT:0:12}..${target_commit:0:12}"
  echo "=========================================="
  echo ""
  printf "  Upstream: %d deleted, %d added, %d renamed, %d modified\n\n" \
    "${#GIT_DELETED[@]}" "${#GIT_ADDED[@]}" "${#GIT_RENAMED[@]}" "${#GIT_MODIFIED[@]}"

  local del_p=0 ren_p=0 mod_p=0

  for f in "${GIT_DELETED[@]}"; do
    [[ -n "${BASE_PASSING[$f]:-}" ]] && { echo "  - $f"; (( ++del_p )); }
  done

  for entry in "${GIT_RENAMED[@]}"; do
    local old="${entry%%	*}" new="${entry##*	}"
    [[ -n "${BASE_PASSING[$old]:-}" ]] && { echo "  R $old -> $new"; (( ++ren_p )); }
  done

  for f in "${GIT_MODIFIED[@]}"; do
    [[ -n "${BASE_PASSING[$f]:-}" ]] && (( ++mod_p ))
  done

  local override_file="${OVERRIDES_DIR}/${label}.tsv"
  local has_overrides=0
  if [[ -f "$override_file" ]]; then
    has_overrides=1
    echo ""
    echo "  Manual overrides (${override_file##*/}):"
    while IFS= read -r line; do
      [[ -z "$line" || "$line" == \#* ]] && continue
      echo "    $line"
    done < "$override_file"
  fi

  echo ""
  printf "  Affecting passing.tsv: %d deleted, %d renamed, %d modified\n" "$del_p" "$ren_p" "$mod_p"
  printf "  New .rs files upstream: %d\n" "${#GIT_ADDED[@]}"
  (( has_overrides )) && echo "  Manual overrides: yes" || echo "  Manual overrides: none"

  # Effective list size
  local effective_count
  effective_count=$(build_effective_passing "$target_commit" "$label" | wc -l)
  printf "  Effective passing list: %d entries\n" "$effective_count"
  echo ""
}

# ---------------------------------------------------------------------------
# Chain mode: incremental diffs between consecutive nightlies
# ---------------------------------------------------------------------------
print_chain() {
  local prev_commit="$BASE_COMMIT" prev_label="$BASE_NIGHTLY"

  for i in "${!COMMITS[@]}"; do
    local target="${COMMITS[$i]}" label="${LABELS[$i]}"

    compute_diff "$prev_commit" "$target"

    echo "=========================================="
    echo "## ${prev_label} -> ${label}"
    echo "##   ${prev_commit:0:12}..${target:0:12}"
    echo "=========================================="
    echo ""
    printf "  Upstream: %d deleted, %d added, %d renamed, %d modified\n\n" \
      "${#GIT_DELETED[@]}" "${#GIT_ADDED[@]}" "${#GIT_RENAMED[@]}" "${#GIT_MODIFIED[@]}"

    for f in "${GIT_DELETED[@]}"; do
      [[ -n "${BASE_PASSING[$f]:-}" ]] && echo "  - $f"
    done
    for entry in "${GIT_RENAMED[@]}"; do
      local old="${entry%%	*}" new="${entry##*	}"
      [[ -n "${BASE_PASSING[$old]:-}" ]] && echo "  R $old -> $new"
    done

    local mod_p=0
    for f in "${GIT_MODIFIED[@]}"; do
      [[ -n "${BASE_PASSING[$f]:-}" ]] && (( ++mod_p ))
    done
    (( mod_p > 0 )) && printf "  (%d modified files in passing.tsv)\n" "$mod_p"

    echo ""
    prev_commit="$target"
    prev_label="$label"
  done
}

# ---------------------------------------------------------------------------
# Header (all modes)
# ---------------------------------------------------------------------------
echo "# UI Test List Diff Report"
echo "#"
echo "# Base: ${BASE_NIGHTLY} (${BASE_COMMIT:0:12})"
echo "# Targets: ${LABELS[*]}"
echo "# Generated: $(date -u '+%Y-%m-%d %H:%M:%S UTC')"
echo "#"
echo "# Base passing.tsv: $(wc -l < "$PASSING_TSV") entries"
echo "# Base failing.tsv: $(wc -l < "$FAILING_TSV") entries"
echo ""

# ---------------------------------------------------------------------------
# Dispatch
# ---------------------------------------------------------------------------
case "$MODE" in
  report)
    for i in "${!COMMITS[@]}"; do
      print_report "${COMMITS[$i]}" "${LABELS[$i]}"
    done
    ;;

  chain)
    print_chain
    ;;

  emit)
    mkdir -p "$OVERRIDES_DIR"
    for i in "${!COMMITS[@]}"; do
      local_label="${LABELS[$i]}"
      local_commit="${COMMITS[$i]}"
      out_dir="${OVERRIDES_DIR}/${local_label}"
      mkdir -p "$out_dir"

      echo "Generating effective lists for ${local_label}..."

      build_effective_passing "$local_commit" "$local_label" > "${out_dir}/passing.tsv"
      build_effective_failing "$local_commit" "$local_label" > "${out_dir}/failing.tsv"

      p_count=$(wc -l < "${out_dir}/passing.tsv")
      f_count=$(wc -l < "${out_dir}/failing.tsv")
      printf "  -> %s/passing.tsv (%d entries)\n" "$out_dir" "$p_count"
      printf "  -> %s/failing.tsv (%d entries)\n" "$out_dir" "$f_count"
    done
    echo ""
    echo "Done. Use these lists with run_ui_tests.sh by setting:"
    echo "  PASSING_TSV=tests/ui/overrides/<nightly>/passing.tsv"
    ;;
esac
