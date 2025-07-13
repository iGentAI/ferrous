# TFORLOOP Implementation Fix Guide

This document explains the critical fixes needed to align the ferrous TFORLOOP implementation with Lua 5.1 specification.

## The Problem

The current ferrous implementation diverges significantly from Lua 5.1:

1. **Split Instruction Pattern**: Uses CALL + TFORLOOP instead of single TFORLOOP
2. **Missing Loop Variable Updates**: VM only updates control variable, not loop variables
3. **Incorrect Function Check**: Has `r_a.is_function()` which doesn't match iterators
4. **Register Corruption Risk**: Iterator can be overwritten by its own results

## Lua 5.1 TFORLOOP Specification

From the official Lua 5.1 VM documentation:

```
TFORLOOP A C
R(A+3), ... ,R(A+2+C) := R(A)(R(A+1), R(A+2))
if R(A+3) ~= nil then 
    R(A+2) = R(A+3)
else 
    PC++
```

Register Layout:
- `R(A)` = Iterator function
- `R(A+1)` = State  
- `R(A+2)` = Control variable
- `R(A+3)...R(A+2+C)` = Loop variables (visible to programmer)

## Required Fixes

### 1. Compiler Fix (codegen.rs)

Replace the current for-in loop compilation:

```rust
// WRONG - Current implementation
self.emit(encode_ABC(OpCode::Call, base_reg, 3, var_count + 1));
self.emit(encode_ABC(OpCode::TForLoop, base_reg, 0, var_count));

// CORRECT - Lua 5.1 compliant
self.emit(encode_ABC(OpCode::TForLoop, base_reg, 0, var_count));
```

Key changes:
- Remove separate CALL instruction
- TFORLOOP handles both call and iteration
- Proper register allocation for iterator triplet

### 2. VM Fix (vm.rs) 

Replace the confusing TFORLOOP handler:

```rust
// WRONG - Current implementation
if r_a.is_function() {
    // Incorrect first iteration logic
} else {
    if r_a.is_nil() {
        // Only updates control, not loop vars
    }
}

// CORRECT - Lua 5.1 compliant
// 1. Call iterator: R(A)(R(A+1), R(A+2))
// 2. Store results in R(A+3)...R(A+2+C)  
// 3. If R(A+3) nil: PC++ (end loop)
// 4. Else: R(A+2) = R(A+3) (update control)
```

### 3. Register Protection

The iterator function R(A) is NEVER modified by TFORLOOP, so:
- No need for save/restore iterator
- No storage register required
- Simplifies implementation significantly

## Implementation Steps

### Step 1: Update ReturnContext

Add proper TForLoop variant:

```rust
pub enum ReturnContext {
    // ... other variants ...
    TForLoop {
        window_idx: usize,
        base: usize,      // A register  
        var_count: usize, // C operand
        pc: usize,        // Current PC
    },
}
```

### Step 2: Fix VM TFORLOOP Handler

```rust
OpCode::TForLoop => {
    // Validate bounds
    validate_tforloop_bounds(&self, window_idx, a, c)?;
    
    // Get iterator, state, control
    let iterator = self.register_windows.get_register(window_idx, a)?.clone();
    let state = self.register_windows.get_register(window_idx, a + 1)?.clone();
    let control = self.register_windows.get_register(window_idx, a + 2)?.clone();
    
    // Queue the call
    match iterator {
        Value::Closure(closure) => {
            tx.queue_operation(PendingOperation::FunctionCall {
                closure,
                args: vec![state, control],
                context: ReturnContext::TForLoop { window_idx, base: a, var_count: c, pc: frame.pc },
            })?;
        },
        Value::CFunction(cfunc) => {
            tx.queue_operation(PendingOperation::CFunctionCall {
                function: cfunc,  
                args: vec![state, control],
                context: ReturnContext::TForLoop { window_idx, base: a, var_count: c, pc: frame.pc },
            })?;
        },
        _ => return Err(TypeError),
    }
}
```

### Step 3: Fix Return Handler

```rust
ReturnContext::TForLoop { window_idx, base, var_count, pc } => {
    // Store results in R(A+3)...R(A+2+C)
    for i in 0..var_count {
        let value = results.get(i).cloned().unwrap_or(Value::Nil);
        self.register_windows.set_register(window_idx, base + 3 + i, value)?;
    }
    
    // Check termination
    let first = results.first().cloned().unwrap_or(Value::Nil);
    if first.is_nil() {
        // End loop - skip JMP
        tx.increment_pc(self.current_thread)?;
    } else {
        // Continue - update control
        self.register_windows.set_register(window_idx, base + 2, first)?;
    }
}
```

### Step 4: Fix Compiler

Update `Statement::ForInLoop` handler to emit single TFORLOOP:

```rust
// Emit jump to TFORLOOP
let jmp_to_tfor = self.current_pc();
self.emit(encode_AsBx(OpCode::Jmp, 0, 0));

// Loop body start
let body_start = self.current_pc();
self.block(body)?;

// TFORLOOP instruction  
let tfor_pc = self.current_pc();
self.emit(encode_ABC(OpCode::TForLoop, base_reg, 0, variables.len()));

// Jump back to body
let offset = body_start as i32 - self.current_pc() as i32 - 1;
self.emit(encode_AsBx(OpCode::Jmp, 0, offset));

// Patch initial jump
self.patch_jump(jmp_to_tfor, tfor_pc - jmp_to_tfor - 1);
```

## Testing the Fix

Use the test script to verify:

```lua
-- Should print k=a v=1, k=b v=2, k=c v=3
for k, v in pairs({a=1, b=2, c=3}) do
    print("k=" .. tostring(k) .. " v=" .. tostring(v))
end

-- Should print i=1 v=10, i=2 v=20, etc
for i, v in ipairs({10, 20, 30}) do
    print("i=" .. i .. " v=" .. v)  
end
```

## Common Pitfalls to Avoid

1. **Don't modify R(A)** - The iterator function must remain unchanged
2. **Results go to R(A+3)** not R(A) - Don't overwrite the iterator triplet
3. **Single instruction** - TFORLOOP does both call and check
4. **Update all loop vars** - Not just the control variable

## Verification Checklist

- [ ] Compiler emits single TFORLOOP (no CALL before it)
- [ ] VM calls iterator with state and control as args
- [ ] Results stored in R(A+3) through R(A+2+C)  
- [ ] First result (R(A+3)) used for nil check
- [ ] Control variable (R(A+2)) updated with first result
- [ ] All loop variables visible in loop body
- [ ] pairs() and ipairs() work correctly
- [ ] Custom iterators work correctly
- [ ] Nested loops work correctly

By following this guide, the TFORLOOP implementation will be fully compliant with Lua 5.1 specification.