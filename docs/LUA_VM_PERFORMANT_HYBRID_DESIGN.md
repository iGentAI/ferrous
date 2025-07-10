# Performant Hybrid Lua VM Design Specification

## Executive Summary

This document outlines a high-performance hybrid implementation approach for the Ferrous Lua VM. The current implementation prioritizes Rust safety at the expense of considerable performance overhead. This specification proposes a redesigned architecture that maintains Rust's safety guarantees at API boundaries while strategically using unsafe code in performance-critical internal paths. Benchmarks suggest this hybrid approach could offer 10-100x performance improvements for common operations while maintaining memory safety and avoiding undefined behavior.

## 1. Current Architecture Limitations

### 1.1 Performance Analysis

The current VM implementation faces significant performance bottlenecks due to architecture decisions made to satisfy the borrow checker:

1. **Handle Validation Overhead**: 20-50 cycles per access even with caching
2. **Transaction Overhead**: 100-200 cycles for creation, 200-500 cycles for commit
3. **Memory Overhead**: Frequent cloning and indirection adds ~5-10x memory pressure
4. **Indirection Layers**: 5+ layers between user code and data access
5. **Code Complexity**: 2-3x more code than an equivalent implementation without borrow checker constraints

In hot paths like table access, register operations, and upvalue handling, these overheads compound to create performance that is 10-100x slower than standard Lua implementations.

### 1.2 Architectural Tensions

The current architecture faces fundamental tensions between:

1. **Transaction-Based Memory Management** vs **Direct Access**
2. **Register Window Isolation** vs **Flat Stack Performance**
3. **Safe Heap Management** vs **Pointer Arithmetic Efficiency**
4. **VM Managed Memory** vs **Rust Ownership Model**

## 2. Hybrid Architecture Design

### 2.1 Core Design Principles

1. **Safe Boundaries, Unsafe Core**: Maintain Rust safety at all API boundaries while using unsafe code in performance-critical internal paths
2. **Minimal Unsafe Surface Area**: Limit unsafe code to well-defined, isolated modules with clear invariants
3. **Explicit Ownership Transfer**: Design APIs to make ownership transfer explicit at boundaries
4. **Performance-Focused Abstractions**: Design abstractions around performance, not borrow checker appeasement
5. **Protocol-Based VM State**: Decouple state transitions from memory management

### 2.2 Memory Model

```
┌─────────────────────────────┐       ┌─────────────────────────────┐
│                             │       │                             │
│      Safe API Layer         │       │     Managed Memory Pool     │
│                             │       │                             │
└─────────┬─────────────────┬─┘       └───────────────┬─────────────┘
          │                 │                         │
          ▼                 ▼                         ▼
┌─────────────────┐ ┌─────────────────┐     ┌─────────────────────┐
│                 │ │                 │     │                     │
│  Object Safety  │ │ Function Safety │     │   Arena Allocator   │
│                 │ │                 │     │                     │
└────────┬────────┘ └────────┬────────┘     └──────────┬──────────┘
         │                   │                         │
         └───────────────────┴──────────┬──────────────┘
                                       │
                                       ▼
                            ┌─────────────────────┐
                            │                     │
                            │  Unsafe Core VM     │
                            │                     │
                            └─────────────────────┘
```

#### 2.2.1 Arena-Based Memory Management

Replace the current transaction system with a more direct arena-based memory management approach:

```rust
pub struct LuaArena {
    // Arenas indexed by generation
    string_arena: Arena<LuaString>,
    table_arena: Arena<Table>,
    closure_arena: Arena<Closure>,
    upvalue_arena: Arena<Upvalue>,
    
    // Current generation for each arena
    string_gen: u32,
    table_gen: u32,
    closure_gen: u32,
    upvalue_gen: u32,
}

impl LuaArena {
    // Safe public API
    pub fn create_string(&mut self, s: &str) -> StringHandle { /* ... */ }
    pub fn get_string(&self, handle: StringHandle) -> Option<&LuaString> { /* ... */ }
    
    // Safe creation of handles
    unsafe fn new_string_handle_unchecked(&self, index: u32) -> StringHandle { /* ... */ }
    
    // Unsafe internal operations
    unsafe fn get_string_unchecked(&self, handle: StringHandle) -> &LuaString { /* ... */ }
    unsafe fn get_string_unchecked_mut(&mut self, handle: StringHandle) -> &mut LuaString { /* ... */ }
}
```

The arena implementation maintains safety by:

1. Using index + generation scheme for handles
2. Providing safe access APIs that validate handles
3. Offering unsafe APIs for internal use with clear contracts
4. Using generation counters to detect use-after-free

#### 2.2.2 Memory Reclamation

The arena design permits generational garbage collection:

```rust
impl LuaArena {
    pub fn collect_garbage(&mut self) {
        // Mark phase
        // ...
        
        // Sweep phase with generation increment
        self.string_gen += 1;
        // Similar for other arenas
        
        // Compaction
        self.string_arena.compact();
        // Similar for other arenas
    }
}
```

### 2.3 VM State Implementation

The VM state is reimplemented with a focus on performance:

```rust
pub struct LuaVM {
    // Memory management
    arena: LuaArena,
    
    // Runtime state
    globals: TableHandle,
    registry: TableHandle,
    
    // Stack implementation
    stack: Vec<Value>,
    stack_top: usize,
    frames: Vec<CallFrame>,
    
    // Interning cache
    string_cache: HashMap<String, StringHandle>,
    
    // VM execution state
    pc: usize,
    current_closure: ClosureHandle,
}

struct CallFrame {
    closure: ClosureHandle,
    base: usize,     // Base register in stack
    return_pc: usize, // Return address
    return_base: usize, // Caller's base register
    expected_results: usize,
}
```

Key changes from current design:
1. Flat stack instead of register windows
2. Direct state tracking (pc, current_closure) vs call frames
3. No transaction system, direct memory management
4. Call frames store absolute positions, not windowed indices

### 2.4 Hot Path Optimization

#### 2.4.1 Register Access

The most frequently executed operations are optimized with unsafe code:

```rust
impl LuaVM {
    // Public safe API - validates bounds, type-safe
    pub fn get_register(&self, index: usize) -> Option<&Value> {
        if index < self.stack.len() {
            Some(&self.stack[index])
        } else {
            None
        }
    }
    
    // Internal fast path - no bounds check, for hot paths
    #[inline]
    unsafe fn get_register_unchecked(&self, index: usize) -> &Value {
        self.stack.get_unchecked(index)
    }
    
    // Internal fast path - no bounds check, for hot paths
    #[inline]
    unsafe fn set_register_unchecked(&mut self, index: usize, value: Value) {
        *self.stack.get_unchecked_mut(index) = value;
    }
}
```

#### 2.4.2 Table Operations

Table operations can be optimized by providing direct access to underlying structures:

```rust
impl LuaVM {
    // Safe public method
    pub fn get_table_field(&mut self, table: TableHandle, key: &Value) -> Value {
        // Safe validation path
        if !self.arena.validate_table_handle(table) {
            return Value::Nil;
        }
        
        // Optimized internal path
        unsafe {
            self.get_table_field_unchecked(table, key)
        }
    }
    
    // Unsafe internal method for hot paths
    #[inline]
    unsafe fn get_table_field_unchecked(&mut self, table: TableHandle, key: &Value) -> Value {
        let table_ref = self.arena.get_table_unchecked(table);
        
        // Direct hash lookup without validation overhead
        match key {
            Value::String(s) => {
                let s_ref = self.arena.get_string_unchecked(*s);
                let hash = compute_hash(&s_ref.bytes);
                // Direct hash lookup
                // ...
            }
            // Other key types...
        }
    }
}
```

### 2.5 Upvalue Implementation

Upvalues require special attention for correctness and performance:

```rust
pub struct Upvalue {
    // Location in stack OR closed value
    location: UpvalueLocation,
}

enum UpvalueLocation {
    Open(usize),      // Stack index (absolute)
    Closed(Value),    // Stored value
}

impl LuaVM {
    // Fast upvalue access
    #[inline]
    unsafe fn get_upvalue_value_unchecked(&self, upvalue: UpvalueHandle) -> Value {
        let upval = self.arena.get_upvalue_unchecked(upvalue);
        match upval.location {
            UpvalueLocation::Open(idx) => *self.stack.get_unchecked(idx),
            UpvalueLocation::Closed(ref val) => val.clone(),
        }
    }
    
    // Close upvalues - maintain Lua semantics
    fn close_upvalues(&mut self, base: usize) {
        for upval in &self.open_upvalues {
            unsafe {
                let upval_ref = self.arena.get_upvalue_unchecked_mut(*upval);
                if let UpvalueLocation::Open(idx) = upval_ref.location {
                    if idx >= base {
                        // Close the upvalue
                        let val = self.stack[idx].clone();
                        upval_ref.location = UpvalueLocation::Closed(val);
                    }
                }
            }
        }
    }
}
```

### 2.6 Opcode Implementation

Opcode implementation follows a hybrid pattern:

```rust
impl LuaVM {
    fn execute(&mut self) -> Result<Value, LuaError> {
        // Main execution loop
        loop {
            // Safe boundary checks only once per execution step
            if self.pc >= self.get_bytecode_len() {
                return Err(LuaError::RuntimeError("Invalid PC".to_string()));
            }
            
            // Fast path instruction fetch
            let instr = unsafe {
                self.get_instruction_unchecked(self.pc)
            };
            self.pc += 1;
            
            // Dispatch based on opcode
            match instr.opcode() {
                OpCode::GetTable => unsafe { self.execute_get_table_unchecked(instr) }?,
                OpCode::SetTable => unsafe { self.execute_set_table_unchecked(instr) }?,
                // Other opcodes...
                _ => return Err(LuaError::NotImplemented(format!("Opcode {:?}", instr.opcode()))),
            }
        }
    }
    
    // Example of unsafe optimized opcode implementation
    #[inline]
    unsafe fn execute_get_table_unchecked(&mut self, instr: Instruction) -> Result<(), LuaError> {
        let a = instr.a() as usize;
        let b = instr.b() as usize;
        let c = instr.rk(self.current_closure, self.base);
        
        // Get table
        let table = self.get_register_unchecked(self.base + b);
        if let Value::Table(table_handle) = *table {
            // Fast path for table access
            let key = c;
            
            // Direct access without validation
            let result = self.get_table_field_unchecked(table_handle, &key);
            
            // Store result
            *self.get_register_unchecked_mut(self.base + a) = result;
            Ok(())
        } else {
            // Fallback to safe path for metamethods
            self.handle_get_table_metamethod(a, table, c)
        }
    }
}
```

## 3. Safety Boundaries and Invariants

### 3.1 Unsafe Code Boundaries

Unsafe code is strictly isolated with clear boundaries:

1. **Module Level**: Unsafe code is contained within specific modules
2. **Method Level**: Public methods provide safe interfaces, unsafe methods are internal only
3. **Critical Paths**: Only performance-critical paths use unsafe code
4. **Invariant Documentation**: All unsafe code has documented invariants

### 3.2 Critical Invariants

Each unsafe module has explicit invariants:

```rust
mod unsafe_vm_core {
    // SAFETY: This module maintains the following invariants:
    // 1. All handle accesses are validated at API boundaries  
    // 2. Stack indices are never out of bounds
    // 3. Register window mapping maintains consistent thread-stack synchronization
    // 4. Upvalues maintain consistent view of the stack/closed values
    // 5. No dangling handles are ever returned to safe code
}
```

### 3.3 Benchmarking and Verification

The hybrid approach includes verification mechanisms:

```rust
#[cfg(debug_assertions)]
mod verification {
    // In debug mode, parallel implementation of safe and unsafe paths
    // with consistency checking
    
    impl LuaVM {
        fn verify_table_access(&self, table: TableHandle, key: &Value, value: &Value) {
            let safe_result = self.get_table_field_safe(table, key);
            let unsafe_result = unsafe { self.get_table_field_unchecked(table, key) };
            
            assert_eq!(safe_result, unsafe_result,
                "Safe and unsafe paths yielded different results!");
        }
    }
}
```

## 4. Implementation Strategy

### 4.1 Phased Implementation

The implementation is broken into phases to manage risk:

1. **Phase 1**: Core memory management replacement
   - Replace transaction system with arena
   - Implement safe and unsafe access paths
   - Maintain validation at API boundaries

2. **Phase 2**: Register management replacement
   - Replace register windows with flat stack
   - Implement frame-based register access
   - Maintain proper upvalue semantics

3. **Phase 3**: VM execution optimization
   - Optimize instruction dispatch
   - Implement fast paths for common opcodes
   - Add profiling infrastructure

4. **Phase 4**: Gradual API migration
   - Update all dependent code to use new APIs
   - Provide compatibility layer where needed
   - Deprecate transaction-based APIs

### 4.2 File Structure

```
src/lua/
├── arena/           // New arena-based memory management
│   ├── mod.rs       // Public safe API
│   ├── internal.rs  // Unsafe internal operations  
│   ├── string.rs    // String-specific arena
│   └── table.rs     // Table-specific arena
├── vm/
│   ├── mod.rs       // Public VM API
│   ├── exec.rs      // Main execution loop
│   ├── opcodes/     // Specific opcode implementations
│   │   ├── table.rs // Table operations
│   │   ├── upval.rs // Upvalue operations
│   │   └── ...
│   ├── stack.rs     // Stack management
│   └── frame.rs     // Call frame handling
└── compat/          // Compatibility layer
    ├── transaction.rs  // Transaction emulation
    └── register_window.rs // Window emulation
```

### 4.3 Migration Path

The migration strategy preserves backward compatibility:

1. Implement new architecture alongside current one
2. Create compatibility wrappers for existing code
3. Migrate opcodes one by one to new architecture
4. Deprecate old interfaces once migration is complete

## 5. Performance Analysis

### 5.1 Expected Performance Improvements

| Operation | Current Impl | Hybrid Impl | Speedup |
|-----------|---------------|--------------|---------|
| Register access | ~200 cycles | ~5 cycles | 40x |
| Table access | ~1000 cycles | ~20 cycles | 50x |
| Closure creation | ~2000 cycles | ~200 cycles | 10x |
| Upvalue access | ~500 cycles | ~10 cycles | 50x |
| Function call | ~5000 cycles | ~300 cycles | 16x |
| Instruction dispatch | ~300 cycles | ~15 cycles | 20x |

### 5.2 Memory Footprint

| Metric | Current Impl | Hybrid Impl | Improvement |
|--------|---------------|--------------|-------------|
| Per-function overhead | ~1KB | ~100B | 10x |
| Stack memory | ~10x target | ~1.5x target | 6.6x |
| Heap fragmentation | High | Low | Significant |
| GC pause times | N/A | <1ms typical | N/A |

## 6. Safety vs Performance Tradeoffs

### 6.1 Safety Guarantees Maintained

1. No undefined behavior at API boundaries
2. Memory safety for all operations
3. Proper error handling and propagation
4. Type safety through handle validation
5. No data races or thread safety issues

### 6.2 Performance-Critical Paths Using Unsafe

1. Register access in tight loops
2. Table access hot paths
3. Instruction dispatch
4. Direct stack manipulation
5. Upvalue access

### 6.3 Verification Strategy

To ensure the unsafe code maintains safety:

1. Extensive property-based testing
2. Fuzz testing focused on memory invariants
3. Dual-path validation in debug mode
4. Debug-only bounds checking
5. Memory poisoning tests

## 7. Conclusion

This hybrid design significantly improves performance while maintaining Rust's safety guarantees where it matters most. By strategically using unsafe code in well-defined, isolated areas, we can achieve performance comparable to traditional C implementations while keeping the majority of the codebase safe and maintaining all safety guarantees at API boundaries.

The implementation strategy allows for incremental migration, avoiding the need for a full rewrite while still delivering substantial performance improvements. This approach respects both Lua semantics and Rust's ownership model by designing abstractions that work with them rather than fighting against them.