# Ferrous Lua Unified Stack Architecture

## Executive Summary

This document defines the architecture for Ferrous Lua, a Rust implementation of Lua 5.1 using a unified stack model with transaction-based safety. This architecture replaces the failed register window approach with a design that maintains full Lua 5.1 compatibility while leveraging Rust's safety guarantees.

## Table of Contents

1. [Architectural Foundation](#architectural-foundation)
2. [The Unified Stack Model](#the-unified-stack-model)
3. [Transaction-Based Safety](#transaction-based-safety)
4. [Calling Conventions](#calling-conventions)
5. [Register Conventions](#register-conventions)
6. [Upvalue Implementation](#upvalue-implementation)
7. [Iterator Protocol](#iterator-protocol)
8. [Memory Management](#memory-management)
9. [Implementation Guidelines](#implementation-guidelines)

## Architectural Foundation

### Why Not Register Windows?

The register window approach failed due to fundamental incompatibilities with Lua 5.1:

1. **Stack Discontinuity**: Lua expects a contiguous stack where any function can access any position. Register windows created isolated 256-register segments that broke this assumption.

2. **C API Incompatibility**: The entire Lua C API assumes direct stack manipulation. Windows required complex translation layers that degraded performance and correctness.

3. **Upvalue Corruption**: Upvalues store absolute stack indices. With windows, these indices became meaningless across window boundaries.

4. **Performance Overhead**: Allocating and deallocating windows for each function call was significantly slower than simple pointer arithmetic.

### The Lua 5.1 Model

Lua 5.1 uses a simple, elegant model:
- Single contiguous stack for all values
- Functions operate on stack slices defined by base pointers
- Registers are just stack positions relative to the base
- Direct memory access enables efficient C interoperability

## The Unified Stack Model

### Core Data Structures

```rust
pub struct LuaState {
    // The unified stack - single contiguous vector
    stack: Vec<TValue>,
    
    // Stack management
    stack_size: usize,      // Current allocated size
    stack_last: usize,      // Last valid index
    base: usize,           // Current function's base
    top: usize,            // Current stack top
    
    // Call frame tracking
    call_info: Vec<CallInfo>,
    ci: usize,             // Current call info index
    
    // Safety and memory management
    transaction_manager: TransactionManager,
    gc: GarbageCollector,
    
    // Global state
    global_state: GlobalState,
}

pub struct CallInfo {
    func: usize,       // Stack index of function
    base: usize,       // Base for this function's activation
    top: usize,        // Top for this function
    saved_pc: usize,   // Saved program counter
    nresults: i32,     // Number of expected results (-1 = multiple)
    tailcalls: u32,    // Number of tail calls in this frame
}

pub struct TValue {
    value: ValueType,
    // Type tag integrated for efficiency
}

pub enum ValueType {
    Nil,
    Boolean(bool),
    Number(f64),
    String(StringRef),
    Table(TableRef),
    Function(FunctionRef),
    UserData(UserDataRef),
    Thread(ThreadRef),
    LightUserData(*const c_void),
}
```

### Stack Layout

The stack is organized as follows:

```
Stack Index    Content                    Frame
-------------------------------------------------
[0]           _ENV (global environment)   
[1]           main function              
[2]           (unused)                   main frame
[3]           local_1                    base = 3
[4]           local_2                    
[5]           temp_value                 
[6]           function_to_call           
[7]           arg_1                      
[8]           arg_2                      call frame
[9]           (where func locals start)  base = 9
[10]          nested_local_1             
...                                      
```

Key principles:
- Stack indices start at 0
- Each function has a base index in the stack
- Register R(n) maps to stack[base + n]
- Function arguments appear just before the new base
- Stack grows upward

## Transaction-Based Safety

### The Safety Challenge

Lua allows arbitrary stack manipulation that can violate Rust's safety guarantees:
- Stack overflow/underflow
- Type confusion
- Dangling references
- Concurrent modification

### Transaction System Design

```rust
pub struct Transaction<'a> {
    state: &'a mut LuaState,
    checkpoint: StackCheckpoint,
    completed: bool,
}

pub struct StackCheckpoint {
    top: usize,
    base: usize,
    ci_depth: usize,
    stack_snapshot: Vec<TValue>, // Copy-on-write optimization possible
}

impl<'a> Transaction<'a> {
    pub fn new(state: &'a mut LuaState) -> Self {
        let checkpoint = StackCheckpoint {
            top: state.top,
            base: state.base,
            ci_depth: state.call_info.len(),
            stack_snapshot: state.stack[0..state.top].to_vec(),
        };
        
        Transaction {
            state,
            checkpoint,
            completed: false,
        }
    }
    
    // Stack manipulation methods
    pub fn push(&mut self, value: TValue) -> Result<(), LuaError> {
        self.check_stack(1)?;
        self.state.stack.push(value);
        self.state.top += 1;
        Ok(())
    }
    
    pub fn pop(&mut self, n: usize) -> Result<Vec<TValue>, LuaError> {
        if self.state.top < self.state.base + n {
            return Err(LuaError::StackUnderflow);
        }
        
        let mut values = Vec::with_capacity(n);
        for _ in 0..n {
            self.state.top -= 1;
            values.push(self.state.stack[self.state.top].clone());
        }
        values.reverse();
        Ok(values)
    }
    
    pub fn get(&self, idx: usize) -> Result<&TValue, LuaError> {
        let abs_idx = self.absolute_index(idx)?;
        self.state.stack.get(abs_idx)
            .ok_or(LuaError::InvalidStackIndex(idx))
    }
    
    pub fn set(&mut self, idx: usize, value: TValue) -> Result<(), LuaError> {
        let abs_idx = self.absolute_index(idx)?;
        if abs_idx >= self.state.stack.len() {
            return Err(LuaError::InvalidStackIndex(idx));
        }
        self.state.stack[abs_idx] = value;
        Ok(())
    }
    
    pub fn commit(mut self) {
        self.completed = true;
        // Changes persist
    }
    
    pub fn rollback(mut self) {
        self.completed = true;
        self.state.restore_checkpoint(&self.checkpoint);
    }
}

impl<'a> Drop for Transaction<'a> {
    fn drop(&mut self) {
        if !self.completed {
            // Automatic rollback on panic/error
            self.state.restore_checkpoint(&self.checkpoint);
        }
    }
}
```

### Transaction Usage in VM

```rust
// Example: CALL opcode implementation
OpCode::Call { func: a, nargs: b, nresults: c } => {
    let mut tx = Transaction::new(state);
    
    let func_idx = base + a;
    let func = tx.get(func_idx)?.clone();
    
    // Validate function
    let (is_lua, expected_args) = match &func {
        TValue::Function(f) => (true, f.arity),
        TValue::CFunction(_) => (false, -1), // C functions handle their own arity
        _ => return Err(LuaError::NotCallable),
    };
    
    // Set up new call frame
    let new_base = func_idx + 1;
    let actual_args = if b == 0 { tx.state.top - new_base } else { b - 1 };
    
    // Create new CallInfo
    let new_ci = CallInfo {
        func: func_idx,
        base: new_base,
        top: new_base + actual_args,
        saved_pc: pc + 1,
        nresults: c - 1,
        tailcalls: 0,
    };
    
    tx.state.call_info.push(new_ci);
    tx.state.ci += 1;
    tx.state.base = new_base;
    
    // Adjust stack top for fixed argument functions
    if is_lua && expected_args >= 0 {
        tx.adjust_top(new_base + expected_args as usize)?;
    }
    
    tx.commit();
    Ok(StepResult::Call(func))
}
```

## Calling Conventions

### Lua Function Calls

1. **Setup Phase**:
   - Function at stack[base + A]
   - Arguments at stack[base + A + 1] through stack[base + A + B]
   - B = 0 means use all values up to current top

2. **Call Phase**:
   - Create new CallInfo with base = func_index + 1
   - Adjust top for expected arguments
   - Save return address (pc + 1)

3. **Return Phase**:
   - Results placed starting at func index
   - Restore previous CallInfo
   - Adjust top based on expected results

### C Function Interface

```rust
pub type CFunction = fn(&mut LuaState) -> Result<i32, LuaError>;

// C function implementation pattern
pub fn lua_print(state: &mut LuaState) -> Result<i32, LuaError> {
    let mut tx = Transaction::new(state);
    
    let nargs = tx.get_top() - tx.get_base();
    
    for i in 0..nargs {
        let value = tx.get(tx.get_base() + i)?;
        print!("{}", value.to_string()?);
        if i < nargs - 1 {
            print!("\t");
        }
    }
    println!();
    
    tx.commit();
    Ok(0) // Number of results
}

// C function call handling in VM
fn call_c_function(state: &mut LuaState, func: CFunction) -> Result<(), LuaError> {
    // C functions operate directly on the stack
    let nresults = func(state)?;
    
    // Adjust stack to have exactly nresults values
    let ci = &state.call_info[state.ci];
    let res_start = ci.func;
    let res_end = res_start + nresults as usize;
    
    // Move results to correct position
    for i in 0..nresults as usize {
        state.stack[res_start + i] = state.stack[state.base + i].clone();
    }
    
    state.top = res_end;
    Ok(())
}
```

### Stack Manipulation API

```rust
impl LuaState {
    // Push operations
    pub fn push_nil(&mut self) { self.push(TValue::Nil); }
    pub fn push_boolean(&mut self, b: bool) { self.push(TValue::Boolean(b)); }
    pub fn push_number(&mut self, n: f64) { self.push(TValue::Number(n)); }
    pub fn push_string(&mut self, s: &str) { self.push(TValue::String(self.new_string(s))); }
    
    // Stack access
    pub fn get_top(&self) -> usize { self.top - self.base }
    pub fn set_top(&mut self, newtop: usize) { self.top = self.base + newtop; }
    
    // Type checking
    pub fn is_nil(&self, idx: usize) -> bool {
        matches!(self.index2value(idx), Some(TValue::Nil))
    }
    
    pub fn is_number(&self, idx: usize) -> bool {
        matches!(self.index2value(idx), Some(TValue::Number(_)))
    }
    
    // Value access
    pub fn to_number(&self, idx: usize) -> Result<f64, LuaError> {
        match self.index2value(idx) {
            Some(TValue::Number(n)) => Ok(*n),
            Some(TValue::String(s)) => s.parse().map_err(|_| LuaError::TypeMismatch),
            _ => Err(LuaError::TypeMismatch),
        }
    }
}
```

## Register Conventions

### Register Mapping

In Lua 5.1, "registers" are simply stack positions relative to the current function's base:

```rust
// Register access macros
fn R(base: usize, n: usize) -> usize { base + n }
fn RK(base: usize, n: usize, k: &[TValue]) -> TValue {
    if n & 0x100 != 0 {
        k[n & 0xFF].clone()
    } else {
        stack[base + n].clone()
    }
}
```

### Register Allocation Rules

1. **Local Variables**: Allocated sequentially from R(0)
2. **Temporaries**: Allocated after locals
3. **Varargs**: Accessed via special handling, not fixed registers
4. **Constants**: Accessed via RK with high bit set

### Register Usage in Opcodes

```rust
// Example: Binary operation
OpCode::Add { dest: a, left: b, right: c } => {
    let mut tx = Transaction::new(state);
    
    let left = if b & 0x100 != 0 {
        constants[b & 0xFF].clone()
    } else {
        tx.get(base + b)?.clone()
    };
    
    let right = if c & 0x100 != 0 {
        constants[c & 0xFF].clone()
    } else {
        tx.get(base + c)?.clone()
    };
    
    let result = match (left, right) {
        (TValue::Number(l), TValue::Number(r)) => TValue::Number(l + r),
        _ => return Err(LuaError::ArithmeticError),
    };
    
    tx.set(base + a, result)?;
    tx.commit();
}
```

## Upvalue Implementation

### Upvalue Representation

```rust
pub struct UpvalDesc {
    name: Option<String>,
    instack: bool,      // Whether the variable is in stack
    idx: u8,           // Register index if instack, upvalue index otherwise
}

pub struct Upvalue {
    location: UpvalueLocation,
    closed_value: Option<TValue>, // Value when closed
}

pub enum UpvalueLocation {
    Open(usize),    // Stack index (absolute)
    Closed,         // Moved to closed_value
}

impl LuaState {
    pub fn new_upvalue(&mut self, stack_idx: usize) -> UpvalueRef {
        let upval = Upvalue {
            location: UpvalueLocation::Open(stack_idx),
            closed_value: None,
        };
        self.alloc_upvalue(upval)
    }
    
    pub fn get_upvalue(&self, upval: &Upvalue) -> Result<TValue, LuaError> {
        match upval.location {
            UpvalueLocation::Open(idx) => Ok(self.stack[idx].clone()),
            UpvalueLocation::Closed => Ok(upval.closed_value.clone().unwrap()),
        }
    }
    
    pub fn set_upvalue(&mut self, upval: &mut Upvalue, value: TValue) -> Result<(), LuaError> {
        match upval.location {
            UpvalueLocation::Open(idx) => {
                self.stack[idx] = value;
                Ok(())
            }
            UpvalueLocation::Closed => {
                upval.closed_value = Some(value);
                Ok(())
            }
        }
    }
    
    pub fn close_upvalues(&mut self, level: usize) {
        // Close all open upvalues pointing to stack positions >= level
        for upval in &mut self.open_upvalues {
            if let UpvalueLocation::Open(idx) = upval.location {
                if idx >= level {
                    upval.closed_value = Some(self.stack[idx].clone());
                    upval.location = UpvalueLocation::Closed;
                }
            }
        }
    }
}
```

### Upvalue Access in Functions

```rust
// GETUPVAL opcode
OpCode::GetUpval { dest: a, upval: b } => {
    let mut tx = Transaction::new(state);
    
    let func = tx.get_current_function()?;
    let upval = func.get_upvalue(b)?;
    let value = tx.get_upvalue_value(upval)?;
    
    tx.set(base + a, value)?;
    tx.commit();
}

// SETUPVAL opcode  
OpCode::SetUpval { upval: a, source: b } => {
    let mut tx = Transaction::new(state);
    
    let value = tx.get(base + b)?.clone();
    let func = tx.get_current_function()?;
    let upval = func.get_upvalue_mut(a)?;
    
    tx.set_upvalue_value(upval, value)?;
    tx.commit();
}
```

## Iterator Protocol

### Iterator Function Contract

The Lua iterator protocol requires specific behavior:

```rust
// Generic for loop: for var_1, ..., var_n in explist do block end
// Translates to:
// local f, s, var = explist
// while true do
//   local var_1, ..., var_n = f(s, var)
//   if var_1 == nil then break end
//   var = var_1
//   block
// end
```

### Standard Iterator Implementations

```rust
// next() - stateless table iterator
pub fn lua_next(state: &mut LuaState) -> Result<i32, LuaError> {
    let mut tx = Transaction::new(state);
    
    // Get arguments
    let table = match tx.get(tx.get_base())? {
        TValue::Table(t) => t,
        _ => return Err(LuaError::TypeError { 
            expected: "table".to_string(),
            got: tx.get(tx.get_base())?.type_name() 
        }),
    };
    
    let key = tx.get(tx.get_base() + 1)?.clone();
    
    // Find next key-value pair
    if let Some((next_key, next_value)) = table.next(&key)? {
        tx.set(tx.get_base(), next_key)?;
        tx.set(tx.get_base() + 1, next_value)?;
        tx.set_top(2);
        tx.commit();
        Ok(2)
    } else {
        tx.set(tx.get_base(), TValue::Nil)?;
        tx.set_top(1);
        tx.commit();
        Ok(1)
    }
}

// pairs() - returns iterator triplet for tables
pub fn lua_pairs(state: &mut LuaState) -> Result<i32, LuaError> {
    let mut tx = Transaction::new(state);
    
    let table = match tx.get(tx.get_base())? {
        TValue::Table(t) => t.clone(),
        _ => return Err(LuaError::TypeError { 
            expected: "table".to_string(),
            got: tx.get(tx.get_base())?.type_name() 
        }),
    };
    
    // Push: next, table, nil
    tx.set(tx.get_base(), TValue::CFunction(lua_next))?;
    tx.set(tx.get_base() + 1, TValue::Table(table))?;
    tx.set(tx.get_base() + 2, TValue::Nil)?;
    tx.set_top(3);
    
    tx.commit();
    Ok(3)
}

// ipairs() - returns iterator triplet for array part
pub fn lua_ipairs(state: &mut LuaState) -> Result<i32, LuaError> {
    let mut tx = Transaction::new(state);
    
    let table = match tx.get(tx.get_base())? {
        TValue::Table(t) => t.clone(),
        _ => return Err(LuaError::TypeError { 
            expected: "table".to_string(),
            got: tx.get(tx.get_base())?.type_name() 
        }),
    };
    
    // Push: ipairs_next, table, 0
    tx.set(tx.get_base(), TValue::CFunction(ipairs_next))?;
    tx.set(tx.get_base() + 1, TValue::Table(table))?;
    tx.set(tx.get_base() + 2, TValue::Number(0.0))?;
    tx.set_top(3);
    
    tx.commit();
    Ok(3)
}

fn ipairs_next(state: &mut LuaState) -> Result<i32, LuaError> {
    let mut tx = Transaction::new(state);
    
    let table = match tx.get(tx.get_base())? {
        TValue::Table(t) => t,
        _ => return Err(LuaError::TypeError { 
            expected: "table".to_string(),
            got: tx.get(tx.get_base())?.type_name() 
        }),
    };
    
    let i = tx.to_number(tx.get_base() + 1)? as i64 + 1;
    
    if let Some(value) = table.get_int(i)? {
        tx.set(tx.get_base(), TValue::Number(i as f64))?;
        tx.set(tx.get_base() + 1, value)?;
        tx.set_top(2);
        tx.commit();
        Ok(2)
    } else {
        tx.set(tx.get_base(), TValue::Nil)?;
        tx.set_top(1);
        tx.commit();
        Ok(1)
    }
}
```

### TFORLOOP Implementation

```rust
OpCode::TForLoop { base: a, nvars: c } => {
    let mut tx = Transaction::new(state);
    
    // R(A) = iterator function
    // R(A+1) = state
    // R(A+2) = control variable
    // R(A+3) and up = loop variables
    
    // Copy state and control for function call
    let state_val = tx.get(base + a + 1)?.clone();
    let control_val = tx.get(base + a + 2)?.clone();
    
    tx.set(base + a + 3, state_val)?;
    tx.set(base + a + 4, control_val)?;
    
    // Call iterator: R(A+3), ..., R(A+2+C) = R(A)(R(A+1), R(A+2))
    tx.commit();
    
    // Perform the call
    let results = vm.call_function(base + a, 2, c)?;
    
    // Check if iteration should continue (first result != nil)
    let mut tx = Transaction::new(state);
    let first_result = tx.get(base + a + 3)?;
    
    if !matches!(first_result, TValue::Nil) {
        // Update control variable
        tx.set(base + a + 2, first_result.clone())?;
        tx.commit();
        Ok(StepResult::Continue)
    } else {
        tx.commit();
        Ok(StepResult::Jump(pc + 1)) // Skip to instruction after loop
    }
}
```

## Memory Management

### Garbage Collection Integration

```rust
pub struct GarbageCollector {
    threshold: usize,
    debt: isize,
    totalbytes: usize,
    gcstate: GCState,
}

pub enum GCState {
    Pause,
    Propagate,
    Atomic,
    Sweep,
}

impl LuaState {
    pub fn check_gc(&mut self) {
        if self.gc.totalbytes >= self.gc.threshold {
            self.run_gc_cycle();
        }
    }
    
    pub fn alloc_value<T: GCObject>(&mut self, value: T) -> Ref<T> {
        let size = std::mem::size_of::<T>();
        self.gc.totalbytes += size;
        
        let reference = self.heap.alloc(value);
        self.check_gc();
        
        reference
    }
}
```

### Stack Protection

```rust
impl Transaction<'_> {
    pub fn check_stack(&self, n: usize) -> Result<(), LuaError> {
        const STACK_MAX: usize = 1_000_000; // 1M values max
        const STACK_ERROR_MARGIN: usize = 200;
        
        let needed = self.state.top + n;
        
        if needed > STACK_MAX - STACK_ERROR_MARGIN {
            Err(LuaError::StackOverflow)
        } else if needed > self.state.stack.capacity() {
            // Need to grow - this would be done carefully
            Ok(())
        } else {
            Ok(())
        }
    }
}
```

## Implementation Guidelines

### Phase 1: Core Infrastructure
1. Implement `LuaState` with unified stack
2. Implement `Transaction` system
3. Create basic value types
4. Add stack manipulation primitives

### Phase 2: VM Execution
1. Implement opcode handlers using transactions
2. Add proper call/return handling
3. Implement tail call optimization
4. Add coroutine support

### Phase 3: Standard Library
1. Implement base library functions
2. Add table manipulation library
3. Implement string library
4. Add math and I/O libraries

### Phase 4: Advanced Features
1. Implement garbage collector
2. Add debug library support
3. Optimize hot paths
4. Add JIT compilation hooks

### Best Practices

1. **Always Use Transactions**: Every stack manipulation should be wrapped in a transaction
2. **Validate Types Early**: Check types at transaction boundaries
3. **Minimize Allocations**: Reuse stack space where possible
4. **Profile Regularly**: The unified stack should be faster than windows
5. **Test Exhaustively**: Use Lua 5.1 test suite for validation

### Common Pitfalls

1. **Direct Stack Access**: Never modify the stack outside a transaction
2. **Register Confusion**: Remember R(n) = stack[base + n], not stack[n]
3. **Upvalue Lifetime**: Always close upvalues when leaving their scope
4. **C Function Returns**: Must return exact count, not approximate
5. **Iterator Protocol**: Must return nil to terminate, not nothing

## Conclusion

This unified stack architecture provides a solid foundation for implementing Lua 5.1 in Rust. By abandoning the register window approach and embracing Lua's original design with added safety through transactions, we achieve:

- Full Lua 5.1 compatibility
- Rust memory safety
- Excellent performance
- Clean, maintainable code

The transaction system ensures that even in the presence of panics or errors, the VM state remains consistent, while the unified stack model ensures compatibility with the entire Lua ecosystem.