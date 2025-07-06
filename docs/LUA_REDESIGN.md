# Lua VM Redesign for Ferrous

## Background and Motivation

After extensive implementation attempts, we've determined that a complete redesign and reimplementation of the Ferrous Lua VM is necessary. The current implementation, while following some of the design principles, has systematic issues that make incremental fixes impractical.

## Current Implementation Issues

The current implementation has several fundamental issues:

1. **Inconsistent Architectural Application**: The transaction-based, handle-oriented architecture is applied inconsistently, creating borrow checker conflicts.

2. **Mixed Execution Models**: The code mixes recursive and non-recursive execution patterns, leading to stack overflows and borrow checker issues.

3. **Direct Heap Access**: Many operations bypass the transaction system, causing ownership conflicts.

4. **C Function Integration Issues**: The handling of C functions creates borrow checker conflicts due to improper boundary management.

5. **Metamethod Processing Conflicts**: Metamethods are processed recursively rather than through the state machine.

## Redesign Approach

Rather than attempting to fix these issues incrementally, we propose a complete redesign and reimplementation based on a systematic application of the core architectural principles.

The redesign will follow these principles:

### 1. Consistent Transaction-Based Access

Every heap operation must go through transactions:

```rust
// In execute_opcode method
let mut tx = Transaction::new(&mut self.heap);

// ALL heap access through the transaction
let string = tx.create_string("index")?;
let table = tx.get_table(handle)?;
tx.set_table_field(table, key, value)?;

// Commit changes atomically
tx.commit()?;
```

### 2. Pure State Machine Execution

Replace all recursive execution with a pure state machine approach:

```rust
// Main execution loop
fn execute_function(&mut self, closure: ClosureHandle, args: &[Value]) -> Result<Value> {
    // Push initial frame
    self.queue_operation(Operation::Call { closure, args: args.to_vec() });
    
    // Single execution loop - NO RECURSION
    while let Some(op) = self.operation_queue.pop_front() {
        match self.execute_operation(op)? {
            ExecutionResult::Continue => continue,
            ExecutionResult::Return(value) => return Ok(value),
            ...
        }
    }
}
```

### 3. Command Pattern for All Operations

All state transitions must be explicit commands:

```rust
// Define all possible operations
enum Operation {
    Call { closure: ClosureHandle, args: Vec<Value> },
    Return { values: Vec<Value> },
    TableGet { table: TableHandle, key: Value, dest: Register },
    TableSet { table: TableHandle, key: Value, value: Value },
    ...
}

// Process operations in a single dispatch function
fn process_operation(&mut self, op: Operation) -> Result<ExecutionStatus> {
    match op {
        Operation::Call { ... } => { /* handle call */ },
        Operation::Return { ... } => { /* handle return */ },
        ...
    }
}
```

### 4. Proper Handle Validation

All handle usage must include validation:

```rust
impl<T> TypedHandle<T> {
    fn validate(&self, heap: &LuaHeap) -> Result<()> {
        if !heap.is_valid_handle(self) {
            return Err(LuaError::InvalidHandle);
        }
        Ok(())
    }
}

fn get_table_field(&mut self, table: TableHandle, key: Value) -> Result<Value> {
    // Validate handle first
    table.validate(&self.heap)?;
    
    // Then proceed with operation
    ...
}
```

### 5. Two-Phase Borrow Pattern

All operations that require multiple heap accesses must use a two-phase pattern:

```rust
// Phase 1: Gather needed handles
let (table_handle, key_handle) = {
    let frame = self.get_current_frame()?;
    let table = self.get_register(frame.base + a)?;
    let key = self.get_register(frame.base + b)?;
    (table, key)
};

// Phase 2: Use the handles (after previous borrows are dropped)
let value = self.get_table_field(table_handle, key_handle)?;
```

## Reimplementation Strategy

### 1. Start with a Clean Slate

We recommend deleting the existing implementation and starting with a clean implementation of the core components.

### 2. Layer-by-Layer Implementation

Implement the system in carefully tested layers:

1. **Arena and Handles**: Implement memory management first
2. **Values**: Implement Lua values with proper handle references
3. **Heap**: Implement object storage with transaction support
4. **VM Core**: Implement the state machine and operation queue
5. **Opcode Handlers**: Implement instruction handling
6. **Compiler**: Implement bytecode generation
7. **Redis Integration**: Implement Redis API

### 3. Strict Testing at Each Layer

Each layer should be thoroughly tested before moving to the next:

- Unit tests for each component
- Integration tests between components
- Conformance tests against Lua 5.1 spec
- Redis compatibility tests

### 4. Progressive Feature Implementation

After the core architecture is solid, implement features in order of complexity:

1. Basic operations (arithmetic, variables, etc.)
2. Control flow (if, while, for)
3. Functions and closures
4. Tables and metatables
5. Standard library functions
6. Redis-specific features

## Conclusion

By reimplementing the Ferrous Lua VM with a clean, consistent architecture that follows the principles outlined above, we can create a system that:

1. Works harmoniously with Rust's ownership model
2. Avoids stack overflows through proper state machine design
3. Maintains safety and correctness guarantees
4. Achieves performance comparable to Redis Lua
5. Provides full Redis compatibility

The investment in a proper reimplementation will pay dividends in maintainability, performance, and correctness compared to continuing with the current problematic implementation.