# Requirements for stable-mir-json

## Functional Requirements

### Core Compilation Compatibility
* **Complete rustc Compatibility**: Compile any given Rust program in the same way as the underlying nightly `rustc` version would
* **Error Handling**: Not crash on an attempt to compile any Rust program that rustc can handle
* **MIR Faithfulness**: Faithfully extract all MIR data of a given Rust program into a JSON representation which contains _no external references_ (with the exception of references to other crates that the Rust program is declared to depend on)

### Output Format Requirements
* **Compact JSON**: Output the JSON for its MIR data in a compact form for space efficiency
* **Deterministic Ordering**: Sort all lookup tables of type `Vec<(Key, Value)>` in the MIR data by the respective `Key`s to facilitate reading and comparing by humans
* **Self-Contained Output**: JSON files must be completely self-contained and not require compiler context to interpret

## Non-Functional Requirements

### Performance Requirements
* **Compilation Speed**: Should not significantly slow down compilation compared to standard rustc
* **Memory Usage**: Should handle large Rust projects without excessive memory consumption
* **JSON Size**: Output files should be reasonably sized for typical Rust programs (aim for <10MB for medium projects)

### Compatibility Requirements
* **Rust Version**: Must work with specific nightly toolchain versions (currently nightly-2024-11-29)
* **Platform Support**: Should work on all platforms supported by rustc with rustc-dev component
* **Crate Ecosystem**: Must handle dependencies from crates.io and local crates correctly

### Quality and Reliability Requirements
* **Test Coverage**: All major Rust language features must be covered by integration tests
* **Deterministic Output**: JSON output must be deterministic after normalization for testing
* **Error Reporting**: Clear error messages when compilation or extraction fails
* **Code Quality**: All code must pass clippy and formatting checks

## Technical Requirements

### Toolchain Dependencies
* **Nightly Rustc**: Requires rustc-private features, so nightly toolchain is mandatory
* **Components**: Requires rustc-dev, rust-src, and llvm-tools components
* **Build System**: Must use build.rs to ensure correct toolchain setup

### API Stability
* **Stable MIR API**: Must use only the stable_mir API for accessing compiler internals
* **Forward Compatibility**: Should gracefully handle additions to MIR representation
* **Backward Compatibility**: JSON format should be versioned and backward compatible

### Integration Requirements
* **Cargo Integration**: Must provide seamless integration with cargo build systems
* **Command Line**: Must accept all rustc command line options and flags
* **Environment Variables**: Should support configuration via environment variables
* **Output Formats**: Must support both JSON and GraphViz dot output formats

## Security Requirements
* **Safe Compilation**: Must not introduce security vulnerabilities during compilation
* **Input Validation**: Should validate all input arguments and handle malformed Rust code safely
* **Output Safety**: JSON output should not contain sensitive information from the build environment

## Documentation Requirements
* **API Documentation**: All public APIs must be documented
* **Usage Examples**: Must provide clear usage examples for common scenarios
* **Integration Guide**: Must document cargo integration setup process
* **Troubleshooting**: Must provide guidance for common issues and debugging

## Future Extensibility Requirements
* **Plugin Architecture**: Should be designed to allow future extensions and plugins
* **Output Format Flexibility**: Should allow for additional output formats beyond JSON and dot
* **Analysis Extensions**: Should provide hooks for additional analysis passes
* **Custom Serialization**: Should allow customization of what data is included in output
