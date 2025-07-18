# Implementation Plan: Upgrade stable-mir-json to Latest Nightly Rustc

## Executive Summary

This plan outlines the approach to upgrade stable-mir-json from its current dependency on `nightly-2024-11-29` to the most recent nightly version of rustc as of July 2025. The project is a sophisticated compiler driver that extracts MIR (Mid-level Intermediate Representation) data from Rust programs and serializes it to JSON format using rustc's internal APIs.

## Problem Analysis

### Current State
- **Current Version**: `nightly-2024-11-29` (approximately 7.5 months old)
- **Target**: Latest available nightly version (likely `nightly-2025-07-17` or similar)
- **Gap Duration**: ~7.5 months of rustc development changes

### Core Dependencies at Risk
1. **stable_mir crate**: Extensive usage throughout codebase for MIR access
2. **rustc_private crates**: Direct usage of rustc internals:
   - `rustc_driver`, `rustc_interface`, `rustc_middle`
   - `rustc_session`, `rustc_smir`, `rustc_span`
   - `rustc_monomorphize`
3. **API Translation Layer**: `rustc_internal` for stable/internal API conversion

### Expected Impact Areas

#### 1. High-Risk Changes (Likely Breaking)
- **stable_mir API evolution**: The stable_mir crate has been actively developed and may have breaking changes
- **rustc_smir bridge changes**: Internal-to-stable API translation layer updates
- **TyCtxt and typing environment changes**: Rust has been refactoring type system internals
- **Instance resolution**: Changes to monomorphization and instance resolution APIs
- **MIR visitor patterns**: Updates to MIR traversal and visitor APIs

#### 2. Medium-Risk Changes (Possibly Breaking)
- **Serialization format compatibility**: Changes to internal data structures affecting JSON output
- **Span and diagnostic handling**: Updates to source location tracking
- **Memory allocation tracking**: Changes to allocation ID and tracking systems
- **Symbol and name mangling**: Updates to symbol handling

#### 3. Low-Risk Changes (Likely Compatibility)
- **Build system integration**: Cargo integration should remain stable
- **Core JSON serialization**: Serde usage should remain compatible
- **Basic file I/O operations**: Standard library usage should be stable

## Implementation Approach

### Phase 1: Environment Setup and Discovery (1-2 days)
1. **Determine Target Nightly Version**
   - Identify the most recent stable nightly build
   - Consider using a specific date-based nightly (e.g., `nightly-2025-07-17`)
   - Verify availability of required components (`rustc-dev`, `rust-src`, `llvm-tools`)

2. **Update Toolchain Configuration**
   - Update `rust-toolchain.toml` to target nightly version
   - Ensure CI/build environment compatibility

3. **Initial Compilation Attempt**
   - Attempt compilation to identify immediate breaking changes
   - Document all compilation errors and their categories
   - Assess scope of required changes

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