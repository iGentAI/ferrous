//! Standard Library for Rc<RefCell> Lua VM
//!
//! This module provides the standard library functions for the Rc<RefCell>-based
//! Lua VM implementation.

use std::rc::Rc;
use std::cell::RefCell;
use std::convert::TryFrom;

use super::error::{LuaError, LuaResult};
use super::rc_value::{
    Value, Table, StringHandle, TableHandle,
};
use super::rc_vm::{ExecutionContext, RcVM};

/// Initialize the standard library for RcVM
pub fn init_stdlib(vm: &mut RcVM) -> LuaResult<()> {
    // Get globals table
    let globals = vm.globals()?;
    
    // Initialize base library
    init_base_lib(vm, &globals)?;
    
    // Initialize string library (would be expanded in full implementation)
    init_string_lib(vm, &globals)?;
    
    // Initialize table library (would be expanded in full implementation)
    init_table_lib(vm, &globals)?;
    
    // Other libraries would be added here
    
    eprintln!("RcVM standard library fully initialized");
    
    Ok(())
}

/// Initialize the base library
fn init_base_lib(vm: &mut RcVM, globals: &TableHandle) -> LuaResult<()> {
    // Create _G as reference to globals
    let g_key = vm.create_string("_G")?;
    vm.set_table_field(globals, &Value::String(g_key), &Value::Table(Rc::clone(globals)))?;
    
    // Register base functions
    let functions = [
        ("print", base_print as super::rc_value::CFunction),
        ("type", base_type as super::rc_value::CFunction),
        ("tostring", base_tostring as super::rc_value::CFunction),
        ("tonumber", base_tonumber as super::rc_value::CFunction),
        ("assert", base_assert as super::rc_value::CFunction),
        ("error", base_error as super::rc_value::CFunction),
        ("getmetatable", base_getmetatable as super::rc_value::CFunction),
        ("setmetatable", base_setmetatable as super::rc_value::CFunction),
        ("rawget", base_rawget as super::rc_value::CFunction),
        ("rawset", base_rawset as super::rc_value::CFunction),
        ("rawequal", base_rawequal as super::rc_value::CFunction),
        ("select", base_select as super::rc_value::CFunction),
        ("next", base_next as super::rc_value::CFunction),
        ("pairs", base_pairs as super::rc_value::CFunction),
        ("ipairs", base_ipairs as super::rc_value::CFunction),
    ];
    
    for (name, func) in functions.iter() {
        let key = vm.create_string(name)?;
        vm.set_table_field(globals, &Value::String(key), &Value::CFunction(*func))?;
    }
    
    Ok(())
}

/// Initialize string library
fn init_string_lib(vm: &mut RcVM, globals: &TableHandle) -> LuaResult<()> {
    // Create string table
    let string_table = vm.create_table()?;
    
    // Register string functions
    let functions = [
        ("len", string_len as super::rc_value::CFunction),
        ("sub", string_sub as super::rc_value::CFunction),
        ("upper", string_upper as super::rc_value::CFunction),
        ("lower", string_lower as super::rc_value::CFunction),
        // More string functions would be added here
    ];
    
    for (name, func) in functions.iter() {
        let key = vm.create_string(name)?;
        vm.set_table_field(&string_table, &Value::String(key), &Value::CFunction(*func))?;
    }
    
    // Add string table to globals
    let key = vm.create_string("string")?;
    vm.set_table_field(globals, &Value::String(key), &Value::Table(string_table))?;
    
    Ok(())
}

/// Initialize table library
fn init_table_lib(vm: &mut RcVM, globals: &TableHandle) -> LuaResult<()> {
    // Create table library
    let table_lib = vm.create_table()?;
    
    // Register table functions
    let functions = [
        ("insert", table_insert as super::rc_value::CFunction),
        ("remove", table_remove as super::rc_value::CFunction),
        ("concat", table_concat as super::rc_value::CFunction),
        // More table functions would be added here
    ];
    
    for (name, func) in functions.iter() {
        let key = vm.create_string(name)?;
        vm.set_table_field(&table_lib, &Value::String(key), &Value::CFunction(*func))?;
    }
    
    // Add table library to globals
    let key = vm.create_string("table")?;
    vm.set_table_field(globals, &Value::String(key), &Value::Table(table_lib))?;
    
    Ok(())
}

//
// Base library functions
//

/// print(...) -> nil
/// Prints the given values to stdout.
fn base_print(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let nargs = ctx.arg_count();
    
    for i in 0..nargs {
        if i > 0 {
            print!("\t");
        }
        let value = ctx.get_arg(i)?;
        
        match &value {
            Value::String(handle) => {
                let string_handle = handle.borrow();
                if let Ok(s) = string_handle.to_str() {
                    print!("{}", s);
                } else {
                    print!("(binary data)");
                }
            },
            _ => {
                print!("{}", value);
            }
        }
    }
    println!();
    
    Ok(0) // No return values
}

/// type(v) -> string
/// Returns the type of its only argument, coded as a string.
fn base_type(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("type expects 1 argument".to_string()));
    }
    
    let value = ctx.get_arg(0)?;
    let type_name = value.type_name();
    
    let string_handle = ctx.create_string(type_name)?;
    ctx.push_result(Value::String(string_handle))?;
    
    Ok(1) // One return value
}

/// tostring(v) -> string
/// Returns a string representation of the value.
fn base_tostring(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("tostring expects 1 argument".to_string()));
    }
    
    let value = ctx.get_arg(0)?;
    
    match &value {
        &Value::String(_) => {
            ctx.push_result(value)?;
        },
        &Value::Number(n) => {
            let s = if n.fract() == 0.0 && n.abs() < 1e14 {
                format!("{:.0}", n)
            } else {
                format!("{}", n)
            };
            
            let string_handle = ctx.create_string(&s)?;
            ctx.push_result(Value::String(string_handle))?;
        },
        &Value::Table(ref table) => {
            // Check for __tostring metamethod
            let globals = ctx.globals_handle()?;
            
            // See if the table has a metatable
            let mt = {
                let table_ref = table.borrow();
                table_ref.metatable.clone()
            };
            
            if let Some(metatable) = mt {
                let mt_ref = metatable.borrow();
                let tostring_key = ctx.create_string("__tostring")?;
                if let Some(tostring_fn) = mt_ref.get_field(&Value::String(tostring_key)) {
                    drop(mt_ref);
                    
                    // Call the metamethod
                    // This would normally be handled by the VM
                    let s = format!("<table: {:p}>", Rc::as_ptr(table));
                    let string_handle = ctx.create_string(&s)?;
                    ctx.push_result(Value::String(string_handle))?;
                    return Ok(1);
                }
            }
            
            // No metamethod, use default representation
            let s = format!("<table: {:p}>", Rc::as_ptr(table));
            let string_handle = ctx.create_string(&s)?;
            ctx.push_result(Value::String(string_handle))?;
        },
        _ => {
            let s = format!("{}", value);
            let string_handle = ctx.create_string(&s)?;
            ctx.push_result(Value::String(string_handle))?;
        }
    }
    
    Ok(1) // One return value
}

/// tonumber(e [, base]) -> number or nil
/// Converts argument to a number if possible, or nil if not.
fn base_tonumber(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("tonumber expects 1 or 2 arguments".to_string()));
    }
    
    let value = ctx.get_arg(0)?;
    let base = if ctx.arg_count() > 1 {
        match ctx.get_arg(1)? {
            Value::Number(n) => {
                if n.fract() != 0.0 || n < 2.0 || n > 36.0 {
                    return Err(LuaError::RuntimeError("base must be between 2 and 36".to_string()));
                }
                Some(n as u32)
            }
            _ => {
                return Err(LuaError::TypeError {
                    expected: "number".to_string(),
                    got: ctx.get_arg(1)?.type_name().to_string(),
                });
            }
        }
    } else {
        None
    };
    
    match value {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n))?;
        },
        Value::String(handle) => {
            let string_ref = handle.borrow();
            if let Ok(s) = string_ref.to_str() {
                let trimmed = s.trim();
                
                // Try to parse according to base
                let result = match base {
                    Some(b) => {
                        match i64::from_str_radix(trimmed, b) {
                            Ok(n) => Some(n as f64),
                            Err(_) => None,
                        }
                    },
                    None => {
                        // Try to parse as a regular number
                        match trimmed.parse::<f64>() {
                            Ok(n) => Some(n),
                            Err(_) => None,
                        }
                    }
                };
                
                match result {
                    Some(n) => ctx.push_result(Value::Number(n))?,
                    None => ctx.push_result(Value::Nil)?,
                }
            } else {
                ctx.push_result(Value::Nil)?;
            }
        },
        _ => {
            ctx.push_result(Value::Nil)?;
        }
    }
    
    Ok(1) // One return value
}

/// assert(v [, message]) -> v
/// Raises an error if v is false or nil, otherwise returns v.
fn base_assert(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("assert expects at least 1 argument".to_string()));
    }
    
    let value = ctx.get_arg(0)?;
    
    if value.is_falsey() {
        let message = if ctx.arg_count() > 1 {
            match ctx.get_arg(1)? {
                Value::String(handle) => {
                    let string_ref = handle.borrow();
                    if let Ok(s) = string_ref.to_str() {
                        s.to_string()
                    } else {
                        "assertion failed!".to_string()
                    }
                },
                _ => "assertion failed!".to_string(),
            }
        } else {
            "assertion failed!".to_string()
        };
        
        return Err(LuaError::RuntimeError(message));
    }
    
    // Return all arguments
    for i in 0..ctx.arg_count() {
        ctx.push_result(ctx.get_arg(i)?)?;
    }
    
    Ok(ctx.arg_count() as i32)
}

/// error(message [, level])
/// Terminates the last protected function called and returns message as the error value.
fn base_error(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("error expects at least 1 argument".to_string()));
    }
    
    let message = match ctx.get_arg(0)? {
        Value::String(handle) => {
            let string_ref = handle.borrow();
            if let Ok(s) = string_ref.to_str() {
                s.to_string()
            } else {
                "error".to_string()
            }
        },
        other => format!("{}", other),
    };
    
    Err(LuaError::RuntimeError(message))
}

/// getmetatable(object) -> table or nil
/// Returns the metatable of the given object, or nil if it doesn't have one.
fn base_getmetatable(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("getmetatable expects 1 argument".to_string()));
    }
    
    let object = ctx.get_arg(0)?;
    
    match object {
        Value::Table(table) => {
            let table_ref = table.borrow();
            if let Some(metatable) = &table_ref.metatable {
                ctx.push_result(Value::Table(Rc::clone(metatable)))?;
            } else {
                ctx.push_result(Value::Nil)?;
            }
        },
        _ => {
            // In Lua 5.1, only tables can have metatables directly
            ctx.push_result(Value::Nil)?;
        }
    }
    
    Ok(1) // One return value
}

/// setmetatable(table, metatable) -> table
/// Sets the metatable for the given table.
fn base_setmetatable(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 2 {
        return Err(LuaError::RuntimeError("setmetatable expects 2 arguments".to_string()));
    }
    
    let table_val = ctx.get_arg(0)?;
    let metatable_val = ctx.get_arg(1)?;
    
    // Check first argument is a table
    let table = match table_val {
        Value::Table(ref handle) => handle,
        _ => {
            return Err(LuaError::TypeError {
                expected: "table".to_string(),
                got: table_val.type_name().to_string(),
            });
        }
    };
    
    // Check second argument is a table or nil
    match metatable_val {
        Value::Table(metatable) => {
            // Set the metatable
            let mut table_ref = table.borrow_mut();
            table_ref.metatable = Some(Rc::clone(&metatable));
        },
        Value::Nil => {
            // Remove metatable
            let mut table_ref = table.borrow_mut();
            table_ref.metatable = None;
        },
        _ => {
            return Err(LuaError::TypeError {
                expected: "table or nil".to_string(),
                got: metatable_val.type_name().to_string(),
            });
        }
    }
    
    // Return the table
    ctx.push_result(Value::Table(Rc::clone(table)))?;
    
    Ok(1) // One return value
}

/// rawget(table, index) -> value
/// Gets the real value of table[index] without invoking metamethods.
fn base_rawget(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 2 {
        return Err(LuaError::RuntimeError("rawget expects 2 arguments".to_string()));
    }
    
    let table_val = ctx.get_arg(0)?;
    let key = ctx.get_arg(1)?;
    
    // Check first argument is a table
    let table = match table_val {
        Value::Table(ref handle) => handle,
        _ => {
            return Err(LuaError::TypeError {
                expected: "table".to_string(),
                got: table_val.type_name().to_string(),
            });
        }
    };
    
    // Get the raw value
    let table_ref = table.borrow();
    let value = table_ref.get_field(&key).unwrap_or(Value::Nil);
    
    // Return the value
    ctx.push_result(value)?;
    
    Ok(1) // One return value
}

/// rawset(table, index, value) -> table
/// Sets the real value of table[index] without invoking metamethods.
fn base_rawset(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 3 {
        return Err(LuaError::RuntimeError("rawset expects 3 arguments".to_string()));
    }
    
    let table_val = ctx.get_arg(0)?;
    let key = ctx.get_arg(1)?;
    let value = ctx.get_arg(2)?;
    
    // Check first argument is a table
    let table = match table_val {
        Value::Table(ref handle) => handle,
        _ => {
            return Err(LuaError::TypeError {
                expected: "table".to_string(),
                got: table_val.type_name().to_string(),
            });
        }
    };
    
    // Set the raw value
    let mut table_mut = table.borrow_mut();
    table_mut.set_field(key, value)?;
    
    // Return the table
    ctx.push_result(Value::Table(Rc::clone(table)))?;
    
    Ok(1) // One return value
}

/// rawequal(v1, v2) -> boolean
/// Checks whether v1 is equal to v2, without invoking metamethods.
fn base_rawequal(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 2 {
        return Err(LuaError::RuntimeError("rawequal expects 2 arguments".to_string()));
    }
    
    let v1 = ctx.get_arg(0)?;
    let v2 = ctx.get_arg(1)?;
    
    // Direct equality check
    let result = v1 == v2;
    
    // Return the result
    ctx.push_result(Value::Boolean(result))?;
    
    Ok(1) // One return value
}

/// select(index, ...) -> ...
/// If index is a number, returns all arguments after index.
/// If index is "#", returns the total number of extra arguments.
fn base_select(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("select expects at least 1 argument".to_string()));
    }
    
    let index_arg = ctx.get_arg(0)?;
    
    match index_arg {
        Value::String(handle) => {
            let string_ref = handle.borrow();
            if let Ok(s) = string_ref.to_str() {
                if s == "#" {
                    // Return argument count
                    ctx.push_result(Value::Number((ctx.arg_count() - 1) as f64))?;
                    return Ok(1);
                }
            }
            
            return Err(LuaError::RuntimeError("invalid argument to select".to_string()));
        },
        Value::Number(n) => {
            let index = if n.fract() == 0.0 && n > 0.0 {
                n as usize
            } else {
                return Err(LuaError::RuntimeError("index must be a positive integer".to_string()));
            };
            
            // Return all arguments from index onward
            let start_idx = index;
            let count = ctx.arg_count();
            
            if start_idx > count {
                // No values to return
                return Ok(0);
            }
            
            for i in start_idx..count {
                ctx.push_result(ctx.get_arg(i)?)?;
            }
            
            return Ok((count - start_idx) as i32);
        },
        _ => {
            return Err(LuaError::TypeError {
                expected: "number or '#'".to_string(),
                got: index_arg.type_name().to_string(),
            });
        }
    }
}

/// next(table [, index]) -> key, value
/// Returns the next key-value pair in a table after the given index.
fn base_next(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("next expects at least 1 argument".to_string()));
    }
    
    let table_val = ctx.get_arg(0)?;
    
    // Check first argument is a table
    let table = match table_val {
        Value::Table(ref handle) => handle,
        _ => {
            return Err(LuaError::TypeError {
                expected: "table".to_string(),
                got: table_val.type_name().to_string(),
            });
        }
    };
    
    // Get current key
    let key = if ctx.arg_count() > 1 {
        ctx.get_arg(1)?
    } else {
        Value::Nil
    };
    
    // Get next key-value pair
    match ctx.table_next(&table, &key)? {
        Some((next_key, next_value)) => {
            // Return key and value
            ctx.push_result(next_key)?;
            ctx.push_result(next_value)?;
            Ok(2) // Two return values
        },
        None => {
            // End of iteration
            ctx.push_result(Value::Nil)?;
            Ok(1) // One return value (nil)
        }
    }
}

/// pairs(t) -> iter_func, t, nil
/// Returns three values that allow iteration over a table's key-value pairs.
fn base_pairs(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("pairs expects 1 argument".to_string()));
    }
    
    let table_val = ctx.get_arg(0)?;
    
    // Check argument is a table
    match table_val {
        Value::Table(_) => {},
        _ => {
            return Err(LuaError::TypeError {
                expected: "table".to_string(),
                got: table_val.type_name().to_string(),
            });
        }
    }
    
    // Get the next function
    let globals = ctx.globals_handle()?;
    let next_key = ctx.create_string("next")?;
    let next_fn = ctx.get_table_field(&globals, &Value::String(next_key))?;
    
    // Return the iterator triplet: next, table, nil
    ctx.push_result(next_fn)?;
    ctx.push_result(table_val)?;
    ctx.push_result(Value::Nil)?;
    
    Ok(3) // Three return values
}

/// ipairs(t) -> iter_func, t, 0
/// Returns three values that allow iteration over a table's array part.
fn base_ipairs(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("ipairs expects 1 argument".to_string()));
    }
    
    let table_val = ctx.get_arg(0)?;
    
    // Check argument is a table
    match table_val {
        Value::Table(_) => {},
        _ => {
            return Err(LuaError::TypeError {
                expected: "table".to_string(),
                got: table_val.type_name().to_string(),
            });
        }
    }
    
    // Get the ipairs iterator function
    let ipairs_iter = Value::CFunction(ipairs_iter as super::rc_value::CFunction);
    
    // Return the iterator triplet: ipairs_iter, table, 0
    ctx.push_result(ipairs_iter)?;
    ctx.push_result(table_val)?;
    ctx.push_result(Value::Number(0.0))?;
    
    Ok(3) // Three return values
}

/// ipairs iterator function
fn ipairs_iter(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 2 {
        return Err(LuaError::RuntimeError("ipairs_iter expects 2 arguments".to_string()));
    }
    
    let table_val = ctx.get_arg(0)?;
    let index_val = ctx.get_arg(1)?;
    
    // Check arguments
    let table = match table_val {
        Value::Table(ref handle) => handle,
        _ => {
            return Err(LuaError::TypeError {
                expected: "table".to_string(),
                got: table_val.type_name().to_string(),
            });
        }
    };
    
    let index = match index_val {
        Value::Number(n) => {
            if n.fract() != 0.0 || n < 0.0 {
                return Err(LuaError::RuntimeError("index must be a non-negative integer".to_string()));
            }
            n as usize
        },
        _ => {
            return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: index_val.type_name().to_string(),
            });
        }
    };
    
    // Get next index
    let next_index = index + 1;
    
    // Get value at next index
    let next_key = Value::Number(next_index as f64);
    let table_ref = table.borrow();
    
    if let Some(value) = table_ref.get_field(&next_key) {
        if value.is_nil() {
            // End of iteration
            ctx.push_result(Value::Nil)?;
            return Ok(1);
        }
        
        // Return index, value pair
        ctx.push_result(next_key)?;
        ctx.push_result(value)?;
        return Ok(2);
    } else {
        // End of iteration
        ctx.push_result(Value::Nil)?;
        return Ok(1);
    }
}

//
// String library functions
//

/// string.len(s) -> number
/// Returns the length of a string in bytes.
fn string_len(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("string.len expects 1 argument".to_string()));
    }
    
    let value = ctx.get_arg(0)?;
    
    let length = match value {
        Value::String(handle) => {
            let string_ref = handle.borrow();
            string_ref.len() as f64
        },
        _ => {
            return Err(LuaError::TypeError {
                expected: "string".to_string(),
                got: value.type_name().to_string(),
            });
        }
    };
    
    ctx.push_result(Value::Number(length))?;
    
    Ok(1) // One return value
}

/// string.sub(s, i [, j]) -> string
/// Returns the substring of s from i to j (inclusive).
fn string_sub(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 2 {
        return Err(LuaError::RuntimeError("string.sub expects at least 2 arguments".to_string()));
    }
    
    let s = ctx.get_arg_str(0)?;
    let i = ctx.get_number_arg(1)? as isize;
    let j = if ctx.arg_count() > 2 {
        ctx.get_number_arg(2)? as isize
    } else {
        -1
    };
    
    // Convert to 0-based indices
    let len = s.len();
    let start = if i < 0 { (len as isize + i) } else { i - 1 };
    let end = if j < 0 { (len as isize + j + 1) } else { j };
    
    // Clamp indices
    let start = start.max(0).min(len as isize) as usize;
    let end = end.max(start as isize).min(len as isize) as usize;
    
    // Extract substring
    let substring = &s[start..end];
    
    // Return substring
    let string_handle = ctx.create_string(substring)?;
    ctx.push_result(Value::String(string_handle))?;
    
    Ok(1) // One return value
}

/// string.upper(s) -> string
/// Returns a copy of s with all letters converted to uppercase.
fn string_upper(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("string.upper expects 1 argument".to_string()));
    }
    
    let s = ctx.get_arg_str(0)?;
    let upper = s.to_uppercase();
    
    let string_handle = ctx.create_string(&upper)?;
    ctx.push_result(Value::String(string_handle))?;
    
    Ok(1) // One return value
}

/// string.lower(s) -> string
/// Returns a copy of s with all letters converted to lowercase.
fn string_lower(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("string.lower expects 1 argument".to_string()));
    }
    
    let s = ctx.get_arg_str(0)?;
    let lower = s.to_lowercase();
    
    let string_handle = ctx.create_string(&lower)?;
    ctx.push_result(Value::String(string_handle))?;
    
    Ok(1) // One return value
}

//
// Table library functions
//

/// table.insert(t, [pos,] value)
/// Inserts element value at position pos in table t, shifting up other elements.
fn table_insert(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 2 {
        return Err(LuaError::RuntimeError("table.insert expects at least 2 arguments".to_string()));
    }
    
    let table_val = ctx.get_arg(0)?;
    
    // Check first argument is a table
    let table = match table_val {
        Value::Table(ref handle) => handle,
        _ => {
            return Err(LuaError::TypeError {
                expected: "table".to_string(),
                got: table_val.type_name().to_string(),
            });
        }
    };
    
    // Get position and value
    let (pos, value) = if ctx.arg_count() >= 3 {
        // table.insert(t, pos, value)
        let pos_val = ctx.get_arg(1)?;
        let pos = match pos_val {
            Value::Number(n) => {
                if n.fract() != 0.0 || n < 1.0 {
                    return Err(LuaError::RuntimeError("position must be a positive integer".to_string()));
                }
                n as usize
            },
            _ => {
                return Err(LuaError::TypeError {
                    expected: "number".to_string(),
                    got: pos_val.type_name().to_string(),
                });
            }
        };
        
        (pos, ctx.get_arg(2)?)
    } else {
        // table.insert(t, value)
        let mut table_mut = table.borrow_mut();
        let pos = table_mut.array_len() + 1;
        drop(table_mut);
        
        (pos, ctx.get_arg(1)?)
    };
    
    // Perform the insertion
    {
        let mut table_mut = table.borrow_mut();
        
        // Get array length
        let len = table_mut.array_len();
        
        if pos <= len + 1 {
            // Insert in array part
            if pos <= len {
                // Shift elements up
                table_mut.array.insert(pos - 1, value);
            } else {
                // Append to end
                table_mut.array.push(value);
            }
        } else {
            // Insert beyond end of array
            while table_mut.array.len() < pos - 1 {
                table_mut.array.push(Value::Nil);
            }
            table_mut.array.push(value);
        }
    }
    
    Ok(0) // No return values
}

/// table.remove(t [, pos]) -> value
/// Removes from t the element at position pos, returning its value.
fn table_remove(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("table.remove expects at least 1 argument".to_string()));
    }
    
    let table_val = ctx.get_arg(0)?;
    
    // Check first argument is a table
    let table = match table_val {
        Value::Table(ref handle) => handle,
        _ => {
            return Err(LuaError::TypeError {
                expected: "table".to_string(),
                got: table_val.type_name().to_string(),
            });
        }
    };
    
    // Get position
    let pos = if ctx.arg_count() >= 2 {
        let pos_val = ctx.get_arg(1)?;
        match pos_val {
            Value::Number(n) => {
                if n.fract() != 0.0 || n < 1.0 {
                    return Err(LuaError::RuntimeError("position must be a positive integer".to_string()));
                }
                n as usize
            },
            _ => {
                return Err(LuaError::TypeError {
                    expected: "number".to_string(),
                    got: pos_val.type_name().to_string(),
                });
            }
        }
    } else {
        let table_ref = table.borrow();
        table_ref.array_len()
    };
    
    // Remove the element
    let removed = {
        let mut table_mut = table.borrow_mut();
        
        // Get array length
        let len = table_mut.array_len();
        
        if pos <= len {
            // Remove from array part
            let value = table_mut.array.remove(pos - 1);
            Some(value)
        } else {
            // Out of bounds
            None
        }
    };
    
    // Return the removed value
    match removed {
        Some(value) => {
            ctx.push_result(value)?;
            Ok(1) // One return value
        },
        None => {
            ctx.push_result(Value::Nil)?;
            Ok(1) // One return value (nil)
        }
    }
}

/// table.concat(t [, sep [, i [, j]]]) -> string
/// Returns a string built by concatenating the elements of t.
fn table_concat(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("table.concat expects at least 1 argument".to_string()));
    }
    
    let table_val = ctx.get_arg(0)?;
    
    // Check first argument is a table
    let table = match table_val {
        Value::Table(ref handle) => handle,
        _ => {
            return Err(LuaError::TypeError {
                expected: "table".to_string(),
                got: table_val.type_name().to_string(),
            });
        }
    };
    
    // Get separator
    let sep = if ctx.arg_count() >= 2 {
        ctx.get_arg_str(1)?
    } else {
        "".to_string()
    };
    
    // Get start and end indices
    let table_ref = table.borrow();
    let len = table_ref.array_len();
    
    let start = if ctx.arg_count() >= 3 {
        let i_val = ctx.get_arg(2)?;
        match i_val {
            Value::Number(n) => {
                if n.fract() != 0.0 || n < 1.0 {
                    return Err(LuaError::RuntimeError("start index must be a positive integer".to_string()));
                }
                n as usize
            },
            _ => {
                return Err(LuaError::TypeError {
                    expected: "number".to_string(),
                    got: i_val.type_name().to_string(),
                });
            }
        }
    } else {
        1
    };
    
    let end_idx = if ctx.arg_count() >= 4 {
        let j_val = ctx.get_arg(3)?;
        match j_val {
            Value::Number(n) => {
                if n.fract() != 0.0 || n < 0.0 {
                    return Err(LuaError::RuntimeError("end index must be a non-negative integer".to_string()));
                }
                n as usize
            },
            _ => {
                return Err(LuaError::TypeError {
                    expected: "number".to_string(),
                    got: j_val.type_name().to_string(),
                });
            }
        }
    } else {
        len
    };
    
    // Concat elements
    let mut result = String::new();
    
    for i in start..=end_idx.min(len) {
        if i > start {
            result.push_str(&sep);
        }
        
        if let Some(value) = table_ref.get_field(&Value::Number(i as f64)) {
            match value {
                Value::String(handle) => {
                    let string_ref = handle.borrow();
                    if let Ok(s) = string_ref.to_str() {
                        result.push_str(s);
                    } else {
                        return Err(LuaError::RuntimeError("table contains a non-string value".to_string()));
                    }
                },
                Value::Number(n) => {
                    result.push_str(&n.to_string());
                },
                _ => {
                    return Err(LuaError::TypeError {
                        expected: "string or number".to_string(),
                        got: value.type_name().to_string(),
                    });
                }
            }
        }
    }
    
    // Return the result
    let string_handle = ctx.create_string(&result)?;
    ctx.push_result(Value::String(string_handle))?;
    
    Ok(1) // One return value
}

// Extension traits for RcVM
pub trait RcVMExt {
    /// Create a string
    fn create_string(&self, s: &str) -> LuaResult<StringHandle>;
    
    /// Create a table
    fn create_table(&self) -> LuaResult<TableHandle>;
    
    /// Get table field
    fn get_table_field(&self, table: &TableHandle, key: &Value) -> LuaResult<Value>;
    
    /// Set table field
    fn set_table_field(&self, table: &TableHandle, key: &Value, value: &Value) -> LuaResult<()>;
    
    /// Get globals table
    fn globals(&self) -> LuaResult<TableHandle>;
}

impl RcVMExt for RcVM {
    fn create_string(&self, s: &str) -> LuaResult<StringHandle> {
        self.heap.create_string(s)
    }
    
    fn create_table(&self) -> LuaResult<TableHandle> {
        Ok(self.heap.create_table())
    }
    
    fn get_table_field(&self, table: &TableHandle, key: &Value) -> LuaResult<Value> {
        self.heap.get_table_field(table, key)
    }
    
    fn set_table_field(&self, table: &TableHandle, key: &Value, value: &Value) -> LuaResult<()> {
        self.heap.set_table_field(table, key, value)
    }
    
    fn globals(&self) -> LuaResult<TableHandle> {
        Ok(self.heap.globals())
    }
}