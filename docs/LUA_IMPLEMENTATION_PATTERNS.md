# Lua VM Implementation Patterns and Anti-Patterns

This document provides concrete implementation patterns to follow and anti-patterns to avoid when implementing the Ferrous Lua VM. These guidelines will help ensure the implementation properly adheres to the architectural principles outlined in `LUA_ARCHITECTURE.md`.

## Core Implementation Patterns

### 1. Transaction-Based Heap Access

#### ✅ Correct Pattern

```rust
// Create a transaction at the beginning of an operation
fn execute_get_table(&mut self, frame: &CallFrame, a: usize, b: usize, c: usize) -> Result<ExecutionStatus> {
    // Start transaction first
    let mut tx = HeapTransaction::new(&mut self.heap);
    
    // Use transaction for ALL heap accesses
    let table_val = tx.read_register(self.current_thread, frame.base_register as usize + b)?;
    let key_val = tx.read_register(self.current_thread, frame.base_register as usize + c)?;
    
    // Process within transaction scope
    let result = match table_val {
        Value::Table(table) => {
            // Access table through transaction
            match tx.read_table_field(table, &key_val) {
                Ok(value) => {
                    tx.set_register(self.current_thread, frame.base_register as usize + a, value);
                    ExecutionStatus::Continue
                },
                Err(_) => {
                    // Even metamethod handling uses transaction
                    let index_str = tx.create_string("__index")?;
                    // ...etc...
                }
            }
        },
        // ...etc...
    };
    
    // Commit transaction at the end
    tx.commit()?;
    
    Ok(result)
}
```

#### ❌ Anti-Pattern

```rust
// Direct heap access - causes borrow checker conflicts
fn execute_get_table(&mut self, frame: &CallFrame, a: usize, b: usize, c: usize) -> Result<ExecutionStatus> {
    // Direct heap access - WRONG
    let table_val = self.heap.get_thread_register(self.current_thread, frame.base_register as usize + b)?;
    
    // Mix of transaction and direct access - WRONG
    let mut tx = HeapTransaction::new(&mut self.heap);
    
    // Now we have multiple mutable borrows - borrowck conflict
    let index_str = self.heap.create_string("__index")?; // WRONG
    
    // ...etc...
}
```

### 2. Non-Recursive Execution Model

#### ✅ Correct Pattern

```rust
// Queue operations, never call directly
fn execute_call(&mut self, tx: &mut HeapTransaction, frame: &CallFrame, a: usize, b: usize, c: usize) -> Result<ExecutionStatus> {
    // Get function and args
    let func = tx.read_register(self.current_thread, frame.base_register as usize + a)?;
    let args = gather_args(tx, frame, a, b)?;
    
    // Increment PC before proceeding
    tx.increment_pc(self.current_thread)?;
    
    match func {
        Value::Closure(closure) => {
            // Queue call to be processed by main loop - NO RECURSION
            tx.queue_operation(PendingOperation::FunctionCall {
                closure,
                args,
                context: PostCallContext::Normal {
                    return_register: Some((frame.base_register, a)),
                },
            });
            
            Ok(ExecutionStatus::Continue)
        },
        // ...etc...
    }
}
```

#### ❌ Anti-Pattern

```rust
// Recursive function call - causes stack overflow
fn execute_call(&mut self, frame: &CallFrame, a: usize, b: usize, c: usize) -> Result<ExecutionStatus> {
    // Get function and args
    let func = self.get_register(frame.base_register, a)?;
    let args = gather_args(frame, a, b)?;
    
    match func {
        Value::Closure(closure) => {
            // WRONG: Recursive call leading to stack overflow
            let result = self.execute_function(closure, &args)?;
            
            // Store result and continue
            self.set_register(frame.base_register, a, result)?;
            self.increment_pc(frame)?;
            
            Ok(ExecutionStatus::Continue)
        },
        // ...etc...
    }
}
```

### 3. Two-Phase Borrow Pattern

#### ✅ Correct Pattern

```rust
// Two-phase borrow for complex operations
fn apply_metamethod(&mut self, table: TableHandle, key: Value) -> Result<Value> {
    // Phase 1: Extract handles only
    let metatable_handle = {
        let table_obj = self.heap.get_table(table)?;
        table_obj.metatable // Copy the handle, borrow ends here
    };
    
    // Phase 2: Use extracted handles
    if let Some(metatable) = metatable_handle {
        // Safe to borrow heap again
        let metamethod = self.heap.get_metamethod(metatable, "__index")?;
        // ...etc...
    }
}
```

#### ❌ Anti-Pattern

```rust
// Single-phase borrow causing conflicts
fn apply_metamethod(&mut self, table: TableHandle, key: Value) -> Result<Value> {
    // Get table - borrows heap
    let table_obj = self.heap.get_table(table)?;
    
    // Get metatable
    if let Some(metatable) = table_obj.metatable {
        // WRONG: Second borrow while first is still active
        let metamethod = self.heap.get_metamethod(metatable, "__index")?;
        // ...
    }
}
```

### 4. Command Pattern for Operations

#### ✅ Correct Pattern

```rust
// Define all operations as data, not function calls
enum Operation {
    Call { closure: ClosureHandle, args: Vec<Value>, return_to: RegisterTarget },
    TableGet { table: TableHandle, key: Value, dest_register: RegisterTarget },
    TableSet { table: TableHandle, key: Value, value: Value },
    // ...etc...
}

// Process operations with a single dispatch function
fn process_operation(&mut self, op: Operation) -> Result<ExecutionStatus> {
    // Create transaction for ALL operations
    let mut tx = HeapTransaction::new(&mut self.heap);
    
    // Pattern match on operation type
    let result = match op {
        Operation::Call { closure, args, return_to } => {
            self.handle_call(&mut tx, closure, args, return_to)?
        },
        Operation::TableGet { table, key, dest_register } => {
            self.handle_table_get(&mut tx, table, key, dest_register)?
        },
        // ...etc...
    };
    
    // Commit transaction
    tx.commit()?;
    
    Ok(result)
}
```

#### ❌ Anti-Pattern

```rust
// Direct function calls for different operations
fn call_function(&mut self, closure: ClosureHandle, args: Vec<Value>) -> Result<Value> {
    // Direct implementation - WRONG
    // ...
}

fn get_table_field(&mut self, table: TableHandle, key: Value) -> Result<Value> {
    // Direct implementation - WRONG
    // ...
}

// Usage - mixing function calls with no transaction boundary
fn execute(&mut self) {
    // WRONG: No transaction, direct calls
    let field = self.get_table_field(table, key)?;
    let result = self.call_function(closure, args)?;
    // ...
}
```

### 5. Proper C Function Handling

#### ✅ Correct Pattern

```rust
// Handle C functions with careful borrow boundaries
fn execute_c_function(&mut self, func: CFunction, args: Vec<Value>) -> Result<Value> {
    // Store thread handle to avoid borrow issues
    let thread_handle = self.current_thread;
    
    // Set up stack with a clear borrow scope
    let stack_base = {
        let thread = self.heap.get_thread_mut(thread_handle)?;
        let base = thread.stack.len();
        
        // Push arguments
        for arg in &args {
            thread.stack.push(arg.clone());
        }
        
        base // Return base position
    }; // Borrow of thread ends here
    
    // Create execution context
    let mut ctx = ExecutionContext {
        vm: self,
        base: stack_base,
        arg_count: args.len(),
    };
    
    // Call the C function
    let ret_count = func(&mut ctx)?;
    
    // Get return values in a new borrow scope
    let result = if ret_count > 0 {
        let value = {
            let thread = self.heap.get_thread(thread_handle)?;
            if stack_base < thread.stack.len() {
                thread.stack[stack_base].clone()
            } else {
                Value::Nil
            }
        }; // Borrow ends here
        
        value
    } else {
        Value::Nil
    };
    
    // Clean up stack in final borrow scope
    {
        let thread = self.heap.get_thread_mut(thread_handle)?;
        thread.stack.truncate(stack_base);
    }
    
    Ok(result)
}
```

#### ❌ Anti-Pattern

```rust
// C function handling with borrow conflicts
fn execute_c_function(&mut self, func: CFunction, args: Vec<Value>) -> Result<Value> {
    // WRONG: Extend mutable borrow across function call
    let thread = self.heap.get_thread_mut(self.current_thread)?;
    let base = thread.stack.len();
    
    // Push arguments
    for arg in &args {
        thread.stack.push(arg.clone());
    }
    
    // Create context that needs to borrow self
    let mut ctx = ExecutionContext {
        vm: self, // WRONG: Self still has a mutable borrow above
        base,
        arg_count: args.len(),
    };
    
    // Call function - borrowck error here
    let ret_count = func(&mut ctx)?;
    
    // ...etc...
}
```

## Implementation Sequence

Following these steps in order will help ensure a consistent implementation:

1. Implement the arena and handle system first
2. Implement Lua values with proper handle references
3. Implement the heap with transaction support
4. Implement the operation queue and state machine
5. Implement opcode handlers using transaction patterns
6. Implement the compiler with clean interface to VM

## Error Handling

### ✅ Correct Pattern

```rust
// Use Result for all operations that might fail
fn execute_operation(&mut self, op: Operation) -> Result<ExecutionStatus> {
    let mut tx = HeapTransaction::new(&mut self.heap);
    
    // Pattern match and handle errors
    let result = match op {
        Operation::Call { closure, args, return_to } => {
            // Validate handle first
            if !tx.is_valid_closure(closure) {
                return Err(LuaError::InvalidHandle);
            }
            
            // Proceed with operation
            // ...
        },
        // ...etc...
    };
    
    // Only commit if no errors occurred
    tx.commit()?;
    
    Ok(result)
}
```

### ❌ Anti-Pattern

```rust
// Missing error handling
fn execute_operation(&mut self, op: Operation) {
    // WRONG: No validation or error handling
    match op {
        Operation::Call { closure, args, return_to } => {
            // No handle validation
            // ...
        },
        // ...etc...
    }
}
```

## Memory Management

### ✅ Correct Pattern

```rust
// Use drop and clean-up patterns
impl Drop for LuaVM {
    fn drop(&mut self) {
        // Clean up any resources
        self.pending_operations.clear();
        
        // Explicitly reset thread to release references
        if let Ok(thread) = self.heap.get_main_thread() {
            let _ = self.heap.reset_thread(thread);
        }
    }
}
```

### ❌ Anti-Pattern

```rust
// No cleanup of resources
impl Drop for LuaVM {
    fn drop(&mut self) {
        // WRONG: Missing cleanup
    }
}
```

By following these patterns and avoiding the anti-patterns, the next implementation session can create a robust, ownership-friendly Lua VM that works with Rust's type system rather than fighting against it.