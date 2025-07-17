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
use crate::lua::refcell_vm::RefCellVM;
use crate::lua::value::Value;

/// Initialize all standard library components
pub fn init_all(vm: &mut RefCellVM) -> LuaResult<()> {
    // Initialize base library (core functions)
    base::init_base_lib(vm)?;
    
    // Verify base library functions to help debug
    println!("DEBUG STDLIB VERIFY: Checking for standard library functions...");
    
    let globals_handle = vm.heap().globals()?;
    
    // Verify key functions are registered
    for &func_name in &["assert", "pairs", "ipairs", "tostring", "type"] {
        let name_handle = vm.heap().create_string(func_name)?;
        let func_value = vm.heap().get_table_field(globals_handle, &Value::String(name_handle))?;
        
        if func_value.is_nil() {
            println!("DEBUG STDLIB VERIFY: WARNING - Function '{}' is NOT registered!", func_name);
        } else {
            println!("DEBUG STDLIB VERIFY: Function '{}' is registered as {}", func_name, func_value.type_name());
        }
    }
    
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
    println!("DEBUG STDLIB VERIFY: Checking for standard library functions...");
    
    // Get globals table stats
    let globals = vm.heap().globals()?;
    let table = vm.heap().get_table(globals)?;
    
    println!("DEBUG STDLIB VERIFY: Globals table has {} array elements and {} hash entries", 
             table.array.len(), table.map.len());
    
    Ok(())
}

/// Backwards compatibility function that delegates to init_all
pub fn init_stdlib(vm: &mut RefCellVM) -> LuaResult<()> {
    init_all(vm)
}