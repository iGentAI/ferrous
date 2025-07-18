# RefCellVM Implementation Guidelines

## Overview

This document provides practical guidelines for implementing Lua features in the RefCellVM architecture. It includes best practices, common patterns, and solutions to frequent challenges when working with the RefCellVM's interior mutability pattern.

## Core Principles

When implementing features for the RefCellVM, follow these core principles:

1. **Safety First**: Always validate handles before use and properly handle borrow errors
2. **Direct Access**: Use direct heap access via RefCell without creating unnecessary abstractions
3. **Two-Phase Borrowing**: Separate data collection from data modification to avoid borrow conflicts
4. **Non-Recursive Execution**: Use the operation queue for operations that would otherwise be recursive
5. **Explicit Error Handling**: Always propagate and handle errors properly with meaningful messages

## Common Implementation Patterns

### Direct Access Pattern

Use direct heap access for operations that need to modify the state:

```rust
// Access registers directly through the heap
self.heap.set_thread_register(self.current_thread, index, &value)?;
let value = self.heap.get_thread_register(self.current_thread, index)?;
```

This pattern is crucial for operations like FOR loops where values need to persist across opcode executions.

### Two-Phase Borrow Pattern

For complex operations that might cause borrow checker conflicts:

```rust
// Phase 1: Extract all needed data with immutable borrows
let data_needed = {
    let immutable_borrow = self.heap.get_something(handle)?;
    immutable_borrow.data.clone()
}; // Borrow is dropped here

// Phase 2: Now perform operations with the extracted data
self.heap.do_something_with(data_needed)?;
```

This pattern is vital when you need to access multiple RefCell-wrapped objects or the same object multiple times.

### Operation Queueing Pattern

For operations that would normally be recursive:

```rust
// Queue the operation instead of executing directly
self.operation_queue.push_back(PendingOperation::FunctionCall {
    func_index,
    nargs,
    expected_results,
});

// In the main loop, process pending operations first
while let Some(op) = self.operation_queue.pop_front() {
    self.process_pending_operation(op)?;
}
```

This prevents stack overflow in complex scenarios like nested function calls.

### Handle Validation Pattern

Always validate handles before using them:

```rust
// Check if handle is valid before using it
if !self.heap.validate_handle(handle)? {
    return Err(LuaError::InvalidHandle);
}

// Now it's safe to use
let object = self.heap.get_object(handle)?;
```

This prevents use-after-free and other memory safety issues.

### String Comparison Pattern

Compare strings by content, not by handle identity:

```rust
// For string comparison operations
match (left, right) {
    (Value::String(a), Value::String(b)) => {
        let str_a = self.heap.get_string_value(*a)?;
        let str_b = self.heap.get_string_value(*b)?;
        Ok(str_a < str_b) // Compare content, not handles
    }
    // Other cases...
}
```

This ensures proper Lua semantics for string comparison.

## Implementing Specific Features

### Implementing Opcodes

Follow this template for implementing new opcodes:

```rust
fn op_example(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
    let a = inst.get_a() as usize;
    let b = inst.get_b() as usize;
    let c = inst.get_c() as usize;
    
    // Read inputs directly from heap
    let b_value = self.heap.get_thread_register(self.current_thread, base as usize + b)?;
    let c_value = self.heap.get_thread_register(self.current_thread, base as usize + c)?;
    
    // Process values
    let result = compute_result(b_value, c_value)?;
    
    // Write result directly to heap
    self.heap.set_thread_register(self.current_thread, base as usize + a, &result)?;
    
    Ok(())
}
```

### Implementing C Functions

Follow this template for implementing new standard library functions:

```rust
pub fn example_function(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    // Validate arguments
    if ctx.arg_count() < 1 {
        return Err(LuaError::BadArgument {
            func: Some("example".to_string()),
            arg: 1,
            msg: "at least 1 argument expected".to_string()
        });
    }
    
    // Extract all arguments first
    let arg1 = ctx.get_arg(0)?;
    
    // Process arguments
    let result = process_arg(arg1)?;
    
    // Push results
    ctx.push_result(result)?;
    
    // Return number of results
    Ok(1)
}
```

### Implementing Table Operations

For table operations, use the two-phase pattern to avoid borrow conflicts:

```rust
fn get_table_field_with_metamethods(&mut self, table: TableHandle, key: &Value) -> LuaResult<Value> {
    // Phase 1: Try direct lookup
    let direct_result = self.heap.get_table_field(table, key)?;
    
    if !direct_result.is_nil() {
        return Ok(direct_result);
    }
    
    // Phase 2: Check for metatable
    let metatable_opt = {
        self.heap.get_table_metatable(table)?
    };
    
    // Phase 3: Apply metamethod if found
    if let Some(metatable) = metatable_opt {
        let index_key = self.heap.create_string("__index")?;
        let metamethod = self.heap.get_table_field(metatable, &Value::String(index_key))?;
        
        match metamethod {
            // Handle different metamethod types...
        }
    }
    
    // Default case
    Ok(Value::Nil)
}
```

## Common Challenges and Solutions

### Challenge 1: Multiple Mutable Borrows

**Problem**: Operations that need multiple mutable accesses to the same RefCell.

**Solution**: Use the two-phase borrow pattern, splitting the operation into data collection phase and mutation phase.

Example:

```rust
// Instead of:
let mut table = self.heap.get_table_mut(table_handle)?; // First mutable borrow
let mut field = self.heap.get_table_mut(field_handle)?; // Error: already borrowed mutably

// Do this:
let field_info = {
    let table = self.heap.get_table(table_handle)?; // Immutable borrow
    (table.some_field.clone(), table.other_data.clone())
}; // Borrow dropped here

// Now do the second operation
let mut field = self.heap.get_table_mut(field_handle)?; // Works now
```

### Challenge 2: Complex Value Transformations

**Problem**: Operations that modify multiple values or require temporary transformations.

**Solution**: Collect all data first, perform transformations, then apply all changes.

Example:

```rust
// Collect values first
let values: Vec<_> = (start..=end).map(|i| {
    let key = Value::Number(i as f64);
    self.heap.get_table_field(table, &key)
}).collect::<Result<Vec<_>, _>>()?;

// Process values
let result = process_values(values)?;

// Apply changes
self.heap.set_table_field(table, &key, &result)?;
```

### Challenge 3: Recursive Operations

**Problem**: Operations that would naturally be recursive, like walking a tree structure.

**Solution**: Convert to an iterative approach using an explicit stack or the operation queue.

Example:

```rust
// Instead of recursive traversal:
fn recursive_traverse(node) {
    // Process node
    recursive_traverse(node.left);
    recursive_traverse(node.right);
}

// Use iterative with explicit stack:
fn iterative_traverse(root) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        // Process node
        stack.push(node.right);
        stack.push(node.left);
    }
}
```

## Testing Guidelines

When implementing new features, follow these testing guidelines:

1. **Unit Test First**: Write unit tests for individual components
2. **Integration Test Second**: Write integration tests for component interactions
3. **Functional Test Last**: Write Lua script tests for end-to-end behavior

For each feature, aim for these test coverage targets:

- **Simple Cases**: 100% coverage
- **Edge Cases**: At least 3 tests per operation
- **Error Handling**: At least 1 test per error condition

## Documentation Guidelines

When implementing new features, update these documentation areas:

1. **Implementation Status**: Update current status in `LUA_CURRENT_IMPLEMENTATION_STATUS.md`
2. **Roadmap**: Update the roadmap in `ROADMAP.md`
3. **Code Comments**: Add clear explanatory comments for complex operations

## Common Pitfalls to Avoid

1. **Direct Mutable Access**: Never take direct `&mut` references to heap objects, always go through the proper RefCell methods.

2. **Nested Borrows**: Avoid nested borrows of the same RefCell, as this causes runtime panics.

3. **Cloning Handles**: Remember to clone handles before using them in multiple operations, as handles are `Copy` but not passed by reference.

4. **Forgetting Validation**: Always validate handles before use, as invalid handles can cause subtle bugs.

5. **Infinite Recursion**: Watch out for metamethod recursion or circular dependencies.

## Performance Considerations

While correctness is the primary goal, keep these performance considerations in mind:

1. **Minimize Cloning**: Only clone values when necessary
2. **Cache String Handles**: Cache frequently used string handles (like metamethod names)
3. **Avoid Redundant Lookups**: Store and reuse intermediate results when possible

## Next Steps for Implementation

Based on the current implementation status, focus on these areas:

1. **Function Implementation**: Complete function definition and call mechanics
2. **Closure and Upvalue Handling**: Implement proper upvalue capture and access
3. **Generic FOR Loop Support**: Implement the pairs() and ipairs() functions
4. **Metamethod Support**: Complete the metamethod handling system

## Conclusion

These guidelines provide a foundation for implementing features in the RefCellVM architecture. By following these patterns and best practices, you can ensure your implementations are correct, maintainable, and consistent with the overall design philosophy of the RefCellVM.

Always prioritize correctness over performance, and maintainability over cleverness. Remember that the primary goal of the RefCellVM is to provide a reliable, memory-safe implementation of Lua 5.1 for Redis scripting, and all implementation decisions should support that goal.