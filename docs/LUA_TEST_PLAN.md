# Lua VM Test Plan

This document outlines a test-driven approach for implementing the Ferrous Lua VM. Following this plan will help ensure that each component is thoroughly tested and meets the requirements before moving on to more complex features.

## 1. Test Architecture

The testing approach will use multiple layers:

### 1.1 Unit Tests

Unit tests focus on individual components in isolation:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_insert_retrieve() {
        let mut arena = Arena::<i32>::new();
        let handle = arena.insert(42);
        assert_eq!(arena.get(handle), Some(&42));
    }
    
    #[test]
    fn test_arena_remove() {
        let mut arena = Arena::<i32>::new();
        let handle = arena.insert(42);
        assert_eq!(arena.remove(handle), Some(42));
        assert_eq!(arena.get(handle), None);
    }
    
    // etc...
}
```

### 1.2 Integration Tests

Integration tests focus on component interactions:

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_handle_validation() {
        let mut heap = LuaHeap::new();
        let handle = heap.create_string("test").unwrap();
        
        // Test validation
        assert!(heap.is_valid_string(handle));
        
        // Use transaction
        let mut tx = HeapTransaction::new(&mut heap);
        assert!(tx.is_valid_string(handle));
    }
    
    // etc...
}
```

### 1.3 Behavioral Tests

Behavioral tests validate VM operation:

```rust
#[cfg(test)]
mod behavioral_tests {
    use super::*;

    #[test]
    fn test_basic_arithmetic() {
        let mut vm = LuaVM::new().unwrap();
        let result = vm.eval("return 2 + 3 * 4").unwrap();
        
        if let Value::Number(num) = result {
            assert_eq!(num, 14.0);
        } else {
            panic!("Expected number, got {:?}", result);
        }
    }
    
    // etc...
}
```

### 1.4 Redis Compatibility Tests

These test Redis integration:

```python
def test_eval_command():
    r = redis.Redis(host='localhost', port=6379)
    
    # Test basic EVAL
    result = r.eval("return 2 + 3 * 4", 0)
    assert result == 14
    
    # Test with keys and arguments
    result = r.eval("return {KEYS[1], ARGV[1]}", 1, "key1", "value1")
    assert result == ["key1", "value1"]
```

## 2. Test Categories

### 2.1 Memory Management Tests

Test arena and handle operations:

- Handle creation and validation
- Handle invalidation
- Reference counting
- Memory allocation and deallocation
- Generation tracking

### 2.2 Value Tests

Test Lua value operations:

- Number, string, boolean operations
- Table creation and access
- Function creation and closures
- Value comparisons
- Value conversion
- Type checking

### 2.3 Transaction Tests

Test transaction operations:

- Transaction creation
- Reading and writing through transactions
- Transaction commit
- Transaction isolation
- Transaction consistency

### 2.4 VM Execution Tests

Test VM instruction execution:

- Basic arithmetic
- Control flow (if, for, while)
- Function calls and returns
- Table operations
- Metamethod handling
- Standard library functions

### 2.5 Error Handling Tests

Test error scenarios:

- Syntax errors
- Runtime errors
- Type errors
- Stack overflow
- Memory limits
- Invalid handles

### 2.6 Redis Integration Tests

Test Redis-specific features:

- EVAL command
- EVALSHA command
- SCRIPT LOAD/EXISTS/FLUSH
- KEYS and ARGV tables
- Redis command access
- Error propagation

## 3. Test Implementation Plan

### 3.1 Phase 1: Unit Components

1. **Arena and Handle Tests**
   - Test handle creation and validation
   - Test handle invalidation
   - Test generation checking

2. **Value System Tests**
   - Test value creation and access
   - Test value comparison
   - Test value conversion

### 3.2 Phase 2: Heap and Transactions

1. **Heap Tests**
   - Test object storage and retrieval
   - Test object mutation
   - Test object invalidation

2. **Transaction Tests**
   - Test transaction creation
   - Test transaction read/write
   - Test transaction commit

### 3.3 Phase 3: VM Core

1. **VM State Machine Tests**
   - Test operation queue
   - Test call frame management
   - Test state transitions

2. **Basic Opcode Tests**
   - Test arithmetic opcodes
   - Test variable access
   - Test control flow

### 3.4 Phase 4: Compiler and Parser

1. **Parser Tests**
   - Test expression parsing
   - Test statement parsing
   - Test error handling

2. **Compiler Tests**
   - Test bytecode generation
   - Test register allocation
   - Test optimization

### 3.5 Phase 5: Full Integration

1. **Script Execution Tests**
   - Test script evaluation
   - Test error propagation
   - Test performance

2. **Redis Compatibility Tests**
   - Test EVAL/EVALSHA
   - Test SCRIPT commands
   - Test Redis API

## 4. Test-Driven Development Approach

For each component, follow this workflow:

1. Write test for the simplest version of a feature
2. Implement minimum code to pass the test
3. Refactor for clarity and performance
4. Add test for next feature
5. Repeat

Example TDD workflow for implementing the Arena:

```rust
// 1. Write test
#[test]
fn test_arena_insert() {
    let mut arena = Arena::<i32>::new();
    let handle = arena.insert(42);
    assert!(arena.contains(handle));
}

// 2. Implement minimum to pass
struct Arena<T> {
    items: Vec<T>,
}

impl<T> Arena<T> {
    fn new() -> Self {
        Arena { items: Vec::new() }
    }
    
    fn insert(&mut self, value: T) -> Handle<T> {
        let index = self.items.len();
        self.items.push(value);
        Handle { index: index as u32, generation: 0, _phantom: PhantomData }
    }
    
    fn contains(&self, handle: Handle<T>) -> bool {
        handle.index < self.items.len() as u32
    }
}

// 3. Write next test
#[test]
fn test_arena_get() {
    let mut arena = Arena::<i32>::new();
    let handle = arena.insert(42);
    assert_eq!(arena.get(handle), Some(&42));
}

// 4. Implement to pass
impl<T> Arena<T> {
    // ...existing code...
    
    fn get(&self, handle: Handle<T>) -> Option<&T> {
        if handle.index < self.items.len() as u32 {
            Some(&self.items[handle.index as usize])
        } else {
            None
        }
    }
}

// 5. Write test for more complex feature
#[test]
fn test_arena_remove() {
    let mut arena = Arena::<i32>::new();
    let handle = arena.insert(42);
    assert_eq!(arena.remove(handle), Some(42));
    assert_eq!(arena.get(handle), None);
}

// And so on...
```

## 5. Test Organization

Organize tests using Rust's module system:

```
src/
  lua/
    arena.rs            // Implementation
    arena/
      tests.rs          // Unit tests
      integration.rs    // Integration tests
    heap.rs
    heap/
      tests.rs
      integration.rs
    ...
tests/
  lua/
    unit/              // Unit tests for each component
      arena_tests.rs
      value_tests.rs
      ...
    integration/       // Integration tests
      heap_tests.rs
      transaction_tests.rs
      ...
    behavioral/        // Behavioral tests
      arithmetic_tests.rs
      function_tests.rs
      ...
    redis/            // Redis integration tests
      eval_tests.rs
      script_tests.rs
      ...
```

## 6. Test Coverage Requirements

Aim for the following coverage levels:

1. Unit tests: 95%+ line coverage
2. Integration tests: 90%+ line coverage
3. Behavioral tests: Cover all Lua 5.1 language features
4. Redis tests: Cover all Redis Lua commands and behaviors

## 7. Test Automation

Automate test execution:

```bash
# Run unit tests
cargo test --lib

# Run integration tests
cargo test --test '*'

# Run behavioral tests
cargo test --test 'behavioral_*'

# Run Redis compatibility tests
cargo test --test 'redis_*'

# Run with full debug output
RUST_BACKTRACE=1 cargo test -- --nocapture
```

By following this test plan, you'll build the Lua VM in a methodical, test-driven manner that ensures correctness and robustness while maintaining compatibility with Redis Lua semantics.