#!/usr/bin/env bash
#
# Ensures a rust checkout (regular or bare+worktree) is at the commit
# specified in rust-toolchain.toml's [metadata] rustc-commit field.
#
# Usage: source this script after setting RUST_DIR to the repo root.
# It sets RUST_SRC_DIR to the directory containing the source files
# (which may differ from RUST_DIR if a worktree is created).

set -u

: "${RUST_DIR:?RUST_DIR must be set before sourcing ensure_rustc_commit.sh}"

# Require yq (mikefarah/yq) for TOML parsing
if ! command -v yq &> /dev/null; then
    echo "Error: yq is required but not installed."
    echo "Install via: brew install yq | apt install yq | nix shell nixpkgs#yq-go"
    echo "See: https://github.com/mikefarah/yq#install"
    exit 1
fi

_SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
_REPO_ROOT=$( cd -- "$_SCRIPT_DIR/../.." &> /dev/null && pwd )

# Read the expected rustc commit from rust-toolchain.toml
RUSTC_COMMIT=$(yq -r '.metadata.rustc-commit' "$_REPO_ROOT/rust-toolchain.toml")
if [ -z "$RUSTC_COMMIT" ] || [ "$RUSTC_COMMIT" = "null" ]; then
    echo "Error: Could not read metadata.rustc-commit from $_REPO_ROOT/rust-toolchain.toml"
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
