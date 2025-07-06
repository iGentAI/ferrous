# Lua VM Completion Roadmap

This document outlines the roadmap for completing the Ferrous Lua VM implementation, focusing on full Lua language compatibility before Redis integration.

## Current State

As of July 2025, the core VM architecture is functional with several critical fixes implemented:

1. **Register Allocation Fix (July 2025)**: Proper register lifetime tracking ensures registers aren't freed prematurely.
2. **Table Return Value Fix (July 2025)**: Tables can now be properly returned from Lua scripts.
3. **LOADNIL Opcode Fix (July 2025)**: Corrected parameter handling for LOADNIL opcode.

The VM can now execute scripts with:
- Basic arithmetic operations
- Table creation and manipulation
- Conditional logic and comparisons
- Simple closures
- Script functions that return tables or primitive values

Many placeholder implementations and TODOs remain throughout the codebase that need to be addressed before the implementation can be considered complete and production-ready.

## Remaining Placeholders

See [LUA_VM_PLACEHOLDER_IMPLEMENTATIONS.md](./LUA_VM_PLACEHOLDER_IMPLEMENTATIONS.md) for a detailed list of all TODOs and placeholder implementations that need to be addressed.

## 1. Closure and Upvalue System

### 1.1 Upvalue Management (High Priority)
- **Description**: Fix and complete upvalue management for complex closure scenarios
- **Required Work**:
  - Ensure upvalues are properly closed when variables go out of scope
  - Implement upvalue sharing between closures capturing the same variables
  - Fix upvalue management in the Thread struct's open_upvalues list
  - Add comprehensive tests for nested closures and mutual recursion
- **Implementation Strategy**: 
  - Update the CLOSE opcode to properly handle all upvalues
  - Enhance find_or_create_upvalue to ensure proper sharing
  - Complete the close_thread_upvalues implementation

### 1.2 Complex Closure Scenarios (High Priority)
- **Description**: Fix issues with complex closure patterns
- **Required Work**:
  - Test and fix nested closure scenarios
  - Ensure proper upvalue sharing between closures
  - Address edge cases in closures with mutual recursion
- **Implementation Strategy**:
  - Create comprehensive test cases for complex closure patterns
  - Update the CLOSURE opcode implementation to handle all scenarios
  - Fix any issues in how function prototypes are loaded and managed

## 2. Standard Library Implementation

### 2.1 Math Library (High Priority)
- **Description**: Implement standard math operations
- **Required Functions**:
  - `math.abs`, `math.ceil`, `math.floor`, `math.max`, `math.min`
  - `math.sin`, `math.cos`, `math.tan`, `math.asin`, `math.acos`, `math.atan`
  - `math.deg`, `math.rad`, `math.exp`, `math.log`, `math.log10`, `math.pow`, `math.sqrt`
  - `math.random`, `math.randomseed`, `math.fmod`
- **Implementation Strategy**: 
  - Wrap Rust's standard math functions
  - Ensure proper error handling for domain/range errors
  - Add comprehensive tests for each function

### 2.2 String Library (High Priority)
- **Description**: Implement string manipulation functions
- **Required Functions**:
  - `string.byte`, `string.char`, `string.dump`
  - `string.find`, `string.format`, `string.gmatch`, `string.gsub`
  - `string.len`, `string.lower`, `string.match`, `string.rep`
  - `string.reverse`, `string.sub`, `string.upper`
- **Implementation Strategy**:
  - Use Rust's string handling capabilities
  - Implement pattern matching with proper Lua pattern semantics
  - Add full test coverage for all string operations

### 2.3 Table Library (High Priority)
- **Description**: Implement table manipulation functions
- **Required Functions**:
  - `table.concat`, `table.insert`, `table.maxn`
  - `table.remove`, `table.sort`, `table.foreach`
  - `table.foreachi`, `table.getn`, `table.setn`
- **Implementation Strategy**:
  - Implement using transaction-safe table operations
  - Ensure proper metamethod invocation when needed
  - Pay special attention to algorithms like table.sort

### 2.4 Error Handling (High Priority)
- **Description**: Complete the error handling system
- **Required Functions and Features**:
  - `pcall` and `xpcall` completion
  - Traceback generation
  - Proper error location information
  - Error propagation through C function boundaries
- **Implementation Strategy**:
  - Enhance the current pcall implementation
  - Add support for tracking error location
  - Create a proper error-to-string formatter

## 3. Testing and Compliance

### 3.1 Comprehensive Test Suite (High Priority)
- **Description**: Create a complete test suite covering all aspects of Lua 5.1
- **Test Categories**:
  - Syntax tests: All language constructs
  - Semantic tests: All language features
  - Standard library tests: All library functions
  - Edge cases and error handling
  - Performance benchmarks
- **Implementation Strategy**:
  - Use existing test cases from Lua 5.1 test suite
  - Add custom tests for Ferrous-specific behavior
  - Implement automated test runner

### 3.2 Compliance Verification (Medium Priority)
- **Description**: Verify compliance with Lua 5.1 specification
- **Key Components**:
  - Language compliance tests
  - Standard library compliance tests
  - Performance benchmarks compared to reference implementation
- **Implementation Strategy**:
  - Run standard Lua test suite
  - Compare behavior against reference implementation
  - Document and resolve any discrepancies

## 4. Performance and Optimization

### 4.1 Memory Management (Medium Priority)
- **Description**: Implement garbage collection
- **Key Components**:
  - Non-recursive mark-and-sweep
  - Generational collection
  - Memory pressure sensitivity
- **Implementation Strategy**:
  - Implement a basic mark-and-sweep collector first
  - Add incremental collection to avoid pauses
  - Tune GC parameters for Redis workloads

### 4.2 VM Optimization (Medium Priority)
- **Description**: Optimize VM execution
- **Key Components**:
  - Instruction dispatch optimization
  - Fast path for common operations
  - Register allocation improvements
- **Implementation Strategy**:
  - Profile VM with realistic workloads
  - Identify and optimize hotspots
  - Benchmark against reference implementation

### 4.3 Compiler Optimization (Low Priority)
- **Description**: Optimize bytecode generation
- **Key Components**:
  - Constant folding
  - Jump optimization
  - Tail call optimization
- **Implementation Strategy**:
  - Implement common compiler optimizations
  - Add optimization level control
  - Benchmark before/after improvements

## 5. Documentation

### 5.1 API Documentation (High Priority)
- **Description**: Document the Lua VM API
- **Key Components**:
  - Public API functions
  - Configuration options
  - Extension points
- **Implementation Strategy**:
  - Add detailed comments to public API
  - Create example usage documentation
  - Document performance characteristics

### 5.2 Implementation Documentation (Medium Priority)
- **Description**: Document internal implementation
- **Key Components**:
  - Architecture overview
  - Component interaction
  - Design decisions
- **Implementation Strategy**:
  - Keep design documents updated
  - Document key algorithms and data structures
  - Include diagrams where helpful

## 6. Redis Integration (Low Priority)

*Note: This is included for completeness but is lower priority per current direction*

### 6.1 Basic Redis Integration
- **Description**: Implement Redis API for Lua
- **Key Components**:
  - `redis.call()` and `redis.pcall()`
  - KEYS and ARGV tables
  - Error propagation to Redis
- **Implementation Strategy**:
  - Implement after core Lua functionality is complete
  - Ensure proper error handling and type conversion
  - Add Redis-specific performance tests

### 6.2 Redis Command Sandboxing
- **Description**: Implement safe execution environment
- **Key Components**:
  - Resource limits (CPU, memory)
  - Execution time monitoring
  - Command allowlist
- **Implementation Strategy**:
  - Add resource tracking hooks to VM
  - Implement limits based on Redis configuration
  - Add sandbox escape detection

## Implementation Timeline and Priorities

In light of recent progress and current state, this timeline reflects revised priorities:

| Phase | Component | Priority | Estimated Effort | Dependencies |
|-------|-----------|----------|-----------------|--------------|
| 1 | Fix Closure/Upvalue System | High | 1-2 weeks | None |
| 1 | Complete Error Handling | High | 1 week | None |
| 2 | Comprehensive Tests | High | 1-2 weeks | Closure System |
| 2 | Complete Standard Library | High | 1-2 weeks | None |
| 3 | Memory Management | Medium | 2 weeks | None |
| 3 | VM Optimization | Medium | 1 week | Tests |
| 3 | Compiler Optimization | Low | 1 week | Tests |
| 4 | Documentation | High | Ongoing | All components |
| 5 | Redis Integration | Low | 2 weeks | Complete Lua VM |

## Next Immediate Steps

1. Fix the closure and upvalue system to handle complex nested closures
2. Complete the string manipulation functions, especially pattern matching
3. Implement xpcall and traceback generation for error handling
4. Add memory management with garbage collection
5. Create a comprehensive test suite for all implemented features

The focus must remain on completing a fully functional, robust Lua VM before moving on to Redis integration.