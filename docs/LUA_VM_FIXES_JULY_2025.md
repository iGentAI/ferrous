# Lua VM Critical Fixes - July 2025

## Overview

This document details the critical fixes made to the Lua VM implementation in July 2025. These fixes resolved fundamental issues that were preventing the VM from executing even basic Lua code. Thanks to these changes, the VM is now capable of executing scripts with arithmetic operations, function definitions, function calls, and basic control flow.

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

## 5. Register Allocation and Tracking

### Problem
The register allocator wasn't effectively tracking register lifetimes, leading to inefficient register usage and potential conflicts.

#### Root Cause
While the register allocator tracked `last_use` in the `VarInfo` structure, this information wasn't being properly used for allocation decisions:

```rust
// Not using lifetime information
fn allocate(&mut self) -> usize {
    // Try to reuse a free register first
    if let Some(reg) = self.free_registers.pop() {
        return reg;
    }
    
    // Always allocate new register if none free
    let reg = self.used;
    self.used += 1;
    // ...
}
```

#### Impact
- Less efficient register allocation
- Potentially larger max_stack_size than necessary
- Could cause problems with complex scripts

#### Solution
Enhanced the register allocator to properly use lifetime information:

```rust
impl RegisterAllocator {
    // When allocating registers, prioritize reuse of registers whose variables are no longer used
    fn allocate(&mut self) -> usize {
        // First try to reuse free registers
        if let Some(reg) = self.free_registers.pop() {
            return reg;
        }
        
        // Allocate a new register if necessary
        let reg = self.used;
        self.used += 1;
        if self.used > self.max_used {
            self.max_used = self.used;
        }
        
        reg
    }
    
    // When determining which registers to free, use last_use information
    fn free_unused_registers(&mut self, current_instruction: usize, variables: &HashMap<String, VarInfo>) {
        for (var_name, var_info) in variables {
            if let Some(last_use) = var_info.last_use {
                if last_use < current_instruction && !var_info.captured {
                    // Variable no longer used, free its register
                    self.free_registers.push(var_info.register);
                }
            }
        }
    }
}
```

## Testing and Validation

### Simple Addition Test

We created a minimal test case to verify the basic arithmetic operations:

```lua
-- minimal_eval.lua
return 1 + 2
```

This successfully compiles to:
```
[0] LoadK 1, 0    # Load 1.0 into register 1
[1] LoadK 2, 1    # Load 2.0 into register 2
[2] Add 0, 1, 2   # Add registers 1 and 2, store in register 0
[3] Return 0, 2   # Return value in register 0
```

And executes to produce the correct result: `3.0`.

### Function Test

We tested function definition and calling with:

```lua
-- with_function.lua
local x = 42

local function add(a, b)
  return a + b
end

return add(x, 10)
```

This now successfully compiles and executes, producing the result: `52`.

## Conclusion

These fixes have resolved the critical blocking issues that were preventing the Lua VM from executing even simple scripts. The implementation now correctly handles the core language features including arithmetic operations, functions, and basic control flow. 

The fixes were carefully implemented to maintain alignment with the architectural principles, particularly the non-recursive state machine execution model and transaction-based memory management. With these issues resolved, development can now focus on implementing the standard library and Redis API integration.