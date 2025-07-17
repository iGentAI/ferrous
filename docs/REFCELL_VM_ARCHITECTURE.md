# RefCellVM Architecture

## Overview

The RefCellVM is a Lua virtual machine implementation for Ferrous that uses Rust's interior mutability pattern (`RefCell`) to safely manage state without the complexity of a transaction-based system. This document outlines the core architecture, components, and design principles behind the RefCellVM.

## Core Principles

1. **Interior Mutability**: Use `RefCell` to safely share mutable state across components
2. **Direct Access**: Provide direct access to heap objects through safe borrowing
3. **Single Source of Truth**: Each value exists in exactly one place, with no duplicated state
4. **Immediate Effect**: All operations take effect immediately, with no delayed application
5. **Explicit Failure**: Operations fail explicitly rather than through transaction rollback
6. **Memory Safety**: Maintain Rust's memory safety guarantees without transactions

## Architectural Components

### RefCellHeap

The `RefCellHeap` is the central component of the system, responsible for managing all Lua objects:

```rust
pub struct RefCellHeap {
    strings: RefCell<Arena<LuaString>>,
    string_cache: RefCell<HashMap<Vec<u8>, StringHandle>>,
    tables: RefCell<Arena<Table>>,
    closures: RefCell<Arena<Closure>>,
    threads: RefCell<Arena<Thread>>,
    upvalues: RefCell<Arena<Upvalue>>,
    userdata: RefCell<Arena<UserData>>,
    function_protos: RefCell<Arena<FunctionProto>>,
    
    globals: Option<TableHandle>,
    registry: Option<TableHandle>,
    main_thread: Option<ThreadHandle>,
    
    resource_limits: ResourceLimits,
}
```

The `RefCell` wrapper around each arena allows for controlled mutation while maintaining Rust's safety guarantees. Each type of Lua object lives in its own arena, with handles used for cross-references.

Key operations:
- **String Management**: Creation, interning, and access
- **Table Operations**: Creation, field access, and manipulation
- **Thread Control**: Stack management, register access, call frames
- **Handle Validation**: Type-safe validation of all object handles

### RefCellVM

The `RefCellVM` is the execution engine that interprets Lua bytecode:

```rust
pub struct RefCellVM {
    heap: RefCellHeap,
    operation_queue: VecDeque<PendingOperation>,
    main_thread: ThreadHandle,
    current_thread: ThreadHandle,
    config: VMConfig,
}
```

Key operations:
- **Instruction Execution**: Interpret bytecode instructions
- **Function Calls**: Execute Lua functions and C functions
- **State Management**: Track execution state across calls

The operation queue allows for non-recursive execution of complex operations, preventing stack overflow in deeply nested scenarios.

### RefCellExecutionContext

The `RefCellExecutionContext` provides a safe interface for C functions to interact with the VM:

```rust
pub struct RefCellExecutionContext<'a> {
    heap: &'a RefCellHeap,
    thread: ThreadHandle,
    base: u16,
    nargs: usize,
    results_pushed: usize,
}
```

Key operations:
- **Argument Access**: Safely retrieve function arguments
- **Return Value Management**: Push function results
- **Heap Interaction**: Access global objects, create new values

## Execution Flow

The execution flow in RefCellVM is straightforward:

1. **Initialization**: Create VM with initial heap state
2. **Module Loading**: Compile Lua source to bytecode
3. **Execution**: Process bytecode instructions one by one
4. **Function Calls**: Non-recursive processing via operation queue
5. **Value Return**: Results propagated back up the call chain

Unlike in the transaction-based VM, there are no transaction boundaries between instructions. Each operation takes immediate effect on the heap, ensuring consistent state throughout execution.

## Key Design Patterns

### Borrowing Pattern

```rust
// Immutable borrowing
pub fn get_table(&self, handle: TableHandle) -> LuaResult<Ref<'_, Table>> {
    let tables = self.tables.borrow()?;
    if !tables.contains(handle.0) {
        return Err(LuaError::InvalidHandle);
    }
    Ok(Ref::map(tables, |t| t.get(handle.0).unwrap()))
}

// Mutable borrowing
pub fn get_table_mut(&self, handle: TableHandle) -> LuaResult<RefMut<'_, Table>> {
    let tables = self.tables.borrow_mut()?;
    if !tables.contains(handle.0) {
        return Err(LuaError::InvalidHandle);
    }
    Ok(RefMut::map(tables, |t| t.get_mut(handle.0).unwrap()))
}
```

### Two-Phase Borrowing

For operations needing multiple borrows:

```rust
// Step 1: Extract data with short-lived borrow
let (handle, value) = {
    let tables = self.tables.borrow();
    let table = tables.get(handle.0).unwrap();
    (table.metatable, table.some_field.clone())
};

// Step 2: Use extracted data with separate borrow
if let Some(mt) = handle {
    let mt_tables = self.tables.borrow();
    // Work with metatable...
}
```

### Register Access Pattern

```rust
pub fn get_thread_register(&self, thread: ThreadHandle, index: usize) -> LuaResult<Value> {
    let threads = self.threads.borrow()?;
    let thread_obj = threads.get(thread.0).ok_or(LuaError::InvalidHandle)?;
    
    thread_obj.stack.get(index)
        .cloned()
        .ok_or_else(|| LuaError::RuntimeError(format!(
            "Register {} out of bounds (stack size: {})",
            index,
            thread_obj.stack.len()
        )))
}

pub fn set_thread_register(&self, thread: ThreadHandle, index: usize, value: &Value) -> LuaResult<()> {
    let mut threads = self.threads.borrow_mut()?;
    let thread_obj = threads.get_mut(thread.0).ok_or(LuaError::InvalidHandle)?;
    
    // Grow stack if needed
    if index >= thread_obj.stack.len() {
        thread_obj.stack.resize(index + 1, Value::Nil);
    }
    
    thread_obj.stack[index] = value.clone();
    Ok(())
}
```

## For Loop Implementation

The key fix for the for loop register corruption issue is in the `op_forprep` and `op_forloop` implementations:

### FORPREP Implementation

```rust
fn op_forprep(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
    let a = inst.get_a() as usize;
    let sbx = inst.get_sbx();
    
    let loop_base = base as usize + a;
    
    // Ensure stack space for all loop registers
    self.heap.grow_stack(self.current_thread, loop_base + 4)?;
    
    // Direct access to all loop registers
    let initial = self.heap.get_thread_register(self.current_thread, loop_base)?;
    let limit = self.heap.get_thread_register(self.current_thread, loop_base + 1)?;
    let step = self.heap.get_thread_register(self.current_thread, loop_base + 2)?;
    
    // Handle nil step - critical fix!
    let step_num = match step {
        Value::Number(n) => n,
        Value::Nil => {
            // Default step is 1.0
            let default_step = Value::Number(1.0);
            self.heap.set_thread_register(self.current_thread, loop_base + 2, &default_step)?;
            1.0
        },
        _ => return Err(LuaError::TypeError {
            expected: "number".to_string(),
            got: step.type_name().to_string(),
        }),
    };
    
    // Continue with loop initialization...
}
```

### FORLOOP Implementation

```rust
fn op_forloop(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
    // Reading register values happens in one operation
    // No transaction boundary to lose values
    
    // Direct access ensures consistent state
    let step = self.heap.get_thread_register(self.current_thread, loop_base + 2)?;
    
    // Step is guaranteed to be a number here because FORPREP either:
    // 1. Verified it was a number, or
    // 2. Set a default value of 1.0
    
    // Continue with loop iteration...
}
```

## Interacting with Existing Code

### Module Interface

The RefCellVM provides the same public interface as the original VM:

- `new()`: Create a new VM
- `execute(closure)`: Execute a Lua closure
- `execute_module(module)`: Execute a Lua module
- `init_stdlib()`: Initialize the standard library

This ensures compatibility with existing code that uses the VM.

## Performance Considerations

The RefCellVM is expected to be more performant than the transaction-based VM for several reasons:

1. **No Pending Operations**: Changes apply immediately, with no overhead for tracking
2. **Fewer Allocations**: No need to clone and track pending register writes
3. **Simpler Execution Path**: Less complexity in the VM core loop
4. **Immediate Effect**: No delayed committing of changes

The interior mutability approach should provide all the safety benefits of the transaction system with less overhead and complexity.

## Error Handling Strategy

The RefCellVM uses the same error types as the transaction-based VM but with more straightforward propagation:

```rust
match heap.borrow() {
    Ok(borrowed) => {
        // Use borrowed reference
        Ok(result)
    },
    Err(e) => Err(LuaError::BorrowError(e.to_string())),
}
```

This approach maintains precise error reporting while simplifying the error path.

## Conclusion

The RefCellVM architecture provides a simpler, more direct way to implement Lua's semantics while maintaining Rust's safety guarantees. By removing the transaction layer and using interior mutability directly, we solve critical issues like the for loop register corruption bug while reducing overall system complexity.

This design aligns better with both Lua's execution model and the needs of Redis integration, where scripts execute in isolation and atomically.