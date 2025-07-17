//! Lua String Library Implementation
//!
//! This module implements the standard Lua 5.1 string library functions
//! following the Ferrous VM's architectural principles:
//! - All heap access through transactions
//! - No recursion - all complex operations are queued
//! - Clean separation from VM internals through ExecutionContext

use crate::lua::error::{LuaError, LuaResult};
use crate::lua::value::{Value, CFunction};
use crate::lua::handle::{StringHandle, TableHandle};
use crate::lua::vm::ExecutionContext;
use crate::lua::transaction::HeapTransaction;

/// String.byte function - returns the internal numeric codes of characters
/// Signature: string.byte(s [, i [, j]])
pub fn string_byte(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let argc = ctx.arg_count();
    if argc < 1 || argc > 3 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: argc,
        });
    }
    
    let s = ctx.get_arg_str(0)?;
    
    let i = if argc >= 2 {
        let index = ctx.get_number_arg(1)?;
        index.floor() as isize
    } else {
        1 // Default to first character
    };
    
    let j = if argc >= 3 {
        let index = ctx.get_number_arg(2)?;
        index.floor() as isize
    } else {
        i // Default to i
    };
    
    // Convert 1-based indices to 0-based
    let mut start = if i >= 0 { i - 1 } else { s.len() as isize + i };
    let mut end = if j >= 0 { j - 1 } else { s.len() as isize + j };
    
    // Clamp to valid ranges
    start = start.clamp(0, s.len() as isize - 1);
    end = end.clamp(start, s.len() as isize - 1);
    
    let bytes = s.as_bytes();
    let mut results_count = 0;
    
    for idx in start..=end {
        let byte = bytes[idx as usize];
        ctx.push_result(Value::Number(byte as f64))?;
        results_count += 1;
    }
    
    Ok(results_count)
}

/// String.char function - returns a string built from character codes
/// Signature: string.char(...)
pub fn string_char(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let argc = ctx.arg_count();
    if argc < 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: argc,
        });
    }
    
    let mut bytes = Vec::with_capacity(argc);
    
    for i in 0..argc {
        let code = ctx.get_number_arg(i)?.floor() as isize;
        
        if code < 0 || code > 255 {
            return Err(LuaError::RuntimeError(
                format!("bad argument #{} to 'char' (value out of range)", i+1)
            ));
        }
        
        bytes.push(code as u8);
    }
    
    let s = String::from_utf8_lossy(&bytes).to_string();
    let handle = ctx.create_string(&s)?;
    
    ctx.push_result(Value::String(handle))?;
    
    Ok(1)
}

/// String.dump function - returns a binary representation of a function
/// Signature: string.dump(function)
pub fn string_dump(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    // Currently a placeholder - would require more complex serialization
    // of the function's bytecode and constants
    let error_msg = ctx.create_string("string.dump not implemented yet")?;
    ctx.push_result(Value::String(error_msg))?;
    
    Ok(1)
}

/// String.find function - finds a pattern in a string
/// Signature: string.find(s, pattern [, init [, plain]])
pub fn string_find(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let argc = ctx.arg_count();
    if argc < 2 || argc > 4 {
        return Err(LuaError::ArgumentError {
            expected: 2,
            got: argc,
        });
    }
    
    let s = ctx.get_arg_str(0)?;
    let pattern = ctx.get_arg_str(1)?;
    
    let init = if argc >= 3 {
        let idx = ctx.get_number_arg(2)?;
        idx.floor() as isize
    } else {
        1 // Default to start of string
    };
    
    let plain = if argc >= 4 {
        ctx.get_bool_arg(3)?
    } else {
        false // Default to pattern matching
    };
    
    // Convert to 0-based index
    let start_idx = if init >= 0 { 
        (init - 1).max(0) as usize 
    } else {
        (s.len() as isize + init).max(0) as usize
    };
    
    if start_idx >= s.len() {
        // No match possible
        ctx.push_result(Value::Nil)?;
        return Ok(1);
    }
    
    if plain {
        // Plain string search
        if let Some(pos) = s[start_idx..].find(&pattern) {
            let start_pos = start_idx + pos;
            let end_pos = start_pos + pattern.len() - 1;
            
            // Lua indices are 1-based
            ctx.push_result(Value::Number((start_pos + 1) as f64))?;
            ctx.push_result(Value::Number((end_pos + 1) as f64))?;
            return Ok(2);
        }
    } else {
        // Implement pattern matching - this is a simple placeholder
        // In a full implementation, this would use Lua's pattern matching rules
        return Err(LuaError::NotImplemented("pattern matching in string.find".to_string()));
    }
    
    // No match found
    ctx.push_result(Value::Nil)?;
    
    Ok(1)
}

/// String.format function - formats a string
/// Signature: string.format(formatstring, ...)
pub fn string_format(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let argc = ctx.arg_count();
    if argc < 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: argc,
        });
    }
    
    let format_string = ctx.get_arg_str(0)?;
    
    // Simple initial implementation that handles basic formatting
    // A full implementation would need to handle all Lua's format specifiers
    
    let mut result = String::new();
    let mut format_chars = format_string.chars().peekable();
    let mut arg_index = 1;
    
    while let Some(c) = format_chars.next() {
        if c == '%' {
            if let Some(&next_char) = format_chars.peek() {
                if next_char == '%' {
                    // Double % escapes to single %
                    format_chars.next();
                    result.push('%');
                    continue;
                }
            }
            
            // Handle format specifiers
            match format_chars.next() {
                Some('s') => {
                    // String
                    if arg_index >= argc {
                        return Err(LuaError::RuntimeError(
                            format!("bad argument #{} to 'format' (no value)", arg_index + 1)
                        ));
                    }
                    
                    let value = ctx.get_arg(arg_index)?;
                    match value {
                        Value::String(handle) => {
                            let s = ctx.get_string_from_handle(handle)?;
                            result.push_str(&s);
                        },
                        Value::Number(n) => {
                            result.push_str(&n.to_string());
                        },
                        Value::Boolean(b) => {
                            result.push_str(if b { "true" } else { "false" });
                        },
                        Value::Nil => {
                            result.push_str("nil");
                        },
                        _ => {
                            return Err(LuaError::TypeError {
                                expected: "string".to_string(),
                                got: value.type_name().to_string(),
                            });
                        }
                    }
                    arg_index += 1;
                },
                Some('d') | Some('i') => {
                    // Integer
                    if arg_index >= argc {
                        return Err(LuaError::RuntimeError(
                            format!("bad argument #{} to 'format' (no value)", arg_index + 1)
                        ));
                    }
                    
                    let n = ctx.get_number_arg(arg_index)?;
                    result.push_str(&format!("{}", n.floor() as i64));
                    arg_index += 1;
                },
                Some('f') => {
                    // Float
                    if arg_index >= argc {
                        return Err(LuaError::RuntimeError(
                            format!("bad argument #{} to 'format' (no value)", arg_index + 1)
                        ));
                    }
                    
                    let n = ctx.get_number_arg(arg_index)?;
                    result.push_str(&n.to_string());
                    arg_index += 1;
                },
                // Other format specifiers would be implemented here
                Some(c) => {
                    return Err(LuaError::RuntimeError(
                        format!("invalid format specifier '%{}'", c)
                    ));
                },
                None => {
                    return Err(LuaError::RuntimeError(
                        format!("invalid format (ends with '%')")
                    ));
                }
            }
        } else {
            result.push(c);
        }
    }
    
    // Create string and return
    let handle = ctx.create_string(&result)?;
    ctx.push_result(Value::String(handle))?;
    
    Ok(1)
}

/// String.len function - returns the length of a string
/// Signature: string.len(s)
pub fn string_len(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let s = ctx.get_arg_str(0)?;
    ctx.push_result(Value::Number(s.len() as f64))?;
    
    Ok(1)
}

/// String.lower function - converts a string to lowercase
/// Signature: string.lower(s)
pub fn string_lower(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let s = ctx.get_arg_str(0)?;
    let result = s.to_lowercase();
    
    let handle = ctx.create_string(&result)?;
    ctx.push_result(Value::String(handle))?;
    
    Ok(1)
}

/// String.match function - finds a pattern in a string
/// Signature: string.match(s, pattern [, init])
pub fn string_match(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let argc = ctx.arg_count();
    if argc < 2 || argc > 3 {
        return Err(LuaError::ArgumentError {
            expected: 2,
            got: argc,
        });
    }
    
    let s = ctx.get_arg_str(0)?;
    let pattern = ctx.get_arg_str(1)?;
    
    // Placeholder implementation - pattern matching in Lua requires
    // a significant implementation. This is a simple approach that
    // just returns the first match without captures
    
    if let Some(capture) = s.find(&pattern) {
        let matched = &s[capture..capture+pattern.len()];
        let handle = ctx.create_string(matched)?;
        ctx.push_result(Value::String(handle))?;
        return Ok(1);
    }
    
    // No match
    ctx.push_result(Value::Nil)?;
    
    Ok(1)
}

/// String.rep function - repeats a string
/// Signature: string.rep(s, n [, sep])
pub fn string_rep(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let argc = ctx.arg_count();
    if argc < 2 || argc > 3 {
        return Err(LuaError::ArgumentError {
            expected: 2,
            got: argc,
        });
    }
    
    let s = ctx.get_arg_str(0)?;
    let n = ctx.get_number_arg(1)?.floor() as isize;
    
    if n < 0 {
        return Err(LuaError::RuntimeError(
            format!("bad argument #2 to 'rep' (non-negative number expected)")
        ));
    }
    
    let sep = if argc >= 3 {
        ctx.get_arg_str(2)?
    } else {
        "".to_string()
    };
    
    if n == 0 {
        // Return empty string
        let handle = ctx.create_string("")?;
        ctx.push_result(Value::String(handle))?;
        return Ok(1);
    }
    
    // Simple implementation without using Rust's repeat method
    // to better match Lua's behavior with the separator
    let mut result = String::with_capacity(s.len() * n as usize + sep.len() * (n as usize - 1));
    
    for i in 0..n {
        result.push_str(&s);
        if i < n - 1 {
            result.push_str(&sep);
        }
    }
    
    let handle = ctx.create_string(&result)?;
    ctx.push_result(Value::String(handle))?;
    
    Ok(1)
}

/// String.reverse function - reverses a string
/// Signature: string.reverse(s)
pub fn string_reverse(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let s = ctx.get_arg_str(0)?;
    let result: String = s.chars().rev().collect();
    
    let handle = ctx.create_string(&result)?;
    ctx.push_result(Value::String(handle))?;
    
    Ok(1)
}

/// String.sub function - returns a substring
/// Signature: string.sub(s, i [, j])
pub fn string_sub(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let argc = ctx.arg_count();
    if argc < 2 || argc > 3 {
        return Err(LuaError::ArgumentError {
            expected: 2,
            got: argc,
        });
    }
    
    let s = ctx.get_arg_str(0)?;
    let i = ctx.get_number_arg(1)?.floor() as isize;
    
    // Default j to -1 (last character)
    let j = if argc >= 3 {
        ctx.get_number_arg(2)?.floor() as isize
    } else {
        -1
    };
    
    // Convert 1-based indices to 0-based
    let start_idx = if i >= 0 { 
        (i - 1).max(0) as usize 
    } else {
        (s.len() as isize + i).max(0) as usize
    };
    
    let end_idx = if j >= 0 { 
        (j as usize).min(s.len()) 
    } else {
        (s.len() as isize + j + 1).max(0) as usize
    };
    
    // Handle invalid ranges
    if start_idx >= s.len() || start_idx >= end_idx {
        let handle = ctx.create_string("")?;
        ctx.push_result(Value::String(handle))?;
        return Ok(1);
    }
    
    // Extract substring
    let result = &s[start_idx..end_idx];
    let handle = ctx.create_string(result)?;
    ctx.push_result(Value::String(handle))?;
    
    Ok(1)
}

/// String.upper function - converts a string to uppercase
/// Signature: string.upper(s)
pub fn string_upper(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let s = ctx.get_arg_str(0)?;
    let result = s.to_uppercase();
    
    let handle = ctx.create_string(&result)?;
    ctx.push_result(Value::String(handle))?;
    
    Ok(1)
}



/// Create a table with all string functions
pub fn create_string_lib() -> Vec<(&'static str, CFunction)> {
    let mut string_funcs = Vec::new();
    
    // Add all string functions
    string_funcs.push(("byte", string_byte as CFunction));
    string_funcs.push(("char", string_char as CFunction));
    string_funcs.push(("dump", string_dump as CFunction));
    string_funcs.push(("find", string_find as CFunction));
    string_funcs.push(("format", string_format as CFunction));
    string_funcs.push(("len", string_len as CFunction));
    string_funcs.push(("lower", string_lower as CFunction));
    string_funcs.push(("match", string_match as CFunction));
    string_funcs.push(("rep", string_rep as CFunction));
    string_funcs.push(("reverse", string_reverse as CFunction));
    string_funcs.push(("sub", string_sub as CFunction));
    string_funcs.push(("upper", string_upper as CFunction));
    
    string_funcs
}

/// Initialize the string library in a Lua state
pub fn init_string_lib(vm: &mut crate::lua::vm::LuaVM) -> LuaResult<()> {
    use crate::lua::transaction::HeapTransaction;
    
    // Create a transaction
    let mut tx = HeapTransaction::new(vm.heap_mut());
    
    // Create string table
    let string_table = tx.create_table()?;
    
    // Get globals table
    let globals = tx.get_globals_table()?;
    
    // Create handle for "string" string
    let string_name = tx.create_string("string")?;
    
    // Add string table to globals
    tx.set_table_field(globals, Value::String(string_name), Value::Table(string_table))?;
    
    // Add string functions
    let funcs = create_string_lib();
    for (name, func) in funcs {
        let name_handle = tx.create_string(name)?;
        tx.set_table_field(string_table, Value::String(name_handle), Value::CFunction(func))?;
    }
    
    // Commit the transaction
    tx.commit()?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lua::vm::LuaVM;
    
    #[test]
    fn test_string_functions() {
        let mut vm = LuaVM::new().unwrap();
        
        // Initialize string library
        init_string_lib(&mut vm).unwrap();
        
        // Test string functions by running a simple script
        // This would be expanded in a real test suite
    }
}