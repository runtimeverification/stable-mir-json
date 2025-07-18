# Implementation Plan: Upgrade stable-mir-json to Latest Nightly Rustc

## Executive Summary

This plan outlines the approach to upgrade stable-mir-json from its current dependency on `nightly-2024-11-29` to the most recent nightly version of rustc as of July 2025. The project is a sophisticated compiler driver that extracts MIR (Mid-level Intermediate Representation) data from Rust programs and serializes it to JSON format using rustc's internal APIs.

## Problem Analysis

### Current State
- **Current Version**: `nightly-2024-11-29` (approximately 7.5 months old)
- **Target**: Latest available nightly version (expecting `nightly-2025-07-17`)
- **Gap Duration**: ~7.5 months of rustc development changes

### Core Dependencies at Risk
1. **stable_mir**: The primary API used throughout the codebase
2. **rustc_smir**: The bridge between rustc internal types and stable_mir
3. **rustc_middle**: MIR type definitions and structures
4. **rustc_hir**: HIR analysis and type information
5. **Build system dependencies**: Version constraints and feature gates

### Identified High-Risk Areas

#### High Risk (Likely Breaking Changes)
- **stable_mir API evolution**: Method signatures, type definitions, and behavioral changes
  - [Stable MIR Project](https://github.com/rust-lang/project-stable-mir) - Official project page
  - [Kani StableMIR Migration Guide](https://model-checking.github.io/kani/stable-mir.html) - Shows API evolution patterns
  - [PR #115092](https://github.com/rust-lang/rust/pull/115092) - Example of stable_mir API additions
- **MIR structure changes**: New statement types, operand formats, or place projections  
  - [MIR Transform Changes](https://github.com/rust-lang/rust/pull/115612) - Dataflow const-prop improvements
  - [LLVM 18 Update](https://github.com/rust-lang/rust/pull/120055) - Major infrastructure update
- **Type system updates**: Changes to how types are represented or analyzed
  - [Next-generation trait solver](https://blog.rust-lang.org/2023/05/12/Rust-1.70.0.html) - Ongoing development
- **Driver interface changes**: How rustc is invoked and configured programmatically
  - [Rustc API Guidelines](https://rustc-dev-guide.rust-lang.org/api-docs.html) - Official guidance

#### Medium Risk (Possible Breaking Changes)  
- **Diagnostic changes**: Error reporting format or API modifications
- **Metadata format**: Changes to how crate metadata is stored/accessed
- **Feature gate dependencies**: Unstable features being stabilized or removed
  - [Unstable Features Research](https://arxiv.org/abs/2310.17186) - Study on unstable feature usage
- **Performance optimizations**: Internal reorganizations affecting external APIs

#### Low Risk (Likely Compatible)
- **Bug fixes**: Most bug fixes maintain API compatibility
- **Documentation updates**: Should not affect functionality
- **Internal optimizations**: Changes that don't affect public APIs

### Summary of Key Changes Between Versions

Based on the research, major changes in the dependency ecosystem between `nightly-2024-11-29` and `nightly-2025-07-17` include:

1. **LLVM Infrastructure**: 
   - Update to LLVM 18 (Rust 1.78.0) and later to LLVM 19/20
   - [LLVM 18 PR](https://github.com/rust-lang/rust/pull/120055) affected codegen and linking
   - Changes to data layout and target specifications

2. **stable_mir API Expansion**: 
   - New methods and types added through multiple PRs
   - [Example PR #115092](https://github.com/rust-lang/rust/pull/115092) shows typical additions
   - Some existing methods may have changed signatures

3. **MIR Optimization Improvements**: 
   - [Enhanced dataflow analysis](https://github.com/rust-lang/rust/pull/115612) affecting MIR structure
   - Improved constant propagation and optimization passes
   - Changes to MIR transform pipeline

4. **Type System Evolution**: 
   - Next-generation trait solver changes affecting type checking
   - Coherence improvements and inference changes
   - Impact on how types are resolved and checked

5. **Platform Support**: 
   - Multiple new tier 2/3 targets added
   - Changes to existing target specifications
   - WASM and embedded platform improvements

## Implementation Approach

### Phase 1: Environment Setup and Discovery (1-2 days)

#### 1.1 Verify Nightly Availability
- **Pre-determined expectation**: Target `nightly-2025-07-17` 
- **Artifact availability check**: All nightly artifacts should be available via rustup
  - `rustc +nightly-2025-07-17 --version` to verify compiler availability
  - `cargo +nightly-2025-07-17 --version` to verify cargo availability  
  - Standard library and rustc-dev components should be available
- **Fallback options**: If 2025-07-17 unavailable, use latest available nightly from July 2025
- **Installation verification**: Ensure all necessary rustup components are present
  - `rustup component add rustc-dev --toolchain nightly-2025-07-17`
  - `rustup component add rust-src --toolchain nightly-2025-07-17`

#### 1.2 Update Build Configuration
- Modify `rust-toolchain.toml` to target `nightly-2025-07-17`
- Update any version-specific configuration in build scripts
- Document the change with rationale in version control

#### 1.3 Initial Compilation Attempt
- Run `cargo +nightly-2025-07-17 check` to identify immediate compilation failures
- Capture and categorize all error messages for systematic resolution
- Create a comprehensive error log for tracking resolution progress

### Phase 2: Core API Migration (3-5 days)
1. **stable_mir API Updates**
   - Update all `stable_mir` imports and usage patterns
   - Adapt to changes in MIR data structures and access patterns
   - Update visitor implementations for MIR traversal

2. **rustc_internal Bridge Updates**
   - Adapt `rustc_internal::internal()` and `rustc_internal::stable()` calls
   - Update type conversions between stable and internal representations
   - Handle changes in instance resolution and monomorphization

3. **Type System Integration**
   - Update `TyCtxt` usage patterns
   - Adapt to changes in typing environment handling
   - Update generic parameter and lifetime handling

### Phase 3: Data Extraction and Serialization (2-3 days)
1. **MIR Data Collection Updates**
   - Update `SmirJson` data structure if needed
   - Adapt visitor patterns in `InterValueCollector` and `UnevaluatedConstantCollector`
   - Ensure all MIR elements are still accessible and extractable

2. **JSON Serialization Compatibility**
   - Verify JSON output format remains consistent
   - Update serialization logic for any changed data structures
   - Maintain self-contained JSON output requirements

3. **Allocation and Span Tracking**
   - Update allocation ID tracking and metadata extraction
   - Adapt span collection and source location mapping
   - Ensure debug information extraction continues working

### Phase 4: Testing and Validation (2-3 days)
1. **Integration Testing**
   - Run existing integration test suite
   - Compare JSON outputs between old and new versions
   - Identify and resolve any behavioral differences

2. **UI Testing**
   - Run UI test suite against rustc test cases
   - Update failing tests if appropriate
   - Document any intentional behavioral changes

3. **Regression Testing**
   - Ensure all core functionality remains intact
   - Verify GraphViz output generation still works
   - Test cargo integration and build processes

### Phase 5: Documentation and Cleanup (1 day)
1. **Update Documentation**
   - Update version requirements in README.md
   - Document any API changes or new requirements
   - Update build instructions if needed

2. **Code Cleanup**
   - Remove any deprecated API usage
   - Clean up conditional compilation if any
   - Ensure code formatting and style compliance

## Risk Assessment and Mitigation Strategies

### High Probability Risks

#### 1. Breaking Changes in stable_mir API
**Risk**: Fundamental changes to stable_mir data structures or access patterns
**Likelihood**: High (80%)
**Impact**: High
**Mitigation**: 
- Systematic review of stable_mir changelog
- Incremental migration approach
- Extensive testing with diverse Rust programs

#### 2. rustc_internal Bridge Changes
**Risk**: Changes to internal/stable API translation layer
**Likelihood**: High (70%)
**Impact**: High
**Mitigation**:
- Focus on minimal API usage
- Consider alternative approaches for data access
- Implement fallback mechanisms where possible

#### 3. Instance Resolution Changes
**Risk**: Changes to monomorphization and type resolution
**Likelihood**: Medium (60%)
**Impact**: High
**Mitigation**:
- Study rustc changes to monomorphization
- Update resolution logic incrementally
- Extensive testing with complex generic code

### Medium Probability Risks

#### 4. JSON Output Format Changes
**Risk**: Unintentional changes to serialized output format
**Likelihood**: Medium (40%)
**Impact**: Medium
**Mitigation**:
- Comprehensive output comparison testing
- Version detection in JSON output
- Backwards compatibility considerations

#### 5. Performance Regressions
**Risk**: Slower compilation or larger memory usage
**Likelihood**: Medium (30%)
**Impact**: Medium
**Mitigation**:
- Performance benchmarking before/after
- Memory usage monitoring
- Optimization of data collection patterns

### Low Probability Risks

#### 6. Toolchain Installation Issues
**Risk**: Required rustc components unavailable
**Likelihood**: Low (10%)
**Impact**: High
**Mitigation**:
- Multiple nightly version testing
- Alternative component installation methods
- Fallback to slightly older nightly if needed

## Alternative Approaches

### Approach 1: Incremental Upgrade (Recommended)
- Upgrade to intermediate nightly versions step by step
- Identify and fix issues gradually
- Lower risk but potentially more time-consuming

### Approach 2: Direct Latest Upgrade
- Jump directly to latest nightly
- Fix all issues in one iteration
- Higher risk but potentially faster if successful

### Approach 3: Hybrid Approach
- Jump to latest nightly for initial assessment
- Fall back to incremental approach if issues are severe
- Balanced risk/time trade-off

## Success Criteria

### Functional Requirements
1. **Compilation Success**: Project compiles without errors on latest nightly
2. **Behavioral Compatibility**: JSON output maintains same semantic information
3. **Test Suite Pass**: All existing tests pass or are appropriately updated
4. **Performance Maintenance**: No significant performance regressions

### Quality Requirements
1. **Code Quality**: Maintains clippy and formatting standards
2. **Documentation**: Updated documentation reflects new requirements
3. **Stability**: No crashes or panics in normal operation
4. **Completeness**: All MIR data extraction features continue working

## Timeline Estimate

- **Total Duration**: 7-12 days
- **Critical Path**: Core API migration (Phase 2)
- **Potential Delays**: Complex stable_mir API changes requiring architectural updates
- **Confidence Level**: Medium (60-70% chance of completing within timeline)

## Dependencies and Prerequisites

### Technical Dependencies
1. Access to latest rustc nightly builds
2. Complete rustc-dev toolchain components
3. Comprehensive test environment

### Knowledge Dependencies
1. Understanding of rustc internal API changes
2. Familiarity with stable_mir evolution
3. Knowledge of MIR representation changes

## Conclusion

This upgrade represents a significant but manageable technical challenge. The systematic approach outlined above provides multiple risk mitigation strategies while maintaining focus on the core goal of preserving stable-mir-json's functionality. The project's modular architecture and comprehensive test suite provide strong foundations for a successful upgrade.

The key to success will be methodical progression through the phases, careful testing at each step, and willingness to adapt the approach based on discovered issues during implementation.