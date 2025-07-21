# RefCellVM to Rc<RefCell> Migration Strategy

## Overview

The current RefCellVM implementation in Ferrous uses a global RefCell for the entire heap, which causes runtime borrow checker panics when operations require multiple simultaneous borrows. This document outlines a migration strategy to a fine-grained Rc<RefCell> architecture that resolves these issues while maintaining Lua's semantics.

## Current Architecture Issues

### 1. Global RefCell Lock Problem

The current implementation pattern:

```rust
pub struct RefCellHeap {
    inner: RefCell<RefCellHeapInner>,
}

struct RefCellHeapInner {
    strings: Arena<LuaString>,
    tables: Arena<Table>,
    closures: Arena<Closure>,
    threads: Arena<LuaThread>,
    // ... etc
}
```

This causes runtime panics in scenarios like:

```rust
// Attempting to read a table while holding a mutable borrow
let table = self.heap.get_table(handle)?;  // borrows heap
let key = self.heap.create_string("key")?; // PANIC! Already borrowed
```

### 2. Shared Upvalue Problem

Lua closures share upvalues when they capture the same local variable:

```lua
function outer()
    local shared = 0
    
    local function inc()
        shared = shared + 1  -- Modifies shared upvalue
    end
    
    local function get()
        return shared        -- Reads shared upvalue
    end
    
    return inc, get
end
```

The current architecture cannot safely represent this sharing pattern without Rc.

### 3. Complex Operation Conflicts

Operations that require multiple heap accesses fail:

- **CLOSURE**: Creating upvalues while borrowing parent closure
- **RETURN**: Closing upvalues while popping stack frame
- **Metamethod calls**: Accessing metatable while modifying table

## Proposed Architecture: Fine-Grained Rc<RefCell>

### 1. Core Type Definitions

```rust
use std::rc::Rc;
use std::cell::RefCell;

// Each heap object type gets its own Rc<RefCell> wrapper
pub type StringHandle = Rc<RefCell<LuaString>>;
pub type TableHandle = Rc<RefCell<Table>>;
pub type ClosureHandle = Rc<RefCell<Closure>>;
pub type ThreadHandle = Rc<RefCell<LuaThread>>;
pub type UpvalueHandle = Rc<RefCell<UpvalueState>>;
pub type FunctionProtoHandle = Rc<FunctionProto>; // Immutable, no RefCell needed

// Upvalue state can be open (on stack) or closed (independent)
pub enum UpvalueState {
    Open {
        thread: ThreadHandle,
        stack_index: usize,
    },
    Closed {
        value: Value,
    },
}

// Values now contain Rc handles
pub enum Value {
    Nil,
    Boolean(bool),
    Number(f64),
    String(StringHandle),
    Table(TableHandle),
    Function(FunctionHandle),
    // ... etc
}

pub enum FunctionHandle {
    Closure(ClosureHandle),
    CFunction(CFunction),
}
```

### 2. Heap Structure Changes

```rust
pub struct RcRefCellHeap {
    // String interning cache
    string_cache: RefCell<HashMap<Vec<u8>, StringHandle>>,
    
    // Registry for strong references (prevents GC)
    registry: RefCell<Vec<Value>>,
    
    // Main thread
    main_thread: ThreadHandle,
    
    // Pre-interned strings
    metamethod_names: MetamethodNames,
}

// Pre-interned metamethod names for fast access
pub struct MetamethodNames {
    pub index: StringHandle,
    pub newindex: StringHandle,
    pub add: StringHandle,
    pub sub: StringHandle,
    // ... etc
}

impl RcRefCellHeap {
    pub fn new() -> LuaResult<Self> {
        let mut heap = RcRefCellHeap {
            string_cache: RefCell::new(HashMap::new()),
            registry: RefCell::new(Vec::new()),
            main_thread: Rc::new(RefCell::new(LuaThread::new())),
            metamethod_names: MetamethodNames::default(),
        };
        
        // Pre-intern common strings
        heap.pre_intern_strings()?;
        
        Ok(heap)
    }
}
```

### 3. Key Implementation Patterns

#### String Creation with Deduplication

```rust
impl RcRefCellHeap {
    pub fn create_string(&self, s: &str) -> LuaResult<StringHandle> {
        let bytes = s.as_bytes();
        
        // Check cache first
        {
            let cache = self.string_cache.borrow();
            if let Some(handle) = cache.get(bytes) {
                return Ok(Rc::clone(handle));
            }
        }
        
        // Create new string
        let lua_string = LuaString::from_bytes(bytes);
        let handle = Rc::new(RefCell::new(lua_string));
        
        // Add to cache
        {
            let mut cache = self.string_cache.borrow_mut();
            cache.insert(bytes.to_vec(), Rc::clone(&handle));
        }
        
        Ok(handle)
    }
}
```

#### Table Operations

```rust
impl RcRefCellHeap {
    pub fn create_table(&self) -> TableHandle {
        Rc::new(RefCell::new(Table::new()))
    }
    
    pub fn table_get(&self, table: &TableHandle, key: &Value) -> LuaResult<Value> {
        let table_ref = table.borrow();
        
        // Direct field access
        if let Some(value) = table_ref.get_field(key) {
            return Ok(value.clone());
        }
        
        // Check metatable
        if let Some(metatable) = &table_ref.metatable {
            // Can safely borrow metatable independently
            let mt_ref = metatable.borrow();
            if let Some(index_mm) = mt_ref.get_field(&Value::String(self.metamethod_names.index.clone())) {
                // Queue metamethod call - no recursion
                return Ok(Value::PendingMetamethod(index_mm.clone()));
            }
        }
        
        Ok(Value::Nil)
    }
}
```

#### Closure and Upvalue Handling

```rust
impl RcRefCellHeap {
    pub fn create_closure(&self, proto: FunctionProtoHandle, parent_upvalues: &[UpvalueHandle]) 
        -> LuaResult<ClosureHandle> {
        let mut upvalues = Vec::new();
        
        // Process each upvalue descriptor
        for desc in &proto.upvalues {
            let upvalue = match desc.kind {
                UpvalueKind::InStack => {
                    // Create new open upvalue
                    self.find_or_create_upvalue(
                        self.main_thread.clone(),
                        desc.stack_index
                    )?
                },
                UpvalueKind::InUpvalue => {
                    // Share parent's upvalue
                    Rc::clone(&parent_upvalues[desc.upvalue_index])
                },
            };
            upvalues.push(upvalue);
        }
        
        let closure = Closure {
            proto: Rc::clone(&proto),
            upvalues,
        };
        
        Ok(Rc::new(RefCell::new(closure)))
    }
    
    fn find_or_create_upvalue(&self, thread: ThreadHandle, index: usize) 
        -> LuaResult<UpvalueHandle> {
        let mut thread_ref = thread.borrow_mut();
        
        // Check if upvalue already exists
        for upvalue in &thread_ref.open_upvalues {
            let uv_ref = upvalue.borrow();
            if let UpvalueState::Open { stack_index, .. } = &*uv_ref {
                if *stack_index == index {
                    return Ok(Rc::clone(upvalue));
                }
            }
        }
        
        // Create new upvalue
        let upvalue = Rc::new(RefCell::new(UpvalueState::Open {
            thread: Rc::clone(&thread),
            stack_index: index,
        }));
        
        thread_ref.open_upvalues.push(Rc::clone(&upvalue));
        
        Ok(upvalue)
    }
}
```

### 4. VM Integration

The VM needs updates to work with Rc<RefCell> handles:

```rust
impl RefCellVM {
    pub fn read_upvalue(&self, upvalue: &UpvalueHandle) -> LuaResult<Value> {
        let uv_ref = upvalue.borrow();
        
        match &*uv_ref {
            UpvalueState::Open { thread, stack_index } => {
                let thread_ref = thread.borrow();
                Ok(thread_ref.stack[*stack_index].clone())
            },
            UpvalueState::Closed { value } => {
                Ok(value.clone())
            },
        }
    }
    
    pub fn write_upvalue(&self, upvalue: &UpvalueHandle, value: Value) -> LuaResult<()> {
        let mut uv_ref = upvalue.borrow_mut();
        
        match &mut *uv_ref {
            UpvalueState::Open { thread, stack_index } => {
                let mut thread_ref = thread.borrow_mut();
                thread_ref.stack[*stack_index] = value;
            },
            UpvalueState::Closed { value: closed_value } => {
                *closed_value = value;
            },
        }
        
        Ok(())
    }
    
    pub fn close_upvalues(&self, thread: &ThreadHandle, level: usize) -> LuaResult<()> {
        let mut thread_ref = thread.borrow_mut();
        
        // Close all upvalues >= level
        let open_upvalues = std::mem::take(&mut thread_ref.open_upvalues);
        drop(thread_ref); // Release borrow before processing
        
        let mut remaining = Vec::new();
        
        for upvalue in open_upvalues {
            let should_close = {
                let uv_ref = upvalue.borrow();
                match &*uv_ref {
                    UpvalueState::Open { stack_index, .. } => *stack_index >= level,
                    UpvalueState::Closed { .. } => false,
                }
            };
            
            if should_close {
                // Close the upvalue
                let mut uv_ref = upvalue.borrow_mut();
                if let UpvalueState::Open { thread, stack_index } = &*uv_ref {
                    let thread_ref = thread.borrow();
                    let value = thread_ref.stack[*stack_index].clone();
                    *uv_ref = UpvalueState::Closed { value };
                }
            } else {
                remaining.push(upvalue);
            }
        }
        
        // Restore remaining open upvalues
        let mut thread_ref = self.main_thread.borrow_mut();
        thread_ref.open_upvalues = remaining;
        
        Ok(())
    }
}
```

## Migration Steps

### Phase 1: Type System Migration

1. Define new Rc<RefCell> handle types
2. Update Value enum to use new handles
3. Create type aliases for easier refactoring

### Phase 2: Heap Structure Migration

1. Replace global RefCell with per-type storage
2. Implement string interning with Rc<RefCell>
3. Update heap creation and initialization

### Phase 3: Core Operations Migration

1. **String Operations**: Update to use StringHandle
2. **Table Operations**: Independent table borrows
3. **Upvalue Operations**: Implement shared upvalue state
4. **Thread Operations**: Per-thread RefCell

### Phase 4: VM Integration

1. Update opcode handlers to use new handles
2. Modify register access patterns
3. Update C function interface

### Phase 5: Testing and Validation

1. Ensure no runtime borrow panics
2. Verify shared upvalue semantics
3. Performance benchmarking

## Benefits of Migration

### 1. Eliminates Runtime Panics
- No global lock means no overlapping borrow conflicts
- Each object can be borrowed independently

### 2. Proper Upvalue Sharing
- Multiple closures can share upvalues via Rc
- Mutations are safe through RefCell

### 3. Cleaner Code Structure
- Explicit ownership through Rc
- Clear borrowing patterns
- Easier to reason about

### 4. Performance Improvements
- Fine-grained locking reduces contention
- Rc clone is cheap (just refcount increment)
- No need for complex phasing to avoid borrows

## Potential Challenges

### 1. Cyclic References
- Tables can reference themselves
- Closures can capture themselves via upvalues
- Need weak references or cycle detection for GC

### 2. Migration Complexity
- Large codebase changes required
- Need to maintain compatibility during migration

### 3. Performance Considerations
- More allocations (each Rc<RefCell> is heap allocated)
- Additional indirection through Rc
- RefCell runtime checks

## Conclusion

Migrating to Rc<RefCell> solves the fundamental architectural issues in the current RefCellVM implementation. While it requires significant changes, the benefits of eliminating runtime panics and properly supporting Lua's shared mutable semantics make it essential for a production-quality implementation.

The migration can be done incrementally, starting with the most problematic areas (upvalues and closures) and expanding to the full system. With careful planning and testing, this architecture will provide a solid foundation for the complete Lua 5.1 implementation.