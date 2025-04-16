# Rust UI Tests

These tests are taken from The [Rust compiler UI test suite](https://github.com/rust-lang/rust/tests/ui/).
Some tests here are not appropriate for us to test with yet, so we need to filter valid tests for
the current state of Stable MIR JSON are generated. To generate and run the tests a checkout of the
rust compiler is required. 

## Usage
To generate the tests

```bash
cd tests/ui/
RUST_TOP=`<PATH-TO-RUST>` ./collect_test_sources.sh > ui_sources.txt
```

To remake the ui tests and filter into the passing and failing directories (optionally storing the output)

```bash
cd tests/ui/
./remake_ui_tests.sh <PATH-TO-RUST> [y|n]
```

To run the passing tests again

```bash
cd tests/ui/
./run_ui_tests.sh <PATH-TO-RUST>
```