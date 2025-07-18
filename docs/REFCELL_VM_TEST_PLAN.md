# RefCellVM Test Plan

## Overview

This document outlines testing strategies and plans specifically for the RefCellVM implementation of the Ferrous Lua interpreter. It defines test categories, methodologies, and priorities to ensure thorough validation of the RefCellVM architecture.

## Test Categories

### 1. Unit Tests

Unit tests focus on individual components of the VM in isolation:

| Component | Test Focus | Priority |
|-----------|------------|----------|
| RefCellHeap | String operations, table operations, upvalue handling | High |
| Handle System | Handle creation, validation, comparison | High |
| Value Types | Creation, conversion, comparison | Medium |
| Opcode Implementation | Individual opcode functionality | High |
| Error Handling | Correct error generation and propagation | Medium |

### 2. Integration Tests

Integration tests verify the interaction between multiple components:

| Integration | Test Focus | Priority |
|-------------|------------|----------|
| VM + Heap | Register access, function calls, value persistence | High |
| VM + Standard Library | Library function execution, C function callbacks | High |
| Compiler + VM | Bytecode execution, module loading | Medium |
| VM + Error Handling | Error propagation and recovery | Medium |

### 3. Functional Tests

Functional tests validate high-level behavior in realistic scenarios:

| Category | Test Focus | Priority |
|----------|------------|----------|
| Basic Language | Variable definitions, arithmetic, strings | High |
| Tables | Creation, access, nested tables, iteration | High |
| Functions | Definition, calls, closures, recursion | High |
| Control Flow | Branching, loops, generic iteration | High |
| Standard Library | Library function behavior | Medium |
| Redis API | Redis integration, KEYS/ARGV tables | High |

### 4. Stress Tests

Stress tests verify behavior under extreme conditions:

| Stress Category | Test Focus | Priority |
|-----------------|------------|----------|
| Memory Usage | Large data structures, many objects | Low |
| Call Depth | Deeply nested function calls | Medium |
| Large Scripts | Very large Lua scripts | Low |
| Concurrency | Multiple VM instances | Medium |

## Testing Strategy

### 1. Test-Driven Development

For new features, follow this TDD approach:

1. Write a failing test that defines expected behavior
2. Implement the minimum code needed to pass the test
3. Refactor the code for clarity and maintainability
4. Repeat for additional functionality

### 2. Regression Prevention

For existing functionality:

1. Maintain a suite of tests for verified features
2. Run all regression tests before and after significant changes
3. Document any behavioral changes that differ from Lua 5.1 specification

### 3. Test Environment

Set up consistent test environments:

1. Use script-based test execution
2. Create isolated VM instances for each test
3. Standardize error handling and reporting

## Test Implementation

### Test File Organization

```
tests/
  lua/
    unit/
      refcell_heap_tests.rs
      handle_tests.rs
      value_tests.rs
      opcode_tests.rs
    integration/
      vm_heap_integration_tests.rs
      stdlib_integration_tests.rs
    functional/
      basic_language_tests.lua
      table_tests.lua
      function_tests.lua
      control_flow_tests.lua
      stdlib_tests.lua
      redis_api_tests.lua
    stress/
      memory_stress_tests.lua
      recursion_tests.lua
```

### Test Case Template

```rust
#[test]
fn test_feature_behavior() {
    // Arrange
    let mut vm = RefCellVM::new().unwrap();
    vm.init_stdlib().unwrap();
    
    // Act
    let result = vm.eval_script("lua code here").unwrap();
    
    // Assert
    match result {
        Value::Number(n) => assert_eq!(n, 42.0),
        _ => panic!("Expected number result"),
    }
}
```

## Test Automation

### Continuous Testing

1. Run the basic test suite with each commit
2. Run the full test suite on PR merges
3. Run stress tests weekly or before releases

### Test Output Analysis

Use standardized test output for analysis:

1. Categorize test failures by feature area
2. Track test coverage over time
3. Generate test status reports

## Test Priorities

Based on current implementation status:

### Phase 1: Core Functionality

1. **FOR Loop Correctness**: Ensure numerical FOR loops work correctly with all parameter variations
2. **Basic Table Operations**: Test table creation, field access, and simple modifications
3. **VM State Management**: Verify register handling and state isolation

### Phase 2: Function Implementation

1. **Function Definition and Call**: Test defining and calling functions
2. **Closure and Upvalue Handling**: Test closure creation and upvalue access
3. **Recursion Handling**: Test both direct and mutual recursion

### Phase 3: Advanced Features

1. **Generic FOR Loops**: Test pairs() and ipairs() iteration
2. **Metamethods**: Test metamethod handling for various operations
3. **Standard Library**: Test comprehensive standard library functions

### Phase 4: Edge Cases

1. **Error Handling**: Test error generation and recovery
2. **Resource Limits**: Test behavior with limited resources
3. **Boundary Conditions**: Test edge cases for all operations

## Feature Test Plan

### FOR Loops (High Priority)

| Test Case | Description | Expected Result |
|-----------|-------------|-----------------|
| Basic FOR | `for i=1,5 do end` | Loop executes 5 times |
| FOR with Step | `for i=1,10,2 do end` | Loop executes 5 times with values 1,3,5,7,9 |
| FOR with Negative Step | `for i=5,1,-1 do end` | Loop executes 5 times with values 5,4,3,2,1 |
| FOR with Fractional Step | `for i=1,2,0.5 do end` | Loop executes 3 times with values 1,1.5,2 |
| FOR with Nil Step | `for i=1,5,nil do end` | Step defaults to 1, loop executes 5 times |
| FOR without Iterations | `for i=1,0 do end` | Loop doesn't execute |

### Table Operations (High Priority)

| Test Case | Description | Expected Result |
|-----------|-------------|-----------------|
| Table Creation | `t = {}` | Empty table created |
| Table Assignment | `t.x = 1` | Field 'x' has value 1 |
| Table Access | `return t.x` | Returns value 1 |
| Array Access | `t[1] = "a"` | Array element 1 has value "a" |
| Mixed Table | `t = {1, 2, x=3}` | Both array and hash parts populated |
| Table Length | `return #t` | Returns correct length of array part |

### Functions (High Priority)

| Test Case | Description | Expected Result |
|-----------|-------------|-----------------|
| Function Definition | `function f() return 1 end` | Function defined and callable |
| Function Call | `return f()` | Returns 1 |
| Function Parameters | `function f(a,b) return a+b end` | Parameters properly received |
| Multiple Returns | `function f() return 1,2 end` | Multiple return values handled |
| Closure | `function f() local x=1 return function() return x end end` | Upvalue captured correctly |
| Recursion | `function fact(n) if n<=1 then return 1 else return n*fact(n-1) end end` | Correct factorial calculation |

## Conclusion

This test plan provides a comprehensive approach to validating the RefCellVM implementation. By prioritizing tests based on the core functionality and current implementation status, we can efficiently verify the VM's correctness while guiding ongoing development efforts. Regular execution of these tests will ensure the RefCellVM remains robust and compatible with Lua 5.1 as required by the Redis specification.