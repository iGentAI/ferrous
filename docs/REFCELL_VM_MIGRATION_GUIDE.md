# RefCellVM Migration Guide

## Overview

This document provides a step-by-step guide for migrating the Ferrous Lua interpreter from the transaction-based VM to the RefCellVM. This migration will completely remove the transaction system and replace it with a simpler, more direct implementation using Rust's interior mutability pattern (RefCell).

## 1. Migration Strategy

The migration will follow a sequential component-by-component approach:

1. **Enhance RefCellVM**: Complete the implementation of opcodes and C function support
2. **Update Standard Library**: Migrate stdlib to use RefCellVM
3. **Update Entry Points**: Change all binaries to use RefCellVM
4. **Remove Transaction System**: Delete transaction.rs and all references to it

## 2. Dependency Analysis

The current codebase has the following dependencies on the transaction system:

| Component | Files | Dependency Type |
|-----------|-------|----------------|
| VM Core | vm.rs | Direct use of HeapTransaction |
| Standard Library | stdlib/base.rs, stdlib/math.rs, etc. | Function registration via HeapTransaction |
| Metadata | metamethod.rs | Metamethod handling with HeapTransaction |
| Compiler | compiler.rs | Module loading with HeapTransaction |
| Entry Points | compile_and_execute.rs, run_lua_tests.rs | Indirect via LuaVM |

## 3. Detailed Migration Steps

### 3.1 Complete RefCellVM Implementation

1. **Fix Method Signatures**

Update method signatures in RefCellHeap to take reference parameters where necessary:

```rust
// Before
pub fn set_table_field(&self, table: TableHandle, key: Value, value: Value) -> LuaResult<()>;

// After
pub fn set_table_field(&self, table: TableHandle, key: &Value, value: &Value) -> LuaResult<()>;
```

2. **Add Missing Methods**

Add methods to support all operations needed by the VM:

```rust
// Add string helpers
pub fn get_string_bytes(&self, handle: StringHandle) -> LuaResult<Vec<u8>>;
pub fn get_string_value(&self, handle: StringHandle) -> LuaResult<String>;

// Add upvalue helpers
pub fn set_upvalue_value(&self, upvalue: UpvalueHandle, value: &Value, thread: ThreadHandle) -> LuaResult<()>;
pub fn find_or_create_upvalue(&self, thread: ThreadHandle, stack_index: usize) -> LuaResult<UpvalueHandle>;
pub fn set_upvalue(&self, handle: UpvalueHandle, value: &Value) -> LuaResult<()>;
```

3. **Implement All Opcodes**

Ensure all opcodes are implemented:

```rust
fn op_move(&mut self, inst: Instruction, base: u16) -> LuaResult<()>;
fn op_loadk(&mut self, inst: Instruction, base: u16) -> LuaResult<()>;
// ... implement all other opcodes
```

4. **Complete the Standard Library Context**

Implement the RefCellExecutionContext fully:

```rust
pub fn get_arg(&self, index: usize) -> LuaResult<Value>;
pub fn get_arg_str(&self, index: usize) -> LuaResult<String>;
pub fn get_number_arg(&self, index: usize) -> LuaResult<f64>;
pub fn get_bool_arg(&self, index: usize) -> LuaResult<bool>;
pub fn table_next(&self, table: TableHandle, key: Value) -> LuaResult<Option<(Value, Value)>>;
// ... other helper methods
```

### 3.2 Migrate Standard Library

1. **Create refcell_stdlib.rs**

Implement a new module specifically for RefCellVM:

```rust
//! Standard Library for RefCellVM

use crate::lua::error::{LuaError, LuaResult};
use crate::lua::value::{Value, CFunction};
use crate::lua::handle::{StringHandle, TableHandle};
use crate::lua::refcell_heap::RefCellHeap;
use crate::lua::refcell_vm::{RefCellVM, RefCellExecutionContext, RefCellCFunction};

pub fn init_refcell_stdlib(vm: &mut RefCellVM) -> LuaResult<()> {
    // Implementation
}

// Standard library functions for RefCellVM
pub fn refcell_print(ctx: &mut RefCellExecutionContext) -> LuaResult<i32> {
    // Implementation
}

// ... other functions
```

2. **Update stdlib/mod.rs**

Update the standard library module to support RefCellVM:

```rust
//! Lua Standard Library Module Organization

pub mod math;
pub mod string;
pub mod base;
pub mod table;

use crate::lua::error::LuaResult;
use crate::lua::refcell_vm::RefCellVM;

/// Initialize all standard library components for RefCellVM
pub fn init_all(vm: &mut RefCellVM) -> LuaResult<()> {
    // Implementation
}

/// Backwards compatibility function
pub fn init_stdlib(vm: &mut RefCellVM) -> LuaResult<()> {
    init_all(vm)
}
```

### 3.3 Update Entry Points

1. **Update compile_and_execute.rs**

```rust
use ferrous::lua::{compile, RefCellVM, Value};

// Replace LuaVM with RefCellVM throughout
let mut vm = RefCellVM::new()?;
```

2. **Update run_lua_tests.rs**

```rust
use ferrous::lua::{RefCellVM, Value, compile};

fn run_script(script_path: &str) -> Result<Value, String> {
    // Create a VM instance
    let mut vm = match RefCellVM::new() {
        Ok(vm) => vm,
        Err(e) => return Err(format!("Failed to create VM: {:?}", e)),
    };
    
    // Continue with RefCellVM...
}
```

3. **Update other entry points**

Follow the same pattern for all binaries.

### 3.4 Update Exports in mod.rs

```rust
// Re-export commonly used types
pub use error::{LuaError, LuaResult};
pub use value::Value;
pub use refcell_vm::RefCellVM; // Instead of vm::LuaVM
```

### 3.5 Remove Transaction System

1. **Remove Imports**

Find and remove all imports of transaction.rs:

```rust
// Remove lines like:
use super::transaction::{HeapTransaction, TransactionState};
use crate::lua::transaction::HeapTransaction;
```

2. **Delete transaction.rs**

Once all dependencies are migrated, delete the file:

```bash
rm src/lua/transaction.rs
```

3. **Update mod.rs**

Remove the transaction module declaration:

```rust
// Remove:
pub mod transaction;
```

## 4. API Compatibility Issues

### 4.1 Differences in Method Signatures

| RefCellHeap Method | Transaction Method | Difference |
|-------------------|------------------|------------|
| `set_table_field(table, &key, &value)` | `set_table_field(table, key, value)` | Takes references vs. values |
| `get_globals()` | `get_globals_table()` | Different name |
| `get_string_value(handle)` | `get_string_value(handle)` | Same |
| Direct mutation via borrowing | Pending writes via transaction | Different design pattern |

### 4.2 VM Interface Changes

| RefCellVM Method | LuaVM Method | Difference |
|-----------------|-------------|------------|
| `execute(closure)` | `execute(closure)` | Same interface |
| `execute_module(module, args)` | `execute_module(module, args)` | Same interface |
| `init_stdlib()` | `init_stdlib()` | Calls different implementation |
| Direct instruction execution | Transaction-based execution | Different internal implementation |

## 5. Testing the Migration

As each component is migrated, test it with appropriate tests:

### 5.1 Basic Value Tests

```rust
#[test]
fn test_refcell_values() {
    let heap = RefCellHeap::new().unwrap();
    
    let string = heap.create_string("test").unwrap();
    let value = heap.get_string_value(string).unwrap();
    
    assert_eq!(value, "test");
}
```

### 5.2 Table Operations

```rust
#[test]
fn test_refcell_tables() {
    let heap = RefCellHeap::new().unwrap();
    
    let table = heap.create_table().unwrap();
    let key_str = heap.create_string("key").unwrap();
    
    heap.set_table_field(table, &Value::String(key_str), &Value::Number(42.0)).unwrap();
    let result = heap.get_table_field(table, &Value::String(key_str)).unwrap();
    
    assert_eq!(result, Value::Number(42.0));
}
```

### 5.3 VM Execution Tests

```rust
#[test]
fn test_refcell_vm_execution() {
    let mut vm = RefCellVM::new().unwrap();
    vm.init_stdlib().unwrap();
    
    let script = "return 1 + 1";
    let module = compile(script).unwrap();
    
    let result = vm.execute_module(&module, &[]).unwrap();
    assert_eq!(result, Value::Number(2.0));
}
```

### 5.4 For Loop Tests

```rust
#[test]
fn test_refcell_vm_for_loop() {
    let mut vm = RefCellVM::new().unwrap();
    vm.init_stdlib().unwrap();
    
    let script = "
        local sum = 0
        for i = 1, 5 do
            sum = sum + i
        end
        return sum
    ";
    
    let module = compile(script).unwrap();
    let result = vm.execute_module(&module, &[]).unwrap();
    
    assert_eq!(result, Value::Number(15.0));
}
```

## 6. Troubleshooting Common Issues

### 6.1 Borrow Checker Errors

**Issue**: Borrowing conflicts when trying to access multiple components at once.

**Solution**: Use the two-phase borrowing pattern:

```rust
// Instead of:
let table = self.heap.get_table(handle)?;
let field = self.heap.get_table_field(table.field_handle, &key)?; // Error!

// Use:
let field_handle = {
    let table = self.heap.get_table(handle)?;
    table.field_handle.clone() // Extract what you need
};
let field = self.heap.get_table_field(field_handle, &key)?; // Works!
```

### 6.2 Method Signature Mismatches

**Issue**: Calls to RefCellHeap methods fail with type errors.

**Solution**: Update all calls to use references:

```rust
// Instead of:
heap.set_table_field(table, key, value);

// Use:
heap.set_table_field(table, &key, &value);
```

### 6.3 Missing Methods

**Issue**: Some methods available in HeapTransaction are missing from RefCellHeap.

**Solution**: Add equivalent methods to RefCellHeap, or restructure the code to avoid needing them.

## 7. Completion Checklist

Use this checklist to ensure migration is complete:

- [ ] All opcodes implemented in RefCellVM
- [ ] RefCellExecutionContext fully implemented
- [ ] Standard library properly initialized
- [ ] All entry points updated
- [ ] All tests pass with RefCellVM
- [ ] No references to transaction.rs remain
- [ ] Documentation updated to reflect new architecture
- [ ] RefCellVM benchmarked against transaction-based VM

## Conclusion

By following this migration guide, you can successfully transition from the transaction-based VM to the RefCellVM. The result will be a simpler, more maintainable, and more correct Lua implementation that avoids the issues associated with the transaction-based approach, particularly the register corruption bug in for loops.