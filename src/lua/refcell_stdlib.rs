//! Standard Library for RefCellVM
//! 
//! This module provides adapted versions of standard library functions
//! that work with RefCellVM's ExecutionContext.

use crate::lua::error::{LuaError, LuaResult};
use crate::lua::value::{Value, CFunction};
use crate::lua::handle::{StringHandle, TableHandle};
use crate::lua::refcell_heap::RefCellHeap;
use crate::lua::refcell_vm::{RefCellVM, RefCellExecutionContext, RefCellCFunction};

/// Initialize the standard library for RefCellVM
pub fn init_refcell_stdlib(vm: &mut RefCellVM) -> LuaResult<()> {
    println!("Initializing RefCellVM standard library");
    
    // Get global table
    let globals = vm.heap().globals()?;
    
    // Set up _G._G = _G as required
    let g_name = vm.heap().create_string("_G")?;
    vm.heap().set_table_field(globals, &Value::String(g_name), &Value::Table(globals))?;
    
    // Register basic print function
    let print_name = vm.heap().create_string("print")?;
    let placeholder: CFunction = Box::new(|ctx| {
        println!("[RefCellVM print placeholder]");
        Ok(0)
    });
    vm.heap().set_table_field(globals, &Value::String(print_name), &Value::CFunction(placeholder))?;
    
    // Register basic type function
    let type_name = vm.heap().create_string("type")?;
    let type_func: CFunction = Box::new(|ctx| {
        if let Ok(arg) = ctx.get_arg(0) {
            println!("Type of argument: {}", arg.type_name());
            // For now, just return nil
            ctx.push_result(Value::Nil)?;
            Ok(1)
        } else {
            println!("Error in type() function");
            Ok(0)
        }
    });
    vm.heap().set_table_field(globals, &Value::String(type_name), &Value::CFunction(type_func))?;
    
    // Add a few more basic placeholder functions
    let basic_funcs = ["assert", "pairs", "ipairs", "tostring", "tonumber"];
    for fname in basic_funcs {
        let name = vm.heap().create_string(fname)?;
        let func: CFunction = Box::new(move |_ctx| {
            println!("[RefCellVM {} placeholder]", fname);
            Ok(0)
        });
        vm.heap().set_table_field(globals, &Value::String(name), &Value::CFunction(func))?;
    }
    
    println!("RefCellVM standard library initialized");
    Ok(())
}