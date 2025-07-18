# Design Architecture of stable-mir-json

## Overview
The software `stable-mir-json` consists of Rust code that links to a specific
nightly version of the Rust compiler `rustc`.
The software has a small driver program `driver.rs` which executes the Rust compiler
(with all provided options) and meanwhile calls a specific _hook_ in the compiler
to extract the Middle Intermediate Representation (MIR) into a self-contained JSON file.

## Core Architecture

### Compiler Integration Strategy
- **Rustc Driver Interception**: Uses rustc's callback mechanism via `rustc_driver::Callbacks`
- **Stable MIR API**: Leverages the `stable_mir` crate for accessing compiler internals safely
- **Nightly Dependency**: Requires specific nightly toolchain (currently nightly-2024-11-29) for rustc_private features

### Data Flow Architecture

```
Rust Source Code → rustc compilation → MIR extraction → JSON serialization → Output File
                                          ↓
                                   SmirJson structure
                                          ↓
                                   Self-contained data with lookup tables
```

### Key Components

#### 1. Driver Module (`src/driver.rs`)
- **Purpose**: Provides compiler context and stable_mir API access
- **Implementation**: Custom `StableMirCallbacks` that implements `rustc_driver::Callbacks`
- **Hook Point**: `after_analysis` callback extracts MIR after type checking and analysis
- **API**: Single function `stable_mir_driver(args, callback_fn)` mimicking `rustc_internal::run_with_tcx!`

#### 2. Printer Module (`src/printer.rs`)
- **Core Data Structure**: `SmirJson<'t>` - the main serialization target
- **Components**:
  - `items`: Vector of compiled Rust functions broken into basic blocks
  - `types`: Type metadata with layout information 
  - `allocs`: Memory allocations and static data
  - `functions`: Link-time function mapping
  - `uneval_consts`: Unevaluated constant expressions
  - `spans`: Source location mapping
  - `machine`: Target machine information
  - `debug`: Optional debug information

#### 3. Graph Generation Module (`src/mk_graph.rs`)
- **Purpose**: Alternative output format as GraphViz dot files
- **Features**: Visualizes MIR items and basic blocks as call graphs
- **Usage**: Activated via `--dot` command line flag

#### 4. Cargo Integration (`src/bin/cargo_stable_mir_json.rs`)
- **Installation**: Creates `.stable_mir_json` directory with build artifacts
- **Integration**: Provides shell scripts that set RUSTC environment variable
- **Profile Support**: Handles both debug and release build configurations

## Data Model and Self-Containment

### Self-Contained JSON Design
The JSON output is designed to be completely self-contained, meaning it includes all necessary information to understand the MIR without requiring access to the original compiler context.

For background information about MIR see the following two web pages:
* https://blog.rust-lang.org/2016/04/19/MIR/
* https://rustc-dev-guide.rust-lang.org/mir/index.html

The JSON data serialised in the file is the data structure `crate::printer::SmirJson`.
We call this data the "MIR data".

Apart from extracting the MIR data as JSON into a file, the software can also output a graph 
representation of the extracted MIR in the form of a `*.dot` file for tools from the `graphviz` suite.

The most essential part of the MIR data is the vector of `items`.
Each item in the vector is a Rust function compiled into its MIR, which breaks down the
function body into _basic block_.

The extraction is done using the `stable_mir` crate within the `rustc` software, which
provides a stable API to the compiler's internals.

Besides the `items`, the MIR data in `stable-mir-json` includes a number of _lookup maps_
which are represented by vectors of pairs (`Vec<(Key, Value)>`). 
The tables are additional data which is not part of `stable_mir` data structures.

#### Lookup Tables Strategy
- **Problem**: `stable_mir` API provides access to data through function calls, not direct data structures
- **Solution**: Extract referenced data into lookup tables represented as `Vec<(Key, Value)>` pairs
- **Implementation**: Visitors traverse MIR structures to collect all referenced types, allocations, and constants

The `stable_mir` package does not require these tables because it is internal to the compiler
and holds similar lookup tables in an internal state (not accessible directly, only through
the `stable_mir` API functions). `stable-mir-json` has to add this information to the MIR data
to become self-contained. 

#### Current Limitations
- **Incomplete Type Extraction**: Not all types used in Rust programs are currently extracted (known issue)
- **External Crate References**: References to external crates are preserved but not fully expanded
- **Work in Progress**: The extraction process is actively being improved

This extraction is work in progress; for instance, a known problem
is that not all types used in the Rust program are extracted into the JSON file.

## Testing Strategy

### Integration Test Framework
- **Golden Tests**: Compare generated JSON against expected output files
- **Normalization**: Uses `jq` filter to handle non-deterministic elements (hashes, IDs)
- **Test Cases**: Covers diverse Rust language features (closures, enums, recursion, etc.)
- **Failure Handling**: Separate directory for tests with known non-deterministic output

### Quality Assurance
- **Clippy Integration**: All code must pass `cargo clippy` without warnings
- **Formatting**: All code must pass `cargo fmt` without changes
- **Build Validation**: Uses `build.rs` to ensure correct toolchain and components

## Module Dependency Graph

The software follows a clear architectural hierarchy:

```
main.rs
├── driver.rs (compiler integration)
├── printer.rs (JSON serialization)
│   └── SmirJson data structure
├── mk_graph.rs (GraphViz output)
│   └── Uses printer::collect_smir()
└── bin/cargo_stable_mir_json.rs (cargo integration)
```

## External Dependencies and References

For background information about MIR see the following resources:
* [MIR Blog Post](https://blog.rust-lang.org/2016/04/19/MIR/)
* [Rustc Dev Guide MIR Chapter](https://rustc-dev-guide.rust-lang.org/mir/index.html)
* [Stable MIR RFC](https://github.com/rust-lang/rfcs/blob/master/text/2594-stable-mir.md)

## Environment Variables and Configuration

- `LINK_ITEMS`: Adds entries to link-time functions map for monomorphic items
- `LINK_INST`: Uses richer key structure for link-time functions map
- `DEBUG`: Serializes additional data and dumps logs to stdout

## Performance Considerations

- **Memory Usage**: Uses visitor patterns to avoid copying large data structures
- **JSON Size**: Implements compact serialization and sorted lookup tables
- **Compilation Speed**: Minimal overhead during compilation process
- **Extensibility**: Modular design allows for future enhancements without major restructuring
