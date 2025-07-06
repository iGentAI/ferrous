//! Standard Library Test Runner
//!
//! This binary loads and executes the standard library test scripts to verify
//! that the implementation is working correctly.

use std::env;
use std::fs;
use std::path::Path;
use std::time::Instant;
use std::process::exit;

use ferrous::lua::error::LuaResult;
use ferrous::lua::value::Value;
use ferrous::lua::vm::LuaVM;
use ferrous::lua::compiler::compile;

fn run_test_script(path: &Path) -> LuaResult<bool> {
    println!("Running test script: {}", path.display());
    
    // Read the script
    let script = fs::read_to_string(path)
        .expect("Failed to read test script");
    
    // Create a VM
    let mut vm = LuaVM::new()?;
    
    // Initialize standard library
    ferrous::lua::stdlib::init_all(&mut vm)?;
    
    // Compile the script
    let start_time = Instant::now();
    let compiled_module = compile(&script)?;
    let compile_duration = start_time.elapsed();
    
    println!("Compilation completed in {:?}", compile_duration);
    
    // Execute the compiled module
    let start_time = Instant::now();
    let result = vm.execute_module(&compiled_module, &[])?;
    let execute_duration = start_time.elapsed();
    
    println!("Execution completed in {:?}", execute_duration);
    
    // Check if the test succeeded (should return true)
    let passed = match result {
        Value::Boolean(true) => true,
        _ => {
            println!("Test script returned {:?} instead of true", result);
            false
        }
    };
    
    Ok(passed)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        eprintln!("Usage: {} <test_script> [test_script2 ...]", args[0]);
        exit(1);
    }
    
    let mut all_passed = true;
    
    for script_path in &args[1..] {
        let path = Path::new(script_path);
        
        match run_test_script(path) {
            Ok(passed) => {
                if !passed {
                    all_passed = false;
                    eprintln!("Test script failed: {}", script_path);
                }
            }
            Err(e) => {
                all_passed = false;
                eprintln!("Error executing test script {}: {}", script_path, e);
            }
        }
    }
    
    if all_passed {
        println!("All tests passed!");
        exit(0);
    } else {
        eprintln!("Some tests failed!");
        exit(1);
    }
}