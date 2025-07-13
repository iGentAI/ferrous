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
- ✅ Basic string and table handling

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

### Known Issues

1. **Nested Function Calls**: Some issues with nested function calls due to borrowing constraints
2. **Metamethod Recursion**: No protection against infinite metamethod recursion
3. **Memory Management**: No proper garbage collection yet
4. **Standard Library Gaps**: Many standard library functions are defined but incomplete

## Testing Status

The implementation passes basic tests including:
- Simple arithmetic and control flow
- Basic table creation and manipulation
- Simple functions and closures

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