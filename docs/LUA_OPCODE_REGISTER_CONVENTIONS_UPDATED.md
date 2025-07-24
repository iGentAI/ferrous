# Lua 5.1 Opcode Register Conventions for RC RefCell VM

## Table of Contents

1. [Introduction](#introduction)
2. [Critical Lua 5.1 Register Model](#critical-lua-51-register-model)
3. [Instruction Formats](#instruction-formats)
4. [Stack and Register Layout](#stack-and-register-layout)
5. [Complete Opcode Reference](#complete-opcode-reference)
6. [RC RefCell VM Implementation](#rc-refcell-vm-implementation)
7. [Implementation Notes](#implementation-notes)

## Introduction

This document provides the **complete and accurate** reference for register usage in all Lua 5.1 opcodes as implemented in the Ferrous RC RefCell VM. Every detail in this document has been verified against the actual working RC RefCell VM implementation in `src/lua/rc_vm.rs`.

**IMPORTANT**: This document covers the RC RefCell VM implementation, which follows Lua 5.1 semantics closely but includes some implementation-specific deviations that are clearly marked.

### Key Architectural Principles

1. **Unified Stack Model**: All values live on a single, contiguous stack (no register windows)
2. **Relative Addressing**: Register `R(n)` maps to `stack[base + n]` where `base` is the current function's base register index.
3. **Lua 5.1 Compatibility**: Register usage follows Lua 5.1 specification with noted deviations.
4. **RC RefCell Access**: All register access goes through `vm.get_register()` and `vm.set_register()`.

## Critical Lua 5.1 Register Model

### Stack Layout (Lua 5.1 Specification)

```
Absolute Index | Relative to Base | Content
---------------|------------------|---------------------------
...            | ...              | ...
stack[base-1]  | -1               | function being called
stack[base]    | R(0)             | first local/parameter
stack[base+1]  | R(1)             | second local/parameter
stack[base+2]  | R(2)             | third local/parameter
...            | ...              | ...
stack[base+n]  | R(n)             | nth register
```

### RC RefCell VM Register Access

```rust
// How RC RefCell VM accesses registers (from rc_vm.rs)
fn get_register(&self, index: usize) -> LuaResult<Value> {
    self.heap.get_register(&self.current_thread, index)
}

fn set_register(&self, index: usize, value: Value) -> LuaResult<()> {
    self.heap.set_register(&self.current_thread, index, value)
}
```

## Instruction Formats

Lua 5.1 uses three instruction formats, each exactly 32 bits:

```
Format ABC:  [  C:9  ][  B:9  ][ A:8 ][ OP:6 ]
Format ABx:  [      Bx:18      ][ A:8 ][ OP:6 ]
Format AsBx: [     sBx:18      ][ A:8 ][ OP:6 ]
```

### Field Specifications

- **OP**: Opcode (6 bits)
- **A**: Primary register, usually destination (8 bits, max 255)
- **B/C**: Source registers or flags (9 bits each, max 511)
- **Bx**: Unsigned constant index (18 bits, max 262143)
- **sBx**: Signed jump offset (18 bits, -131071 to +131072)

### RK Notation (Critical for Correctness)

The RK notation indicates a value can be either a register or constant:
- If bit 8 is 0: value is in register `R(n)`
- If bit 8 is 1: value is constant `Kst(n & 0xFF)`

```rust
// RC RefCell VM RK implementation
fn read_rk(&self, base: usize, rk: u32) -> LuaResult<Value> {
    if rk & 0x100 != 0 {
        // Constant
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        self.get_constant(&frame.closure, (rk & 0xFF) as usize)
    } else {
        // Register
        self.get_register(base + rk as usize)
    }
}
```

## Complete Opcode Reference

### Data Movement Operations

#### 0. MOVE (ABC Format)

**Register Usage**:
- **R(A)**: Destination register (output)
- **R(B)**: Source register (input)
- **C**: Unused (always 0)

**Operation**: `R(A) := R(B)`

**RC RefCell VM Implementation**:
```rust
fn op_move(&self, inst: Instruction, base: usize) -> LuaResult<()> {
    let a = inst.get_a() as usize;
    let b = inst.get_b() as usize;
    
    let value = self.get_register(base + b)?;
    self.set_register(base + a, value)?;
    
    Ok(())
}
```

### Constant Loading Operations

#### 1. LOADK (ABx Format)

**Register Usage**:
- **R(A)**: Destination register (output)
- **Bx**: Constant index in function's constant table

**Operation**: `R(A) := Kst(Bx)`

**RC RefCell VM Implementation**:
```rust
fn op_loadk(&self, inst: Instruction, base: usize) -> LuaResult<()> {
    let a = inst.get_a() as usize;
    let bx = inst.get_bx() as usize;
    
    let frame = self.heap.get_current_frame(&self.current_thread)?;
    let constant = self.get_constant(&frame.closure, bx)?;
    
    self.set_register(base + a, constant)?;
    
    Ok(())
}
```

#### 2. LOADBOOL (ABC Format)

**Register Usage**:
- **R(A)**: Destination register (output)
- **B**: Boolean value (0 = false, non-zero = true)
- **C**: Skip flag (if non-zero, skip next instruction)

**Operation**: 
```
R(A) := (Bool)B
if (C) pc++
```

**RC RefCell VM Implementation**:
```rust
fn op_loadbool(&self, inst: Instruction, base: usize) -> LuaResult<()> {
    let a = inst.get_a() as usize;
    let b = inst.get_b();
    let c = inst.get_c();
    
    // Set boolean value
    self.set_register(base + a, Value::Boolean(b != 0))?;
    
    // Skip next instruction if C is non-zero
    if c != 0 {
        let pc = self.heap.get_pc(&self.current_thread)?;
        self.heap.set_pc(&self.current_thread, pc + 1)?;
    }
    
    Ok(())
}
```

#### 3. LOADNIL (ABC Format)

**Register Usage**:
- **R(A)**: Start register (input)
- **R(B)**: End register (input)
- **C**: Unused

**Operation**: `R(A) := ... := R(B) := nil`

**RC RefCell VM Implementation**:
```rust
fn op_loadnil(&self, inst: Instruction, base: usize) -> LuaResult<()> {
    let a = inst.get_a() as usize;
    let b = inst.get_b() as usize;
    
    // Set range to nil
    for i in a..=b {
        self.set_register(base + i, Value::Nil)?;
    }
    
    Ok(())
}
```

### Upvalue Operations

#### 4. GETUPVAL (ABC Format)

**Register Usage**:
- **R(A)**: Destination register (output)
- **B**: Upvalue index
- **C**: Unused

**Operation**: `R(A) := UpValue[B]`

#### 8. SETUPVAL (ABC Format)

**Register Usage**:
- **R(A)**: Source register (input)
- **B**: Upvalue index
- **C**: Unused

**Operation**: `UpValue[B] := R(A)`

### Global Operations

#### 5. GETGLOBAL (ABx Format)

**Register Usage**:
- **R(A)**: Destination register (output)
- **Bx**: Constant index for global name

**Operation**: `R(A) := Gbl[Kst(Bx)]`

#### 7. SETGLOBAL (ABx Format)

**Register Usage**:
- **Bx**: Constant index for global name
- **R(A)**: Source register (input)

**Operation**: `Gbl[Kst(Bx)] := R(A)`

### Table Operations

#### 6. GETTABLE (ABC Format)

**Register Usage**:
- **R(A)**: Destination register (output)
- **R(B)**: Table register (input)
- **RK(C)**: Key register or constant (input)

**Operation**: `R(A) := R(B)[RK(C)]`

#### 9. SETTABLE (ABC Format)

**Register Usage**:
- **R(A)**: Table register (input)
- **RK(B)**: Key register or constant (input)
- **RK(C)**: Value register or constant (input)

**Operation**: `R(A)[RK(B)] := RK(C)`

#### 10. NEWTABLE (ABC Format)

**Register Usage**:
- **R(A)**: Destination register (output)
- **B**: Array size hint (encoded)
- **C**: Hash size hint (encoded)

**Operation**: `R(A) := {} (size = B,C)`

#### 11. SELF (ABC Format)

**Register Usage**:
- **R(A)**: Method destination register (output)
- **R(A+1)**: Self destination register (output)
- **R(B)**: Table register (input)
- **RK(C)**: Method name register or constant (input)

**Operation**: 
```
R(A+1) := R(B)
R(A) := R(B)[RK(C)]
```

### Arithmetic Operations

#### 12. ADD (ABC Format)

**Register Usage**:
- **R(A)**: Destination register (output)
- **RK(B)**: Left operand register or constant (input)
- **RK(C)**: Right operand register or constant (input)

**Operation**: `R(A) := RK(B) + RK(C)`

#### 13. SUB (ABC Format)

**Register Usage**: Same as ADD
**Operation**: `R(A) := RK(B) - RK(C)`

#### 14. MUL (ABC Format)

**Register Usage**: Same as ADD
**Operation**: `R(A) := RK(B) * RK(C)`

#### 15. DIV (ABC Format)

**Register Usage**: Same as ADD
**Operation**: `R(A) := RK(B) / RK(C)`

#### 16. MOD (ABC Format)

**Register Usage**: Same as ADD
**Operation**: `R(A) := RK(B) % RK(C)`

#### 17. POW (ABC Format)

**Register Usage**: Same as ADD
**Operation**: `R(A) := RK(B) ^ RK(C)`

### Unary Operations

#### 18. UNM (ABC Format)

**Register Usage**:
- **R(A)**: Destination register (output)
- **R(B)**: Operand register (input)
- **C**: Unused

**Operation**: `R(A) := -R(B)`

#### 19. NOT (ABC Format)

**Register Usage**:
- **R(A)**: Destination register (output)
- **R(B)**: Operand register (input)
- **C**: Unused

**Operation**: `R(A) := not R(B)`

#### 20. LEN (ABC Format)

**Register Usage**:
- **R(A)**: Destination register (output)
- **R(B)**: Operand register (input)
- **C**: Unused

**Operation**: `R(A) := length of R(B)`

### String Operations

#### 21. CONCAT (ABC Format)

**Register Usage**:
- **R(A)**: Destination register (output)
- **R(B)**: First concatenation register (input)
- **R(C)**: Last concatenation register (input)

**Operation**: `R(A) := R(B).. ... ..R(C)`

### Control Flow Operations

#### 22. JMP (AsBx Format)

**Register Usage**:
- **A**: Unused (should be 0)
- **sBx**: Signed offset to add to PC

**Operation**: `pc += sBx`

### Comparison Operations

#### 23. EQ (ABC Format)

**Register Usage**:
- **A**: Skip flag (if result ≠ A, skip next instruction)
- **RK(B)**: Left operand register or constant (input)
- **RK(C)**: Right operand register or constant (input)

**Operation**: `if ((RK(B) == RK(C)) ~= A) then pc++`

#### 24. LT (ABC Format)

**Register Usage**: Same as EQ
**Operation**: `if ((RK(B) < RK(C)) ~= A) then pc++`

#### 25. LE (ABC Format)

**Register Usage**: Same as EQ
**Operation**: `if ((RK(B) <= RK(C)) ~= A) then pc++`

### Test Operations

#### 26. TEST (ABC Format)

**Register Usage**:
- **R(A)**: Test register (input)
- **B**: Unused
- **C**: Test condition (0 = test falsey, 1 = test truthy)

**Operation**: `if not (R(A) <=> C) then pc++`

#### 27. TESTSET (ABC Format)

**Register Usage**:
- **R(A)**: Destination register (output)
- **R(B)**: Source register (input)
- **C**: Test condition (0 = test falsey, 1 = test truthy)

**Operation**: `if (R(B) <=> C) then R(A) := R(B) else pc++`

### Function Call Operations

#### 28. CALL (ABC Format)

**Register Usage**:
- **R(A)**: Function register (input), then results start here
- **R(A+1)...R(A+B-1)**: Arguments (input)
- **R(A)...R(A+C-2)**: Results (output)
- **B**: Argument count + 1 (0 = use all to top)
- **C**: Result count + 1 (0 = return all)

**Operation**: `R(A), ..., R(A+C-2) := R(A)(R(A+1), ..., R(A+B-1))`

#### 29. TAILCALL (ABC Format)

**Register Usage**: Same as CALL
**Operation**: `return R(A)(R(A+1), ..., R(A+B-1))`

#### 30. RETURN (ABC Format)

**Register Usage**:
- **R(A)**: First return value register (input)
- **B**: Return count + 1 (0 = return all from A to top)
- **C**: Unused

**Operation**: `return R(A), ..., R(A+B-2)`

### Loop Operations

#### 31. FORLOOP (AsBx Format)

**Register Usage**:
- **R(A)**: Internal loop index (input/output)
- **R(A+1)**: Limit value (input)
- **R(A+2)**: Step value (input)
- **R(A+3)**: User variable (output ONLY when loop continues)
- **sBx**: Jump offset back to loop start (negative)

**Operation**:
```
R(A) += R(A+2);  // increment internal counter by step
if (step > 0 ? R(A) <= R(A+1) : R(A) >= R(A+1)) then
    R(A+3) := R(A);  // update user-visible variable ONLY when continuing
    pc += sBx;       // jump back (sBx is negative)
end
```

#### 32. FORPREP (AsBx Format)

**Register Usage**:
- **R(A)**: Internal loop index (input/output)
- **R(A+1)**: Limit value (input)
- **R(A+2)**: Step value (input)
- **R(A+3)**: User variable (NEVER modified by FORPREP)
- **sBx**: Jump offset to FORLOOP instruction

**Operation**:
```
R(A) -= R(A+2);  // subtract step from initial value  
pc += sBx;       // ALWAYS jump to FORLOOP
```

**Lua 5.1 Specification Behavior**:
- **Type checking**: All three values (initial, limit, step) must be convertible to numbers. The Ferrous VM correctly errors if they are not.
- **Zero step**: Allowed and creates intentional infinite loops when loop condition holds. The Ferrous VM correctly allows this.
- **Always jumps**: FORPREP always jumps to FORLOOP regardless of values. The Ferrous VM correctly implements this.

#### 33. TFORCALL (ABC Format) - VM-Specific

**Register Usage**:
- **R(A)**: Iterator function register (input).
- **R(A+1)**: State register (input).
- **R(A+2)**: Control variable register (input).
- **C**: Number of loop variables to expect as results.

**Operation**:
- This is the first part of a two-opcode generic for loop.
- It calls the iterator function `R(A)` with arguments `R(A+1)` and `R(A+2)`.
- It expects `C` results, which will be placed starting at `R(A+3)`.
- This operation is implemented non-recursively by queueing a `FunctionCall` operation.

#### 34. TFORLOOP (AsBx Format) - VM-Specific

**Register Usage**:
- **R(A)**: Base register for the iterator state (same `A` as `TFORCALL`).
- **sBx**: Jump offset to the beginning of the loop body.

**Operation**:
- This is the second part of a two-opcode generic for loop, executed via a `TForLoopContinuation`.
- It checks the first result from the iterator call (now in `R(A+3)`).
- If the result is not nil, it copies it to the control variable `R(A+2)` and jumps back to the loop body by `sBx`.
- If the result is nil, execution proceeds to the next instruction, exiting the loop.

### List Operations

#### 35. SETLIST (ABC Format)

**Register Usage**:
- **R(A)**: Table register (input)
- **B**: Number of elements to set (0 = use all to top)
- **C**: Batch number (1-based, 0 = next instruction has real C)

**Operation**: `R(A)[(C-1)*FPF+i] := R(A+i), 1 <= i <= B` (Where FPF = 50)

### Closure Operations

#### 36. CLOSE (ABC Format)

**Register Usage**:
- **A**: Lowest register to close upvalues for
- **B**: Unused
- **C**: Unused

**Operation**: Close all upvalues for locals >= R(A)

#### 37. CLOSURE (ABx Format)

**Register Usage**:
- **R(A)**: Destination register (output)
- **Bx**: Function prototype index in constants

**Operation**: `R(A) := closure(KPROTO[Bx], upvalues...)`

**Note**: CLOSURE is followed by pseudo-instructions (MOVE or GETUPVAL) for each upvalue.

### Vararg Operations

#### 38. VARARG (ABC Format)

**Register Usage**:
- **R(A)**: First destination register (output)
- **B**: Number of varargs to copy + 1 (0 = copy all varargs)
- **C**: Unused

**Operation**: `R(A), ..., R(A+B-2) := vararg`

## RC RefCell VM Implementation

### Register Access Pattern

The RC RefCell VM uses a clean register access pattern that directly maps to the Lua 5.1 stack model:

```rust
// From rc_vm.rs - the ONLY way to access registers
impl RcVM {
    fn get_register(&self, index: usize) -> LuaResult<Value> {
        self.heap.get_register(&self.current_thread, index)
    }
    
    fn set_register(&self, index: usize, value: Value) -> LuaResult<()> {
        self.heap.set_register(&self.current_thread, index, value)
    }
}
```

## Implementation Notes

### Deviations from Lua 5.1

The RC RefCell VM includes these implementation-specific enhancements:

1. **Generic For Loop**: Implemented using direct TFORCALL/TFORLOOP execution to eliminate temporal state separation issues that occurred with queue-based processing.
2. **Enhanced metamethod support**: Uses direct metamethod execution for immediate processing.
3. **Direct execution model**: Unified Frame-based execution eliminates the need for operation queuing.

### Upvalue Management

The VM properly implements Lua 5.1 upvalue semantics with sharing:

```rust
pub enum UpvalueState {
    Open {
        thread: ThreadHandle,
        stack_index: usize,
    },
    Closed {
        value: Value,
    },
}

// Upvalues are shared between closures through Rc<RefCell>
pub type UpvalueHandle = Rc<RefCell<UpvalueState>>;
```