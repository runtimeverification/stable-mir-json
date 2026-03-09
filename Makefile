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

.PHONY: integration-test
integration-test: TESTS     ?= $(shell find $(TESTDIR) -type f -name "*.rs")
integration-test: SMIR      ?= cargo run -- "-Zno-codegen"
# override this to tweak how expectations are formatted
integration-test: NORMALIZE ?= jq -S -e -f $(TESTDIR)/../normalise-filter.jq
# override this to re-make golden files
integration-test: DIFF      ?= | diff -
## Run integration tests against expected outputs
integration-test:
	errors=""; \
	report() { echo "$$1: $$2"; errors="$$errors\n$$1: $$2"; }; \
	for rust in $(TESTS); do \
		target=$${rust%.rs}.smir.json; \
		dir=$$(dirname $${rust}); \
		echo "$$rust"; \
		$(SMIR) --out-dir $${dir} $${rust} || report "$$rust" "Conversion failed"; \
		[ -f $${target} ] \
			&& $(NORMALIZE) $${target} $(DIFF) $${target}.expected \
			&& rm $${target} \
			|| report "$$rust" "Unexpected json output"; \
		done; \
	[ -z "$$errors" ] || (echo "===============\nFAILING TESTS:$$errors"; exit 1)

.PHONY: golden
## Regenerate expected test outputs (golden files)
golden:
	$(MAKE) integration-test DIFF=">"

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

### Diagnostics

.PHONY: build-info
## Show build.rs cfg detection output (rustc commit-date, enabled flags)
build-info:
	@touch build.rs
	@cargo build -vv 2>&1 | grep '\] build\.rs:'

### Code quality

.PHONY: format
## Format Rust and Nix source files
format:
	cargo fmt
	bash -O globstar -c 'nixfmt **/*.nix'

.PHONY: style-check
## Run format + clippy lint checks
style-check: format
	cargo clippy -- -Dwarnings

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
