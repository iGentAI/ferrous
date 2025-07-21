//! Lua VM test script compiler and executor
//!
//! This binary compiles and executes a Lua script using the Ferrous RC RefCell Lua VM.
//! It takes a filepath as the only argument and reports the result.

use std::env;
use std::fs;
use std::path::Path;
use std::process;

use ferrous::lua::{compile, RcVM};
use ferrous::lua::rc_value::Value;

fn main() {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <luascript.lua>", args[0]);
        process::exit(1);
    }
    
    let filepath = &args[1];
    
    // Read the file
    println!("Reading Lua script: {}", filepath);
    let source = match fs::read_to_string(filepath) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            process::exit(1);
        }
    };
    
    // Extract script name for reporting
    let script_name = Path::new(filepath).file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(filepath);
    
    println!("Compiling script: {}", script_name);
    
    // Compile the script
    let module = match compile(&source) {
        Ok(m) => {
            println!("Compilation successful!");
            m
        },
        Err(e) => {
            eprintln!("Compilation error: {}", e);
            process::exit(1);
        }
    };
    
    println!("Running script...");
    
    // Create VM and initialize standard library
    let mut vm = match RcVM::new() {
        Ok(vm) => vm,
        Err(e) => {
            eprintln!("Error creating VM: {}", e);
            process::exit(1);
        }
    };
    
    // Initialize standard library
    if let Err(e) = vm.init_stdlib() {
        eprintln!("Error initializing standard library: {}", e);
        process::exit(1);
    }
    
    // Execute the module
    let result = match vm.execute_module(&module, &[]) {
        Ok(value) => value,
        Err(e) => {
            eprintln!("Execution error: {}", e);
            process::exit(1);
        }
    };
    
    // Print the result using RC RefCell value types
    match result {
        Value::Nil => println!("Result: nil"),
        Value::Boolean(b) => println!("Result: {}", b),
        Value::Number(n) => println!("Result: {}", n),
        Value::String(string_handle) => {
            // Access string value through RC RefCell
            let string_ref = string_handle.borrow();
            match string_ref.to_str() {
                Ok(s) => println!("Result: \"{}\"", s),
                Err(_) => println!("Result: <binary string data>"),
            }
        },
        Value::Table(table_handle) => {
            println!("DEBUG: Handling table return value");
            
            // Get basic table info through RC RefCell
            let table_ref = table_handle.borrow();
            let array_len = table_ref.array.len();
            let hash_len = table_ref.map.len();
            
            println!("Result: <table with {} array elements and {} hash entries>", 
                     array_len, hash_len);
            
            // For small tables, print some content
            if array_len > 0 {
                println!("  Array elements:");
                for i in 0..std::cmp::min(array_len, 5) {
                    println!("    [{}]: {:?}", i+1, table_ref.array[i]);
                }
                if array_len > 5 {
                    println!("    ... ({} more elements)", array_len - 5);
                }
            }
            
            if hash_len > 0 {
                println!("  Hash entries:");
                let mut printed = 0;
                for (k, v) in &table_ref.map {
                    if printed < 5 {
                        println!("    {:?} => {:?}", k, v);
                        printed += 1;
                    } else {
                        break;
                    }
                }
                if hash_len > 5 {
                    println!("    ... ({} more entries)", hash_len - 5);
                }
            }
        },
        Value::Closure(_) => println!("Result: <function>"),
        Value::Thread(_) => println!("Result: <thread>"),
        Value::CFunction(_) => println!("Result: <c function>"),
        Value::UserData(_) => println!("Result: <userdata>"),
        Value::FunctionProto(_) => println!("Result: <function prototype>"),
        Value::PendingMetamethod(_) => println!("Result: <pending metamethod>"),
    }
    println!("Script execution completed successfully!");
}