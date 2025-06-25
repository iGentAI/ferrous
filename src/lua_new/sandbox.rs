//! Lua sandbox for secure script execution
//!
//! This module implements the sandboxing restrictions for Lua scripts,
//! ensuring they can't access unsafe features like filesystem, networking,
//! or OS operations.

use crate::lua_new::vm::{LuaVM, ExecutionContext};
use crate::lua_new::value::{Value, CFunction, TableHandle};
use crate::lua_new::error::{LuaError, Result};
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
        println!("[LUA_SANDBOX] Applying sandbox restrictions");
        
        // Get globals table
        let globals = vm.globals();
        
        // Remove unsafe libraries
        self.remove_libraries(vm, globals)?;
        
        // Register safe stdlib functions
        self.register_safe_stdlib(vm, globals)?;
        
        Ok(())
    }
    
    /// Remove unsafe libraries
    fn remove_libraries(&self, vm: &mut LuaVM, globals: TableHandle) -> Result<()> {
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
    fn register_safe_stdlib(&self, vm: &mut LuaVM, globals: TableHandle) -> Result<()> {
        // Register critical global functions - These MUST be available for basic Lua functionality
        println!("[LUA_SANDBOX] Registering core Lua standard functions");
        
        // Register global functions
        let key_type = vm.heap.create_string("type");
        let func_type = Value::CFunction(lua_type);
        
        // First set the method to the globals table directly
        vm.heap.get_table_mut(globals)?
            .set(Value::String(key_type), func_type);
            
        // Register other essential functions directly to globals
        self.register_global_function(vm, globals, "tostring", tostring_func)?;
        self.register_global_function(vm, globals, "tonumber", tonumber_func)?;
        self.register_global_function(vm, globals, "print", lua_print)?;
        self.register_global_function(vm, globals, "assert", assert_func)?;
        self.register_global_function(vm, globals, "pairs", lua_pairs)?;
        self.register_global_function(vm, globals, "ipairs", lua_ipairs)?;
        self.register_global_function(vm, globals, "next", lua_next)?;
        self.register_global_function(vm, globals, "pcall", lua_pcall)?;
        
        // Register table library
        let table_lib = vm.heap.alloc_table();
        let table_key = vm.heap.create_string("table");
        
        // Register table library functions
        self.register_table_function(vm, table_lib, "concat", table_concat)?;
        self.register_table_function(vm, table_lib, "insert", table_insert)?;
        self.register_table_function(vm, table_lib, "remove", table_remove)?;
        
        // Set table library in globals
        vm.heap.get_table_mut(globals)?
            .set(Value::String(table_key), Value::Table(table_lib));
        
        // Register string library
        let string_lib = vm.heap.alloc_table();
        let string_key = vm.heap.create_string("string");
        
        // Register string library functions
        self.register_table_function(vm, string_lib, "len", string_len)?;
        self.register_table_function(vm, string_lib, "sub", string_sub)?;
        self.register_table_function(vm, string_lib, "byte", string_byte)?;
        self.register_table_function(vm, string_lib, "char", string_char)?;
        
        // Set string library in globals
        vm.heap.get_table_mut(globals)?
            .set(Value::String(string_key), Value::Table(string_lib));
        
        // Register math library
        let math_lib = vm.heap.alloc_table();
        let math_key = vm.heap.create_string("math");
        
        // Register math library functions
        self.register_table_function(vm, math_lib, "abs", math_abs)?;
        self.register_table_function(vm, math_lib, "floor", math_floor)?;
        self.register_table_function(vm, math_lib, "ceil", math_ceil)?;
        
        // Set math library in globals
        vm.heap.get_table_mut(globals)?
            .set(Value::String(math_key), Value::Table(math_lib));
        
        println!("[LUA_SANDBOX] Core Lua standard functions registered successfully");
        
        // Verify type function registration by running a simple test
        let type_key = vm.heap.create_string("type");
        match vm.heap.get_table(globals)?.get(&Value::String(type_key)) {
            Some(&Value::CFunction(_)) => {
                println!("[LUA_SANDBOX] Type function verification successful");
            },
            _ => {
                println!("[LUA_SANDBOX] WARNING: Type function not found in globals!");
            }
        }
        
        Ok(())
    }
    
    /// Register a global function
    fn register_global_function(&self, vm: &mut LuaVM, globals: TableHandle, name: &str, func: CFunction) -> Result<()> {
        let key = vm.heap.create_string(name);
        vm.heap.get_table_mut(globals)?
            .set(Value::String(key), Value::CFunction(func));
        Ok(())
    }
    
    /// Register a library function
    fn register_table_function(&self, vm: &mut LuaVM, table: TableHandle, name: &str, func: CFunction) -> Result<()> {
        let key = vm.heap.create_string(name);
        vm.heap.get_table_mut(table)?
            .set(Value::String(key), Value::CFunction(func));
        Ok(())
    }
    
    /// Register a function in a table (deprecated - use register_table_function instead)
    fn register_function_in_table(&self, vm: &mut LuaVM, table: TableHandle, name: &str, func: CFunction) -> Result<()> {
        self.register_table_function(vm, table, name, func)
    }
    
    /// Register a function in globals (deprecated - use register_global_function instead)
    fn register_function(&self, vm: &mut LuaVM, table: TableHandle, name: &str, func: CFunction) -> Result<()> {
        self.register_global_function(vm, table, name, func)
    }
}

/// Implementation of Lua's type() function
pub fn lua_type(ctx: &mut ExecutionContext) -> Result<i32> {
    // This is a critical function that must work perfectly for type checking
    println!("[LUA_TYPE] Type function called with {} args", ctx.get_arg_count());
    
    if ctx.get_arg_count() < 1 {
        return Err(LuaError::Runtime("type() requires an argument".to_string()));
    }
    
    let value = ctx.get_arg(0)?;
    
    // Type names MUST exactly match Lua 5.1 specifications
    let type_name = match value {
        Value::Nil => "nil",
        Value::Boolean(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Table(_) => "table",
        Value::Closure(_) | Value::CFunction(_) => "function",
        Value::Thread(_) => "thread",
    };
    
    // Debug output
    println!("[LUA_TYPE] Value: {:?}, type: {}", value, type_name);
    
    // Create string and push to stack
    let handle = ctx.heap().create_string(type_name);
    ctx.push_result(Value::String(handle))?;
    
    // Function pushed one result to stack
    Ok(1)
}

/// Implementation of Lua's print() function
pub fn lua_print(ctx: &mut ExecutionContext) -> Result<i32> {
    let count = ctx.get_arg_count();
    
    let mut output = String::new();
    
    for i in 0..count {
        if i > 0 {
            output.push('\t');
        }
        
        let value = ctx.get_arg(i)?;
        
        let str_value = match value {
            Value::Nil => "nil".to_string(),
            Value::Boolean(b) => if b { "true".to_string() } else { "false".to_string() },
            Value::Number(n) => n.to_string(),
            Value::String(s) => {
                let bytes = ctx.heap().get_string(s)?;
                match std::str::from_utf8(bytes) {
                    Ok(s) => s.to_string(),
                    Err(_) => "[invalid UTF-8]".to_string(),
                }
            },
            Value::Table(_) => "table".to_string(),
            Value::Closure(_) | Value::CFunction(_) => "function".to_string(),
            Value::Thread(_) => "thread".to_string(),
        };
        
        output.push_str(&str_value);
    }
    
    // Print to stdout (or log)
    println!("[LUA_PRINT] {}", output);
    
    // Return 0 values
    Ok(0)
}

/// Implementation of Lua's pairs() function
pub fn lua_pairs(ctx: &mut ExecutionContext) -> Result<i32> {
    if ctx.get_arg_count() < 1 {
        return Err(LuaError::Runtime("pairs requires a table argument".to_string()));
    }
    
    // Just return the next function, the table, and nil as initial state
    let next_fn = {
        let key = ctx.heap().create_string("next");
        let globals = ctx.vm.globals();
        match ctx.vm.table_get(globals, Value::String(key))? {
            Value::CFunction(f) => f,
            _ => return Err(LuaError::Runtime("next function not found".to_string())),
        }
    };
    
    // Push the three return values
    ctx.push_result(Value::CFunction(next_fn))?; // Iterator function
    ctx.push_result(ctx.get_arg(0)?)?;            // Table
    ctx.push_result(Value::Nil)?;                 // Initial state (nil)
    
    Ok(3) // Three return values
}

/// Implementation of Lua's ipairs() function
pub fn lua_ipairs(ctx: &mut ExecutionContext) -> Result<i32> {
    // Similar to pairs but for array-like tables
    // For now, let's use pairs as ipairs is similar in behavior though specialized for arrays
    lua_pairs(ctx)
}

/// Implementation of Lua's next() function
pub fn lua_next(ctx: &mut ExecutionContext) -> Result<i32> {
    if ctx.get_arg_count() < 1 {
        return Err(LuaError::Runtime("next requires a table argument".to_string()));
    }
    
    // Get table
    let table = match ctx.get_arg(0)? {
        Value::Table(t) => t,
        _ => return Err(LuaError::TypeError("next requires a table argument".to_string())),
    };
    
    // Get key (or nil for first iteration)
    let key = if ctx.get_arg_count() > 1 { ctx.get_arg(1)? } else { Value::Nil };
    
    // Get the next key-value pair
    // For now, we'll return nil (indicating no more entries) since implementation is complex
    // In a full implementation, we'd track table iteration
    ctx.push_result(Value::Nil)?;  // No more keys
    ctx.push_result(Value::Nil)?;  // No value
    
    Ok(0) // Return 0 as signal that there are no more elements
}

/// Implementation of Lua's pcall() function
pub fn lua_pcall(ctx: &mut ExecutionContext) -> Result<i32> {
    // Simplistic pcall implementation
    // In a full implementation, this would catch errors
    if ctx.get_arg_count() < 1 {
        return Err(LuaError::Runtime("pcall requires a function argument".to_string()));
    }
    
    // Get the function
    let func = ctx.get_arg(0)?;
    
    // For now just return success
    ctx.push_result(Value::Boolean(true))?;  // Success
    
    Ok(1) // One return value
}

/// Implementation of table.concat function
fn table_concat(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.get_arg_count() == 0 {
        return Err(LuaError::Runtime(
            "table.concat requires a table argument".to_string()
        ));
    }
    
    // Create empty result string
    let empty = ctx.heap().create_string("");
    ctx.push_result(Value::String(empty))?;
    
    Ok(1) // One return value
}

/// Implementation of table.insert function
fn table_insert(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.get_arg_count() < 2 {
        return Err(LuaError::Runtime(
            "table.insert requires at least 2 arguments".to_string()
        ));
    }
    
    // Just return nil (placeholder)
    ctx.push_result(Value::Nil)?;
    
    Ok(0) // No return value
}

/// Implementation of table.remove function
fn table_remove(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.get_arg_count() < 1 {
        return Err(LuaError::Runtime(
            "table.remove requires a table argument".to_string()
        ));
    }
    
    // Just return nil (placeholder)
    ctx.push_result(Value::Nil)?;
    
    Ok(0) // No return value
}

/// Implementation of string.len function
fn string_len(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.get_arg_count() == 0 {
        return Err(LuaError::Runtime(
            "string.len requires a string argument".to_string()
        ));
    }
    
    // Get string
    let value = ctx.get_arg(0)?;
    let len = match value {
        Value::String(s) => {
            let bytes = ctx.heap().get_string(s)?;
            bytes.len()
        }
        _ => return Err(LuaError::TypeError(
            "string.len: argument must be a string".to_string()
        )),
    };
    
    // Return length
    ctx.push_result(Value::Number(len as f64))?;
    
    Ok(1) // One return value
}

/// Implementation of string.sub function
fn string_sub(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.get_arg_count() < 3 {
        return Err(LuaError::Runtime(
            "string.sub requires a string and two positions".to_string()
        ));
    }
    
    // Return empty string for now (placeholder)
    let empty = ctx.heap().create_string("");
    ctx.push_result(Value::String(empty))?;
    
    Ok(1) // One return value
}

/// Implementation of string.byte function
fn string_byte(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.get_arg_count() < 1 {
        return Err(LuaError::Runtime(
            "string.byte requires a string argument".to_string()
        ));
    }
    
    // Return 0 for now (placeholder)
    ctx.push_result(Value::Number(0.0))?;
    
    Ok(1) // One return value
}

/// Implementation of string.char function
fn string_char(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.get_arg_count() < 1 {
        return Err(LuaError::Runtime(
            "string.char requires at least one byte value".to_string()
        ));
    }
    
    // Return empty string for now (placeholder)
    let empty = ctx.heap().create_string("");
    ctx.push_result(Value::String(empty))?;
    
    Ok(1) // One return value
}

/// Implementation of math.abs function
fn math_abs(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.get_arg_count() == 0 {
        return Err(LuaError::Runtime(
            "math.abs requires a number argument".to_string()
        ));
    }
    
    // Get number
    let n = match ctx.get_arg(0)? {
        Value::Number(n) => n,
        _ => return Err(LuaError::TypeError(
            "math.abs: argument must be a number".to_string()
        )),
    };
    
    // Return absolute value
    ctx.push_result(Value::Number(n.abs()))?;
    
    Ok(1) // One return value
}

/// Implementation of math.floor function
fn math_floor(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.get_arg_count() == 0 {
        return Err(LuaError::Runtime(
            "math.floor requires a number argument".to_string()
        ));
    }
    
    // Get number
    let n = match ctx.get_arg(0)? {
        Value::Number(n) => n,
        _ => return Err(LuaError::TypeError(
            "math.floor: argument must be a number".to_string()
        )),
    };
    
    // Return floor value
    ctx.push_result(Value::Number(n.floor()))?;
    
    Ok(1) // One return value
}

/// Implementation of math.ceil function
fn math_ceil(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.get_arg_count() == 0 {
        return Err(LuaError::Runtime(
            "math.ceil requires a number argument".to_string()
        ));
    }
    
    // Get number
    let n = match ctx.get_arg(0)? {
        Value::Number(n) => n,
        _ => return Err(LuaError::TypeError(
            "math.ceil: argument must be a number".to_string()
        )),
    };
    
    // Return ceiling value
    ctx.push_result(Value::Number(n.ceil()))?;
    
    Ok(1) // One return value
}

/// Implementation of assert function
fn assert_func(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.get_arg_count() == 0 {
        return Err(LuaError::Runtime(
            "assert requires at least 1 argument".to_string()
        ));
    }
    
    // Get value to assert
    let value = ctx.get_arg(0)?;
    
    if !value.to_bool() {
        // Get error message if provided
        let msg = if ctx.get_arg_count() > 1 {
            match ctx.get_arg(1)? {
                Value::String(s) => {
                    let bytes = ctx.heap().get_string(s)?;
                    match std::str::from_utf8(bytes) {
                        Ok(s) => s.to_string(),
                        Err(_) => "assertion failed!".to_string(),
                    }
                }
                _ => "assertion failed!".to_string(),
            }
        } else {
            "assertion failed!".to_string()
        };
        
        return Err(LuaError::Runtime(msg));
    }
    
    // Return the value that was asserted
    ctx.push_result(value)?;
    
    Ok(1) // One return value
}

/// Implementation of tonumber function
fn tonumber_func(ctx: &mut ExecutionContext) -> Result<i32> {
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
            let bytes = ctx.heap().get_string(s)?;
            match std::str::from_utf8(bytes) {
                Ok(s_str) => {
                    if let Ok(n) = s_str.parse::<f64>() {
                        ctx.push_result(Value::Number(n))?;
                    } else {
                        ctx.push_result(Value::Nil)?;
                    }
                }
                Err(_) => {
                    ctx.push_result(Value::Nil)?;
                }
            }
        }
        _ => {
            ctx.push_result(Value::Nil)?;
        }
    }
    
    Ok(1) // One return value
}

/// Implementation of tostring function
fn tostring_func(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.get_arg_count() == 0 {
        let handle = ctx.heap().create_string("");
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
            let handle = ctx.heap().create_string(&num_str);
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
    
    let handle = ctx.heap().create_string(result);
    ctx.push_result(Value::String(handle))?;
    
    Ok(1) // One return value
}