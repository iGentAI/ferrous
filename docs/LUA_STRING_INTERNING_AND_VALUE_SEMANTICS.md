# Lua VM String Interning and Value Semantics Design

## Overview

This document outlines key design refinements to the Ferrous Lua VM architecture to address fundamental semantic issues with string interning and value equality. These refinements ensure proper Lua semantics while maintaining alignment with Rust's ownership model and the transaction-based design.

## 1. String Interning Issue

### 1.1 Problem Statement

The current implementation creates a mismatch between Lua's value semantics for strings and the handle-based identity model used in the Ferrous architecture. This manifests as:

- Different handles being created for identical string content during different phases (initialization vs. module loading)
- String handles being compared by identity (pointer equality) rather than content equality
- Function lookups failing because global table keys use different handles than those used during lookup

This problem arises from the tension between Rust's ownership model (which favors unique identity) and Lua's value semantics (which require content-based equality).

### 1.2 Design Requirements

A proper solution must:

1. **Maintain Lua Semantics**: Ensure that string identity is determined by content, not by handle identity
2. **Integrate with Transactions**: Preserve the transaction-based memory safety model
3. **Optimize Common Cases**: Ensure common strings (like function names) are efficiently handled
4. **Support Determinism**: Ensure consistent behavior across executions
5. **Avoid Borrow Checker Conflicts**: Design with Rust's ownership model in mind

## 2. Recommended Solution: Arena-Based String Deduplication

### 2.1 Design Overview

The recommended approach is **Arena-Based String Deduplication with Static Lifetime Extension**:

1. **Static String Pool**: Pre-intern common strings during heap initialization
2. **String Arena**: Continue using the generational arena for string storage
3. **Enhanced Deduplication Map**: Improve string lookup to ensure consistent handles

### 2.2 Implementation Strategy

```rust
struct LuaHeap {
    // Existing fields...
    
    // Enhanced string cache for interning
    string_cache: HashMap<Vec<u8>, StringHandle>,
}

impl LuaHeap {
    // Pre-intern common strings (function names, operators, etc.)
    fn pre_intern_common_strings(&mut self) -> LuaResult<()> {
        const COMMON_STRINGS: &[&str] = &[
            // Standard library functions
            "print", "type", "tostring", "tonumber", "assert", "error",
            "select", "next", "pairs", "ipairs", "setmetatable", "getmetatable",
            // Metamethods
            "__index", "__newindex", "__call", "__tostring", "__concat",
            "__add", "__sub", "__mul", "__div", "__mod", "__pow",
            "__unm", "__len", "__eq", "__lt", "__le",
            // Redis-specific
            "redis", "call", "pcall", "KEYS", "ARGV",
        ];
        
        for s in COMMON_STRINGS {
            self.create_string_internal(s)?;
        }
        
        Ok(())
    }
    
    // Enhanced string creation with better interning
    pub(crate) fn create_string_internal(&mut self, s: &str) -> LuaResult<StringHandle> {
        let bytes = s.as_bytes().to_vec();
        
        // Check string cache first
        if let Some(&handle) = self.string_cache.get(&bytes) {
            // Validate that the cached handle is still valid
            if self.strings.contains(handle.0) {
                return Ok(handle);
            }
            // Remove stale cache entry if handle is invalid
            self.string_cache.remove(&bytes);
        }
        
        // Create new string if not found or stale
        let lua_string = LuaString::from_bytes(bytes.clone());
        let handle = StringHandle::from(self.strings.insert(lua_string));
        
        // Add to cache
        self.string_cache.insert(bytes, handle);
        
        Ok(handle)
    }
}
```

### 2.3 Module Loading Integration

The module loading process must integrate with the string interning system:

```rust
// In compiler.rs - loader module
pub fn load_module<'a>(tx: &mut HeapTransaction<'a>, module: &CompiledModule) -> LuaResult<FunctionProtoHandle> {
    // Create string handles with proper interning
    let mut string_handles = Vec::with_capacity(module.strings.len());
    for s in &module.strings {
        // Use transaction's create_string which leverages interning
        string_handles.push(tx.create_string(s)?);
    }
    
    // Rest of module loading...
}
```

## 3. Impact on Other Components

### 3.1 Tables

The string interning solution impacts table operations in several ways:

1. **Key Equality**: Table key equality must use string content, not handle identity
2. **Hash Calculation**: Hash values must be computed based on content, not handles
3. **MetaTable Resolution**: Metamethod names must be interned consistently

#### Recommended Table Refinements:

```rust
impl Table {
    // Current implementation - will work correctly with proper string interning
    pub fn get_field(&self, key: &Value) -> Option<&Value> {
        // ...
    }
    
    pub fn set_field(&mut self, key: Value, value: Value) -> LuaResult<()> {
        // ...
    }
}

// HashableValue - no changes needed if string interning is fixed
// The existing handle-based comparison will work if handles are properly interned
```

### 3.2 Function Prototypes and Closures

Function prototypes might have similar identity vs. equality issues:

1. **Function Identity**: Should identical function code be considered the same function?
2. **Upvalue Capture**: How should upvalue identity be handled across closures?

While these issues don't immediately manifest like string interning, they should be monitored.

#### Recommended Function Refinements:

- For now, the implementation can treat function prototypes as unique by handle
- Future refinements might consider function equality based on bytecode/upvalue structure

### 3.3 C Functions

C functions present unique challenges for identity:

1. **Function Pointer Comparison**: The current implementation compares function pointers directly
2. **Handle vs. Pointer**: C functions don't have handles in the same way as other VM objects

#### Recommended C Function Refinements:

```rust
// Future implementation could use a registry approach
struct CFunctionRegistry {
    functions: Vec<CFunction>,
    function_to_id: HashMap<*const (), usize>,
}

// Then C functions could be compared by ID rather than pointer
```

## 4. Future Design Considerations

### 4.1 Value Semantics Beyond Strings

Our analysis identified several areas where Lua's value semantics might conflict with Rust's ownership model:

1. **Table Equality**: Tables with the same contents should be considered equal in some contexts
2. **Function Equality**: Functions might need content-based equality in some cases
3. **Userdata Comparison**: User-defined objects need clear equality semantics

### 4.2 Memory Management Refinements

The current design lacks garbage collection, which may require future refinements:

1. **Non-Recursive Mark and Sweep**: A non-recursive GC algorithm compatible with the VM's design
2. **Generational Collection**: Separate handling for short-lived vs. long-lived objects
3. **Incremental Collection**: Breaking collection into smaller steps to avoid pauses

### 4.3 Redis-Specific Design Elements

For full Redis Lua support, additional design elements are needed:

1. **KEYS and ARGV Tables**: Special handling for script arguments
2. **redis.call and redis.pcall**: Integration with Redis command execution
3. **Script Caching**: EVALSHA command implementation
4. **Resource Limits**: Memory and CPU time restrictions for scripts

## 5. Implementation Strategy

### 5.1 Phased Approach

The recommended implementation strategy follows these phases:

1. **Phase 1**: Implement string interning with pre-interning of common strings
2. **Phase 2**: Enhance the module loader to use string interning properly
3. **Phase 3**: Add test cases specifically verifying string identity semantics
4. **Phase 4**: Address any remaining value semantics issues in tables or functions
5. **Phase 5**: Implement Redis-specific functions and features

### 5.2 Validation Strategy

To ensure the design changes work correctly:

1. **Unit Tests**: Specific tests for string interning and identity
2. **Red Flag Tests**: Tests for previously identified failure cases
3. **Semantic Tests**: Lua code that exercises value semantics edge cases
4. **Benchmarks**: Performance testing to ensure interning doesn't degrade performance

## 6. Conclusion

The string interning issue highlights a fundamental tension in implementing a dynamically-typed language with value semantics (Lua) in a statically-typed language with ownership semantics (Rust). The proposed design refinements reconcile these tensions while maintaining the core architectural principles of the Ferrous Lua VM:

1. Transaction-based heap access
2. Non-recursive state machine execution
3. Type-safe handle validation
4. Clean component separation

By addressing these design issues now, we avoid more complex problems later when implementing advanced Lua features and Redis integration.