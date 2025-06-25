//! cjson library for Lua - Redis-compatible JSON encoding/decoding
//!
//! This module implements the cjson library required for Redis Lua compatibility.
//! It provides JSON encoding/decoding between Lua values and JSON strings.

use crate::lua_new::vm::{LuaVM, ExecutionContext};
use crate::lua_new::value::{Value, TableHandle};
use crate::lua_new::error::{LuaError, Result};
use std::collections::HashSet;

/// Register cjson library in the VM
pub fn register(vm: &mut LuaVM) -> Result<()> {
    // Create cjson table
    let cjson_table = vm.heap.alloc_table();
    
    // Register encode function
    let encode_key = vm.heap.create_string("encode");
    vm.heap.get_table_mut(cjson_table)?.set(
        Value::String(encode_key),
        Value::CFunction(cjson_encode)
    );
    
    // Register decode function
    let decode_key = vm.heap.create_string("decode");
    vm.heap.get_table_mut(cjson_table)?.set(
        Value::String(decode_key),
        Value::CFunction(cjson_decode)
    );
    
    // Register encode_sparse_array function
    let sparse_key = vm.heap.create_string("encode_sparse_array");
    vm.heap.get_table_mut(cjson_table)?.set(
        Value::String(sparse_key),
        Value::CFunction(cjson_encode_sparse_array)
    );
    
    // Set default configuration
    let null_key = vm.heap.create_string("null");
    let null_value = vm.heap.alloc_table(); // Special marker for JSON null
    vm.heap.get_table_mut(cjson_table)?.set(
        Value::String(null_key),
        Value::Table(null_value)
    );
    
    // Set in globals
    let globals = vm.globals();
    let cjson_name = vm.heap.create_string("cjson");
    vm.heap.get_table_mut(globals)?.set(
        Value::String(cjson_name),
        Value::Table(cjson_table)
    );
    
    Ok(())
}

/// cjson.encode implementation using an approach that avoids borrow checker issues
fn cjson_encode(ctx: &mut ExecutionContext) -> Result<i32> {
    if ctx.get_arg_count() < 1 {
        return Err(LuaError::Runtime("cjson.encode requires a value to encode".to_string()));
    }
    
    let value = ctx.get_arg(0)?;
    
    let mut visited = HashSet::new();
    let json = encode_value(ctx, value, &mut visited, 0)?;
    
    // Push the result
    let handle = ctx.heap().create_string(&json);
    ctx.push_result(Value::String(handle))?;
    
    Ok(1)
}

/// Properly escape a string for JSON
fn escape_json_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 2);
    result.push('"');
    
    for ch in s.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\u{0008}' => result.push_str("\\b"),
            '\u{000C}' => result.push_str("\\f"),
            c if c.is_control() => {
                // Escape control characters as \uXXXX
                result.push_str(&format!("\\u{:04x}", c as u32));
            },
            c => result.push(c),
        }
    }
    
    result.push('"');
    result
}

/// Encode a value to JSON
fn encode_value(ctx: &mut ExecutionContext, value: Value, visited: &mut HashSet<u32>, depth: usize) -> Result<String> {
    // Prevent stack overflow on deeply nested structures
    if depth > 32 {
        return Ok("null".to_string());
    }
    
    match value {
        Value::Nil => Ok("null".to_string()),
        
        Value::Boolean(b) => Ok(if b { "true".to_string() } else { "false".to_string() }),
        
        Value::Number(n) => {
            if n.is_nan() || n.is_infinite() {
                Ok("null".to_string()) // JSON doesn't support NaN/Infinity
            } else if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
                // Format as integer
                Ok((n as i64).to_string()) 
            } else {
                // Format as float
                Ok(n.to_string())
            }
        },
        
        Value::String(s) => {
            // Get string bytes
            let bytes = ctx.heap().get_string(s)?;
            let s = std::str::from_utf8(bytes).map_err(|_| LuaError::InvalidEncoding)?;
            
            // Escape for JSON
            Ok(escape_json_string(s))
        },
        
        Value::Table(t) => encode_table(ctx, t, visited, depth),
        
        _ => Ok("null".to_string()), // Functions, threads, etc.
    }
}

/// Encode a table to JSON, avoiding borrow checker issues
fn encode_table(ctx: &mut ExecutionContext, table: TableHandle, visited: &mut HashSet<u32>, depth: usize) -> Result<String> {
    // Check for cycles to prevent stack overflow
    let handle_idx = table.0.index;
    if visited.contains(&handle_idx) {
        return Ok("null".to_string()); // Break cycles
    }
    
    // Mark as visited for cycle detection
    visited.insert(handle_idx);
    
    // Phase 1: Extract all data we need to avoid multiple borrows
    struct TableData {
        array_values: Vec<Value>,
        map_entries: Vec<(Value, Value)>,
        is_array: bool,
    }
    
    // Collect all the data we need in a single pass
    let data = {
        let table_obj = ctx.heap().get_table(table)?;
        
        let array_values = table_obj.array.clone();
        
        let mut map_entries = Vec::new();
        let mut is_array = true;
        
        for (k, &v) in &table_obj.map {
            // Check if this is a key that would make it not an array
            match k {
                Value::Number(n) => {
                    if n.fract() != 0.0 || *n <= 0.0 || *n as usize > array_values.len() + map_entries.len() {
                        is_array = false;
                    }
                },
                _ => {
                    // Non-numeric key means it's not an array
                    is_array = false;
                }
            }
            
            map_entries.push((k.clone(), v));
        }
        
        // If there's no array part and array_like is true, double check all keys are sequential
        if array_values.is_empty() && is_array && !map_entries.is_empty() {
            // Sort entries by key (for numeric keys)
            let mut numeric_keys = map_entries.iter()
                .filter_map(|(k, _)| {
                    if let Value::Number(n) = k {
                        if n.fract() == 0.0 && *n > 0.0 {
                            Some(*n as usize)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            
            // Check if keys are a sequential range starting from 1
            numeric_keys.sort_unstable();
            if numeric_keys.is_empty() || numeric_keys[0] != 1 {
                is_array = false;
            } else {
                for i in 1..numeric_keys.len() {
                    if numeric_keys[i] != numeric_keys[i-1] + 1 {
                        is_array = false;
                        break;
                    }
                }
            }
        }
        
        // Return all collected data
        TableData { array_values, map_entries, is_array }
    };
    
    // Phase 2: Now we can format the JSON without borrowing from ctx
    let result = if data.is_array && (data.array_values.len() > 0 || data.map_entries.len() > 0) {
        // Encode as JSON array
        let mut json = String::from("[");
        
        // First process array values
        for (i, val) in data.array_values.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            
            json.push_str(&encode_value(ctx, *val, visited, depth + 1)?);
        }
        
        // Then process numeric indices from map that extend the array
        if !data.map_entries.is_empty() {
            let mut numeric_entries = Vec::new();
            
            for (k, v) in data.map_entries.iter() {
                if let Value::Number(n) = k {
                    if n.fract() == 0.0 && *n > 0.0 {
                        let idx = *n as usize;
                        if idx > data.array_values.len() {
                            numeric_entries.push((idx, *v));
                        }
                    }
                }
            }
            
            // Sort by index
            numeric_entries.sort_by_key(|(idx, _)| *idx);
            
            // Add to JSON
            let mut last_idx = data.array_values.len();
            for (idx, val) in numeric_entries {
                // Insert comma if needed
                if last_idx > 0 {
                    json.push(',');
                }
                
                // Insert null values for any gaps
                for _ in last_idx + 1..idx {
                    json.push_str("null,");
                }
                
                // Add the value
                json.push_str(&encode_value(ctx, val, visited, depth + 1)?);
                
                last_idx = idx;
            }
        }
        
        json.push(']');
        json
    } else {
        // Encode as JSON object
        let mut json = String::from("{");
        let mut first = true;
        
        // First add string keys (sorted for deterministic output)
        let mut string_keys = Vec::new();
        for (k, v) in data.map_entries.iter() {
            if let Value::String(s) = k {
                let bytes = ctx.heap().get_string(*s)?;
                match std::str::from_utf8(bytes) {
                    Ok(key_str) => string_keys.push((key_str.to_string(), *v)),
                    Err(_) => continue, // Skip invalid UTF-8
                }
            }
        }
        
        // Sort string keys
        string_keys.sort_by(|(a, _), (b, _)| a.cmp(b));
        
        // Add string entries
        for (key, val) in string_keys {
            if !first {
                json.push(',');
            }
            first = false;
            
            json.push_str(&escape_json_string(&key));
            json.push(':');
            json.push_str(&encode_value(ctx, val, visited, depth + 1)?);
        }
        
        // Add numeric keys
        let mut numeric_keys = Vec::new();
        for (k, v) in data.map_entries.iter() {
            if let Value::Number(n) = k {
                numeric_keys.push((n.to_string(), *v));
            }
        }
        
        // Sort numeric keys
        numeric_keys.sort_by(|(a, _), (b, _)| {
            // Try parsing as numbers first for natural sorting
            if let (Ok(a_num), Ok(b_num)) = (a.parse::<f64>(), b.parse::<f64>()) {
                return a_num.partial_cmp(&b_num).unwrap_or(std::cmp::Ordering::Equal);
            }
            a.cmp(b)
        });
        
        // Add numeric entries
        for (key, val) in numeric_keys {
            if !first {
                json.push(',');
            }
            first = false;
            
            json.push_str(&escape_json_string(&key));
            json.push(':');
            json.push_str(&encode_value(ctx, val, visited, depth + 1)?);
        }
        
        // Add array values as numeric indices if not already added
        let array_offset = if data.is_array { 0 } else { 1 };
        for (i, val) in data.array_values.iter().enumerate() {
            // Skip nil values
            if !matches!(val, Value::Nil) {
                // For arrays, we've already added these values, so skip
                if data.is_array {
                    continue;
                }
                
                if !first {
                    json.push(',');
                }
                first = false;
                
                let key_str = (i + array_offset).to_string();
                json.push_str(&escape_json_string(&key_str));
                json.push(':');
                json.push_str(&encode_value(ctx, *val, visited, depth + 1)?);
            }
        }
        
        json.push('}');
        json
    };
    
    // Unmark as visited
    visited.remove(&handle_idx);
    
    Ok(result)
}

/// cjson.decode implementation
fn cjson_decode(ctx: &mut ExecutionContext) -> Result<i32> {
    if ctx.get_arg_count() < 1 {
        return Err(LuaError::Runtime("cjson.decode requires a JSON string".to_string()));
    }
    
    // Get string argument but mark as unused with underscore
    let _json_str = match ctx.get_arg(0)? {
        Value::String(s) => {
            let bytes = ctx.heap().get_string(s)?;
            std::str::from_utf8(bytes)
                .map_err(|_| LuaError::InvalidEncoding)?
                .to_string()
        },
        _ => return Err(LuaError::TypeError("cjson.decode requires a string".to_string())),
    };
    
    // For now, just return an empty table
    // In a full implementation, we would parse the JSON string here
    let table = ctx.heap().alloc_table();
    ctx.push_result(Value::Table(table))?;
    
    // In a future implementation, parse and decode the JSON into a Lua table
    
    Ok(1)
}

/// cjson.encode_sparse_array implementation
fn cjson_encode_sparse_array(_ctx: &mut ExecutionContext) -> Result<i32> {
    // This is a configuration function, just return 0
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lua_new::VMConfig;
    
    #[test]
    fn test_json_encode_basic() {
        let config = VMConfig::default();
        let mut vm = LuaVM::new(config);
        
        // Register cjson
        register(&mut vm).unwrap();
        
        // Test encoding various values
        // TODO: Add actual test execution once VM is more complete
    }
    
    #[test]
    fn test_json_decode_basic() {
        let config = VMConfig::default();
        let mut vm = LuaVM::new(config);
        
        // Register cjson
        register(&mut vm).unwrap();
        
        // Test decoding various JSON strings
        // TODO: Add actual test execution once VM is more complete
    }
}