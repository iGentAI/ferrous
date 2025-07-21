# Lua 5.1 Opcode Register Conventions (Updated)

## Table of Contents

1. [Introduction](#introduction)
2. [Instruction Formats](#instruction-formats)
3. [Register Addressing](#register-addressing)
4. [Opcode Categories](#opcode-categories)
   - [Data Movement Operations](#data-movement-operations)
   - [Constant Loading Operations](#constant-loading-operations)
   - [Global Variable Operations](#global-variable-operations)
   - [Table Operations](#table-operations)
   - [Arithmetic Operations](#arithmetic-operations)
   - [Unary Operations](#unary-operations)
   - [String Operations](#string-operations)
   - [Control Flow Operations](#control-flow-operations)
   - [Comparison Operations](#comparison-operations)
   - [Function Call Operations](#function-call-operations)
   - [Loop Control Operations](#loop-control-operations)
   - [Closure Operations](#closure-operations)
   - [Advanced Operations](#advanced-operations)
5. [Metamethod Support](#metamethod-support)
6. [Transaction-Based Implementation](#transaction-based-implementation)
7. [Rc<RefCell> Migration Strategy](#rcrefcell-migration-strategy)
8. [Register Window History](#register-window-history)

## Introduction

This document serves as the definitive reference for register usage in all Lua 5.1 opcodes as implemented in Ferrous. The Ferrous VM uses a unified stack architecture where registers are simply stack positions relative to the current function's base pointer.

### Key Principles

1. **Unified Stack Model**: All values live on a single, contiguous stack
2. **Relative Addressing**: Register `R(n)` maps to `stack[base + n]`
3. **Transaction Safety**: All register access must go through the transaction system
4. **Lua 5.1 Compatibility**: Register usage exactly matches Lua 5.1 specification
5. **Metamethod Support**: Many operations trigger metamethods for non-standard types

## Instruction Formats

Lua 5.1 uses three instruction formats, each 32 bits:

```
Format ABC:  [  C:9  ][  B:9  ][ A:8 ][ OP:6 ]
Format ABx:  [      Bx:18      ][ A:8 ][ OP:6 ]
Format AsBx: [     sBx:18      ][ A:8 ][ OP:6 ]
```

### Field Descriptions

- **OP**: Opcode (6 bits, values 0-37)
- **A**: Primary register, usually destination (8 bits, max 255)
- **B/C**: Source registers or flags (9 bits each, max 511)
- **Bx**: Unsigned constant index (18 bits, max 262143)
- **sBx**: Signed jump offset (18 bits, -131071 to +131072)

### RK Notation

The RK notation indicates a value can be either a register or constant:
- If bit 8 is 0: value is in register `R(n)`
- If bit 8 is 1: value is constant `Kst(n & 0xFF)`

## Register Addressing

### Stack Layout

```
Absolute Index | Relative to Base | Content
---------------|------------------|---------------------------
stack[0]       | -                | _ENV (global environment)
stack[1]       | -                | main chunk function
...            | ...              | ...
stack[base-1]  | -1               | function being called
stack[base]    | R(0)             | first local/parameter
stack[base+1]  | R(1)             | second local/parameter
stack[base+2]  | R(2)             | third local/parameter
...            | ...              | ...
stack[base+n]  | R(n)             | nth register
```

### Transaction-Based Access

```rust
// Reading a register
let value = tx.read_register(thread, base + register_index)?;

// Writing a register  
tx.set_register(thread, base + register_index, value)?;
```

## Opcode Categories

### Data Movement Operations

#### 0. MOVE (ABC Format)

**Purpose**: Copy value from one register to another

**Register Usage**:
- **R(A)**: Destination register (output)
- **R(B)**: Source register (input)
- **C**: Unused (always 0)

**Operation**: `R(A) := R(B)`

**Implementation Requirements**:
```rust
fn op_move(tx: &mut HeapTransaction, inst: Instruction, base: u16, thread: ThreadHandle) -> LuaResult<()> {
    let a = inst.get_a() as usize;
    let b = inst.get_b() as usize;
    
    let value = tx.read_register(thread, base as usize + b)?;
    tx.set_register(thread, base as usize + a, value)?;
    
    Ok(())
}
```

**Transaction Notes**: Simple read-write pattern, no heap allocation needed

---

### Constant Loading Operations

#### 1. LOADK (ABx Format)

**Purpose**: Load constant into register

**Register Usage**:
- **R(A)**: Destination register (output)
- **Bx**: Constant index in function's constant table

**Operation**: `R(A) := Kst(Bx)`

**Implementation Requirements**:
- Must validate constant index against function's constant table size
- Constants are stored per-function, not globally
- Must handle all constant types (nil, boolean, number, string)

---

#### 2. LOADBOOL (ABC Format)

**Purpose**: Load boolean value and optionally skip next instruction

**Register Usage**:
- **R(A)**: Destination register (output)
- **B**: Boolean value (0 = false, non-zero = true)
- **C**: Skip flag (if non-zero, skip next instruction)

**Operation**: 
```
R(A) := (Bool)B
if (C) pc++
```

**Special Considerations**:
- Used for implementing boolean results from comparisons
- C flag enables comparison short-circuiting

---

#### 3. LOADNIL (ABC Format)

**Purpose**: Set a range of registers to nil

**Register Usage**:
- **R(A)**: First register to set (output)
- **R(B)**: Last register to set (output)
- **C**: Unused

**Operation**: `R(A) := ... := R(B) := nil`

**Implementation Notes**:
- B is an absolute register index, not relative to A
- Used for initializing locals and clearing stack ranges

---

### Global Variable Operations

#### 5. GETGLOBAL (ABx Format)

**Purpose**: Read global variable

**Register Usage**:
- **R(A)**: Destination register (output)
- **Bx**: Constant index of variable name (must be string)

**Operation**: `R(A) := Gbl[Kst(Bx)]`

**Transaction Requirements**:
1. Read constant string from current function
2. Access global table (_ENV)
3. Perform table lookup with metamethod support (__index)

---

#### 7. SETGLOBAL (ABx Format)

**Purpose**: Write global variable

**Register Usage**:
- **R(A)**: Source value register (input)
- **Bx**: Constant index of variable name (must be string)

**Operation**: `Gbl[Kst(Bx)] := R(A)`

**Transaction Requirements**:
1. Read value from register
2. Read constant string 
3. Access global table
4. Set table field (may trigger __newindex)

---

### Table Operations

#### 6. GETTABLE (ABC Format)

**Purpose**: Table field read

**Register Usage**:
- **R(A)**: Destination register (output)
- **R(B)**: Table register (input)
- **RK(C)**: Key (register or constant) (input)

**Operation**: `R(A) := R(B)[RK(C)]`

**Stack Diagram**:
```
Before:             After:
R(B): <table>      R(A): <value>
RK(C): <key>       R(B): <table>
                   RK(C): <key>
```

**Metamethod Support**: Must check for __index if field is nil

---

#### 9. SETTABLE (ABC Format)

**Purpose**: Table field write

**Register Usage**:
- **R(A)**: Table register (input/output)
- **RK(B)**: Key (register or constant) (input)
- **RK(C)**: Value (register or constant) (input)

**Operation**: `R(A)[RK(B)] := RK(C)`

**Metamethod Support**: May trigger __newindex for new keys

**Transaction Pattern**:
```rust
let table = tx.read_register(thread, base + a)?;
let key = read_rk(tx, base, b)?;
let value = read_rk(tx, base, c)?;
tx.set_table_field(table.as_table()?, key, value)?;
```

---

#### 10. NEWTABLE (ABC Format)

**Purpose**: Create new empty table

**Register Usage**:
- **R(A)**: Destination register (output)
- **B**: Array size hint (encoded)
- **C**: Hash size hint (encoded)

**Operation**: `R(A) := {} (size = B,C)`

**Size Encoding**:
- If B or C is 0: that part has size 0
- Otherwise: size = 2^(x-1) where x is the field value

---

#### 11. SELF (ABC Format)

**Purpose**: Prepare for method call (t:method())

**Register Usage**:
- **R(A)**: Base register for call (output: method function)
- **R(A+1)**: Self parameter (output: table copy)
- **R(B)**: Table (input)
- **RK(C)**: Method name key (input)

**Operation**:
```
R(A+1) := R(B)
R(A) := R(B)[RK(C)]
```

---

### Arithmetic Operations

All arithmetic operations follow the same pattern:

#### 12-17. ADD, SUB, MUL, DIV, MOD, POW (ABC Format)

**Register Usage**:
- **R(A)**: Destination register (output)
- **RK(B)**: Left operand (input)
- **RK(C)**: Right operand (input)

**Operations**:
- **ADD (12)**: `R(A) := RK(B) + RK(C)`
- **SUB (13)**: `R(A) := RK(B) - RK(C)`
- **MUL (14)**: `R(A) := RK(B) * RK(C)`
- **DIV (15)**: `R(A) := RK(B) / RK(C)`
- **MOD (16)**: `R(A) := RK(B) % RK(C)`
- **POW (17)**: `R(A) := RK(B) ^ RK(C)`

**Metamethod Support**: Triggers __add, __sub, __mul, __div, __mod, __pow if operands are not numbers

**Error Handling**:
- Type errors if operands not numbers and no metamethods
- Division by zero for DIV and MOD

---

### Unary Operations

#### 18. UNM (ABC Format)

**Purpose**: Unary minus (negation)

**Register Usage**:
- **R(A)**: Destination register (output)
- **R(B)**: Operand register (input)
- **C**: Unused

**Operation**: `R(A) := -R(B)`

**Metamethod Support**: Triggers __unm if operand is not a number

---

#### 19. NOT (ABC Format)

**Purpose**: Logical not

**Register Usage**:
- **R(A)**: Destination register (output)
- **R(B)**: Operand register (input)
- **C**: Unused

**Operation**: `R(A) := not R(B)`

**Truth Table**:
- nil → true
- false → true
- anything else → false

---

#### 20. LEN (ABC Format)

**Purpose**: Length operator

**Register Usage**:
- **R(A)**: Destination register (output)
- **R(B)**: Operand register (input)
- **C**: Unused

**Operation**: `R(A) := length of R(B)`

**Type Behavior**:
- String: byte length
- Table: array part length (highest integer key) or __len metamethod

**Metamethod Support**: Triggers __len for tables

---

### String Operations

#### 21. CONCAT (ABC Format)

**Purpose**: String concatenation

**Register Usage**:
- **R(A)**: Destination register (output)
- **R(B)**: First operand register (input)
- **R(C)**: Last operand register (input)

**Operation**: `R(A) := R(B).. ... ..R(C)`

**Special Behavior**:
- Concatenates all values from R(B) to R(C) inclusive
- Numbers are converted to strings
- Other types cause type error or trigger __concat metamethod
- Creates new string in heap

---

### Control Flow Operations

#### 22. JMP (AsBx Format)

**Purpose**: Unconditional jump

**Register Usage**:
- **A**: Unused (always 0)
- **sBx**: Signed PC offset

**Operation**: `pc += sBx`

**Implementation Note**: PC has already been incremented when instruction executes

---

### Comparison Operations

#### 23-25. EQ, LT, LE (ABC Format)

**Purpose**: Compare values and conditionally skip

**Register Usage**:
- **A**: Test polarity (0 or 1)
- **RK(B)**: Left operand (input)
- **RK(C)**: Right operand (input)

**Operations**:
- **EQ (23)**: `if ((RK(B) == RK(C)) ~= A) then pc++`
- **LT (24)**: `if ((RK(B) < RK(C)) ~= A) then pc++`
- **LE (25)**: `if ((RK(B) <= RK(C)) ~= A) then pc++`

**Metamethod Support**: Triggers __eq, __lt, __le if operands are not numbers/strings

**Usage Pattern**:
```lua
-- Lua: if a < b then ... end
-- Bytecode:
LT 1 R(a) R(b)  ; skip next if NOT (a < b)
JMP end_label   ; skip then-block
; then-block code here
```

---

#### 26. TEST (ABC Format)

**Purpose**: Test register truthiness

**Register Usage**:
- **R(A)**: Register to test (input)
- **B**: Unused
- **C**: Test polarity

**Operation**: `if not (R(A) <=> C) then pc++`

**Truth Testing**:
- C=0: skip if R(A) is truthy
- C=1: skip if R(A) is falsy

---

#### 27. TESTSET (ABC Format)

**Purpose**: Test and conditionally assign

**Register Usage**:
- **R(A)**: Destination register (output)
- **R(B)**: Source/test register (input)
- **C**: Test polarity

**Operation**: `if (R(B) <=> C) then R(A) := R(B) else pc++`

**Common Use**: Implementing short-circuit `and`/`or` operators

---

### Function Call Operations

#### 28. CALL (ABC Format)

**Purpose**: Function call

**Register Usage**:
- **R(A)**: Function register (input)
- **R(A+1)...R(A+B-1)**: Arguments (input)
- **R(A)...R(A+C-2)**: Results (output)
- **B**: Argument count + 1 (0 = use all to top)
- **C**: Result count + 1 (0 = return all)

**Stack Layout**:
```
Before call:          After call:
R(A):   <function>    R(A):   <result1>
R(A+1): <arg1>        R(A+1): <result2>
R(A+2): <arg2>        ...
...                   R(A+C-2): <resultN>
```

**Transaction Requirements**:
1. Validate function type (may trigger __call metamethod)
2. Queue appropriate operation (Lua/C function)
3. Set up new call frame for Lua functions

---

#### 29. TAILCALL (ABC Format)

**Purpose**: Tail call (reuse current frame)

**Register Usage**: Same as CALL

**Operation**: Like CALL but reuses current call frame

**Special Requirements**:
- Must be last instruction before RETURN
- Reuses caller's expected result count
- No new call frame created

---

#### 30. RETURN (ABC Format)

**Purpose**: Return from function

**Register Usage**:
- **R(A)...R(A+B-2)**: Return values (input)
- **B**: Return count + 1 (0 = return all from R(A) to top)
- **C**: Unused

**Operation**: 
```
close all open upvalues >= base
return R(A), ... ,R(A+B-2)
```

**Frame Cleanup**:
- Close all open upvalues >= base before popping frame
- Pops current call frame
- Places results at calling function's position
- Restores previous frame's PC

---

### Loop Control Operations

#### 32. FORPREP (AsBx Format)

**Purpose**: Initialize numeric for loop

**Register Usage**:
- **R(A)**: Internal loop index (input/output)
- **R(A+1)**: Limit value (input)
- **R(A+2)**: Step value (input)
- **R(A+3)**: User variable (not modified by FORPREP)
- **sBx**: Jump offset to loop end

**Operation**:
```
R(A) -= R(A+2);  -- prepare index
pc += sBx;       -- always jump to FORLOOP
```

**Register Layout**:
```
R(A):   internal index (modified by VM)
R(A+1): limit (constant during loop)
R(A+2): step (constant during loop) 
R(A+3): user variable (set by FORLOOP, not FORPREP)
```

**Critical Implementation Note**: 
- FORPREP always jumps to FORLOOP (sBx points over loop body)
- FORPREP never modifies R(A+3). Only FORLOOP should update the user-visible variable.
- The official Lua 5.1 implementation always performs the jump

**Compiler Requirements**: These 4 registers must be allocated consecutively before any expression compilation, and the compiler must ensure they are not corrupted by operations in the loop body.

---

#### 31. FORLOOP (AsBx Format)

**Purpose**: Numeric for loop iteration

**Register Usage**:
- **R(A)**: Internal loop index (input/output)
- **R(A+1)**: Limit value (input)
- **R(A+2)**: Step value (input)  
- **R(A+3)**: User variable (output)
- **sBx**: Jump offset back to loop start (negative)

**Operation**:
```
R(A) += R(A+2);  -- increment internal counter
if (step > 0 ? R(A) <= R(A+1) : R(A) >= R(A+1)) then
    R(A+3) := R(A);  -- update user-visible variable
    pc += sBx;       -- jump back to start (sBx is negative)
end
```

**Critical Implementation Note**: 
- FORLOOP handles both positive and negative steps
- FORLOOP only updates the user variable R(A+3) when the loop continues
- This is the only opcode that should write to R(A+3) in a numeric for loop

---

#### 33. TFORLOOP (ABC Format)

**Purpose**: Generic for loop iteration (for-in loops using pairs/ipairs)

**Register Usage**:
- **R(A)**: Iterator function (input)
- **R(A+1)**: State value (input/output)
- **R(A+2)**: Control variable (input/output)
- **R(A+3)...R(A+2+C)**: Loop variables (output)
- **C**: Number of loop variables

**Operation**:
```
R(A+3), ..., R(A+2+C) := R(A)(R(A+1), R(A+2));  -- Call iterator with state and control
if R(A+3) ~= nil then  -- First result determines if iteration continues
    R(A+2) := R(A+3);  -- Update control variable to first result for next iteration
else
    pc++;  -- Skip next instruction (the JMP back to loop start)
end
```

**Iterator Protocol**: 
The Lua 5.1 iterator protocol works as follows:
1. Iterator function takes two args: state and control variable
2. Iterator returns nil when iteration is complete
3. First return value becomes the new control variable for next iteration
4. For pairs/ipairs, this means (key, value) where key becomes the control

**Compiler-VM Coordination**: The compiler must ensure the registers for iterator function, state, control variable, and loop variables are allocated consecutively and preserved throughout the loop body.

---

### Closure Operations

#### 36. CLOSURE (ABx Format)

**Purpose**: Create closure from prototype

**Register Usage**:
- **R(A)**: Destination register (output)
- **Bx**: Function prototype index in constants

**Operation**: `R(A) := closure(KPROTO[Bx], upvalues...)`

**Upvalue Capture**:
- Following instructions specify upvalue sources
- Uses MOVE or GETUPVAL pseudo-instructions
- Creates upvalue objects as needed (shared between closures)

---

#### 4. GETUPVAL (ABC Format)

**Purpose**: Read upvalue

**Register Usage**:
- **R(A)**: Destination register (output)
- **B**: Upvalue index in current closure
- **C**: Unused

**Operation**: `R(A) := UpValue[B]`

---

#### 8. SETUPVAL (ABC Format)

**Purpose**: Write upvalue

**Register Usage**:
- **A**: Upvalue index in current closure
- **R(B)**: Source register (input)
- **C**: Unused

**Operation**: `UpValue[A] := R(B)`

---

#### 35. CLOSE (ABC Format)

**Purpose**: Close upvalues

**Register Usage**:
- **A**: Stack level (all upvalues >= R(A) are closed)
- **B**: Unused
- **C**: Unused

**Operation**: Close all upvalues pointing to stack positions >= R(A)

**Use Cases**:
- Before leaving scope with local variables
- Before RETURN when locals might be captured

---

### Advanced Operations

#### 34. SETLIST (ABC Format)

**Purpose**: Bulk array assignment

**Register Usage**:
- **R(A)**: Table (input/output)
- **R(A+1)...R(A+B)**: Values to assign (input)
- **C**: Batch number (50 elements per batch)

**Operation**: 
```
for i = 1, B do
    R(A)[C*FPF + i] := R(A+i)  -- Where FPF (Fields Per Flush) = 50 in Lua 5.1
end
```

**Special Cases**: 
- If **B = 0**, use all values up to stack top
- If **C = 0**, next instruction contains the real C value (>255)

**Implementation Note**: This opcode efficiently implements array initialization for tables. The FPF constant (50) is defined in the Lua 5.1 source as `LFIELDS_PER_FLUSH`.

---

#### 37. VARARG (ABC Format)

**Purpose**: Access variable arguments

**Register Usage**:
- **R(A)...R(A+B-2)**: Destination registers (output)
- **B**: Number of values + 1 (0 = copy all varargs)
- **C**: Unused

**Operation**: Copy varargs to R(A)...R(A+B-2)

---

## Metamethod Support

Many Lua 5.1 operations support metamethods to allow user-defined behavior for custom types:

### Arithmetic Metamethods
- **__add**: Addition (ADD opcode)
- **__sub**: Subtraction (SUB opcode)
- **__mul**: Multiplication (MUL opcode)
- **__div**: Division (DIV opcode)
- **__mod**: Modulo (MOD opcode)
- **__pow**: Exponentiation (POW opcode)
- **__unm**: Unary minus (UNM opcode)
- **__concat**: Concatenation (CONCAT opcode)

### Comparison Metamethods
- **__eq**: Equality (EQ opcode)
- **__lt**: Less than (LT opcode)
- **__le**: Less than or equal (LE opcode)

### Table Access Metamethods
- **__index**: Table indexing (GETTABLE, GETGLOBAL)
- **__newindex**: Table assignment (SETTABLE, SETGLOBAL)

### Other Metamethods
- **__len**: Length operator (LEN opcode)
- **__call**: Function call on non-function values (CALL opcode)
- **__tostring**: String conversion (used by tostring function)

Metamethods are queued as operations to maintain non-recursive execution.

## Transaction-Based Implementation

### Core Transaction Pattern

All register operations must follow this pattern:

```rust
fn execute_opcode(vm: &mut VM, inst: Instruction) -> LuaResult<StepResult> {
    // 1. Create transaction
    let mut tx = HeapTransaction::new(&mut vm.heap);
    
    // 2. Read inputs through transaction
    let inputs = read_instruction_inputs(&tx, inst)?;
    
    // 3. Perform computation (pure, no heap access)
    let outputs = compute_results(inputs)?;
    
    // 4. Write outputs through transaction
    write_instruction_outputs(&mut tx, inst, outputs)?;
    
    // 5. Queue any follow-up operations
    if needs_metamethod_call(&outputs) {
        tx.queue_operation(create_metamethod_operation());
    }
    
    // 6. Commit transaction
    tx.commit()?;
    
    Ok(StepResult::Continue)
}
```

### Register Access Methods

```rust
impl<'a> HeapTransaction<'a> {
    /// Read register relative to function base
    pub fn read_register(&self, thread: ThreadHandle, abs_index: usize) -> LuaResult<Value> {
        self.validate_thread(thread)?;
        self.heap.get_thread(thread)?
            .stack
            .get(abs_index)
            .cloned()
            .ok_or(LuaError::StackIndexOutOfBounds)
    }
    
    /// Write register relative to function base  
    pub fn set_register(&mut self, thread: ThreadHandle, abs_index: usize, value: Value) -> LuaResult<()> {
        self.validate_thread(thread)?;
        self.ensure_stack_space(thread, abs_index + 1)?;
        self.pending_register_writes.push((thread, abs_index, value));
        Ok(())
    }
    
    /// Read RK value (register or constant)
    pub fn read_rk(&self, thread: ThreadHandle, base: usize, rk: u32) -> LuaResult<Value> {
        if rk & 0x100 != 0 {
            // Constant
            self.get_constant(thread, (rk & 0xFF) as usize)
        } else {
            // Register
            self.read_register(thread, base + rk as usize)
        }
    }
}
```

### Transaction Safety Rules

1. **No Nested Transactions**: One transaction per VM step
2. **No Direct Heap Access**: All access through transaction methods
3. **Clone on Read**: Always clone values when reading
4. **Queue Complex Operations**: Don't execute recursively
5. **Validate All Handles**: Check handle validity before use

## Rc<RefCell> Migration Strategy

### Current Issues with RefCellVM

The current RefCellVM implementation uses a global RefCell for the entire heap, causing runtime panics when:
- Multiple borrows overlap (e.g., reading while holding a mutable borrow)
- Shared mutable state (upvalues) is accessed from multiple closures
- Complex operations need interleaved access patterns

### Migration to Per-Object Rc<RefCell>

To resolve these issues, migrate to fine-grained Rc<RefCell> for individual objects:

#### 1. Refactor Type Definitions

```rust
// Upvalues become independently borrowable
type UpvalueHandle = Rc<RefCell<UpvalueState>>;
enum UpvalueState {
    Open { stack_index: usize },
    Closed { value: Value },
}

// Tables become independently borrowable
type TableHandle = Rc<RefCell<TableInner>>;
struct TableInner {
    array: Vec<Value>,
    hash: HashMap<HashableValue, Value>,
    metatable: Option<TableHandle>,
}

// Threads become independently borrowable
type ThreadHandle = Rc<RefCell<LuaThread>>;
struct LuaThread {
    stack: Vec<Value>,
    frames: Vec<CallFrame>,
    open_upvalues: Vec<UpvalueHandle>,
}

// Closures contain shared upvalue references
struct Closure {
    proto: FunctionProtoHandle,
    upvalues: Vec<UpvalueHandle>, // Rc allows sharing
}
```

#### 2. Update Access Patterns

```rust
// Instead of global borrow
let closure = self.heap.get_closure(handle)?; // borrows entire heap

// Use local borrow
let closure_rc = self.closures.get(handle).clone();
let closure = closure_rc.borrow(); // only borrows this closure

// For upvalue access
let upvalue_rc = closure.upvalues[idx].clone();
let upvalue = upvalue_rc.borrow();
match &*upvalue {
    UpvalueState::Open { stack_index } => {
        let thread = self.current_thread.borrow();
        thread.stack[*stack_index].clone()
    }
    UpvalueState::Closed { value } => value.clone(),
}
```

#### 3. Benefits

- **No Global Lock**: Each object borrows independently
- **Safe Upvalue Sharing**: Multiple closures can share upvalues via Rc
- **Reduced Conflicts**: Operations can borrow different objects simultaneously
- **Clear Ownership**: Rc makes sharing explicit

#### 4. Implementation Priority

1. **Upvalues First**: Most critical for closure functionality
2. **Tables Next**: Enable independent table operations
3. **Threads**: Allow concurrent access to different threads
4. **Retain Queuing**: Keep non-recursive execution model

## Register Window History

### Why Register Windows Failed

The original design attempted to use 256-register windows for each function:

```rust
// FAILED DESIGN - DO NOT USE
struct RegisterWindow {
    registers: [Value; 256],
    base: usize,  // Absolute stack position
}
```

**Problems Encountered**:

1. **Stack Discontinuity**: Lua bytecode assumes arithmetic on register indices works across function boundaries

2. **Upvalue Corruption**: Upvalues store absolute stack indices, windows broke this assumption

3. **C API Incompatibility**: Entire C API assumes contiguous stack access

4. **Performance Overhead**: Window allocation/deallocation was expensive

### Unified Stack Solution

The current design uses a simple, flat stack model:

```rust
pub struct LuaThread {
    stack: Vec<Value>,           // Single contiguous stack
    frames: Vec<CallFrame>,      // Call frame stack
}

pub struct CallFrame {
    closure: ClosureHandle,      // Currently executing closure
    base_register: u16,          // Base for this function's registers
    pc: usize,                   // Program counter
}
```

**Benefits**:
- Direct compatibility with Lua 5.1 semantics
- Simple register arithmetic
- Efficient memory access patterns
- Natural upvalue implementation

### Lessons Learned

1. **Respect the Original Design**: Lua's stack model is fundamental to its semantics
2. **Premature Abstraction**: Register windows added complexity without benefits
3. **Transaction Safety Works**: The transaction layer provides safety without changing semantics
4. **Simplicity Wins**: The flat stack is both faster and more correct

## Appendix: Quick Reference Table

| Op | Name      | Format | Registers Used           | Operation                           |
|----|-----------|--------|--------------------------|-------------------------------------|
| 0  | MOVE      | ABC    | R(A)←R(B)               | R(A) := R(B)                        |
| 1  | LOADK     | ABx    | R(A)←K(Bx)              | R(A) := Kst(Bx)                     |
| 2  | LOADBOOL  | ABC    | R(A)←bool               | R(A) := (Bool)B; if (C) pc++        |
| 3  | LOADNIL   | ABC    | R(A)...R(B)←nil         | R(A) := ... := R(B) := nil          |
| 4  | GETUPVAL  | ABC    | R(A)←U[B]               | R(A) := UpValue[B]                  |
| 5  | GETGLOBAL | ABx    | R(A)←G[K(Bx)]           | R(A) := Gbl[Kst(Bx)]                |
| 6  | GETTABLE  | ABC    | R(A)←R(B)[RK(C)]        | R(A) := R(B)[RK(C)]                 |
| 7  | SETGLOBAL | ABx    | G[K(Bx)]←R(A)           | Gbl[Kst(Bx)] := R(A)                |
| 8  | SETUPVAL  | ABC    | U[A]←R(B)               | UpValue[A] := R(B)                  |
| 9  | SETTABLE  | ABC    | R(A)[RK(B)]←RK(C)       | R(A)[RK(B)] := RK(C)                |
| 10 | NEWTABLE  | ABC    | R(A)←{}                 | R(A) := {} (size = B,C)             |
| 11 | SELF      | ABC    | R(A),R(A+1)←R(B)[RK(C)] | R(A+1):=R(B); R(A):=R(B)[RK(C)]    |
| 12 | ADD       | ABC    | R(A)←RK(B)+RK(C)        | R(A) := RK(B) + RK(C)               |
| 13 | SUB       | ABC    | R(A)←RK(B)-RK(C)        | R(A) := RK(B) - RK(C)               |
| 14 | MUL       | ABC    | R(A)←RK(B)*RK(C)        | R(A) := RK(B) * RK(C)               |
| 15 | DIV       | ABC    | R(A)←RK(B)/RK(C)        | R(A) := RK(B) / RK(C)               |
| 16 | MOD       | ABC    | R(A)←RK(B)%RK(C)        | R(A) := RK(B) % RK(C)               |
| 17 | POW       | ABC    | R(A)←RK(B)^RK(C)        | R(A) := RK(B) ^ RK(C)               |
| 18 | UNM       | ABC    | R(A)←-R(B)              | R(A) := -R(B)                       |
| 19 | NOT       | ABC    | R(A)←not R(B)           | R(A) := not R(B)                    |
| 20 | LEN       | ABC    | R(A)←#R(B)              | R(A) := length of R(B)              |
| 21 | CONCAT    | ABC    | R(A)←R(B)..R(C)         | R(A) := R(B).. ... ..R(C)           |
| 22 | JMP       | AsBx   | pc+=sBx                 | pc += sBx                           |
| 23 | EQ        | ABC    | RK(B)==RK(C)            | if ((RK(B)==RK(C))~=A) then pc++    |
| 24 | LT        | ABC    | RK(B)<RK(C)             | if ((RK(B)<RK(C))~=A) then pc++     |
| 25 | LE        | ABC    | RK(B)<=RK(C)            | if ((RK(B)<=RK(C))~=A) then pc++    |
| 26 | TEST      | ABC    | R(A)                    | if not (R(A) <=> C) then pc++       |
| 27 | TESTSET   | ABC    | R(B)→R(A)               | if (R(B) <=> C) then R(A):=R(B) else pc++ |
| 28 | CALL      | ABC    | R(A)..←R(A)(R(A+1)..)   | R(A), ..., R(A+C-2) := R(A)(R(A+1), ..., R(A+B-1)) |
| 29 | TAILCALL  | ABC    | return R(A)(R(A+1)..)   | return R(A)(R(A+1), ... ,R(A+B-1))  |
| 30 | RETURN    | ABC    | return R(A)..           | close upvalues >= base; return R(A), ... ,R(A+B-2) |
| 31 | FORLOOP   | AsBx   | R(A)+=R(A+2)            | R(A)+=R(A+2); if (step>0 ? R(A)<=R(A+1) : R(A)>=R(A+1)) then {R(A+3)=R(A); pc+=sBx} |
| 32 | FORPREP   | AsBx   | R(A)-=R(A+2)            | R(A)-=R(A+2); pc+=sBx               |
| 33 | TFORLOOP  | ABC    | R(A+3)..←R(A)(R(A+1)..) | R(A+3), ..., R(A+2+C) := R(A)(R(A+1), R(A+2)) |
| 34 | SETLIST   | ABC    | R(A)[i]←R(A+i)          | R(A)[(C-1)*FPF+i] := R(A+i), 1 <= i <= B |
| 35 | CLOSE     | ABC    | close upvalues          | close all upvalues >= R(A)          |
| 36 | CLOSURE   | ABx    | R(A)←closure            | R(A) := closure(KPROTO[Bx], R(A), ... ,R(A+n)) |
| 37 | VARARG    | ABC    | R(A)..←varargs          | R(A), R(A+1), ..., R(A+B-2) = vararg |