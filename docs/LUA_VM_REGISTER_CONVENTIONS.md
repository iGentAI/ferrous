# Lua VM Register Usage Conventions

## Overview

This document provides a systematic reference for register usage patterns across all opcodes in the Ferrous Lua VM implementation. Each opcode's register requirements, side effects, and special considerations are documented to ensure alignment between the VM execution engine, bytecode compiler, and standard library functions.

## Register Encoding Conventions

- **R(n)**: Register at index n
- **RK(n)**: Register or constant - if n >= 256, then constant at index (n & 0xFF)
- **Kst(n)**: Constant at index n
- **UpValue[n]**: Upvalue at index n

## Register Window System

The Ferrous Lua VM uses a register window system where each function call gets its own isolated window:

- Window index is stored in `CallFrame.base_register`
- Stack position calculation: `window_idx * 256 + register`
- Windows are allocated/deallocated on function entry/exit
- Each window has a maximum size, typically 256 registers

## Core Register Usage Principles

1. **Ownership**: Each register has a clear "owner" (opcode or block) at any point in execution
2. **Preservation**: Registers that need to survive nested operations must be explicitly preserved
3. **Isolation**: Different function calls cannot access each other's registers
4. **Lifecycle**: Registers have a defined lifespan from allocation to final use

## Opcode Register Usage Patterns

### MOVE (0)
**Format**: `MOVE A B`  
**Operation**: `R(A) := R(B)`  
**Input Registers**: 
- R(B): Source value

**Output Registers**: 
- R(A): Destination (copy of R(B))

**Special Considerations**: Simple register copy, no side effects

### LOADK (1)
**Format**: `LOADK A Bx`  
**Operation**: `R(A) := Kst(Bx)`  
**Input Registers**: None

**Output Registers**: 
- R(A): Loaded constant value

**Special Considerations**: Bx indexes into function's constant table

### LOADBOOL (2)
**Format**: `LOADBOOL A B C`  
**Operation**: `R(A) := (Bool)B; if (C) pc++`  
**Input Registers**: None

**Output Registers**: 
- R(A): Boolean value (true if B≠0, false if B=0)

**Special Considerations**: C controls skip of next instruction

### LOADNIL (3)
**Format**: `LOADNIL A B`  
**Operation**: `R(A), R(A+1), ..., R(A+B-1) := nil`  
**Input Registers**: None

**Output Registers**: 
- R(A) through R(A+B-1): All set to nil

**Special Considerations**: B specifies total number of registers to nil

### GETUPVAL (4)
**Format**: `GETUPVAL A B`  
**Operation**: `R(A) := UpValue[B]`  
**Input Registers**: None (reads from upvalue)

**Output Registers**: 
- R(A): Value from upvalue

**Special Considerations**: May read from open (stack) or closed upvalue

### GETGLOBAL (5)
**Format**: `GETGLOBAL A Bx`  
**Operation**: `R(A) := Gbl[Kst(Bx)]`  
**Input Registers**: None

**Output Registers**: 
- R(A): Global variable value

**Special Considerations**: Bx is constant index containing global name

### SETGLOBAL (6)
**Format**: `SETGLOBAL A Bx`  
**Operation**: `Gbl[Kst(Bx)] := R(A)`  
**Input Registers**: 
- R(A): Value to store

**Output Registers**: None

**Special Considerations**: Modifies global environment

### SETUPVAL (7)
**Format**: `SETUPVAL A B`  
**Operation**: `UpValue[B] := R(A)`  
**Input Registers**: 
- R(A): Value to store

**Output Registers**: None

**Special Considerations**: Updates upvalue (open or closed)

### GETTABLE (8)
**Format**: `GETTABLE A B C`  
**Operation**: `R(A) := R(B)[RK(C)]`  
**Input Registers**: 
- R(B): Table
- RK(C): Key (register or constant)

**Output Registers**: 
- R(A): Retrieved value

**Special Considerations**: May trigger __index metamethod

### SETTABLE (9)
**Format**: `SETTABLE A B C`  
**Operation**: `R(A)[RK(B)] := RK(C)`  
**Input Registers**: 
- R(A): Table
- RK(B): Key
- RK(C): Value

**Output Registers**: None

**Special Considerations**: May trigger __newindex metamethod

### NEWTABLE (10)
**Format**: `NEWTABLE A B C`  
**Operation**: `R(A) := {} (size = B,C)`  
**Input Registers**: None

**Output Registers**: 
- R(A): New empty table

**Special Considerations**: B and C are size hints (array/hash)

### SELF (11)
**Format**: `SELF A B C`  
**Operation**: `R(A+1) := R(B); R(A) := R(B)[RK(C)]`  
**Input Registers**: 
- R(B): Table (object)
- RK(C): Method name

**Output Registers**: 
- R(A): Method function
- R(A+1): Object (self parameter)

**Special Considerations**: Sets up method call with self

### ADD (12)
**Format**: `ADD A B C`  
**Operation**: `R(A) := RK(B) + RK(C)`  
**Input Registers**: 
- RK(B): Left operand
- RK(C): Right operand

**Output Registers**: 
- R(A): Result

**Special Considerations**: May trigger __add metamethod

### SUB (13)
**Format**: `SUB A B C`  
**Operation**: `R(A) := RK(B) - RK(C)`  
**Input Registers**: 
- RK(B): Left operand
- RK(C): Right operand

**Output Registers**: 
- R(A): Result

**Special Considerations**: May trigger __sub metamethod

### MUL (14)
**Format**: `MUL A B C`  
**Operation**: `R(A) := RK(B) * RK(C)`  
**Input Registers**: 
- RK(B): Left operand
- RK(C): Right operand

**Output Registers**: 
- R(A): Result

**Special Considerations**: May trigger __mul metamethod

### DIV (15)
**Format**: `DIV A B C`  
**Operation**: `R(A) := RK(B) / RK(C)`  
**Input Registers**: 
- RK(B): Left operand
- RK(C): Right operand

**Output Registers**: 
- R(A): Result

**Special Considerations**: May trigger __div metamethod

### MOD (16)
**Format**: `MOD A B C`  
**Operation**: `R(A) := RK(B) % RK(C)`  
**Input Registers**: 
- RK(B): Left operand
- RK(C): Right operand

**Output Registers**: 
- R(A): Result

**Special Considerations**: May trigger __mod metamethod

### POW (17)
**Format**: `POW A B C`  
**Operation**: `R(A) := RK(B) ^ RK(C)`  
**Input Registers**: 
- RK(B): Left operand
- RK(C): Right operand

**Output Registers**: 
- R(A): Result

**Special Considerations**: May trigger __pow metamethod

### UNM (18)
**Format**: `UNM A B`  
**Operation**: `R(A) := -R(B)`  
**Input Registers**: 
- R(B): Operand

**Output Registers**: 
- R(A): Negated value

**Special Considerations**: May trigger __unm metamethod

### NOT (19)
**Format**: `NOT A B`  
**Operation**: `R(A) := not R(B)`  
**Input Registers**: 
- R(B): Operand

**Output Registers**: 
- R(A): Boolean result

**Special Considerations**: Only nil and false are falsy

### LEN (20)
**Format**: `LEN A B`  
**Operation**: `R(A) := length of R(B)`  
**Input Registers**: 
- R(B): Table or string

**Output Registers**: 
- R(A): Length as number

**Special Considerations**: May trigger __len metamethod

### CONCAT (21)
**Format**: `CONCAT A B C`  
**Operation**: `R(A) := R(B).. ... ..R(C)`  
**Input Registers**: 
- R(B) through R(C): Values to concatenate

**Output Registers**: 
- R(A): Concatenated string

**Special Considerations**: 
- May trigger __concat metamethod
- Processes values sequentially
- Must preserve operands during intermediate processing

### JMP (22)
**Format**: `JMP sBx`  
**Operation**: `pc += sBx`  
**Input Registers**: None

**Output Registers**: None

**Special Considerations**: sBx is signed offset

### EQ (23)
**Format**: `EQ A B C`  
**Operation**: `if ((RK(B) == RK(C)) ~= A) then pc++`  
**Input Registers**: 
- RK(B): Left operand
- RK(C): Right operand

**Output Registers**: None

**Special Considerations**: 
- A is expected result (0=false, 1=true)
- Skips next instruction on mismatch
- May trigger __eq metamethod

### LT (24)
**Format**: `LT A B C`  
**Operation**: `if ((RK(B) < RK(C)) ~= A) then pc++`  
**Input Registers**: 
- RK(B): Left operand
- RK(C): Right operand

**Output Registers**: None

**Special Considerations**: 
- A is expected result (0=false, 1=true)
- Skips next instruction on mismatch
- May trigger __lt metamethod

### LE (25)
**Format**: `LE A B C`  
**Operation**: `if ((RK(B) <= RK(C)) ~= A) then pc++`  
**Input Registers**: 
- RK(B): Left operand
- RK(C): Right operand

**Output Registers**: None

**Special Considerations**: 
- A is expected result (0=false, 1=true)
- Skips next instruction on mismatch
- May trigger __le metamethod or use __lt

### TEST (26)
**Format**: `TEST A C`  
**Operation**: `if not (R(A) <=> C) then pc++`  
**Input Registers**: 
- R(A): Value to test

**Output Registers**: None

**Special Considerations**: C is expected truthiness

### TESTSET (27)
**Format**: `TESTSET A B C`  
**Operation**: `if (R(B) <=> C) then R(A) := R(B) else pc++`  
**Input Registers**: 
- R(B): Value to test

**Output Registers**: 
- R(A): Set to R(B) if test passes

**Special Considerations**: Combined test and conditional assignment

### CALL (28)
**Format**: `CALL A B C`  
**Operation**: `R(A), ..., R(A+C-2) := R(A)(R(A+1), ..., R(A+B-1))`  
**Input Registers**: 
- R(A): Function
- R(A+1) through R(A+B-1): Arguments

**Output Registers**: 
- R(A) through R(A+C-2): Results

**Special Considerations**: 
- B=0: use all values from R(A+1) to top
- C=0: all returned values are saved
- Function register must be preserved during argument evaluation
- Return values overwrite function and argument registers

### TAILCALL (29)
**Format**: `TAILCALL A B`  
**Operation**: `return R(A)(R(A+1), ..., R(A+B-1))`  
**Input Registers**: 
- R(A): Function
- R(A+1) through R(A+B-1): Arguments

**Output Registers**: None (returns directly)

**Special Considerations**: 
- Reuses current window 
- B=0: use all values from R(A+1) to top

### RETURN (30)
**Format**: `RETURN A B`  
**Operation**: `return R(A), ..., R(A+B-2)`  
**Input Registers**: 
- R(A) through R(A+B-2): Values to return

**Output Registers**: None

**Special Considerations**: 
- B=0: return all from R(A) to top
- B=1: return no values

### FORPREP (31)
**Format**: `FORPREP A sBx`  
**Operation**: `R(A) -= R(A+2); pc += sBx`  
**Input Registers**: 
- R(A): Initial value
- R(A+1): Limit
- R(A+2): Step

**Output Registers**: 
- R(A): Adjusted index

**Special Considerations**: Prepares numeric for loop

### FORLOOP (32)
**Format**: `FORLOOP A sBx`  
**Operation**: `R(A) += R(A+2); if R(A) <?= R(A+1) then { pc+=sBx; R(A+3) = R(A) }`  
**Input Registers**: 
- R(A): Current index
- R(A+1): Limit
- R(A+2): Step

**Output Registers**: 
- R(A): Updated index
- R(A+3): Loop variable (if continuing)

**Special Considerations**: Direction depends on step sign

### TFORLOOP (33)
**Format**: `TFORLOOP A C`  
**Register Layout**:
- R(A): Iterator function
- R(A+1): State value
- R(A+2): Control variable
- R(A+3) onwards: Loop variables

**Operation**: 
1. Call iterator: `R(A+3)...R(A+3+C) := R(A)(R(A+1), R(A+2))`
2. If R(A+3) ≠ nil: `R(A+2) := R(A+3)` and continue loop

**Input Registers**: 
- R(A): Iterator function
- R(A+1): State
- R(A+2): Control

**Output Registers**: 
- R(A+2): Updated control
- R(A+3) through R(A+3+C-1): Loop variables

**Special Considerations**: 
- Must store iterator function before calling it (typically in R(A+3+C))
- Must restore iterator function after each iteration
- Must validate register bounds to ensure storage register is in window

### SETLIST (34)
**Format**: `SETLIST A B C`  
**Operation**: `R(A)[(C-1)*FPF+i] := R(A+i), 1 <= i <= B`  
**Input Registers**: 
- R(A): Table
- R(A+1) through R(A+B): Values to set

**Output Registers**: None

**Special Considerations**: 
- B=0 means use all values from R(A+1) to top
- FPF (fields per flush) = 50

### VARARG (35)
**Format**: `VARARG A B`  
**Operation**: `R(A), R(A+1), ..., R(A+B-2) = vararg`  
**Input Registers**: None (reads from varargs)

**Output Registers**: 
- R(A) through R(A+B-2): Vararg values

**Special Considerations**: 
- B=0: load all varargs
- Pads with nil if insufficient varargs

### CLOSURE (36)
**Format**: `CLOSURE A Bx`  
**Operation**: `R(A) := closure(KPROTO[Bx])`  
**Input Registers**: Various (for upvalue capture)

**Output Registers**: 
- R(A): New closure

**Special Considerations**: 
- Creates upvalues
- Syncs window to stack for captures
- Multiple transaction phases to avoid borrow checker issues

### CLOSE (37)
**Format**: `CLOSE A`  
**Operation**: `close all upvalues >= R(A)`  
**Input Registers**: None

**Output Registers**: None

**Special Considerations**: Converts open to closed upvalues

### EVAL (39)
**Format**: `EVAL A B C`  
**Operation**: `R(A)...R(A+C-1) := eval(R(B))`  
**Input Registers**: 
- R(B): Source code string

**Output Registers**: 
- R(A) through R(A+C-1): Evaluation results

**Special Considerations**: 
- C=0: all results
- Queues compilation and execution

## Special Register Preservation Rules

### Function Calls

When evaluating function arguments, the function register must be preserved:

```rust
// CRITICAL: Preserve the function register
// This ensures it won't be overwritten during argument evaluation
self.registers.preserve_register(func_reg);
```

### Table Operations

Table registers must be preserved during key evaluation:

```rust
// CRITICAL: Preserve the table register
self.registers.preserve_register(table_reg);
```

### Concatenation

Values being concatenated must be preserved during intermediate operations:

```rust
// Preserve all operands during concatenation
for &reg in &operand_regs {
    self.registers.preserve_register(reg);
}
```

### TForLoop Implementations

The iterator function must be saved before calling it and restored after returning:

```rust
// Constants for consistent access
const TFORLOOP_ITER_OFFSET: usize = 0;    // R(A) = iterator function
const TFORLOOP_STATE_OFFSET: usize = 1;   // R(A+1) = state value
const TFORLOOP_CONTROL_OFFSET: usize = 2; // R(A+2) = control variable
const TFORLOOP_VAR_OFFSET: usize = 3;     // R(A+3) = first loop variable

// Calculate storage register safely
let storage_reg = a + TFORLOOP_VAR_OFFSET + c;

// Bounds checking
if storage_reg >= window_size {
    return Err(LuaError::RuntimeError(format!(
        "TForLoop would access register {} but window only has {} registers",
        storage_reg, window_size
    )));
}

// Save iterator before calling it
self.register_windows.set_register(window_idx, storage_reg, iterator.clone())?;

// After resuming from iterator call:
// Restore iterator to original position
let saved_iterator = self.register_windows.get_register(window_idx, storage_reg)?.clone();
self.register_windows.set_register(window_idx, a + TFORLOOP_ITER_OFFSET, saved_iterator)?;
```

## Standard Library Contracts

### Iterator Functions

All iterator functions (pairs, ipairs, etc.) must return a consistent triplet:

1. Iterator function
2. Invariant state
3. Initial control variable

Example (ipairs):

```rust
// Return the iterator triplet
ctx.push_result(Value::CFunction(ipairs_iter))?;  // Iterator function
ctx.push_result(table_val)?;                      // State (the table)
ctx.push_result(Value::Number(0.0))?;             // Initial control variable (index 0)
```

The iterator function itself must:

1. Accept state and control variable as arguments
2. Return new control variable followed by values for loop variables
3. Return nil to terminate iteration

Example:

```rust
fn ipairs_iter(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    // Table is arg 0, control value is arg 1
    let table = match ctx.get_arg(0)? {
        Value::Table(handle) => handle,
        _ => return Err(LuaError::TypeError {...}),
    };
    
    // Get current index from control value
    let current_idx = match ctx.get_arg(1)? {
        Value::Number(n) => n as i64,
        Value::Nil => 0,
        _ => return Err(LuaError::TypeError {...}),
    };
    
    let next_idx = current_idx + 1;
    let value = ctx.table_get(table, Value::Number(next_idx as f64))?;
    
    if value.is_nil() {
        // End iteration
        ctx.push_result(Value::Nil)?;
        return Ok(1); // Return nil to signal end
    } else {
        // Continue iteration
        ctx.push_result(Value::Number(next_idx as f64))?; // Next index
        ctx.push_result(value)?;                          // Value at index
        return Ok(2); // Return 2 values
    }
}
```

## Implementation Best Practices

1. **Use Constants**: Define register layout constants at the module level for consistency.
2. **Bounds Checking**: Always validate register access is within window bounds.
3. **Register Preservation**: Explicitly preserve registers needed across nested operations.
4. **Clear Comments**: Document register usage patterns with clear comments.
5. **Consistent Patterns**: Follow the same register layout conventions throughout.

## Conclusion

These register usage conventions ensure alignment between all components of the Lua VM implementation. By following these guidelines consistently, we can avoid subtle bugs related to mismatched register expectations across different parts of the system.