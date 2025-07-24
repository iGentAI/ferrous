# RC RefCell VM Implementation Patterns

## Overview

This document provides definitive implementation patterns for the Ferrous RC RefCell Lua VM. All patterns shown here are taken from the actual working implementation and verified to be correct.

## Core Architectural Patterns

### 1. Register Access Pattern

**CORRECT Pattern** - Always use VM register methods:

```rust
// From rc_vm.rs - the ONLY correct way to access registers
fn op_move(&self, inst: Instruction, base: usize) -> LuaResult<()> {
    let a = inst.get_a() as usize;
    let b = inst.get_b() as usize;
    
    let value = self.get_register(base + b)?;
    self.set_register(base + a, value)?;
    
    Ok(())
}
```

**ANTI-PATTERN** - Never access stack directly:
```rust
// WRONG - Don't do this
let thread_ref = self.current_thread.borrow();
let value = thread_ref.stack[index]; // WRONG - bypasses bounds checking
```

### 2. RC RefCell Access Pattern

**CORRECT Pattern** - Use two-phase borrowing:

```rust
// From rc_heap.rs - correct way to access RC RefCell objects
pub fn get_table_field(&self, table: &TableHandle, key: &Value) -> LuaResult<Value> {
    // Phase 1: Try direct access
    let table_ref = table.borrow();
    
    if let Some(value) = table_ref.get_field(key) {
        return Ok(value);
    }
    
    // Phase 2: Check metatable (extract handle first)
    let metatable_opt = if let Some(ref metatable) = table_ref.metatable {
        Some(Rc::clone(metatable))
    } else {
        None
    };
    
    // Release table borrow before accessing metatable
    drop(table_ref);
    
    // Phase 3: Access metatable independently
    if let Some(metatable) = metatable_opt {
        let mt_ref = metatable.borrow();
        // ... access metatable ...
    }
    
    Ok(Value::Nil)
}
```

### 3. Direct Execution Pattern (**UPDATED ARCHITECTURE**)

**CORRECT Pattern** - Execute operations immediately instead of queueing:

```rust
// From rc_vm.rs - correct direct execution pattern
fn op_call(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
    let func = self.get_register(base + a)?;
    let args = collect_arguments(/* ... */)?;
    
    // CORRECT: Execute function directly
    self.execute_function_call(func, args, expected_results, result_base, false, None)
}
```

**ANTI-PATTERN** - Never queue operations for later processing:
```rust
// WRONG - Queue-based execution has been eliminated
fn op_call(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
    let func = self.get_register(base + a)?;
    // WRONG - Queueing eliminated to prevent temporal state separation
    self.operation_queue.push_back(PendingOperation::FunctionCall { ... });
    Ok(())
}
```

### 4. Upvalue Sharing Pattern

**CORRECT Pattern** - Use find_or_create_upvalue for proper sharing:

```rust
// From rc_vm.rs - correct upvalue creation
fn op_closure(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
    // ... get function prototype ...
    
    let mut upvalues = Vec::with_capacity(num_upvalues);
    
    for (i, upvalue_info) in proto_value.upvalues.iter().enumerate() {
        let upvalue = match pseudo.get_opcode() {
            OpCode::Move => {
                // Local variable - CORRECT: use find_or_create for sharing
                let idx = pseudo.get_b() as usize;
                self.heap.find_or_create_upvalue(&self.current_thread, base + idx)?
            },
            OpCode::GetUpval => {
                // Parent upvalue - CORRECT: share existing upvalue
                let idx = pseudo.get_b() as usize;
                Rc::clone(&closure_ref.upvalues[idx])
            },
            _ => return Err(/* ... */),
        };
        
        upvalues.push(upvalue);
    }
    
    // Create closure with shared upvalues
    let new_closure = self.heap.create_closure(proto_value, upvalues);
    self.set_register(base + a, Value::Closure(new_closure))?;
    
    Ok(())
}
```

### 5. Error Handling Pattern

**CORRECT Pattern** - Handle all error cases:

```rust
// From rc_vm.rs - correct error handling
fn op_add(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
    let left = self.get_register(base + b_idx)?;  // Can fail
    let right = self.get_register(base + c_idx)?; // Can fail
    
    let result = match (&left, &right) {
        (Value::Number(l), Value::Number(r)) => {
            Ok(Value::Number(l + r))
        },
        (Value::Nil, _) | (_, Value::Nil) => {
            // CORRECT: Try metamethods first
            if let Some(mm) = self.find_metamethod(&left, &add_key)? {
                return self.execute_metamethod_call(mm, vec![left, right]);
            }
            
            // CORRECT: Provide specific error message
            Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: "nil".to_string(),
            })
        },
        _ => {
            // CORRECT: Try metamethods before giving up
            // ... metamethod handling ...
        }
    }?;
    
    self.set_register(base + a, result)?; // Can fail
    
    Ok(())
}
```

### 6. String Creation Pattern

**CORRECT Pattern** - Use heap string creation for interning:

```rust
// From rc_stdlib.rs - correct string creation
fn base_tostring(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let value = ctx.get_arg(0)?;
    
    let string_text = match &value {
        Value::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 1e14 {
                format!("{:.0}", n)
            } else {
                format!("{}", n)
            }
        },
        Value::Boolean(b) => b.to_string(),
        _ => format!("{}", value),
    };
    
    // CORRECT: Use heap method for string interning
    let string_handle = ctx.create_string(&string_text)?;
    ctx.push_result(Value::String(string_handle))?;
    
    Ok(1)
}
```

### 7. Table Access Pattern

**CORRECT Pattern** - Handle metamethods properly:

```rust
// From rc_vm.rs - correct table access
fn op_gettable(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
    let table_val = self.get_register(base + b)?;
    
    let table_handle = match table_val {
        Value::Table(ref handle) => handle,
        _ => {
            return Err(LuaError::TypeError {
                expected: "table".to_string(),
                got: table_val.type_name().to_string(),
            });
        }
    };
    
    let key = self.read_rk(base, c_rk)?;
    
    // CORRECT: Get value with metamethod support
    let value = self.heap.get_table_field(table_handle, &key)?;
    
    // CORRECT: Handle metamethod results
    let final_value = match value {
        Value::PendingMetamethod(boxed_mm) => {
            // Execute metamethod call directly
            return self.execute_metamethod_call(*boxed_mm, 
                vec![Value::Table(Rc::clone(table_handle)), key.clone()]);
        },
        other => other,
    };
    
    self.set_register(base + a, final_value)?;
    
    Ok(())
}
```

### 8. FOR Loop Implementation Pattern (CRITICAL)

**CORRECT Pattern** - Exactly follow Lua 5.1 specification:

```rust
// From rc_vm.rs - CORRECT FOR loop implementation
fn op_forprep(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
    let a = inst.get_a() as usize;
    let sbx = inst.get_sbx();
    
    // CRITICAL: Handle nil step value
    let step = self.get_register(base + a + 2)?;
    let step_num = match step {
        Value::Number(n) => n,
        Value::Nil => {
            // CRITICAL: Set default step immediately
            self.set_register(base + a + 2, Value::Number(1.0))?;
            1.0
        },
        _ => return Err(LuaError::TypeError { /* ... */ }),
    };
    
    // CRITICAL: Validate step != 0
    if step_num == 0.0 {
        return Err(LuaError::RuntimeError("For loop step cannot be zero".to_string()));
    }
    
    // CRITICAL: Subtract step from initial value (Lua 5.1 specification)
    let initial = self.get_register(base + a)?;
    let initial_num = initial.to_number().ok_or_else(/* ... */)?;
    let prepared = initial_num - step_num;
    self.set_register(base + a, Value::Number(prepared))?;
    
    // CRITICAL: ALWAYS jump to FORLOOP (Lua 5.1 specification)
    let pc = self.heap.get_pc(&self.current_thread)?;
    let new_pc = (pc as isize + sbx as isize) as usize;
    self.heap.set_pc(&self.current_thread, new_pc)?;
    
    Ok(())
}
```

## Testing Patterns

### Unit Test Pattern

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_register_operations() -> LuaResult<()> {
        let vm = RcVM::new()?;
        
        // Test basic register access
        vm.set_register(0, Value::Number(42.0))?;
        let value = vm.get_register(0)?;
        
        assert_eq!(value, Value::Number(42.0));
        
        Ok(())
    }
}
```

### Integration Test Pattern

```rust
#[test]
fn test_lua_script_execution() -> LuaResult<()> {
    let mut vm = RcVM::new()?;
    vm.init_stdlib()?;
    
    let script = "return 2 + 3 * 4";
    let module = compile(script)?;
    let result = vm.execute_module(&module, &[])?;
    
    assert_eq!(result, Value::Number(14.0));
    
    Ok(())
}
```

## Anti-Patterns to Avoid

### ❌ Direct Stack Access
```rust
// WRONG - Never access stack directly  
let thread = self.current_thread.borrow();
let value = thread.stack[index]; // DANGEROUS - no bounds checking
```

### ❌ Queue-Based Operations
```rust
// WRONG - Never queue operations for later processing
fn execute_func(&mut self, func: Value) -> LuaResult<Value> {
    self.operation_queue.push_back(operation); // WRONG - causes temporal state separation
}
```

### ❌ Multiple RC RefCell Borrows
```rust
// WRONG - Don't hold multiple borrows
let table_ref = table.borrow_mut();
let meta_ref = table_ref.metatable.borrow(); // WRONG - panic at runtime
```

### ❌ Ignoring Error Cases
```rust
// WRONG - Always handle errors
let value = self.get_register(index).unwrap(); // WRONG - can panic
```

## Best Practices

1. **Always use VM register methods** - Never access stack directly
2. **Use two-phase borrowing** - Extract handles before making new borrows
3. **Execute operations immediately** - Maintain direct execution principles
4. **Handle all error cases** - Provide meaningful error messages
5. **Follow Lua 5.1 specification exactly** - Especially for FOR loops and function calls
6. **Test with actual Lua scripts** - Verify behavior against known results

This document provides the definitive patterns for the direct execution VM implementation. Following these patterns ensures correct Lua 5.1 behavior and prevents temporal state separation issues.