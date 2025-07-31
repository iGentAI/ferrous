//! Test Runner for String Interning Validation
//! 
//! This binary loads and executes a Lua script focused on testing that string interning
//! works correctly for function lookup, table operations, and metamethods.

use std::fs;
use std::path::Path;
use ferrous::lua::{LuaVM, Value, compiler};
use ferrous::lua::transaction::HeapTransaction;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script_path = Path::new("tests/lua/string_interning_test.lua");
    println!("Loading string interning test from: {}", script_path.display());
    let source = fs::read_to_string(&script_path)?;
    
    // Create VM and initialize standard library
    println!("Creating Lua VM...");
    let mut vm = LuaVM::new()?;
    
    println!("Initializing standard library...");
    vm.init_stdlib()?;
    
    // Compile script
    println!("Compiling script...");
    let module = compiler::compile(&source)?;
    println!("Compilation successful!");
    
    // Execute script
    println!("Executing script...");
    let result = vm.execute_module(&module, &[])?;
    
    // Print result
    match &result {
        Value::Nil => println!("Result: nil"),
        Value::Boolean(b) => println!("Result: {}", b),
        Value::Number(n) => println!("Result: {}", n),
        Value::String(_) => {
            let mut tx = HeapTransaction::new(vm.heap_mut());
            println!("Result: \"{}\"", match result {
                Value::String(handle) => tx.get_string_value(handle)?,
                _ => unreachable!(),
            });
            tx.commit()?;
        },
        Value::Table(_) => println!("Result: <table>"),
        Value::Closure(_) => println!("Result: <function>"),
        Value::Thread(_) => println!("Result: <thread>"),
        Value::CFunction(_) => println!("Result: <C function>"),
        Value::UserData(_) => println!("Result: <userdata>"),
        Value::FunctionProto(_) => println!("Result: <function prototype>"),
    }
    
    println!("String interning test completed successfully!");
    Ok(())
}