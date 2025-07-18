# RefCellVM Architecture - Current Implementation

## Overview

This document describes the RefCellVM architecture, the current implementation approach used in Ferrous for executing Lua code. The RefCellVM replaces the previous transaction-based VM approach, providing improved memory safety, execution reliability, and simplified code organization while maintaining compatibility with Lua 5.1.

## Core Design Principles

The RefCellVM architecture is built around several key design principles:

1. **Interior Mutability**: Uses Rust's `RefCell` pattern for safe mutable access to shared values
2. **Direct Memory Access**: Provides immediate heap access without transaction boundaries
3. **Non-Recursive Execution**: Uses a queue-based approach for function calls to prevent stack overflow
4. **Type-Safe Handles**: Leverages Rust's type system to ensure memory safety
5. **Handle Validation**: Ensures handles are valid before use to prevent memory corruption
6. **Lua 5.1 Compatibility**: Maintains compatibility with Redis Lua scripting semantics

## Memory Architecture

### RefCellHeap

The `RefCellHeap` is the central component that manages all Lua objects:

```rust
pub struct RefCellHeap {
    /// Arena for string storage
    strings: RefCell<Arena<LuaString>>,
    
    /// String interning cache for deduplication
    string_cache: RefCell<HashMap<Vec<u8>, StringHandle>>,
    
    /// Arena for table storage
    tables: RefCell<Arena<Table>>,
    
    /// Arena for closure storage
    closures: RefCell<Arena<Closure>>,
    
    /// Arena for thread storage
    threads: RefCell<Arena<Thread>>,
    
    /// Arena for upvalue storage
    upvalues: RefCell<Arena<Upvalue>>,
    
    /// Arena for userdata storage
    userdata: RefCell<Arena<UserData>>,
    
    /// Arena for function prototype storage
    function_protos: RefCell<Arena<FunctionProto>>,
    
    /// Global table handle
    globals: Option<TableHandle>,
    
    /// Registry table handle
    registry: Option<TableHandle>,
    
    /// Main thread handle
    main_thread: Option<ThreadHandle>,
    
    /// Resource limits for this VM instance 
    pub resource_limits: ResourceLimits,
}
```

Each resource type is stored in a separate generational arena wrapped in a `RefCell` to allow safe mutable access. The heap maintains global state like the globals table, registry, and main thread.

### Type-Safe Handles

The RefCellVM uses type-safe handles to reference objects in the heap:

```rust
pub struct StringHandle(pub(crate) Handle<LuaString>);
pub struct TableHandle(pub(crate) Handle<Table>);
pub struct ClosureHandle(pub(crate) Handle<Closure>);
// etc.
```

These typed handles provide compile-time type safety while allowing memory-safe access to heap objects.

## Execution Model

### RefCellVM

The `RefCellVM` is the execution engine that interprets Lua bytecode:

```rust
pub struct RefCellVM {
    /// The heap with RefCell interior mutability
    heap: RefCellHeap,
    
    /// Operation queue for handling calls/returns
    operation_queue: VecDeque<PendingOperation>,
    
    /// Main thread handle
    main_thread: ThreadHandle,
    
    /// Currently executing thread
    current_thread: ThreadHandle,
    
    /// VM configuration
    config: VMConfig,
}
```

### Non-Recursive Call Model

The execution model uses a queue-based approach for function calls to prevent stack overflow in deeply nested calls. Operations are queued as `PendingOperation` values:

```rust
pub enum PendingOperation {
    /// Function call operation
    FunctionCall {
        func_index: usize,
        nargs: usize,
        expected_results: i32,
    },
    
    /// C function call operation
    CFunctionCall {
        function: CFunction,
        base: u16,
        nargs: usize,
        expected_results: i32,
    },
    
    /// Return from function
    Return {
        values: Vec<Value>,
    },
    
    /// TFORLOOP continuation after iterator function returns
    TForLoopContinuation {
        base: usize,
        a: usize,
        c: usize,
        pc_before_call: usize,
    },
}
```

### Direct Register Access

The RefCellVM accesses registers directly through the heap, without transaction boundaries:

```rust
// Access registers directly
self.heap.set_thread_register(self.current_thread, base as usize + a, &value)?;
let value = self.heap.get_thread_register(self.current_thread, base as usize + b)?;
```

This direct access pattern is especially important for FOR loops, where the step value must persist between FORPREP and FORLOOP operations.

## Standard Library Integration

The standard library functions now interact with the VM through a trait-based abstraction:

```rust
pub trait ExecutionContext {
    /// Get the number of arguments passed to this C function
    fn arg_count(&self) -> usize;
    
    /// Get an argument value by index (0-based)
    fn get_arg(&self, index: usize) -> LuaResult<Value>;
    
    /// Push a return value
    fn push_result(&mut self, value: Value) -> LuaResult<()>;
    
    // Additional methods...
}
```

This trait-based approach allows for different VM implementations to work with the same standard library code.

## Critical Improvements

### FOR Loop Register Persistence

The previous transaction-based VM had issues with register corruption in FOR loops due to transaction boundaries. The RefCellVM fixes this by providing direct heap access:

```rust
// FORPREP - prepare loop variables
fn op_forprep(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
    // Read loop variables
    let loop_base = base as usize + inst.get_a() as usize;
    let initial = self.heap.get_thread_register(self.current_thread, loop_base)?;
    let limit = self.heap.get_thread_register(self.current_thread, loop_base + 1)?;
    let step = self.heap.get_thread_register(self.current_thread, loop_base + 2)?;
    
    // Handle nil step with default 1.0
    let step_num = match step {
        Value::Number(n) => n,
        Value::Nil => {
            // CRITICAL: Write the default step value immediately
            self.heap.set_thread_register(self.current_thread, loop_base + 2, &Value::Number(1.0))?;
            1.0
        },
        _ => /* error handling */
    };
    
    // Prepare initial value
    let prepared_initial = initial_num - step_num;
    self.heap.set_thread_register(self.current_thread, loop_base, &Value::Number(prepared_initial))?;
}
```

### Two-Phase Borrow Pattern

The RefCellVM uses a two-phase borrow pattern to avoid borrow checker conflicts:

```rust
// Phase 1: Extract needed data with immutable borrows
let upvalue_handle = {
    let frame = self.heap.get_current_frame(self.current_thread)?;
    let closure = self.heap.get_closure(frame.closure)?;
    closure.upvalues[a]
};

// Phase 2: Perform mutable operations without active borrows
self.set_upvalue_value(upvalue_handle, &value)?;
```

## Status and Limitations

The RefCellVM architecture is functional for basic Lua operations but has some limitations:

1. **Completed Features**:
   - Basic language features (assignment, arithmetic, strings)
   - Numeric FOR loops
   - Simple table operations
   - Basic standard library functions (print, type, tostring)

2. **Partial Implementation**:
   - Complex table operations
   - Metamethods (__index, __newindex)
   - Error handling

3. **Not Implemented Yet**:
   - Function closures
   - Upvalue handling
   - Generic FOR loops with iterators
   - Tail call optimization
   - Coroutines
   - Garbage collection

## Future Improvements

Future development should focus on:

1. **Function Completion**: Implement function closures and upvalues
2. **Generic Iteration**: Complete the TFORLOOP implementation and iterator functions
3. **Standard Library**: Expand standard library coverage
4. **Metamethods**: Enhance metamethod support
5. **Garbage Collection**: Add memory management

## Comparison to Previous Approaches

The RefCellVM architecture addresses several issues with the previous approaches:

1. **Register Windows**: The initial approach tried to use separate register windows for each function call. This broke the continuity assumption in Lua where all registers are part of a single stack.

2. **Transaction-Based VM**: The transaction approach wrapped all heap operations in atomic transactions to ensure memory safety but created artificial boundaries between operations that logically needed to be continuous.

3. **RefCellVM Solution**: The current approach uses Rust's `RefCell` for interior mutability, providing the same safety guarantees but without problematic transaction boundaries.

## Conclusion

The RefCellVM architecture provides a solid foundation for the Ferrous Lua implementation, properly balancing Rust's safety requirements with Lua's semantics. While there is still significant work to complete the implementation, the core architecture is sound and numerous problematic edge cases from the previous approaches have been resolved.