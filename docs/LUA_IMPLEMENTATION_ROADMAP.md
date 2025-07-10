# Lua Implementation Roadmap

This document outlines the roadmap for completing the Lua VM implementation in the Ferrous project.

## Current Status (July 2025)

The Lua VM implementation has made significant progress:
- All major Lua opcodes have been implemented with register window support
- Basic upvalue and closure functionality is working
- The VM can execute simple scripts with arithmetic, functions, closures, and basic control flow

However, several aspects still need refinement or completion:

## Short-term Goals (Next 2-4 Weeks)

### High Priority

1. **Return Value Handling**: Fix issues with return value propagation between function calls. Currently, values are not always correctly positioned for the caller to access.

2. **SetUpval/GetUpval Refinement**: Improve upvalue access and modification to ensure correct state maintenance across function calls.

3. **Table Metamethod Support**: Enhance GetTable/SetTable with proper metamethod handling, maintaining transaction safety.

4. **Error Propagation**: Implement proper error handling with meaningful stack traces and error messages.

5. **Standard Library Completion**: Finish implementing the missing standard library functions.

### Medium Priority  

1. **C Function Integration**: Improve C function handling to ensure proper argument passing and return value collection.

2. **Arithmetic Metamethods**: Implement metamethods for all arithmetic operations (add, sub, mul, etc.).

3. **Comparison Metamethods**: Ensure all comparison operations correctly use metamethods when needed.

4. **Garbage Collection**: Implement basic mark-and-sweep garbage collection for Lua objects.

5. **Performance Profiling**: Add instrumentation to identify bottlenecks in the current implementation.

### Low Priority  

1. **Debug Information**: Add detailed debug information for better error reporting and debugging.

2. **Proper Module System**: Enhance the module loading and execution system.

3. **Window Recycling**: Implement window recycling to reduce allocation overhead.

## Mid-term Goals (1-3 Months)

1. **Redis Integration**: Once the core VM is stable and complete, integrate it with the Redis-compatible storage engine.

2. **Redis Lua Commands**: Implement EVAL, EVALSHA, and related commands.

3. **Redis API**: Add the redis.* API for Lua scripts.

4. **Resource Limits**: Implement proper resource limiting for Lua scripts (memory, CPU time, etc.).

5. **Test Suite Expansion**: Create a comprehensive test suite covering all VM functionality.

## Long-term Goals (3+ Months)

1. **Consider Hybrid Design**: Evaluate implementing the hybrid design outlined in [LUA_VM_PERFORMANT_HYBRID_DESIGN.md](./LUA_VM_PERFORMANT_HYBRID_DESIGN.md).

2. **Performance Optimization**: Optimize the VM for better performance without sacrificing safety.

3. **Extended Functionality**: Add additional Lua 5.1 functionality not required for Redis compatibility.

## Implementation Approach

Based on our experience implementing closures and upvalues, future implementations should follow these guidelines:

### Opcode Implementation Pattern

1. **Extreme Phase Separation**: Extract all needed data in distinct phases with no overlapping borrows:
   ```rust
   // Phase 1: Extract data A
   let data_a = { /* extraction with early scope end */ };
   
   // Phase 2: Extract data B
   let data_b = { /* extraction with early scope end */ };
   
   // Phase 3: Work with data
   do_something(data_a, data_b);
   ```

2. **Window-Stack Synchronization**: Always sync register windows to the thread stack before creating upvalues:
   ```rust
   sync_window_to_stack_helper(&mut tx, &self.register_windows, 
                             self.current_thread, window_idx, register_count)?;
   ```

3. **Transaction Lifecycle**: Do not commit transactions early in opcodes unless absolutely necessary:
   ```rust
   // Let step() handle the commit
   return Ok(StepResult::Continue);
   ```

4. **Error Handling**: Always commit transactions before returning errors:
   ```rust
   if error_condition {
       tx.commit()?;
       return Err(LuaError::SomeError("Error message".to_string()));
   }
   ```

5. **Register Protection**: Protect registers during complex operations:
   ```rust
   // Before function calls, metamethods, etc.
   self.register_windows.unprotect_all(window_idx)?;
   ```

## Known Challenges

1. **Borrow Checker Conflicts**: The register window and transaction-based architecture causes complex borrow checker issues. See [REGISTER_WINDOW_BORROW_PATTERNS.md](./REGISTER_WINDOW_BORROW_PATTERNS.md) for patterns to address these.

2. **Performance Overhead**: The current approach has significant performance overhead. This is accepted for now in favor of safety, with a long-term plan to optimize as outlined in [LUA_VM_PERFORMANT_HYBRID_DESIGN.md](./LUA_VM_PERFORMANT_HYBRID_DESIGN.md).

3. **Complex State Management**: Managing state across function calls and ensuring proper upvalue behavior requires careful attention to register window and stack synchronization.

## Success Criteria

The Lua implementation will be considered complete when:

1. All Lua 5.1 features required for Redis compatibility are implemented
2. The test suite passes all tests
3. The VM can execute Redis-typical Lua scripts at acceptable performance
4. Error handling and resource limits are properly implemented

## Conclusion

The Lua VM implementation has made significant progress, particularly with the recent fixes to upvalues and closures. By following the patterns established in these fixes, we can systematically address the remaining implementation tasks and complete the Lua VM.