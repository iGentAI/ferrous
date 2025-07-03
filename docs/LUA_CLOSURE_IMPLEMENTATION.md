# Lua VM Closure System Implementation Guide

## Current Implementation Status

The closure system in the Ferrous Lua VM is currently in an **incomplete state** with several placeholder implementations. This document details the current limitations and provides guidance for a complete implementation that adheres to the architectural principles.

### Critical Issues in Current Implementation

1. **Closure Opcode Creates Dummy Closures**:
   ```rust
   // Current implementation (simplified):
   let new_closure = crate::lua::value::Closure {
       proto: crate::lua::value::FunctionProto {
           bytecode: vec![0x40000001], // Return nil for now
           constants: vec![],
           num_params: 0,
           is_vararg: false,
           max_stack_size: 2,
           upvalues: vec![],
       },
       upvalues: vec![],
   };
   ```
   This creates an empty closure rather than using the actual function prototype from constants.

2. **No Upvalue Instruction Processing**:
   After a Closure opcode, Lua bytecode includes instructions that tell the VM how to capture upvalues. The current implementation does not process these instructions.

3. **No Thread-Wide Upvalue Management**:
   The Close opcode only handles upvalues from the current closure, not all open upvalues that reference the closing stack region.

4. **Incomplete Upvalue Lifecycle Handling**:
   While GetUpval and SetUpval opcodes follow the two-phase pattern, they don't fully integrate with a proper upvalue lifecycle management system.

## Requirements for Full Implementation

### 1. Function Prototypes in Constants

In Lua, closures are created from function prototypes stored in the constants table:

```rust
// Needs to be implemented:
let proto = match &closure_obj.proto.constants[bx] {
    Value::FunctionProto(proto) => proto.clone(),
    _ => return Err(LuaError::TypeError {
        expected: "function prototype".to_string(),
        got: closure_obj.proto.constants[bx].type_name().to_string(),
    }),
};
```

* Add support for function prototypes as a value type
* Implement proto serialization in the compiler

### 2. Upvalue Instruction Processing

After every Closure instruction, Lua bytecode includes instructions that indicate how to initialize each upvalue. These can be:

1. **Move-up instructions** - capture a local variable as an upvalue
2. **Get-parent instructions** - reuse an upvalue from the parent closure

```rust
// Needs to be implemented:
for i in 0..proto.upvalues.len() {
    // Read next instruction (this would actually be VM reading next bytecode)
    let upvalue_instr = get_next_instruction();
    
    let upvalue_handle = if upvalue_instr.is_move_up() {
        // Create upvalue pointing to stack slot
        let stack_idx = upvalue_instr.index();
        create_upvalue(Some(stack_idx), None)
    } else {
        // Use parent's upvalue
        let upvalue_idx = upvalue_instr.index();
        parent_closure.upvalues[upvalue_idx]
    };
    
    upvalues.push(upvalue_handle);
}
```

### 3. Thread-Wide Upvalue Management

The Thread struct should maintain a list of all open upvalues, sorted by stack index:

```rust
pub struct Thread {
    pub call_frames: Vec<CallFrame>,
    pub stack: Vec<Value>,
    pub status: ThreadStatus,
    pub open_upvalues: Vec<UpvalueHandle>,  // Should be added
}
```

This enables:
* Finding upvalues for the same variable across closures
* Properly closing upvalues when variables go out of scope
* Closing upvalues in order from highest to lowest stack index

### 4. Complete Close Opcode Implementation

A proper Close opcode implementation should:
1. Find all upvalues that reference stack slots >= the threshold
2. Close each upvalue (capture current value and detach from stack)
3. Remove closed upvalues from the thread's open_upvalues list

```rust
// Conceptual implementation (needs proper transaction handling):
let close_threshold = frame.base_register as usize + a;

// Find upvalues to close
let mut indices_to_remove = Vec::new();
for (i, upvalue_handle) in thread.open_upvalues.iter().enumerate() {
    let upvalue = get_upvalue(upvalue_handle);
    
    if let Some(stack_idx) = upvalue.stack_index {
        if stack_idx >= close_threshold {
            // Read current value
            let value = read_register(thread, stack_idx);
            
            // Close upvalue
            close_upvalue(upvalue_handle, value);
            
            // Mark for removal from open_upvalues list
            indices_to_remove.push(i);
        }
    }
}

// Remove closed upvalues from list
for idx in indices_to_remove.iter().rev() {
    thread.open_upvalues.remove(*idx);
}
```

### 5. Upvalue Reuse Optimization

When a new upvalue is created pointing to a stack slot, check if an upvalue for that slot already exists and reuse it:

```rust
fn find_or_create_upvalue(thread: ThreadHandle, stack_idx: usize) -> UpvalueHandle {
    // First check if an upvalue already exists for this stack slot
    for upvalue_handle in thread.open_upvalues {
        let upvalue = get_upvalue(upvalue_handle);
        if upvalue.stack_index == Some(stack_idx) {
            return upvalue_handle;
        }
    }
    
    // Create a new upvalue if none exists
    let upvalue = Upvalue {
        stack_index: Some(stack_idx),
        value: None,
    };
    
    let handle = create_upvalue(upvalue);
    
    // Add to thread's open upvalues list
    thread.open_upvalues.push(handle);
    
    // Return the new upvalue
    handle
}
```

## Implementation Approach using Transaction Pattern

The challenge with closure implementation is managing the complex relationships between handles without violating Rust's borrow rules. 

### Two-Phase Pattern for Upvalue Operations

All upvalue operations should follow the two-phase pattern:

```rust
// Phase 1: Extract needed information
let upvalue_info = {
    let upvalue = tx.get_upvalue(upvalue_handle)?;
    (upvalue.stack_index, upvalue.value.clone())
};

// Phase 2: Process information
match upvalue_info {
    (Some(idx), None) => {
        // Open upvalue
        let value = tx.read_register(thread, idx)?;
        // Process value...
    }
    (None, Some(val)) => {
        // Closed upvalue
        // Process value...
    }
    _ => return Err(LuaError::RuntimeError("Invalid upvalue state")),
}
```

### Transaction Boundaries

When processing upvalue instructions, each individual upvalue should be handled in its own transaction to avoid having long-lived borrows. For example:

```rust
// Process each upvalue in its own transaction
for i in 0..proto.upvalues.len() {
    let upvalue_handle = {
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Read upvalue instruction (simplified)
        let upvalue_instr = tx.get_instruction(frame.closure, frame.pc + 1 + i)?;
        
        // Process based on instruction type
        let handle = if upvalue_instr.opcode() == OpCode::Move {
            // Create or find upvalue for local variable
            find_or_create_upvalue(tx, thread, base + upvalue_instr.b())?
        } else {
            // Get upvalue from parent
            let parent_upvalues = tx.get_closure_upvalues(frame.closure)?;
            parent_upvalues[upvalue_instr.b()]  // Simplification
        };
        
        tx.commit()?;
        handle
    };
    
    upvalues.push(upvalue_handle);
}
```

## Testing Requirements

Full closure implementation should include tests for:

1. **Basic Closure Creation**: Verify closures are created with correct prototype
2. **Upvalue Capture**: Test capturing local variables from outer scopes
3. **Nested Closures**: Verify closures inside closures work correctly
4. **Shared Upvalues**: Test multiple closures sharing the same upvalue
5. **Upvalue Closing**: Verify upvalues are properly closed when variables go out of scope

## Implementation Strategy

1. **Extend Value System**:
   - Add function prototype as a value type
   - Update the compiler to generate proper function prototypes

2. **Enhance Thread Structure**:
   - Add open_upvalues field
   - Implement upvalue management functions

3. **Update Closure Opcode**:
   - Extract actual function prototype from constants
   - Process upvalue instructions correctly
   - Create closure with proper upvalues

4. **Enhance Close Opcode**:
   - Close all upvalues for variables going out of scope
   - Update thread's open_upvalues list

5. **Refine GetUpval/SetUpval**:
   - Ensure they handle both open and closed upvalues correctly
   - Integrate with thread-wide upvalue management

This implementation will require a dedicated session focused solely on the closure system to ensure all components work together correctly and maintain the architectural integrity of the VM.