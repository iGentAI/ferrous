//! Lua sandbox for secure script execution
//!
//! This module implements the sandboxing restrictions for Lua scripts,
//! ensuring they can't access unsafe features like filesystem, networking,
//! or OS operations.

use crate::lua_new::vm::LuaVM;
use crate::lua_new::value::{Value, CFunction};
use crate::lua_new::error::Result;
use std::collections::HashSet;

/// Lua sandbox configuration
pub struct LuaSandbox {
    /// Enable deterministic mode (no random operations)
    deterministic: bool,
}

impl LuaSandbox {
    /// Create a new sandbox with Redis-compatible settings
    pub fn redis_compatible() -> Self {
        Self {
            deterministic: true,
        }
    }
    
    /// Apply sandbox restrictions to a VM
    pub fn apply(&self, vm: &mut LuaVM) -> Result<()> {
        // Get globals table
        let globals = vm.globals();
        
        // Remove unsafe libraries
        self.remove_libraries(vm, globals)?;
        
        // Register safe stdlib functions
        self.register_safe_stdlib(vm, globals)?;
        
        Ok(())
    }
    
    /// Remove unsafe libraries
    fn remove_libraries(&self, vm: &mut LuaVM, globals: crate::lua_new::value::TableHandle) -> Result<()> {
        let unsafe_libs = ["io", "os", "debug", "package", "coroutine", "dofile", "loadfile"];
        
        for lib in &unsafe_libs {
            let key = vm.heap.create_string(lib);
            vm.heap.get_table_mut(globals)?.set(Value::String(key), Value::Nil);
        }
        
        // Ensure math.random and math.randomseed are removed if deterministic mode is enabled
        if self.deterministic {
            let math_key = vm.heap.create_string("math");
            
            // Use the heap directly instead of the private vm.table_get method
            let math_table_opt = {
                let global_table = vm.heap.get_table(globals)?;
                match global_table.get(&Value::String(math_key)) {
                    Some(&Value::Table(table)) => Some(table),
                    _ => None,
                }
            };
            
            if let Some(math_table) = math_table_opt {
                let random_key = vm.heap.create_string("random");
                let randomseed_key = vm.heap.create_string("randomseed");
                
                vm.heap.get_table_mut(math_table)?.set(Value::String(random_key), Value::Nil);
                vm.heap.get_table_mut(math_table)?.set(Value::String(randomseed_key), Value::Nil);
            }
        }
        
        Ok(())
    }
    
    /// Register safe standard library functions
    fn register_safe_stdlib(&self, vm: &mut LuaVM, globals: crate::lua_new::value::TableHandle) -> Result<()> {
        // Register table library
        let table_lib = vm.heap.alloc_table();
        let table_key = vm.heap.create_string("table");
        
        // Register table.concat function
        let concat_key = vm.heap.create_string("concat");
        vm.heap.get_table_mut(table_lib)?.set(
            Value::String(concat_key),
            Value::CFunction(table_concat)
        );
        
        // Register table.insert function
        let insert_key = vm.heap.create_string("insert");
        vm.heap.get_table_mut(table_lib)?.set(
            Value::String(insert_key),
            Value::CFunction(table_insert)
        );
        
        // Add table library to globals
        vm.heap.get_table_mut(globals)?.set(
            Value::String(table_key),
            Value::Table(table_lib)
        );
        
        // Register string library
        let string_lib = vm.heap.alloc_table();
        let string_key = vm.heap.create_string("string");
        
        // Register string.len function
        let len_key = vm.heap.create_string("len");
        vm.heap.get_table_mut(string_lib)?.set(
            Value::String(len_key),
            Value::CFunction(string_len)
        );
        
        // Add string library to globals
        vm.heap.get_table_mut(globals)?.set(
            Value::String(string_key),
            Value::Table(string_lib)
        );
        
        // Register math library
        let math_lib = vm.heap.alloc_table();
        let math_key = vm.heap.create_string("math");
        
        // Register math.abs function
        let abs_key = vm.heap.create_string("abs");
        vm.heap.get_table_mut(math_lib)?.set(
            Value::String(abs_key),
            Value::CFunction(math_abs)
        );
        
        // Add math library to globals
        vm.heap.get_table_mut(globals)?.set(
            Value::String(math_key),
            Value::Table(math_lib)
        );
        
        // Register global functions
        
        // assert function
        let assert_key = vm.heap.create_string("assert");
        vm.heap.get_table_mut(globals)?.set(
            Value::String(assert_key),
            Value::CFunction(assert_func)
        );
        
        // tonumber function
        let tonumber_key = vm.heap.create_string("tonumber");
        vm.heap.get_table_mut(globals)?.set(
            Value::String(tonumber_key),
            Value::CFunction(tonumber_func)
        );
        
        // tostring function
        let tostring_key = vm.heap.create_string("tostring");
        vm.heap.get_table_mut(globals)?.set(
            Value::String(tostring_key),
            Value::CFunction(tostring_func)
        );
        
        Ok(())
    }
}

/// Implementation of table.concat function
fn table_concat(ctx: &mut crate::lua_new::vm::ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.get_arg_count() == 0 {
        return Err(crate::lua_new::error::LuaError::Runtime(
            "table.concat requires a table argument".to_string()
        ));
    }
    
    // Create empty result string
    let empty = ctx.vm.heap.create_string("");
    ctx.push_result(Value::String(empty))?;
    
    Ok(1) // One return value
}

/// Implementation of table.insert function
fn table_insert(ctx: &mut crate::lua_new::vm::ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.get_arg_count() < 2 {
        return Err(crate::lua_new::error::LuaError::Runtime(
            "table.insert requires at least 2 arguments".to_string()
        ));
    }
    
    // Just return nil (placeholder)
    ctx.push_result(Value::Nil)?;
    
    Ok(0) // No return value
}

/// Implementation of string.len function
fn string_len(ctx: &mut crate::lua_new::vm::ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.get_arg_count() == 0 {
        return Err(crate::lua_new::error::LuaError::Runtime(
            "string.len requires a string argument".to_string()
        ));
    }
    
    // Get string
    let value = ctx.get_arg(0)?;
    let len = match value {
        Value::String(s) => {
            let bytes = ctx.vm.heap.get_string(s)?;
            bytes.len()
        }
        _ => return Err(crate::lua_new::error::LuaError::TypeError(
            "string.len: argument must be a string".to_string()
        )),
    };
    
    // Return length
    ctx.push_result(Value::Number(len as f64))?;
    
    Ok(1) // One return value
}

/// Implementation of math.abs function
fn math_abs(ctx: &mut crate::lua_new::vm::ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.get_arg_count() == 0 {
        return Err(crate::lua_new::error::LuaError::Runtime(
            "math.abs requires a number argument".to_string()
        ));
    }
    
    // Get number
    let n = match ctx.get_arg(0)? {
        Value::Number(n) => n,
        _ => return Err(crate::lua_new::error::LuaError::TypeError(
            "math.abs: argument must be a number".to_string()
        )),
    };
    
    // Return absolute value
    ctx.push_result(Value::Number(n.abs()))?;
    
    Ok(1) // One return value
}

/// Implementation of assert function
fn assert_func(ctx: &mut crate::lua_new::vm::ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.get_arg_count() == 0 {
        return Err(crate::lua_new::error::LuaError::Runtime(
            "assert requires at least 1 argument".to_string()
        ));
    }
    
    // Get value to assert
    let value = ctx.get_arg(0)?;
    
    if !value.to_bool() {
        // Get error message if provided
        let msg = if ctx.get_arg_count() > 1 {
            match ctx.get_arg(1)? {
                Value::String(s) => ctx.vm.heap.get_string_utf8(s)?.to_string(),
                _ => "assertion failed!".to_string(),
            }
        } else {
            "assertion failed!".to_string()
        };
        
        return Err(crate::lua_new::error::LuaError::Runtime(msg));
    }
    
    // Return the value that was asserted
    ctx.push_result(value)?;
    
    Ok(1) // One return value
}

/// Implementation of tonumber function
fn tonumber_func(ctx: &mut crate::lua_new::vm::ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.get_arg_count() == 0 {
        ctx.push_result(Value::Nil)?;
        return Ok(1);
    }
    
    // Get value to convert
    let value = ctx.get_arg(0)?;
    
    match value {
        Value::Number(n) => {
            ctx.push_result(Value::Number(n))?;
        }
        Value::String(s) => {
            let s_str = ctx.vm.heap.get_string_utf8(s)?;
            if let Ok(n) = s_str.parse::<f64>() {
                ctx.push_result(Value::Number(n))?;
            } else {
                ctx.push_result(Value::Nil)?;
            }
        }
        _ => {
            ctx.push_result(Value::Nil)?;
        }
    }
    
    Ok(1) // One return value
}

/// Implementation of tostring function
fn tostring_func(ctx: &mut crate::lua_new::vm::ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.get_arg_count() == 0 {
        let handle = ctx.vm.heap.create_string("");
        ctx.push_result(Value::String(handle))?;
        return Ok(1);
    }
    
    // Get value to convert
    let value = ctx.get_arg(0)?;
    
    let result = match value {
        Value::Nil => "nil",
        Value::Boolean(b) => if b { "true" } else { "false" },
        Value::Number(n) => {
            let num_str = n.to_string();
            let handle = ctx.vm.heap.create_string(&num_str);
            ctx.push_result(Value::String(handle))?;
            return Ok(1);
        }
        Value::String(s) => {
            // Just return the string
            ctx.push_result(Value::String(s))?;
            return Ok(1);
        }
        Value::Table(_) => "table",
        Value::Closure(_) => "function",
        Value::CFunction(_) => "function",
        Value::Thread(_) => "thread",
    };
    
    let handle = ctx.vm.heap.create_string(result);
    ctx.push_result(Value::String(handle))?;
    
    Ok(1) // One return value
}