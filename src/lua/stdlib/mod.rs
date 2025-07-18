//! Standard Library Module
//! 
//! This module contains all the standard library implementations
//! for the Ferrous Lua VM.

pub mod base;
pub mod math;
pub mod string;
pub mod table;

use crate::lua::error::LuaResult;
use crate::lua::refcell_vm::RefCellVM;

/// Initialize all standard libraries in the RefCellVM
pub fn init_all_stdlib(vm: &mut RefCellVM) -> LuaResult<()> {
    println!("Initializing all standard libraries for RefCellVM");
    
    // Initialize base library (must be first)
    base::init_base_lib(vm)?;
    
    // Initialize other libraries
    math::init_math_lib(vm)?;
    string::init_string_lib(vm)?;
    table::init_table_lib(vm)?;
    
    println!("All standard libraries initialized successfully");
    Ok(())
}