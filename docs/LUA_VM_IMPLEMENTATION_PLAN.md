# Ferrous Lua VM Implementation Plan

## Executive Summary

This document outlines the comprehensive plan for refactoring and completing the Lua VM implementation in Ferrous to address the fundamental architectural issues identified in our analysis. The plan focuses on creating a robust, maintainable, and Rust-friendly architecture that naturally supports all required Lua features while working with (rather than against) Rust's ownership model.

## Root Causes of Current Issues

Our architectural analysis has identified several fundamental issues that prevent the current implementation from supporting complex Lua features:

1. **Central Ownership Conflict**: Multiple components need simultaneous mutable access to the heap, creating irreconcilable conflicts with Rust's borrow checker.
   
2. **Missing Abstraction Layers**: No clear separation exists between compile-time and runtime operations, with direct heap manipulation scattered throughout the codebase.
   
3. **Incomplete Type Representations**: Critical data structures like `FunctionProto` lack fields required to support nested functions.
   
4. **Unsafe Context Passing**: The current approach uses unsafe pointer casting to pass context between components.

These root causes manifest as symptoms like:
- Inability to support nested functions ("not implemented: nested prototypes")
- Generic for loop failures ("not implemented: generic for loops")
- Table field concatenation failures
- Borrow checker conflicts in the VM

## Architectural Solution

Our solution is based on the following key principles:

1. **Separate Compilation from Execution**: Compilation produces immutable artifacts that execution consumes.
2. **Clean Ownership Boundaries**: Components interact through clear APIs that respect Rust's ownership rules.
3. **Complete Type Representations**: Data structures fully represent all required Lua concepts.
4. **Safe Context Management**: Replace unsafe pointers with proper context handling mechanisms.
5. **Command-Based Interfaces**: Use command patterns for operations to avoid borrow checker conflicts.

## Implementation Phases and Progress

### Phase 1: Decouple Compilation from Runtime ✅ (Complete)

- ✅ Create a new compilation module with self-contained types:
  - ✅ `CompilationValue`: Represents constants during compilation
  - ✅ `CompilationProto`: Represents function prototypes without heap dependency
  - ✅ `CompilationScript`: Encapsulates compilation results with string pool

- ✅ Update compiler to use the new types:
  - ✅ Add string pooling for string deduplication
  - ✅ Properly handle nested function prototypes
  - ✅ Update all compile methods

- ✅ Update VM to load from compiled artifacts:
  - ✅ Implement loading compiled scripts
  - ✅ Create functions to materialize values from compilation artifacts
  - ✅ Update execution to work with loaded scripts

### Phase 2: Implement Proper Nested Functions 🔄 (In Progress)

- ✅ Add proper function prototype representation (through `CompilationProto`)
- ✅ Ensure compiler correctly builds nested function hierarchy
- ✅ Update VM's `get_proto` function to access nested prototypes
- 🔄 Fix nested function execution (stack overflow bug)
- ⏲️ Complete proper upvalue handling

### Phase 3: Safe Context Management ⏲️

- ⏲️ Implement thread-local storage for execution context
- ⏲️ Create a safe context registry for Redis API integration 
- ⏲️ Remove unsafe pointer usage
- ⏲️ Implement proper error handling and propagation

### Phase 4: Fix Table Operations ⏲️

- ⏲️ Redesign table operations to avoid borrow checker conflicts
- ⏲️ Implement clean register allocation for concatenation
- ⏲️ Add support for proper metamethods
- ⏲️ Fix table field concatenation bugs

## Current Progress (June 27, 2025)

We have successfully completed:

1. The creation of a self-contained compilation module (`compilation.rs`) ✅
2. Full update of the `Compiler` to use the new compilation types ✅
3. Integration of nested function prototype support in the compiler ✅
4. Update of the VM to load and execute compiled scripts ✅
5. Update of the executor to support our new compilation model ✅
6. Update of the GIL to connect compilation and execution properly ✅

A test program has been created to verify our changes, and it shows that basic functionality is now working properly:
- Simple scripts ✅
- Local variables ✅
- Lua expressions ✅
- Basic table operations ✅

However, one test that uses nested functions is still failing with a stack overflow error. This is progress compared to before (where "nested prototypes" were not implemented at all), but indicates that we still have work to do to properly handle execution of nested functions.

The update to the compiler was extensive and included:
- A complete rewrite to use `CompilationValue` instead of `Value`
- A string pooling system for deduplication of strings
- Proper storage of nested function prototypes in the parent prototype
- Complete support for all the existing language features

Next steps:
1. Fix the stack overflow issue with nested function execution
2. Implement safe context management
3. Fix table operations, particularly concatenation

## Implementation Strategy

Rather than making small changes to the current architecture, this approach requires a more significant refactoring:

1. **Start with clean separation**: Implement the compilation/execution boundary first
2. **Build on solid foundation**: Then implement nested functions properly
3. **Improve safety**: Replace unsafe context passing with proper registry
4. **Fix operand handling**: Implement clean register allocation for table operations

Each phase builds on the previous one, creating a solid foundation for the next set of changes. This ensures a coherent solution rather than a collection of patches.

## Why This Approach Will Succeed

Previous attempts failed because they tried to work within the constraints of the existing architecture, which fundamentally conflicts with Rust's ownership model. By redesigning the architecture to align with Rust's ownership patterns:

1. We eliminate the need for unsafe workarounds
2. We create clean abstraction layers with well-defined interfaces
3. We enable natural implementation of all required Lua features
4. We simplify the code by removing defensive handle checks

This approach requires more upfront investment but will lead to a more robust, maintainable, and complete implementation in the long run.

