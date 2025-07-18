# Migration from Transaction VM to RefCellVM

## Overview

This document explains the migration of the Ferrous Lua VM from a transaction-based architecture to the RefCellVM architecture using Rust's interior mutability pattern. It covers the motivation behind the change, key design decisions, implementation challenges, and lessons learned during the migration process.

## Motivation for Migration

The transaction-based VM was designed to ensure memory safety in Rust by wrapping all heap operations in atomic transactions. While this approach theoretically provided safety, it created several serious issues in practice:

1. **Register Corruption in FOR Loops**: The most critical issue was register corruption in numeric FOR loops. The transaction boundary between FORPREP and FORLOOP instructions resulted in step values being lost, causing broken loops or infinite iterations.

2. **Artificial Boundaries**: The transaction-based approach created artificial boundaries between operations that logically needed to be continuous.

3. **Complex State Management**: Tracking pending changes and committing them atomically added significant complexity to the codebase.

4. **Performance Overhead**: The transaction system added overhead for cloning values, tracking changes, and committing transactions.

## The RefCellVM Alternative

The RefCellVM architecture uses Rust's `RefCell` type to provide interior mutability, allowing safe mutable access to shared data without transaction boundaries:

1. **Direct Heap Access**: Operations access the heap directly through RefCell borrowing, eliminating transaction boundaries.

2. **Preserved Memory Safety**: The RefCell type ensures Rust's memory safety rules are still enforced at runtime.

3. **Simplified Design**: The design is more straightforward without the complex transaction tracking machinery.

4. **Two-Phase Borrow Pattern**: Complex operations use a two-phase borrow pattern to avoid borrow checker conflicts.

## Key Design Decisions

### 1. Interior Mutability with RefCell

The core design choice was to wrap each arena in a `RefCell` to enable interior mutability:

```rust
pub struct RefCellHeap {
    strings: RefCell<Arena<LuaString>>,
    tables: RefCell<Arena<Table>>,
    // Other arenas...
}
```

This allows mutable access to individual arenas without requiring mutable access to the entire heap.

### 2. Type-Safe Handles

We maintained the typed handle approach from the original design to ensure type safety:

```rust
pub struct StringHandle(pub(crate) Handle<LuaString>);
pub struct TableHandle(pub(crate) Handle<Table>);
// Other handle types...
```

These provide compile-time type checking while allowing runtime validation.

### 3. Non-Recursive Execution

We preserved the non-recursive execution model using a queue-based approach for function calls and returns:

```rust
pub enum PendingOperation {
    FunctionCall { func_index: usize, nargs: usize, expected_results: i32 },
    CFunctionCall { function: CFunction, base: u16, nargs: usize, expected_results: i32 },
    Return { values: Vec<Value> },
    // Other operations...
}
```

This prevents stack overflow in deeply nested operations.

### 4. ExecutionContext Trait

We introduced a trait-based abstraction for C function interaction:

```rust
pub trait ExecutionContext {
    fn arg_count(&self) -> usize;
    fn get_arg(&self, index: usize) -> LuaResult<Value>;
    fn push_result(&mut self, value: Value) -> LuaResult<()>;
    // Other methods...
}
```

This provides a clean interface for standard library functions without requiring knowledge of the VM implementation details.

## Implementation Challenges and Solutions

### 1. Register Corruption in FOR Loops

**Problem**: In the transaction-based VM, the FORPREP and FORLOOP operations were separated by transaction boundaries, causing register corruption.

**Solution**: The RefCellVM implementation provides direct access to registers without transaction boundaries:

```rust
// Handle step value with default
let step_num = match step {
    Value::Number(n) => n,
    Value::Nil => {
        // Directly update the step register
        self.heap.set_thread_register(self.current_thread, loop_base + 2, &Value::Number(1.0))?;
        1.0
    },
    _ => return Err(LuaError::TypeError { /* ... */ }),
};
```

### 2. Borrow Checker Conflicts

**Problem**: Mutable borrows of the same `RefCell` in complex operations caused runtime borrow errors.

**Solution**: The two-phase borrow pattern extracts all needed data before performing mutations:

```rust
// Phase 1: Get all data needed with immutable borrows
let data = {
    let immutable_borrow = some_ref_cell.borrow()?;
    immutable_borrow.some_data.clone()
};
// Borrow dropped here

// Phase 2: Now perform mutations with a new borrow
let mut mutable_borrow = some_ref_cell.borrow_mut()?;
mutable_borrow.modify_using(data);
```

### 3. String Comparison

**Problem**: String handles were compared by identity (handle value) rather than content.

**Solution**: We maintained a content-based string interning system and additional content-based comparison:

```rust
impl PartialEq for HashableValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (HashableValue::String(handle_a, hash_a), HashableValue::String(handle_b, hash_b)) => {
                // Fast path: same handle means same string
                if handle_a == handle_b {
                    return true;
                }
                // Content-based comparison using cached hashes
                hash_a == hash_b
            },
            // Other cases...
        }
    }
}
```

### 4. C Function Integration

**Problem**: The C function type definition needed to match the ExecutionContext trait.

**Solution**: Updated the C function type to take a trait object:

```rust
pub type CFunction = fn(&mut dyn ExecutionContext) -> LuaResult<i32>;
```

## Migration Process

The migration followed these steps:

1. **Dual Implementation**: Initially maintained both VM implementations side by side
2. **Refcell Implementation**: Implemented the RefCellVM and RefCellHeap classes
3. **Standard Library Adaptation**: Updated standard library functions to use the ExecutionContext trait
4. **VM Alias**: Aliased RefCellVM as LuaVM in the public API
5. **Removal**: Removed the transaction-based VM after testing

## Verification and Testing

Various tests were conducted to ensure the new implementation worked correctly:

1. **Simple Scripts**: Basic features like variable assignment and arithmetic
2. **FOR Loops**: Numeric for loops with various step values
3. **Table Operations**: Simple table creation and access
4. **Full Test Suite**: Complete test suite to identify working and non-working features

## Lessons Learned

1. **Transaction Boundaries Matter**: The transaction boundaries introduced subtle bugs that were difficult to diagnose.

2. **Interior Mutability is Powerful**: Rust's interior mutability patterns provide powerful tools for managing complex shared state.

3. **Two-Phase Borrowing**: The two-phase borrow pattern (read everything needed, then modify) is essential when working with complex systems using RefCell.

4. **Architecture Before Implementation**: Investing in the right architecture pays off later in terms of correctness and maintainability.

5. **Gradual Migration**: Maintaining both implementations during the transition enabled proper testing and validation.

## Future Considerations

While the RefCellVM architecture resolves the immediate issues, some areas need further consideration:

1. **Performance Optimization**: The current implementation might have performance overhead from runtime borrow checking.

2. **Garbage Collection**: A proper garbage collection mechanism will need to be implemented.

3. **Coroutine Support**: Implementing coroutines with the RefCell pattern will require careful design.

## Conclusion

The migration from transaction-based VM to RefCellVM has successfully addressed the critical issues with FOR loop register corruption and simplified the codebase. The new architecture provides a solid foundation for completing the remaining Lua features while maintaining Rust's memory safety guarantees.

While significant work remains to fully implement all Lua features, the core architectural issues have been resolved, paving the way for more straightforward development of the remaining functionality.