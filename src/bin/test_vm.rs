//! Test Runner for Lua VM implementation
//! 
//! This binary loads and executes a Lua script to test the VM implementation.

use std::fs;
use std::path::Path;
use std::env;
use ferrous::lua::{LuaVM, Value, compiler};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get script path from args or use default
    let args: Vec<String> = env::args().collect();
    let script_path = if args.len() > 1 {
        Path::new(&args[1]).to_path_buf()
    } else {
        Path::new("tests/lua/minimal.lua").to_path_buf()
    };
    
    println!("Loading script from: {}", script_path.display());
    let source = fs::read_to_string(&script_path)?;
    
    // Create VM and initialize standard library
    println!("Creating Lua VM...");
    let mut vm = LuaVM::new()?;
    
    println!("Initializing standard library...");
    vm.init_stdlib()?;
    
    // Compile script
    println!("Compiling script: {}", source);
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
            let mut tx = ferrous::lua::transaction::HeapTransaction::new(vm.heap_mut());
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
    
    println!("Test completed successfully!");
    Ok(())
}