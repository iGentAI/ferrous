# Debugging Compiler-VM Coherence Issues

When Lua code behaves incorrectly, it's often due to compiler-VM misalignment.
This guide helps identify and fix these issues.

## Quick Diagnosis

### Symptoms of Misalignment

1. **Type errors at runtime**: "expected function, got string"
   - Usually means registers are misaligned
   - Check: Does compiler put function where VM expects it?

2. **Nil values in loop variables**
   - TFORLOOP not updating loop variables
   - Check: Is VM copying all results or just control?

3. **Functions getting overwritten**
   - Register reuse issues  
   - Check: Is compiler preserving the function register?

4. **Wrong number of results**
   - CALL result count mismatch
   - Check: Do compiler and VM agree on C parameter?

## Debugging Process

### Step 1: Trace the Bytecode
```bash
# Add debug prints to compiler
println!("Generated: {:?} A={} B={} C={}", opcode, a, b, c);
```

### Step 2: Trace the Execution
```rust
// In VM step() function
println!("Executing: {:?} A={} B={} C={}", opcode, a, b, c);
println!("Window {}: R({})={:?}", window_idx, a, self.register_windows.get_register(window_idx, a));
```

### Step 3: Compare Register States

**Compiler View**:
```
R(0) = iterator_func
R(1) = state  
R(2) = control
R(3) = key (loop var)
R(4) = value (loop var)
```

**VM View** (print actual values):
```
R(0) = Closure(0x123)
R(1) = Table(0x456)
R(2) = Number(0)
R(3) = Nil        <-- MISMATCH! Should be Number(1)
R(4) = Nil        <-- MISMATCH! Should be String("hello")
```

### Step 4: Find the Divergence

Check these critical points:
1. **Compiler**: What bytecode did it generate?
2. **VM**: How did it interpret that bytecode?
3. **Register Windows**: Are we in the right window?
4. **Protection**: Are registers properly protected?

## Common Misalignments and Fixes

### TFORLOOP Misalignment

**Wrong**: Compiler emits CALL + TFORLOOP
```
CALL R(0) 3 3      // Calls iterator
TFORLOOP R(0) 0 2  // Just checks nil
```

**Right**: Single TFORLOOP instruction
```
TFORLOOP R(0) 2    // Calls AND updates loop vars
```

**Fix**: Update compiler to emit single instruction, update VM to handle call internally

### Register Overwrite

**Wrong**: Compiler reuses R(0) during call
```rust
self.expression(&call.function, 0, 1)?;  // R(0) = function
self.expression(&arg1, 0, 1)?;          // R(0) = arg (OVERWRITES!)
```

**Right**: Preserve function register
```rust
let func_reg = self.registers.allocate();
self.registers.preserve_register(func_reg);
self.expression(&call.function, func_reg, 1)?;
```

## Verification Tools

### 1. Bytecode Dumper
```rust
fn dump_bytecode(instructions: &[u32]) {
    for (i, instr) in instructions.iter().enumerate() {
        let (opcode, a, b, c) = decode_instruction(*instr);
        println!("{}: {} A={} B={} C={}", i, opcode, a, b, c);
    }
}
```

### 2. Register State Dumper
```rust
fn dump_registers(windows: &RegisterWindowSystem, window: usize) {
    println!("=== Window {} ===", window);
    for i in 0..10 {
        let val = windows.get_register(window, i).unwrap_or(&Value::Nil);
        println!("R({}) = {:?}", i, val);
    }
}
```

### 3. Execution Tracer
Enable with environment variable:
```bash
LUA_TRACE=1 cargo run script.lua
```

## Prevention

1. **Never** change opcodes in isolation
2. **Always** update contracts first
3. **Always** test with register dumps enabled
4. **Review** compiler and VM changes together