# Register Window Borrow Patterns

This document outlines the key patterns and techniques used to address Rust's borrow checker constraints in the Ferrous Lua VM implementation, particularly around the register window system.

## Core Challenge

The fundamental challenge in our VM implementation is that Rust's ownership rules conflict with traditional VM designs:

1. **Multiple Mutable Borrows**: VM operations often need to modify multiple components (heap, register windows, etc.) simultaneously
2. **Nested Mutation**: Operations like function calls or metamethod handling require nested mutations
3. **Self-Reference**: The VM often needs to reference and modify its own state during execution

## General Patterns

### The Two-Phase Pattern

The most important pattern for navigating borrow checker issues is the "two-phase pattern":

```rust
// Phase 1: Extract all needed data as owned values
let extracted_data = {
    // Temporary borrow of some resource
    let resource = self.get_resource()?;
    
    // Extract all needed data as owned copies
    let data1 = resource.field1.clone();
    let data2 = resource.compute_value()?;
    
    (data1, data2)
}; // Resource borrow ends here

// Phase 2: Work with the extracted data
let (value1, value2) = extracted_data;
self.do_something_with(value1)?;
self.another_operation(value2)?;
```

This pattern is used extensively in opcode implementations to avoid overlapping borrows.

### Transaction-Based Memory Management

For heap access, we use transactions to batch changes:

```rust
// Create transaction
let mut tx = HeapTransaction::new(&mut self.heap);

// Queue changes (no immediate application)
tx.set_table_field(table, key, value)?;
tx.set_register(thread, index, value)?;

// Apply all changes at once
tx.commit()?;
```

This provides a clean abstraction for memory management but introduces performance overhead.

## Specific Opcode Patterns

### Closures and Upvalues

Closures represent one of the most complex borrow checker challenges. Our solution:

```rust
OpCode::Closure => {
    // Phase 1: Extract data from parent
    let proto_handle = {
        let parent_closure = tx.get_closure(frame.closure)?;
        // Extract proto handle only, avoiding nested borrows
        // ...
    };
    
    // Phase 2: Get proto copy in separate step
    let proto_copy = tx.get_function_proto_copy(proto_handle)?;
    
    // Phase 3: Extract parent upvalues separately
    let parent_upvalues = {
        let parent_closure = tx.get_closure(frame.closure)?;
        parent_closure.upvalues.clone()
    };
    
    // Phase 4-7: Create upvalues, closure, and store result
    // ...
}
```

Key insights:
1. Extract each piece of data in a completely separate phase
2. Use scoping to ensure borrows are dropped
3. Work with owned data (like cloned upvalues) rather than references

### Register Window to Stack Synchronization

For upvalues to work correctly, we must sync register windows to the thread stack:

```rust
// Standalone helper function to avoid self borrows
fn sync_window_to_stack_helper(
    tx: &mut HeapTransaction,
    register_windows: &RegisterWindowSystem,
    thread: ThreadHandle,
    window_idx: usize,
    register_count: usize
) -> LuaResult<()> {
    // For each register in the window
    for i in 0..register_count {
        // Get value from window
        let value = match register_windows.get_register(window_idx, i) {
            Ok(v) => v.clone(),
            Err(_) => Value::Nil,
        };
        
        // Map to stack position
        let stack_position = window_idx * 256 + i;
        
        // Ensure it's in thread stack
        tx.set_register(thread, stack_position, value)?;
    }
    
    Ok(())
}
```

This function is called before upvalue creation to ensure values are properly accessible.

### Error Handling with Transactions

When returning errors within transaction-using code:

```rust
if some_error_condition {
    // Always commit transaction before returning error
    tx.commit()?;
    return Err(LuaError::SomeError("Error message".to_string()));
}
```

This prevents transaction state errors from compounding the original error.

## Transaction Lifecycle Management

A common source of errors is mismanaging the transaction lifecycle:

### Anti-Pattern: Early Commit

```rust
// AVOID: Early commit in opcode handlers
tx.commit()?; // Commits too early

// Later in step() method
if should_increment_pc {
    tx.increment_pc(self.current_thread)?; // Error: transaction already committed!
}
```

### Pattern: Let step() Handle Commit

```rust
// DO: Let step() handle transaction lifecycle
// In opcode handler
self.register_windows.set_register(window_idx, a, value)?;
return Ok(StepResult::Continue);

// In step() method
if should_increment_pc {
    tx.increment_pc(self.current_thread)?;
}
tx.commit()?; // Single commit point
```

This ensures consistent transaction management.

## Register Protection

To prevent register values from being corrupted during complex operations:

```rust
// Protect registers during operation
self.register_windows.protect_register(window_idx, register_idx)?;

// Perform potentially register-modifying operation
self.complex_operation()?;

// Unprotect when done
self.register_windows.unprotect_register(window_idx, register_idx)?;
```

For function calls, we often unprotect all registers in the target window:

```rust
// Unprotect all registers in the current window
let _ = self.register_windows.unprotect_all(window_idx);
```

## Stack Position Calculation

A critical aspect of upvalue handling is proper stack position calculation:

```rust
// Calculate absolute stack position from window index and register
let stack_position = window_idx * 256 + register_idx;
```

This fixed mapping ensures upvalues consistently refer to the same stack location.

## Looking Ahead: Potential Improvements

The current implementation successfully navigates Rust's borrow checker constraints but faces significant performance overhead. Future improvements could include:

1. **Interior Mutability**: Replace transactions with RefCell-based approach
2. **RAII Guards**: Automate resource management with Drop trait
3. **Minimal Unsafe Blocks**: Strategically use unsafe for performance-critical paths

See [LUA_VM_PERFORMANT_HYBRID_DESIGN.md](./LUA_VM_PERFORMANT_HYBRID_DESIGN.md) for a detailed exploration of these alternatives.