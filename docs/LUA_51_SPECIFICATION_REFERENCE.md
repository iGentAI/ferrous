# Complete Lua 5.1 Implementation Specification - All Details Required

## Overview

This document provides comprehensive implementation details for every Lua 5.1 opcode, stack management requirement, and VM semantic necessary for correct implementation. This specification addresses all critical gaps that were previously missing.

## Stack Management Requirements

### Function Entry Stack Setup
```c
// On function entry, VM must:
callee_base = result_base + 1;
frame_top = callee_base + maxstacksize;
ensure_stack_space(frame_top);

// Initialize parameters
for (i = 0; i < min(nargs, num_params); i++) {
    stack[callee_base + i] = args[i];
}
for (i = min(nargs, num_params); i < num_params; i++) {
    stack[callee_base + i] = nil;
}

// Set top based on function type
if (is_vararg) {
    top = callee_base + max(nargs, num_params);  // Keep extras for VARARG
} else {
    top = callee_base + num_params;  // Discard extras
}
```

### Register Access
```c
// R(n) maps to stack[base + n]
if (base + n >= frame_top) error("stack overflow");
if (base + n >= stack.len()) stack.resize(base + n + 1, nil);
```

## Complete Opcode Implementation Details (0-37)

### 0. MOVE (ABC Format)
**Operation**: `R(A) := R(B)`
**Register Usage**: A=destination register, B=source register, C=unused (0)
**Stack Effects**: Copies value from source to destination
**Return Values**: None (register operation)
**Implementation**:
```rust
fn op_move(base: usize, inst: Instruction) -> Result<()> {
    let a = inst.get_a() as usize;
    let b = inst.get_b() as usize;
    let value = get_register(base + b)?;
    set_register(base + a, value)?;
    Ok(())
}
```

### 1. LOADK (ABx Format)
**Operation**: `R(A) := Kst(Bx)`
**Register Usage**: A=destination register, Bx=constant table index
**Stack Effects**: Loads constant into register
**Return Values**: None (register operation)
**Error Conditions**: Bx out of bounds of constant table
**Implementation**:
```rust
fn op_loadk(base: usize, inst: Instruction) -> Result<()> {
    let a = inst.get_a() as usize;
    let bx = inst.get_bx() as usize;
    let constant = get_constant(bx)?;  // May error if bx >= constants.len()
    set_register(base + a, constant)?;
    Ok(())
}
```

### 2. LOADBOOL (ABC Format)
**Operation**: `R(A) := (Bool)B; if (C) pc++`
**Register Usage**: A=destination register, B=boolean value (0/1), C=skip flag (0/1)
**Stack Effects**: Loads boolean and conditionally skips next instruction
**Return Values**: None (register operation)
**PC Effects**: PC increment if C != 0
**Implementation**:
```rust
fn op_loadbool(base: usize, inst: Instruction) -> Result<()> {
    let a = inst.get_a() as usize;
    let b = inst.get_b();
    let c = inst.get_c();
    
    set_register(base + a, Value::Boolean(b != 0))?;
    
    if c != 0 {
        pc += 1;  // Skip next instruction
    }
    Ok(())
}
```

### 3. LOADNIL (ABC Format)
**Operation**: `R(A) := nil; R(A+1) := nil; ...; R(B) := nil`
**Register Usage**: A=start register, B=end register, C=unused
**Stack Effects**: Sets range of registers to nil
**Return Values**: None (register operation)
**Implementation**:
```rust
fn op_loadnil(base: usize, inst: Instruction) -> Result<()> {
    let a = inst.get_a() as usize;
    let b = inst.get_b() as usize;
    
    for i in a..=b {
        set_register(base + i, Value::Nil)?;
    }
    Ok(())
}
```

### 4. GETUPVAL (ABC Format)
**Operation**: `R(A) := UpValue[B]`
**Register Usage**: A=destination register, B=upvalue index, C=unused
**Stack Effects**: Loads upvalue into register
**Return Values**: None (register operation)
**Error Conditions**: B out of bounds of upvalue array
**Upvalue Handling**: Load from open (stack reference) or closed (stored value)
**Implementation**:
```rust
fn op_getupval(base: usize, inst: Instruction) -> Result<()> {
    let a = inst.get_a() as usize;
    let b = inst.get_b() as usize;
    
    let upvalue = get_upvalue(b)?;  // May error if b >= upvalues.len()
    let value = match upvalue_state {
        Open { thread, stack_index } => thread.stack[stack_index].clone(),
        Closed { value } => value.clone(),
    };
    set_register(base + a, value)?;
    Ok(())
}
```

### 5. GETGLOBAL (ABx Format)
**Operation**: `R(A) := Gbl[Kst(Bx)]` with __index metamethod if missing
**Register Usage**: A=destination register, Bx=constant table index for key
**Stack Effects**: Loads global variable into register
**Return Values**: None (register operation)
**Metamethod**: Triggers __index if key missing from environment
**Environment**: Uses closure's env field (cl->env), NOT upvalue[0] - Lua 5.1 specific
**Implementation**:
```rust
fn op_getglobal(base: usize, inst: Instruction) -> Result<()> {
    let a = inst.get_a() as usize;
    let bx = inst.get_bx() as usize;
    
    let key = get_constant(bx)?;
    let env_table = current_closure.env;  // Lua 5.1 uses cl->env field
    
    let value = get_table_field_with_metamethod(env_table, key)?;
    set_register(base + a, value)?;
    Ok(())
}
```

### 6. GETTABLE (ABC Format)
**Operation**: `R(A) := R(B)[RK(C)]` with __index metamethod if missing
**Register Usage**: A=destination register, B=table register, C=key (RK encoded)
**Stack Effects**: Loads table field into register
**Return Values**: None (register operation)
**Metamethod**: Triggers __index if key missing, calls with (table, key), expects 1 result
**RK Encoding**: C & 0x100 ? constants[C & 0xFF] : R(C)
**Implementation**:
```rust
fn op_gettable(base: usize, inst: Instruction) -> Result<StepResult> {
    let a = inst.get_a() as usize;
    let b = inst.get_b() as usize;
    let table = get_register(base + b)?;
    let key = get_rk_value(base, inst.get_c())?;
    
    let value = get_table_field_with_metamethod(table, key)?;
    
    // Handle metamethod call if needed
    if let Value::PendingMetamethod(mm) = value {
        return execute_function_call(mm, vec![table, key], 1, base + a, false, None);
    }
    
    set_register(base + a, value)?;
    Ok(StepResult::Continue)
}
```

### 7. SETGLOBAL (ABx Format)
**Operation**: `Gbl[Kst(Bx)] := R(A)` with __newindex metamethod if missing
**Register Usage**: A=source register, Bx=constant table index for key
**Stack Effects**: Sets global variable from register
**Return Values**: None (register operation)
**Metamethod**: Triggers __newindex if key missing, calls with (table, key, value)
**Environment**: Uses closure's env field (cl->env), NOT upvalue[0] - Lua 5.1 specific

### 8. SETUPVAL (ABC Format)
**Operation**: `UpValue[B] := R(A)`
**Register Usage**: A=source register, B=upvalue index, C=unused
**Stack Effects**: Sets upvalue from register
**Return Values**: None (register operation)
**Upvalue Handling**: Store to open (stack reference) or closed (stored value)

### 9. SETTABLE (ABC Format)
**Operation**: `R(A)[RK(B)] := RK(C)` with __newindex metamethod if missing
**Register Usage**: A=table register, B=key (RK), C=value (RK)
**Stack Effects**: Sets table field from register values
**Return Values**: None (register operation)
**Metamethod**: Triggers __newindex if key missing, calls with (table, key, value)

### 10. NEWTABLE (ABC Format)
**Operation**: `R(A) := {}` (create new table with size hints)
**Register Usage**: A=destination register, B=array size hint, C=hash size hint
**Stack Effects**: Creates new table and stores in register
**Return Values**: None (register operation)
**Size Hints**: B and C are fb2int encoded per Lua 5.1 source
**FB2INT Decoding**: `fb2int(x) = (e == 0) ? x : ((x & 7) + 8) << (e - 1)` where `e = (x >> 3) & 0x1f`
**Implementation**:
```rust
fn fb2int(x: u32) -> usize {
    let e = (x >> 3) & 0x1f;
    if e == 0 {
        x as usize
    } else {
        (((x & 7) + 8) << (e - 1)) as usize
    }
}

fn op_newtable(base: usize, inst: Instruction) -> Result<()> {
    let a = inst.get_a() as usize;
    let b = inst.get_b();
    let c = inst.get_c();
    
    let array_size = fb2int(b);
    let hash_size = fb2int(c);
    
    let table = create_table_with_capacity(array_size, hash_size);
    set_register(base + a, Value::Table(table))?;
    Ok(())
}
```

### 11. SELF (ABC Format)
**Operation**: `R(A+1) := R(B); R(A) := R(B)[RK(C)]` with __index
**Register Usage**: A=method dest register, B=table register, C=method name (RK)
**Stack Effects**: Copies table to A+1, gets method to A
**Return Values**: None (register operation)
**Metamethod**: Uses __index if method not found directly in table

### 12-17. Arithmetic Operations (ABC Format)
**Operations**: ADD, SUB, MUL, DIV, MOD, POW
**Operation**: `R(A) := RK(B) <op> RK(C)`
**Register Usage**: A=destination, B=left operand (RK), C=right operand (RK)
**Stack Effects**: Performs arithmetic and stores result
**Return Values**: None (register operation)
**Type Handling**: Try numeric operation first, then metamethod
**Metamethods**: __add, __sub, __mul, __div, __mod, __pow (left first, then right)
**Error Conditions**: Division/modulo by zero, non-numeric with no metamethod

### 18-20. Unary Operations (ABC Format)
**Operations**: UNM (-), NOT (not), LEN (#)
**Operation**: `R(A) := <op> R(B)`
**Register Usage**: A=destination, B=operand, C=unused
**Stack Effects**: Performs unary operation and stores result
**Metamethods**: __unm, __len (NOT has no metamethod)

### 21. CONCAT (ABC Format)
**Operation**: `R(A) := R(B) .. R(B+1) .. ... .. R(C)`
**Register Usage**: A=destination, B=start register, C=end register
**Stack Effects**: Concatenates range of values
**Return Values**: None (register operation)
**Type Handling**: String/number concat directly, else __concat metamethod pairwise
**Metamethod**: __concat called right-associative (concat(B, concat(B+1, ...)))

### 22. JMP (AsBx Format)
**Operation**: `pc += sBx` with upvalue closing
**Register Usage**: A=upvalue close level (NOT unused), sBx=signed offset
**Stack Effects**: Closes upvalues >= R(A) if A > 0, then jumps
**Return Values**: None (control flow)
**PC Effects**: Adds signed offset to program counter
**Upvalue Closing**: If A > 0, close upvalues >= base + A - 1 before jump
**Implementation**:
```rust
fn op_jmp(base: usize, inst: Instruction) -> Result<()> {
    let a = inst.get_a();
    let sbx = inst.get_sbx();
    
    // Close upvalues if A > 0
    if a > 0 {
        close_upvalues(base + a as usize - 1)?;
    }
    
    pc = (pc as isize + sbx as isize) as usize;
    Ok(())
}
```

### 23-25. Comparison Operations (ABC Format)
**Operations**: EQ, LT, LE
**Operation**: `if (RK(B) <op> RK(C)) ~= A then pc++`
**Register Usage**: A=invert flag, B=left operand (RK), C=right operand (RK)  
**Stack Effects**: None (conditional jump)
**Return Values**: None (control flow)
**PC Effects**: Skip next instruction if condition false
**Metamethods**: __eq (only if same function in both metatables), __lt, __le

### 26. TEST (ABC Format)
**Operation**: `if not (R(A) <=> C) then pc++`
**Register Usage**: A=value register, B=unused, C=test flag
**Stack Effects**: None (conditional jump)
**Return Values**: None (control flow)
**PC Effects**: Skip if testhint doesn't match value truthiness
**Test Logic**: C=0 tests for falsy (skip if truthy), C=1 tests for truthy (skip if falsy)

### 27. TESTSET (ABC Format)
**Operation**: `if (R(B) <=> C) then R(A) := R(B) else pc++`
**Register Usage**: A=destination, B=source, C=test flag
**Stack Effects**: Conditional assignment or jump
**Return Values**: None (register operation or control flow)
**PC Effects**: Skip next instruction if test fails

### 28. CALL (ABC Format) - CRITICAL FOR FUNCTION CALLS
**Operation**: `R(A), ... ,R(A+C-2) := R(A)(R(A+1), ... ,R(A+B-1))`
**Register Usage**: 
- A=function register (and result base)
- B=argument count + 1 (0 means "to top")
- C=result count + 1 (0 means "multi-value")
**Stack Effects**: 
- Function at R(A), arguments R(A+1) to R(A+B-1)
- Results placed from R(A) to R(A+C-2)
- For C=0: place all results, adjust top accordingly
**Return Values**: C-1 results (or all results if C=0)
**Multi-Value**: If C=0, return all results and adjust logical top
**Function Types**: Lua function (push frame), C function (direct call), table with __call
**Error Conditions**: R(A) not callable (function, C function, or table with __call)

### 29. TAILCALL (ABC Format)
**Operation**: Tail call - reuse current frame
**Register Usage**: A=function register, B=argument count + 1, C=unused
**Stack Effects**: Replace current frame with new call
**Return Values**: Inherits caller's expected result count
**Frame Management**: Pop current frame, use result_base for new call

### 30. RETURN (ABC Format) - CRITICAL FOR FUNCTION RETURNS
**Operation**: `return R(A), ... ,R(A+B-2)`
**Register Usage**: 
- A=first return value register
- B=return count + 1 (0 means "multi-return to top")
- C=unused
**Stack Effects**: Close upvalues >= base, collect return values, return to caller
**Return Values**: B-1 values (or all values from A to top if B=0)
**Upvalue Closing**: MUST close upvalues >= base BEFORE collecting values
**Multi-Value**: If B=0, return all values from R(A) to current top
**Frame Management**: Pop frame, place results at frame.result_base

### 31. FORPREP (AsBx Format)
**Operation**: Numeric for loop preparation
**Register Usage**: A=loop base, sBx=jump offset
**Stack Layout**: R(A)=initial, R(A+1)=limit, R(A+2)=step
**Stack Effects**: R(A) := R(A) - R(A+2), pc += sBx
**Return Values**: None (control flow)
**Type Coercion**: Convert string values to numbers

### 32. FORLOOP (AsBx Format)
**Operation**: Numeric for loop iteration
**Register Usage**: A=loop base, sBx=jump offset (negative)
**Stack Layout**: R(A)=counter, R(A+1)=limit, R(A+2)=step, R(A+3)=loop variable
**Stack Effects**: Increment counter, check condition, update loop variable if continuing
**Return Values**: None (control flow)
**Loop Logic**: If (step > 0 and counter <= limit) or (step <= 0 and counter >= limit) then continue

### 33. TFORLOOP (ABC Format) - CRITICAL FOR ITERATOR LOOPS
**Operation**: Generic for loop - calls iterator and tests result
**Register Usage**: A=iterator base, B=unused, C=number of loop variables
**Stack Layout**: R(A)=iterator function, R(A+1)=state, R(A+2)=control var
**Stack Effects**: 
1. Manual stack setup: copy func/state/ctrl to R(A+3)/R(A+4)/R(A+5), set top
2. Call iterator expecting C results placed at R(A+3) to R(A+2+C)
3. If R(A+3) ~= nil then R(A+2) := R(A+3); use sBx from NEXT instruction for back-jump
4. If R(A+3) == nil then pc += 1 to skip following JMP instruction and exit loop
**Return Values**: None (control flow)
**CRITICAL**: TFORLOOP is ALWAYS followed by JMP instruction with back-jump sBx
**PC Effects**: For continue: pc += sBx from next instruction; For end: pc += 1 to skip JMP
**Implementation**:
```rust
fn op_tforloop(base: usize, inst: Instruction) -> Result<()> {
    let a = inst.get_a() as usize;
    let c = inst.get_c() as usize;
    
    // Manual stack setup for iterator call
    let cb = base + a + 3;
    set_register(cb, get_register(base + a)?)?;      // func
    set_register(cb + 1, get_register(base + a + 1)?)?;  // state
    set_register(cb + 2, get_register(base + a + 2)?)?;  // control
    set_top(cb + 3);
    
    // Call with nresults = C
    execute_function_call(get_register(cb)?, vec![], c as i32, cb, false, None)?;
    
    // Reset top to frame limit
    set_top(frame_top);
    
    // Test first result and conditionally jump using FOLLOWING instruction
    let first_result = get_register(base + a + 3)?;
    if !first_result.is_nil() {
        set_register(base + a + 2, first_result)?;  // Update control
        let next_inst = get_instruction(pc)?;  // Get following JMP instruction
        let sbx = get_sbx(next_inst);  // Extract sBx from JMP
        pc = (pc as isize + sbx as isize) as usize;  // Back-jump using JMP's sBx
    } else {
        pc += 1;  // Skip the following JMP instruction to exit loop
    }
    
    Ok(())
}
```

### 34. SETLIST (ABC Format)
**Operation**: `R(A)[FPF*(C-1)+i] := R(A+i)` for i=1 to B
**Register Usage**: A=table register, B=element count, C=batch number
**Stack Effects**: Sets table array elements in batch
**Return Values**: None (register operation)
**Batch Size**: FPF (50) elements per batch
**Special Cases**: 
- B=0 means elements to top
- C=0 means read next instruction for C value: `c = GETARG_Ax(*(pc++))` and skip it
**Large Table Support**: For C=0, consumes extra instruction for batch number
**Implementation**:
```rust
fn op_setlist(base: usize, inst: Instruction) -> Result<()> {
    let a = inst.get_a() as usize;
    let mut b = inst.get_b() as usize;
    let mut c = inst.get_c() as usize;
    
    let table = get_register(base + a)?;
    
    if c == 0 {
        // Read C from next instruction and skip it
        c = get_instruction(pc)? as usize;  // Full instruction as Ax
        pc += 1;
    }
    
    let first_index = (c - 1) * 50;  // FPF = 50
    
    if b == 0 {
        b = current_top() - (base + a + 1);  // Elements to top
    }
    
    for i in 1..=b {
        let value = get_register(base + a + i)?;
        table_set_array_element(table, first_index + i, value)?;
    }
    
    Ok(())
}
```

### 35. CLOSE (ABC Format)
**Operation**: Close upvalues >= R(A)
**Register Usage**: A=minimum register index, B=unused, C=unused
**Stack Effects**: None (upvalue management)
**Return Values**: None (upvalue management)
**Upvalue Closing**: Move all open upvalues >= R(A) to closed state

### 36. CLOSURE (ABx Format) - CRITICAL IMPLEMENTATION MISSING BEFORE
**Operation**: `R(A) := closure(Kst(Bx))` + upvalue capture
**Register Usage**: A=destination register, Bx=function prototype constant index
**Stack Effects**: Creates new closure and stores in register
**Return Values**: None (register operation)
**CRITICAL IMPLEMENTATION DETAILS**:
1. Load function prototype from constants[Bx]
2. Create new closure from prototype  
3. Process upvalue capture via pseudo-instructions
4. **Store closure in R(A)** - THIS WAS MISSING
5. Advance PC by number of upvalues processed

**Pseudo-Instruction Processing**:
```rust
// CRITICAL: After CLOSURE instruction, process upvalue pseudo-instructions
for i in 0..proto.num_upvalues {
    let pseudo_inst = get_instruction(pc + i);
    
    if pseudo_inst.opcode == MOVE {
        // Capture local variable from R(B)
        let local_idx = base + pseudo_inst.get_b();
        upvalue = find_or_create_upvalue(thread, local_idx);
    } else if pseudo_inst.opcode == GETUPVAL {
        // Capture parent upvalue from current closure
        let parent_upval_idx = pseudo_inst.get_b();
        upvalue = current_closure.upvalues[parent_upval_idx];
    }
    
    new_closure.upvalues[i] = upvalue;
}

// CRITICAL: Store completed closure in destination register
set_register(base + a, Value::Closure(new_closure))?;

// CRITICAL: Skip all processed pseudo-instructions
pc += proto.num_upvalues;
```

**THIS IS THE ROOT CAUSE OF "EXPECTED FUNCTION, GOT NIL"** - The CLOSURE implementation was not properly storing the created function in the register, making it inaccessible to subsequent CALL instructions.

### 37. VARARG (ABC Format)
**Operation**: `R(A), R(A+1), ..., R(A+B-2) := ...` (copy varargs)
**Register Usage**: A=destination base, B=count + 1 (0 means all), C=unused
**Stack Effects**: Copies vararg values to consecutive registers
**Return Values**: None (register operation)
**Vararg Source**: Computed as arguments beyond num_params
**Multi-Copy**: If B=0, copy all varargs and adjust top

## Critical Implementation Notes

### Function Call/Return Mechanics
1. **CALL setup**: Function at result_base, args at result_base+1, callee execution at result_base+1
2. **Result placement**: Always at original result_base location
3. **Multi-value handling**: C=0 in CALL means all results, adjust top accordingly

### Upvalue Management
1. **Sharing**: Multiple closures sharing same local variable share same upvalue object
2. **Closing**: Done on scope exit (CLOSE) and function return (RETURN)
3. **State transition**: Open (stack reference) to Closed (stored value)

### Stack Management
1. **Frame limits**: Fixed frame_top = callee_base + maxstacksize
2. **Dynamic growth**: Within frame limits with nil initialization
3. **Bounds checking**: Error on access beyond frame_top

## Error Handling
- Stack overflow on access beyond frame_top
- Type errors for invalid operations
- Bounds errors for constant/upvalue access
- Division by zero for arithmetic operations

This complete specification provides all implementation details necessary for proper Lua 5.1 VM functionality, with particular focus on the CLOSURE opcode semantics that were previously missing and causing systematic function call failures.