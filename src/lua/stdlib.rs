//! Lua Standard Library Implementation
//!
//! Provides implementations of the core Lua standard library functions
//! including math, string, table, and base functions.

use std::fmt::Write;
use std::cmp::Ordering;

use super::error::{LuaError, Result};
use super::value::{Value, StringHandle, TableHandle, CFunction};
use super::vm::ExecutionContext;
use super::heap::LuaHeap;

impl<'a> ExecutionContext<'a> {
    /// Push a value to the thread stack without borrow checker issues
    pub fn push_thread_stack(&mut self, value: Value) -> Result<()> {
        // Get the current thread handle first to avoid borrowing issues
        let current_thread = self.vm.current_thread;
        
        // Then use it to push to the stack
        self.heap_mut().push_thread_stack(current_thread, value)
    }
    
    /// Improved version of push_result that uses push_thread_stack internally
    pub fn push_result(&mut self, value: Value) -> Result<()> {
        self.push_thread_stack(value)
    }
}

/// Helper function to create a standard library table
pub fn create_stdlib_table(vm: &mut super::vm::LuaVM, name: &str) -> Result<TableHandle> {
    // Create the table
    let table = vm.create_table()?;
    
    // Add it to the global environment
    let globals = vm.globals();
    let name_handle = vm.create_string(name)?;
    vm.set_table(globals, Value::String(name_handle), Value::Table(table))?;
    
    Ok(table)
}

/// Register all standard library functions
pub fn register_stdlib(vm: &mut super::vm::LuaVM) -> Result<()> {
    // Register base library
    register_base_lib(vm)?;
    
    // Register string library
    register_string_lib(vm)?;
    
    // Register table library
    register_table_lib(vm)?;
    
    // Register math library
    register_math_lib(vm)?;
    
    Ok(())
}

/// Check the number of arguments
fn check_arg_count(ctx: &ExecutionContext, min: usize, max: Option<usize>) -> Result<()> {
    let count = ctx.get_arg_count();
    
    if count < min {
        Err(LuaError::ArgError(0, format!("expected at least {} arguments, got {}", min, count)))
    } else if let Some(max_count) = max {
        if count > max_count {
            Err(LuaError::ArgError(0, format!("expected at most {} arguments, got {}", max_count, count)))
        } else {
            Ok(())
        }
    } else {
        Ok(())
    }
}

/// Get a number argument
fn get_number_arg(ctx: &ExecutionContext, index: usize) -> Result<f64> {
    let value = ctx.get_arg(index)?;
    
    match value {
        Value::Number(n) => Ok(n),
        Value::String(h) => {
            // Try to convert string to number
            let string = ctx.heap().get_string_bytes(h)?;
            let str_value = std::str::from_utf8(string).map_err(|_| LuaError::InvalidEncoding)?;
            
            match str_value.parse::<f64>() {
                Ok(n) => Ok(n),
                Err(_) => Err(LuaError::ArgError(index, format!("number expected, got string"))),
            }
        },
        _ => Err(LuaError::ArgError(index, format!("number expected, got {}", value.type_name()))),
    }
}

/// Get a string argument
fn get_string_arg(ctx: &ExecutionContext, index: usize) -> Result<String> {
    let value = ctx.get_arg(index)?;
    
    match value {
        Value::String(h) => {
            let string = ctx.heap().get_string_bytes(h)?;
            let str_value = std::str::from_utf8(string).map_err(|_| LuaError::InvalidEncoding)?;
            Ok(str_value.to_string())
        },
        Value::Number(n) => {
            Ok(n.to_string())
        },
        _ => Err(LuaError::ArgError(index, format!("string expected, got {}", value.type_name()))),
    }
}

/// Get a table argument
fn get_table_arg(ctx: &ExecutionContext, index: usize) -> Result<TableHandle> {
    let value = ctx.get_arg(index)?;
    
    match value {
        Value::Table(h) => Ok(h),
        _ => Err(LuaError::ArgError(index, format!("table expected, got {}", value.type_name()))),
    }
}

/// Check argument type
fn check_arg(ctx: &ExecutionContext, index: usize, expected_type: &str) -> Result<()> {
    if index >= ctx.get_arg_count() {
        return Err(LuaError::ArgError(index, format!("argument #{} expected", index + 1)));
    }
    
    let value = ctx.get_arg(index)?;
    
    match (expected_type, &value) {
        ("number", Value::Number(_)) => Ok(()),
        ("string", Value::String(_)) => Ok(()),
        ("table", Value::Table(_)) => Ok(()),
        ("function", Value::Closure(_)) | ("function", Value::CFunction(_)) => Ok(()),
        ("thread", Value::Thread(_)) => Ok(()),
        ("userdata", Value::UserData(_)) => Ok(()),
        ("boolean", Value::Boolean(_)) => Ok(()),
        ("nil", Value::Nil) => Ok(()),
        _ => Err(LuaError::ArgError(index, format!("{} expected, got {}", expected_type, value.type_name()))),
    }
}

//
// BASE LIBRARY
//

/// Register the base library functions
fn register_base_lib(vm: &mut super::vm::LuaVM) -> Result<()> {
    // Most functions go in the global table
    let globals = vm.globals();
    
    // Register functions
    let functions = [
        ("print", base_print as CFunction),
        ("type", base_type as CFunction),
        ("tostring", base_tostring as CFunction),
        ("tonumber", base_tonumber as CFunction),
        ("assert", base_assert as CFunction),
        ("error", base_error as CFunction),
        ("pcall", base_pcall as CFunction),
        ("pairs", base_pairs as CFunction),
        ("ipairs", base_ipairs as CFunction),
        ("next", base_next as CFunction),
        ("setmetatable", base_setmetatable as CFunction),
        ("getmetatable", base_getmetatable as CFunction),
        ("rawget", base_rawget as CFunction),
        ("rawset", base_rawset as CFunction),
        ("rawequal", base_rawequal as CFunction),
        ("select", base_select as CFunction),
        ("_G", base_get_global_table as CFunction),
    ];
    
    for (name, func) in &functions {
        let name_handle = vm.create_string(name)?;
        vm.set_table(globals, Value::String(name_handle), Value::CFunction(*func))?;
    }
    
    Ok(())
}

/// Implementation of print()
fn base_print(ctx: &mut ExecutionContext) -> Result<i32> {
    let count = ctx.get_arg_count();
    
    for i in 0..count {
        if i > 0 {
            print!("\t");
        }
        
        let value = ctx.get_arg(i)?;
        
        match value {
            Value::String(h) => {
                let bytes = ctx.heap().get_string_bytes(h)?;
                let s = std::str::from_utf8(bytes).map_err(|_| LuaError::InvalidEncoding)?;
                print!("{}", s);
            },
            Value::Number(n) => {
                print!("{}", n);
            },
            Value::Boolean(b) => {
                print!("{}", b);
            },
            Value::Nil => {
                print!("nil");
            },
            Value::Table(h) => {
                print!("table: {:?}", h);
            },
            Value::Closure(h) => {
                print!("function: {:?}", h);
            },
            Value::Thread(h) => {
                print!("thread: {:?}", h);
            },
            Value::CFunction(_) => {
                print!("function: C");
            },
            Value::UserData(h) => {
                print!("userdata: {:?}", h);
            },
        }
    }
    
    println!(); // End with newline
    
    Ok(0) // No return values
}

/// Implementation of type()
fn base_type(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let value = ctx.get_arg(0)?;
    let type_name = value.type_name();
    
    let type_str_handle = ctx.vm.create_string(type_name)?;
    ctx.push_result(Value::String(type_str_handle))?;
    
    Ok(1) // One return value
}

/// Implementation of tostring()
fn base_tostring(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let value = ctx.get_arg(0)?;
    
    // Check for __tostring metamethod if it's a table
    if let Value::Table(h) = value {
        let metatable_opt = ctx.heap().get_metatable(h)?;
        
        if let Some(metatable) = metatable_opt {
            let tostring_key = ctx.vm.create_string("__tostring")?;
            let metamethod = ctx.heap().get_table_field(metatable, &Value::String(tostring_key))?;
            
            if let Value::Closure(closure) = metamethod {
                // Call metamethod
                let result = ctx.vm.execute_function(closure, &[value])?;
                
                // Make sure result is a string
                if let Value::String(_) = result {
                    ctx.push_result(result)?;
                    return Ok(1);
                }
                
                return Err(LuaError::TypeError("__tostring must return a string".to_string()));
            }
        }
    }
    
    // No metamethod or not a table, use default string representation
    let str_value = match value {
        Value::Nil => "nil".to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(h) => {
            let bytes = ctx.heap().get_string_bytes(h)?;
            std::str::from_utf8(bytes).map_err(|_| LuaError::InvalidEncoding)?.to_string()
        },
        Value::Table(h) => format!("table: {:?}", h),
        Value::Closure(h) => format!("function: {:?}", h),
        Value::Thread(h) => format!("thread: {:?}", h),
        Value::CFunction(_) => "function: C".to_string(),
        Value::UserData(h) => format!("userdata: {:?}", h),
    };
    
    let str_handle = ctx.vm.create_string(&str_value)?;
    ctx.push_result(Value::String(str_handle))?;
    
    Ok(1) // One return value
}

/// Implementation of tonumber()
fn base_tonumber(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(2))?;
    
    let value = ctx.get_arg(0)?;
    
    // Get optional base
    let base = if ctx.get_arg_count() > 1 {
        let base_value = ctx.get_arg(1)?;
        
        if let Value::Number(n) = base_value {
            Some(n as i32)
        } else {
            return Err(LuaError::ArgError(1, format!("number expected for base, got {}", base_value.type_name())));
        }
    } else {
        None
    };
    
    // Convert to number
    match value {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n))?;
        },
        Value::String(h) => {
            let bytes = ctx.heap().get_string_bytes(h)?;
            let s = std::str::from_utf8(bytes).map_err(|_| LuaError::InvalidEncoding)?;
            
            if let Some(base) = base {
                if base < 2 || base > 36 {
                    return Err(LuaError::ArgError(1, format!("base out of range (2-36)")));
                }
                
                // Parse with custom base
                match i64::from_str_radix(s.trim(), base as u32) {
                    Ok(n) => ctx.push_result(Value::Number(n as f64))?,
                    Err(_) => ctx.push_result(Value::Nil)?,
                }
            } else {
                // Parse as decimal or float
                match s.parse::<f64>() {
                    Ok(n) => ctx.push_result(Value::Number(n))?,
                    Err(_) => ctx.push_result(Value::Nil)?,
                }
            }
        },
        _ => {
            ctx.push_result(Value::Nil)?;
        },
    }
    
    Ok(1) // One return value
}

/// Implementation of assert()
fn base_assert(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, None)?;
    
    let value = ctx.get_arg(0)?;
    
    // Check if condition is false
    if matches!(value, Value::Nil | Value::Boolean(false)) {
        if ctx.get_arg_count() > 1 {
            // Use provided error message
            let msg = get_string_arg(ctx, 1)?;
            return Err(LuaError::RuntimeError(msg));
        } else {
            // Default error message
            return Err(LuaError::RuntimeError("assertion failed!".to_string()));
        }
    }
    
    // Return all arguments
    for i in 0..ctx.get_arg_count() {
        let arg = ctx.get_arg(i)?;
        ctx.push_result(arg)?;
    }
    
    Ok(ctx.get_arg_count() as i32)
}

/// Implementation of error()
fn base_error(ctx: &mut ExecutionContext) -> Result<i32> {
    let message = if ctx.get_arg_count() > 0 {
        get_string_arg(ctx, 0)?
    } else {
        "error".to_string()
    };
    
    Err(LuaError::RuntimeError(message))
}

/// Implementation of pcall()
fn base_pcall(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, None)?;
    
    // Get function
    let func = ctx.get_arg(0)?;
    
    // Get arguments
    let arg_count = ctx.get_arg_count();
    let mut args = Vec::with_capacity(arg_count - 1);
    
    for i in 1..arg_count {
        args.push(ctx.get_arg(i)?);
    }
    
    // Call function in protected mode
    let result = match func {
        Value::Closure(closure) => {
            match ctx.vm.execute_function(closure, &args) {
                Ok(result) => {
                    // Success
                    ctx.push_result(Value::Boolean(true))?;
                    ctx.push_result(result)?;
                    2 // status + result
                },
                Err(e) => {
                    // Error
                    ctx.push_result(Value::Boolean(false))?;
                    
                    // Convert error to string
                    let error_str = format!("{}", e);
                    let str_handle = ctx.vm.create_string(&error_str)?;
                    ctx.push_result(Value::String(str_handle))?;
                    
                    2 // status + error message
                }
            }
        },
        Value::CFunction(cfunc) => {
            // Create a sub context
            let stack_base = ctx.base + arg_count;
            
            // Push arguments to stack
            for arg in &args {
                ctx.push_thread_stack(arg.clone())?;
            }
            
            // Create context
            let mut subcall_ctx = ExecutionContext {
                vm: ctx.vm,
                base: stack_base,
                arg_count: args.len(),
            };
            
            // Call C function
            match cfunc(&mut subcall_ctx) {
                Ok(ret_count) => {
                    // Success
                    ctx.push_result(Value::Boolean(true))?;
                    
                    // Copy return values
                    for i in 0..ret_count as usize {
                        let value = ctx.vm.heap.get_thread_stack_value(ctx.vm.current_thread, stack_base + i)?;
                        ctx.push_result(value)?;
                    }
                    
                    1 + ret_count // status + results
                },
                Err(e) => {
                    // Error
                    ctx.push_result(Value::Boolean(false))?;
                    
                    // Convert error to string
                    let error_str = format!("{}", e);
                    let str_handle = ctx.vm.create_string(&error_str)?;
                    ctx.push_result(Value::String(str_handle))?;
                    
                    2 // status + error message
                }
            }
        },
        _ => {
            return Err(LuaError::ArgError(0, format!("function expected, got {}", func.type_name())));
        }
    };
    
    Ok(result)
}

fn base_pairs(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    // Get table
    let table = get_table_arg(ctx, 0)?;
    
    // First get the next function
    let next_name = ctx.vm.create_string("next")?;
    let globals = ctx.vm.globals();
    let next_fn = ctx.vm.get_table(globals, Value::String(next_name))?;
    
    // Now push results
    ctx.push_thread_stack(next_fn)?;
    ctx.push_thread_stack(Value::Table(table))?;
    ctx.push_thread_stack(Value::Nil)?;
    
    Ok(3) // Three return values
}

/// Implementation of ipairs()
fn base_ipairs(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    // Get table
    let table = get_table_arg(ctx, 0)?;
    
    // Create iterator function then use the iterator
    let iter_str = r#"
    local function ipairs_iter(t, i)
        i = i + 1
        local v = t[i]
        if v == nil then return nil end
        return i, v
    end
    return ipairs_iter
    "#;
    
    // Compile the iterator function
    let mut compiler = super::compiler::Compiler::new();
    let closure = compiler.compile_and_load(iter_str, &mut ctx.vm.heap)?;
    
    // Execute it to get the iterator function
    let result = ctx.vm.execute_function(closure, &[])?;
    
    // Return the iteration function, table, and 0
    ctx.push_thread_stack(result)?;
    ctx.push_thread_stack(Value::Table(table))?;
    ctx.push_thread_stack(Value::Number(0.0))?;
    
    Ok(3) // 3 return values
}

/// Implementation of next()
fn base_next(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(2))?;
    
    // Get table
    let table = get_table_arg(ctx, 0)?;
    
    // Fetch all data we'll need up front
    let (array_data, map_data) = {
        // Get a copy of the table data to work with
        let table_obj = ctx.heap().get_table(table)?;
        (table_obj.array.clone(), table_obj.hash_map.clone())
    };
    
    // Get current key
    let current_key = if ctx.get_arg_count() > 1 {
        ctx.get_arg(1)?
    } else {
        Value::Nil
    };
    
    // Get next key-value pair
    if current_key == Value::Nil {
        // Start of iteration - return first element
        if !array_data.is_empty() {
            // First array element
            ctx.push_result(Value::Number(1.0))?;
            ctx.push_result(array_data[0].clone())?;
            return Ok(2); // Key and value
        } else if !map_data.is_empty() {
            // First hash element
            let (ref key, ref value) = map_data[0];
            ctx.push_result(key.clone())?;
            ctx.push_result(value.clone())?;
            return Ok(2); // Key and value
        } else {
            // Empty table
            ctx.push_result(Value::Nil)?;
            return Ok(1); // Just nil
        }
    }
    
    // Find current key and return next
    if let Value::Number(n) = current_key {
        if n.fract() == 0.0 && n > 0.0 && (n as usize) < array_data.len() {
            // Array part
            let idx = n as usize;
            ctx.push_result(Value::Number((idx + 1) as f64))?;
            ctx.push_result(array_data[idx].clone())?;
            return Ok(2); // Key and value
        }
    }
    
    // Search hash part
    for (i, (key, value)) in map_data.iter().enumerate() {
        if *key == current_key && i + 1 < map_data.len() {
            // Return next pair
            let (ref next_key, ref next_value) = map_data[i + 1];
            ctx.push_result(next_key.clone())?;
            ctx.push_result(next_value.clone())?;
            return Ok(2); // Key and value
        }
    }
    
    // No more elements
    ctx.push_result(Value::Nil)?;
    Ok(1) // Just nil
}

/// Implementation of setmetatable()
fn base_setmetatable(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 2, Some(2))?;
    
    // Get table
    let table = get_table_arg(ctx, 0)?;
    
    // Get metatable (or nil)
    let metatable = match ctx.get_arg(1)? {
        Value::Table(h) => Some(h),
        Value::Nil => None,
        _ => return Err(LuaError::ArgError(1, format!("table expected, got {}", ctx.get_arg(1)?.type_name()))),
    };
    
    // Set metatable - do this first before further operations
    ctx.heap_mut().set_metatable(table, metatable)?;
    
    // Return the table
    ctx.push_result(Value::Table(table))?;
    
    Ok(1) // One return value
}

/// Implementation of getmetatable()
fn base_getmetatable(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    // Get object
    let value = ctx.get_arg(0)?;
    
    match value {
        Value::Table(h) => {
            // First get metatable
            let metatable_opt = ctx.heap().get_metatable(h)?;
            
            if let Some(metatable) = metatable_opt {
                // First create the metatable key
                let mmt_key = ctx.vm.create_string("__metatable")?;
                
                // Now get the metatable fields with a new scope
                let result_value = {
                    let mt_obj = ctx.heap().get_table(metatable)?;
                    
                    // Look for __metatable field
                    let mut has_metatable_field = false;
                    let mut metatable_field = Value::Nil;
                    
                    for (k, v) in &mt_obj.hash_map {
                        if let Value::String(s) = k {
                            if let Ok(key_bytes) = ctx.heap().get_string_bytes(*s) {
                                if key_bytes == b"__metatable" {
                                    has_metatable_field = true;
                                    metatable_field = v.clone();
                                    break;
                                }
                            }
                        }
                    }
                    
                    if has_metatable_field {
                        metatable_field
                    } else {
                        Value::Table(metatable)
                    }
                };
                
                // Push the result
                ctx.push_result(result_value)?;
            } else {
                // No metatable
                ctx.push_result(Value::Nil)?;
            }
        },
        _ => {
            // Other types don't have metatables in our implementation
            ctx.push_result(Value::Nil)?;
        }
    }
    
    Ok(1) // One return value
}

/// Implementation of rawget()
fn base_rawget(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 2, Some(2))?;
    
    // Get table
    let table = get_table_arg(ctx, 0)?;
    
    // Get key
    let key = ctx.get_arg(1)?;
    
    // Get value directly (no metamethods)
    let table_obj = ctx.heap().get_table(table)?;
    
    if let Value::Number(n) = key {
        if n.fract() == 0.0 && n > 0.0 && n <= table_obj.array.len() as f64 {
            // Array part
            let idx = n as usize - 1; // Lua is 1-indexed
            ctx.push_result(table_obj.array[idx].clone())?;
            return Ok(1);
        }
    }
    
    // Hash part
    for (k, v) in &table_obj.hash_map {
        if *k == key {
            ctx.push_result(v.clone())?;
            return Ok(1);
        }
    }
    
    // Not found
    ctx.push_result(Value::Nil)?;
    
    Ok(1) // One return value
}

/// Implementation of rawset()
fn base_rawset(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 3, Some(3))?;
    
    // Get table
    let table = get_table_arg(ctx, 0)?;
    
    // Get key and value
    let key = ctx.get_arg(1)?;
    let value = ctx.get_arg(2)?;
    
    // Set value directly (no metamethods)
    let mut table_obj = ctx.heap_mut().get_table_mut(table)?;
    
    if let Value::Number(n) = key {
        if n.fract() == 0.0 && n > 0.0 {
            // Array part
            let idx = n as usize - 1; // Lua is 1-indexed
            
            // Resize if necessary
            if idx >= table_obj.array.len() {
                table_obj.array.resize(idx + 1, Value::Nil);
            }
            
            table_obj.array[idx] = value;
            
            // Return table
            ctx.push_result(Value::Table(table))?;
            return Ok(1);
        }
    }
    
    // Hash part
    for (k, v) in table_obj.hash_map.iter_mut() {
        if *k == key {
            *v = value;
            
            // Return table
            ctx.push_result(Value::Table(table))?;
            return Ok(1);
        }
    }
    
    // Not found, add new entry
    table_obj.hash_map.push((key, value));
    
    // Return table
    ctx.push_result(Value::Table(table))?;
    
    Ok(1) // One return value
}

/// Implementation of rawequal()
fn base_rawequal(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 2, Some(2))?;
    
    // Get values
    let v1 = ctx.get_arg(0)?;
    let v2 = ctx.get_arg(1)?;
    
    // Compare (no metamethods)
    ctx.push_result(Value::Boolean(v1 == v2))?;
    
    Ok(1) // One return value
}

/// Implementation of select()
fn base_select(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, None)?;
    
    // Get index
    let index = ctx.get_arg(0)?;
    
    match index {
        Value::String(h) => {
            // Check for "#" - returns count of arguments
            let s = ctx.heap().get_string_value(h)?;
            if s == "#" {
                ctx.push_result(Value::Number((ctx.get_arg_count() - 1) as f64))?;
                return Ok(1);
            }
            
            return Err(LuaError::ArgError(0, "invalid option".to_string()));
        },
        Value::Number(n) => {
            if n.fract() != 0.0 || n <= 0.0 {
                return Err(LuaError::ArgError(0, "index out of range".to_string()));
            }
            
            let idx = n as usize;
            
            // Check if index is valid
            if idx >= ctx.get_arg_count() {
                return Err(LuaError::ArgError(0, "index out of range".to_string()));
            }
            
            // Return selected arguments
            let mut count = 0;
            for i in idx..ctx.get_arg_count() {
                let arg = ctx.get_arg(i)?;
                ctx.push_result(arg)?;
                count += 1;
            }
            
            return Ok(count);
        },
        _ => {
            return Err(LuaError::ArgError(0, "number or '#' expected".to_string()));
        }
    }
}

/// Implementation of _G
fn base_get_global_table(ctx: &mut ExecutionContext) -> Result<i32> {
    // Get globals table
    let globals = ctx.vm.globals();
    ctx.push_result(Value::Table(globals))?;
    
    Ok(1) // One return value
}

//
// STRING LIBRARY
//

/// Register the string library functions
fn register_string_lib(vm: &mut super::vm::LuaVM) -> Result<()> {
    // Create string table
    let string_table = create_stdlib_table(vm, "string")?;
    
    // Register functions
    let functions = [
        ("len", string_len as CFunction),
        ("sub", string_sub as CFunction),
        ("lower", string_lower as CFunction),
        ("upper", string_upper as CFunction),
        ("char", string_char as CFunction),
        ("byte", string_byte as CFunction),
        ("rep", string_rep as CFunction),
        ("reverse", string_reverse as CFunction),
        ("format", string_format as CFunction),
        ("find", string_find as CFunction),
        ("match", string_match as CFunction),
        ("gsub", string_gsub as CFunction),
        ("gmatch", string_gmatch as CFunction),
    ];
    
    for (name, func) in &functions {
        let name_handle = vm.create_string(name)?;
        vm.set_table(string_table, Value::String(name_handle), Value::CFunction(*func))?;
    }
    
    Ok(())
}

/// Implementation of string.len()
fn string_len(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let s = get_string_arg(ctx, 0)?;
    
    ctx.push_result(Value::Number(s.len() as f64))?;
    
    Ok(1) // One return value
}

/// Implementation of string.sub()
fn string_sub(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 2, Some(3))?;
    
    let s = get_string_arg(ctx, 0)?;
    let i = get_number_arg(ctx, 1)? as isize;
    let j = if ctx.get_arg_count() > 2 {
        get_number_arg(ctx, 2)? as isize
    } else {
        -1
    };
    
    // Convert indices
    let len = s.len() as isize;
    let start = if i < 0 {
        (len + i + 1).max(1) as usize - 1
    } else {
        (i - 1).max(0) as usize
    };
    
    let end = if j < 0 {
        (len + j + 1).max(0) as usize
    } else {
        j.min(len) as usize
    };
    
    // Extract substring
    let result = if start < s.len() && start < end {
        s[start..end.min(s.len())].to_string()
    } else {
        "".to_string()
    };
    
    let result_handle = ctx.vm.create_string(&result)?;
    ctx.push_result(Value::String(result_handle))?;
    
    Ok(1) // One return value
}

/// Implementation of string.lower()
fn string_lower(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let s = get_string_arg(ctx, 0)?;
    let result = s.to_lowercase();
    
    let result_handle = ctx.vm.create_string(&result)?;
    ctx.push_result(Value::String(result_handle))?;
    
    Ok(1) // One return value
}

/// Implementation of string.upper()
fn string_upper(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let s = get_string_arg(ctx, 0)?;
    let result = s.to_uppercase();
    
    let result_handle = ctx.vm.create_string(&result)?;
    ctx.push_result(Value::String(result_handle))?;
    
    Ok(1) // One return value
}

/// Implementation of string.char()
fn string_char(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, None)?;
    
    // Convert all arguments to chars
    let mut result = String::new();
    
    for i in 0..ctx.get_arg_count() {
        let n = get_number_arg(ctx, i)? as u32;
        
        if let Some(c) = std::char::from_u32(n) {
            result.push(c);
        } else {
            return Err(LuaError::ArgError(i, format!("invalid value for character code")));
        }
    }
    
    let result_handle = ctx.vm.create_string(&result)?;
    ctx.push_thread_stack(Value::String(result_handle))?;
    
    Ok(1) // One return value
}

/// Implementation of string.byte()
fn string_byte(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(3))?;
    
    let s = get_string_arg(ctx, 0)?;
    let i = if ctx.get_arg_count() > 1 {
        get_number_arg(ctx, 1)? as isize
    } else {
        1
    };
    
    let j = if ctx.get_arg_count() > 2 {
        get_number_arg(ctx, 2)? as isize
    } else {
        i
    };
    
    // Convert indices
    let len = s.len() as isize;
    let start = if i < 0 {
        (len + i + 1).max(1) as usize - 1
    } else {
        (i - 1).max(0) as usize
    };
    
    let end = if j < 0 {
        (len + j + 1).max(0) as usize
    } else {
        j.min(len) as usize
    };
    
    // Get bytes
    let bytes = s.as_bytes();
    let mut count = 0;
    
    for i in start..end.min(bytes.len()) {
        ctx.push_result(Value::Number(bytes[i] as f64))?;
        count += 1;
    }
    
    Ok(count) // Return byte values
}

/// Implementation of string.rep()
fn string_rep(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 2, Some(3))?;
    
    let s = get_string_arg(ctx, 0)?;
    let n = get_number_arg(ctx, 1)? as usize;
    let sep = if ctx.get_arg_count() > 2 {
        get_string_arg(ctx, 2)?
    } else {
        "".to_string()
    };
    
    // Check for reasonable limits to avoid memory issues
    if n > 1000000 || s.len() * n > 1000000 {
        return Err(LuaError::RuntimeError("string size overflow".to_string()));
    }
    
    // Create repeated string
    let mut result = String::with_capacity(s.len() * n + sep.len() * (n - 1));
    
    for i in 0..n {
        if i > 0 {
            result.push_str(&sep);
        }
        result.push_str(&s);
    }
    
    let result_handle = ctx.vm.create_string(&result)?;
    ctx.push_result(Value::String(result_handle))?;
    
    Ok(1) // One return value
}

/// Implementation of string.reverse()
fn string_reverse(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let s = get_string_arg(ctx, 0)?;
    
    // Reverse string
    let result: String = s.chars().rev().collect();
    
    let result_handle = ctx.vm.create_string(&result)?;
    ctx.push_result(Value::String(result_handle))?;
    
    Ok(1) // One return value
}

/// Implementation of string.format()
fn string_format(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, None)?;
    
    let format = get_string_arg(ctx, 0)?;
    
    // Basic format string implementation
    let mut result = String::new();
    let mut chars = format.chars().peekable();
    let mut arg_index = 1;
    
    while let Some(c) = chars.next() {
        if c == '%' {
            if let Some(&next) = chars.peek() {
                if next == '%' {
                    // Literal %
                    result.push('%');
                    chars.next(); // Skip second %
                    continue;
                }
            }
            
            // Format specifier
            let mut spec = String::new();
            let mut flags = String::new();
            
            // Read flags
            while let Some(&next) = chars.peek() {
                if "+-0# ".contains(next) {
                    flags.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            
            // Read width
            let mut width = String::new();
            while let Some(&next) = chars.peek() {
                if next.is_ascii_digit() {
                    width.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            
            // Read precision
            let mut precision = None;
            if let Some(&next) = chars.peek() {
                if next == '.' {
                    chars.next();
                    let mut prec = String::new();
                    while let Some(&next) = chars.peek() {
                        if next.is_ascii_digit() {
                            prec.push(next);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    precision = Some(prec);
                }
            }
            
            // Read specifier
            if let Some(specifier) = chars.next() {
                spec.push(specifier);
                
                // Format argument
                if arg_index >= ctx.get_arg_count() {
                    return Err(LuaError::RuntimeError(format!("bad argument #{} to 'format' (no value)", arg_index)));
                }
                
                let arg = ctx.get_arg(arg_index)?;
                arg_index += 1;
                
                match specifier {
                    'd' | 'i' => {
                        // Integer
                        let n = get_number_arg(ctx, arg_index - 1)? as i64;
                        write!(result, "{}", n).unwrap();
                    },
                    'f' => {
                        // Float
                        let n = get_number_arg(ctx, arg_index - 1)?;
                        if let Some(prec) = precision {
                            let prec = prec.parse::<usize>().unwrap_or(6);
                            write!(result, "{:.*}", prec, n).unwrap();
                        } else {
                            write!(result, "{}", n).unwrap();
                        }
                    },
                    's' => {
                        // String
                        let s = match arg {
                            Value::String(h) => {
                                let bytes = ctx.heap().get_string_bytes(h)?;
                                std::str::from_utf8(bytes).map_err(|_| LuaError::InvalidEncoding)?.to_string()
                            },
                            Value::Number(n) => n.to_string(),
                            Value::Boolean(b) => b.to_string(),
                            Value::Nil => "nil".to_string(),
                            _ => format!("{}", arg.type_name()),
                        };
                        
                        if let Some(prec) = precision {
                            let prec = prec.parse::<usize>().unwrap_or(0);
                            if prec < s.len() {
                                result.push_str(&s[..prec]);
                            } else {
                                result.push_str(&s);
                            }
                        } else {
                            result.push_str(&s);
                        }
                    },
                    'c' => {
                        // Character
                        let n = get_number_arg(ctx, arg_index - 1)? as u32;
                        if let Some(c) = std::char::from_u32(n) {
                            result.push(c);
                        } else {
                            return Err(LuaError::ArgError(arg_index - 1, "invalid value for character code".to_string()));
                        }
                    },
                    'p' => {
                        // Pointer
                        match arg {
                            Value::Table(h) => write!(result, "table: {:?}", h).unwrap(),
                            Value::Closure(h) => write!(result, "function: {:?}", h).unwrap(),
                            Value::Thread(h) => write!(result, "thread: {:?}", h).unwrap(),
                            Value::CFunction(_) => write!(result, "function: C").unwrap(),
                            Value::UserData(h) => write!(result, "userdata: {:?}", h).unwrap(),
                            _ => write!(result, "{}", arg.type_name()).unwrap(),
                        }
                    },
                    _ => {
                        return Err(LuaError::RuntimeError(format!("invalid format specifier '%{}'", specifier)));
                    }
                }
            } else {
                // Missing specifier
                return Err(LuaError::RuntimeError("invalid format (ends with '%')".to_string()));
            }
        } else {
            result.push(c);
        }
    }
    
    let result_handle = ctx.vm.create_string(&result)?;
    ctx.push_result(Value::String(result_handle))?;
    
    Ok(1) // One return value
}

/// Implementation of string.find() - basic version without pattern matching
fn string_find(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 2, Some(4))?;
    
    // Extract all arguments first
    let s = get_string_arg(ctx, 0)?;
    let pattern = get_string_arg(ctx, 1)?;
    
    let init = if ctx.get_arg_count() > 2 {
        get_number_arg(ctx, 2)? as isize
    } else {
        1
    };
    
    let plain = if ctx.get_arg_count() > 3 {
        match ctx.get_arg(3)? {
            Value::Boolean(b) => b,
            _ => false,
        }
    } else {
        false
    };
    
    // Using plain search for now (no pattern matching)
    if plain || true { // Always plain until we implement pattern matching
        // Convert init to index
        let start = if init < 0 {
            (s.len() as isize + init).max(0) as usize
        } else {
            (init - 1).max(0) as usize
        };
        
        // Search substring
        if start < s.len() {
            if let Some(pos) = s[start..].find(&pattern) {
                let start_idx = start + pos + 1; // 1-indexed
                let end_idx = start_idx + pattern.len() - 1; // Inclusive end
                
                ctx.push_result(Value::Number(start_idx as f64))?;
                ctx.push_result(Value::Number(end_idx as f64))?;
                
                return Ok(2); // Two return values
            }
        }
        
        // Not found
        ctx.push_result(Value::Nil)?;
        return Ok(1); // One return value
    }
    
    // Pattern matching not implemented yet
    Err(LuaError::NotImplemented("pattern matching".to_string()))
}

/// Implementation of string.match() - basic version without pattern matching
fn string_match(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 2, Some(3))?;
    
    let s = get_string_arg(ctx, 0)?;
    let pattern = get_string_arg(ctx, 1)?;
    
    // For now, just return substring if found (no pattern matching)
    if let Some(pos) = s.find(&pattern) {
        let result_handle = ctx.vm.create_string(&pattern)?;
        ctx.push_result(Value::String(result_handle))?;
        
        return Ok(1); // One return value
    } else {
        // Not found
        ctx.push_result(Value::Nil)?;
        return Ok(1); // One return value
    }
}

/// Implementation of string.gsub() - basic version without pattern matching
fn string_gsub(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 3, Some(4))?;
    
    let s = get_string_arg(ctx, 0)?;
    let pattern = get_string_arg(ctx, 1)?;
    
    // Get replacement
    let replacement = match ctx.get_arg(2)? {
        Value::String(h) => {
            let bytes = ctx.heap().get_string_bytes(h)?;
            let str_value = std::str::from_utf8(bytes).map_err(|_| LuaError::InvalidEncoding)?;
            Left(str_value.to_string())
        },
        Value::Table(h) => {
            // Table of replacements
            Middle(h)
        },
        Value::Closure(h) => {
            // Function replacement
            Right(h)
        },
        Value::CFunction(f) => {
            // C function replacement
            RightC(f)
        },
        _ => {
            return Err(LuaError::ArgError(2, format!("string/function/table expected, got {}", ctx.get_arg(2)?.type_name())));
        },
    };
    
    let n = if ctx.get_arg_count() > 3 {
        get_number_arg(ctx, 3)? as usize
    } else {
        std::usize::MAX
    };
    
    // Simple gsub with string replacement for now
    if let Left(rep) = replacement {
        let mut result = s.clone();
        let mut count = 0;
        
        // Replace up to n occurrences
        while count < n {
            if let Some(pos) = result.find(&pattern) {
                result.replace_range(pos..pos + pattern.len(), &rep);
                count += 1;
            } else {
                break;
            }
        }
        
        let result_handle = ctx.vm.create_string(&result)?;
        ctx.push_result(Value::String(result_handle))?;
        ctx.push_result(Value::Number(count as f64))?;
        
        return Ok(2); // Two return values (result and count)
    }
    
    // Pattern matching with tables and functions not implemented yet
    Err(LuaError::NotImplemented("gsub with table/function replacement".to_string()))
}

/// Helper for string.gsub to manage different replacement types
enum Either<A, B, C, D> {
    Left(A),
    Middle(B),
    Right(C),
    RightC(D),
}

use Either::{Left, Middle, Right, RightC};

/// Implementation of string.gmatch() - just a stub for now
fn string_gmatch(ctx: &mut ExecutionContext) -> Result<i32> {
    // Pattern matching not implemented yet
    Err(LuaError::NotImplemented("pattern matching".to_string()))
}

//
// TABLE LIBRARY
//

/// Register the table library functions
fn register_table_lib(vm: &mut super::vm::LuaVM) -> Result<()> {
    // Create table table
    let table_table = create_stdlib_table(vm, "table")?;
    
    // Register functions
    let functions = [
        ("concat", table_concat as CFunction),
        ("insert", table_insert as CFunction),
        ("remove", table_remove as CFunction),
        ("sort", table_sort as CFunction),
        ("maxn", table_maxn as CFunction),
        ("unpack", table_unpack as CFunction),
    ];
    
    for (name, func) in &functions {
        let name_handle = vm.create_string(name)?;
        vm.set_table(table_table, Value::String(name_handle), Value::CFunction(*func))?;
    }
    
    Ok(())
}

/// Implementation of table.concat()
fn table_concat(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(4))?;
    
    // Collect all arguments upfront before any mutation
    let table = get_table_arg(ctx, 0)?;
    let sep = if ctx.get_arg_count() > 1 {
        get_string_arg(ctx, 1)?
    } else {
        "".to_string()
    };
    
    let i = if ctx.get_arg_count() > 2 {
        get_number_arg(ctx, 2)? as usize
    } else {
        1
    };
    
    let j = if ctx.get_arg_count() > 3 {
        get_number_arg(ctx, 3)? as usize
    } else {
        // We'll get the length later
        0 // temporary value
    };
    
    // Clone the array elements to avoid borrow checker issues
    let mut elements = Vec::new();
    {
        let table_obj = ctx.heap().get_table(table)?;
        elements = table_obj.array.clone();
    }
    
    let len = elements.len();
    let j_final = if ctx.get_arg_count() > 3 { j } else { len };
    
    // Now build the result string
    let mut result = String::new();
    
    for idx in i..=j_final.min(len) {
        if idx > i {  // Only add separator after first element
            result.push_str(&sep);
        }
        
        if idx <= len {
            let value = &elements[idx - 1];
            
            match value {
                Value::String(h) => {
                    let bytes = ctx.heap().get_string_bytes(*h)?;
                    let str_value = std::str::from_utf8(bytes).map_err(|_| LuaError::InvalidEncoding)?;
                    result.push_str(str_value);
                },
                Value::Number(n) => {
                    result.push_str(&n.to_string());
                },
                _ => {
                    return Err(LuaError::RuntimeError(format!("invalid value at index {} (string or number expected, got {})", 
                                                   idx, value.type_name())));
                }
            }
        }
    }
    
    let result_handle = ctx.vm.create_string(&result)?;
    ctx.push_result(Value::String(result_handle))?;
    
    Ok(1) // One return value
}

/// Implementation of table.insert()
fn table_insert(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 2, Some(3))?;
    
    // Get all arguments up front to avoid borrowing issues
    let table = get_table_arg(ctx, 0)?;
    let array_len = {
        let table_obj = ctx.heap().get_table(table)?;
        table_obj.array.len()
    };

    // Now handle the different cases with all the data we need
    if ctx.get_arg_count() == 2 {
        // Just value - append at end
        let value = ctx.get_arg(1)?;
        
        let mut table_obj = ctx.heap_mut().get_table_mut(table)?;
        table_obj.array.push(value);
    } else {
        // Position and value - get these upfront
        let pos = get_number_arg(ctx, 1)? as usize;
        let value = ctx.get_arg(2)?;
        
        // Check position with data we gathered earlier
        if pos < 1 || pos > array_len + 1 {
            return Err(LuaError::ArgError(1, "position out of bounds".to_string()));
        }
        
        // Now modify the table
        let mut table_obj = ctx.heap_mut().get_table_mut(table)?;
        table_obj.array.insert(pos - 1, value);
    }
    
    Ok(0) // No return values
}

/// Implementation of table.remove()
fn table_remove(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(2))?;
    
    // Get all arguments first
    let table = get_table_arg(ctx, 0)?;
    
    // Get length and collect any other needed data upfront
    let array_len = {
        let table_obj = ctx.heap().get_table(table)?;
        table_obj.array.len()
    };
    
    // Get position
    let pos = if ctx.get_arg_count() > 1 {
        get_number_arg(ctx, 1)? as usize
    } else {
        // Default is last element
        array_len
    };
    
    // Check position once
    if pos < 1 || pos > array_len {
        // Out of bounds, return nil
        ctx.push_result(Value::Nil)?;
        return Ok(1);
    }
    
    // Now modify - we already have all info we need
    let mut table_obj = ctx.heap_mut().get_table_mut(table)?;
    
    // Remove element (1-indexed)
    let removed = table_obj.array.remove(pos - 1);
    
    // Return removed value
    ctx.push_result(removed)?;
    
    Ok(1) // One return value
}

fn table_sort(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(2))?;
    
    // Get table
    let table = get_table_arg(ctx, 0)?;
    
    // Extract the array elements to sort - clone them to avoid borrow checker issues
    let mut elements = {
        let table_obj = ctx.heap().get_table(table)?;
        table_obj.array.clone()
    };
    
    // Get comparator function (if provided)
    let comp_fn = if ctx.get_arg_count() > 1 {
        let comp_val = ctx.get_arg(1)?;
        match comp_val {
            Value::Closure(closure) => Some(closure),
            Value::CFunction(_) => None, // Simplified - we don't support C function comparators
            _ => None,
        }
    } else {
        None
    };
    
    // Sort the elements
    if let Some(closure) = comp_fn {
        // Use a simpler sorting algorithm to avoid borrow checker issues
        // Bubble sort is inefficient but easy to implement with our restrictions
        let n = elements.len();
        for i in 0..n {
            for j in 0..n-i-1 {
                // Call the comparator to check if elements[j+1] < elements[j]
                // If true, elements need to be swapped
                let left = elements[j].clone();
                let right = elements[j+1].clone();
                let result = ctx.vm.execute_function(closure, &[right, left])?;
                
                if let Value::Boolean(true) = result {
                    elements.swap(j, j+1);
                }
            }
        }
    } else {
        // Use a simple sorting algorithm that's compatible with our architecture
        let mut swapped = true;
        while swapped {
            swapped = false;
            for i in 0..elements.len().saturating_sub(1) {
                if compare_values(&elements[i], &elements[i+1]) == Ordering::Greater {
                    elements.swap(i, i+1);
                    swapped = true;
                }
            }
        }
    }
    
    // Update the table with the sorted elements
    {
        let mut table_obj = ctx.heap_mut().get_table_mut(table)?;
        table_obj.array = elements;
    }
    
    Ok(0) // No return values
}

/// Helper function to compare values
fn compare_values(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Number(a_num), Value::Number(b_num)) => {
            a_num.partial_cmp(b_num).unwrap_or(Ordering::Equal)
        },
        (Value::String(a_str), Value::String(b_str)) => {
            // We can't compare string bytes easily here, so just compare handles
            // This isn't ideal for sorting, but works for consistency
            a_str.0.index.cmp(&b_str.0.index)
        },
        // Mixed types - order by type name as fallback
        _ => a.type_name().cmp(b.type_name()),
    }
}

/// Implementation of table.maxn()
fn table_maxn(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    // Get table
    let table = get_table_arg(ctx, 0)?;
    let table_obj = ctx.heap().get_table(table)?;
    
    // Find maximum numerical index in both array and hash parts
    let mut max_n = 0.0;
    
    // Check array part
    if !table_obj.array.is_empty() {
        max_n = table_obj.array.len() as f64;
    }
    
    // Check hash part
    for (key, _) in &table_obj.hash_map {
        if let Value::Number(n) = key {
            if *n > max_n && n.fract() == 0.0 && *n > 0.0 {
                max_n = *n;
            }
        }
    }
    
    ctx.push_result(Value::Number(max_n))?;
    
    Ok(1) // One return value
}

fn table_unpack(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(3))?;
    
    // Get table
    let table = get_table_arg(ctx, 0)?;
    
    // Get table elements as a vector to avoid borrow checker issues
    let elements = {
        let table_obj = ctx.heap().get_table(table)?;
        table_obj.array.clone()
    };
    
    // Get start and end
    let i = if ctx.get_arg_count() > 1 {
        get_number_arg(ctx, 1)? as usize
    } else {
        1
    };
    
    let j = if ctx.get_arg_count() > 2 {
        get_number_arg(ctx, 2)? as usize
    } else {
        elements.len()
    };
    
    // Unpack table elements
    let mut count = 0;
    
    for idx in i..=j {
        if idx <= elements.len() {
            // Get array element (1-indexed)
            ctx.push_result(elements[idx - 1].clone())?;
            count += 1;
        } else {
            // Past end of array, return nil
            ctx.push_result(Value::Nil)?;
            count += 1;
        }
    }
    
    Ok(count) // Return all unpacked values
}

//
// MATH LIBRARY
//

/// Register the math library functions
fn register_math_lib(vm: &mut super::vm::LuaVM) -> Result<()> {
    // Create math table
    let math_table = create_stdlib_table(vm, "math")?;
    
    // Register constants
    let constants = [
        ("pi", std::f64::consts::PI),
        ("huge", std::f64::INFINITY),
    ];
    
    for (name, value) in &constants {
        let name_handle = vm.create_string(name)?;
        vm.set_table(math_table, Value::String(name_handle), Value::Number(*value))?;
    }
    
    // Register functions
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
        ("randomseed", math_randomseed as CFunction),
        ("sin", math_sin as CFunction),
        ("sinh", math_sinh as CFunction),
        ("sqrt", math_sqrt as CFunction),
        ("tan", math_tan as CFunction),
        ("tanh", math_tanh as CFunction),
    ];
    
    for (name, func) in &functions {
        let name_handle = vm.create_string(name)?;
        vm.set_table(math_table, Value::String(name_handle), Value::CFunction(*func))?;
    }
    
    Ok(())
}

/// Implementation of math.abs()
fn math_abs(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let x = get_number_arg(ctx, 0)?;
    ctx.push_result(Value::Number(x.abs()))?;
    
    Ok(1) // One return value
}

/// Implementation of math.acos()
fn math_acos(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let x = get_number_arg(ctx, 0)?;
    ctx.push_result(Value::Number(x.acos()))?;
    
    Ok(1)
}

/// Implementation of math.asin()
fn math_asin(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let x = get_number_arg(ctx, 0)?;
    ctx.push_result(Value::Number(x.asin()))?;
    
    Ok(1)
}

/// Implementation of math.atan()
fn math_atan(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let x = get_number_arg(ctx, 0)?;
    ctx.push_result(Value::Number(x.atan()))?;
    
    Ok(1)
}

/// Implementation of math.atan2()
fn math_atan2(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 2, Some(2))?;
    
    let y = get_number_arg(ctx, 0)?;
    let x = get_number_arg(ctx, 1)?;
    
    ctx.push_result(Value::Number(y.atan2(x)))?;
    
    Ok(1)
}

/// Implementation of math.ceil()
fn math_ceil(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let x = get_number_arg(ctx, 0)?;
    ctx.push_result(Value::Number(x.ceil()))?;
    
    Ok(1)
}

/// Implementation of math.cos()
fn math_cos(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let x = get_number_arg(ctx, 0)?;
    ctx.push_result(Value::Number(x.cos()))?;
    
    Ok(1)
}

/// Implementation of math.cosh()
fn math_cosh(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let x = get_number_arg(ctx, 0)?;
    ctx.push_result(Value::Number(x.cosh()))?;
    
    Ok(1)
}

/// Implementation of math.deg()
fn math_deg(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let x = get_number_arg(ctx, 0)?;
    ctx.push_result(Value::Number(x * 180.0 / std::f64::consts::PI))?;
    
    Ok(1)
}

/// Implementation of math.exp()
fn math_exp(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let x = get_number_arg(ctx, 0)?;
    ctx.push_result(Value::Number(x.exp()))?;
    
    Ok(1)
}

/// Implementation of math.floor()
fn math_floor(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let x = get_number_arg(ctx, 0)?;
    ctx.push_result(Value::Number(x.floor()))?;
    
    Ok(1)
}

/// Implementation of math.fmod()
fn math_fmod(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 2, Some(2))?;
    
    let x = get_number_arg(ctx, 0)?;
    let y = get_number_arg(ctx, 1)?;
    
    let result = x % y;
    ctx.push_result(Value::Number(result))?;
    
    Ok(1)
}

/// Implementation of math.frexp()
fn math_frexp(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let x = get_number_arg(ctx, 0)?;
    
    if x == 0.0 {
        ctx.push_result(Value::Number(0.0))?;
        ctx.push_result(Value::Number(0.0))?;
    } else {
        let exp = x.abs().log2().floor() + 1.0;
        let mantissa = x / 2.0_f64.powf(exp);
        
        ctx.push_result(Value::Number(mantissa))?;
        ctx.push_result(Value::Number(exp))?;
    }
    
    Ok(2) // Two return values
}

/// Implementation of math.ldexp()
fn math_ldexp(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 2, Some(2))?;
    
    let m = get_number_arg(ctx, 0)?;
    let e = get_number_arg(ctx, 1)?;
    
    let result = m * 2.0_f64.powf(e);
    ctx.push_result(Value::Number(result))?;
    
    Ok(1)
}

/// Implementation of math.log()
fn math_log(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let x = get_number_arg(ctx, 0)?;
    ctx.push_result(Value::Number(x.ln()))?;
    
    Ok(1)
}

/// Implementation of math.log10()
fn math_log10(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let x = get_number_arg(ctx, 0)?;
    ctx.push_result(Value::Number(x.log10()))?;
    
    Ok(1)
}

/// Implementation of math.max()
fn math_max(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, None)?;
    
    // Initial max is first argument
    let mut max = get_number_arg(ctx, 0)?;
    
    // Compare with rest of arguments
    for i in 1..ctx.get_arg_count() {
        let x = get_number_arg(ctx, i)?;
        if x > max {
            max = x;
        }
    }
    
    ctx.push_result(Value::Number(max))?;
    
    Ok(1)
}

/// Implementation of math.min()
fn math_min(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, None)?;
    
    // Initial min is first argument
    let mut min = get_number_arg(ctx, 0)?;
    
    // Compare with rest of arguments
    for i in 1..ctx.get_arg_count() {
        let x = get_number_arg(ctx, i)?;
        if x < min {
            min = x;
        }
    }
    
    ctx.push_result(Value::Number(min))?;
    
    Ok(1)
}

/// Implementation of math.modf()
fn math_modf(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let x = get_number_arg(ctx, 0)?;
    
    let int_part = x.trunc();
    let frac_part = x - int_part;
    
    ctx.push_result(Value::Number(int_part))?;
    ctx.push_result(Value::Number(frac_part))?;
    
    Ok(2) // Two return values
}

/// Implementation of math.pow()
fn math_pow(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 2, Some(2))?;
    
    let x = get_number_arg(ctx, 0)?;
    let y = get_number_arg(ctx, 1)?;
    
    ctx.push_result(Value::Number(x.powf(y)))?;
    
    Ok(1)
}

/// Implementation of math.rad()
fn math_rad(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let x = get_number_arg(ctx, 0)?;
    ctx.push_result(Value::Number(x * std::f64::consts::PI / 180.0))?;
    
    Ok(1)
}

/// Implementation of math.random()
fn math_random(ctx: &mut ExecutionContext) -> Result<i32> {
    // Using the simplest possible implementation for now
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .subsec_nanos() as f64 / 1_000_000_000.0;
    
    match ctx.get_arg_count() {
        0 => {
            // Return random float in [0, 1)
            ctx.push_result(Value::Number(now % 1.0))?;
        },
        1 => {
            // Return integer in [1, m]
            let m = get_number_arg(ctx, 0)? as i64;
            let r = ((now * 1000.0) as i64 % m) + 1;
            ctx.push_result(Value::Number(r as f64))?;
        },
        _ => {
            // Return integer in [m, n]
            let m = get_number_arg(ctx, 0)? as i64;
            let n = get_number_arg(ctx, 1)? as i64;
            let r = ((now * 1000.0) as i64 % (n - m + 1)) + m;
            ctx.push_result(Value::Number(r as f64))?;
        }
    }
    
    Ok(1)
}

/// Implementation of math.randomseed()
fn math_randomseed(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    // We don't actually use this in our simplified implementation
    let _ = get_number_arg(ctx, 0)?;
    
    Ok(0) // No return values
}

/// Implementation of math.sin()
fn math_sin(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let x = get_number_arg(ctx, 0)?;
    ctx.push_result(Value::Number(x.sin()))?;
    
    Ok(1)
}

/// Implementation of math.sinh()
fn math_sinh(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let x = get_number_arg(ctx, 0)?;
    ctx.push_result(Value::Number(x.sinh()))?;
    
    Ok(1)
}

/// Implementation of math.sqrt()
fn math_sqrt(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let x = get_number_arg(ctx, 0)?;
    ctx.push_result(Value::Number(x.sqrt()))?;
    
    Ok(1)
}

/// Implementation of math.tan()
fn math_tan(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let x = get_number_arg(ctx, 0)?;
    ctx.push_result(Value::Number(x.tan()))?;
    
    Ok(1)
}

/// Implementation of math.tanh()
fn math_tanh(ctx: &mut ExecutionContext) -> Result<i32> {
    check_arg_count(ctx, 1, Some(1))?;
    
    let x = get_number_arg(ctx, 0)?;
    ctx.push_result(Value::Number(x.tanh()))?;
    
    Ok(1)
}

