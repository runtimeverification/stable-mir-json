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

# Directories and file paths
UI_DIR         := tests/ui
UI_SOURCES     := $(UI_DIR)/ui_sources.txt
FAILING_TSV    := $(UI_DIR)/failing.tsv
PASSING_TSV    := $(UI_DIR)/passing.tsv
FAILING_DIR    := $(UI_DIR)/failing
PASSING_DIR    := $(UI_DIR)/passing

.PHONY: remake-ui-tests test-ui

remake-ui-tests:
	@# Check if RV_TEST_DIR is set
	@if [ -z "$$RV_TEST_DIR" ]; then \
	  echo "Error: RV_TEST_DIR is not set. Please set it to the absolute path to mir-semantics-compiletest."; \
	  exit 1; \
	fi
	@echo "Resetting UI test directories and TSVs..."
	@rm -f $(FAILING_TSV) $(PASSING_TSV) $(UI_DIR)/not_found.tsv
	@rm -rf $(FAILING_DIR) $(PASSING_DIR)
	@touch $(FAILING_TSV) $(PASSING_TSV) $(UI_DIR)/not_found.tsv
	@mkdir -p $(FAILING_DIR) $(PASSING_DIR)
	@echo "Running UI tests..."
	@while read -r test; do \
	  full_path="$$RV_TEST_DIR/$$test"; \
	  if [ ! -f "$$full_path" ]; then \
	    echo "Warning: Test file '$$full_path' not found."; \
	    echo "$$test" >> $(UI_DIR)/not_found.tsv; \
	    continue; \
	  fi; \
	  echo "Running test: $$test"; \
	  cargo run -- -Zno-codegen "$$full_path" > tmp.stdout 2> tmp.stderr; \
	  status=$$?; \
	  base_test=$$(basename $$test); \
	  json_file="$$(basename $$test .rs).smir.json"; \
	  if [ $$status -ne 0 ]; then \
	    echo "Test $$test FAILED with exit code $$status"; \
	    cp "$$full_path" "$(FAILING_DIR)/$$base_test"; \
	    cp tmp.stdout "$(FAILING_DIR)/$$base_test.stdout"; \
	    cp tmp.stderr "$(FAILING_DIR)/$$base_test.stderr"; \
	    echo "$$base_test	$$status" >> $(FAILING_TSV); \
	  else \
	    echo "Test $$test PASSED"; \
	    cp "$$full_path" "$(PASSING_DIR)/$$base_test"; \
	    if [ -f "$$json_file" ]; then \
	      mv "$$json_file" "$(PASSING_DIR)/$$json_file"; \
	    fi; \
	    echo "$$base_test" >> $(PASSING_TSV); \
	  fi; \
	done < $(UI_SOURCES)
	@echo "Sorting TSV files..."
	@if [ -s $(FAILING_TSV) ]; then sort $(FAILING_TSV) -o $(FAILING_TSV); fi
	@if [ -s $(PASSING_TSV) ]; then sort $(PASSING_TSV) -o $(PASSING_TSV); fi
	@if [ -s $(UI_DIR)/not_found.tsv ]; then sort $(UI_DIR)/not_found.tsv -o $(UI_DIR)/not_found.tsv; fi
	@echo "UI tests remade."

test-ui:
	@echo "Running regression tests for passing UI cases..."
	@failed=0; \
	while read -r test; do \
	  test_path="$(UI_DIR)/passing/$$test"; \
	  json_file="$$(basename $$test .rs).smir.json"; \
	  cargo run -- -Zno-codegen "$$test_path" > /dev/null 2>&1; \
	  status=$$?; \
	  if [ $$status -ne 0 ]; then \
	    echo "FAILED: $$test_path (exit $$status)"; \
	    failed=1; \
	  fi; \
	  [ -f "$$json_file" ] && rm -f "$$json_file"; \
	done < $(PASSING_TSV); \
	if [ $$failed -ne 0 ]; then \
	  echo "Some tests FAILED."; \
	  exit 1; \
	else \
	  echo "All regression tests passed."; \
	fi
