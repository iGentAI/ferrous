# Lua VM Implementation Test Plan

This document outlines the testing strategy for verifying the correctness of the Lua VM implementation in Ferrous. The goal is to ensure that each component adheres to the architectural principles outlined in the design documents, with special focus on the challenging aspects of handle validation, Redis function integration, and metamethod handling.

## Testing Principles

1. **Component-level Testing**: Each component should have dedicated tests
2. **Integration Testing**: Components should be tested together
3. **Architectural Compliance**: Tests should verify adherence to architectural principles
4. **Edge Case Coverage**: Focus on boundary conditions and error handling
5. **Redis Compatibility**: Ensure compatibility with Redis Lua behavior

## Test Categories

### 1. Memory Management Tests

#### 1.1 Arena Tests

| Test Name | Description | Expectation |
|-----------|-------------|-------------|
| `test_arena_insert_retrieve` | Basic insertion and retrieval | Handle retrieves correct value |
| `test_arena_remove` | Removing items from arena | Item removed, handle invalidated |
| `test_arena_reuse` | Slot reuse after removal | New handle reuses slot with new generation |
| `test_generation_checking` | Verify generation safety | Old handles rejected after reuse |
| `test_arena_clear` | Clear all items | All handles invalid after clear |

#### 1.2 Handle Tests

| Test Name | Description | Expectation |
|-----------|-------------|-------------|
| `test_typed_handles` | Type-safety of handles | Compilation error if mixing types |
| `test_handle_copy` | Copy semantics | Handles are copied properly |
| `test_handle_eq` | Equality comparison | Same index+generation = equal |
| `test_handle_hash` | Hashing behavior | Consistent hash values |
| `test_invalid_handle` | Using invalid handles | Proper error returned |

### 2. Value System Tests

#### 2.1 Value Types

| Test Name | Description | Expectation |
|-----------|-------------|-------------|
| `test_nil_value` | Nil value behaviors | is_nil() returns true |
| `test_boolean_value` | Boolean semantics | Proper truthiness values |
| `test_number_value` | Number operations | Proper conversions |
| `test_string_value` | String operations | UTF-8 handling correct |
| `test_value_type_names` | Type name retrieval | Correct names returned |

#### 2.2 Table Tests

| Test Name | Description | Expectation |
|-----------|-------------|-------------|
| `test_table_creation` | Basic table creation | Empty table created |
| `test_array_access` | Array part access | 1-based indexing works |
| `test_map_access` | Hash part access | Various key types work |
| `test_mixed_access` | Mixed array/map | Correct priority of array vs hash |
| `test_metatable` | Metatable setting/getting | Metatable correctly associated |

### 3. Transaction Tests

#### 3.1 Basic Transaction Operations

| Test Name | Description | Expectation |
|-----------|-------------|-------------|
| `test_transaction_create_commit` | Basic transaction cycle | Changes committed successfully |
| `test_transaction_abort` | Aborting transactions | No changes applied after abort |
| `test_transaction_reset` | Reusing transactions | Transaction reusable after reset |
| `test_transaction_errors` | Error during transaction | No changes applied on error |

#### 3.2 Handle Validation Tests

| Test Name | Description | Expectation |
|-----------|-------------|-------------|
| `test_validation_basic` | Basic handle validation | Valid handles pass validation |
| `test_validation_stale_handle` | Stale handle detection | Proper error for stale handles |
| `test_validation_invalid_handle` | Invalid handle detection | Proper error for invalid handles |
| `test_validation_type_checking` | Type-specific validation | Error when wrong handle type |
| `test_validation_caching` | Validation caching | Performance improved with cache |

### 4. VM Execution Tests

#### 4.1 Basic VM Operations

| Test Name | Description | Expectation |
|-----------|-------------|-------------|
| `test_register_ops` | Register operations | Correct movement between registers |
| `test_constants` | Loading constants | Constants loaded correctly |
| `test_table_ops` | Table operations | Tables properly manipulated |
| `test_arithmetic` | Arithmetic operations | Correct calculation results |
| `test_control_flow` | Control flow | Proper branching behavior |

#### 4.2 Function Call Tests

| Test Name | Description | Expectation |
|-----------|-------------|-------------|
| `test_simple_call` | Basic function calls | Function executes with proper args |
| `test_nested_calls` | Nested function calls | Proper return handling |
| `test_call_depth` | Deep call stacks | No stack overflow, proper limits |
| `test_error_propagation` | Error handling in calls | Errors properly propagated |

### 5. Redis API Function Tests

#### 5.1 Context Tests

| Test Name | Description | Expectation |
|-----------|-------------|-------------|
| `test_keys_argv_setup` | KEYS and ARGV tables | Tables correctly populated |
| `test_redis_table_setup` | Redis API table | Functions properly registered |
| `test_empty_args` | No arguments | Empty tables handled correctly |

#### 5.2 Redis Function Tests

| Test Name | Description | Expectation |
|-----------|-------------|-------------|
| `test_redis_call_basic` | Basic redis.call usage | Command executes correctly |
| `test_redis_call_error` | Error in redis.call | Errors properly propagated |
| `test_redis_pcall` | Protected call | Errors returned as values |
| `test_redis_call_complex` | Complex data types | Proper conversion both ways |

#### 5.3 Integration Tests

| Test Name | Description | Expectation |
|-----------|-------------|-------------|
| `test_script_with_redis` | Full script execution | Script interacts with Redis correct |
| `test_script_keys_access` | Key access patterns | KEYS properly accessible |
| `test_script_argv_access` | Argument access | ARGV properly accessible |

### 6. Metamethod Tests

#### 6.1 Basic Metamethod Tests

| Test Name | Description | Expectation |
|-----------|-------------|-------------|
| `test_index_metamethod` | __index resolution | Correct field access |
| `test_newindex_metamethod` | __newindex behavior | Proper field setting |
| `test_call_metamethod` | __call behavior | Non-function values callable |
| `test_arithmetic_metamethods` | Math metamethods | Operations work with metamethods |
| `test_comparison_metamethods` | Comparison metamethods | Comparisons work with metamethods |

#### 6.2 Advanced Metamethod Tests

| Test Name | Description | Expectation |
|-----------|-------------|-------------|
| `test_metamethod_chain` | Chained metamethods | Correct resolution order |
| `test_metamethod_recursion` | Recursive metamethods | Proper depth limiting |
| `test_metamethod_error_handling` | Error in metamethod | Proper propagation |

## Test Implementation Approach

### Phase 1: Unit Tests for Individual Components

Start with simple unit tests for each component:

```rust
#[test]
fn test_arena_basic() {
    let mut arena = Arena::<i32>::new();
    
    let h1 = arena.insert(42);
    let h2 = arena.insert(84);
    
    assert_eq!(arena.get(h1), Some(&42));
    assert_eq!(arena.get(h2), Some(&84));
    
    assert_eq!(arena.remove(h1), Some(42));
    assert_eq!(arena.get(h1), None);
}
```

### Phase 2: Component Integration Tests

Test interaction between components:

```rust
#[test]
fn test_heap_transaction_integration() -> LuaResult<()> {
    let mut heap = LuaHeap::new()?;
    
    // Create and use elements through transactions
    let (table, key, value) = {
        let mut tx = HeapTransaction::new(&mut heap);
        let table = tx.create_table()?;
        let key = tx.create_string("test_key")?;
        let value = tx.create_string("test_value")?;
        tx.commit()?;
        (table, key, value)
    };
    
    // Use in another transaction
    {
        let mut tx = HeapTransaction::new(&mut heap);
        tx.set_table_field(table, Value::String(key), Value::String(value))?;
        tx.commit()?;
    }
    
    // Verify in third transaction
    {
        let tx = HeapTransaction::new(&mut heap);
        let result = tx.read_table_field(table, &Value::String(key))?;
        assert_eq!(result, Value::String(value));
    }
    
    Ok(())
}
```

### Phase 3: Architectural Requirements Tests

Create tests specifically targeting architectural compliance:

```rust
#[test]
fn test_transaction_boundary() -> LuaResult<()> {
    let mut heap = LuaHeap::new()?;
    
    // Create a table
    let table = {
        let mut tx = HeapTransaction::new(&mut heap);
        let t = tx.create_table()?;
        tx.commit()?;
        t
    };
    
    // Attempt to modify outside transaction - should not compile
    // heap.get_table_mut(table)?;  // This line should not compile!
    
    // Proper way to modify
    {
        let mut tx = HeapTransaction::new(&mut heap);
        tx.set_table_field(table, Value::Number(1.0), Value::Number(42.0))?;
        tx.commit()?;
    }
    
    Ok(())
}

#[test]
fn test_non_recursive_execution() -> LuaResult<()> {
    let mut vm = LuaVM::new()?;
    
    // Create a script with recursive function
    let script = r#"
        function factorial(n)
            if n <= 1 then
                return 1
            else
                return n * factorial(n-1) 
            end
        end
        
        return factorial(5)
    "#;
    
    // Execute script
    let result = vm.eval_script(script)?;
    
    // Verify result
    assert_eq!(result, Value::Number(120.0));
    
    // Verify execution was non-recursive
    // (Check depth counter never exceeded 1 - this would need instrumentation)
    // ...
    
    Ok(())
}
```

### Phase 4: Edge Case and Error Tests

Focus on error handling and boundary conditions:

```rust
#[test]
fn test_validation_failure() -> LuaResult<()> {
    let mut heap = LuaHeap::new()?;
    
    // Create a string
    let string = {
        let mut tx = HeapTransaction::new(&mut heap);
        let s = tx.create_string("test")?;
        tx.commit()?;
        s
    };
    
    // Create invalid handle
    let invalid = StringHandle::from(Handle {
        index: 9999,
        generation: 0,
        _phantom: PhantomData,
    });
    
    // Attempt to use invalid handle
    let mut tx = HeapTransaction::new(&mut heap);
    let result = tx.get_string_value(invalid);
    
    // Should fail with appropriate error
    assert!(matches!(result, Err(LuaError::InvalidHandle)));
    
    Ok(())
}
```

### Phase 5: Full Integration Tests

Test the entire VM with complex scripts:

```rust
#[test]
fn test_complex_script() -> LuaResult<()> {
    let mut vm = LuaVM::new()?;
    
    // Create a script using multiple features
    let script = r#"
        local t = {}
        t.key1 = "value1"
        t.key2 = 42
        
        function t:method()
            return self.key1 .. " - " .. self.key2
        end
        
        local mt = {
            __index = function(tbl, key)
                return "metamethod: " .. key
            end
        }
        
        local t2 = {}
        setmetatable(t2, mt)
        
        return t:method() .. ", " .. t2.missing
    "#;
    
    // Execute script
    let result = vm.eval_script(script)?;
    
    // Verify result
    match result {
        Value::String(handle) => {
            let mut tx = HeapTransaction::new(vm.heap_mut());
            let value = tx.get_string_value(handle)?;
            assert_eq!(value, "value1 - 42, metamethod: missing");
            Ok(())
        }
        _ => Err(LuaError::RuntimeError("Unexpected result type".to_string())),
    }
}
```

## Critical Testing Areas

Special attention must be paid to the following "tricky bits":

### 1. Handle Validation Testing

Focus on comprehensive tests for handle validation, which is crucial for memory safety:

```rust
#[test]
fn test_validation_edge_cases() -> LuaResult<()> {
    let mut heap = LuaHeap::new()?;
    
    // 1. Test stale handle detection
    let stale_handle = {
        // Create and remove string
        let mut tx = HeapTransaction::new(&mut heap);
        let handle = tx.create_string("temp")?;
        tx.commit()?;
        
        // Remove string, making handle stale
        let mut tx = HeapTransaction::new(&mut heap);
        // ... remove string ...
        tx.commit()?;
        
        // Create new string in same slot
        let mut tx = HeapTransaction::new(&mut heap);
        let _ = tx.create_string("new")?;
        tx.commit()?;
        
        // Original handle is now stale
        handle
    };
    
    // Try to use stale handle
    let mut tx = HeapTransaction::new(&mut heap);
    let result = tx.get_string_value(stale_handle);
    assert!(matches!(result, Err(LuaError::StaleHandle)));
    
    // 2. Test validation cache
    let handle = {
        let mut tx = HeapTransaction::new(&mut heap);
        let h = tx.create_string("test")?;
        tx.commit()?;
        h
    };
    
    // Validate multiple times - should use cache after first
    let mut tx = HeapTransaction::new(&mut heap);
    assert!(tx.validate_handle(handle.0).is_ok());
    // Second validation should hit cache
    assert!(tx.validate_handle(handle.0).is_ok());
    
    Ok(())
}
```

### 2. Redis Function Interface Testing

Test the boundary between Lua and Redis functionality:

```rust
#[test]
fn test_redis_api_isolation() -> LuaResult<()> {
    // Setup test environment
    let storage = Arc::new(StorageEngine::new_in_memory());
    
    // Create context
    let context = ScriptContext {
        storage: storage.clone(),
        db: 0,
        keys: vec![b"key1".to_vec()],
        args: vec![b"arg1".to_vec()],
        timeout: Duration::from_secs(5),
    };
    
    // Create VM
    let mut vm = LuaVM::new()?;
    vm.set_context(context)?;
    
    // Script that tries to do something harmful
    let script = r#"
        -- Attempt to access VM state directly
        local attempt1 = redis.call('GET', 'key1')
        
        -- Attempt to modify heap directly
        -- This should not be possible from the script
        
        return attempt1
    "#;
    
    // Run script - should execute safely
    // We're testing that the redis.call function is properly isolated
    vm.eval_script(script)?;
    
    Ok(())
}
```

### 3. Metamethod Behavior Testing

Test non-recursive metamethod resolution:

```rust
#[test]
fn test_metamethod_recursion_prevention() -> LuaResult<()> {
    let mut vm = LuaVM::new()?;
    
    // Create a script with potentially recursive metamethods
    let script = r#"
        local t1 = {}
        local t2 = {}
        
        -- Create mutually recursive __index metamethods
        local mt1 = {
            __index = function(t, k)
                print("mt1.__index", k)
                return t2[k] -- This will invoke t2's __index
            end
        }
        
        local mt2 = {
            __index = function(t, k)
                print("mt2.__index", k)
                return t1[k] -- This will invoke t1's __index
            end
        }
        
        setmetatable(t1, mt1)
        setmetatable(t2, mt2)
        
        -- This would cause infinite recursion if not handled properly
        return t1.foo
    "#;
    
    // Execute script - should either return nil or error with stack overflow
    // rather than actually recurring infinitely
    let result = vm.eval_script(script);
    
    // It should be an error, but shouldn't crash
    assert!(result.is_err());
    
    Ok(())
}
```

## Test Infrastructure

### Test Helper Functions

Create reusable test helpers:

```rust
// Setup function for VM tests
fn setup_test_vm() -> LuaResult<LuaVM> {
    let mut vm = LuaVM::new()?;
    // Add any common setup
    Ok(vm)
}

// Script execution helper
fn execute_script(vm: &mut LuaVM, script: &str) -> LuaResult<Value> {
    vm.eval_script(script)
}

// Redis context setup helper
fn setup_redis_context(storage: Arc<StorageEngine>) -> ScriptContext {
    ScriptContext {
        storage,
        db: 0,
        keys: vec![b"test_key".to_vec()],
        args: vec![b"test_arg".to_vec()],
        timeout: Duration::from_secs(5),
    }
}
```

## Test Metrics

For each component group, aim for these test coverage metrics:

| Component | Test Coverage Target | Critical Path Coverage |
|-----------|----------------------|------------------------|
| Arena | 95% | 100% |
| Handles | 90% | 100% |
| Value Types | 85% | 100% |
| Heap | 90% | 100% |
| Transaction | 95% | 100% |
| VM Core | 90% | 100% |
| Redis API | 90% | 100% |
| Metamethods | 95% | 100% |

Where "critical path" refers to core functionality that could affect memory safety or cause hard-to-debug issues.

## Integration with Existing Test Suite

The tests described in this document should complement and extend the existing test suite in `ferrous/tests/lua`. Specifically:

1. `ferrous/tests/lua/lua_specification_test_suite.rs`: Comprehensive tests against Lua 5.1 specification
2. `ferrous/tests/lua/arena_tests.rs`: Existing arena tests
3. `ferrous/tests/lua/minimal_vm_test.rs`: Basic VM functionality tests

Our new tests should focus on:
1. Areas not covered by existing tests
2. Architectural compliance
3. Corner cases and error conditions
4. Redis integration specifics

## Test Execution Strategy

1. **During Development**:
   - Run focused unit tests for components being worked on
   - Use TDD approach - write tests before implementation

2. **After Major Changes**:
   - Run the full test suite
   - Profile execution for performance regressions

3. **Before Integration**:
   - Run all tests with memory checking enabled
   - Verify no memory leaks or unsafe operations

## Conclusion

Following this test plan will help ensure that the Lua VM implementation is:
1. Architecturally sound following the design principles
2. Memory safe through proper handle validation
3. Non-recursive in its execution model
4. Correctly integrated with Redis
5. Comprehensive in its Lua language support

Progress will be tracked in the LUA_IMPLEMENTATION_STATUS.md document.