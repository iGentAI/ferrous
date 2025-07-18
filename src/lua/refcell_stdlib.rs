//! RefCell-based Standard Library Implementation
//! 
//! This module provides the standard library functions for the RefCellVM implementation.

use super::error::{LuaError, LuaResult};
use super::handle::{StringHandle, TableHandle, ClosureHandle};
use super::value::{Value, CFunction};
use super::refcell_vm::{RefCellVM, ExecutionContext};

/// Print function adapter matching CFunction signature
pub fn refcell_print_adapter(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let arg_count = ctx.arg_count();
    
    // Collect all arguments first to build the output
    let mut parts = Vec::with_capacity(arg_count);
    for i in 0..arg_count {
        let val = ctx.get_arg(i)?;
        
        // Convert value to string representation
        let s = match val {
            Value::String(h) => ctx.get_string_from_handle(h)?,
            Value::Nil => "nil".to_string(),
            Value::Boolean(b) => b.to_string(),
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
            _ => val.type_name().to_string(),
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
    if value.is_falsey() {
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
pub fn refcell_pairs_adapter(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("bad argument #1 to 'pairs' (table expected)".to_string()));
    }
    
    let table = ctx.get_arg(0)?;
    if !table.is_table() {
        return Err(LuaError::RuntimeError("bad argument #1 to 'pairs' (table expected)".to_string()));
    }
    
    // Return: iterator function, table, nil (initial key)
    ctx.push_result(Value::CFunction(pairs_iterator))?;
    ctx.push_result(table)?;
    ctx.push_result(Value::Nil)?;
    
    Ok(3)
}

/// IPairs iterator function
fn ipairs_iterator(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    // ipairs iterator takes (table, index) and returns (nextindex, value)
    if ctx.arg_count() < 2 {
        return Err(LuaError::RuntimeError("ipairs iterator expects 2 arguments".to_string()));
    }
    
    let table = ctx.get_arg(0)?;
    let index = match ctx.get_arg(1)? {
        Value::Number(n) => n as i32,
        _ => return Err(LuaError::RuntimeError("ipairs iterator expects numeric index".to_string())),
    };
    
    // Try to get the next array element
    let next_index = index + 1;
    if let Value::Table(h) = table {
        // Get reference to table and check array bounds
        match ctx.get_table_field_by_int(h, next_index as isize) {
            Ok(value) if !value.is_nil() => {
                // Return next index and value
                ctx.push_result(Value::Number(next_index as f64))?;
                ctx.push_result(value)?;
                Ok(2)
            },
            _ => {
                // End of array part
                ctx.push_result(Value::Nil)?;
                Ok(1)
            }
        }
    } else {
        ctx.push_result(Value::Nil)?;
        Ok(1)
    }
}

/// IPairs function adapter matching CFunction signature
pub fn refcell_ipairs_adapter(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("bad argument #1 to 'ipairs' (table expected)".to_string()));
    }
    
    let table = ctx.get_arg(0)?;
    if !table.is_table() {
        return Err(LuaError::RuntimeError("bad argument #1 to 'ipairs' (table expected)".to_string()));
    }
    
    // Return: iterator function, table, 0 (initial index)
    ctx.push_result(Value::CFunction(ipairs_iterator))?;
    ctx.push_result(table)?;
    ctx.push_result(Value::Number(0.0))?;
    
    Ok(3)
}

/// Tostring function adapter matching CFunction signature
pub fn refcell_tostring_adapter(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::RuntimeError("bad argument #1 to 'tostring' (value expected)".to_string()));
    }
    
    let arg = ctx.get_arg(0)?;
    
    // Convert value to string
    let result = match arg {
        Value::Nil => ctx.create_string("nil")?,
        Value::Boolean(b) => ctx.create_string(if b { "true" } else { "false" })?,
        Value::Number(n) => {
            // Format numbers like Lua does
            let s = if n.fract() == 0.0 && n.abs() < 1e14 {
                format!("{:.0}", n)
            } else {
                n.to_string()
            };
            ctx.create_string(&s)?
        },
        Value::String(h) => h, // Already a string
        Value::Table(h) => {
            // Format as "table: 0x..." with handle format
            let s = format!("table: {:?}", h);
            ctx.create_string(&s)?
        },
        Value::Closure(h) => {
            let s = format!("function: {:?}", h);
            ctx.create_string(&s)?
        },
        Value::CFunction(f) => {
            let s = format!("function: {:p}", f as *const ());
            ctx.create_string(&s)?
        },
        _ => {
            let s = format!("{}: {:?}", arg.type_name(), arg);
            ctx.create_string(&s)?
        },
    };
    
    ctx.push_result(Value::String(result))?;
    
    Ok(1) // One result
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
        ("pairs", refcell_pairs_adapter),
        ("ipairs", refcell_ipairs_adapter),
        ("tostring", refcell_tostring_adapter),
        ("tonumber", refcell_tonumber_adapter),
    ];
    
    for &(name, function) in name_function_pairs {
        let name_handle = vm.heap().create_string(name)?;
        vm.heap().set_table_field(
            globals, 
            &Value::String(name_handle), 
            &Value::CFunction(function)
        )?;
    }
    
    println!("RefCellVM standard library initialized with {} functions", name_function_pairs.len());
    Ok(())
}