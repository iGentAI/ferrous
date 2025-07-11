# Lua Iterator Implementation Guide

## Overview

This document provides a comprehensive guide for implementing and understanding Lua iterators in the Ferrous Lua VM. Iterators in Lua follow a specific contract between the compiler, VM execution, and standard library functions that must be precisely coordinated.

## 1. Iterator Fundamentals

### 1.1 Generic For Loop Architecture

```
for <vars> in <explist> do
  <body>
end
```

This translates to the following operations:

```
1. Evaluate <explist> to obtain iterator function, state, and control variable
2. Call iterator(state, control) to get values for <vars>
3. If first value is nil, exit loop
4. Assign values to <vars> and execute <body>
5. Update control variable and repeat from step 2
```

### 1.2 Register Layout for Iterators

```
┌───────────────────────────────────────────────────────────┐
│                 Register Window                           │
├───────┬───────┬───────┬───────┬───────┬───────┬───────────┤
│ R(A)  │ R(A+1)│ R(A+2)│ R(A+3)│ ...   │ ...   │R(A+3+C)   │
├───────┼───────┼───────┼───────┼───────┼───────┼───────────┤
│Iterator│ State │Control│ Var 1 │ Var 2 │ ...   │Storage    │
│Function│       │Var    │       │       │       │Register   │
└───────┴───────┴───────┴───────┴───────┴───────┴───────────┘
```

Key positions:
- `R(A)`: Iterator function
- `R(A+1)`: State (invariant throughout iteration)
- `R(A+2)`: Control variable (updated each iteration)
- `R(A+3)` to `R(A+3+C-1)`: Loop variables (C is the number of variables)
- `R(A+3+C)`: Storage register to preserve iterator function

## 2. Bytecode Execution Flow

### 2.1 TFORLOOP Opcode Execution

1. **Before First Iteration**:
   ```
   JMP to TFORLOOP
   ```

2. **TFORLOOP Execution**:
   ```
   1. Save iterator function to storage register (R(A+3+C))
   2. Call function: R(A)(R(A+1), R(A+2))
   3. Results go to R(A+3), R(A+4), ...
   4. Check if R(A+3) is nil:
       a. If nil: Skip JMP (exit loop)
       b. If not nil: Set R(A+2) = R(A+3) and continue
   ```

3. **Loop Body**

4. **Back to TFORLOOP**:
   ```
   JMP back to TFORLOOP
   ```

### 2.2 Critical TFORLOOP Implementation Details

```rust
// 1. Calculate storage register
let storage_reg = a + TFORLOOP_VAR_OFFSET + c;

// 2. Save iterator before calling it
self.register_windows.save_tforloop_iterator(window_idx, a, c)?;

// 3. Get iterator arguments
let state = self.register_windows.get_register(window_idx, a + 1)?.clone();
let control = self.register_windows.get_register(window_idx, a + 2)?.clone();

// 4. Call the iterator
tx.queue_operation(PendingOperation::FunctionCall {
    closure,
    args: vec![state, control],
    context: ReturnContext::ForLoop { 
        window_idx, a, c, pc: frame.pc,
        sbx, storage_reg,
    },
})?;
```

## 3. Standard Library Implementation Requirements

### 3.1 `pairs()` Implementation

The `pairs()` function MUST return:
1. The `next` function (iterator)
2. The table (state)
3. `nil` (initial control value)

```rust
fn pairs_function(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    // 1. Get table argument
    let table_val = ctx.get_arg(0)?;
    let table = match table_val {
        Value::Table(handle) => handle,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: table_val.type_name().to_string()
        })
    };
    
    // 2. Get next function
    let next_func = ctx.get_global_function("next")?;
    
    // 3. Push results: next function, table, nil
    ctx.push_result(next_func)?;
    ctx.push_result(Value::Table(table))?;
    ctx.push_result(Value::Nil)?;
    
    // 4. Return number of results (always 3)
    Ok(3)
}
```

### 3.2 `ipairs()` Implementation

The `ipairs()` function MUST return:
1. The `ipairs_iter` function (custom iterator)
2. The table (state)
3. `0` (initial control value)

```rust
fn ipairs_function(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    // 1. Get table argument
    let table_val = ctx.get_arg(0)?;
    let table = match table_val {
        Value::Table(handle) => handle,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: table_val.type_name().to_string()
        })
    };
    
    // 2. Push results: ipairs_iter, table, 0
    ctx.push_result(Value::CFunction(ipairs_iter))?;
    ctx.push_result(Value::Table(table))?;
    ctx.push_result(Value::Number(0.0))?;
    
    // 3. Return number of results (always 3)
    Ok(3)
}

fn ipairs_iter(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    // 1. Get arguments: table and index
    let table = match ctx.get_arg(0)? {
        Value::Table(handle) => handle,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: ctx.get_arg(0)?.type_name().to_string()
        })
    };
    
    let index = match ctx.get_arg(1)? {
        Value::Number(n) => n as i64,
        _ => return Err(LuaError::TypeError {
            expected: "number".to_string(),
            got: ctx.get_arg(1)?.type_name().to_string()
        })
    };
    
    // 2. Calculate next index
    let next_index = index + 1;
    
    // 3. Get value at next index
    let value = ctx.table_get(table, Value::Number(next_index as f64))?;
    
    // 4. Check if we should continue
    if value.is_nil() {
        ctx.push_result(Value::Nil)?;
        return Ok(1);
    }
    
    // 5. Push index and value
    ctx.push_result(Value::Number(next_index as f64))?;
    ctx.push_result(value)?;
    
    // 6. Return number of results
    Ok(2)
}
```

### 3.3 `next()` Implementation

The `next()` function MUST:
1. Accept a table and key (or nil for first iteration)
2. Return the next key-value pair (or nil to end iteration)

```rust
fn next_function(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    // 1. Get table argument
    let table = match ctx.get_arg(0)? {
        Value::Table(handle) => handle,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: ctx.get_arg(0)?.type_name().to_string()
        })
    };
    
    // 2. Get key argument
    let key = ctx.get_arg(1)?;
    
    // 3. Find next pair
    let next_pair = ctx.table_next(table, key.clone())?;
    
    // 4. Return results
    match next_pair {
        Some((next_key, value)) => {
            // Return key and value
            ctx.push_result(next_key)?;
            ctx.push_result(value)?;
            Ok(2)
        },
        None => {
            // End of iteration
            ctx.push_result(Value::Nil)?;
            Ok(1)
        }
    }
}
```

## 4. ForLoop Return Handler Implementation

The ForLoop return handler must correctly process the results of an iterator call:

```rust
ReturnContext::ForLoop { window_idx, a, c, pc, sbx, storage_reg } => {
    // 1. Restore iterator function from storage register
    self.register_windows.restore_tforloop_iterator(*window_idx, *a, *c)?;
    
    // 2. Check for nil result (end of iteration)
    let first_result = values.first().cloned().unwrap_or(Value::Nil);
    
    if !first_result.is_nil() {
        // 3. Set control variable (index) for next iteration
        self.register_windows.set_register(*window_idx, *a + TFORLOOP_CONTROL_OFFSET, first_result.clone())?;
        
        // 4. Set loop variables
        for (i, value) in values.iter().enumerate() {
            if i < *c {
                let target_reg = *a + TFORLOOP_VAR_OFFSET + i;
                self.register_windows.set_register(*window_idx, target_reg, value.clone())?;
            }
        }
        
        // 5. Let JMP handle the loop back
    } else {
        // 6. End loop by incrementing PC to skip JMP
        tx.increment_pc(self.current_thread)?;
    }
    
    Ok(StepResult::Continue)
}
```

## 5. Common Coordination Failures

### 5.1 Type Mismatch Failures

Most common failure: passing string instead of table to `next()`
```
ERROR: TypeError { expected: "table", got: "string" }
```

Solution:
- Validate types in C functions
- Ensure proper window-stack synchronization

### 5.2 Missing Iterator Return Values

Failure: Iterator doesn't return sufficient values
```
ERROR: Nil value in loop variable (key or index)
```

Solution:
- Ensure iterator functions return correct triplet
- Validate return values before using

### 5.3 Control Variable Corruption

Failure: Loop doesn't terminate or skips iterations
```
DEBUG: Inconsistent control variable values
```

Solution:
- Ensure control variable updates correctly
- Protect registers during iterator execution

## 6. Testing Strategy

### 6.1 Unit Testing

Test each iterator function independently:
- `next()` with various tables and keys
- `pairs()` with different table types
- `ipairs()` with arrays, sparse arrays, empty tables

### 6.2 Integration Testing

Create tests that exercise the full iteration cycle:
1. Iterator function returns triplet
2. TFORLOOP executes triplet
3. Loop variables are correctly assigned
4. Control flow works properly

### 6.3 Edge Cases

Test edge cases:
- Empty tables
- Tables with nil values
- Tables with unusual keys (numbers, booleans)
- Nested iterations

By following the implementation guidelines in this document, iterator operations will be correctly executed across the compiler, VM, and standard library, ensuring proper Lua semantics.