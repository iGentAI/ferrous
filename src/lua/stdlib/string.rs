//! Lua String Library Implementation
//!
//! This module implements the standard Lua 5.1 string library functions
//! following the Ferrous VM's architectural principles.

use crate::lua::error::{LuaError, LuaResult};
use crate::lua::value::{Value, CFunction};
use crate::lua::handle::StringHandle;
use crate::lua::refcell_vm::ExecutionContext;

/// Helper function to normalize string indices (1-based to 0-based)
fn normalize_string_index(index: isize, len: isize) -> usize {
    if index < 0 {
        // Negative indices count from the end of the string
        (len + index + 1).max(0) as usize
    } else if index == 0 {
        // Lua strings start at index 1, so 0 becomes 1
        0
    } else {
        // Adjust 1-based index to 0-based
        (index - 1) as usize
    }
}

/// string.byte function - returns the internal numeric codes of characters
/// Signature: string.byte(s [, i [, j]])
pub fn string_byte(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let nargs = ctx.arg_count();
    if nargs < 1 || nargs > 3 {
        return Err(LuaError::BadArgument {
            func: Some("byte".to_string()),
            arg: 1,
            msg: "1 to 3 arguments expected".to_string()
        });
    }
    
    let s = ctx.get_arg_str(0)?;
    
    // Default start is 1
    let start = if nargs >= 2 {
        ctx.get_number_arg(1)? as isize
    } else {
        1
    };
    
    // Default end is start
    let end = if nargs >= 3 {
        ctx.get_number_arg(2)? as isize
    } else {
        start
    };
    
    let bytes = s.as_bytes();
    let len = bytes.len() as isize;
    
    // Convert to 0-based indices
    let start_idx = normalize_string_index(start, len);
    let end_idx = normalize_string_index(end, len).min(bytes.len() - 1);
    
    // Invalid range returns no values
    if start_idx >= bytes.len() || start_idx > end_idx {
        return Ok(0);
    }
    
    // Return byte values
    let mut count = 0;
    for i in start_idx..=end_idx {
        ctx.push_result(Value::Number(bytes[i] as f64))?;
        count += 1;
    }
    
    Ok(count)
}

/// string.char function - returns a string from character codes
/// Signature: string.char(...)
pub fn string_char(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let nargs = ctx.arg_count();
    if nargs < 1 {
        return Err(LuaError::BadArgument {
            func: Some("char".to_string()),
            arg: 1,
            msg: "at least 1 argument expected".to_string()
        });
    }
    
    let mut bytes = Vec::with_capacity(nargs);
    
    for i in 0..nargs {
        let n = ctx.get_number_arg(i)?;
        
        if n < 0.0 || n > 255.0 || n.fract() != 0.0 {
            return Err(LuaError::BadArgument {
                func: Some("char".to_string()),
                arg: i as i32 + 1,
                msg: "value must be an integer between 0 and 255".to_string()
            });
        }
        
        bytes.push(n as u8);
    }
    
    // Create result string
    let s = String::from_utf8_lossy(&bytes).to_string();
    let handle = ctx.create_string(&s)?;
    ctx.push_result(Value::String(handle))?;
    
    Ok(1)
}

/// string.dump function - returns a binary representation of a function
/// Signature: string.dump(function)
pub fn string_dump(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("dump".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let value = ctx.get_arg(0)?;
    if !matches!(value, Value::Closure(_)) {
        return Err(LuaError::TypeError {
            expected: "function".to_string(),
            got: value.type_name().to_string(),
        });
    }
    
    // This is a placeholder implementation - in a real VM, this would serialize the function bytecode
    let handle = ctx.create_string("<function bytecode not implemented>")?;
    ctx.push_result(Value::String(handle))?;
    
    Ok(1)
}

/// string.find function - finds a pattern in a string
/// Signature: string.find(s, pattern [, init [, plain]])
pub fn string_find(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let nargs = ctx.arg_count();
    if nargs < 2 || nargs > 4 {
        return Err(LuaError::BadArgument {
            func: Some("find".to_string()),
            arg: 1,
            msg: "2 to 4 arguments expected".to_string()
        });
    }
    
    let s = ctx.get_arg_str(0)?;
    let pattern = ctx.get_arg_str(1)?;
    
    // Get init (default 1)
    let init = if nargs >= 3 {
        let init_num = ctx.get_number_arg(2)? as isize;
        if init_num == 0 {
            1
        } else {
            init_num
        }
    } else {
        1
    };
    
    // Get plain flag (default false)
    let plain = if nargs >= 4 {
        match ctx.get_arg(3)? {
            Value::Boolean(b) => b,
            Value::Nil => false,
            _ => true, // In Lua, any non-nil/false value is considered true
        }
    } else {
        false
    };
    
    // Convert to 0-based index
    let start_idx = normalize_string_index(init, s.len() as isize);
    if start_idx >= s.len() {
        // No match possible if start is past end of string
        ctx.push_result(Value::Nil)?;
        return Ok(1);
    }
    
    // Simple string search (no pattern matching yet)
    if plain {
        if let Some(pos) = s[start_idx..].find(&pattern) {
            let match_start = start_idx + pos;
            let match_end = match_start + pattern.len() - 1;
            
            // Convert to 1-based indices
            ctx.push_result(Value::Number((match_start + 1) as f64))?;
            ctx.push_result(Value::Number((match_end + 1) as f64))?;
            return Ok(2);
        } else {
            ctx.push_result(Value::Nil)?;
            return Ok(1);
        }
    } else {
        // Pattern matching not implemented yet
        return Err(LuaError::NotImplemented("pattern matching in string.find".to_string()));
    }
}

/// string.format function - formats a string
/// Signature: string.format(formatstring, ...)
pub fn string_format(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let nargs = ctx.arg_count();
    if nargs < 1 {
        return Err(LuaError::BadArgument {
            func: Some("format".to_string()),
            arg: 1,
            msg: "at least 1 argument expected".to_string()
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
            let mut fmt_spec = String::new();
            fmt_spec.push('%');
            
            let mut c = ' ';
            let mut reached_conv = false;
            
            while let Some(next_c) = format_chars.next() {
                fmt_spec.push(next_c);
                c = next_c;
                
                // Check if we reached a conversion specifier
                if matches!(c, 'd' | 'i' | 'u' | 'o' | 'x' | 'X' | 'f' | 'F' | 'e' | 'E' | 
                             'g' | 'G' | 'a' | 'A' | 'c' | 's' | 'p' | '%') {
                    reached_conv = true;
                    break;
                }
            }
            
            if !reached_conv {
                return Err(LuaError::RuntimeError(
                    "invalid format (ends with '%')".to_string()
                ));
            }
            
            if c == '%' {
                result.push('%');
                continue;
            }
            
            // Check sufficient arguments
            if arg_index >= nargs {
                return Err(LuaError::BadArgument {
                    func: Some("format".to_string()),
                    arg: arg_index as i32 + 1,
                    msg: "no value".to_string()
                });
            }
            
            // Get the value and format it
            let value = ctx.get_arg(arg_index)?;
            match (c, value.clone()) {
                ('s', Value::String(handle)) => {
                    let s = ctx.get_string_from_handle(handle)?;
                    result.push_str(&s);
                },
                ('s', _) => {
                    result.push_str(&value.type_name());
                },
                ('d' | 'i', Value::Number(n)) => {
                    let i = n as i64;
                    result.push_str(&i.to_string());
                },
                ('f', Value::Number(n)) => {
                    result.push_str(&n.to_string());
                },
                _ => {
                    // Just push the value's string representation
                    match value {
                        Value::Nil => result.push_str("nil"),
                        Value::Boolean(b) => result.push_str(if b { "true" } else { "false" }),
                        Value::Number(n) => result.push_str(&n.to_string()),
                        Value::String(h) => result.push_str(&ctx.get_string_from_handle(h)?),
                        _ => result.push_str(&value.type_name()),
                    }
                }
            }
            
            arg_index += 1;
        } else {
            result.push(c);
        }
    }
    
    // Create result string
    let handle = ctx.create_string(&result)?;
    ctx.push_result(Value::String(handle))?;
    
    Ok(1)
}

/// Iterator function for gmatch  
fn gmatch_iterator(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    // Always return nil (end of iteration) - this is a placeholder
    ctx.push_result(Value::Nil)?;
    Ok(1)
}

/// string.gmatch function - iterates a string pattern
/// Signature: string.gmatch(s, pattern)
pub fn string_gmatch(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 2 {
        return Err(LuaError::BadArgument {
            func: Some("gmatch".to_string()),
            arg: 1,
            msg: "2 arguments expected".to_string()
        });
    }
    
    // Not implemented yet - return a dummy function
    let iter_func: CFunction = gmatch_iterator;
    
    ctx.push_result(Value::CFunction(iter_func))?;
    
    // For a real gmatch implementation, we'd also push the
    // string and pattern as upvalues, but this is a placeholder
    
    Ok(1)
}

/// string.gsub function - global string replacement
/// Signature: string.gsub(s, pattern, repl [, n])
pub fn string_gsub(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let nargs = ctx.arg_count();
    if nargs < 3 || nargs > 4 {
        return Err(LuaError::BadArgument {
            func: Some("gsub".to_string()),
            arg: 1,
            msg: "3 or 4 arguments expected".to_string()
        });
    }
    
    // Not fully implemented - placeholder implementation for basic string replacement
    let s = ctx.get_arg_str(0)?;
    let pattern = ctx.get_arg_str(1)?;
    
    // Get the replacement
    let replacement = match ctx.get_arg(2)? {
        Value::String(h) => ctx.get_string_from_handle(h)?,
        Value::Table(_) => return Err(LuaError::NotImplemented("table replacement in gsub".to_string())),
        Value::Closure(_) | Value::CFunction(_) => return Err(LuaError::NotImplemented("function replacement in gsub".to_string())),
        _ => return Err(LuaError::TypeError {
            expected: "string, table, or function".to_string(),
            got: ctx.get_arg(2)?.type_name().to_string(),
        }),
    };
    
    // Get max replacements (default all)
    let max_replacements = if nargs >= 4 {
        let n = ctx.get_number_arg(3)?;
        if n < 0.0 {
            0
        } else {
            n as usize
        }
    } else {
        usize::MAX
    };
    
    // Only support plain string replacement for now (not pattern matching)
    let mut result = s.clone();
    let mut count = 0;
    
    if !pattern.is_empty() && max_replacements > 0 {
        result = s.replacen(&pattern, &replacement, max_replacements);
        
        // Count replacements
        let orig_len = s.len();
        let new_len = result.len();
        let pat_len = pattern.len();
        let rep_len = replacement.len();
        
        if pat_len == rep_len {
            // If same length, we need to actually compare
            let s_chars: Vec<char> = s.chars().collect();
            let result_chars: Vec<char> = result.chars().collect();
            
            let mut i = 0;
            while i < s_chars.len() && i < result_chars.len() {
                if s_chars[i] != result_chars[i] {
                    count += 1;
                    i += pat_len.max(1);
                } else {
                    i += 1;
                }
            }
        } else {
            // Different lengths, we can compute
            if pat_len > rep_len {
                count = (orig_len - new_len) / (pat_len - rep_len);
            } else if rep_len > pat_len {
                count = (new_len - orig_len) / (rep_len - pat_len);
            }
        }
    }
    
    // Return result and count
    let handle = ctx.create_string(&result)?;
    ctx.push_result(Value::String(handle))?;
    ctx.push_result(Value::Number(count as f64))?;
    
    Ok(2)
}

/// string.len function - returns the length of a string
/// Signature: string.len(s)
pub fn string_len(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("len".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let s = ctx.get_arg_str(0)?;
    ctx.push_result(Value::Number(s.len() as f64))?;
    
    Ok(1)
}

/// string.lower function - converts a string to lowercase
/// Signature: string.lower(s)
pub fn string_lower(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("lower".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let s = ctx.get_arg_str(0)?;
    let result = s.to_lowercase();
    
    let handle = ctx.create_string(&result)?;
    ctx.push_result(Value::String(handle))?;
    
    Ok(1)
}

/// string.match function - finds a pattern in a string
/// Signature: string.match(s, pattern [, init])
pub fn string_match(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let nargs = ctx.arg_count();
    if nargs < 2 || nargs > 3 {
        return Err(LuaError::BadArgument {
            func: Some("match".to_string()),
            arg: 1,
            msg: "2 or 3 arguments expected".to_string()
        });
    }
    
    let s = ctx.get_arg_str(0)?;
    let pattern = ctx.get_arg_str(1)?;
    
    // Get init (default 1)
    let init = if nargs >= 3 {
        let init_num = ctx.get_number_arg(2)? as isize;
        if init_num == 0 {
            1
        } else {
            init_num
        }
    } else {
        1
    };
    
    // Convert to 0-based index and bounds check
    let start_idx = normalize_string_index(init, s.len() as isize);
    if start_idx >= s.len() {
        // No match possible
        ctx.push_result(Value::Nil)?;
        return Ok(1);
    }
    
    // Simple pattern matching placeholder for now
    // In full implementation, this would handle proper Lua pattern matching
    // This is just a basic substring match
    if let Some(pos) = s[start_idx..].find(&pattern) {
        let matches = &s[start_idx + pos..start_idx + pos + pattern.len()];
        let handle = ctx.create_string(matches)?;
        ctx.push_result(Value::String(handle))?;
        return Ok(1);
    }
    
    // No match
    ctx.push_result(Value::Nil)?;
    Ok(1)
}

/// string.rep function - repeats a string
/// Signature: string.rep(s, n [, sep])
pub fn string_rep(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let nargs = ctx.arg_count();
    if nargs < 2 || nargs > 3 {
        return Err(LuaError::BadArgument {
            func: Some("rep".to_string()),
            arg: 1,
            msg: "2 or 3 arguments expected".to_string()
        });
    }
    
    let s = ctx.get_arg_str(0)?;
    let n = ctx.get_number_arg(1)?;
    
    if n < 0.0 || !n.is_finite() {
        return Err(LuaError::BadArgument {
            func: Some("rep".to_string()),
            arg: 2,
            msg: "invalid count".to_string()
        });
    }
    
    let count = n as usize;
    
    // Check separator
    let sep = if nargs >= 3 {
        ctx.get_arg_str(2)?
    } else {
        String::new()
    };
    
    if count == 0 {
        // Return empty string
        let handle = ctx.create_string("")?;
        ctx.push_result(Value::String(handle))?;
        return Ok(1);
    }
    
    // Build result with separator
    let mut result = String::with_capacity(s.len() * count + sep.len() * (count - 1));
    for i in 0..count {
        if i > 0 {
            result.push_str(&sep);
        }
        result.push_str(&s);
    }
    
    let handle = ctx.create_string(&result)?;
    ctx.push_result(Value::String(handle))?;
    
    Ok(1)
}

/// string.reverse function - reverses a string
/// Signature: string.reverse(s)
pub fn string_reverse(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("reverse".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let s = ctx.get_arg_str(0)?;
    let result: String = s.chars().rev().collect();
    
    let handle = ctx.create_string(&result)?;
    ctx.push_result(Value::String(handle))?;
    
    Ok(1)
}

/// string.sub function - returns a substring
/// Signature: string.sub(s, i [, j])
pub fn string_sub(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let nargs = ctx.arg_count();
    if nargs < 2 || nargs > 3 {
        return Err(LuaError::BadArgument {
            func: Some("sub".to_string()),
            arg: 1,
            msg: "2 or 3 arguments expected".to_string()
        });
    }
    
    let s = ctx.get_arg_str(0)?;
    let i = ctx.get_number_arg(1)? as isize;
    let j = if nargs >= 3 {
        ctx.get_number_arg(2)? as isize
    } else {
        -1  // Default to end of string
    };
    
    // Convert to 0-based indices
    let start = normalize_string_index(i, s.len() as isize);
    let end = normalize_string_index(j, s.len() as isize) + 1; // +1 because range is exclusive at end
    
    // Handle invalid ranges
    if start >= s.len() || end <= start {
        let empty = ctx.create_string("")?;
        ctx.push_result(Value::String(empty))?;
        return Ok(1);
    }
    
    // Extract the substring
    let end = end.min(s.len());
    let sub = &s[start..end];
    
    let handle = ctx.create_string(sub)?;
    ctx.push_result(Value::String(handle))?;
    
    Ok(1)
}

/// string.upper function - converts a string to uppercase
/// Signature: string.upper(s)
pub fn string_upper(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("upper".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let s = ctx.get_arg_str(0)?;
    let result = s.to_uppercase();
    
    let handle = ctx.create_string(&result)?;
    ctx.push_result(Value::String(handle))?;
    
    Ok(1)
}

/// Create table with all string functions
pub fn create_string_lib() -> Vec<(&'static str, CFunction)> {
    let mut string_funcs = Vec::new();
    
    // Add all string functions
    string_funcs.push(("byte", string_byte as CFunction));
    string_funcs.push(("char", string_char as CFunction));
    string_funcs.push(("dump", string_dump as CFunction));
    string_funcs.push(("find", string_find as CFunction));
    string_funcs.push(("format", string_format as CFunction));
    string_funcs.push(("gmatch", string_gmatch as CFunction));
    string_funcs.push(("gsub", string_gsub as CFunction));
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
pub fn init_string_lib(vm: &mut crate::lua::refcell_vm::RefCellVM) -> LuaResult<()> {
    // Create string library table
    let string_table = vm.heap().create_table()?;
    
    // Get globals table
    let globals = vm.heap().globals()?;
    
    // Create handle for "string" string
    let string_name = vm.heap().create_string("string")?;
    
    // Add string table to globals
    vm.heap().set_table_field(globals, &Value::String(string_name), &Value::Table(string_table))?;
    
    // Add string functions
    let funcs = create_string_lib();
    for (name, func) in funcs {
        let name_handle = vm.heap().create_string(name)?;
        vm.heap().set_table_field(string_table, &Value::String(name_handle), &Value::CFunction(func))?;
    }
    
    Ok(())
}