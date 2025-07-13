# Register Allocation Contract

This document defines how registers are allocated by the compiler and used by the VM.
Both components MUST follow these rules exactly.

## Core Principles

1. **Window Isolation**: Each function call gets a new register window
2. **Register Preservation**: Parent registers must not be modified by children
3. **Consistent Indexing**: Register N in compiler = Register N in VM window

## Compiler Register Allocation Rules

### Function Calls
When compiling `func(arg1, arg2)`:
1. Allocate R(N) for function
2. Mark R(N) as preserved
3. Allocate R(N+1), R(N+2) for arguments  
4. Do NOT reuse R(N) until after CALL instruction
5. Results overwrite starting at R(N)

### Loops (TFORLOOP)
When compiling `for k,v in iter do`:
1. Base = current register level
2. R(Base) = iterator function (MUST preserve)
3. R(Base+1) = state
4. R(Base+2) = control variable
5. R(Base+3), R(Base+4) = loop variables k,v
6. R(Base+5) = storage for iterator (if 2 loop vars)

### Register Lifetime Tracking
```rust
compiler.registers.allocate();        // Get new register
compiler.registers.preserve(reg);     // Prevent reuse
compiler.registers.release(reg);      // Allow reuse
compiler.registers.level();          // Current allocation level
```

## VM Register Usage Rules

### Window Allocation
```rust
let window = register_windows.allocate_window(size)?;
// Window index != register index
// Register access: register_windows.get_register(window, register_index)
```

### Register Protection for CALL
```rust
let guard = register_windows.protect_call_registers(window, func_reg, arg_count)?;
// Function register protected during argument evaluation
```

### Register Protection for TFORLOOP  
```rust
let guard = register_windows.protect_tforloop_registers(window, base, var_count)?;
// Iterator, state, control, and storage registers protected
```

## Verification Points

Before ANY register operation:
- [ ] Compiler: Is this register still allocated?
- [ ] Compiler: Is this register preserved?
- [ ] VM: Is this register in window bounds?
- [ ] VM: Is this register protected?
- [ ] Both: Do we agree on what this register contains?

## Common Mistakes to Avoid

1. **Compiler**: Reusing function register before CALL completes
2. **Compiler**: Not preserving iterator in TFORLOOP  
3. **VM**: Not saving iterator before call in TFORLOOP
4. **VM**: Writing to protected registers
5. **Both**: Mismatched understanding of register contents