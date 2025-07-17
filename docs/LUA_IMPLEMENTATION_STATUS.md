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
- ✅ Stack and call frame management with proper stack growth
- ✅ Basic opcodes: MOVE, LOADK, LOADBOOL, LOADNIL
- ✅ Table operations: NEWTABLE, GETTABLE, SETTABLE
- ✅ Table array initialization with SETLIST
- ✅ Arithmetic operations: ADD, SUB, MUL, DIV, MOD, POW, UNM
- ✅ String operations: CONCAT
- ✅ String type operations and conversions
- ✅ Control flow: JMP, TEST, TESTSET
- ✅ Function calls: CALL, RETURN (both global and local functions)
- ✅ Method calls with proper self parameter handling
- ✅ Tail calls (TAILCALL) for optimized recursion
- ✅ Variable argument functions (VARARG opcode)
- ✅ Basic upvalue support: GETUPVAL, SETUPVAL, CLOSURE, CLOSE
- ✅ Closures with proper upvalue capturing
- ✅ Basic metatable mechanism (__index metamethod)
- ✅ Safe execution via transaction system
- ✅ String interning with content-based comparison
- ✅ Table operations with proper string key handling
- ✅ Global table access with string literal keys
- ✅ Basic standard library functions (print, type, tostring, tonumber, assert)
- ✅ Circular reference handling in nested calls
- ✅ Deep recursive function support

### Partially Implemented Features

- ⚠️ Numerical for loops: FORPREP, FORLOOP (still has issues with step register handling)
- ⚠️ Table manipulation: table.insert, table.remove, etc.
- ⚠️ Metamethod support: Basic table metamethods
- ⚠️ TFORLOOP opcode for generic iteration
- ⚠️ Standard library: Partial implementation of base, string, table libraries

### Not Yet Implemented

- ❌ Generic FOR loops (for k,v in pairs()) - The infrastructure exists but reliability issues remain
- ❌ Complete metamethod support
- ❌ Coroutines
- ❌ Complete standard library implementation
- ❌ Error handling with traceback
- ❌ Garbage collection

### Recent Improvements

1. **TAILCALL Implementation**: Added full support for tail call optimization, allowing recursive functions that don't grow the stack. This implementation correctly reuses the current stack frame, closes any required upvalues, and handles call result propagation according to the Lua 5.1 specification.

2. **VARARG Opcode Support**: Implemented the VARARG opcode for handling variable argument functions. The implementation correctly handles both explicit argument count (B > 0) and "all varargs" mode (B = 0).

3. **SETLIST Implementation**: Added support for bulk array initialization with the SETLIST opcode. This allows efficient table array initialization, properly handling the FPF (Fields Per Flush) constant from Lua 5.1.

4. **Improved Metatable Support**: Enhanced the implementation of the __index metamethod, allowing tables to properly inherit properties from their metatables.

5. **String Handling Improvements**: Enhanced string concatenation (CONCAT opcode) with proper memory management and type conversion, and improved the string interning system to ensure consistent handles for identical strings.

### Known Issues

1. **For Loop Register Corruption**: Numeric for loops (FORPREP/FORLOOP) have issues with register handling, particularly with the step register becoming nil during execution.

2. **TFORLOOP Reliability**: While implemented, TFORLOOP (generic for loops) has reliability issues with complex iteration.

3. **Metamethod Recursion**: No protection against infinite metamethod recursion.

4. **Memory Management**: No proper garbage collection yet.

5. **Standard Library Gaps**: Many standard library functions are defined but incomplete.

## Testing Status

Our testing confirms that the implementation successfully handles:
- ✅ Simple arithmetic and control flow
- ✅ Table creation, access and manipulation
- ✅ String operations (concatenation, type conversion)
- ✅ Function definition and calls
- ✅ Closure creation with proper upvalue handling
- ✅ Method calls with self parameter
- ✅ Tail calls and recursion
- ✅ Variable argument functions
- ✅ Basic metamethod functionality (__index)

However, the following still have issues:
- ❌ Numeric for loops (register corruption issues)
- ❌ Generic for loops with pairs/ipairs (unreliable)
- ❌ Complex metamethod chains

## Next Steps

1. Fix the for loop register corruption issues
2. Complete and stabilize TFORLOOP implementation
3. Enhance metamethod support
4. Add proper error handling with traceback
5. Extend standard library coverage
6. Implement memory management/garbage collection
7. Add coroutine support

## Implementation Approach

The implementation follows these key patterns:

1. **Non-Recursive Execution**: All function calls and complex operations are queued rather than executed recursively to avoid stack overflow
2. **Transaction-Based Safety**: All memory operations use transactions for safety and clean error handling
3. **Static Opcode Handlers**: Opcode handlers are implemented as static methods to avoid borrowing conflicts
4. **Two-Phase Borrowing**: Complex operations that need multiple borrows use a two-phase approach
5. **String Interning**: All string creation goes through a deduplication system to ensure content-based equality
6. **Dynamic Stack Management**: Stack automatically grows as needed to support deep recursion and complex call patterns
7. **Strict Register Allocation**: Register allocation carefully follows Lua 5.1 specification to avoid corruption

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