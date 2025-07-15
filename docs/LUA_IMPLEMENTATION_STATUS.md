# Lua VM Implementation Status

## Implementation Overview

The Lua VM for Ferrous is being implemented following a unified stack architecture, which is different from the originally planned register window approach. This document outlines the current implementation status, architecture decisions, and known limitations.

## Architecture Decisions

### Unified Stack Model vs Register Windows

The initial approach for the Lua VM was to use a register window model, which would allocate separate windows of registers for each function call. However, this approach was abandoned due to fundamental incompatibilities with Lua 5.1's design:

1. **Stack Continuity Requirement**: Lua 5.1 assumes a contiguous stack where any function can access any stack position. Register windows create isolated segments that violate this assumption.

2. **C API Compatibility**: Lua's C API relies heavily on direct stack manipulation. Windows would require complex translation layers that degrade performance and correctness.

3. **Upvalue Handling**: Upvalues in Lua store absolute stack indices. With windows, these indices become meaningless across window boundaries.

4. **Performance Concerns**: Window allocation/deallocation overhead and complex index translation on every access outweigh any potential benefits.

Instead, we've implemented a unified stack model where:
- A single contiguous stack exists for all values
- Functions operate on stack slices defined by base pointers
- Registers are simply stack positions relative to the base
- This enables efficient C interoperability

### Transaction-Based Memory Safety

Rust's ownership model presents challenges for a garbage-collected VM. To ensure memory safety without sacrificing performance, we've implemented:

1. **Transaction System**: All heap access is through transactions that validate handles before use
2. **Handle Validation**: Strong type safety for different handle types (tables, strings, etc.)  
3. **Two-phase Borrowing**: Complex operations follow a two-phase pattern to avoid borrowing conflicts

### String Interning System

A proper string interning system has been implemented that ensures:

1. **Content-Based Equality**: Strings are compared by content, not handle identity
2. **Pre-interning of Common Strings**: Standard library function names and common metamethod names are pre-interned
3. **Consistent Handle Assignment**: The same string content always gets the same handle within a VM instance
4. **Transaction Integration**: String creation properly integrates with the transaction system

## Current Implementation Status

### Working Features

- ✅ VM core architecture with unified stack model
- ✅ Stack and call frame management
- ✅ Basic opcodes: MOVE, LOADK, LOADBOOL, LOADNIL
- ✅ Table operations: NEWTABLE, GETTABLE, SETTABLE
- ✅ Arithmetic operations: ADD, SUB, MUL, DIV, MOD, POW, UNM
- ✅ String operations: CONCAT
- ✅ Control flow: JMP, TEST, TESTSET
- ✅ Numerical for loops: FORPREP, FORLOOP
- ✅ Function calls: CALL, RETURN
- ✅ Basic upvalue support: GETUPVAL, SETUPVAL, CLOSURE, CLOSE
- ✅ Safe execution via transaction system
- ✅ String interning with content-based comparison
- ✅ Table operations with proper string key handling
- ✅ Global table access with string literal keys
- ✅ Basic standard library functions (print, type, tostring, tonumber, assert)

### Partially Implemented Features

- ⚠️ Table manipulation: table.insert, table.remove, etc.
- ⚠️ Metamethod support: Basic table metamethods
- ⚠️ Standard library: Partial implementation of base, string, table libraries

### Not Yet Implemented

- ❌ Generic FOR loops (for k,v in pairs())
- ❌ TFORLOOP opcode for generic iteration
- ❌ Complete metamethod support
- ❌ Coroutines
- ❌ Complete standard library implementation
- ❌ Error handling with traceback
- ❌ Garbage collection

### Recent Improvements

1. **String Interning Fix**: The string interning system now properly deduplicates strings based on content, ensuring that identical string literals get the same handle. This fixed issues with global table lookups where function names weren't being found.

2. **Table Key Handling**: Tables now correctly handle string keys with proper content-based hash calculation and equality comparison. This ensures that table field access works correctly with string keys.

3. **Standard Library Registration**: Basic standard library functions are now properly registered in the global table and accessible from Lua scripts. Functions like `print`, `type`, `tostring`, `tonumber`, and `assert` are working correctly.

### Known Issues

1. **Nested Function Calls**: Some issues with nested function calls due to borrowing constraints
2. **Complex Dynamic String Operations**: Tests with extensive dynamic string operations may cause infinite loops
3. **Metamethod Recursion**: No protection against infinite metamethod recursion
4. **Memory Management**: No proper garbage collection yet
5. **Standard Library Gaps**: Many standard library functions are defined but incomplete

## Testing Status

The implementation passes basic tests including:
- Simple arithmetic and control flow
- Basic table creation and manipulation  
- Simple functions and closures
- Standard library function calls
- String interning and table key access

More complex tests involving generic iteration and metamethods are still failing.

## Next Steps

1. Implement generic FOR loops (TFORLOOP)
2. Enhance metamethod support
3. Add proper error handling with traceback
4. Extend standard library coverage
5. Implement memory management/garbage collection
6. Add coroutine support

## Implementation Approach

The implementation follows these key patterns:

1. **Non-Recursive Execution**: All function calls and complex operations are queued rather than executed recursively to avoid stack overflow
2. **Transaction-Based Safety**: All memory operations use transactions for safety and clean error handling
3. **Static Opcode Handlers**: Opcode handlers are implemented as static methods to avoid borrowing conflicts
4. **Two-Phase Borrowing**: Complex operations that need multiple borrows use a two-phase approach
5. **String Interning**: All string creation goes through a deduplication system to ensure content-based equality

## Bytecode Compatibility

The VM is designed to be compatible with Lua 5.1 bytecode. The bytecode format follows the Lua 5.1 specification:
- 32-bit instructions
- 6-bit opcode field
- Various operand formats (ABC, ABx, AsBx)

## C API Compatibility

The C API compatibility is being maintained through careful mapping of C functions to the VM's internal structure:
- C functions receive an ExecutionContext that safely wraps the VM state
- Proper stack manipulation APIs are provided
- Type checking and error handling follow Lua 5.1 conventions