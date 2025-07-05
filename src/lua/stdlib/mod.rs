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

/// Initialize all standard library components
pub fn init_all(vm: &mut LuaVM) -> LuaResult<()> {
    // Initialize base library (core functions)
    base::init_base_lib(vm)?;
    
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
    
    Ok(())
}

/// Backwards compatibility function that delegates to init_all
pub fn init_stdlib(vm: &mut LuaVM) -> LuaResult<()> {
    init_all(vm)
}