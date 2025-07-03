# Lua VM Core Completion Plan

This document outlines the approach for completing the core Lua VM implementation, focusing on the remaining gaps and placeholders. It provides a structured roadmap for finishing the VM before integrating with other components.

## Current Implementation Status

- **Core VM Architecture**: ~90% implemented (arena, handle, heap, transaction systems)
- **Basic VM Operations**: ~70% implemented (arithmetic, control flow, table access)
- **Advanced VM Features**: ~30% implemented (closures, upvalues, metamethods)

## Completion Priorities

### 1. Closure System (Highest Priority)

#### Current Issues:
- Closure opcode creates dummy prototypes instead of using actual constants
- No upvalue instruction processing
- Thread doesn't track open upvalues
- Close opcode only handles upvalues in current closure

#### Implementation Tasks:
1. **Add Function Prototype Value Type**
   - Extend the Value enum to include FunctionProto as a first-class value
   - Update relevant equality, hashing, and debug implementations

2. **Enhance Thread Structure**
   - Add open_upvalues field to Thread struct
   - Implement methods for finding, adding, and removing upvalues
   - Ensure upvalues are kept sorted by stack index

3. **Implement Upvalue Management**
   - Create find_or_create_upvalue function for reusing upvalues
   - Add methods to close upvalues by stack index range
   - Implement proper sorting of upvalues list

4. **Update Closure Opcode**
   - Extract actual function prototype from constants
   - Process upvalue instructions following the Closure opcode
   - Create upvalues based on instruction type (move or get-parent)
   - Apply transaction pattern and two-phase borrowing

5. **Enhance Close Opcode**
   - Find all upvalues in thread.open_upvalues referencing the relevant stack region
   - Close upvalues in reverse order (highest stack index first)
   - Update the thread's open_upvalues list

6. **Refine GetUpval/SetUpval**
   - Ensure integration with thread-wide upvalue management
   - Maintain consistent behavior with other closure operations

### 2. Complete Pending Operations (High Priority)

#### Current Issues:
- __concat metamethod returns NotImplemented error
- TableIndex, TableNewIndex, ArithmeticOp operations are defined but never used
- Some operations end with general "not implemented" errors

#### Implementation Tasks:
1. **Complete Concatenation with __concat**
   - Properly handle the __concat metamethod
   - Support all value types with appropriate coercion

2. **Implement TableIndex Operation**
   - Complete the processing with proper metamethod support
   - Follow transaction pattern for all operations

3. **Implement TableNewIndex Operation**
   - Complete the processing with proper metamethod support
   - Follow transaction pattern for all operations

4. **Implement ArithmeticOp Operation**
   - Support all arithmetic operations with metamethod handling
   - Ensure proper coercion rules are followed

### 3. Missing Opcodes (Medium Priority)

#### Current Issues:
- Self opcode for method calls is missing
- VarArg opcode for variable arguments is missing
- ExtraArg opcode for extended arguments is missing

#### Implementation Tasks:
1. **Implement Self Opcode**
   - Add SELF to the OpCode enum with proper value
   - Implement handler following transaction pattern
   - Add tests for method call syntax

2. **Implement VarArg Opcode**
   - Add VARARG to the OpCode enum with proper value
   - Implement handler following transaction pattern
   - Update function call mechanism to support varargs

3. **Implement ExtraArg Opcode**
   - Add EXTRAARG to the OpCode enum with proper value
   - Implement handler following transaction pattern
   - Update relevant operations to use extended arguments

### 4. Optimize Existing Implementations (Medium Priority)

#### Current Issues:
- SetList C=0 case uses placeholder value
- TailCall doesn't implement true tail call optimization
- Some error handling is minimal or generic

#### Implementation Tasks:
1. **Complete SetList Implementation**
   - Properly handle C=0 case by reading the next instruction
   - Follow transaction pattern for all operations

2. **Enhance TailCall Optimization**
   - Implement proper tail call optimization to reuse frames
   - Ensure compliance with Lua 5.1 semantics

3. **Improve Error Handling**
   - Add source location information where possible
   - Provide more specific error messages
   - Ensure consistent error handling across VM

## Implementation Approach

### Phase 1: Closure System (Estimated Effort: High)

This is the most complex part of the VM that remains to be implemented and will require its own focused session.

1. **Design Session**
   - Finalize upvalue representation and management
   - Design closure creation and upvalue instruction handling
   - Ensure architectural compliance for all components

2. **Implementation**
   - Update Thread struct and relevant methods
   - Implement upvalue management functions
   - Update Closure, GetUpval, SetUpval, and Close opcodes
   - Add tests for upvalue capture, sharing, and closing

3. **Validation**
   - Run specific closure tests
   - Verify lexical scoping rules are followed
   - Ensure compliance with Lua 5.1 semantics

### Phase 2: Pending Operations (Estimated Effort: Medium)

1. **Complete Concatenation**
   - Implement __concat metamethod handling
   - Update Concatenation operation to use it

2. **Implement Remaining Operations**
   - Complete TableIndex, TableNewIndex, ArithmeticOp
   - Add tests for each operation

### Phase 3: Missing Opcodes (Estimated Effort: Medium)

1. **Implement Self, VarArg, ExtraArg**
   - Add opcode handlers following architectural patterns
   - Update supporting code as needed
   - Add tests for each opcode

### Phase 4: Optimization (Estimated Effort: Low)

1. **Fix SetList C=0 Case**
   - Update to read next instruction
   - Add test for this case

2. **Optimize TailCall**
   - Implement proper frame reuse
   - Update test expectations

3. **Enhance Error Handling**
   - Add source location and better messages
   - Improve error context throughout VM

## Testing Strategy

For each implementation phase:
1. **Unit Tests** - Verify individual components work correctly
2. **Integration Tests** - Ensure components work together as expected
3. **Compliance Tests** - Check conformance with Lua 5.1 semantics

Focus test development on the closure system, as this is the most complex and incomplete part of the VM.

## Architectural Compliance

Throughout implementation, ensure adherence to the core architectural principles:
1. Non-recursive state machine execution model
2. Transaction-based heap access
3. Handle-based memory management with validation
4. Two-phase borrowing pattern for complex operations
5. Clean component separation

## Conclusion

Completing the core VM implementation is essential before moving on to other components like the compiler or Redis integration. The closure system represents the most significant challenge remaining, requiring careful design and implementation to maintain the VM's architectural integrity while supporting Lua's complex lexical scoping rules.

By following this plan, the core VM can be completed in a systematic way that preserves the architectural vision of the project.