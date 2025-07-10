//! Lua Standard Library Module Organization
//!
//! This module organizes the standard library implementation into submodules
//! for better maintainability.

// Re-export submodules
pub mod math;
pub mod string;
pub mod base;
pub mod table;

// Re-export initialization functions for easy use
pub use base::init_base_lib;
pub use math::init_math_lib;
pub use string::init_string_lib;
pub use table::init_table_lib;

use crate::lua::error::LuaResult;
use crate::lua::vm::LuaVM;
use crate::lua::transaction::HeapTransaction;

/// Initialize all standard library components
pub fn init_all(vm: &mut LuaVM) -> LuaResult<()> {
    // Initialize base library (core functions)
    base::init_base_lib(vm)?;
    
    // Verify base library functions to help debug
    verify_stdlib_functions(vm, &["assert", "pairs", "ipairs", "tostring", "type"])?;
    
    // Initialize math library if available
    if let Ok(_) = math::init_math_lib(vm) {
        println!("Math library initialized");
    }
    
    // Initialize string library if available
    if let Ok(_) = string::init_string_lib(vm) {
        println!("String library initialized");
    }
    
    // Initialize table library if available
    if let Ok(_) = table::init_table_lib(vm) {
        println!("Table library initialized");
    }
    
    // Additional libraries can be added here
    
    // Do one final verification after all libraries are loaded
    verify_stdlib_functions(vm, &["assert", "pairs", "ipairs", "tostring", "type"])?;
    
    Ok(())
}

/// Debug helper to verify standard library functions are properly registered
fn verify_stdlib_functions(vm: &mut LuaVM, functions: &[&str]) -> LuaResult<()> {
    let mut tx = HeapTransaction::new(vm.heap_mut());
    let globals = tx.get_globals_table()?;
    
    println!("DEBUG STDLIB VERIFY: Checking for standard library functions...");
    
    for &func_name in functions {
        let name_handle = tx.create_string(func_name)?;
        let func_value = tx.read_table_field(globals, &crate::lua::value::Value::String(name_handle))?;
        
        if func_value.is_nil() {
            println!("DEBUG STDLIB VERIFY: WARNING - Function '{}' is NOT registered!", func_name);
        } else {
            println!("DEBUG STDLIB VERIFY: Function '{}' is registered as {}", func_name, func_value.type_name());
        }
    }
    
    // Get table stats first
    let (array_len, map_len) = {
        let table_obj = tx.get_table(globals)?;
        (table_obj.array.len(), table_obj.map.len())
    };
    
    println!("DEBUG STDLIB VERIFY: Globals table has {} array elements and {} hash elements", 
             array_len, map_len);
             
    // Now gather string handles in a phase 1 approach to avoid borrowing tx twice
    let mut entries = Vec::new();
    {
        let table_obj = tx.get_table(globals)?;
        
        // Collect the first 25 string entries
        let mut count = 0;
        for (k, v) in &table_obj.map {
            if count < 25 {  // Limit to 25 entries to avoid flooding output
                if let crate::lua::value::HashableValue::String(s) = k {
                    entries.push((*s, v.clone()));
                    count += 1;
                }
            } else {
                break;
            }
        }
    }
    
    // Process the collected entries with their string names in phase 2
    for (s, v) in entries {
        let name = tx.get_string_value(s.into())?;
        println!("DEBUG STDLIB VERIFY: Global '{}' -> {} ({})", 
                 name, v, v.type_name());
    }
    
    // Print remaining entries count
    if map_len > 25 {
        println!("DEBUG STDLIB VERIFY: ... and {} more entries", map_len - 25);
    }
    
    tx.commit()?;
    
    Ok(())
}

/// Backwards compatibility function that delegates to init_all
pub fn init_stdlib(vm: &mut LuaVM) -> LuaResult<()> {
    init_all(vm)
}