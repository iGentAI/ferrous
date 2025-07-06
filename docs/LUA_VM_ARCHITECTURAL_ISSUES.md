# Lua VM Architectural Issues Tracker

This document tracks significant architectural issues in the Ferrous Lua VM implementation, categorizing them as resolved or pending. This helps maintain awareness of design-level issues that go beyond simple TODOs or placeholders.

Last updated: July 2025

## Resolved Architectural Issues

### 1. Register Allocation System (Fixed July 2025)

**Issue**: The compiler's register allocation system was fundamentally mismatched with the VM's execution model, leading to register conflicts in nested expressions.

**Root Cause**: The register allocator's `free_to()` method was completely resetting allocation state without respecting register lifetimes:

```rust
// Update used register count - THIS WAS THE PROBLEM
self.used = level;  // This completely reset allocation state!
```

**Impact**: 
- Register conflicts between parent and child contexts
- Function handles being overwritten by nested expression results
- Type errors when executing valid code (e.g., "expected function, got string")

**Solution**:
- Implemented proper register lifetime tracking system
- Added register preservation mechanism across nested expressions
- Updated all compiler operations to preserve registers as needed
- Fixed VM operations to safely handle register values

### 2. String Interning (Verified June 2025)

**Issue**: String handles for identical content were inconsistent, leading to incorrect function lookups.

**Root Cause**: Inconsistent string interning between compilation and execution phases.

**Impact**: 
- Function lookups failing when strings appeared in different contexts
- Value identity semantics not properly maintained
- Unnecessary string duplication

**Solution**:
- Verified proper string interning throughout the codebase
- Added pre-interning for common strings in heap initialization
- Enhanced string cache for consistent handles
- Ensured proper string identity semantics in comparison operations

### 3. Bytecode Encoding (Fixed July 2025)

**Issue**: Bytecode generator was producing incorrect opcode numbers.

**Root Cause**: Direct enum casting instead of using the mapping function.

**Impact**:
- Wrong operations being executed
- Return statements interpreted as ForPrep instructions
- Out of bounds memory access

**Solution**:
- Modified encoding functions to use proper opcode mapping
- Ensured consistency between compiler and VM opcode values

### 4. RETURN Opcode Parameter Handling (Fixed July 2025)

**Issue**: Table return values weren't being properly returned from Lua scripts.

**Root Cause**: The compiler's `emit_return` method was generating RETURN instructions with B=1 (meaning "return 0 values") even when returning a table expression.

**Impact**:
- Tables created in scripts couldn't be returned
- Scripts returning tables would instead return nil
- Inability to return complex data structures from scripts

**Solution**:
- Fixed the `emit_return` method to properly calculate the B parameter
- Ensured that B=2 (meaning "return 1 value") is used when expressions exist
- Added fallback mechanism when register count doesn't match expression count
- Verified that tables can now be returned from scripts

### 5. LOADNIL Opcode Parameter Handling (Fixed July 2025)

**Issue**: LOADNIL was setting B+1 registers to nil instead of B registers.

**Root Cause**: Loop condition in the opcode implementation used an inclusive range instead of exclusive:

```rust
// Incorrect implementation
for i in 0..=b {
    tx.set_register(self.current_thread, base + a + i, Value::Nil)?;
}
```

**Impact**:
- One extra register being set to nil beyond what's expected
- Potential unwanted register overwriting
- Deviation from Lua 5.1 specification

**Solution**:
- Modified the loop to use exclusive range to match Lua 5.1 spec:
```rust
// Fixed implementation
for i in 0..b {
    tx.set_register(self.current_thread, base + a + i, Value::Nil)?;
}
```

## Pending Architectural Issues

### 1. Memory Management (Unimplemented)

**Issue**: The VM has no garbage collection mechanism, leading to unbounded memory growth.

**Root Cause**: The implementation focuses on a handle-based memory system without a reclamation strategy.

**Impact**:
- Memory leaks during long-running scripts
- No pressure monitoring or limits
- Potential crashes in resource-constrained environments

**Required Solution**:
- Implement a non-recursive mark-and-sweep garbage collector
- Add generational collection for performance
- Include memory pressure monitoring
- Enforce memory limits

### 2. Transaction Pattern Inconsistency (Partial)

**Issue**: The codebase has inconsistent usage patterns of the transaction system.

**Root Cause**: Different components were developed with slightly different assumptions about transaction creation and ownership.

**Impact**:
- Some code creates transactions for every operation
- Other code expects transactions to be passed in
- Potential for subtle bugs from transaction interaction
- Nested transaction issues

**Required Solution**:
- Standardize transaction usage patterns
- Clearly define transaction ownership rules
- Implement proper transaction nesting if needed
- Update VM and stdlib to use consistent patterns

### 3. Execution Context Abstraction (Partial)

**Issue**: The ExecutionContext abstraction has inconsistencies and leaky abstractions.

**Root Cause**: The context tries to provide a clean boundary but often has to reach into VM internals.

**Impact**:
- Standard library functions often need to create transactions to access heap
- Inconsistent helper methods across different modules
- Some interfaces require direct VM/heap access

**Required Solution**:
- Refine the ExecutionContext abstraction
- Provide consistent helper methods for common operations
- Clearly separate VM internals from C function interface

### 4. Metamethod Resolution (Inconsistent)

**Issue**: Metamethod resolution is implemented inconsistently across different operation types.

**Root Cause**: Each operation type (arithmetic, table access, comparison, etc.) has its own metamethod lookup logic with varying approaches.

**Impact**:
- Some operations properly check for metamethods
- Other operations have placeholder implementations
- Inconsistent behavior across operation types

**Required Solution**:
- Standardize metamethod resolution across all operation types
- Ensure proper precedence rules for metamethods
- Complete all metamethod implementations

### 5. Error Handling Architecture (Incomplete)

**Issue**: Error handling lacks proper traceback generation and error context.

**Root Cause**: Initial focus on core execution rather than error facilities.

**Impact**:
- Limited error information
- No stack traces
- Inconsistent error handling between VM and C functions

**Required Solution**:
- Implement traceback generation
- Add source location to errors
- Complete pcall/xpcall implementation
- Ensure error propagation is consistent

### 6. Closure and Upvalue Handling (Partial)

**Issue**: Complex closure scenarios and upvalue management have limitations.

**Root Cause**: The upvalue handling logic isn't fully handling complex closure nesting and lifecycle management.

**Impact**:
- Simple closures work but complex patterns fail
- Upvalue capture may not work correctly across multiple closure levels
- Upvalue closing needs improvement

**Required Solution**:
- Complete the upvalue management system
- Ensure proper upvalue sharing between closures
- Fix upvalue closing mechanism
- Test with complex closure patterns

## Architectural Principles Compliance

Despite the pending issues, the implementation follows these core architectural principles:

1. **Non-Recursive State Machine**: ✅ Compliant
   - Execution loop has no recursion
   - Operations are properly queued
   - No stack overflow risk from nested calls

2. **Transaction-Based Heap Access**: ✅ Compliant
   - All heap operations go through transactions
   - No direct heap access outside transactions
   - Proper commit/rollback semantics

3. **Handle-Based Memory Management**: ✅ Compliant
   - All dynamic objects use arena-based handles
   - Generational IDs prevent use-after-free
   - Type-safe handle wrappers

4. **Two-Phase Borrowing**: ✅ Compliant
   - Complex operations properly use two-phase pattern
   - Avoids borrow checker fights

5. **Type-Safe Handle Validation**: ✅ Compliant
   - Validation happens at transaction boundaries
   - Validation caching for performance
   - Proper error propagation

## Conclusion

The core VM architecture issues have been fixed, making the VM capable of executing basic to moderately complex Lua scripts. Tables can now be properly returned from scripts, and opcodes are correctly implemented according to the Lua 5.1 specification. However, there are still significant architectural gaps in memory management, upvalue handling, transaction patterns, and error handling. These should be addressed systematically while completing the standard library and before moving to Redis integration.

The most critical issues to address are the upvalue/closure system and memory management, as these limitations will prevent the VM from running many real-world Lua scripts.