# Lua VM Transaction Patterns Guide

This document provides detailed guidance on transaction usage patterns for the Ferrous Lua VM. Transactions are the central mechanism for maintaining memory safety and borrow checker compliance in the Rust implementation.

## 1. Core Transaction Principles

### 1.1 Transaction-Only Access

**FUNDAMENTAL RULE:** All heap access must go through transactions. Direct heap access is forbidden outside of the transaction system itself.

```rust
// INCORRECT ❌
let value = self.heap.get_table_field(table, key)?;

// CORRECT ✅
let mut tx = HeapTransaction::new(&mut self.heap);
let value = tx.read_table_field(table, key)?;
tx.commit()?;
```

### 1.2 Single Transaction Lifecycle

Each operation should use exactly one transaction from beginning to end:

```
┌──────────────────┐     ┌──────────────────┐     ┌─────────────┐
│  Begin           │────►│  Queue Changes   │────►│  Commit     │
│  let mut tx = .. │     │  tx.set_*(..)    │     │  tx.commit()│
└──────────────────┘     └──────────────────┘     └─────────────┘
```

**INCORRECT Pattern** (creating multiple transactions):

```rust
// INCORRECT ❌ - Multiple transactions in same operation
fn execute_instruction(&mut self, instr: Instruction) -> Result<()> {
    let mut tx1 = HeapTransaction::new(&mut self.heap);
    let value = tx1.read_register(instr.a())?;
    tx1.commit()?;
    
    let mut tx2 = HeapTransaction::new(&mut self.heap);
    tx2.set_register(instr.b(), value);
    tx2.commit()?;
}
```

**CORRECT Pattern** (single transaction):

```rust
// CORRECT ✅ - Single transaction for whole operation
fn execute_instruction(&mut self, instr: Instruction) -> Result<()> {
    let mut tx = HeapTransaction::new(&mut self.heap);
    
    let value = tx.read_register(instr.a())?;
    tx.set_register(instr.b(), value);
    
    tx.commit()?;
}
```

### 1.3 Transaction Commit Doesn't Consume Self

Transactions must remain usable after commit for error handling:

```rust
// INCORRECT ❌
pub fn commit(self) -> Result<()> { /* ... */ }

// CORRECT ✅
pub fn commit(&mut self) -> Result<()> { /* ... */ }
```

### 1.4 Queue Operations, Don't Execute Directly

Operations that would cause recursion must be queued:

```rust
// INCORRECT ❌
let result = self.execute_function(closure, &args)?;

// CORRECT ✅
tx.queue_operation(PendingOperation::FunctionCall {
    closure, 
    args: args.clone(),
    context: ReturnContext::Register { /* ... */ },
});
```

## 2. Transaction Usage Patterns

### 2.1 Standard Instruction Pattern

For most VM instructions, follow this pattern:

```rust
fn execute_instruction(&mut self, instr: Instruction) -> Result<StepResult> {
    // 1. Create transaction
    let mut tx = HeapTransaction::new(&mut self.heap);
    
    // 2. Extract instruction parameters
    let a = instr.a() as usize;
    let b = instr.b() as usize;
    let c = instr.c() as usize;
    let base = self.get_current_base_register()?;
    
    // 3. Read input values through transaction
    let value_b = tx.read_register(self.current_thread.clone(), base + b)?;
    let value_c = tx.read_register(self.current_thread.clone(), base + c)?;
    
    // 4. Perform computation
    let result = self.compute_result(value_b, value_c)?;
    
    // 5. Write results through transaction
    tx.set_register(self.current_thread.clone(), base + a, result);
    
    // 6. Queue any pending operations
    if needs_metamethod(&result) {
        tx.queue_operation(PendingOperation::MetamethodCall { /* ... */ });
    }
    
    // 7. Commit transaction
    let pending_ops = tx.commit()?;
    
    // 8. Process any pending operations
    for op in pending_ops {
        self.pending_operations.push_back(op);
    }
    
    // 9. Return step result
    Ok(StepResult::Continue)
}
```

### 2.2 Two-Phase Borrow Pattern

When an operation needs multiple heap accesses that would conflict:

```rust
fn get_metamethod_value(&mut self, table: TableHandle, method: &str) -> Result<Value> {
    // Phase 1: Extract information needed for lookup
    let (has_metatable, metatable_handle) = {
        let mut tx = HeapTransaction::new(&mut self.heap);
        let table_obj = tx.get_table(table.clone())?;
        
        // Get the information we need and drop borrow immediately
        let result = (
            table_obj.metatable.is_some(),
            table_obj.metatable.clone()
        );
        
        tx.commit()?;
        
        result
    };
    
    // Phase 2: Use the extracted information
    if has_metatable {
        if let Some(metatable) = metatable_handle {
            let mut tx = HeapTransaction::new(&mut self.heap);
            let method_handle = tx.create_string(method)?;
            let result = tx.read_table_field(metatable.clone(), &Value::String(method_handle.clone()))?;
            tx.commit()?;
            Ok(result)
        } else {
            Ok(Value::Nil)
        }
    } else {
        Ok(Value::Nil)
    }
}
```

### 2.3 C Function Execution Pattern

When executing C functions:

```rust
fn call_c_function(&mut self, cfunc: CFunction, args: &[Value]) -> Result<Vec<Value>> {
    // 1. Setup isolated execution context
    let mut ctx = CExecutionContext::new(self, args);
    
    // 2. Execute function with controlled access
    let result_count = cfunc(&mut ctx)?;
    
    // 3. Process results
    let results = ctx.get_results(result_count)?;
    
    // 4. Clean up context
    ctx.finalize()?;
    
    Ok(results)
}
```

### 2.4 Transaction Error Recovery

Handling errors within transactions:

```rust
fn execute_with_recovery<F, R>(&mut self, f: F) -> Result<R>
where F: FnOnce(&mut HeapTransaction) -> Result<R>
{
    let mut tx = HeapTransaction::new(&mut self.heap);
    let savepoint = tx.savepoint();
    
    match f(&mut tx) {
        Ok(result) => {
            tx.commit()?;
            Ok(result)
        }
        Err(e) => {
            // Perform rollback to savepoint
            tx.rollback(savepoint)?;
            
            // Handle specific errors
            match e {
                LuaError::MemoryError => {
                    // Run garbage collection and retry
                    self.collect_garbage()?;
                    self.execute_with_recovery(f)
                }
                _ => Err(e),
            }
        }
    }
}
```

## 3. Reference Patterns for Common Operations

### 3.1 Table Access with Metamethods

```rust
fn get_table_field(&mut self, table: TableHandle, key: Value) -> Result<Value> {
    let mut tx = HeapTransaction::new(&mut self.heap);
    
    // Try direct lookup
    match tx.read_table_field(table.clone(), &key) {
        Ok(Value::Nil) => {
            // Check for __index metamethod
            let mm_handle = tx.create_string("__index")?;
            let metatable = tx.get_table_metatable(table.clone())?;
            
            if let Some(mt) = metatable {
                let metamethod = tx.read_table_field(mt.clone(), &Value::String(mm_handle.clone()))?;
                
                match metamethod {
                    Value::Table(meta_table) => {
                        // __index is a table, try lookup there
                        let result = tx.read_table_field(meta_table.clone(), &key)?;
                        tx.commit()?;
                        Ok(result)
                    }
                    Value::Closure(closure) => {
                        // __index is a function, queue call
                        tx.queue_operation(PendingOperation::MetamethodCall {
                            method: mm_handle,
                            object: Value::Table(table.clone()),
                            args: vec![Value::Table(table), key.clone()],
                            continuation: MetamethodContinuation::ReplaceResult,
                        });
                        tx.commit()?;
                        Ok(Value::Nil) // Placeholder, will be replaced
                    }
                    _ => {
                        // No usable metamethod
                        tx.commit()?;
                        Ok(Value::Nil)
                    }
                }
            } else {
                // No metatable
                tx.commit()?;
                Ok(Value::Nil)
            }
        }
        Ok(value) => {
            // Direct lookup succeeded
            tx.commit()?;
            Ok(value)
        }
        Err(e) => {
            // Error during lookup
            tx.abort()?;
            Err(e)
        }
    }
}
```

### 3.2 Concatenation Operation

```rust
fn execute_concat(&mut self, values: Vec<Value>, target: usize) -> Result<()> {
    // Initial transaction to collect string values
    let mut tx = HeapTransaction::new(&mut self.heap);
    let mut accumulated = Vec::new();
    let mut to_stringify = Vec::new();
    
    // First parse, collect strings and identify non-strings
    for (i, value) in values.iter().enumerate() {
        match value {
            Value::String(handle) => {
                let string = tx.get_string_value(handle.clone())?;
                accumulated.push(string);
            }
            Value::Number(n) => {
                accumulated.push(n.to_string());
            }
            _ => {
                // Need to convert through __tostring
                to_stringify.push((i, value.clone()));
            }
        }
    }
    
    // If all strings, just concatenate and we're done
    if to_stringify.is_empty() {
        let result = accumulated.join("");
        let str_handle = tx.create_string(&result)?;
        tx.set_register(self.current_thread.clone(), target, Value::String(str_handle));
        tx.commit()?;
        return Ok(());
    }
    
    // Queue a concatenation operation for non-string values
    tx.queue_operation(PendingOperation::Concatenation {
        values,
        current_index: 0,
        accumulated,
        non_strings: to_stringify,
    });
    tx.commit()?;
    
    Ok(())
}
```

### 3.3 Function Call Pattern

```rust
fn execute_call(&mut self, func: Value, args: Vec<Value>) -> Result<Vec<Value>> {
    // Phase 1: Determine how to handle the call
    let call_info = match func {
        Value::Closure(closure) => {
            // Queue closure call
            let mut tx = HeapTransaction::new(&mut self.heap);
            
            tx.queue_operation(PendingOperation::FunctionCall {
                closure,
                args,
                return_context: ReturnContext::FinalResult,
            });
            
            tx.commit()?;
            
            return Ok(Vec::new()); // Will be replaced with real result
        }
        Value::CFunction(cfunc) => {
            // Execute C function directly
            let args_copy = args.clone(); // Avoid ownership issues
            
            self.call_c_function(cfunc, &args_copy)?
        }
        _ => {
            // Check for __call metamethod
            let mut tx = HeapTransaction::new(&mut self.heap);
            let mm_handle = tx.create_string("__call")?;
            
            // Get metatable if any
            let metatable = match &func {
                Value::Table(table) => tx.get_table_metatable(table.clone())?,
                Value::UserData(ud) => tx.get_userdata_metatable(ud.clone())?,
                _ => None,
            };
            
            if let Some(mt) = metatable {
                let metamethod = tx.read_table_field(mt.clone(), &Value::String(mm_handle.clone()))?;
                
                if !metamethod.is_nil() {
                    // Queue metamethod call
                    let mut call_args = Vec::with_capacity(args.len() + 1);
                    call_args.push(func.clone()); // Self as first argument
                    call_args.extend(args);
                    
                    tx.queue_operation(PendingOperation::MetamethodCall {
                        method: mm_handle,
                        object: func.clone(),
                        args: call_args,
                        continuation: MetamethodContinuation::ReplaceResult,
                    });
                    
                    tx.commit()?;
                    
                    return Ok(Vec::new()); // Will be replaced with real result
                }
            }
            
            // No metamethod, error
            tx.commit()?;
            return Err(LuaError::TypeError(format!("attempt to call a {} value", func.type_name())));
        }
    };
    
    Ok(call_info)
}
```

## 4. Common Implementation Pitfalls

### 4.1 Transaction Nesting

**WRONG:**
```rust
fn complex_operation(&mut self) -> Result<()> {
    let mut tx1 = HeapTransaction::new(&mut self.heap);
    // Some operations...
    
    // ❌ Nested transaction while tx1 is still active
    let mut tx2 = HeapTransaction::new(&mut self.heap); // BORROW ERROR!
    
    tx1.commit()?;
    tx2.commit()?;
    Ok(())
}
```

**CORRECT:**
```rust
fn complex_operation(&mut self) -> Result<()> {
    // First phase
    {
        let mut tx = HeapTransaction::new(&mut self.heap);
        // First set of changes...
        tx.commit()?;
    } // tx is dropped here
    
    // Second phase - new transaction
    {
        let mut tx = HeapTransaction::new(&mut self.heap);
        // Second set of changes...
        tx.commit()?;
    }
    
    Ok(())
}
```

### 4.2 Direct Heap Access after Transaction Creation

**WRONG:**
```rust
fn mixed_access(&mut self) -> Result<()> {
    let mut tx = HeapTransaction::new(&mut self.heap);
    
    // Operations through transaction...
    let value = tx.read_register(self.current_thread.clone(), 0)?;
    
    // ❌ Direct heap access with transaction active
    let string = self.heap.get_string_value(handle)?; // BORROW ERROR!
    
    tx.commit()?;
    Ok(())
}
```

**CORRECT:**
```rust
fn consistent_access(&mut self) -> Result<()> {
    let mut tx = HeapTransaction::new(&mut self.heap);
    
    // All access through the transaction
    let value = tx.read_register(self.current_thread.clone(), 0)?;
    let string = tx.get_string_value(handle.clone())?;
    
    tx.commit()?;
    Ok(())
}
```

### 4.3 Forgetting to `.clone()` Handles

**WRONG:**
```rust
fn handle_ownership_issue(&mut self, table: TableHandle) -> Result<Value> {
    let mut tx = HeapTransaction::new(&mut self.heap);
    
    // ❌ Moving table into the transaction
    tx.set_table_field(table, key, value)?; // handle consumed here
    
    // ❌ Can't use table anymore, it was moved
    let other_value = tx.read_table_field(table, other_key)?; // ERROR!
    
    tx.commit()?;
    Ok(other_value)
}
```

**CORRECT:**
```rust
fn handle_ownership_correct(&mut self, table: TableHandle) -> Result<Value> {
    let mut tx = HeapTransaction::new(&mut self.heap);
    
    // ✅ Clone handle to avoid consuming it
    tx.set_table_field(table.clone(), key, value)?; 
    
    // ✅ Can still use table
    let other_value = tx.read_table_field(table, other_key)?;
    
    tx.commit()?;
    Ok(other_value)
}
```

### 4.4 Keeping Transactions Active Too Long

**WRONG:**
```rust
fn long_transaction(&mut self) -> Result<()> {
    let mut tx = HeapTransaction::new(&mut self.heap);
    
    // First operation
    let value = tx.read_register(self.current_thread.clone(), 0)?;
    
    // ❌ Long computation with transaction active
    let result = self.expensive_computation(value)?;
    
    // Set result
    tx.set_register(self.current_thread.clone(), 1, result);
    tx.commit()?;
    
    Ok(())
}
```

**CORRECT:**
```rust
fn phased_operations(&mut self) -> Result<()> {
    // Phase 1: Read inputs
    let value = {
        let mut tx = HeapTransaction::new(&mut self.heap);
        let val = tx.read_register(self.current_thread.clone(), 0)?;
        tx.commit()?;
        val
    };
    
    // Phase 2: Computation (no active transaction)
    let result = self.expensive_computation(value)?;
    
    // Phase 3: Store results
    {
        let mut tx = HeapTransaction::new(&mut self.heap);
        tx.set_register(self.current_thread.clone(), 1, result);
        tx.commit()?;
    }
    
    Ok(())
}
```

## 5. Transaction Implementation Tips

### 5.1 Use Debug Assertions

Add runtime checks to catch transaction bugs:

```rust
impl<'a> HeapTransaction<'a> {
    pub fn read_register(&self, thread: ThreadHandle, index: usize) -> Result<Value> {
        debug_assert!(self.state == TransactionState::Active, 
            "Attempted to read register from inactive transaction");
        
        // Rest of implementation...
    }
}
```

### 5.2 Avoid Self-References in Transactions

Don't create data structures that reference other parts of the heap:

```rust
// INCORRECT ❌ - Creates self-reference in heap
fn bad_reference(&mut self, table: TableHandle) -> Result<()> {
    let mut tx = HeapTransaction::new(&mut self.heap);
    
    // Creating a reference to another part of the heap
    tx.set_table_field(table.clone(), 
        Value::String(tx.create_string("self")?),
        Value::Table(table))?;  // Table refers to itself!
    
    tx.commit()?;
    Ok(())
}
```

### 5.3 Clone Values When Reading

Always clone values when reading to avoid ownership issues:

```rust
impl<'a> HeapTransaction<'a> {
    pub fn read_register(&self, thread: ThreadHandle, index: usize) -> Result<Value> {
        // Get a reference
        let value_ref = self.heap.get_thread_register(thread, index)?;
        
        // Return a clone
        Ok(value_ref.clone())
    }
}
```

### 5.4 Validate All External Handles

Always validate handles from external sources:

```rust
impl<'a> HeapTransaction<'a> {
    pub fn read_table_field(&self, table: TableHandle, key: &Value) -> Result<Value> {
        // Validate the handle first
        self.validate_handle(table)?;
        
        // Then proceed with operation
        // ...
    }
}
```

### 5.5 Value Semantics Patterns

Ensure value semantics (especially for strings) are preserved despite handle-based implementation:

```rust
// String interning pattern - ensuring consistent handles for identical strings
impl LuaHeap {
    fn create_string_internal(&mut self, s: &str) -> LuaResult<StringHandle> {
        // Check cache first for content-based lookup
        if let Some(&handle) = self.string_cache.get(s.as_bytes()) {
            if self.strings.contains(handle.0) {
                // Debug output for critical strings
                if s.len() < 30 && (s == "print" || s == "type" || s.starts_with("__")) {
                    println!("Debug: Reused interned string '{}' with handle {:?}", s, handle);
                }
                return Ok(handle);
            }
            // Handle was invalidated, remove from cache
            self.string_cache.remove(s.as_bytes());
        }
        
        // Create new string
        let lua_string = LuaString::new(s);
        let handle = StringHandle::from(self.strings.insert(lua_string));
        
        // Add to cache for future lookups
        self.string_cache.insert(s.as_bytes().to_vec(), handle);
        
        // Debug output for critical strings
        if s.len() < 30 && (s == "print" || s == "type" || s.starts_with("__")) {
            println!("Debug: Created new string '{}' with handle {:?}", s, handle);
        }
        
        Ok(handle)
    }
    
    fn pre_intern_common_strings(&mut self) -> LuaResult<()> {
        // Pre-intern strings used for critical VM operations
        const COMMON_STRINGS: &[&str] = &[
            // Standard library functions
            "print", "type", "tostring", "tonumber", 
            "next", "pairs", "ipairs", 
            "getmetatable", "setmetatable",
            "rawget", "rawset", "rawequal",
            
            // Metamethods
            "__index", "__newindex", "__call", "__tostring",
            "__add", "__sub", "__mul", "__div", "__mod", "__pow",
            "__concat", "__len", "__eq", "__lt", "__le",
            
            // Common keys and values
            "value", "self", "key",
        ];
        
        for s in COMMON_STRINGS {
            self.create_string_internal(s)?;
        }
        
        Ok(())
    }
}

// Module loading pattern - ensure proper string handling during loading
pub mod loader {
    pub fn load_module<'a>(tx: &mut HeapTransaction<'a>, module: &CompiledModule) -> LuaResult<FunctionProtoHandle> {
        // Step 1: Create all string handles with proper interning
        let mut string_handles = Vec::with_capacity(module.strings.len());
        for s in &module.strings {
            // This leverages string interning to get consistent handles
            string_handles.push(tx.create_string(s)?);
        }
        
        // Rest of module loading...
    }
}
```

### 5.6 String Reference Patterns

For string content access in performance-critical paths:

```rust
impl<'a> HeapTransaction<'a> {
    // Fast path for string equality in table lookup
    fn is_string_equal(&self, a: StringHandle, b: StringHandle) -> LuaResult<bool> {
        // Fast path: same handle means same string (if interning works properly)
        if a == b {
            return Ok(true);
        }
        
        // Slow path: compare content
        let content_a = self.heap.get_string(a)?.to_str()?;
        let content_b = self.heap.get_string(b)?.to_str()?;
        
        Ok(content_a == content_b)
    }
}
```

## 6. Testing Transaction Correctness

### 6.1 Invariant Tests

Write tests that verify transaction invariants:

```rust
#[test]
fn test_transaction_atomicity() {
    let mut heap = LuaHeap::new();
    let table = heap.create_table_internal();
    
    // Create a transaction that will fail
    let mut tx = HeapTransaction::new(&mut heap);
    
    // Make some changes
    tx.set_table_field(table.clone(), Value::Number(1.0), Value::Number(42.0))?;
    tx.set_table_field(table.clone(), Value::Number(2.0), Value::Number(84.0))?;
    
    // Force a failure in the middle
    tx.set_table_field(TableHandle(Handle::new(999999, 0)), // Invalid handle
        Value::Number(3.0), Value::Number(126.0))?;
    
    // Commit should fail
    assert!(tx.commit().is_err());
    
    // Verify that no changes were applied
    assert_eq!(heap.get_table_field(table, &Value::Number(1.0))?, Value::Nil);
    assert_eq!(heap.get_table_field(table, &Value::Number(2.0))?, Value::Nil);
}
```

### 6.2 Borrow Safety Tests

Test for borrow checker violations:

```rust
#[test]
fn test_transaction_borrow_safety() {
    let mut heap = LuaHeap::new();
    let table = heap.create_table_internal();
    
    // Create a transaction
    let mut tx = HeapTransaction::new(&mut heap);
    
    // Make a change
    tx.set_table_field(table.clone(), Value::Number(1.0), Value::Number(42.0))?;
    
    // This would compile-time fail if we tried to access heap directly here
    // let value = heap.get_table_field(table, &Value::Number(1.0))?;
    
    // But we can read through the transaction
    let value = tx.read_table_field(table.clone(), &Value::Number(1.0))?;
    
    assert_eq!(value, Value::Number(42.0));
    
    // Commit changes
    tx.commit()?;
    
    // Now we can access the heap directly
    let value = heap.get_table_field(table, &Value::Number(1.0))?;
    assert_eq!(value, Value::Number(42.0));
}
```

## 7. Conclusion

Following these transaction patterns rigorously will ensure that the Lua VM implementation works harmoniously with Rust's ownership model and borrow checker. The key is absolute consistency in applying these patterns throughout the codebase.