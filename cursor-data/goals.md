# Goals of stable-mir-json

## Primary Purpose
This software `stable-mir-json` compiles a Rust program and extracts the
middle intermediate representation (MIR) from the compiler.
The extracted MIR is saved in a file as self-contained JSON data so that
Rust verification and inspection tools can provide insight into the 
program's inner workings and behaviour.

## Use Cases and Applications

### Program Analysis and Verification
- **Static Analysis Tools**: Provide structured MIR data for tools that analyze Rust programs for safety, security, and correctness
- **Formal Verification**: Enable verification tools like Creusot, Prusti, or KANI to work with standardized MIR representations
- **Security Auditing**: Allow security researchers to examine the compiled representation of Rust code for vulnerability analysis

### Development and Debugging
- **Compiler Education**: Help developers understand how Rust code is represented internally after compilation
- **Performance Analysis**: Enable analysis of optimization decisions and code structure at the MIR level
- **Debug Information**: Provide detailed insights into how high-level Rust constructs are lowered to MIR

### Research and Tooling
- **Academic Research**: Support research into programming language semantics, compiler optimizations, and program analysis
- **Tool Development**: Serve as a foundation for building new Rust analysis and transformation tools
- **Cross-compilation Analysis**: Understand target-specific compilation differences through MIR examination

## Target Audience
- Rust verification tool developers
- Compiler researchers and educators
- Security analysts working with Rust programs
- Tool builders requiring access to Rust's internal representations
- Academic researchers studying programming language implementation
