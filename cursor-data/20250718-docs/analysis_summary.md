# stable-mir-json Analysis Summary

## What is stable-mir-json?

A Rust compiler driver that intercepts compilation to extract Middle Intermediate Representation (MIR) data and serialize it to self-contained JSON format for analysis tools.

## Key Technical Characteristics

- **Compiler Integration**: Uses rustc callbacks via `rustc_driver::Callbacks`
- **MIR Access**: Leverages stable_mir API for safe compiler internals access
- **Self-Contained Output**: JSON includes all necessary lookup tables
- **Dual Output**: Supports both JSON and GraphViz dot formats
- **Cargo Integration**: Seamless integration with cargo build systems

## Target Users

- Verification tool developers (Creusot, Prusti, KANI)
- Security researchers and auditors
- Compiler researchers and educators
- Static analysis tool builders
- Academic programming language researchers

## Architecture Components

1. **Driver** (`src/driver.rs`) - Compiler hook mechanism
2. **Printer** (`src/printer.rs`) - JSON serialization with SmirJson structure
3. **Graph** (`src/mk_graph.rs`) - GraphViz visualization output
4. **Cargo Integration** (`src/bin/cargo_stable_mir_json.rs`) - Build system integration
5. **Testing** - Comprehensive golden file tests with normalization

## Documentation Enhancements Made

### goals.md
- Added comprehensive use cases (verification, debugging, research)
- Defined clear target audience
- Explained practical applications

### design.md  
- Detailed architectural documentation
- Component descriptions and data flow
- Self-contained design explanation
- Testing strategy and quality processes
- Performance considerations

### requirements.md
- Functional, non-functional, and technical requirements
- Security and documentation requirements
- Future extensibility considerations
- Performance and compatibility specifications

## Key Insights

1. **Sophisticated Tool**: More complex than apparent, with advanced cargo integration
2. **Self-Contained Strategy**: Clever solution to stable_mir API limitations using lookup tables
3. **Quality Focus**: Strong testing framework with JSON normalization
4. **Nightly Dependency**: Requires specific rustc nightly for rustc_private features
5. **Research-Oriented**: Primarily serves academic and verification tool development