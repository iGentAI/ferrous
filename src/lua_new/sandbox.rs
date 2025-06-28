//! Lua sandbox for secure script execution
//!
//! This module implements the sandboxing restrictions for Lua scripts,
//! ensuring they can't access unsafe features like filesystem, networking,
//! or OS operations.

use crate::lua_new::vm::{LuaVM, ExecutionContext};
use crate::lua_new::value::{Value, CFunction, TableHandle};
use crate::lua_new::error::{LuaError, Result};

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
        
        // Remove unsafe libraries
        self.remove_libraries(vm)?;
        
        // Register safe stdlib functions
        self.register_safe_stdlib(vm)?;
        
        Ok(())
    }
    
    /// Remove unsafe libraries
    fn remove_libraries(&self, vm: &mut LuaVM) -> Result<()> {
        let unsafe_libs = ["io", "os", "debug", "package", "coroutine", "dofile", "loadfile"];
        let globals = vm.globals();
        
        for lib in &unsafe_libs {
            let key = vm.heap.create_string(lib);
            vm.table_set(globals, Value::String(key), Value::Nil)?;
        }
        
        // Ensure math.random and math.randomseed are removed if deterministic mode is enabled
        if self.deterministic {
            let math_key = vm.heap.create_string("math");
            
            // Get the math table using two-phase approach
            let math_table_opt = {
                let math_value = vm.table_get(globals, Value::String(math_key))?;
                if let Value::Table(table) = math_value {
                    Some(table)
                } else {
                    None
                }
            };
            
            if let Some(math_table) = math_table_opt {
                let random_key = vm.heap.create_string("random");
                let randomseed_key = vm.heap.create_string("randomseed");
                
                vm.table_set(math_table, Value::String(random_key), Value::Nil)?;
                vm.table_set(math_table, Value::String(randomseed_key), Value::Nil)?;
            }
        }
        
        Ok(())
    }
    
    /// Register safe standard library functions
    fn register_safe_stdlib(&self, vm: &mut LuaVM) -> Result<()> {
        // Register critical global functions - These MUST be available for basic Lua functionality
        println!("[LUA_SANDBOX] Registering core Lua standard functions");
        
        // Register essential global functions
        self.register_global_function(vm, "type", lua_type)?;
        self.register_global_function(vm, "tostring", tostring_func)?;
        self.register_global_function(vm, "tonumber", tonumber_func)?;
        self.register_global_function(vm, "print", lua_print)?;
        self.register_global_function(vm, "assert", assert_func)?;
        
        // Register iterator functions - critical for for-loops
        self.register_global_function(vm, "pairs", lua_pairs)?;
        self.register_global_function(vm, "ipairs", lua_ipairs)?;
        self.register_global_function(vm, "next", lua_next)?;
        
        // Other core functions
        self.register_global_function(vm, "pcall", lua_pcall)?;
        
        // Register table library
        let table_lib = vm.heap.alloc_table();
        let table_key = vm.heap.create_string("table");
        
        // Register table library functions
        self.register_table_function(vm, table_lib, "concat", table_concat)?;
        self.register_table_function(vm, table_lib, "insert", table_insert)?;
        self.register_table_function(vm, table_lib, "remove", table_remove)?;
        
        // Set table library in globals
        vm.table_set(vm.globals(), Value::String(table_key), Value::Table(table_lib))?;
        
        // Register string library
        let string_lib = vm.heap.alloc_table();
        let string_key = vm.heap.create_string("string");
        
        // Register string library functions
        self.register_table_function(vm, string_lib, "len", string_len)?;
        self.register_table_function(vm, string_lib, "sub", string_sub)?;
        self.register_table_function(vm, string_lib, "byte", string_byte)?;
        self.register_table_function(vm, string_lib, "char", string_char)?;
        
        // Set string library in globals
        vm.table_set(vm.globals(), Value::String(string_key), Value::Table(string_lib))?;
        
        // Register math library
        let math_lib = vm.heap.alloc_table();
        let math_key = vm.heap.create_string("math");
        
        // Register math library functions
        self.register_table_function(vm, math_lib, "abs", math_abs)?;
        self.register_table_function(vm, math_lib, "floor", math_floor)?;
        self.register_table_function(vm, math_lib, "ceil", math_ceil)?;
        
        // Set math library in globals
        vm.table_set(vm.globals(), Value::String(math_key), Value::Table(math_lib))?;
        
        println!("[LUA_SANDBOX] Core Lua standard functions registered successfully");
        
        // Verify type function registration by running a simple test
        let type_key = vm.heap.create_string("type");
        let type_value = vm.table_get(vm.globals(), Value::String(type_key))?;
        
        if matches!(type_value, Value::CFunction(_)) {
            println!("[LUA_SANDBOX] Type function verification successful");
        } else {
            println!("[LUA_SANDBOX] WARNING: Type function not found in globals!");
        }
        
        Ok(())
    }
    
    /// Register a global function
    fn register_global_function(&self, vm: &mut LuaVM, name: &str, func: CFunction) -> Result<()> {
        let key = vm.heap.create_string(name);
        vm.table_set(vm.globals(), Value::String(key), Value::CFunction(func))
    }
    
    /// Register a library function
    fn register_table_function(&self, vm: &mut LuaVM, table: TableHandle, name: &str, func: CFunction) -> Result<()> {
        let key = vm.heap.create_string(name);
        vm.table_set(table, Value::String(key), Value::CFunction(func))
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
    
    let arg = ctx.get_arg(0)?;
    
    // Check that the argument is a table
    let table = match arg {
        Value::Table(t) => t,
        _ => return Err(LuaError::TypeError(format!("bad argument #1 to 'pairs' (table expected, got {})", arg.type_name()))),
    };
    
    // Phase 1: Collect next function with a single immutable borrow
    let next_fn = {
        let globals = ctx.vm.globals();
        let next_key = ctx.heap().create_string("next");
        
        // Get globals table and look up next function
        let globals_obj = ctx.heap().get_table(globals)?;
        match globals_obj.get(&Value::String(next_key)) {
            Some(&Value::CFunction(f)) => Value::CFunction(f),
            Some(&Value::Closure(c)) => Value::Closure(c), 
            _ => return Err(LuaError::Runtime("next function not found".to_string())),
        }
    };
    
    // Phase 2: Push results without holding any borrows
    ctx.push_result(next_fn)?;               // Iterator function
    ctx.push_result(Value::Table(table))?;   // Table
    ctx.push_result(Value::Nil)?;            // Initial state (nil)
    
    Ok(3) // Three return values
}

/// Implementation of ipairs_iter function for ipairs
fn ipairs_iter(ctx: &mut ExecutionContext) -> Result<i32> {
    if ctx.get_arg_count() < 2 {
        return Err(LuaError::Runtime("ipairs iterator requires table and index arguments".to_string()));
    }
    
    // Get table and index arguments with proper error handling
    let table_arg = ctx.get_arg(0)?;
    let idx_arg = ctx.get_arg(1)?;
    
    let table = match table_arg {
        Value::Table(t) => t,
        _ => return Err(LuaError::TypeError("bad argument #1 to 'ipairs iterator' (table expected)".to_string())),
    };
    
    let index = match idx_arg {
        Value::Number(n) => n,
        _ => return Err(LuaError::TypeError("bad argument #2 to 'ipairs iterator' (number expected)".to_string())),
    };
    
    // Calculate next index (1-based)
    let next_index = index + 1.0;
    
    // Phase 1: Get value at next_index using the two-phase pattern
    let next_value = {
        let table_obj = ctx.heap().get_table(table)?;
        
        let idx = next_index as usize - 1; // Convert to 0-based
        if idx < table_obj.array.len() {
            table_obj.array[idx]  // Values implement Copy, so this is efficient
        } else {
            match table_obj.map.get(&Value::Number(next_index)) {
                Some(&v) => v,  // Copy the value
                None => Value::Nil
            }
        }
    };
    
    // Phase 2: Process the value without holding any borrows
    if next_value == Value::Nil {
        // End of iteration
        return Ok(0); 
    }
    
    // Return next index and value
    ctx.push_result(Value::Number(next_index))?;
    ctx.push_result(next_value)?;
    
    Ok(2) // 2 return values: index and value
}

/// Implementation of Lua's ipairs() function
pub fn lua_ipairs(ctx: &mut ExecutionContext) -> Result<i32> {
    if ctx.get_arg_count() < 1 {
        return Err(LuaError::Runtime("ipairs requires a table argument".to_string()));
    }
    
    let arg = ctx.get_arg(0)?;
    
    // Check that the argument is a table
    let table = match arg {
        Value::Table(t) => t,
        _ => return Err(LuaError::TypeError(format!("bad argument #1 to 'ipairs' (table expected, got {})", arg.type_name()))),
    };
    
    // Push iteration factory values
    ctx.push_result(Value::CFunction(ipairs_iter))?; // Iterator function
    ctx.push_result(Value::Table(table))?;           // Table
    ctx.push_result(Value::Number(0.0))?;            // Initial index (0)
    
    Ok(3) // Return 3 values
}

/// Implementation of Lua's next() function for table iteration
pub fn lua_next(ctx: &mut ExecutionContext) -> Result<i32> {
    if ctx.get_arg_count() < 1 {
        return Err(LuaError::TypeError("bad argument #1 to 'next' (table expected)".to_string()));
    }
    
    let table_arg = ctx.get_arg(0)?;
    let key_arg = if ctx.get_arg_count() >= 2 { ctx.get_arg(1)? } else { Value::Nil };
    
    // Get table
    let table = match table_arg {
        Value::Table(t) => t,
        _ => return Err(LuaError::TypeError(format!("bad argument #1 to 'next' (table expected, got {})", table_arg.type_name()))),
    };
    
    // Phase 1: Collect all data from the table with a single immutable borrow
    let (array_values, map_entries) = {
        let table_obj = ctx.heap().get_table(table)?;
        
        // Collect array part with non-nil values
        let array_values: Vec<(usize, Value)> = table_obj.array
            .iter()
            .enumerate()
            .filter(|(_, &v)| !matches!(v, Value::Nil))
            .map(|(i, &v)| (i, v))  // Value implements Copy
            .collect();
        
        // Collect map part
        let map_entries: Vec<(Value, Value)> = table_obj.map
            .iter()
            .map(|(k, &v)| (k.clone(), v))  // Clone key, copy value
            .collect();
        
        (array_values, map_entries)
    };
    
    // Phase 2: Find the next key-value pair without holding any borrows
    
    // Case 1: Start of iteration (key is nil)
    if key_arg == Value::Nil {
        if let Some((i, value)) = array_values.first() {
            ctx.push_result(Value::Number((i + 1) as f64))?; // 1-indexed
            ctx.push_result(*value)?;  // Value is Copy
            return Ok(2);
        }
        
        if let Some((key, value)) = map_entries.first() {
            ctx.push_result(key.clone())?;
            ctx.push_result(*value)?;  // Value is Copy
            return Ok(2);
        }
        
        return Ok(0); // Empty table
    }
    
    // Case 2: Continue iteration after a numeric key
    if let Value::Number(n) = key_arg {
        if n.fract() == 0.0 && n >= 1.0 {
            let idx = n as usize - 1; // Convert to 0-based
            
            // Find next array element
            for (i, value) in &array_values {
                if *i > idx {
                    ctx.push_result(Value::Number((i + 1) as f64))?;
                    ctx.push_result(*value)?; // Value is Copy
                    return Ok(2);
                }
            }
            
            // If no more array elements, check map
            if let Some((key, value)) = map_entries.first() {
                ctx.push_result(key.clone())?;
                ctx.push_result(*value)?; // Value is Copy
                return Ok(2);
            }
            
            return Ok(0); // No more elements
        }
    }
    
    // Case 3: Continue iteration after a non-numeric key
    // Sort entries for consistent iteration
    let mut sorted_entries = map_entries;
    sorted_entries.sort_by(|(k1, _), (k2, _)| {
        match (k1, k2) {
            (Value::String(s1), Value::String(s2)) => s1.0.index.cmp(&s2.0.index),
            (Value::Number(n1), Value::Number(n2)) => n1.partial_cmp(n2).unwrap_or(std::cmp::Ordering::Equal),
            _ => std::cmp::Ordering::Equal, // Default ordering for mixed types
        }
    });
    
    let mut found_key = false;
    for (key, value) in &sorted_entries {
        if found_key {
            // This is the next key after the one we found
            ctx.push_result(key.clone())?;
            ctx.push_result(*value)?; // Value is Copy
            return Ok(2);
        }
        
        if key == &key_arg {
            found_key = true;
        }
    }
    
    // No more elements
    Ok(0)
}

/// Implementation of Lua's pcall() function
pub fn lua_pcall(ctx: &mut ExecutionContext) -> Result<i32> {
    // Simplified pcall implementation
    if ctx.get_arg_count() < 1 {
        return Err(LuaError::Runtime("pcall requires a function argument".to_string()));
    }
    
    // Just return success without calling the function for now
    ctx.push_result(Value::Boolean(true))?; // Success status
    
    Ok(1) // One return value (success status)
}

/// Implementation of table.concat function
fn table_concat(ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if ctx.get_arg_count() < 1 {
        return Err(LuaError::Runtime(
            "table.concat requires a table argument".to_string()
        ));
    }
    
    // Simple placeholder: return empty string
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
    
    // Phase 1: Get the string and calculate its length
    let len = {
        let value = ctx.get_arg(0)?;
        match value {
            Value::String(s) => {
                let bytes = ctx.heap().get_string(s)?;
                bytes.len()
            }
            _ => return Err(LuaError::TypeError(
                "string.len: argument must be a string".to_string()
            )),
        }
    };
    
    // Phase 2: Return the length
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
    
    // Phase 1: Get and validate the number
    let n = {
        let value = ctx.get_arg(0)?;
        match value {
            Value::Number(n) => n,
            _ => return Err(LuaError::TypeError(
                "math.abs: argument must be a number".to_string()
            )),
        }
    };
    
    // Phase 2: Return absolute value
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
    
    // Phase 1: Get and validate the number
    let n = {
        let value = ctx.get_arg(0)?;
        match value {
            Value::Number(n) => n,
            _ => return Err(LuaError::TypeError(
                "math.floor: argument must be a number".to_string()
            )),
        }
    };
    
    // Phase 2: Return floor value
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
    
    // Phase 1: Get and validate the number
    let n = {
        let value = ctx.get_arg(0)?;
        match value {
            Value::Number(n) => n,
            _ => return Err(LuaError::TypeError(
                "math.ceil: argument must be a number".to_string()
            )),
        }
    };
    
    // Phase 2: Return ceiling value
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
    
    // Phase 1: Get value and check if it's truthy
    let (value, should_error) = {
        let value = ctx.get_arg(0)?;
        (value, !value.to_bool())
    };
    
    // Phase 2: Handle the assertion
    if should_error {
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
    
    // Phase 1: Get the value to convert
    let result = {
        let value = ctx.get_arg(0)?;
        
        match value {
            Value::Number(n) => Value::Number(n),
            Value::String(s) => {
                let bytes = ctx.heap().get_string(s)?;
                match std::str::from_utf8(bytes) {
                    Ok(s_str) => {
                        match s_str.parse::<f64>() {
                            Ok(n) => Value::Number(n),
                            Err(_) => Value::Nil,
                        }
                    }
                    Err(_) => Value::Nil,
                }
            }
            _ => Value::Nil,
        }
    };
    
    // Phase 2: Push the result
    ctx.push_result(result)?;
    
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
    
    // Phase 1: Get value and determine representation
    let result = {
        let value = ctx.get_arg(0)?;
        
        match value {
            Value::Nil => ctx.heap().create_string("nil"),
            Value::Boolean(b) => {
                let s = if b { "true" } else { "false" };
                ctx.heap().create_string(s)
            },
            Value::Number(n) => {
                let num_str = n.to_string();
                ctx.heap().create_string(&num_str)
            },
            Value::String(s) => s, // Just return the string
            Value::Table(_) => ctx.heap().create_string("table"),
            Value::Closure(_) => ctx.heap().create_string("function"),
            Value::CFunction(_) => ctx.heap().create_string("function"),
            Value::Thread(_) => ctx.heap().create_string("thread"),
        }
    };
    
    // Phase 2: Push result
    ctx.push_result(Value::String(result))?;
    
    Ok(1) // One return value
}