RELEASE_FLAG=
TOOLCHAIN_NAME=''

default: build

build:
	cargo build ${RELEASE_FLAG}

clean: rustup-clear-toolchain clean-graphs
	cargo clean

.PHONY: rustup-clear-toolchain
rustup-clear-toolchain:
	rustup override unset
	rustup override unset --nonexistent
	rustup toolchain uninstall "${TOOLCHAIN_NAME}"

TESTDIR=$(CURDIR)/tests/integration/programs

.PHONY: integration-test
integration-test: TESTS     ?= $(shell find $(TESTDIR) -type f -name "*.rs")
integration-test: SMIR      ?= cargo run -- "-Zno-codegen"
# override this to tweak how expectations are formatted
integration-test: NORMALIZE ?= jq -S -e -f $(TESTDIR)/../normalise-filter.jq
# override this to re-make golden files
integration-test: DIFF      ?= | diff -
integration-test:
	errors=""; \
	report() { echo "$$1: $$2"; errors="$$errors\n$$1: $$2"; }; \
	for rust in ${TESTS}; do \
		target=$${rust%.rs}.smir.json; \
		dir=$$(dirname $${rust}); \
		echo "$$rust"; \
		${SMIR} --out-dir $${dir} $${rust} || report "$$rust" "Conversion failed"; \
		[ -f $${target} ] \
			&& ${NORMALIZE} $${target} ${DIFF} $${target}.expected \
			&& rm $${target} \
			|| report "$$rust" "Unexpected json output"; \
		done; \
	[ -z "$$errors" ] || (echo "===============\nFAILING TESTS:$$errors"; exit 1)


golden:
	make integration-test DIFF=">"

format: 
	cargo fmt
	bash -O globstar -c 'nixfmt **/*.nix'

style-check: format
	cargo clippy

.PHONY: remake-ui-tests test-ui

remake-ui-tests:
	# Check if RUST_DIR_ROOT is set
	if [ -z "$$RUST_DIR_ROOT" ]; then \
	  echo "Error: RUST_DIR_ROOT is not set. Please set it to the absolute path to rust compiler checkout."; \
	  exit 1; \
	fi
	# This will run without saving source files. Run the script manually to do this.
	bash tests/ui/remake_ui_tests.sh "$$RUST_DIR_ROOT"

test-ui: VERBOSE?=0
test-ui:
	# Check if RUST_DIR_ROOT is set
	if [ -z "$$RUST_DIR_ROOT" ]; then \
	  echo "Error: RUST_DIR_ROOT is not set. Please set it to the absolute path to rust compiler checkout."; \
	  exit 1; \
	fi
	bash tests/ui/run_ui_tests.sh "$$RUST_DIR_ROOT" "${VERBOSE}"

.PHONY: dot svg png d2 clean-graphs

OUTDIR_DOT=output-dot
OUTDIR_SVG=output-svg
OUTDIR_PNG=output-png
OUTDIR_D2=output-d2

clean-graphs:
	@rm -rf $(OUTDIR_DOT) $(OUTDIR_SVG) $(OUTDIR_PNG) $(OUTDIR_D2)

dot:
	@mkdir -p $(OUTDIR_DOT)
	@for rs in $(TESTDIR)/*.rs; do \
		name=$$(basename $$rs .rs); \
		echo "Generating $$name.smir.dot"; \
		cargo run --release -- --dot -Zno-codegen $$rs 2>/dev/null; \
		mv $$name.smir.dot $(OUTDIR_DOT)/ 2>/dev/null || true; \
	done

svg: dot
	@mkdir -p $(OUTDIR_SVG)
	@for dotfile in $(OUTDIR_DOT)/*.dot; do \
		name=$$(basename $$dotfile .dot); \
		echo "Converting $$name.dot -> $$name.svg"; \
		dot -Tsvg $$dotfile -o $(OUTDIR_SVG)/$$name.svg; \
	done

png: dot
	@mkdir -p $(OUTDIR_PNG)
	@for dotfile in $(OUTDIR_DOT)/*.dot; do \
		name=$$(basename $$dotfile .dot); \
		echo "Converting $$name.dot -> $$name.png"; \
		dot -Tpng $$dotfile -o $(OUTDIR_PNG)/$$name.png; \
	done

d2:
	@mkdir -p $(OUTDIR_D2)
	@for rs in $(TESTDIR)/*.rs; do \
		name=$$(basename $$rs .rs); \
		echo "Generating $$name.smir.d2"; \
		cargo run --release -- --d2 -Zno-codegen $$rs 2>/dev/null; \
		mv $$name.smir.d2 $(OUTDIR_D2)/ 2>/dev/null || true; \
	done
