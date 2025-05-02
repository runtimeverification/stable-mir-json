RELEASE_FLAG=
TOOLCHAIN_NAME=''

default: build

build:
	cargo build ${RELEASE_FLAG}

clean: rustup-clear-toolchain
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

test-ui: VERBOSE=0
test-ui:
	# Check if RUST_DIR_ROOT is set
	if [ -z "$$RUST_DIR_ROOT" ]; then \
	  echo "Error: RUST_DIR_ROOT is not set. Please set it to the absolute path to rust compiler checkout."; \
	  exit 1; \
	fi
	bash tests/ui/run_ui_tests.sh "$$RUST_DIR_ROOT" "${VERBOSE}"
