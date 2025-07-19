//! RefCell-based Standard Library Implementation
//! 
//! This module provides the standard library functions for the RefCellVM implementation.

use super::error::{LuaError, LuaResult};
use super::handle::{StringHandle, TableHandle, ClosureHandle};
use super::value::{Value, CFunction};
use super::refcell_vm::{RefCellVM, ExecutionContext};
use super::stdlib::{math, string, table};

/// Print function adapter matching CFunction signature
pub fn refcell_print_adapter(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let arg_count = ctx.arg_count();
    
    // Collect all arguments first to build the output
    let mut parts = Vec::with_capacity(arg_count);
    for i in 0..arg_count {
        let val = ctx.get_arg(i)?;
        
        // Debug: Log the raw value being processed
        eprintln!("DEBUG print: Processing arg {} = {:?}", i, val);
        
        // Convert value to string representation
        let s = match val {
            Value::String(h) => ctx.get_string_from_handle(h)?,
            Value::Nil => "nil".to_string(),
            Value::Boolean(b) => {
                // Explicitly convert boolean to string to ensure correctness
                let string_repr = if b { "true" } else { "false" };
                eprintln!("DEBUG print: Boolean {} -> '{}'", b, string_repr);
                string_repr.to_string()
            },
            Value::Number(n) => {
                // Format numbers like Lua does
                if n.fract() == 0.0 && n.abs() < 1e14 {
                    format!("{:.0}", n)
                } else {
                    n.to_string()
                }
            },
            Value::Table(_) => "table".to_string(),
            Value::Closure(_) => "function".to_string(),
            Value::CFunction(_) => "function".to_string(),
            _ => {
                eprintln!("DEBUG print: Unhandled value type, using type_name()");
                val.type_name().to_string()
            },
        };
        parts.push(s);
    }
    
    // Print with tab separation like Lua's print
    println!("{}", parts.join("\t"));
    
    Ok(0) // No results
}

/// Type function adapter matching CFunction signature
pub fn refcell_type_adapter(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("bad argument #1 to 'type' (value expected)".to_string()));
    }
    
    let value = ctx.get_arg(0)?;
    let type_name = value.type_name();
    
    let handle = ctx.create_string(type_name)?;
    ctx.push_result(Value::String(handle))?;
    
    Ok(1) // One result
}

/// Assert function adapter matching CFunction signature
pub fn refcell_assert_adapter(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("bad argument #1 to 'assert' (value expected)".to_string()));
    }
    
    let value = ctx.get_arg(0)?;
    
    // Debug: Log the actual value received by assert
    match &value {
        Value::Boolean(b) => println!("DEBUG: assert received boolean: {}", b),
        Value::Number(n) => println!("DEBUG: assert received number: {}", n),
        Value::Nil => println!("DEBUG: assert received nil"),
        _ => println!("DEBUG: assert received {}: {:?}", value.type_name(), value),
    }
    
    if value.is_falsey() {
        println!("DEBUG: assert condition is falsey, failing assertion");
        let message = if ctx.arg_count() >= 2 {
            match ctx.get_arg(1)? {
                Value::String(h) => ctx.get_string_from_handle(h)?,
                _ => "assertion failed!".to_string(),
            }
        } else {
            "assertion failed!".to_string()
        };
        
        return Err(LuaError::RuntimeError(message));
    }
    
    println!("DEBUG: assert condition is truthy, assertion passes");
    
    // Return all arguments
    let arg_count = ctx.arg_count();
    
    // Store all arguments first to avoid multiple mutable borrows
    let mut args = Vec::with_capacity(arg_count);
    for i in 0..arg_count {
        args.push(ctx.get_arg(i)?);
    }
    
    // Now push all stored arguments
    for arg in args {
        ctx.push_result(arg)?;
    }
    
    Ok(arg_count as i32)
}

/// Next function adapter matching CFunction signature
/// 
/// According to Lua 5.1 specification:
/// - next (table [, index])
/// - First argument must be a table
/// - Second argument is an index in the table (nil is the initial value)
/// - Returns the next index and its associated value
/// - Returns nil when there are no more elements
pub fn refcell_next_adapter(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    // Check argument count
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("bad argument #1 to 'next' (table expected)".to_string()));
    }
    
    // First argument must be a table
    let table = ctx.get_arg(0)?;
    let table_handle = match table {
        Value::Table(h) => h,
        _ => return Err(LuaError::RuntimeError(format!(
            "bad argument #1 to 'next' (table expected, got {})",
            table.type_name()
        ))),
    };
    
    // Second argument is the key (nil if absent or explicitly nil)
    let key = if ctx.arg_count() >= 2 {
        ctx.get_arg(1)?
    } else {
        Value::Nil
    };
    
    eprintln!("DEBUG next: table={:?}, key={:?}", table_handle, key);
    
    // Get the next key-value pair
    match ctx.table_next(table_handle, key)? {
        Some((next_key, next_value)) => {
            eprintln!("DEBUG next: returning key={:?}, value={:?}", next_key, next_value);
            ctx.push_result(next_key)?;
            ctx.push_result(next_value)?;
            Ok(2) // Two results - key and value
        },
        None => {
            eprintln!("DEBUG next: end of table, returning nil");
            ctx.push_result(Value::Nil)?;
            Ok(1) // One result - nil (end of iteration)
        }
    }
}



/// Pairs iterator function
fn pairs_iterator(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    // pairs iterator takes (table, lastkey) and returns (nextkey, nextvalue)
    if ctx.arg_count() < 2 {
        return Err(LuaError::RuntimeError("pairs iterator expects 2 arguments".to_string()));
    }
    
    // For now, return nil to indicate end of iteration
    ctx.push_result(Value::Nil)?;
    
    Ok(1)
}

/// Pairs function adapter matching CFunction signature
/// According to Lua 5.1 spec, pairs must return exactly 3 values in this order:
/// 1. next function (iterator)
/// 2. table (state)
/// 3. nil (initial control value)
pub fn refcell_pairs_adapter(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("bad argument #1 to 'pairs' (table expected)".to_string()));
    }
    
    let table = ctx.get_arg(0)?;
    if !table.is_table() {
        return Err(LuaError::RuntimeError(format!(
            "bad argument #1 to 'pairs' (table expected, got {})", 
            table.type_name()
        )));
    }
    
    // Get the global 'next' function
    let next_func = ctx.globals_get("next")?;
    if next_func.is_nil() {
        return Err(LuaError::RuntimeError("'next' function not found in global table".to_string()));
    }
    
    eprintln!("DEBUG pairs: Returning next function, table, nil triplet");
    
    // According to spec, return exactly:
    // 1. next function (iterator function)
    // 2. table (state parameter)
    // 3. nil (initial control value)
    ctx.push_result(next_func)?;
    ctx.push_result(table)?;
    ctx.push_result(Value::Nil)?;
    
    Ok(3) // Three results
}

/// Implementation of the ipairs iterator function
/// According to Lua 5.1 specification, the iterator function for ipairs:
/// - Takes two arguments: table and index
/// - Returns nil when the iteration is complete
/// - Otherwise returns the next index and value pair
fn ipairs_iterator(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    eprintln!("DEBUG ipairs_iterator: Called with {} arguments", ctx.arg_count());
    
    // First argument must be a table
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("ipairs iterator expects a table argument".to_string()));
    }
    
    let table = ctx.get_arg(0)?;
    let table_handle = match table {
        Value::Table(h) => h,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: table.type_name().to_string(),
        }),
    };
    
    // Get the index
    let index = match ctx.get_arg(1)? {
        Value::Number(n) => n as i64,
        Value::Nil => 0, // Start of iteration
        _ => return Err(LuaError::TypeError {
            expected: "number or nil".to_string(),
            got: ctx.get_arg(1)?.type_name().to_string(),
        }),
    };
    
    eprintln!("DEBUG ipairs_iterator: table={:?}, index={}", table_handle, index);
    
    // Calculate next index
    let next_index = index + 1;
    
    // Check if the next element exists
    let next_key = Value::Number(next_index as f64);
    let next_value = ctx.table_raw_get(table_handle, next_key.clone())?;
    
    if !next_value.is_nil() {
        // Return next_index, next_value
        eprintln!("DEBUG ipairs_iterator: Found element at index {}: {:?}", next_index, next_value);
        ctx.push_result(next_key)?;
        ctx.push_result(next_value)?;
        return Ok(2); // Two results: index and value
    } else {
        // End of array - return nil
        eprintln!("DEBUG ipairs_iterator: End of array at index {}", next_index);
        ctx.push_result(Value::Nil)?;
        return Ok(1); // One result: nil
    }
}

/// IPairs function adapter matching CFunction signature
/// According to Lua 5.1 spec, ipairs must return exactly 3 values in this order:
/// 1. ipairs_iterator function
/// 2. table (state)
/// 3. 0 (initial index value)
pub fn refcell_ipairs_adapter(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("bad argument #1 to 'ipairs' (table expected)".to_string()));
    }
    
    let table = ctx.get_arg(0)?;
    if !table.is_table() {
        return Err(LuaError::RuntimeError(format!(
            "bad argument #1 to 'ipairs' (table expected, got {})",
            table.type_name()
        )));
    }
    
    eprintln!("DEBUG ipairs: Returning iterator, table, 0 triplet");
    
    // According to spec, return exactly:
    // 1. ipairs_iterator function
    // 2. table (state parameter)
    // 3. 0 (initial control value)
    ctx.push_result(Value::CFunction(ipairs_iterator))?;
    ctx.push_result(table)?;
    ctx.push_result(Value::Number(0.0))?;
    
    Ok(3) // Three results
}

/// Tostring function adapter matching CFunction signature
pub fn refcell_tostring_adapter(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("bad argument #1 to 'tostring' (value expected)".to_string()));
    }
    
    let value = ctx.get_arg(0)?;
    
    // Check for __tostring metamethod first
    if let Value::Table(handle) = &value {
        if let Some(metatable) = ctx.get_metatable(*handle)? {
            let tostring_key = ctx.create_string("__tostring")?;
            let tostring_method = ctx.get_table_field(metatable, &Value::String(tostring_key))?;
            
            if !tostring_method.is_nil() {
                eprintln!("DEBUG tostring_adapter: Found __tostring metamethod");
                
                // For now, we'll use the raw string conversion since metamethod calls
                // are not yet fully implemented in the current context
                eprintln!("DEBUG tostring_adapter: __tostring metamethod calls not yet fully supported in C context");
            }
        }
    }
    
    // Default string conversion
    let s = match value {
        Value::Nil => "nil".to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 1e14 {
                format!("{:.0}", n)
            } else {
                n.to_string()
            }
        }
        Value::String(handle) => ctx.get_string_from_handle(handle)?,
        Value::Table(_) => {
            // Table without __tostring
            format!("table: {:?}", value)
        }
        Value::Closure(_) => format!("function: {:?}", value),
        Value::Thread(_) => format!("thread: {:?}", value),
        Value::CFunction(_) => "function".to_string(),
        Value::FunctionProto(_) => "proto".to_string(),
        Value::UserData(_) => format!("userdata: {:?}", value),
    };
    
    let result = ctx.create_string(&s)?;
    ctx.push_result(Value::String(result))?;
    Ok(1)
}

/// Tonumber function adapter matching CFunction signature
pub fn refcell_tonumber_adapter(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("bad argument #1 to 'tonumber' (value expected)".to_string()));
    }
    
    let base = if ctx.arg_count() >= 2 {
        match ctx.get_arg(1)? {
            Value::Number(n) => {
                let b = n as i32;
                if b < 2 || b > 36 {
                    return Err(LuaError::RuntimeError("bad argument #2 to 'tonumber' (base out of range)".to_string()));
                }
                Some(b as u32)
            },
            _ => return Err(LuaError::RuntimeError("bad argument #2 to 'tonumber' (number expected)".to_string())),
        }
    } else {
        None
    };
    
    match ctx.get_arg(0)? {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n))?;
        },
        Value::String(h) => {
            let s = ctx.get_string_from_handle(h)?;
            let s = s.trim(); // Lua tonumber trims whitespace
            
            let parsed = if let Some(base) = base {
                // Parse with specified base
                i64::from_str_radix(s, base).map(|n| n as f64).ok()
            } else {
                // Try to parse as float
                s.parse::<f64>().ok()
            };
            
            match parsed {
                Some(n) => ctx.push_result(Value::Number(n))?,
                None => ctx.push_result(Value::Nil)?,
            }
        },
        _ => {
            ctx.push_result(Value::Nil)?;
        },
    }
    
    Ok(1) // One result
}

/// Getmetatable function adapter matching CFunction signature
pub fn refcell_getmetatable_adapter(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("bad argument #1 to 'getmetatable' (value expected)".to_string()));
    }
    
    let value = ctx.get_arg(0)?;
    
    match value {
        Value::Table(handle) => {
            if let Some(mt) = ctx.get_metatable(handle)? {
                // Check for __metatable field (metatable protection)
                let metatable_key = ctx.create_string("__metatable")?;
                let protected_value = ctx.get_table_field(mt, &Value::String(metatable_key))?;
                
                if !protected_value.is_nil() {
                    // Return the __metatable value instead of the actual metatable
                    ctx.push_result(protected_value)?;
                } else {
                    // Return the actual metatable
                    ctx.push_result(Value::Table(mt))?;
                }
            } else {
                ctx.push_result(Value::Nil)?;
            }
        }
        _ => {
            // Non-table values return nil
            ctx.push_result(Value::Nil)?;
        }
    }
    Ok(1)
}

/// Setmetatable function adapter matching CFunction signature
pub fn refcell_setmetatable_adapter(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 2 {
        return Err(LuaError::RuntimeError("bad argument #2 to 'setmetatable' (nil or table expected)".to_string()));
    }
    
    let table = ctx.get_arg(0)?;
    let metatable = ctx.get_arg(1)?;
    
    // First argument must be a table
    let table_handle = match table {
        Value::Table(h) => h,
        _ => return Err(LuaError::RuntimeError(format!(
            "bad argument #1 to 'setmetatable' (table expected, got {})",
            table.type_name()
        ))),
    };
    
    // Second argument must be nil or table
    let metatable_handle = match metatable {
        Value::Nil => None,
        Value::Table(h) => Some(h),
        _ => return Err(LuaError::RuntimeError(format!(
            "bad argument #2 to 'setmetatable' (nil or table expected, got {})",
            metatable.type_name()
        ))),
    };
    
    // Set the metatable
    ctx.set_metatable(table_handle, metatable_handle)?;
    
    // Return the table
    ctx.push_result(Value::Table(table_handle))?;
    
    Ok(1) // One result
}

/// Pcall function adapter matching CFunction signature
/// 
/// pcall (f, arg1, ···)
/// 
/// Calls function f with the given arguments in protected mode. This means that
/// any error inside f is not propagated; instead, pcall catches the error and
/// returns a status code. Its first result is the status code (a boolean), which
/// is true if the call succeeds without errors. In such case, pcall also returns
/// all results from the call, after this first result. In case of any error,
/// pcall returns false plus the error message.
pub fn refcell_pcall_adapter(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    // pcall requires at least 1 argument (the function to call)
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("bad argument #1 to 'pcall' (value expected)".to_string()));
    }
    
    // Get the function to call
    let func = ctx.get_arg(0)?;
    
    eprintln!("DEBUG pcall: Function to call: {:?}", func);
    
    // Validate that the first argument is callable
    match &func {
        Value::Closure(closure_handle) => {
            eprintln!("DEBUG pcall: Calling Lua closure");
            
            // Collect arguments for the function (all arguments after the function itself)
            let arg_count = ctx.arg_count();
            let mut args = Vec::with_capacity(arg_count - 1);
            for i in 1..arg_count {
                args.push(ctx.get_arg(i)?);
            }
            
            eprintln!("DEBUG pcall: Passing {} arguments to function", args.len());
            
            // Try to execute the function in protected mode
            match ctx.execute_function(*closure_handle, &args) {
                Ok(result) => {
                    eprintln!("DEBUG pcall: Function call succeeded");
                    // Success: return true followed by the function's results
                    ctx.push_result(Value::Boolean(true))?;
                    ctx.push_result(result)?;
                    Ok(2) // true + one result (execute_function returns a single value)
                },
                Err(LuaError::NotImplemented(_)) => {
                    eprintln!("DEBUG pcall: execute_function not implemented, falling back");
                    // The execute_function method is not implemented in RefCellVM yet
                    // For now, return an error indicating pcall is not fully supported
                    ctx.push_result(Value::Boolean(false))?;
                    let error_msg = ctx.create_string("pcall: protected calls not yet fully implemented in RefCellVM")?;
                    ctx.push_result(Value::String(error_msg))?;
                    Ok(2) // false + error message
                },
                Err(e) => {
                    eprintln!("DEBUG pcall: Function call failed with error: {:?}", e);
                    // Error occurred: return false and error message
                    ctx.push_result(Value::Boolean(false))?;
                    let error_msg = ctx.create_string(&e.to_string())?;
                    ctx.push_result(Value::String(error_msg))?;
                    Ok(2) // false + error message
                }
            }
        },
        Value::CFunction(cfunc) => {
            eprintln!("DEBUG pcall: Calling C function");
            
            // For C functions, we need to call them directly
            // Since we can't directly invoke a C function from within another C function
            // in the current architecture, we report this limitation
            ctx.push_result(Value::Boolean(false))?;
            let error_msg = ctx.create_string("pcall: protected C function calls not yet implemented")?;
            ctx.push_result(Value::String(error_msg))?;
            Ok(2) // false + error message
        },
        _ => {
            eprintln!("DEBUG pcall: Attempted to call non-function value: {:?}", func);
            // Not a function - return error
            ctx.push_result(Value::Boolean(false))?;
            let error_msg = ctx.create_string(&format!("attempt to call a {} value", func.type_name()))?;
            ctx.push_result(Value::String(error_msg))?;
            Ok(2) // false + error message
        }
    }
}

/// Rawget function adapter matching CFunction signature
/// 
/// rawget (table, index)
/// 
/// Gets the real value of table[index], without invoking any metamethod.
/// table must be a table; index may be any value.
pub fn refcell_rawget_adapter(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 2 {
        return Err(LuaError::RuntimeError("bad argument #2 to 'rawget' (value expected)".to_string()));
    }
    
    let table = ctx.get_arg(0)?;
    let key = ctx.get_arg(1)?;
    
    eprintln!("DEBUG rawget: table={:?}, key={:?}", table, key);
    
    // First argument must be a table
    let table_handle = match table {
        Value::Table(h) => h,
        _ => return Err(LuaError::RuntimeError(format!(
            "bad argument #1 to 'rawget' (table expected, got {})",
            table.type_name()
        ))),
    };
    
    // Perform raw table access (no metamethods)
    // Use table_raw_get for clarity that this bypasses metamethods
    let value = ctx.table_raw_get(table_handle, key)?;
    
    eprintln!("DEBUG rawget: retrieved value={:?}", value);
    
    ctx.push_result(value)?;
    
    Ok(1) // One result
}

/// Rawset function adapter matching CFunction signature
/// 
/// rawset (table, index, value)
/// 
/// Sets the real value of table[index] to value, without invoking any metamethod.
/// table must be a table, index any value different from nil, and value any Lua value.
/// This function returns table.
pub fn refcell_rawset_adapter(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 3 {
        return Err(LuaError::RuntimeError("bad argument #3 to 'rawset' (value expected)".to_string()));
    }
    
    let table = ctx.get_arg(0)?;
    let key = ctx.get_arg(1)?;
    let value = ctx.get_arg(2)?;
    
    eprintln!("DEBUG rawset: table={:?}, key={:?}, value={:?}", table, key, value);
    
    // First argument must be a table
    let table_handle = match table {
        Value::Table(h) => h,
        _ => return Err(LuaError::RuntimeError(format!(
            "bad argument #1 to 'rawset' (table expected, got {})",
            table.type_name()
        ))),
    };
    
    // Key cannot be nil
    if key.is_nil() {
        return Err(LuaError::RuntimeError("table index is nil".to_string()));
    }
    
    // Perform raw table assignment (no metamethods)
    // Use table_raw_set for clarity that this bypasses metamethods
    ctx.table_raw_set(table_handle, key, value)?;
    
    eprintln!("DEBUG rawset: assignment complete");
    
    // Return the table
    ctx.push_result(Value::Table(table_handle))?;
    
    Ok(1) // One result
}

/// Rawequal function adapter matching CFunction signature
/// 
/// rawequal (v1, v2)
/// 
/// Checks whether v1 is equal to v2, without invoking any metamethod.
/// Returns a boolean.
pub fn refcell_rawequal_adapter(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 2 {
        return Err(LuaError::RuntimeError("bad argument #2 to 'rawequal' (value expected)".to_string()));
    }
    
    let v1 = ctx.get_arg(0)?;
    let v2 = ctx.get_arg(1)?;
    
    eprintln!("DEBUG rawequal: v1={:?}, v2={:?}", v1, v2);
    
    // Perform raw equality comparison without metamethods
    let equal = match (&v1, &v2) {
        (Value::Nil, Value::Nil) => true,
        (Value::Boolean(b1), Value::Boolean(b2)) => b1 == b2,
        (Value::Number(n1), Value::Number(n2)) => n1 == n2,
        (Value::String(s1), Value::String(s2)) => {
            // Compare string handles - they should be interned so handle equality works
            s1 == s2
        },
        (Value::Table(t1), Value::Table(t2)) => {
            // Tables are equal only if they're the same object
            t1 == t2
        },
        (Value::Closure(c1), Value::Closure(c2)) => {
            // Closures are equal only if they're the same object
            c1 == c2
        },
        (Value::CFunction(f1), Value::CFunction(f2)) => {
            // Compare function pointers
            std::ptr::eq(*f1 as *const (), *f2 as *const ())
        },
        _ => false, // Different types are never equal
    };
    
    eprintln!("DEBUG rawequal: result={}", equal);
    
    ctx.push_result(Value::Boolean(equal))?;
    
    Ok(1) // One result
}

/// Initialize the standard library for RefCellVM
pub fn init_refcell_stdlib(vm: &mut RefCellVM) -> LuaResult<()> {
    println!("Initializing RefCellVM standard library");
    
    // Get global table
    let globals = vm.heap().globals()?;
    
    // Set up _G._G = _G as required
    let g_name = vm.heap().create_string("_G")?;
    vm.heap().set_table_field(globals, &Value::String(g_name), &Value::Table(globals))?;
    
    // Register basic functions - these are already correctly typed as CFunction
    let name_function_pairs: &[(&str, CFunction)] = &[
        ("print", refcell_print_adapter),
        ("type", refcell_type_adapter),
        ("assert", refcell_assert_adapter),
        ("next", refcell_next_adapter),
        ("pairs", refcell_pairs_adapter),
        ("ipairs", refcell_ipairs_adapter),
        ("tostring", refcell_tostring_adapter),
        ("tonumber", refcell_tonumber_adapter),
        ("pcall", refcell_pcall_adapter),
        ("getmetatable", refcell_getmetatable_adapter),
        ("setmetatable", refcell_setmetatable_adapter),
        ("rawget", refcell_rawget_adapter),
        ("rawset", refcell_rawset_adapter),
        ("rawequal", refcell_rawequal_adapter),
    ];
    
    for &(name, function) in name_function_pairs {
        let name_handle = vm.heap().create_string(name)?;
        vm.heap().set_table_field(
            globals, 
            &Value::String(name_handle), 
            &Value::CFunction(function)
        )?;
    }
    
    println!("RefCellVM base library initialized with {} functions", name_function_pairs.len());
    
    // Initialize additional standard libraries
    println!("Initializing math library");
    math::init_math_lib(vm)?;
    
    println!("Initializing string library");
    string::init_string_lib(vm)?;
    
    println!("Initializing table library");
    table::init_table_lib(vm)?;
    
    println!("RefCellVM standard library fully initialized");
    Ok(())
}