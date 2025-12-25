# Rust Stable MIR Pretty Printing

This package provides a program that will emit a JSON serialisation of the Stable MIR of a Rust program

## Building

```shell
cargo build
```

NOTE: requries [rustup](https://www.rust-lang.org/tools/install)

The `build.rs` script will ensure that the correct version of rust and the required components are installed and defaulted. What `rustup` commands are run can be seen by adding verbosity flag `-vv` to `cargo`.

## Usage

Use the wrapper script `run.sh` (or `cargo run`, but this may also initiate a build).
The options that this tool accepts are identical to `rustc`.
To generate stable MIR output without building a binary, you can invoke the tool as follows:

```shell
cargo run -- <rustc_flags> <path_from_crate_root>
```

There is experimental support for rendering the Stable-MIR items and their basic blocks as a 
call graph in graphviz' dot format. 

To produce a dot file `*.smir.dot` (instead of `*.smir.json`), one can invoke the driver with
_first_ argument `--dot`. When using `--json` as the first argument, the `*.smir.json` file
will be written. Any other strings given as first argument will be passed to the compiler 
(like all subsequent arguments).

To generate visualizations for all test programs:

```shell
make dot   # Generate .dot files in output-dot/
make svg   # Generate .svg files in output-svg/ (requires graphviz)
make png   # Generate .png files in output-png/ (requires graphviz)
make d2    # Generate .d2 files in output-d2/
```

There are a few environment variables that can be set to control the tools output:

1.  `LINK_ITEMS` - add entries to the link-time `functions` map for each monomorphic item in the crate;
2.  `LINK_INST`  - use a richer key-structure for the link-time `functions` map which uses keys that are pairs of a function type (`Ty`) _and_ an function instance kind (`InstanceKind`)
3.  `DEBUG` - serialize additional data in the JSON file and dump logs to stdout

## Development

To ensure code quality, all code is required to pass `cargo clippy`, `cargo fmt`, and `nixfmt **/*.nix` without warning to pass CI.

You can install `nixfmt` by [installing nix](https://nixos.org/download/) and running `nix profile install nixpkgs#nixfmt-rfc-style`.

## Tests

Integration tests for `stable-mir-pretty` consist of compiling a number of (small)
programs with the wrapper compiler, and checking the output against expected JSON
data ("golden" tests).

The tests are stored [in `src/tests/integration/programs`](./src/tests/integration/programs).

To compensate for any non-determinism in the output, the JSON file is first processed
to sort the array contents and remove data which changes with dependencies (such as 
the crate hash suffix in the symbol names).

The JSON post-processing is performed [with `jq` using the script in `src/tests/integration/normalise-filter.jq`](./src/tests/integration/normalise-filter.jq).

Some tests have non-deterministic output and are therefore expected to fail. 
These tests are stored [in `src/tests/integration/failing`](./src/tests/integration/failing).

### Running the Tests

To run the tests, do the following:

```shell
make integration-test
```

## Integration with `cargo`
Currently the system to integrate with cargo is to create a `.stable_mir_json` package that contains the libraries, binaries, and run scripts for `stable_mir_json`. These run scripts ensure that the the same library that built `stable_mir_json` is used in the `cargo` project. Here are the steps required:

1. Navigate to the project root of `stable_mir_json` and run
```bash
cargo run --bin cargo_stable_mir_json -- $PWD [OPTIONAL-PATH-FOR-DIR]
```

2. Navigate to `cargo` project and run script for the appropriate profile, e.g. `debug` or `release`
```bash
RUSTC=<PATH-TO-.stable_mir_json>/<PROFILE>.sh cargo build
```
NOTE: by default `<PATH-TO-.stable_mir_json>` is `~/.stable_mir_json`