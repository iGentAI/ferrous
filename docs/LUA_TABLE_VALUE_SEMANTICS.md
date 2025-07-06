# Lua VM Table Value Semantics Design

## Overview

This document outlines the design considerations for table operations in the Ferrous Lua VM, focusing on value semantics, key equality, and their interaction with the string interning system. It builds on the architectural refinements described in `LUA_STRING_INTERNING_AND_VALUE_SEMANTICS.md`.

## 1. Table Key Equality

### 1.1 Current Implementation

The current table implementation uses the `HashableValue` enum for table keys:

```rust
pub enum HashableValue {
    Nil,
    Boolean(bool),
    Number(OrderedFloat),
    String(StringHandle),
}

impl PartialEq for HashableValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (HashableValue::String(a), HashableValue::String(b)) => a == b, // Identity comparison
            // Other types...
        }
    }
}
```

This implementation compares string keys by handle identity, not content. While proper string interning can mitigate this issue, it represents a deeper architectural mismatch between Lua's value semantics and our handle-based implementation.

### 1.2 Design Requirements

A robust table implementation must:

1. **Ensure Content Equality**: Table key lookup must use content-based comparison for strings
2. **Maintain Transaction Safety**: Continue using the transaction model for memory safety
3. **Optimize Common Operations**: Table gets/sets are performance-critical operations
4. **Support Metamethods**: Properly integrate with metamethod handling
5. **Allow For Custom Keys**: Support user-defined keys with __index and __newindex

## 2. Design Options

### 2.1 Option 1: Enhanced String Interning Only

The simplest approach is to rely solely on better string interning:

```rust
// No changes to HashableValue, but enhance string interning to ensure
// the same string content always gets the same handle
```

**Pros**:
- Minimal changes to the table implementation
- Works well for standard Lua usage

**Cons**:
- Fragile - relies entirely on string interning working perfectly
- Can still break with dynamically generated strings

### 2.2 Option 2: Content-Based Hash with Identity Fast Path

A more robust approach uses content-based hashing with an identity-based fast path:

```rust
impl Hash for HashableValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            HashableValue::String(handle) => {
                // Hash the string content, not just the handle
                // Get string content from the heap and hash it
                let content = get_string_content(*handle);
                content.hash(state);
            },
            // Hash for other types...
        }
    }
}

impl PartialEq for HashableValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (HashableValue::String(a), HashableValue::String(b)) => {
                // Fast path: Same handle means same content if interning works
                if a == b {
                    true
                } else {
                    // Slow path: Compare content when handles differ
                    let content_a = get_string_content(*a);
                    let content_b = get_string_content(*b);
                    content_a == content_b
                }
            },
            // Other comparisons...
        }
    }
}
```

**Pros**:
- More robust - works even if string interning isn't perfect
- Maintains performance for common case (interned strings)

**Cons**:
- More complex implementation
- Requires access to string content during comparison

### 2.3 Option 3: Cached Content in HashableValue

Another approach would store string content directly in HashableValue:

```rust
pub enum HashableValue {
    Nil,
    Boolean(bool),
    Number(OrderedFloat),
    String {
        handle: StringHandle, 
        content: Arc<String>,  // Cached content for comparisons
    },
}
```

**Pros**:
- Most robust - content is always available for comparison
- No need to access heap during comparison

**Cons**:
- Higher memory usage from content duplication
- More complex conversion to/from Value

## 3. Recommended Approach

Based on our analysis of the Ferrous architecture, we recommend **Option 2: Content-Based Hash with Identity Fast Path**.

This approach balances robustness and performance by:
1. Using enhanced string interning as the primary mechanism
2. Falling back to content-based comparison when handles don't match
3. Avoiding memory overhead of storing duplicate string content

## 4. Table Metamethod Interaction

Table metamethods like `__index` and `__newindex` also rely on string keys:

```rust
// Typical metamethod access
let index_name = tx.create_string("__index")?;
let index_mm = tx.read_table_field(metatable, &Value::String(index_name))?;
```

These operations must be updated to ensure consistent string handles for metamethod names. The pre-interning approach solves this by ensuring all metamethod names are pre-interned during heap initialization.

## 5. Table Iteration Semantics

Lua's `pairs()` and `next()` functions rely on stable table iteration:

```rust
// Table iteration needs consistent key equality
pub fn table_next(&mut self, table: TableHandle, current_key: Value) -> LuaResult<Option<(Value, Value)>> {
    // ... implementation ...
}
```

With enhanced string interning and proper key comparison, the existing implementation should work correctly. However, we should add specific tests for table iteration with dynamically generated string keys.

## 6. Implementation Phases

### 6.1 Phase 1: Enhanced String Interning

First implement the enhanced string interning system as described in `LUA_STRING_INTERNING_AND_VALUE_SEMANTICS.md`.

### 6.2 Phase 2: Content-Based Key Comparison

Modify HashableValue equality and hashing to use content comparison for strings when handles differ.

### 6.3 Phase 3: Comprehensive Testing

Add tests specifically verifying:
- Table lookup with dynamically generated strings
- Table iteration order consistency
- Metamethod resolution with various string keys

## 7. Impact on Other Components

### 7.1 VM Implementation

The VM implementation doesn't need significant changes beyond ensuring all string creation goes through the enhanced interning system.

### 7.2 Standard Library

The standard library implementation should use the transaction system consistently to create strings, which will ensure properly interned string handles.

### 7.3 Redis Integration

For Redis Lua support, we need to ensure Redis command names are pre-interned, and that dynamically generated keys (which are common in Redis scripts) work correctly with table operations.

## 8. Conclusion

The refinements to table operations ensure proper value semantics while preserving the transaction-based architecture. By addressing the string interning issue and enhancing table key comparison, we maintain Lua's semantics while still leveraging Rust's ownership model effectively.