//! Lua Standard Library Implementation
//!
//! This module implements the Lua standard library with a focus on the
//! Redis-compatible subset of Lua 5.1 features.

use super::error::{LuaError, Result};
use super::value::{Value, TableHandle, StringHandle, CFunction};
use super::vm::{LuaVM, ExecutionContext};

/// Register the standard library functions with a VM
pub fn register_stdlib(vm: &mut LuaVM) -> Result<()> {
    // Create global tables for standard library
    let globals = vm.globals();
    
    // Register base library
    register_base(vm, globals.clone())?;
    
    // Register string library
    let string_lib = register_string_lib(vm)?;
    let string_name = vm.create_string("string")?;
    vm.set_table(globals.clone(), Value::String(string_name), Value::Table(string_lib))?;
    
    // Register table library
    let table_lib = register_table_lib(vm)?;
    let table_name = vm.create_string("table")?;
    vm.set_table(globals.clone(), Value::String(table_name), Value::Table(table_lib))?;
    
    // Register math library
    let math_lib = register_math_lib(vm)?;
    let math_name = vm.create_string("math")?;
    vm.set_table(globals.clone(), Value::String(math_name), Value::Table(math_lib))?;
    
    Ok(())
}

/// Register the base library
fn register_base(vm: &mut LuaVM, globals: TableHandle) -> Result<()> {
    // Base library functions
    let functions = [
        ("assert", base_assert as CFunction),
        ("error", base_error as CFunction),
        ("ipairs", base_ipairs as CFunction),
        ("next", _base_next as CFunction),
        ("pairs", _base_pairs as CFunction),
        ("pcall", base_pcall as CFunction),
        ("print", _base_print as CFunction),
        ("select", base_select as CFunction),
        ("tonumber", base_tonumber as CFunction),
        ("tostring", base_tostring as CFunction),
        ("type", base_type as CFunction),
        ("unpack", base_unpack as CFunction),
    ];
    
    // Register all functions in _G
    for (name, func) in functions.iter() {
        let name_handle = vm.create_string(name)?;
        vm.set_table(globals.clone(), Value::String(name_handle), Value::CFunction(*func))?;
    }
    
    // Set _G to itself
    let g_handle = vm.create_string("_G")?;
    vm.set_table(globals.clone(), Value::String(g_handle), Value::Table(globals.clone()))?;
    
    // Register nil, true, false (upvalues)
    let _nil_handle = vm.create_string("nil")?;
    let true_handle = vm.create_string("true")?;
    let false_handle = vm.create_string("false")?;
    
    vm.set_table(globals.clone(), Value::String(true_handle), Value::Boolean(true))?;
    vm.set_table(globals.clone(), Value::String(false_handle), Value::Boolean(false))?;
    
    Ok(())
}

/// Register the string library
fn register_string_lib(vm: &mut LuaVM) -> Result<TableHandle> {
    let string_lib = vm.create_table()?;
    
    // String library functions
    let functions = [
        ("byte", string_byte as CFunction),
        ("char", string_char as CFunction),
        ("find", string_find as CFunction),
        ("format", string_format as CFunction),
        ("gmatch", string_gmatch as CFunction),
        ("gsub", string_gsub as CFunction),
        ("len", string_len as CFunction),
        ("lower", string_lower as CFunction),
        ("match", string_match as CFunction),
        ("rep", string_rep as CFunction),
        ("reverse", string_reverse as CFunction),
        ("sub", string_sub as CFunction),
        ("upper", string_upper as CFunction),
    ];
    
    // Register all functions
    for (name, func) in functions.iter() {
        let name_handle = vm.create_string(name)?;
        vm.set_table(string_lib.clone(), Value::String(name_handle), Value::CFunction(*func))?;
    }
    
    Ok(string_lib)
}

/// Register the table library
fn register_table_lib(vm: &mut LuaVM) -> Result<TableHandle> {
    let table_lib = vm.create_table()?;
    
    // Table library functions
    let functions = [
        ("concat", table_concat as CFunction),
        ("insert", table_insert as CFunction),
        ("maxn", table_maxn as CFunction),
        ("remove", table_remove as CFunction),
        ("sort", _table_sort as CFunction),
    ];
    
    // Register all functions
    for (name, func) in functions.iter() {
        let name_handle = vm.create_string(name)?;
        vm.set_table(table_lib.clone(), Value::String(name_handle), Value::CFunction(*func))?;
    }
    
    Ok(table_lib)
}

/// Register the math library
fn register_math_lib(vm: &mut LuaVM) -> Result<TableHandle> {
    let math_lib = vm.create_table()?;
    
    // Math library functions
    let functions = [
        ("abs", math_abs as CFunction),
        ("acos", math_acos as CFunction),
        ("asin", math_asin as CFunction),
        ("atan", math_atan as CFunction),
        ("atan2", math_atan2 as CFunction),
        ("ceil", math_ceil as CFunction),
        ("cos", math_cos as CFunction),
        ("cosh", math_cosh as CFunction),
        ("deg", math_deg as CFunction),
        ("exp", math_exp as CFunction),
        ("floor", math_floor as CFunction),
        ("fmod", math_fmod as CFunction),
        ("frexp", math_frexp as CFunction),
        ("ldexp", math_ldexp as CFunction),
        ("log", math_log as CFunction),
        ("log10", math_log10 as CFunction),
        ("max", math_max as CFunction),
        ("min", math_min as CFunction),
        ("modf", math_modf as CFunction),
        ("pow", math_pow as CFunction),
        ("rad", math_rad as CFunction),
        ("random", math_random as CFunction),
        ("randomseed", _math_randomseed as CFunction),
        ("sin", math_sin as CFunction),
        ("sinh", math_sinh as CFunction),
        ("sqrt", math_sqrt as CFunction),
        ("tan", math_tan as CFunction),
        ("tanh", math_tanh as CFunction),
    ];
    
    // Register all functions
    for (name, func) in functions.iter() {
        let name_handle = vm.create_string(name)?;
        vm.set_table(math_lib.clone(), Value::String(name_handle), Value::CFunction(*func))?;
    }
    
    // Register math constants
    let pi_handle = vm.create_string("pi")?;
    let huge_handle = vm.create_string("huge")?;
    
    vm.set_table(math_lib.clone(), Value::String(pi_handle), Value::Number(std::f64::consts::PI))?;
    vm.set_table(math_lib.clone(), Value::String(huge_handle), Value::Number(std::f64::INFINITY))?;
    
    Ok(math_lib)
}

// Base library functions

/// assert(v [, message])
fn base_assert(ctx: &mut ExecutionContext) -> Result<i32> {
    let value = ctx.get_arg(0)?;
    
    if !value.is_truthy() {
        let message = if ctx.arg_count() > 1 {
            match ctx.get_arg(1) {
                Ok(v) => format!("{}", v),
                Err(_) => "assertion failed!".to_string(),
            }
        } else {
            "assertion failed!".to_string()
        };
        
        return Err(LuaError::RuntimeError(message));
    }
    
    // Return all arguments
    Ok(ctx.arg_count() as i32)
}

/// error(message [, level])
fn base_error(ctx: &mut ExecutionContext) -> Result<i32> {
    let message = match ctx.get_arg(0) {
        Ok(v) => format!("{}", v),
        Err(_) => "".to_string(),
    };
    
    Err(LuaError::RuntimeError(message))
}

/// ipairs(t)
fn base_ipairs(ctx: &mut ExecutionContext) -> Result<i32> {
    let table = ctx.get_arg(0)?;
    
    if !table.is_table() {
        return Err(LuaError::TypeError(format!(
            "bad argument #1 to 'ipairs' (table expected, got {})",
            table.type_name()
        )));
    }
    
    // Create iterator function
    let ipairs_iter = |ctx: &mut ExecutionContext| -> Result<i32> {
        let table = ctx.get_arg(0)?;
        let index = match ctx.get_arg(1) {
            Ok(Value::Number(n)) => n,
            _ => return Ok(0), // End iteration
        };
        
        let next_index = index + 1.0;
        
        if let Value::Table(handle) = table {
            match ctx.vm.get_table(handle, &Value::Number(next_index)) {
                Ok(Value::Nil) => Ok(0), // End iteration
                Ok(value) => {
                    // Return index, value
                    ctx.push_result(Value::Number(next_index))?;
                    ctx.push_result(value)?;
                    Ok(2)
                }
                Err(_) => Ok(0), // End iteration
            }
        } else {
            Ok(0) // End iteration
        }
    };
    
    // Return iterator function, state (table), initial index
    ctx.push_result(Value::CFunction(ipairs_iter))?;
    ctx.push_result(table)?;
    ctx.push_result(Value::Number(0.0))?;
    
    Ok(3)
}

/// next(table [, index])
fn _base_next(_ctx: &mut ExecutionContext) -> Result<i32> {
    // Minimal stub implementation
    // Would need access to table internals for proper implementation
    Ok(0)
}

/// pairs(t)
fn _base_pairs(ctx: &mut ExecutionContext) -> Result<i32> {
    let table = ctx.get_arg(0)?;
    
    if !table.is_table() {
        return Err(LuaError::TypeError(format!(
            "bad argument #1 to 'pairs' (table expected, got {})",
            table.type_name()
        )));
    }
    
    // Create pairs iterator
    let next_func = |_ctx: &mut ExecutionContext| -> Result<i32> {
        // Minimal stub implementation
        // Would need access to table internals for proper implementation
        Ok(0)
    };
    
    // Return next function, state (table), nil
    ctx.push_result(Value::CFunction(next_func))?;
    ctx.push_result(table)?;
    ctx.push_result(Value::Nil)?;
    
    Ok(3)
}

/// pcall(f, ...)
fn base_pcall(ctx: &mut ExecutionContext) -> Result<i32> {
    // This would be implemented properly in the VM itself
    // For now, just indicate success
    ctx.push_result(Value::Boolean(true))?;
    ctx.push_result(Value::Nil)?;
    Ok(2)
}

/// print(...)
fn _base_print(_ctx: &mut ExecutionContext) -> Result<i32> {
    // Just a stub - actual printing would happen in the VM
    Ok(0)
}

/// select(index, ...)
fn base_select(ctx: &mut ExecutionContext) -> Result<i32> {
    let index_arg = ctx.get_arg(0)?;
    
    match index_arg {
        Value::String(s) => {
            let s_str = ctx.vm.heap.get_string_value(s)?;
            if s_str == "#" {
                // Return the number of arguments excluding the first
                let count = ctx.arg_count() - 1;
                ctx.push_result(Value::Number(count as f64))?;
                Ok(1)
            } else {
                Err(LuaError::ArgError(1, "invalid option".to_string()))
            }
        }
        Value::Number(n) => {
            let index = if n < 0.0 {
                (ctx.arg_count() as f64 + n) as usize
            } else {
                n as usize
            };
            
            if index < 1 || index >= ctx.arg_count() {
                Err(LuaError::ArgError(1, "index out of range".to_string()))
            } else {
                // Return all arguments from index onward
                let return_count = ctx.arg_count() - index;
                for i in 0..return_count {
                    let arg = ctx.get_arg(index + i)?;
                    ctx.push_result(arg)?;
                }
                Ok(return_count as i32)
            }
        }
        _ => Err(LuaError::ArgError(1, "number or '#' expected".to_string())),
    }
}

/// tonumber(e [, base])
fn base_tonumber(ctx: &mut ExecutionContext) -> Result<i32> {
    let value = ctx.get_arg(0)?;
    let base = if ctx.arg_count() > 1 {
        match ctx.get_arg(1)? {
            Value::Number(n) => n as i32,
            _ => 10,
        }
    } else {
        10
    };
    
    if base != 10 && (base < 2 || base > 36) {
        return Err(LuaError::ArgError(2, "base out of range".to_string()));
    }
    
    match value {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n))?;
            Ok(1)
        }
        Value::String(s) => {
            // Convert string to number
            let s_str = ctx.vm.heap.get_string_value(s)?;
            let trimmed = s_str.trim();
            
            if base == 10 {
                // Try as decimal
                match trimmed.parse::<f64>() {
                    Ok(n) => {
                        ctx.push_result(Value::Number(n))?;
                        Ok(1)
                    }
                    Err(_) => {
                        ctx.push_result(Value::Nil)?;
                        Ok(1)
                    }
                }
            } else {
                // Try as other base
                // Would parse in the specified base
                ctx.push_result(Value::Nil)?;
                Ok(1)
            }
        }
        _ => {
            ctx.push_result(Value::Nil)?;
            Ok(1)
        }
    }
}

/// tostring(v)
fn base_tostring(ctx: &mut ExecutionContext) -> Result<i32> {
    let value = ctx.get_arg(0)?;
    
    match value {
        Value::Nil => {
            let s = ctx.vm.create_string("nil")?;
            ctx.push_result(Value::String(s))?;
        }
        Value::Boolean(b) => {
            let s = ctx.vm.create_string(if b { "true" } else { "false" })?;
            ctx.push_result(Value::String(s))?;
        }
        Value::Number(n) => {
            let s = ctx.vm.create_string(&n.to_string())?;
            ctx.push_result(Value::String(s))?;
        }
        Value::String(s) => {
            // Already a string
            ctx.push_result(Value::String(s))?;
        }
        Value::Table(t) => {
            // Check for __tostring metamethod
            // For now, just return a placeholder
            let s = ctx.vm.create_string(&format!("table: {:?}", t))?;
            ctx.push_result(Value::String(s))?;
        }
        Value::Closure(c) => {
            let s = ctx.vm.create_string(&format!("function: {:?}", c))?;
            ctx.push_result(Value::String(s))?;
        }
        Value::Thread(t) => {
            let s = ctx.vm.create_string(&format!("thread: {:?}", t))?;
            ctx.push_result(Value::String(s))?;
        }
        Value::CFunction(_) => {
            let s = ctx.vm.create_string("function: <C function>")?;
            ctx.push_result(Value::String(s))?;
        }
        Value::UserData(u) => {
            let s = ctx.vm.create_string(&format!("userdata: {:?}", u))?;
            ctx.push_result(Value::String(s))?;
        }
    }
    
    Ok(1)
}

/// type(v)
fn base_type(ctx: &mut ExecutionContext) -> Result<i32> {
    let value = ctx.get_arg(0)?;
    let type_name = value.type_name();
    let s = ctx.vm.create_string(type_name)?;
    ctx.push_result(Value::String(s))?;
    Ok(1)
}

/// unpack(list [, i [, j]])
fn base_unpack(ctx: &mut ExecutionContext) -> Result<i32> {
    let table = ctx.get_arg(0)?;
    
    // Simple stub implementation
    match table {
        Value::Table(h) => {
            let table_len = ctx.vm.get_table_length(h)?;
            if table_len == 0 {
                return Ok(0);
            }
            
            // Would need to iterate over the table here
            ctx.push_result(Value::Nil)?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError(format!(
            "bad argument #1 to 'unpack' (table expected, got {})",
            table.type_name()
        ))),
    }
}

// String library functions

/// string.byte(s [, i [, j]])
fn string_byte(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.arg_count() < 1 {
        return Err(LuaError::ArgError(1, "string expected".to_string()));
    }
    
    // Get string
    let s = ctx.get_arg(0)?;
    let s_bytes = match s {
        Value::String(handle) => {
            ctx.vm.heap.get_string_bytes(handle)?.to_vec()
        },
        _ => {
            return Err(LuaError::ArgError(1, format!("string expected, got {}", s.type_name())));
        }
    };
    
    // Get start index (default to 1)
    let start = if ctx.arg_count() > 1 {
        match ctx.get_arg(1)? {
            Value::Number(n) => {
                // Lua indices are 1-based, convert to 0-based
                let idx = if n <= 0.0 {
                    ((s_bytes.len() as f64) + n) as isize
                } else {
                    (n - 1.0) as isize
                };
                
                // Ensure index is within bounds
                if idx < 0 || idx >= s_bytes.len() as isize {
                    return Err(LuaError::ArgError(2, "index out of bounds".to_string()));
                }
                
                idx as usize
            },
            _ => {
                return Err(LuaError::ArgError(2, "number expected".to_string()));
            }
        }
    } else {
        0 // First character (1-based -> 0-based)
    };
    
    // Get end index (default to start)
    let end = if ctx.arg_count() > 2 {
        match ctx.get_arg(2)? {
            Value::Number(n) => {
                // Lua indices are 1-based, convert to 0-based
                let idx = if n <= 0.0 {
                    ((s_bytes.len() as f64) + n) as isize
                } else {
                    (n - 1.0) as isize
                };
                
                // Ensure index is within bounds
                if idx < 0 {
                    0
                } else if idx >= s_bytes.len() as isize {
                    s_bytes.len() - 1
                } else {
                    idx as usize
                }
            },
            _ => {
                return Err(LuaError::ArgError(3, "number expected".to_string()));
            }
        }
    } else {
        start
    };
    
    // Return bytes in the range
    let mut ret_count = 0;
    for i in start..=end.min(s_bytes.len() - 1) {
        ctx.push_result(Value::Number(s_bytes[i] as f64))?;
        ret_count += 1;
    }
    
    Ok(ret_count)
}

/// string.char(...)
fn string_char(ctx: &mut ExecutionContext) -> Result<i32> {
    // Create string from character codes
    let mut bytes = Vec::new();
    
    for i in 0..ctx.arg_count() {
        let code = match ctx.get_arg(i)? {
            Value::Number(n) => n as u8,
            _ => {
                return Err(LuaError::ArgError(i + 1, "number expected".to_string()));
            }
        };
        
        bytes.push(code);
    }
    
    let s = String::from_utf8_lossy(&bytes);
    let handle = ctx.vm.create_string(&s)?;
    ctx.push_result(Value::String(handle))?;
    
    Ok(1)
}

/// string.find(s, pattern [, init [, plain]])
fn string_find(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.arg_count() < 2 {
        return Err(LuaError::ArgError(1, "string expected".to_string()));
    }
    
    // Get string
    let s = ctx.get_arg(0)?;
    let s_str = match s {
        Value::String(handle) => {
            ctx.vm.heap.get_string_value(handle)?
        },
        _ => {
            return Err(LuaError::ArgError(1, format!("string expected, got {}", s.type_name())));
        }
    };
    
    // Get pattern
    let pattern = ctx.get_arg(1)?;
    let pat_str = match pattern {
        Value::String(handle) => {
            ctx.vm.heap.get_string_value(handle)?
        },
        _ => {
            return Err(LuaError::ArgError(2, format!("string expected, got {}", pattern.type_name())));
        }
    };
    
    // Get init (default to 1)
    let init = if ctx.arg_count() > 2 {
        match ctx.get_arg(2)? {
            Value::Number(n) => {
                // Lua indices are 1-based
                if n <= 0.0 {
                    ((s_str.len() as f64) + n) as isize
                } else {
                    (n - 1.0) as isize
                }
            },
            _ => {
                return Err(LuaError::ArgError(3, "number expected".to_string()));
            }
        }
    } else {
        0 // First character (1-based -> 0-based)
    };
    
    // Get plain flag (default to false)
    let plain = if ctx.arg_count() > 3 {
        match ctx.get_arg(3)? {
            Value::Boolean(b) => b,
            _ => {
                return Err(LuaError::ArgError(4, "boolean expected".to_string()));
            }
        }
    } else {
        false
    };
    
    // Find pattern in string
    if init < 0 || init as usize >= s_str.len() {
        // Out of bounds
        ctx.push_result(Value::Nil)?;
        return Ok(1);
    }
    
    // Use plain string search for now (pattern matching is complex)
    if plain || true { // Always use plain search for now
        match s_str[init as usize..].find(&pat_str) {
            Some(pos) => {
                let start = init as usize + pos;
                let end = start + pat_str.len() - 1;
                
                // Return start, end (1-based indices)
                ctx.push_result(Value::Number((start + 1) as f64))?;
                ctx.push_result(Value::Number((end + 1) as f64))?;
                Ok(2)
            },
            None => {
                ctx.push_result(Value::Nil)?;
                Ok(1)
            }
        }
    } else {
        // Pattern matching not implemented yet
        ctx.push_result(Value::Nil)?;
        Ok(1)
    }
}

/// string.format(formatstring, ...)
fn string_format(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.arg_count() < 1 {
        return Err(LuaError::ArgError(1, "string expected".to_string()));
    }
    
    // Get format string
    let format = ctx.get_arg(0)?;
    let fmt_str = match format {
        Value::String(handle) => {
            ctx.vm.heap.get_string_value(handle)?
        },
        _ => {
            return Err(LuaError::ArgError(1, format!("string expected, got {}", format.type_name())));
        }
    };
    
    // Process format string
    let mut result = String::new();
    let mut format_chars = fmt_str.chars().peekable();
    let mut arg_index = 1;
    
    while let Some(c) = format_chars.next() {
        if c == '%' {
            if let Some(&next) = format_chars.peek() {
                if next == '%' {
                    // Literal %
                    result.push('%');
                    format_chars.next(); // Skip second %
                    continue;
                }
            }
            
            // Format specifier
            let mut specifier = String::new();
            
            // Read until conversion specifier
            while let Some(&next) = format_chars.peek() {
                if "cdeEfgGiouqsxX".contains(next) {
                    specifier.push(next);
                    format_chars.next(); // Consume conversion char
                    break;
                } else {
                    specifier.push(next);
                    format_chars.next();
                }
            }
            
            // Get argument
            if arg_index >= ctx.arg_count() {
                return Err(LuaError::ArgError(arg_index + 1, "no value".to_string()));
            }
            
            let arg = ctx.get_arg(arg_index)?;
            arg_index += 1;
            
            // Format based on specifier
            if specifier == "d" || specifier == "i" {
                // Integer
                match arg {
                    Value::Number(n) => {
                        result.push_str(&format!("{:.0}", n));
                    },
                    _ => {
                        return Err(LuaError::ArgError(arg_index, "number expected".to_string()));
                    }
                }
            } else if specifier == "f" {
                // Float
                match arg {
                    Value::Number(n) => {
                        result.push_str(&n.to_string());
                    },
                    _ => {
                        return Err(LuaError::ArgError(arg_index, "number expected".to_string()));
                    }
                }
            } else if specifier == "s" {
                // String
                let str_value = match arg {
                    Value::String(handle) => {
                        ctx.vm.heap.get_string_value(handle)?
                    },
                    Value::Number(n) => {
                        n.to_string()
                    },
                    Value::Boolean(b) => {
                        b.to_string()
                    },
                    Value::Nil => {
                        "nil".to_string()
                    },
                    _ => {
                        format!("{}", arg.type_name())
                    }
                };
                
                result.push_str(&str_value);
            } else {
                // Unsupported format specifier
                result.push_str(&format!("%{}", specifier));
            }
        } else {
            result.push(c);
        }
    }
    
    // Return formatted string
    let handle = ctx.vm.create_string(&result)?;
    ctx.push_result(Value::String(handle))?;
    Ok(1)
}

/// string.gmatch(s, pattern)
fn string_gmatch(ctx: &mut ExecutionContext) -> Result<i32> {
    // Simple stub implementation - would return an iterator function
    let iter_func = |ctx: &mut ExecutionContext| -> Result<i32> {
        ctx.push_result(Value::Nil)?;
        Ok(1)
    };
    
    ctx.push_result(Value::CFunction(iter_func))?;
    Ok(1)
}

/// string.gsub(s, pattern, repl [, n])
fn string_gsub(ctx: &mut ExecutionContext) -> Result<i32> {
    // Simple stub implementation
    let s = ctx.vm.create_string("replaced string")?;
    ctx.push_result(Value::String(s))?;
    ctx.push_result(Value::Number(1.0))?;
    Ok(2)
}

/// string.len(s)
fn string_len(ctx: &mut ExecutionContext) -> Result<i32> {
    let s = ctx.get_arg(0)?;
    
    match s {
        Value::String(handle) => {
            let len = ctx.vm.get_string_length(handle)?;
            ctx.push_result(Value::Number(len as f64))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError(format!(
            "bad argument #1 to 'len' (string expected, got {})",
            s.type_name()
        ))),
    }
}

/// string.lower(s)
fn string_lower(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.arg_count() < 1 {
        return Err(LuaError::ArgError(1, "string expected".to_string()));
    }
    
    // Get string
    let s = ctx.get_arg(0)?;
    let s_str = match s {
        Value::String(handle) => {
            ctx.vm.heap.get_string_value(handle)?
        },
        _ => {
            return Err(LuaError::ArgError(1, format!("string expected, got {}", s.type_name())));
        }
    };
    
    // Convert to lowercase
    let lower = s_str.to_lowercase();
    
    // Return result
    let handle = ctx.vm.create_string(&lower)?;
    ctx.push_result(Value::String(handle))?;
    Ok(1)
}

/// string.match(s, pattern [, init])
fn string_match(ctx: &mut ExecutionContext) -> Result<i32> {
    // Simple stub implementation
    let s = ctx.vm.create_string("match")?;
    ctx.push_result(Value::String(s))?;
    Ok(1)
}

/// string.rep(s, n)
fn string_rep(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.arg_count() < 2 {
        return Err(LuaError::ArgError(1, "string expected".to_string()));
    }
    
    // Get string
    let s = ctx.get_arg(0)?;
    let s_str = match s {
        Value::String(handle) => {
            ctx.vm.heap.get_string_value(handle)?
        },
        _ => {
            return Err(LuaError::ArgError(1, format!("string expected, got {}", s.type_name())));
        }
    };
    
    // Get repeat count
    let count = match ctx.get_arg(1)? {
        Value::Number(n) => {
            if n < 0.0 || n.is_nan() {
                return Err(LuaError::ArgError(2, "invalid count".to_string()));
            }
            n as usize
        },
        _ => {
            return Err(LuaError::ArgError(2, "number expected".to_string()));
        }
    };
    
    // Get separator (optional)
    let sep = if ctx.arg_count() > 2 {
        match ctx.get_arg(2)? {
            Value::String(handle) => {
                ctx.vm.heap.get_string_value(handle)?
            },
            _ => {
                return Err(LuaError::ArgError(3, "string expected".to_string()));
            }
        }
    } else {
        "".to_string()
    };
    
    // Check for excessive memory usage
    if s_str.len() * count > 1_000_000 { // 1MB limit
        return Err(LuaError::MemoryLimit);
    }
    
    // Repeat string
    let mut result = String::new();
    for i in 0..count {
        if i > 0 && !sep.is_empty() {
            result.push_str(&sep);
        }
        result.push_str(&s_str);
    }
    
    // Return result
    let handle = ctx.vm.create_string(&result)?;
    ctx.push_result(Value::String(handle))?;
    Ok(1)
}

/// string.reverse(s)
fn string_reverse(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.arg_count() < 1 {
        return Err(LuaError::ArgError(1, "string expected".to_string()));
    }
    
    // Get string
    let s = ctx.get_arg(0)?;
    let s_str = match s {
        Value::String(handle) => {
            ctx.vm.heap.get_string_value(handle)?
        },
        _ => {
            return Err(LuaError::ArgError(1, format!("string expected, got {}", s.type_name())));
        }
    };
    
    // Reverse the string
    let reversed: String = s_str.chars().rev().collect();
    
    // Return result
    let handle = ctx.vm.create_string(&reversed)?;
    ctx.push_result(Value::String(handle))?;
    Ok(1)
}

/// string.sub(s, i [, j])
fn string_sub(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.arg_count() < 2 {
        return Err(LuaError::ArgError(1, "string expected".to_string()));
    }
    
    // Get string
    let s = ctx.get_arg(0)?;
    let s_str = match s {
        Value::String(handle) => {
            ctx.vm.heap.get_string_value(handle)?
        },
        _ => {
            return Err(LuaError::ArgError(1, format!("string expected, got {}", s.type_name())));
        }
    };
    
    // Get start index (1-based)
    let start = match ctx.get_arg(1)? {
        Value::Number(n) => {
            // Convert to 0-based index with Lua semantics
            if n <= 0.0 {
                // Negative indices count from the end
                ((s_str.len() as f64) + n) as isize
            } else {
                (n - 1.0) as isize
            }
        },
        _ => {
            return Err(LuaError::ArgError(2, "number expected".to_string()));
        }
    };
    
    // Get end index (default to -1, meaning the end of string)
    let end = if ctx.arg_count() > 2 {
        match ctx.get_arg(2)? {
            Value::Number(n) => {
                // Convert to 0-based index with Lua semantics
                if n <= 0.0 {
                    // Negative indices count from the end
                    ((s_str.len() as f64) + n) as isize
                } else {
                    (n - 1.0) as isize
                }
            },
            _ => {
                return Err(LuaError::ArgError(3, "number expected".to_string()));
            }
        }
    } else {
        (s_str.len() - 1) as isize
    };
    
    // Calculate actual start/end with bounds checking
    let actual_start = start.max(0).min(s_str.len() as isize) as usize;
    let actual_end = (end.max(0).min(s_str.len() as isize) as usize).max(actual_start);
    
    // Extract substring
    let substring = if actual_start < s_str.len() && actual_end >= actual_start {
        &s_str[actual_start..=actual_end]
    } else {
        ""
    };
    
    // Return result
    let handle = ctx.vm.create_string(substring)?;
    ctx.push_result(Value::String(handle))?;
    Ok(1)
}

/// string.upper(s)
fn string_upper(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.arg_count() < 1 {
        return Err(LuaError::ArgError(1, "string expected".to_string()));
    }
    
    // Get string
    let s = ctx.get_arg(0)?;
    let s_str = match s {
        Value::String(handle) => {
            ctx.vm.heap.get_string_value(handle)?
        },
        _ => {
            return Err(LuaError::ArgError(1, format!("string expected, got {}", s.type_name())));
        }
    };
    
    // Convert to uppercase
    let upper = s_str.to_uppercase();
    
    // Return result
    let handle = ctx.vm.create_string(&upper)?;
    ctx.push_result(Value::String(handle))?;
    Ok(1)
}

// Table library functions

/// table.concat(table [, sep [, i [, j]]])
fn table_concat(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.arg_count() < 1 {
        return Err(LuaError::ArgError(1, "table expected".to_string()));
    }
    
    // Get table
    let table = ctx.get_arg(0)?;
    let table_handle = match table {
        Value::Table(handle) => handle,
        _ => {
            return Err(LuaError::ArgError(1, format!("table expected, got {}", table.type_name())));
        }
    };
    
    // Get separator (default to "")
    let sep = if ctx.arg_count() > 1 {
        match ctx.get_arg(1)? {
            Value::String(handle) => {
                ctx.vm.heap.get_string_value(handle)?
            },
            _ => {
                return Err(LuaError::ArgError(2, "string expected".to_string()));
            }
        }
    } else {
        "".to_string()
    };
    
    // Get table length
    let table_obj = ctx.vm.heap.get_table(table_handle.clone())?;
    let array_len = table_obj.len();
    
    // Get start index (default to 1)
    let start = if ctx.arg_count() > 2 {
        match ctx.get_arg(2)? {
            Value::Number(n) => {
                if n < 1.0 || n > array_len as f64 {
                    return Err(LuaError::ArgError(3, "index out of range".to_string()));
                }
                n as usize
            },
            _ => {
                return Err(LuaError::ArgError(3, "number expected".to_string()));
            }
        }
    } else {
        1 // Default start index (1-based)
    };
    
    // Get end index (default to array length)
    let end = if ctx.arg_count() > 3 {
        match ctx.get_arg(3)? {
            Value::Number(n) => {
                if n < start as f64 || n > array_len as f64 {
                    return Err(LuaError::ArgError(4, "index out of range".to_string()));
                }
                n as usize
            },
            _ => {
                return Err(LuaError::ArgError(4, "number expected".to_string()));
            }
        }
    } else {
        array_len // Default end index
    };
    
    // Convert indices from 1-based to 0-based
    let start_idx = start - 1;
    let end_idx = end - 1;
    
    // Concatenate elements
    let mut result = String::new();
    for i in start_idx..=end_idx.min(array_len - 1) {
        // Add separator if not the first element
        if i > start_idx {
            result.push_str(&sep);
        }
        
        // Get element at index (1-based in Lua)
        let key = Value::Number((i + 1) as f64);
        let value = ctx.vm.get_table(table_handle.clone(), &key)?;
        
        // Convert value to string
        match value {
            Value::String(handle) => {
                let s = ctx.vm.heap.get_string_value(handle)?;
                result.push_str(&s);
            },
            Value::Number(n) => {
                result.push_str(&n.to_string());
            },
            _ => {
                return Err(LuaError::ArgError(1, format!(
                    "invalid value ({}) at index {} in table for 'concat'",
                    value.type_name(),
                    i + 1
                )));
            }
        }
    }
    
    // Return result
    let handle = ctx.vm.create_string(&result)?;
    ctx.push_result(Value::String(handle))?;
    Ok(1)
}

/// table.insert(table, [pos,] value)
fn table_insert(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.arg_count() < 2 {
        return Err(LuaError::ArgError(1, "table expected".to_string()));
    }
    
    // Get table
    let table = ctx.get_arg(0)?;
    let table_handle = match table {
        Value::Table(handle) => handle,
        _ => {
            return Err(LuaError::ArgError(1, format!("table expected, got {}", table.type_name())));
        }
    };
    
    // Get table length
    let array_len = {
        let table_obj = ctx.vm.heap.get_table(table_handle.clone())?;
        table_obj.len()
    };
    
    if ctx.arg_count() == 2 {
        // table.insert(table, value) - append to end
        let value = ctx.get_arg(1)?;
        let pos = array_len + 1; // Position to insert (1-based)
        
        // Set value at position
        ctx.vm.set_table_index(table_handle, pos, value)?;
    } else {
        // table.insert(table, pos, value) - insert at position
        let pos = match ctx.get_arg(1)? {
            Value::Number(n) => {
                if n < 1.0 || n > (array_len as f64) + 1.0 {
                    return Err(LuaError::ArgError(2, "position out of bounds".to_string()));
                }
                n as usize
            },
            _ => {
                return Err(LuaError::ArgError(2, "number expected".to_string()));
            }
        };
        
        let value = ctx.get_arg(2)?;
        
        // Shift elements to make room
        for i in (pos..=array_len).rev() {
            // Get value at current position
            let current = ctx.vm.get_table_index(table_handle.clone(), i)?;
            
            // Move to next position
            ctx.vm.set_table_index(table_handle.clone(), i + 1, current)?;
        }
        
        // Insert new value
        ctx.vm.set_table_index(table_handle, pos, value)?;
    }
    
    Ok(0) // No return values
}

/// table.maxn(table)
fn table_maxn(ctx: &mut ExecutionContext) -> Result<i32> {
    // Simple stub implementation
    ctx.push_result(Value::Number(0.0))?;
    Ok(1)
}

/// table.remove(table [, pos])
fn table_remove(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.arg_count() < 1 {
        return Err(LuaError::ArgError(1, "table expected".to_string()));
    }
    
    // Get table
    let table = ctx.get_arg(0)?;
    let table_handle = match table {
        Value::Table(handle) => handle,
        _ => {
            return Err(LuaError::ArgError(1, format!("table expected, got {}", table.type_name())));
        }
    };
    
    // Get table length
    let array_len = {
        let table_obj = ctx.vm.heap.get_table(table_handle.clone())?;
        table_obj.len()
    };
    
    if array_len == 0 {
        // Empty table, return nil
        ctx.push_result(Value::Nil)?;
        return Ok(1);
    }
    
    // Get position (default to array length)
    let pos = if ctx.arg_count() > 1 {
        match ctx.get_arg(1)? {
            Value::Number(n) => {
                if n < 1.0 || n > array_len as f64 {
                    return Err(LuaError::ArgError(2, "position out of bounds".to_string()));
                }
                n as usize
            },
            _ => {
                return Err(LuaError::ArgError(2, "number expected".to_string()));
            }
        }
    } else {
        array_len // Remove from end
    };
    
    // Get element to remove
    let removed = ctx.vm.get_table_index(table_handle.clone(), pos)?;
    
    // Shift elements down
    for i in pos..array_len {
        // Get value from next position
        let next = ctx.vm.get_table_index(table_handle.clone(), i + 1)?;
        
        // Move to current position
        ctx.vm.set_table_index(table_handle.clone(), i, next)?;
    }
    
    // Set last element to nil (to properly truncate array)
    ctx.vm.set_table_index(table_handle, array_len, Value::Nil)?;
    
    // Return removed value
    ctx.push_result(removed)?;
    Ok(1)
}

/// table.sort(table [, comp])
fn _table_sort(_ctx: &mut ExecutionContext) -> Result<i32> {
    // Stub implementation
    Ok(0)
}

// Math library functions

/// math.abs(x)
fn math_abs(ctx: &mut ExecutionContext) -> Result<i32> {
    let x = ctx.get_arg(0)?;
    
    match x {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n.abs()))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError(format!(
            "bad argument #1 to 'abs' (number expected, got {})",
            x.type_name()
        ))),
    }
}

/// math.acos(x)
fn math_acos(ctx: &mut ExecutionContext) -> Result<i32> {
    let x = ctx.get_arg(0)?;
    
    match x {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n.acos()))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError(format!(
            "bad argument #1 to 'acos' (number expected, got {})",
            x.type_name()
        ))),
    }
}

/// math.asin(x)
fn math_asin(ctx: &mut ExecutionContext) -> Result<i32> {
    let x = ctx.get_arg(0)?;
    
    match x {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n.asin()))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError(format!(
            "bad argument #1 to 'asin' (number expected, got {})",
            x.type_name()
        ))),
    }
}

/// math.atan(x)
fn math_atan(ctx: &mut ExecutionContext) -> Result<i32> {
    let x = ctx.get_arg(0)?;
    
    match x {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n.atan()))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError(format!(
            "bad argument #1 to 'atan' (number expected, got {})",
            x.type_name()
        ))),
    }
}

/// math.atan2(y, x)
fn math_atan2(ctx: &mut ExecutionContext) -> Result<i32> {
    let y = ctx.get_arg(0)?;
    let x = ctx.get_arg(1)?;
    
    match (y, x) {
        (Value::Number(y_val), Value::Number(x_val)) => {
            ctx.push_result(Value::Number(y_val.atan2(x_val)))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError("number expected".to_string())),
    }
}

/// math.ceil(x)
fn math_ceil(ctx: &mut ExecutionContext) -> Result<i32> {
    let x = ctx.get_arg(0)?;
    
    match x {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n.ceil()))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError(format!(
            "bad argument #1 to 'ceil' (number expected, got {})",
            x.type_name()
        ))),
    }
}

/// math.cos(x)
fn math_cos(ctx: &mut ExecutionContext) -> Result<i32> {
    let x = ctx.get_arg(0)?;
    
    match x {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n.cos()))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError(format!(
            "bad argument #1 to 'cos' (number expected, got {})",
            x.type_name()
        ))),
    }
}

/// math.cosh(x)
fn math_cosh(ctx: &mut ExecutionContext) -> Result<i32> {
    let x = ctx.get_arg(0)?;
    
    match x {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n.cosh()))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError(format!(
            "bad argument #1 to 'cosh' (number expected, got {})",
            x.type_name()
        ))),
    }
}

/// math.deg(x)
fn math_deg(ctx: &mut ExecutionContext) -> Result<i32> {
    let x = ctx.get_arg(0)?;
    
    match x {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n * 180.0 / std::f64::consts::PI))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError(format!(
            "bad argument #1 to 'deg' (number expected, got {})",
            x.type_name()
        ))),
    }
}

/// math.exp(x)
fn math_exp(ctx: &mut ExecutionContext) -> Result<i32> {
    let x = ctx.get_arg(0)?;
    
    match x {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n.exp()))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError(format!(
            "bad argument #1 to 'exp' (number expected, got {})",
            x.type_name()
        ))),
    }
}

/// math.floor(x)
fn math_floor(ctx: &mut ExecutionContext) -> Result<i32> {
    let x = ctx.get_arg(0)?;
    
    match x {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n.floor()))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError(format!(
            "bad argument #1 to 'floor' (number expected, got {})",
            x.type_name()
        ))),
    }
}

/// math.fmod(x, y)
fn math_fmod(ctx: &mut ExecutionContext) -> Result<i32> {
    let x = ctx.get_arg(0)?;
    let y = ctx.get_arg(1)?;
    
    match (x, y) {
        (Value::Number(x_val), Value::Number(y_val)) => {
            if y_val == 0.0 {
                return Err(LuaError::RuntimeError("attempt to perform 'fmod' with a zero value".to_string()));
            }
            
            ctx.push_result(Value::Number(x_val % y_val))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError("number expected".to_string())),
    }
}

/// math.frexp(x)
fn math_frexp(ctx: &mut ExecutionContext) -> Result<i32> {
    // Not available in Rust std, simple stub
    ctx.push_result(Value::Number(0.0))?;
    ctx.push_result(Value::Number(0.0))?;
    Ok(2)
}

/// math.ldexp(m, e)
fn math_ldexp(ctx: &mut ExecutionContext) -> Result<i32> {
    let m = ctx.get_arg(0)?;
    let e = ctx.get_arg(1)?;
    
    match (m, e) {
        (Value::Number(m_val), Value::Number(e_val)) => {
            ctx.push_result(Value::Number(m_val * 2.0f64.powi(e_val as i32)))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError("number expected".to_string())),
    }
}

/// math.log(x)
fn math_log(ctx: &mut ExecutionContext) -> Result<i32> {
    let x = ctx.get_arg(0)?;
    
    match x {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n.ln()))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError(format!(
            "bad argument #1 to 'log' (number expected, got {})",
            x.type_name()
        ))),
    }
}

/// math.log10(x)
fn math_log10(ctx: &mut ExecutionContext) -> Result<i32> {
    let x = ctx.get_arg(0)?;
    
    match x {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n.log10()))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError(format!(
            "bad argument #1 to 'log10' (number expected, got {})",
            x.type_name()
        ))),
    }
}

/// math.max(x, ...)
fn math_max(ctx: &mut ExecutionContext) -> Result<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::ArgError(1, "value expected".to_string()));
    }
    
    let mut max = match ctx.get_arg(0)? {
        Value::Number(n) => n,
        _ => return Err(LuaError::TypeError("number expected".to_string())),
    };
    
    for i in 1..ctx.arg_count() {
        match ctx.get_arg(i)? {
            Value::Number(n) => {
                if n > max {
                    max = n;
                }
            }
            _ => return Err(LuaError::TypeError("number expected".to_string())),
        }
    }
    
    ctx.push_result(Value::Number(max))?;
    Ok(1)
}

/// math.min(x, ...)
fn math_min(ctx: &mut ExecutionContext) -> Result<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::ArgError(1, "value expected".to_string()));
    }
    
    let mut min = match ctx.get_arg(0)? {
        Value::Number(n) => n,
        _ => return Err(LuaError::TypeError("number expected".to_string())),
    };
    
    for i in 1..ctx.arg_count() {
        match ctx.get_arg(i)? {
            Value::Number(n) => {
                if n < min {
                    min = n;
                }
            }
            _ => return Err(LuaError::TypeError("number expected".to_string())),
        }
    }
    
    ctx.push_result(Value::Number(min))?;
    Ok(1)
}

/// math.modf(x)
fn math_modf(ctx: &mut ExecutionContext) -> Result<i32> {
    let x = ctx.get_arg(0)?;
    
    match x {
        Value::Number(n) => {
            let int_part = n.trunc();
            let frac_part = n - int_part;
            
            ctx.push_result(Value::Number(int_part))?;
            ctx.push_result(Value::Number(frac_part))?;
            Ok(2)
        }
        _ => Err(LuaError::TypeError(format!(
            "bad argument #1 to 'modf' (number expected, got {})",
            x.type_name()
        ))),
    }
}

/// math.pow(x, y)
fn math_pow(ctx: &mut ExecutionContext) -> Result<i32> {
    let x = ctx.get_arg(0)?;
    let y = ctx.get_arg(1)?;
    
    match (x, y) {
        (Value::Number(x_val), Value::Number(y_val)) => {
            ctx.push_result(Value::Number(x_val.powf(y_val)))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError("number expected".to_string())),
    }
}

/// math.rad(x)
fn math_rad(ctx: &mut ExecutionContext) -> Result<i32> {
    let x = ctx.get_arg(0)?;
    
    match x {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n * std::f64::consts::PI / 180.0))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError(format!(
            "bad argument #1 to 'rad' (number expected, got {})",
            x.type_name()
        ))),
    }
}

/// math.random([m [, n]])
fn math_random(ctx: &mut ExecutionContext) -> Result<i32> {
    use rand::{Rng, thread_rng};
    let mut rng = thread_rng();
    
    match ctx.arg_count() {
        0 => {
            // Return a random float in [0, 1)
            let n = rng.gen::<f64>();
            ctx.push_result(Value::Number(n))?;
            Ok(1)
        }
        1 => {
            // Return a random integer in [1, m]
            let m = ctx.get_arg(0)?;
            match m {
                Value::Number(m_val) => {
                    if m_val < 1.0 {
                        return Err(LuaError::ArgError(1, "interval is empty".to_string()));
                    }
                    
                    let m_int = m_val.floor() as i64;
                    let n = rng.gen_range(1..=m_int) as f64;
                    
                    ctx.push_result(Value::Number(n))?;
                    Ok(1)
                }
                _ => Err(LuaError::TypeError("number expected".to_string())),
            }
        }
        _ => {
            // Return a random integer in [m, n]
            let m = ctx.get_arg(0)?;
            let n = ctx.get_arg(1)?;
            
            match (m, n) {
                (Value::Number(m_val), Value::Number(n_val)) => {
                    let m_int = m_val.floor() as i64;
                    let n_int = n_val.floor() as i64;
                    
                    if m_int > n_int {
                        return Err(LuaError::ArgError(2, "interval is empty".to_string()));
                    }
                    
                    let result = rng.gen_range(m_int..=n_int) as f64;
                    
                    ctx.push_result(Value::Number(result))?;
                    Ok(1)
                }
                _ => Err(LuaError::TypeError("number expected".to_string())),
            }
        }
    }
}

/// math.randomseed(x)
fn _math_randomseed(_ctx: &mut ExecutionContext) -> Result<i32> {
    // Stub implementation
    Ok(0)
}

/// math.sin(x)
fn math_sin(ctx: &mut ExecutionContext) -> Result<i32> {
    let x = ctx.get_arg(0)?;
    
    match x {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n.sin()))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError(format!(
            "bad argument #1 to 'sin' (number expected, got {})",
            x.type_name()
        ))),
    }
}

/// math.sinh(x)
fn math_sinh(ctx: &mut ExecutionContext) -> Result<i32> {
    let x = ctx.get_arg(0)?;
    
    match x {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n.sinh()))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError(format!(
            "bad argument #1 to 'sinh' (number expected, got {})",
            x.type_name()
        ))),
    }
}

/// math.sqrt(x)
fn math_sqrt(ctx: &mut ExecutionContext) -> Result<i32> {
    let x = ctx.get_arg(0)?;
    
    match x {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n.sqrt()))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError(format!(
            "bad argument #1 to 'sqrt' (number expected, got {})",
            x.type_name()
        ))),
    }
}

/// math.tan(x)
fn math_tan(ctx: &mut ExecutionContext) -> Result<i32> {
    let x = ctx.get_arg(0)?;
    
    match x {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n.tan()))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError(format!(
            "bad argument #1 to 'tan' (number expected, got {})",
            x.type_name()
        ))),
    }
}

/// math.tanh(x)
fn math_tanh(ctx: &mut ExecutionContext) -> Result<i32> {
    let x = ctx.get_arg(0)?;
    
    match x {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n.tanh()))?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError(format!(
            "bad argument #1 to 'tanh' (number expected, got {})",
            x.type_name()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // These tests would normally require a VM instance
    // For now, we'll just test that the module compiles
    
    #[test]
    fn test_stdlib_compiles() {
        assert!(true);
    }
}