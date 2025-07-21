# RC RefCell VM Implementation Test Plan

## Overview

This document describes the testing strategy for the Ferrous RC RefCell Lua VM, updated to reflect the established 24% baseline and current test infrastructure using compile_and_execute.

## Current Test Status (July 2025)

### **Baseline Results**
- **Total Tests**: 21 comprehensive Lua tests
- **Pass Rate**: 24% (5 passed, 16 failed)
- **Test Infrastructure**: Functional using compile_and_execute binary

### **Category Breakdown**

| Category | Passed | Failed | Pass Rate | Priority |
|----------|---------|--------|-----------|-----------|
| Basic Language Features | 3/6 | 3/6 | 50% | Medium |
| Table Operations | 0/3 | 3/3 | 0% | **HIGH** |
| Functions and Closures | 1/5 | 4/5 | 20% | **HIGH** |
| Control Flow | 1/3 | 2/3 | 33% | Medium |
| Standard Library | 0/4 | 4/4 | 0% | **CRITICAL** |

## Test Infrastructure

### Primary Test Runner

**`test_suite.sh`** - Comprehensive test suite that runs 21 Lua scripts:
```bash
./test_suite.sh
```

**`compile_and_execute` Binary** - Test execution engine:
```bash
./target/release/compile_and_execute script.lua
```

### Test Organization

```
tests/lua/
├── basic/          # Basic language features (50% pass rate)
├── tables/         # Table operations (0% pass rate)  
├── functions/      # Functions and closures (20% pass rate)
├── control/        # Control flow (33% pass rate)
└── stdlib/         # Standard library (0% pass rate)
```

## Priority Test Areas

### **CRITICAL Priority: Standard Library (0% Pass Rate)**

**Issue**: All standard library functions are failing, indicating integration problems.

**Tests Failing**:
- `type()` function
- `tostring()` function  
- `assert()` function
- Error handling

**Root Cause**: C function interface integration issues between rc_stdlib.rs and rc_vm.rs

**Implementation Focus**:
1. Fix ExecutionContext interface in rc_stdlib.rs
2. Ensure proper C function calling convention
3. Verify standard library function registration

### **HIGH Priority: Table Operations (0% Pass Rate)**

**Issue**: All table operations are failing, indicating fundamental table system problems.

**Tests Failing**:
- Table creation: `{a=1, b=2}`
- Raw operations: `rawget`, `rawset`
- Metamethods: `__index`, `__newindex`

**Root Cause**: Table field access and metamethod integration issues

**Implementation Focus**:
1. Fix table field access in rc_heap.rs
2. Repair metamethod integration
3. Ensure proper table construction

### **HIGH Priority: Functions and Closures (20% Pass Rate)**

**Issue**: Advanced function features are broken, but basic definitions work.

**Tests Passing**: Function definitions
**Tests Failing**: Closures, upvalues, variable arguments, tail calls

**Root Cause**: Upvalue sharing and closure creation issues

**Implementation Focus**:
1. Fix upvalue capture in the CLOSURE opcode
2. Ensure proper upvalue sharing between closures
3. Repair variable arguments handling

## Test Development Strategy

### Phase 1: Standard Library Recovery (Week 1)

**Goal**: Achieve 40%+ pass rate by fixing standard library

**Tests to Fix**:
1. `tests/lua/basic/type.lua`
2. `tests/lua/basic/tostring.lua`  
3. `tests/lua/stdlib/base.lua`
4. `tests/lua/stdlib/errors.lua`

**Technical Tasks**:
1. Fix ExecutionContext trait implementation
2. Repair C function calling in process_c_function_call
3. Update standard library registration

### Phase 2: Table System Recovery (Week 2)

**Goal**: Achieve 60%+ pass rate by fixing table operations

**Tests to Fix**:
1. `tests/lua/tables/create.lua`
2. `tests/lua/tables/rawops.lua`
3. `tests/lua/basic/concat.lua` (table construction)

**Technical Tasks**:
1. Fix table field access in get_table_field/set_table_field
2. Repair table construction in NEWTABLE opcode
3. Fix metamethod integration

### Phase 3: Functions and Closures (Week 3)

**Goal**: Achieve 80%+ pass rate by fixing advanced function features

**Tests to Fix**:
1. `tests/lua/functions/closure.lua`
2. `tests/lua/functions/upvalue_simple.lua`
3. `tests/lua/functions/varargs.lua`

**Technical Tasks**:
1. Fix CLOSURE opcode upvalue capture
2. Repair upvalue sharing mechanism
3. Fix VARARG opcode implementation

## Testing Methodology

### Regression Testing

Run full test suite after each fix:
```bash
./test_suite.sh | tee test_results_$(date +%Y%m%d).txt
```

### Debugging Failed Tests

For failed tests, examine debug output:
```bash
./target/release/compile_and_execute tests/lua/basic/type.lua
```

Debug output shows:
- Compilation process (parser, codegen)
- VM execution (register operations, opcodes)
- Error points (where execution fails)

### Unit Testing

Individual opcode testing:
```rust
#[test]
fn test_add_opcode() -> LuaResult<()> {
    let mut vm = RcVM::new()?;
    let script = "return 2 + 3";
    let module = compile(script)?;
    let result = vm.execute_module(&module, &[])?;
    assert_eq!(result, Value::Number(5.0));
    Ok(())
}
```

### Integration Testing

Full script testing:
```rust
#[test]
fn test_function_call() -> LuaResult<()> {
    let mut vm = RcVM::new()?;
    vm.init_stdlib()?;
    let script = "function add(a, b) return a + b end; return add(2, 3)";
    let module = compile(script)?;
    let result = vm.execute_module(&module, &[])?;
    assert_eq!(result, Value::Number(5.0));
    Ok(())
}
```

## Performance Testing

Once functionality is restored, performance testing:

```bash
# Time basic operations
./target/release/compile_and_execute tests/lua/performance/arithmetic.lua

# Memory usage testing
valgrind ./target/release/compile_and_execute tests/lua/performance/memory.lua
```

## Success Metrics

### Short Term (1 Month)
- **60%+ Pass Rate**: Fix standard library and tables
- **Clean Debug Output**: Reduce error spam in test runs
- **Stable Architecture**: No runtime panics

### Medium Term (3 Months)  
- **80%+ Pass Rate**: Fix functions and closures
- **Redis Integration**: Basic EVAL command working
- **Performance Baseline**: Establish speed benchmarks

### Long Term (6 Months)
- **95%+ Pass Rate**: Near-complete Lua 5.1 compatibility
- **Production Ready**: Full Redis Lua compatibility
- **Optimized Performance**: Competitive with other Lua VMs

## Conclusion

The RC RefCell VM has a solid foundation with a 24% baseline. The test infrastructure is functional and provides clear feedback on what needs to be fixed. The priority order is:

1. **Standard Library** (CRITICAL) - 0% pass rate blocking basic functionality
2. **Table Operations** (HIGH) - 0% pass rate blocking core Lua features  
3. **Functions/Closures** (HIGH) - 20% pass rate with missing advanced features

Following this plan will systematically improve the pass rate and achieve full Lua 5.1 compatibility.