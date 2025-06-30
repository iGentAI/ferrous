# Lua VM Implementation Plan for Ferrous

## Current State Assessment

After our implementation attempts, we've identified several critical architectural issues that must be addressed in a systematic reimplementation:

1. **Inconsistent Transaction Usage**: The transaction pattern is applied inconsistently, causing borrow checker conflicts.
2. **Recursive Execution Model**: Despite the design spec calling for a non-recursive model, many operations call directly into recursive functions.
3. **Direct Heap Access**: Many operations bypass the transaction system, causing ownership conflicts.
4. **C Function Integration Issues**: Special handling for C functions creates borrow checker conflicts.
5. **Metamethod Handling Inconsistency**: Metamethods are sometimes called directly rather than queued.

## Revised Implementation Phases

### Phase 1: Core Infrastructure (1-2 weeks)

1. **Memory Management System**
   - Implement generational arena with proper handle validation
   - Create typed handles for all object types
   - Implement validation scope for safe handle usage
   - Comprehensive unit testing for memory safety

2. **Value System**
   - Implement all Lua value types with proper handle references
   - Ensure values have proper Clone but not Copy implementations
   - Create value comparison and conversion utilities
   - Unit test all value operations

### Phase 2: Heap and Transaction System (1-2 weeks)

1. **Heap Implementation**
   - Implement arenas for all object types
   - Create proper access methods that enforce handle validation
   - Ensure no direct arena access is possible outside the heap
   - Unit test heap operations

2. **Transaction System**
   - Implement change tracking for all operations
   - Create a commit system that applies changes atomically
   - Ensure transaction.commit() doesn't consume self
   - Implement rollback capability for error handling
   - Test transaction isolation and consistency

### Phase 3: VM Core (2-3 weeks)

1. **VM State Machine**
   - Implement single execution loop with no recursion
   - Create pending operation queue for state transitions
   - Implement proper frame stack in the heap
   - Create execution context for C function calls
   - Test execution of basic operations

2. **Opcode Handlers**
   - Implement handlers for all Lua opcodes
   - Ensure all handlers use transactions consistently
   - Implement proper CALL/RETURN handling without recursion
   - Test each opcode handler individually and in combination

### Phase 4: Compiler and Parser (2-3 weeks)

1. **Parser Implementation**
   - Create AST representation of Lua code
   - Implement recursive descent parser
   - Handle all Lua syntax features
   - Test parser with various inputs

2. **Compiler Implementation**
   - Create bytecode generation from AST
   - Implement register allocation
   - Handle upvalue and closure creation
   - Test compiler with various functions

### Phase 5: Standard Library and Redis Integration (1-2 weeks)

1. **Standard Library**
   - Implement Lua standard library functions
   - Create proper C function bindings
   - Test library functions

2. **Redis API**
   - Implement redis.call and redis.pcall
   - Create Redis command integration
   - Test with Redis command suite

### Phase 6: Production Readiness (1-2 weeks)

1. **Performance Optimization**
   - Implement instruction caching
   - Optimize memory usage
   - Implement string interning
   - Benchmark against Redis Lua

2. **Error Handling and Diagnostics**
   - Implement comprehensive error messages
   - Create debugging utilities
   - Add logging and tracing
   - Test error scenarios

## Implementation Requirements

### Transaction Consistency

**ALL** operations that access the heap must go through transactions:

```rust
// Create transaction
let mut tx = HeapTransaction::new(&mut self.heap);

// Use transaction for ALL access
let index_str = tx.create_string("__index")?;
let metatable = tx.get_metatable(table)?;

// Only commit at the very end
tx.commit()?;
```

### Function Call Requirements

**NO** recursive function calls are allowed:

```rust
// NEVER do this
let result = self.execute_function(closure, &args)?;

// ALWAYS do this
tx.queue_operation(PendingOperation::FunctionCall {
    closure,
    args: args.clone(),
    context: CallContext::Register { base, offset },
});
// ... and let the main execution loop handle it
```

### Metamethod Handling

Metamethods must be handled non-recursively:

```rust
// Queue metamethod call for later execution
tx.queue_operation(PendingOperation::MetamethodCall {
    method_name,
    table,
    args: vec![table_value, key_value],
    context: CallContext::OpResult { register },
});

// Commit transaction
tx.commit()?;

// Let main loop handle the call
```

### C Function Integration

C functions require special handling:

```rust
// Extract needed values before any borrows
let cfunc_copy = cfunc.clone();
let args_copy = args.clone();

// Create a clean execution context
let mut ctx = ExecutionContext::new(self, stack_base, args_copy.len());

// Call C function with clean borrow boundaries
let ret_count = cfunc_copy(&mut ctx)?;
```

## Testing Strategy

1. **Unit Tests**
   - Test each component in isolation
   - Test transaction consistency
   - Test handle validation
   - Test memory management

2. **Integration Tests**
   - Test opcode sequence execution
   - Test function calls and returns
   - Test error propagation
   - Test C function integration

3. **Compatibility Tests**
   - Test against Lua 5.1 test suite
   - Test Redis-specific functionality
   - Test error behavior matches Redis

4. **Performance Tests**
   - Benchmark against Redis Lua
   - Measure memory usage
   - Test with large scripts

## Success Criteria

The implementation will be considered successful when:

1. All unit and integration tests pass
2. The Lua specification test suite passes
3. Redis integration tests pass
4. Performance is within 20% of Redis Lua
5. Memory usage is reasonable
6. No Rust safety or borrow checker errors occur

## Conclusion

This revised implementation plan addresses the architectural issues we've encountered in our previous attempts. By systematically rebuilding with proper transaction discipline, non-recursive execution, and consistent access patterns, we can create a Rust Lua VM that works harmoniously with Rust's ownership system rather than fighting against it.