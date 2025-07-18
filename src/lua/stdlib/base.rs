//! Base Standard Library Functions
//! 
//! This module implements the core Lua standard library functions
//! that are always available in the global environment.

use crate::lua::error::{LuaError, LuaResult};
use crate::lua::value::{Value, CFunction};
use crate::lua::refcell_vm::RefCellVM;
use crate::lua::refcell_vm::ExecutionContext;

/// Initialize the base library functions
pub fn init_base_lib(vm: &mut RefCellVM) -> LuaResult<()> {
    println!("Initializing base library");
    
    let globals = vm.heap().globals()?;
    
    // Register assert
    register_assert(vm, globals)?;
    
    // Register type
    register_type(vm, globals)?;
    
    // Register pairs/ipairs and next
    register_iteration_functions(vm, globals)?;
    
    // Register tostring/tonumber
    register_conversion_functions(vm, globals)?;
    
    // Register print
    register_print(vm, globals)?;
    
    // Register error
    register_error(vm, globals)?;
    
    // Register pcall/xpcall
    register_protected_calls(vm, globals)?;
    
    // Register select
    register_select(vm, globals)?;
    
    // Register rawget/rawset/rawequal
    register_raw_functions(vm, globals)?;
    
    // Register getmetatable/setmetatable
    register_metatable_functions(vm, globals)?;
    
    Ok(())
}

/// Lua assert function implementation
fn lua_assert(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() == 0 {
        return Err(LuaError::RuntimeError("bad argument #1 to 'assert' (value expected)".to_string()));
    }
    
    let cond = ctx.get_arg(0)?;
    if cond.is_falsey() {
        let msg = if ctx.arg_count() > 1 {
            match ctx.get_arg(1)? {
                Value::String(h) => ctx.get_string_from_handle(h)?,
                v => format!("{:?}", v),
            }
        } else {
            "assertion failed!".to_string()
        };
        
        Err(LuaError::RuntimeError(msg))
    } else {
        // Return all arguments
        for i in 0..ctx.arg_count() {
            ctx.push_result(ctx.get_arg(i)?)?;
        }
        Ok(ctx.arg_count() as i32)
    }
}

fn register_assert(vm: &mut RefCellVM, globals: crate::lua::handle::TableHandle) -> LuaResult<()> {
    let name = vm.heap().create_string("assert")?;
    let func: CFunction = lua_assert;
    
    vm.heap().set_table_field(globals, &Value::String(name), &Value::CFunction(func))
}

/// Lua type function implementation
fn lua_type(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() == 0 {
        return Err(LuaError::RuntimeError("bad argument #1 to 'type' (value expected)".to_string()));
    }
    
    let arg = ctx.get_arg(0)?;
    let type_name = arg.type_name();
    let type_str = ctx.create_string(type_name)?;
    ctx.push_result(Value::String(type_str))?;
    Ok(1)
}

fn register_type(vm: &mut RefCellVM, globals: crate::lua::handle::TableHandle) -> LuaResult<()> {
    let name = vm.heap().create_string("type")?;
    let func: CFunction = lua_type;
    
    vm.heap().set_table_field(globals, &Value::String(name), &Value::CFunction(func))
}

/// Lua next function implementation
fn lua_next(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() == 0 {
        return Err(LuaError::RuntimeError("bad argument #1 to 'next' (table expected)".to_string()));
    }
    
    let table_arg = ctx.get_arg(0)?;
    let key = if ctx.arg_count() > 1 {
        ctx.get_arg(1)?
    } else {
        Value::Nil
    };
    
    match table_arg {
        Value::Table(handle) => {
            match ctx.table_next(handle, key)? {
                Some((k, v)) => {
                    ctx.push_result(k)?;
                    ctx.push_result(v)?;
                    Ok(2)
                }
                None => {
                    ctx.push_result(Value::Nil)?;
                    Ok(1)
                }
            }
        }
        _ => Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: table_arg.type_name().to_string(),
        })
    }
}

/// Lua pairs function implementation
fn lua_pairs(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() == 0 {
        return Err(LuaError::RuntimeError("bad argument #1 to 'pairs' (table expected)".to_string()));
    }
    
    let table = ctx.get_arg(0)?;
    match table {
        Value::Table(_) => {
            // Return next, table, nil
            let next_func = ctx.globals_get("next")?;
            ctx.push_result(next_func)?;
            ctx.push_result(table)?;
            ctx.push_result(Value::Nil)?;
            Ok(3)
        }
        _ => Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: table.type_name().to_string(),
        })
    }
}

/// Lua ipairs iterator function implementation
fn lua_ipairs_iterator(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let table = ctx.get_arg(0)?;
    let index = ctx.get_arg(1)?;
    
    match (table, index) {
        (Value::Table(handle), Value::Number(n)) => {
            let i = (n as i32) + 1;
            let key = Value::Number(i as f64);
            let value = ctx.table_raw_get(handle, key.clone())?;
            
            if value.is_nil() {
                ctx.push_result(Value::Nil)?;
                Ok(1)
            } else {
                ctx.push_result(key)?;
                ctx.push_result(value)?;
                Ok(2)
            }
        }
        _ => Err(LuaError::RuntimeError("invalid ipairs iterator state".to_string()))
    }
}

/// Lua ipairs function implementation
fn lua_ipairs(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() == 0 {
        return Err(LuaError::RuntimeError("bad argument #1 to 'ipairs' (table expected)".to_string()));
    }
    
    let table = ctx.get_arg(0)?;
    match table {
        Value::Table(_) => {
            // Return iterator, table, 0
            let iter = ctx.globals_get("__ipairs_iterator")?;
            ctx.push_result(iter)?;
            ctx.push_result(table)?;
            ctx.push_result(Value::Number(0.0))?;
            Ok(3)
        }
        _ => Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: table.type_name().to_string(),
        })
    }
}

fn register_iteration_functions(vm: &mut RefCellVM, globals: crate::lua::handle::TableHandle) -> LuaResult<()> {
    // Register next
    let next_name = vm.heap().create_string("next")?;
    let next_func: CFunction = lua_next;
    vm.heap().set_table_field(globals, &Value::String(next_name), &Value::CFunction(next_func))?;
    
    // Register pairs
    let pairs_name = vm.heap().create_string("pairs")?;
    let pairs_func: CFunction = lua_pairs;
    vm.heap().set_table_field(globals, &Value::String(pairs_name), &Value::CFunction(pairs_func))?;
    
    // Register ipairs iterator
    let ipairs_iter_name = vm.heap().create_string("__ipairs_iterator")?;
    let ipairs_iter: CFunction = lua_ipairs_iterator;
    vm.heap().set_table_field(globals, &Value::String(ipairs_iter_name), &Value::CFunction(ipairs_iter))?;
    
    // Register ipairs
    let ipairs_name = vm.heap().create_string("ipairs")?;
    let ipairs_func: CFunction = lua_ipairs;
    vm.heap().set_table_field(globals, &Value::String(ipairs_name), &Value::CFunction(ipairs_func))
}

/// Lua tostring function implementation
fn lua_tostring(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() == 0 {
        return Err(LuaError::RuntimeError("bad argument #1 to 'tostring' (value expected)".to_string()));
    }
    
    let arg = ctx.get_arg(0)?;
    
    // Check for __tostring metamethod
    if let Some(method) = ctx.check_metamethod(&arg, "__tostring")? {
        // Call metamethod - for now just use default conversion
        // TODO: Actually call the metamethod
    }
    
    let result = match arg {
        Value::Nil => ctx.create_string("nil")?,
        Value::Boolean(b) => ctx.create_string(if b { "true" } else { "false" })?,
        Value::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 1e14 {
                ctx.create_string(&format!("{:.0}", n))?
            } else {
                ctx.create_string(&n.to_string())?
            }
        }
        Value::String(h) => h,
        Value::Table(_) => ctx.create_string(&format!("table: {:p}", &arg))?,
        Value::Closure(_) => ctx.create_string(&format!("function: {:p}", &arg))?,
        Value::CFunction(_) => ctx.create_string(&format!("function: {:p}", &arg))?,
        _ => ctx.create_string(&format!("{}: {:p}", arg.type_name(), &arg))?,
    };
    
    ctx.push_result(Value::String(result))?;
    Ok(1)
}

/// Lua tonumber function implementation
fn lua_tonumber(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() == 0 {
        return Err(LuaError::RuntimeError("bad argument #1 to 'tonumber' (value expected)".to_string()));
    }
    
    let arg = ctx.get_arg(0)?;
    let base = if ctx.arg_count() > 1 {
        match ctx.get_arg(1)? {
            Value::Number(n) => n as i32,
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: ctx.get_arg(1)?.type_name().to_string(),
            })
        }
    } else {
        10
    };
    
    if base < 2 || base > 36 {
        return Err(LuaError::RuntimeError("base out of range".to_string()));
    }
    
    let result = match arg {
        Value::Number(n) => Value::Number(n),
        Value::String(h) => {
            let s = ctx.get_string_from_handle(h)?;
            if base == 10 {
                match s.trim().parse::<f64>() {
                    Ok(n) => Value::Number(n),
                    Err(_) => Value::Nil,
                }
            } else {
                // For non-base-10, try to parse as integer
                match i64::from_str_radix(s.trim(), base as u32) {
                    Ok(n) => Value::Number(n as f64),
                    Err(_) => Value::Nil,
                }
            }
        }
        _ => Value::Nil,
    };
    
    ctx.push_result(result)?;
    Ok(1)
}

fn register_conversion_functions(vm: &mut RefCellVM, globals: crate::lua::handle::TableHandle) -> LuaResult<()> {
    // tostring
    let tostring_name = vm.heap().create_string("tostring")?;
    let tostring_func: CFunction = lua_tostring;
    vm.heap().set_table_field(globals, &Value::String(tostring_name), &Value::CFunction(tostring_func))?;
    
    // tonumber
    let tonumber_name = vm.heap().create_string("tonumber")?;
    let tonumber_func: CFunction = lua_tonumber;
    vm.heap().set_table_field(globals, &Value::String(tonumber_name), &Value::CFunction(tonumber_func))
}

/// Lua print function implementation
fn lua_print(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let mut output = String::new();
    
    for i in 0..ctx.arg_count() {
        if i > 0 {
            output.push('\t');
        }
        
        let arg = ctx.get_arg(i)?;
        let s = match arg {
            Value::String(h) => ctx.get_string_from_handle(h)?,
            _ => {
                // Call tostring on the value
                let tostring = ctx.globals_get("tostring")?;
                match tostring {
                    Value::CFunction(_) => {
                        // For now, use default conversion
                        format!("{:?}", arg)
                    }
                    _ => format!("{:?}", arg),
                }
            }
        };
        
        output.push_str(&s);
    }
    
    println!("{}", output);
    Ok(0)
}

fn register_print(vm: &mut RefCellVM, globals: crate::lua::handle::TableHandle) -> LuaResult<()> {
    let name = vm.heap().create_string("print")?;
    let func: CFunction = lua_print;
    
    vm.heap().set_table_field(globals, &Value::String(name), &Value::CFunction(func))
}

/// Lua error function implementation
fn lua_error(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let msg = if ctx.arg_count() > 0 {
        match ctx.get_arg(0)? {
            Value::String(h) => ctx.get_string_from_handle(h)?,
            v => format!("{:?}", v),
        }
    } else {
        String::new()
    };
    
    Err(LuaError::RuntimeError(msg))
}

fn register_error(vm: &mut RefCellVM, globals: crate::lua::handle::TableHandle) -> LuaResult<()> {
    let name = vm.heap().create_string("error")?;
    let func: CFunction = lua_error;
    
    vm.heap().set_table_field(globals, &Value::String(name), &Value::CFunction(func))
}

/// Lua pcall function implementation (placeholder)
fn lua_pcall(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    // For now, just return false and error message
    ctx.push_result(Value::Boolean(false))?;
    let err_msg = ctx.create_string("pcall not implemented")?;
    ctx.push_result(Value::String(err_msg))?;
    Ok(2)
}

/// Lua xpcall function implementation (placeholder)
fn lua_xpcall(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    ctx.push_result(Value::Boolean(false))?;
    let err_msg = ctx.create_string("xpcall not implemented")?;
    ctx.push_result(Value::String(err_msg))?;
    Ok(2)
}

fn register_protected_calls(vm: &mut RefCellVM, globals: crate::lua::handle::TableHandle) -> LuaResult<()> {
    // pcall
    let pcall_name = vm.heap().create_string("pcall")?;
    let pcall_func: CFunction = lua_pcall;
    vm.heap().set_table_field(globals, &Value::String(pcall_name), &Value::CFunction(pcall_func))?;
    
    // xpcall
    let xpcall_name = vm.heap().create_string("xpcall")?;
    let xpcall_func: CFunction = lua_xpcall;
    vm.heap().set_table_field(globals, &Value::String(xpcall_name), &Value::CFunction(xpcall_func))
}

/// Lua select function implementation
fn lua_select(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() == 0 {
        return Err(LuaError::RuntimeError("bad argument #1 to 'select' (number expected)".to_string()));
    }
    
    let index = ctx.get_arg(0)?;
    match index {
        Value::String(h) => {
            let s = ctx.get_string_from_handle(h)?;
            if s == "#" {
                // Return count of remaining arguments
                ctx.push_result(Value::Number((ctx.arg_count() - 1) as f64))?;
                Ok(1)
            } else {
                Err(LuaError::RuntimeError("bad argument #1 to 'select' (number expected, got string)".to_string()))
            }
        }
        Value::Number(n) => {
            let idx = n as i32;
            if idx < 1 {
                return Err(LuaError::RuntimeError("bad argument #1 to 'select' (index out of range)".to_string()));
            }
            
            // Return all arguments starting from index
            let start = idx as usize;
            if start >= ctx.arg_count() {
                Ok(0)
            } else {
                for i in start..ctx.arg_count() {
                    ctx.push_result(ctx.get_arg(i)?)?;
                }
                Ok((ctx.arg_count() - start) as i32)
            }
        }
        _ => Err(LuaError::RuntimeError("bad argument #1 to 'select' (number expected)".to_string()))
    }
}

fn register_select(vm: &mut RefCellVM, globals: crate::lua::handle::TableHandle) -> LuaResult<()> {
    let name = vm.heap().create_string("select")?;
    let func: CFunction = lua_select;
    
    vm.heap().set_table_field(globals, &Value::String(name), &Value::CFunction(func))
}

/// Lua rawget function implementation
fn lua_rawget(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 2 {
        return Err(LuaError::RuntimeError("rawget expects 2 arguments".to_string()));
    }
    
    let table = ctx.get_arg(0)?;
    let key = ctx.get_arg(1)?;
    
    match table {
        Value::Table(handle) => {
            let value = ctx.table_raw_get(handle, key)?;
            ctx.push_result(value)?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: table.type_name().to_string(),
        })
    }
}

/// Lua rawset function implementation
fn lua_rawset(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 3 {
        return Err(LuaError::RuntimeError("rawset expects 3 arguments".to_string()));
    }
    
    let table = ctx.get_arg(0)?;
    let key = ctx.get_arg(1)?;
    let value = ctx.get_arg(2)?;
    
    match table {
        Value::Table(handle) => {
            ctx.table_raw_set(handle, key, value)?;
            ctx.push_result(table)?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: table.type_name().to_string(),
        })
    }
}

/// Lua rawequal function implementation
fn lua_rawequal(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 2 {
        return Err(LuaError::RuntimeError("rawequal expects 2 arguments".to_string()));
    }
    
    let a = ctx.get_arg(0)?;
    let b = ctx.get_arg(1)?;
    
    // Raw equality check
    let equal = match (a, b) {
        (Value::Nil, Value::Nil) => true,
        (Value::Boolean(a), Value::Boolean(b)) => a == b,
        (Value::Number(a), Value::Number(b)) => a == b,
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Table(a), Value::Table(b)) => a == b,
        (Value::Closure(a), Value::Closure(b)) => a == b,
        _ => false,
    };
    
    ctx.push_result(Value::Boolean(equal))?;
    Ok(1)
}

fn register_raw_functions(vm: &mut RefCellVM, globals: crate::lua::handle::TableHandle) -> LuaResult<()> {
    // rawget
    let rawget_name = vm.heap().create_string("rawget")?;
    let rawget_func: CFunction = lua_rawget;
    vm.heap().set_table_field(globals, &Value::String(rawget_name), &Value::CFunction(rawget_func))?;
    
    // rawset
    let rawset_name = vm.heap().create_string("rawset")?;
    let rawset_func: CFunction = lua_rawset;
    vm.heap().set_table_field(globals, &Value::String(rawset_name), &Value::CFunction(rawset_func))?;
    
    // rawequal
    let rawequal_name = vm.heap().create_string("rawequal")?;
    let rawequal_func: CFunction = lua_rawequal;
    vm.heap().set_table_field(globals, &Value::String(rawequal_name), &Value::CFunction(rawequal_func))
}

/// Lua getmetatable function implementation
fn lua_getmetatable(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() == 0 {
        return Err(LuaError::RuntimeError("bad argument #1 to 'getmetatable' (value expected)".to_string()));
    }
    
    let arg = ctx.get_arg(0)?;
    match arg {
        Value::Table(handle) => {
            match ctx.get_metatable(handle)? {
                Some(mt) => ctx.push_result(Value::Table(mt))?,
                None => ctx.push_result(Value::Nil)?,
            }
            Ok(1)
        }
        _ => {
            // Other types don't have metatables yet
            ctx.push_result(Value::Nil)?;
            Ok(1)
        }
    }
}

/// Lua setmetatable function implementation
fn lua_setmetatable(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 2 {
        return Err(LuaError::RuntimeError("setmetatable expects 2 arguments".to_string()));
    }
    
    let table = ctx.get_arg(0)?;
    let metatable = ctx.get_arg(1)?;
    
    match table {
        Value::Table(handle) => {
            let mt = match metatable {
                Value::Table(mt) => Some(mt),
                Value::Nil => None,
                _ => return Err(LuaError::TypeError {
                    expected: "nil or table".to_string(),
                    got: metatable.type_name().to_string(),
                })
            };
            
            ctx.set_metatable(handle, mt)?;
            ctx.push_result(table)?;
            Ok(1)
        }
        _ => Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: table.type_name().to_string(),
        })
    }
}

fn register_metatable_functions(vm: &mut RefCellVM, globals: crate::lua::handle::TableHandle) -> LuaResult<()> {
    // getmetatable
    let getmetatable_name = vm.heap().create_string("getmetatable")?;
    let getmetatable_func: CFunction = lua_getmetatable;
    vm.heap().set_table_field(globals, &Value::String(getmetatable_name), &Value::CFunction(getmetatable_func))?;
    
    // setmetatable
    let setmetatable_name = vm.heap().create_string("setmetatable")?;
    let setmetatable_func: CFunction = lua_setmetatable;
    vm.heap().set_table_field(globals, &Value::String(setmetatable_name), &Value::CFunction(setmetatable_func))
}