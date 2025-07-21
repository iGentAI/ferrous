# Rc<RefCell> Implementation Guide for the Ferrous Lua VM

## Introduction

This guide outlines a detailed implementation strategy for migrating the Ferrous Lua VM from the current global RefCell architecture to a fine-grained Rc<RefCell> architecture. This migration is necessary to resolve fundamental borrow checker issues, particularly with closures and upvalues, while maintaining Lua's semantics for shared mutable state.

## 1. Current Architecture Issues

The current RefCellVM implementation uses a global-level RefCell wrapper around the entire heap:

```rust
pub struct RefCellHeap {
    // Arena for string storage
    strings: RefCell<Arena<LuaString>>,
    
    // Arena for table storage
    tables: RefCell<Arena<Table>>,
    
    // Arena for closure storage
    closures: RefCell<Arena<Closure>>,
    
    // etc...
}
```

This creates several critical issues:

### 1.1 Global-Level Borrowing

Currently, each arena is separately wrapped in a RefCell, but operations often need to access multiple arenas (e.g., looking up function information, then accessing upvalues). This leads to runtime panics when:

1. A function tries to read from one arena while holding a mutable borrow to another
2. Complex operations like function calls need to access various parts of the heap simultaneously
3. Closures attempt to access parent upvalues while creating new ones

### 1.2 Shared Upvalue Problems

Lua closures share upvalues when they capture the same local variable:

```lua
function outer()
    local x = 0
    
    local function inc()
        x = x + 1  -- Both closures share the same 'x'
    end
    
    local function get()
        return x 
    end
    
    return inc, get
end
```

The current implementation cannot properly represent this sharing pattern, as upvalues are stored in an arena and accessed directly, creating borrow conflicts when multiple closures access the same upvalue simultaneously.

### 1.3 Critical Operations with Runtime Panics

These specific VM operations consistently cause runtime panics:

1. **CLOSURE opcode**: Creating a new closure while accessing parent closure upvalues
2. **RETURN opcode**: Closing upvalues while popping stack frames
3. **Metamethod handling**: Accessing a metatable while performing another heap operation
4. **TFORLOOP**: Function calls within generic for loops

## 2. The Rc<RefCell> Solution

The solution is to migrate to fine-grained Rc<RefCell> for individual heap objects:

### 2.1 Core Type Changes

```rust
// Instead of handles storing indices into arenas
use std::rc::Rc;
use std::cell::RefCell;

// New handle types with direct references
pub type StringHandle = Rc<RefCell<LuaString>>;
pub type TableHandle = Rc<RefCell<Table>>;
pub type ClosureHandle = Rc<RefCell<Closure>>;
pub type ThreadHandle = Rc<RefCell<Thread>>;
pub type UpvalueHandle = Rc<RefCell<UpvalueState>>;

// Function prototypes are immutable, so Rc is sufficient
pub type FunctionProtoHandle = Rc<FunctionProto>;
```

### 2.2 Key Benefits

1. **Independent Borrowing**: Each object can be borrowed independently without locking the entire heap
2. **Proper Sharing**: Upvalues can be shared between closures by cloning the Rc
3. **No Global Locks**: Resolves all runtime panics caused by RefCell conflicts
4. **Clean Architecture**: Better matches Lua's semantics of shared mutable state

## 3. Implementation Steps

### 3.1 Update Value Types

Modify the `Value` enum to use the new handle types:

```rust
pub enum Value {
    Nil,
    Boolean(bool),
    Number(f64),
    String(Rc<RefCell<LuaString>>),
    Table(Rc<RefCell<Table>>),
    Closure(Rc<RefCell<Closure>>),
    Thread(Rc<RefCell<Thread>>),
    Upvalue(Rc<RefCell<UpvalueState>>),
    // etc...
}
```

### 3.2 Redesign Upvalue State

```rust
pub enum UpvalueState {
    // Open upvalue points to a stack location
    Open {
        thread: ThreadHandle,
        stack_index: usize,
    },
    // Closed upvalue stores the value directly
    Closed {
        value: Value,
    }
}

impl Closure {
    // Upvalues are now Rcs that can be directly shared
    pub upvalues: Vec<Rc<RefCell<UpvalueState>>>,
    // ...
}
```

### 3.3 Revise Heap Operations

The heap becomes a registry of interned values and objects:

```rust
pub struct RcRefCellHeap {
    // String interning cache 
    string_cache: RefCell<HashMap<Vec<u8>, StringHandle>>,
    
    // Registry for keeping strong references
    registry: RefCell<Vec<Value>>,
    
    // Main thread and globals
    main_thread: ThreadHandle,
    globals: TableHandle,
    
    // Pre-interned strings for metamethods
    metamethod_names: MetamethodNames,
}
```

### 3.4 Update Value Creation Functions

```rust
impl RcRefCellHeap {
    // Create a string with interning
    pub fn create_string(&self, s: &str) -> LuaResult<StringHandle> {
        // Check cache
        {
            let cache = self.string_cache.borrow();
            if let Some(handle) = cache.get(s.as_bytes()) {
                return Ok(Rc::clone(handle));
            }
        }
        
        // Create new string
        let lua_string = LuaString::new(s);
        let handle = Rc::new(RefCell::new(lua_string));
        
        // Add to cache
        self.string_cache.borrow_mut().insert(
            s.as_bytes().to_vec(),
            Rc::clone(&handle)
        );
        
        Ok(handle)
    }
    
    // Create a table
    pub fn create_table(&self) -> LuaResult<TableHandle> {
        let table = Table::new();
        Ok(Rc::new(RefCell::new(table)))
    }
    
    // etc...
}
```

### 3.5 Update Access Patterns

Table operations become:

```rust
// Get table field
pub fn get_table_field(&self, table: &TableHandle, key: &Value) -> LuaResult<Value> {
    let table_ref = table.borrow();
    
    // Direct field access
    if let Some(value) = table_ref.get_field(key) {
        return Ok(value.clone());
    }
    
    // Check metatable if needed
    if let Some(metatable) = &table_ref.metatable {
        let mt_ref = metatable.borrow();
        // Access metatable independently
    }
    
    Ok(Value::Nil)
}

// Set table field 
pub fn set_table_field(&self, table: &TableHandle, key: Value, value: Value) -> LuaResult<()> {
    let mut table_ref = table.borrow_mut();
    table_ref.set_field(key, value);
    Ok(())
}
```

### 3.6 Upvalue Operations

```rust
// Find or create upvalue
pub fn find_or_create_upvalue(&self, thread: &ThreadHandle, index: usize) 
    -> LuaResult<UpvalueHandle> 
{
    let thread_ref = thread.borrow();
    
    // Check for existing upvalue
    for upvalue in &thread_ref.open_upvalues {
        let uv_ref = upvalue.borrow();
        if let UpvalueState::Open { stack_index, .. } = &*uv_ref {
            if *stack_index == index {
                return Ok(Rc::clone(upvalue));
            }
        }
    }
    
    // Create new upvalue
    drop(thread_ref); // Drop borrow before creating
    
    let upvalue = Rc::new(RefCell::new(UpvalueState::Open {
        thread: Rc::clone(thread),
        stack_index: index,
    }));
    
    // Add to thread's open upvalues list
    thread.borrow_mut().open_upvalues.push(Rc::clone(&upvalue));
    
    Ok(upvalue)
} 

// Close upvalue
pub fn close_upvalue(&self, upvalue: &UpvalueHandle) -> LuaResult<()> {
    let value = {
        // Get current value
        let uv_ref = upvalue.borrow();
        match &*uv_ref {
            UpvalueState::Open { thread, stack_index } => {
                let thread_ref = thread.borrow();
                thread_ref.stack[*stack_index].clone()
            }
            UpvalueState::Closed { value } => value.clone(),
        }
    };
    
    // Update upvalue state
    let mut uv_ref = upvalue.borrow_mut();
    *uv_ref = UpvalueState::Closed { value };
    Ok(())
}

// Close upvalues at or above index
pub fn close_upvalues_above(&self, thread: &ThreadHandle, index: usize) -> LuaResult<()> {
    let upvalues_to_close = {
        let thread_ref = thread.borrow();
        let mut to_close = Vec::new();
        
        for upvalue in &thread_ref.open_upvalues {
            let uv_ref = upvalue.borrow();
            if let UpvalueState::Open { stack_index, .. } = &*uv_ref {
                if *stack_index >= index {
                    to_close.push(Rc::clone(upvalue));
                }
            }
        }
        
        to_close
    };
    
    // Close upvalues
    for upvalue in upvalues_to_close {
        self.close_upvalue(&upvalue)?;
    }
    
    // Update thread's open_upvalues list
    let mut thread_ref = thread.borrow_mut();
    thread_ref.open_upvalues.retain(|uv| {
        let uv_ref = uv.borrow();
        !matches!(&*uv_ref, UpvalueState::Closed { .. })
    });
    
    Ok(())
}
```

## 4. Opcode Implementation Examples

### 4.1 CLOSURE Implementation

```rust
fn op_closure(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
    let a = inst.get_a() as usize;
    let bx = inst.get_bx() as usize;
    
    // Get current frame
    let frame = self.current_frame.borrow();
    let closure = frame.closure.borrow();
    
    // Get function prototype
    let proto_handle = match &closure.proto.constants[bx] {
        Value::FunctionProto(h) => Rc::clone(h),
        _ => return Err(LuaError::TypeError { /* ... */ }),
    };
    
    // Build upvalues list
    let mut upvalues = Vec::new();
    drop(closure); // Free the borrow
    
    // Process each upvalue
    for i in 0..proto_handle.upvalues.len() {
        // Read pseudo-instruction
        let frame = self.current_frame.borrow();
        let closure = frame.closure.borrow();
        let pc = frame.pc;
        
        let pseudo_inst = closure.proto.bytecode[pc + i];
        let inst = Instruction(pseudo_inst);
        
        drop(closure);
        drop(frame);
        
        let upvalue = match inst.get_opcode() {
            OpCode::Move => {
                // Local variable
                let local_idx = base + inst.get_b() as usize;
                self.find_or_create_upvalue(&self.current_thread, local_idx)?
            },
            OpCode::GetUpVal => {
                // Parent upvalue
                let frame = self.current_frame.borrow();
                let closure = frame.closure.borrow();
                let parent_upval = Rc::clone(&closure.upvalues[inst.get_b() as usize]);
                drop(closure);
                drop(frame);
                parent_upval
            },
            _ => return Err(LuaError::RuntimeError("Invalid pseudo-instruction")),
        };
        
        upvalues.push(upvalue);
    }
    
    // Create closure
    let new_closure = Closure {
        proto: proto_handle,
        upvalues,
    };
    
    let closure_handle = Rc::new(RefCell::new(new_closure));
    
    // Set register
    self.set_register(base + a, Value::Closure(closure_handle))?;
    
    // Update PC to skip pseudo-instructions
    let frame = self.current_frame.borrow();
    let pc = frame.pc;
    drop(frame);
    
    let new_pc = pc + proto_handle.upvalues.len();
    self.set_pc(new_pc)?;
    
    Ok(())
}
```

### 4.2 RETURN Implementation

```rust
fn op_return(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
    let a = inst.get_a() as usize;
    let b = inst.get_b();
    
    // First, close all upvalues at or above base
    self.close_upvalues_above(&self.current_thread, base)?;
    
    // Collect return values
    let mut values = Vec::new();
    
    if b == 0 {
        // Return all values from R(A) to top
        let thread = self.current_thread.borrow();
        let stack_size = thread.stack.len();
        drop(thread);
        
        for i in 0..(stack_size - base - a) {
            let value = self.get_register(base + a + i)?;
            values.push(value);
        }
    } else {
        // Return specific count
        for i in 0..(b-1) as usize {
            let value = self.get_register(base + a + i)?;
            values.push(value);
        }
    }
    
    // Pop frame
    self.pop_frame()?;
    
    // Queue return operation
    self.operation_queue.push_back(PendingOperation::Return { values });
    
    Ok(())
}
```

### 4.3 TFORLOOP Implementation

```rust
fn op_tforloop(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
    let a = inst.get_a() as usize;
    let c = inst.get_c() as usize;
    
    // Function and arguments
    let iter_func = self.get_register(base + a)?;
    let state = self.get_register(base + a + 1)?;
    let control = self.get_register(base + a + 2)?;
    
    // Queue call with continuation
    self.queue_call(iter_func, vec![state, control], c, Some(TForLoopContext {
        base,
        a,
        c,
        pc_before: self.get_pc()? - 1, // Current PC is already at TFORLOOP + 1
    }))?;
    
    Ok(())
}

// TFORLOOP continuation handling
fn handle_tforloop_continuation(&mut self, results: Vec<Value>, ctx: TForLoopContext) -> LuaResult<()> {
    let base = ctx.base;
    let a = ctx.a;
    
    // Check if iteration should continue
    if results.is_empty() || results[0].is_nil() {
        // End of iteration - skip next instruction (JMP back)
        let pc = self.get_pc()?;
        self.set_pc(pc + 1)?;
        return Ok(());
    }
    
    // Continue iteration
    
    // Update control variable
    self.set_register(base + a + 2, results[0].clone())?;
    
    // Copy results to loop variables
    for i in 0..ctx.c {
        let value = if i < results.len() {
            results[i].clone()
        } else {
            Value::Nil
        };
        
        self.set_register(base + a + 3 + i, value)?;
    }
    
    // Jump back to start of loop
    self.set_pc(ctx.pc_before)?;
    
    Ok(())
}
```

## 5. Register Operations

The register operations are simplified as they don't need to go through the heap:

```rust
// Get register
fn get_register(&self, index: usize) -> LuaResult<Value> {
    let thread = self.current_thread.borrow();
    
    if index >= thread.stack.len() {
        return Err(LuaError::StackIndexOutOfBounds);
    }
    
    Ok(thread.stack[index].clone())
}

// Set register
fn set_register(&self, index: usize, value: Value) -> LuaResult<()> {
    let mut thread = self.current_thread.borrow_mut();
    
    // Grow stack if needed
    if index >= thread.stack.len() {
        thread.stack.resize(index + 1, Value::Nil);
    }
    
    thread.stack[index] = value;
    Ok(())
}
```

## 6. Migration Strategy

### 6.1 Phased Implementation

The migration should be done in phases:

1. **Phase 1**: Create new Rc<RefCell> handle types and update Value enum
2. **Phase 2**: Implement core heap operations with the new types
3. **Phase 3**: Update upvalue handling for proper sharing
4. **Phase 4**: Migrate VM operations to use the new types
5. **Phase 5**: Implement remaining standard library functions

### 6.2 Testing Strategy

Each phase should include:

1. Unit tests for new types and operations
2. Integration tests for object interactions 
3. End-to-end tests running Lua code
4. Specific tests for closures and shared upvalues

### 6.3 Compatibility Layer

To ease migration, implement a compatibility layer:

```rust
// Compatibility layer
impl LegacyHeap {
    pub fn get_table(&self, handle: TableHandle) -> LuaResult<Ref<'_, Table>> {
        // Convert legacy handle to Rc<RefCell> and borrow
        let table_rc = self.get_table_rc(handle)?;
        
        // This is complex and may require additional work
        // to map between lifetime systems
    }
}
```

## 7. Performance Considerations

### 7.1 Advantages

1. **Fewer Conflicts**: No global lock means less contention
2. **Simpler Code**: More intuitive borrowing patterns
3. **More Lua-Like**: Better matches Lua's semantics

### 7.2 Disadvantages

1. **More Allocations**: Each Rc<RefCell> is a separate allocation
2. **Runtime Checks**: RefCell still has runtime borrow checks
3. **Migration Complexity**: Extensive code changes required

### 7.3 Optimizations

1. **Object Pooling**: Reuse common values like small integers
2. **Lazy Cloning**: Only clone Rc handles when necessary
3. **Reference Counting**: Be careful with circular references

## 8. Future Extensions

Once the Rc<RefCell> migration is complete, these additional features become easier:

1. **Garbage Collection**: Implement a cycle detector for garbage collection
2. **Coroutines**: Multiple threads with shared state
3. **Concurrent Access**: Allow multiple VMs to share data safely

## Conclusion

The migration to Rc<RefCell> resolves fundamental issues in the current RefCellVM implementation, particularly around closures and upvalues. While it requires significant code changes, it provides a more robust and Lua-semantics-compatible architecture that will resolve all current runtime panics and borrow checker issues.

The key to success is a methodical, phased approach with comprehensive testing at each stage. Once complete, this architecture will provide a solid foundation for implementing the remaining Lua features and passing all tests.