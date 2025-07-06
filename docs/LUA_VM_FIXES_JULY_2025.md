# Lua VM Critical Fixes - July 2025

## Overview

This document details the critical fixes made to the Lua VM implementation in July 2025. These fixes resolved fundamental issues that were preventing the VM from executing even basic Lua code. Thanks to these changes, the VM is now capable of executing scripts with arithmetic operations, function definitions, function calls, basic control flow, and string concatenation.

## 1. Bytecode Encoding Fix

### Problem
The bytecode generator was producing incorrect opcodes due to a fundamental issue in the encoding strategy.

#### Root Cause
The `encode_ABC`, `encode_ABx`, and `encode_AsBx` functions were directly casting the `OpCode` enum to an integer:

```rust
fn encode_ABC(opcode: OpCode, a: u8, b: u16, c: u16) -> u32 {
    let op = opcode as u32 & 0x3F; // Direct enum casting
    let a = (a as u32) << 6;
    let c = (c as u32) << 14;
    let b = (b as u32) << 23;
    
    op | a | c | b
}
```

This resulted in opcodes being encoded based on their position in the enum declaration rather than their actual opcode values. For example:
- `Add` (which should be opcode 12) was encoded as opcode 13
- `Return` (which should be opcode 30) was encoded as opcode 31 (ForPrep)

#### Impact
- Addition operations were executed as subtraction
- Return statements were executed as ForPrep, causing out of bounds errors

#### Solution
Modified the encoding functions to use the proper mapping function `opcode_to_u8`:

```rust
fn encode_ABC(opcode: OpCode, a: u8, b: u16, c: u16) -> u32 {
    let op = opcode_to_u8(opcode) as u32 & 0x3F; // Fixed: Use mapping function
    let a = (a as u32) << 6;
    let c = (c as u32) << 14;
    let b = (b as u32) << 23;
    
    op | a | c | b
}
```

This ensures that opcodes are encoded with the correct numeric value that corresponds to the VM's expectations.

## 2. Stack Management Fix

### Problem
The VM was throwing "Stack index X out of bounds" errors when executing instructions that access registers.

#### Root Cause
The VM didn't properly initialize stack space before executing a function. The architecture requires that all registers a function might access must be pre-allocated, but this wasn't happening for the main function execution path.

```rust
// Initial implementation - missing stack space initialization
pub fn execute_module(&mut self, module: &Module, args: &[Value]) -> LuaResult<Value> {
    // Load module and create closure
    // ...
    
    // Execute without ensuring stack space - ERROR!
    self.execute_function(closure_handle, args)
}
```

#### Impact
- Stack access errors when executing even simple code
- Unpredictable behavior due to out-of-bounds memory access
- "Stack index out of bounds" errors in the logs

#### Solution
Implemented proper stack initialization before function execution:

```rust
pub fn execute_module(&mut self, module: &Module, args: &[Value]) -> LuaResult<Value> {
    // ... existing code ...
    
    // CRITICAL FIX: Ensure the stack has sufficient space
    let needed_stack_size = proto.max_stack_size as usize + 10; // Safety margin
    
    if current_stack_size < needed_stack_size {
        // Add Nil values to extend the stack
        for i in current_stack_size..needed_stack_size {
            tx.push_stack(thread_handle, Value::Nil)?;
        }
    }
    
    // ... rest of function ...
}
```

Also improved safety of register access with automatic stack growth:

```rust
pub(crate) fn set_thread_register_internal(&mut self, thread: ThreadHandle, index: usize, value: Value) -> LuaResult<()> {
    let thread_obj = self.get_thread_mut(thread)?;
    
    // If index is out of bounds, grow the stack
    if index >= thread_obj.stack.len() {
        let additional_needed = index + 1 - thread_obj.stack.len();
        thread_obj.stack.reserve(additional_needed);
        
        // Fill with Nil values
        for _ in 0..additional_needed-1 {
            thread_obj.stack.push(Value::Nil);
        }
        
        // Add the target value at index
        thread_obj.stack.push(value);
        return Ok(());
    }
    
    // Normal case - set value in existing slot
    thread_obj.stack[index] = value;
    Ok(())
}
```

## 3. Function Prototype Handling Fix

### Problem
Scripts with function definitions were failing with "Invalid function prototype index: 0" errors.

#### Root Cause
Two critical issues were identified:
1. The `CompleteCompilationOutput` structure didn't properly transfer function prototypes from the CodeGenerator to the resulting module
2. The prototype reference handling in the loader couldn't handle forward references between prototypes

#### Impact
- Compiler could generate function prototypes, but they were lost during module construction
- VM would fail when trying to load nested functions
- "Invalid function prototype index" errors

#### Solution
1. Fixed module creation to properly propagate function prototypes:

```rust
pub fn generate_bytecode(chunk: &Chunk) -> LuaResult<CompleteCompilationOutput> {
    let mut generator = CodeGenerator::new();
    
    // Generate bytecode
    let main = generator.generate(chunk)?;
    
    // Get the strings AFTER generation
    let strings = generator.strings;
    
    // Return all necessary data, including prototypes
    Ok(CompleteCompilationOutput {
        main,  // Main now includes prototypes
        strings,
    })
}
```

2. Implemented a two-pass loader for function prototypes:

```rust
// First pass - create all prototypes with placeholder constants
let mut proto_handles = Vec::with_capacity(module.prototypes.len());
// Create all prototypes with Nil for function prototype references

// Second pass - update function prototype references
for (i, constants) in proto_constants.iter().enumerate() {
    // Update constants that are function prototype references
    for (j, constant) in constants.iter().enumerate() {
        if let CompilationConstant::FunctionProto(proto_idx) = constant {
            proto.constants[j] = Value::FunctionProto(proto_handles[*proto_idx]);
        }
    }
}
```

This approach ensures that all function prototypes are created first, then their references to each other are resolved in a second pass.

## 4. Parser Function Body Fix

### Problem
The parser was throwing syntax errors when parsing functions with return statements: "Expected end after function body: expected End, got Return".

#### Root Cause
The parser's `check_block_end()` method incorrectly treated `Return` tokens as block terminators rather than as regular statements that can appear within blocks:

```rust
// Incorrect implementation
fn check_block_end(&self) -> bool {
    self.check(&Token::End) ||
    self.check(&Token::Else) ||
    self.check(&Token::ElseIf) ||
    self.check(&Token::Until) ||
    self.check(&Token::Eof) ||
    self.check(&Token::Return)  // This is wrong! Return is a statement
}
```

This caused the parser to stop parsing when it encountered a return statement, leaving the return unparsed.

#### Impact
- Functions with return statements couldn't be parsed
- Even simple functions would fail with syntax errors
- "Expected End, got Return" error in parser

#### Solution
1. Removed `Return` from the block terminators:

```rust
// Fixed implementation
fn check_block_end(&self) -> bool {
    self.check(&Token::End) ||
    self.check(&Token::Else) ||
    self.check(&Token::ElseIf) ||
    self.check(&Token::Until) ||
    self.check(&Token::Eof)
    // Removed Token::Return from here
}
```

2. Added proper handling of Return in the `statement()` method:

```rust
// Handle return statement in function bodies
else if self.match_token(Token::Return) {
    // Parse return values
    let mut expressions = Vec::new();
    
    if !self.check_statement_end() {
        expressions.push(self.expression()?);
        
        while self.match_token(Token::Comma) {
            expressions.push(self.expression()?);
        }
    }
    
    // Optional semicolon
    self.match_token(Token::Semicolon);
    
    Ok(Statement::Return { expressions })
}
```

3. Added a `Return` variant to the `Statement` enum to represent return statements in the AST.

## 5. Register Allocation Architecture Fix

### Problem
The compiler's register allocation system was fundamentally mismatched with the VM's execution model, leading to register conflicts particularly for nested expressions and function calls.

#### Root Cause
The register allocator's `free_to()` method was completely resetting allocation state:
```rust
fn free_to(&mut self, level: usize) {
    // Add all registers above level to free list
    for reg in level..self.used {
        if !self.free_registers.contains(&reg) {
            self.free_registers.push(reg);
            
            // Also remove from variable mapping if present
            self.register_to_variable.remove(&reg);
        }
    }
    
    // Update used register count - THIS IS THE ACTUAL PROBLEM
    self.used = level;  // This completely resets allocation state!
    
    // Sort free registers for better allocation patterns
    self.free_registers.sort_unstable();
}
```

This caused registers to be prematurely freed when they were still needed by the VM, especially during nested expressions like function calls with concatenation arguments (`print(a .. b)`).

#### Impact
- Register conflicts between parent and child contexts
- Function handle being overwritten by concatenation result
- Type errors when executing seemingly valid code 
- Most notably: "expected function, got string" errors when passing concatenation results to functions

#### Solution
Implemented a proper register lifetime tracking system:

1. Enhanced RegisterAllocator to track which registers need preservation:
```rust
/// Register allocator with lifetime tracking
struct RegisterAllocator {
    // Existing fields...
    
    /// Registers that should be preserved during state restoration
    preserved_registers: HashSet<usize>,
}

impl RegisterAllocator {
    /// Mark a register to be preserved when restoring state
    fn preserve_register(&mut self, reg: usize) {
        self.preserved_registers.insert(reg);
    }
    
    /// Restore the allocation state to a previously saved state
    fn restore_state(&mut self, saved_state: usize) {
        // Add all registers allocated since saved_state to the free list
        // EXCEPT those marked as preserved
        for reg in saved_state..self.used {
            if !self.preserved_registers.contains(&reg) && !self.free_registers.contains(&reg) {
                self.free_registers.push(reg);
                self.register_to_variable.remove(&reg);
            }
        }
        
        // Reset allocation pointer to saved state
        self.used = saved_state;
        
        // Sort free registers
        self.free_registers.sort_unstable();
    }
}
```

2. Updated all compiler operation handlers to use this system consistently:
   - **Binary operations**: Preserved operand registers during expression evaluation
   ```rust
   // Mark the operand registers as preserved
   self.registers.preserve_register(left_reg);
   self.registers.preserve_register(right_reg);
   ```
   
   - **Function calls**: Preserved function register during argument evaluation 
   ```rust
   // CRITICAL: Preserve the function register
   // This ensures it won't be overwritten during argument evaluation
   self.registers.preserve_register(func_reg);
   ```
   
   - **Table operations**: Preserved all needed registers across operations
   ```rust
   // CRITICAL: Preserve the table register
   self.registers.preserve_register(table_reg);
   ```

3. Modified the VM's CONCAT handler to properly handle register values:
   ```rust
   // Create a vector of all operand register values first
   // This ensures we don't modify any registers until after reading them all
   let mut operand_values = Vec::with_capacity((c - b + 1) as usize);
   for i in b..=c {
       let value = tx.read_register(self.current_thread, base + i)?;
       operand_values.push(value);
   }
   ```

This solution follows standard compiler techniques for register allocation, ensuring that registers aren't freed until they're truly no longer needed.

## 6. CONCAT Operation Handling Fix

### Problem
The CONCAT opcode was incorrectly implemented as a purely deferred operation, which didn't align with the VM's register-based execution model.

#### Root Cause
The VM was incorrectly classifying all string concatenation operations as deferred operations, even when metamethod invocation wasn't needed:

```rust
// Old implementation always queued a pending operation
tx.queue_operation(PendingOperation::Concatenation {
    values: values.clone(),
    current_index: 0,
    dest_register: frame.base_register + a as u16,
    accumulated: Vec::new(),
})?;
```

This broke the code when register values were needed immediately after the CONCAT instruction.

#### Impact
- Register state was inconsistent after CONCAT operations
- Functions receiving concatenated strings would get incorrect values
- "expected function, got string" errors in common patterns like `print(a .. b)`

#### Solution
Updated CONCAT to use a hybrid approach that distinguishes between immediate and deferred execution paths:

```rust
// Determine if we need metamethod handling
let mut needs_metamethod = false;
let mut mm_index = 0;

for (i, value) in operand_values.iter().enumerate() {
    // Only defer for actual metamethods, not just any non-string value
    if let Some(_) = crate::lua::metamethod::resolve_metamethod(
        &mut tx, value, crate::lua::metamethod::MetamethodType::Concat
    )? {
        needs_metamethod = true;
        mm_index = i;
        break;
    }
    
    // Check for __tostring metamethod too
    // ...
}

if needs_metamethod {
    // Use the Pending Operation system for metamethod handling
    tx.queue_operation(PendingOperation::Concatenation {
        values: operand_values,
        current_index: mm_index,
        dest_register: frame.base_register + a as u16,
        accumulated: Vec::new(),
    })?;
} else {
    // We can concatenate immediately 
    let mut result = String::new();
    
    // Process all values right away
    for value in &operand_values {
        // String conversion logic...
    }
    
    // Create the final string
    let string_handle = tx.create_string(&result)?;
    tx.set_register(self.current_thread, base + a, Value::String(string_handle))?;
}
```

This ensures that simple concatenations complete immediately, while complex ones that need metamethods still use the pending operation system.

## Conclusion

These fixes have resolved the critical blocking issues that were preventing the Lua VM from executing even moderately complex scripts. The implementation now correctly handles core language features including arithmetic operations, functions, control flow, and concatenation operations with nested expressions. 

With these architectural fixes in place, the focus can now shift to completing the standard library, improving error handling, implementing garbage collection, and eventually integrating with Redis. There are still placeholder implementations throughout the codebase that will need to be addressed, but the core VM architecture is now sound.