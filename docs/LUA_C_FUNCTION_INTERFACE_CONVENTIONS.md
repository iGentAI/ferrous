# Lua C Function Interface Conventions

## Overview

This document defines the conventions for interfacing between the Ferrous Lua VM's register window system and C functions (standard library and custom functions). Proper coordination on both sides of this interface is critical for correct behavior.

## 1. Core Architectural Principles

### 1.1 Register Window to Stack Mapping

```
┌────────────────┐       ┌────────────────┐       ┌────────────────┐
│ Register Window │       │ Thread Stack   │       │ C Function     │
│                │       │                │       │                │
│ R(0)  R(1)  .. │ ────▶ │ [base]  ...    │ ────▶ │ arg(0) arg(1)..│
└────────────────┘       └────────────────┘       └────────────────┘
```

The VM must ensure:
1. Register values are correctly transferred to the thread stack at stack_base + i
2. Stack positions are properly mapped to C function arguments
3. Return values from C functions are transferred back to registers

### 1.2 Type Validation Requirements

Both the VM and C functions must validate types:
- VM should validate types when pushing to the stack
- C functions should validate argument types before use
- Type errors should include context (function name, argument position)

## 2. Argument Marshaling Protocol

### 2.1 C Function Call Sequence

```rust
// Stack Position Calculation
let stack_base = tx.get_stack_size(thread_handle)?;

// Step 1: Push arguments to the stack
for (i, arg) in args.iter().enumerate() {
    tx.push_stack(thread_handle, arg.clone())?;
}

// Step 2: Create execution context that points to stack, not registers
let ctx = ExecutionContext::new(stack_base, args.len(), thread_handle);

// Step 3: Execute C function with context
let result_count = cfunc(&mut ctx)?;

// Step 4: Collect results from stack
let mut results = Vec::with_capacity(result_count as usize);
for i in 0..result_count as usize {
    results.push(tx.read_register(thread_handle, stack_base + i)?);
}

// Step 5: Clean up stack
tx.pop_stack(thread_handle, args.len().max(result_count as usize))?;
```

### 2.2 Window-Stack Synchronization Points

Synchronization must occur at these points:
1. Before pushing C function arguments to the stack
2. After collecting C function results
3. Before cleaning up the stack

### 2.3 Error Propagation

Errors from C functions must be propagated with proper context:

```rust
// Catching and enhancing C function errors
match cfunc(&mut ctx) {
    Ok(result_count) => {
        // Process results...
    },
    Err(e) => {
        // Add context to the error
        let enhanced_error = match e {
            LuaError::TypeError { expected, got } => LuaError::TypeError {
                expected,
                got: format!("{} (in function call to {})", got, function_name)
            },
            _ => e
        };
        return Err(enhanced_error);
    }
}
```

## 3. ExecutionContext Implementation

### 3.1 Argument Access Pattern

C functions use ExecutionContext to access arguments:

```rust
fn example_c_function(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    // Get first argument and ensure it's a table
    let table = match ctx.get_arg(0)? {
        Value::Table(handle) => handle,
        other => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: other.type_name().to_string()
        })
    };
    
    // Access second argument (control variable)
    let control = ctx.get_arg(1)?;
    
    // Process...
    
    // Return number of results pushed
    Ok(2)
}
```

### 3.2 Result Handling

C functions must:
1. Push results using `ctx.push_result()`
2. Return the number of results pushed (`i32`)
3. Return `0` for no results, not `None` or empty values

## 4. Iterator Function Requirements

### 4.1 `next()` Implementation

The `next()` function MUST:
1. Accept exactly 2 arguments:
   - `table`: a valid table to iterate
   - `index`: the previous key (or nil for first iteration)
2. Return the next key-value pair, or nil if iteration is complete

```rust
// Example next() implementation
fn next_function(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    // Get table argument
    let table = match ctx.get_arg(0)? {
        Value::Table(handle) => handle,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: ctx.get_arg(0)?.type_name().to_string()
        })
    };
    
    // Get index argument
    let index = ctx.get_arg(1)?;
    
    // Get next pair
    if let Some((next_key, next_value)) = ctx.table_next(table, index)? {
        // Return key, value pair
        ctx.push_result(next_key)?;
        ctx.push_result(next_value)?;
        return Ok(2);
    } else {
        // End of iteration
        ctx.push_result(Value::Nil)?;
        return Ok(1);
    }
}
```

### 4.2 `pairs()` and `ipairs()` Contract

The VM expects these functions to return a specific triplet:
1. Iterator function
2. State (usually table)
3. Initial control value (nil or 0)

```rust
// Example pairs() implementation
fn pairs_function(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    // Get table argument
    let table = match ctx.get_arg(0)? {
        Value::Table(handle) => handle,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: ctx.get_arg(0)?.type_name().to_string()
        })
    };
    
    // Get next function
    let next = match ctx.globals_get("next")? {
        Value::CFunction(f) => f,
        _ => return Err(LuaError::RuntimeError("Cannot find next function".to_string()))
    };
    
    // Return triplet: next, table, nil
    ctx.push_result(Value::CFunction(next))?;
    ctx.push_result(Value::Table(table))?;
    ctx.push_result(Value::Nil)?;
    
    Ok(3) // Return 3 values
}
```

## 5. VM Implementation Requirements

### 5.1 Register Window to Stack Synchronization

The VM MUST synchronize registers with the thread stack:

```rust
// Proper synchronization before C function call
fn handle_c_function_call_with_windows(&mut self, func: CFunction, args: Vec<Value>, 
                                       window_idx: usize, result_register: usize, 
                                       thread_handle: ThreadHandle) -> LuaResult<StepResult> {
    // Step 1: Synchronize register window with thread stack
    sync_window_to_stack_helper(&mut tx, &self.register_windows, 
                             thread_handle, window_idx, window_size)?;
    
    // Step 2: Push arguments to stack
    // ... rest of implementation ...
}
```

### 5.2 Type Checking and Error Handling

Always validate types before operations:

```rust
// Validate table type before table operations
match value {
    Value::Table(handle) => {
        // Proceed with table operation
    },
    _ => {
        return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: value.type_name().to_string()
        });
    }
}
```

## 6. Testing and Verification

### 6.1 C Function Testing

Test each C function with several scenarios:
1. Valid arguments of expected types
2. Invalid arguments (wrong type, wrong number)
3. Edge cases (nil, empty tables, etc.)

### 6.2 Integration Testing

Verify correct operation end-to-end:
1. Lua code calls C functions
2. C functions manipulate thread stack
3. Results correctly return to Lua code

By following these conventions, the VM and C functions can properly coordinate their expectations and ensure type safety, proper error handling, and consistent behavior.