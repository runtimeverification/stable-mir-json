RELEASE_FLAG=
TOOLCHAIN_NAME=

.DEFAULT_GOAL := build

.PHONY: help
## Show this help message
help:
	@echo "Available targets:"
	@awk 'BEGIN {FS = ":.*"; printf "\nUsage:\n  make \033[36m<target>\033[0m\n"} \
		/^###/ {printf "\n\033[1m%s\033[0m\n", substr($$0, 5); next} \
		/^##/ {description=substr($$0, 4)} \
		/^[a-zA-Z0-9_-]+:/ { \
			if (description) { \
				printf "  \033[36m%-18s\033[0m %s\n", $$1, description; \
				description = ""; \
			} \
		}' $(MAKEFILE_LIST)

### Build

.PHONY: build
## Build the project (use RELEASE_FLAG=--release for release)
build:
	cargo build $(RELEASE_FLAG)

.PHONY: clean
## Clean build artifacts, toolchain overrides, and graphs
clean: rustup-clear-toolchain clean-graphs
	cargo clean

.PHONY: rustup-clear-toolchain
rustup-clear-toolchain:
	rustup override unset
	rustup override unset --nonexistent
	rustup toolchain uninstall "$(TOOLCHAIN_NAME)"

### Test

TESTDIR=tests/integration/programs

# Detect the active nightly for golden-file lookup.
# The nightly name (e.g. nightly-2025-03-01) is derived from rustc's
# commit-date, which is one day before the nightly date.  We add one day
# to align with the toolchain channel name that users actually see.
NIGHTLY_COMMIT_DATE := $(shell rustc -vV 2>/dev/null | awk '/^commit-date:/{print $$2}')
# Portable +1 day: macOS date(1) uses -v+1d, GNU date uses -d "+1 day".
NIGHTLY_DATE := $(shell \
	if [ -n "$(NIGHTLY_COMMIT_DATE)" ]; then \
		date -j -v+1d -f "%Y-%m-%d" "$(NIGHTLY_COMMIT_DATE)" "+%Y-%m-%d" 2>/dev/null \
		|| date -d "$(NIGHTLY_COMMIT_DATE) +1 day" "+%Y-%m-%d" 2>/dev/null \
		|| echo "$(NIGHTLY_COMMIT_DATE)"; \
	fi)
ACTIVE_NIGHTLY := nightly-$(NIGHTLY_DATE)

# Golden file directory: per-nightly expected outputs.
# Falls back to the pinned nightly (from rust-toolchain.toml) if no
# directory exists for the active nightly.
PINNED_NIGHTLY := $(shell awk -F'"' '/^channel/{print $$2}' rust-toolchain.toml)
GOLDEN_BASE    := tests/integration/expected
GOLDEN_DIR     := $(shell \
	if [ -d "$(GOLDEN_BASE)/$(ACTIVE_NIGHTLY)" ]; then \
		echo "$(GOLDEN_BASE)/$(ACTIVE_NIGHTLY)"; \
	else \
		echo "$(GOLDEN_BASE)/$(PINNED_NIGHTLY)"; \
	fi)

.PHONY: integration-test
integration-test: TESTS     ?= $(shell find $(TESTDIR) -type f -name "*.rs")
integration-test: SMIR      ?= cargo run -- "-Zno-codegen"
# override this to tweak how expectations are formatted
integration-test: NORMALIZE ?= jq -S -e -f $(TESTDIR)/../normalise-filter.jq
# override this to re-make golden files
integration-test: DIFF      ?= | diff -
## Run integration tests against expected outputs
integration-test:
	@echo "Using golden files from: $(GOLDEN_DIR)"
	errors=""; \
	report() { echo "$$1: $$2"; errors="$$errors\n$$1: $$2"; }; \
	for rust in $(TESTS); do \
		target=$${rust%.rs}.smir.json; \
		name=$$(basename $${rust%.rs}); \
		dir=$$(dirname $${rust}); \
		expected="$(GOLDEN_DIR)/$${name}.smir.json.expected"; \
		echo "$$rust"; \
		$(SMIR) --out-dir $${dir} $${rust} || report "$$rust" "Conversion failed"; \
		[ -f $${target} ] \
			&& $(NORMALIZE) $${target} $(DIFF) $${expected} \
			&& rm $${target} \
			|| report "$$rust" "Unexpected json output"; \
		done; \
	[ -z "$$errors" ] || (echo "===============\nFAILING TESTS:$$errors"; exit 1)

.PHONY: golden
## Regenerate expected test outputs (golden files) for the active nightly
golden:
	@mkdir -p "$(GOLDEN_BASE)/$(ACTIVE_NIGHTLY)"
	$(MAKE) integration-test DIFF=">" GOLDEN_DIR="$(GOLDEN_BASE)/$(ACTIVE_NIGHTLY)"

.PHONY: remake-ui-tests
## Regenerate UI test fixtures (requires RUST_DIR_ROOT)
remake-ui-tests:
	# Check if RUST_DIR_ROOT is set
	if [ -z "$$RUST_DIR_ROOT" ]; then \
	  echo "Error: RUST_DIR_ROOT is not set. Please set it to the absolute path to rust compiler checkout."; \
	  exit 1; \
	fi
	# This will run without saving source files. Run the script manually to do this.
	bash tests/ui/remake_ui_tests.sh "$$RUST_DIR_ROOT"

.PHONY: test-ui
## Run UI tests (requires RUST_DIR_ROOT, VERBOSE=1 for details)
test-ui: VERBOSE?=0
test-ui:
	bash tests/ui/run_ui_tests.sh $(if $(filter 1,$(VERBOSE)),--verbose) "$$RUST_DIR_ROOT"

.PHONY: test-directives
## Run unit tests for the directive parser (parse_test_directives.awk)
test-directives:
	bash tests/ui/test_directives_test.sh

.PHONY: test-ui-emit
## Generate effective UI test lists for a nightly (requires RUST_DIR_ROOT, NIGHTLY=nightly-YYYY-MM-DD)
test-ui-emit:
	bash tests/ui/diff_test_lists.sh --emit "$$RUST_DIR_ROOT" $(NIGHTLY)

### Nightly management

.PHONY: nightly-add
## Add support for a new nightly (requires NIGHTLY, RUST_DIR_ROOT)
nightly-add:
	@test -n "$$NIGHTLY" || { echo "Error: NIGHTLY not set (e.g., NIGHTLY=nightly-2025-08-01)"; exit 1; }
	@test -n "$$RUST_DIR_ROOT" || { echo "Error: RUST_DIR_ROOT not set"; exit 1; }
	python3 scripts/nightly_admin.py add "$$NIGHTLY" --rust-dir "$$RUST_DIR_ROOT"

.PHONY: nightly-check
## Run all tests for a nightly (requires NIGHTLY, RUST_DIR_ROOT)
nightly-check:
	@test -n "$$NIGHTLY" || { echo "Error: NIGHTLY not set"; exit 1; }
	@test -n "$$RUST_DIR_ROOT" || { echo "Error: RUST_DIR_ROOT not set"; exit 1; }
	python3 scripts/nightly_admin.py check "$$NIGHTLY" --rust-dir "$$RUST_DIR_ROOT"

.PHONY: nightly-bump
## Bump the pinned nightly (requires NIGHTLY)
nightly-bump:
	@test -n "$$NIGHTLY" || { echo "Error: NIGHTLY not set"; exit 1; }
	python3 scripts/nightly_admin.py bump "$$NIGHTLY"

### Diagnostics

.PHONY: build-info
## Show build.rs cfg detection output (rustc commit-date, enabled flags)
build-info:
	@touch build.rs
	@cargo build -vv 2>&1 | grep '\] build\.rs:'

### Code quality

.PHONY: fmt format
## Format Rust and Nix source files
fmt format:
	cargo fmt
	bash -O globstar -c 'nixfmt **/*.nix'

.PHONY: clippy
## Run clippy lint checks (deny warnings)
clippy:
	cargo clippy -- -Dwarnings

.PHONY: style-check
## Run format + clippy lint checks
style-check: format clippy

### Graph generation

OUTDIR_DOT=output-dot
OUTDIR_SVG=output-svg
OUTDIR_PNG=output-png
OUTDIR_D2=output-d2

.PHONY: check-graphviz
check-graphviz:
	@command -v dot >/dev/null 2>&1 || { \
		echo "Error: Graphviz is not installed or 'dot' is not in PATH."; \
		echo "Please install Graphviz for your system and ensure 'dot' is available."; \
		echo "See: https://graphviz.org/download/"; \
		exit 1; \
	}

.PHONY: dot
## Generate DOT files from test programs
dot:
	@mkdir -p $(OUTDIR_DOT)
	@for rs in $(TESTDIR)/*.rs; do \
		name=$$(basename $$rs .rs); \
		echo "Generating $$name.smir.dot"; \
		cargo run --release -- --dot -Zno-codegen $$rs 2>/dev/null; \
		mv $$name.smir.dot $(OUTDIR_DOT)/ 2>/dev/null || true; \
	done

.PHONY: svg
## Generate SVG files from DOT (requires graphviz)
svg: check-graphviz dot
	@mkdir -p $(OUTDIR_SVG)
	@for dotfile in $(OUTDIR_DOT)/*.dot; do \
		name=$$(basename $$dotfile .dot); \
		echo "Converting $$name.dot -> $$name.svg"; \
		dot -Tsvg $$dotfile -o $(OUTDIR_SVG)/$$name.svg; \
	done

.PHONY: png
## Generate PNG files from DOT (requires graphviz)
png: check-graphviz dot
	@mkdir -p $(OUTDIR_PNG)
	@for dotfile in $(OUTDIR_DOT)/*.dot; do \
		name=$$(basename $$dotfile .dot); \
		echo "Converting $$name.dot -> $$name.png"; \
		dot -Tpng $$dotfile -o $(OUTDIR_PNG)/$$name.png; \
	done

.PHONY: d2
## Generate D2 diagram files from test programs
d2:
	@mkdir -p $(OUTDIR_D2)
	@for rs in $(TESTDIR)/*.rs; do \
		name=$$(basename $$rs .rs); \
		echo "Generating $$name.smir.d2"; \
		cargo run --release -- --d2 -Zno-codegen $$rs 2>/dev/null; \
		mv $$name.smir.d2 $(OUTDIR_D2)/ 2>/dev/null || true; \
	done

.PHONY: clean-graphs
## Remove generated graph output directories
clean-graphs:
	@rm -rf $(OUTDIR_DOT) $(OUTDIR_SVG) $(OUTDIR_PNG) $(OUTDIR_D2)

### stdlib smir.json

STDLIB_OUTDIR=tests/stdlib-artifacts
STDLIB_TARGET=$(shell rustc --print target-triple 2>/dev/null || rustc -vV | grep host | awk '{print $$2}')
SYSROOT=$(shell rustc --print sysroot)
SMIR_BIN=$(CURDIR)/target/debug/stable_mir_json

.PHONY: stdlib-smir
## Generate smir.json for stdlib via -Zbuild-std
stdlib-smir: build
	@# Create a throwaway crate to drive -Zbuild-std
	$(eval STDLIB_TMPDIR := $(shell mktemp -d))
	@echo '[package]'                      >  $(STDLIB_TMPDIR)/Cargo.toml
	@echo 'name = "stdlib-smir"'           >> $(STDLIB_TMPDIR)/Cargo.toml
	@echo 'version = "0.0.0"'             >> $(STDLIB_TMPDIR)/Cargo.toml
	@echo 'edition = "2021"'              >> $(STDLIB_TMPDIR)/Cargo.toml
	@echo '[[bin]]'                        >> $(STDLIB_TMPDIR)/Cargo.toml
	@echo 'name = "stdlib-smir"'           >> $(STDLIB_TMPDIR)/Cargo.toml
	@echo 'path = "main.rs"'              >> $(STDLIB_TMPDIR)/Cargo.toml
	@echo 'fn main() {}'                   >  $(STDLIB_TMPDIR)/main.rs
	@# Build stdlib through our driver; set library path the same way
	@# cargo does when it runs our binary via `cargo run`
	cd $(STDLIB_TMPDIR) && \
		DYLD_LIBRARY_PATH=$(SYSROOT)/lib \
		LD_LIBRARY_PATH=$(SYSROOT)/lib \
		RUSTC=$(SMIR_BIN) \
		cargo build -Zbuild-std --target $(STDLIB_TARGET)
	@# Collect artifacts, stripping hash suffixes from filenames
	@rm -rf $(STDLIB_OUTDIR)
	@mkdir -p $(STDLIB_OUTDIR)
	@for f in $(STDLIB_TMPDIR)/target/$(STDLIB_TARGET)/debug/deps/*.smir.json; do \
		name=$$(basename "$$f" | sed 's/-[0-9a-f]*\.smir\.json/.smir.json/'); \
		case "$$name" in stdlib_smir*) continue ;; esac; \
		cp "$$f" $(STDLIB_OUTDIR)/$$name; \
	done
	@rm -rf $(STDLIB_TMPDIR)
	@echo "stdlib smir.json artifacts written to $(STDLIB_OUTDIR)/"
	@ls -lhS $(STDLIB_OUTDIR)/

.PHONY: clean-stdlib-smir
## Remove stdlib smir.json artifacts
clean-stdlib-smir:
	@rm -rf $(STDLIB_OUTDIR)
