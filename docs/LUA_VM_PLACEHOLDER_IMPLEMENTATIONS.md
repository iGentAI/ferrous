# Lua VM Placeholder Implementations

This document catalogues all current placeholder implementations, TODOs, and incomplete features in the Ferrous Lua VM. This serves as a reference for future development work.

Last updated: July 2025

## Core VM Components

### Virtual Machine (vm.rs)

1. **Script Execution**
   ```rust
   // TODO: Setup KEYS and ARGV tables
   // TODO: Compile script
   // TODO: Execute compiled script
   // Return a placeholder string for now
   ```
   The VM's integration with Redis script execution is largely placeholder, with missing implementation for script compilation, evaluation, and environment setup.

2. **Resource Management**
   ```rust
   // TODO: Check kill flag
   ```
   Script resource limits and cancellation are not fully implemented.

3. **VM Reset**
   ```rust
   // TODO: Implement VM reset
   ```
   VM resets for reuse in a pool are not implemented.

### Module System (mod.rs)

1. **Redis Integration**
   ```rust
   "evalsha" => {
       // Simplified implementation
       Ok(RespFrame::error("ERR EVALSHA not implemented yet"))
   }
   ```
   Cached script execution isn't implemented.

   ```rust
   match subcommand.as_str() {
       "load" | "exists" | "flush" | "kill" => {
           Ok(RespFrame::error("ERR SCRIPT subcommand not implemented yet"))
       }
   }
   ```
   Script management commands are stub implementations.

2. **VM Pooling**
   ```rust
   // In a real implementation, we'd use a singleton
   ```
   The `LuaGIL` lacks proper singleton implementation.

### Compiler (compiler.rs, codegen.rs)

1. **Jump Instructions**
   ```rust
   self.emit(Self::encode_AsBx(OpCode::Jmp, 0, 0)); // Placeholder
   ```
   Several jump offsets in control flow operations are initially set as placeholders then patched later.

2. **Function Prototypes**
   ```rust
   // Use Nil as placeholder for function prototypes
   ```
   The compiler uses Nil placeholders during initial function prototype creation before resolving real references.

## Standard Library Implementation

### Base Library (stdlib.rs)

1. **Metamethod Resolution**
   ```rust
   // TODO: Check for __tostring metamethod first
   ```
   Metamethod lookup for `tostring` is incomplete.

2. **Error Handling**
   ```rust
   // TODO: Handle level argument for error location
   ```
   Error level and traceback generation for the `error()` function is missing.

3. **Load Function**
   ```rust
   // Currently a placeholder - would require more complex serialization
   let error_msg = ctx.create_string("load not fully implemented")?;
   ```
   The `load()` function for loading Lua code at runtime is a stub.

### String Library (stdlib/string.rs)

1. **String.dump**
   ```rust
   // Currently a placeholder - would require more complex serialization
   let error_msg = ctx.create_string("string.dump not implemented yet")?;
   ```
   Function serialization isn't implemented.

2. **Pattern Matching**
   ```rust
   // Placeholder implementation - pattern matching in Lua requires 
   // a significant implementation. This is a simple approach
   return Err(LuaError::NotImplemented("pattern matching in string.find".to_string()));
   ```
   Pattern matching in string functions is either not implemented or uses naive approaches.

## Memory Management

1. **Garbage Collection**
   The VM implements handle-based memory management but completely lacks garbage collection. Memory will grow unbounded with no reclamation.

## Error Handling

1. **Error Facilities**
   ```rust
   // NotImplemented error variant
   ture) => write!(f, "not implemented: {}", feature),
   ```
   Some features throw "not implemented" errors rather than providing actual implementations.

2. **Tracebacks**
   Error reporting lacks proper traceback generation and error location information.

## Redis Integration

1. **Table Conversions**
   ```rust
   // For now, return a placeholder for tables
   Ok(RespFrame::bulk_string("<table>".to_string()))
   ```
   Table-to-RESP conversion is incomplete.

## Transaction Pattern

1. **Placeholder Support**
   ```rust
   // Must return nil as a placeholder
   ```
   Some transaction operations return stub values.

## Planned Development Work

Based on the placeholders identified, these are the key areas that need implementation:

1. Complete standard library implementation, especially:
   - String pattern matching functions
   - Table manipulation functions
   - String and function serialization

2. Memory management:
   - Non-recursive garbage collection
   - Resource limits and memory pressure monitoring

3. Error handling:
   - Traceback generation
   - Error location reporting
   - Complete pcall/xpcall

4. Redis integration:
   - Script caching and management
   - KEYS/ARGV tables
   - External API boundaries

5. Transaction pattern consistency:
   - Standardize transaction creation/usage
   - Ensure proper transaction nesting/isolation