#!/usr/bin/env bash
#
# Ensures a rust checkout (regular or bare+worktree) is at the commit
# that backs the active nightly toolchain (derived from `rustc -vV`).
#
# Usage: source this script after setting RUST_DIR to the repo root.
# It sets RUST_SRC_DIR to the directory containing the source files
# (which may differ from RUST_DIR if a worktree is created).

set -u

: "${RUST_DIR:?RUST_DIR must be set before sourcing ensure_rustc_commit.sh}"

# Derive the rustc commit from the active toolchain; rust-toolchain.toml
# selects the nightly, so this stays in sync automatically.
# Run from the repo root so rustup picks up rust-toolchain.toml even when
# the caller's CWD is outside the repository.
_SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)
_REPO_ROOT=$(cd -- "$_SCRIPT_DIR/../.." &>/dev/null && pwd)
RUSTC_COMMIT=$(cd "$_REPO_ROOT" && rustc -vV | grep 'commit-hash' | cut -d' ' -f2)
if [ -z "$RUSTC_COMMIT" ]; then
    echo "Error: Could not determine rustc commit-hash from 'rustc -vV'"
    exit 1
fi

SHORT_COMMIT="${RUSTC_COMMIT:0:12}"

# Detect whether RUST_DIR is a bare repo
IS_BARE=$(git -C "$RUST_DIR" rev-parse --is-bare-repository 2>/dev/null)

if [ "$IS_BARE" = "true" ]; then
    # Bare repo: use worktrees. Check if one already exists at this commit.
    WORKTREE_DIR="$RUST_DIR/$SHORT_COMMIT"

    if [ -d "$WORKTREE_DIR" ]; then
        WORKTREE_COMMIT=$(git -C "$WORKTREE_DIR" rev-parse HEAD 2>/dev/null)
        if [ "${WORKTREE_COMMIT}" = "${RUSTC_COMMIT}" ]; then
            echo "Worktree already exists at ${WORKTREE_DIR} (${SHORT_COMMIT})"
            RUST_SRC_DIR="$WORKTREE_DIR"
        else
            echo "Error: Worktree at ${WORKTREE_DIR} is at wrong commit (${WORKTREE_COMMIT})"
            exit 1
        fi
    else
        # Ensure the commit is available locally; fetch if needed.
        if ! git -C "$RUST_DIR" cat-file -e "$RUSTC_COMMIT" 2>/dev/null; then
            echo "Commit ${SHORT_COMMIT} not found locally; fetching..."
            git -C "$RUST_DIR" fetch origin "$RUSTC_COMMIT" --quiet 2>/dev/null || \
            git -C "$RUST_DIR" fetch origin --quiet || {
                echo "Error: Could not fetch commit ${RUSTC_COMMIT}."
                echo "Ensure ${RUST_DIR} is a clone of https://github.com/rust-lang/rust"
                exit 1
            }
        fi
        echo "Creating worktree at ${WORKTREE_DIR} for commit ${SHORT_COMMIT}..."
        git -C "$RUST_DIR" worktree add "$WORKTREE_DIR" "$RUSTC_COMMIT" --detach --quiet || {
            echo "Error: Failed to create worktree for commit ${RUSTC_COMMIT} in ${RUST_DIR}"
            exit 1
        }
        RUST_SRC_DIR="$WORKTREE_DIR"
    fi
else
    # Regular repo: checkout the commit directly
    CURRENT_COMMIT=$(git -C "$RUST_DIR" rev-parse HEAD 2>/dev/null)
    if [ "${CURRENT_COMMIT}" != "${RUSTC_COMMIT}" ]; then
        # Ensure the commit is available locally; fetch if needed.
        if ! git -C "$RUST_DIR" cat-file -e "$RUSTC_COMMIT" 2>/dev/null; then
            echo "Commit ${SHORT_COMMIT} not found locally; fetching..."
            git -C "$RUST_DIR" fetch origin "$RUSTC_COMMIT" --quiet 2>/dev/null || \
            git -C "$RUST_DIR" fetch origin --quiet || {
                echo "Error: Could not fetch commit ${RUSTC_COMMIT}."
                echo "Ensure ${RUST_DIR} is a clone of https://github.com/rust-lang/rust"
                exit 1
            }
        fi
        echo "Checking out rustc commit ${SHORT_COMMIT} in ${RUST_DIR}..."
        git -C "$RUST_DIR" checkout "$RUSTC_COMMIT" --quiet || {
            echo "Error: Failed to checkout commit ${RUSTC_COMMIT} in ${RUST_DIR}"
            exit 1
        }
    else
        echo "Rust checkout already at expected commit ${SHORT_COMMIT}"
    fi
    RUST_SRC_DIR="$RUST_DIR"
fi

export RUST_SRC_DIR
