# Handle Validation Guide for Lua VM

This document provides detailed guidelines for implementing proper handle validation in the Ferrous Lua VM. Correct handle validation is critical for memory safety, preventing use-after-free bugs, and ensuring the VM operates correctly.

## 1. Handle Validation Fundamentals

### 1.1 What is a Handle?

In the Ferrous Lua VM, a handle is a reference to an object stored in the memory arena. Handles have three key components:

```rust
pub struct Handle<T> {
    index: u32,          // Position in the arena
    generation: u32,     // Generation counter to detect stale handles
    _phantom: PhantomData<T>, // Type parameter for compile-time safety
}
```

### 1.2 Why Validate Handles?

Handles can become invalid for several reasons:
1. The object may have been freed and the arena slot reused
2. The handle might be from a different VM instance
3. The handle might reference an object of the wrong type
4. The handle might have been corrupted or fabricated

### 1.3 When Validation Occurs

Handle validation must occur at specific points in the system:

| Point | Description | Example |
|-------|-------------|---------|
| **Transaction Entry** | When handles enter a transaction | `tx.read_table_field(handle, key)` |
| **Memory Reallocation** | Before operations that might reallocate memory | `tx.push_array_element(handle)` |
| **External Source** | When handles come from external code | C function arguments |
| **Persistence** | When loading handles from saved state | Loading from files/DB |

## 2. Validation Implementation

### 2.1 Basic Validation Method

The core validation method verifies that:
1. The handle's index is within bounds
2. The generation matches the current entry
3. The entry is occupied, not free

```rust
impl LuaHeap {
    pub fn validate_handle<T>(&self, handle: &Handle<T>) -> Result<()> {
        // Check if index is in range
        if handle.index as usize >= self.arena_size_for_type::<T>() {
            return Err(LuaError::InvalidHandle);
        }
        
        // Get the entry and check generation
        // Implementation depends on type
        let stored_generation = self.get_generation_for_handle::<T>(handle.index)?;
        
        if stored_generation != handle.generation {
            return Err(LuaError::StaleHandle);
        }
        
        Ok(())
    }
}
```

### 2.2 Type-Specific Validation

Different types may require different validation approaches:

```rust
impl LuaHeap {
    fn validate_string_handle(&self, handle: &StringHandle) -> Result<()> {
        // Check if in string arena
        if handle.0.index as usize >= self.strings.len() {
            return Err(LuaError::InvalidHandle);
        }
        
        // Check generation
        match &self.strings[handle.0.index as usize] {
            Entry::Occupied { generation, .. } => {
                if *generation != handle.0.generation {
                    return Err(LuaError::StaleHandle);
                }
            },
            Entry::Free { .. } => {
                return Err(LuaError::InvalidHandle);
            },
        }
        
        Ok(())
    }
    
    // Similar methods for tables, closures, etc.
}
```

### 2.3 Transaction Validation Caching

To avoid validating the same handle multiple times in a transaction, implement validation caching:

```rust
impl<'a> HeapTransaction<'a> {
    // Validation cache types
    type ValidatedHandleId = (std::any::TypeId, u32, u32); // (type, index, generation)
    
    // Validate a handle with caching
    pub fn validate_handle<T: 'static>(&mut self, handle: &Handle<T>) -> Result<()> {
        // Create cache key
        let key = (std::any::TypeId::of::<T>(), handle.index, handle.generation);
        
        // Check cache first
        if self.validation_cache.contains(&key) {
            return Ok(());
        }
        
        // Validate through heap
        self.heap.validate_handle(handle)?;
        
        // Cache validation result
        self.validation_cache.insert(key);
        
        Ok(())
    }
}
```

### 2.4 Automatic Validation in Transaction Methods

Every transaction method that takes a handle should validate it:

```rust
impl<'a> HeapTransaction<'a> {
    pub fn read_table_field(&mut self, table: TableHandle, key: &Value) -> Result<Value> {
        // Validate handle
        self.validate_handle(&table.0)?;
        
        // Proceed with operation
        // ... implementation ...
    }
    
    pub fn set_table_field(&mut self, table: TableHandle, key: Value, value: Value) -> Result<()> {
        // Validate handle
        self.validate_handle(&table.0)?;
        
        // Queue change
        self.changes.push(HeapChange::SetTableField { table, key, value });
        
        Ok(())
    }
}
```

## 3. Handle Category Rules

Different handle sources have different validation requirements:

### 3.1 Transaction-Created Handles

Handles created within the current transaction don't need validation:

```rust
impl<'a> HeapTransaction<'a> {
    pub fn create_string(&mut self, s: &str) -> Result<StringHandle> {
        // Create in heap
        let handle = self.heap.create_string_internal(s)?;
        
        // Add to created list - these don't need validation
        self.created_handles.push(HandleKey::from(&handle.0));
        
        Ok(handle)
    }
}
```

### 3.2 VM State Handles

Handles from VM state (like current thread) need validation once per transaction:

```rust
impl<'a> HeapTransaction<'a> {
    pub fn read_register(&mut self, thread: ThreadHandle, index: usize) -> Result<Value> {
        // Validate VM state handle once per transaction
        self.validate_handle(&thread.0)?;
        
        // Proceed with operation
        // ... implementation ...
    }
}
```

### 3.3 External Handles

Handles from external sources (like C functions) need validation on every use:

```rust
impl CExecutionContext<'_> {
    pub fn get_table_field(&mut self, table: TableHandle, key: Value) -> Result<Value> {
        // Always validate external handles
        self.with_transaction(|tx| {
            // Validate inside the transaction
            tx.validate_handle(&table.0)?;
            
            // Proceed with operation
            tx.read_table_field(table, &key)
        })
    }
}
```

### 3.4 Cached Handles

Handles from caches need validation on retrieval:

```rust
impl LuaHeap {
    pub fn get_cached_string(&mut self, key: &str) -> Result<StringHandle> {
        if let Some(handle) = self.string_cache.get(key) {
            // Validate cached handle
            self.validate_handle(&handle.0)?;
            Ok(*handle)
        } else {
            // Create new
            // ... implementation ...
        }
    }
}
```

## 4. Implementation Patterns

### 4.1 Validate-Before-Use Pattern

Always validate handles before using them:

```rust
fn process_table(&mut self, table: TableHandle) -> Result<()> {
    let mut tx = HeapTransaction::new(&mut self.heap);
    
    // Validate first
    tx.validate_handle(&table.0)?;
    
    // Then use
    let value = tx.read_table_field(table, &key)?;
    
    tx.commit()?;
    Ok(())
}
```

### 4.2 Clone-After-Validation Pattern

When passing validated handles, clone after validation:

```rust
fn process_tables(&mut self, tables: &[TableHandle]) -> Result<()> {
    let mut tx = HeapTransaction::new(&mut self.heap);
    
    for table in tables {
        // Validate
        tx.validate_handle(&table.0)?;
        
        // Clone after validation
        let validated_table = table.clone();
        
        // Use cloned handle
        self.process_one_table(&mut tx, validated_table)?;
    }
    
    tx.commit()?;
    Ok(())
}
```

### 4.3 Validation Assertions

Use debug assertions for validation rules:

```rust
impl<'a> HeapTransaction<'a> {
    pub fn read_register(&mut self, thread: ThreadHandle, index: usize) -> Result<Value> {
        debug_assert!(self.state == TransactionState::Active, 
            "Transaction not active during read_register");
        
        self.validate_handle(&thread.0)?;
        
        // ... implementation ...
    }
}
```

## 5. Testing Handle Validation

### 5.1 Validation Test Cases

Write explicit tests for handle validation:

```rust
#[test]
fn test_stale_handle_detection() {
    let mut heap = LuaHeap::new();
    
    // Create a handle
    let handle = heap.create_string_internal("test")?;
    
    // Remove the string
    heap.destroy_string(handle)?;
    
    // Create a new string, reusing the slot
    let handle2 = heap.create_string_internal("new")?;
    
    // Verify the generations differ
    assert_ne!(handle.0.generation, handle2.0.generation);
    
    // Try to use the stale handle - should fail
    let mut tx = HeapTransaction::new(&mut heap);
    assert!(tx.validate_handle(&handle.0).is_err());
}
```

### 5.2 Validation Performance Testing

Test validation cache performance:

```rust
#[bench]
fn bench_validation_caching(b: &mut Bencher) {
    let mut heap = LuaHeap::new();
    let handle = heap.create_string_internal("test")?;
    
    b.iter(|| {
        let mut tx = HeapTransaction::new(&mut heap);
        
        // First validation should check against heap
        tx.validate_handle(&handle.0).unwrap();
        
        // Subsequent validations should use cache
        for _ in 0..1000 {
            tx.validate_handle(&handle.0).unwrap();
        }
        
        tx.commit().unwrap();
    })
}
```

## 6. Common Validation Pitfalls

### 6.1 Forgetting to Validate

**WRONG:**
```rust
// ❌ Missing validation
fn get_string(&mut self, handle: StringHandle) -> Result<String> {
    let mut tx = HeapTransaction::new(&mut self.heap);
    let result = tx.get_string_bytes(handle)?;
    tx.commit()?;
    String::from_utf8(result.to_vec()).map_err(|_| LuaError::InvalidEncoding)
}
```

**CORRECT:**
```rust
// ✅ With proper validation
fn get_string(&mut self, handle: StringHandle) -> Result<String> {
    let mut tx = HeapTransaction::new(&mut self.heap);
    
    // Validate handle
    tx.validate_handle(&handle.0)?;
    
    let result = tx.get_string_bytes(handle)?;
    tx.commit()?;
    String::from_utf8(result.to_vec()).map_err(|_| LuaError::InvalidEncoding)
}
```

### 6.2 Skipping Validation for "Known Good" Handles

**WRONG:**
```rust
// ❌ Dangerous assumption
fn trusted_operation(&mut self) -> Result<()> {
    let mut tx = HeapTransaction::new(&mut self.heap);
    
    // Skip validation because we "know" it's good - WRONG!
    let result = tx.read_table_field(self.globals, &key)?;
    
    tx.commit()?;
    Ok(())
}
```

**CORRECT:**
```rust
// ✅ Always validate
fn trusted_operation(&mut self) -> Result<()> {
    let mut tx = HeapTransaction::new(&mut self.heap);
    
    // Even globals need validation
    tx.validate_handle(&self.globals.0)?;
    
    let result = tx.read_table_field(self.globals, &key)?;
    
    tx.commit()?;
    Ok(())
}
```

### 6.3 Forgetting to Clone Handles

**WRONG:**
```rust
// ❌ Moving handle after validation
fn process_handle(&mut self, handle: TableHandle) -> Result<()> {
    let mut tx = HeapTransaction::new(&mut self.heap);
    
    // Validate handle
    tx.validate_handle(&handle.0)?;
    
    // Pass the handle, moving it
    self.inner_process(tx, handle)?; // handle moved here!
    
    // Can't use handle anymore
    let size = self.get_table_size(handle)?; // ERROR - use after move
    
    tx.commit()?;
    Ok(())
}
```

**CORRECT:**
```rust
// ✅ Clone after validation
fn process_handle(&mut self, handle: TableHandle) -> Result<()> {
    let mut tx = HeapTransaction::new(&mut self.heap);
    
    // Validate handle
    tx.validate_handle(&handle.0)?;
    
    // Clone before use
    let handle_copy = handle.clone();
    
    // Use clone
    self.inner_process(tx, handle_copy)?;
    
    // Can still use original
    let size = self.get_table_size(handle)?;
    
    tx.commit()?;
    Ok(())
}
```

## 7. Handle Validation Reference

### 7.1 Function Arguments

| Function Type | Validation Required |
|---------------|---------------------|
| VM API methods | Yes - at transaction start |
| Transaction methods | Yes - within method |
| Internal heap methods | No - already validated |
| C function callbacks | Yes - on every call |

### 7.2 Validation Methods

| Method | Description |
|--------|-------------|
| `heap.validate_handle(handle)` | Core validation against arena |
| `transaction.validate_handle(handle)` | Cached validation using transaction |
| `scope.validate(handle)` | Validation within scope boundaries |

## 8. Conclusion

Proper handle validation is critical to the memory safety and correctness of the Lua VM. By following these guidelines, you ensure that:

1. Invalid handles are detected before they cause corruption
2. Memory safety is preserved even with complex operations
3. The VM remains robust against misuse

Remember: When in doubt, validate. The small overhead of validation is well worth the safety it provides.