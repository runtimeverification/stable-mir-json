# UI Tests

Regression tests drawn from the [Rust compiler UI test suite](https://github.com/rust-lang/rust/tree/master/tests/ui). We run a curated subset of rustc's UI tests through stable-mir-json and check that they process successfully (exit 0). A checkout of the rust compiler source is required.

## Quick start

```bash
# Run against the pinned nightly (uses base passing.tsv):
RUST_DIR_ROOT=/path/to/rust make test-ui

# Run against a different nightly (uses effective list if available):
RUSTUP_TOOLCHAIN=nightly-2025-03-01 RUST_DIR_ROOT=/path/to/rust make test-ui
```

## Directory layout

```
tests/ui/
├── base-nightly.txt              # which nightly the base lists were generated against
├── passing.tsv                   # base passing list (one test path per line)
├── failing.tsv                   # base failing list (path<TAB>exit_code)
│
├── overrides/
│   ├── nightly-2025-03-01.tsv    # manual overrides for behavior changes
│   └── nightly-2025-03-01/
│       ├── passing.tsv           # effective passing list (generated)
│       └── failing.tsv           # effective failing list (generated)
│
├── run_ui_tests.sh               # test runner
├── diff_test_lists.sh            # generates effective lists from base+delta
├── collect_test_sources.sh       # discovers candidate tests from rustc source
├── remake_ui_tests.sh            # regenerates base lists from scratch
├── ensure_rustc_commit.sh        # checks out the right rustc commit
├── rustc_mir.sh                  # helper: runs rustc to emit MIR (used by collect)
├── has_match.sh                  # helper: grep wrapper (used by collect)
└── ui_sources.txt                # intermediate output from collect_test_sources.sh
```

## How it works

The base lists (`passing.tsv`, `failing.tsv`) are the ground truth, generated against the nightly recorded in `base-nightly.txt`. They don't change when you target a different nightly.

For other nightlies, upstream test files may have been deleted, renamed, or modified. Rather than maintaining full copies of the lists per nightly (they're 99.5% identical), `diff_test_lists.sh` computes the delta:

1. **Deletions**: files removed upstream are dropped from the list
2. **Renames**: files that moved get their paths updated
3. **Manual overrides**: behavior changes that git can't detect (e.g., a test was rewritten to use syntax our driver doesn't handle) are recorded in `overrides/<nightly>.tsv`

`run_ui_tests.sh` detects the active nightly via `rustup show active-toolchain` and automatically uses the effective list from `overrides/<nightly>/` if it exists, falling back to the base list otherwise.

## Adding support for a new nightly

```bash
# 1. See what changed:
./tests/ui/diff_test_lists.sh /path/to/rust nightly-YYYY-MM-DD

# 2. If any tests have behavior changes, create a manual override file:
#    tests/ui/overrides/nightly-YYYY-MM-DD.tsv
#    Format: action<TAB>path
#    Actions: skip, -, +, fail (with exit code), pass

# 3. Generate the effective lists:
./tests/ui/diff_test_lists.sh --emit /path/to/rust nightly-YYYY-MM-DD

# 4. Run the tests:
RUSTUP_TOOLCHAIN=nightly-YYYY-MM-DD RUST_DIR_ROOT=/path/to/rust make test-ui

# 5. Commit the override file and generated lists.
```

## Regenerating the base lists

To regenerate from scratch against a new base nightly (only needed when re-baselining, not for routine nightly bumps):

```bash
# Discover candidate test sources:
RUST_TOP=/path/to/rust ./tests/ui/collect_test_sources.sh > tests/ui/ui_sources.txt

# Regenerate passing/failing lists:
./tests/ui/remake_ui_tests.sh /path/to/rust

# Update the base nightly marker:
echo "nightly-YYYY-MM-DD" > tests/ui/base-nightly.txt
```

## diff_test_lists.sh modes

| Mode | Description |
|------|-------------|
| `--report` (default) | Human-readable summary per nightly: deletions, renames, modifications, effective list size |
| `--chain` | Incremental diffs between consecutive breakpoint nightlies |
| `--emit` | Write effective `passing.tsv` and `failing.tsv` to `overrides/<nightly>/` |
