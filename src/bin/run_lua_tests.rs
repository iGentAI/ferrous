//! Lua Test Runner
//!
//! This binary provides a simple CLI to run Lua compliance tests,
//! testing all opcodes and language features.

use ferrous::lua::{RefCellVM, Value, compile};
use std::env;
use std::fs;
use std::process;
use std::path::Path;

/// Run a single Lua script and return its result
fn run_script(script_path: &str) -> Result<Value, String> {
    // Read the script file
    let script = match fs::read_to_string(script_path) {
        Ok(content) => content,
        Err(e) => return Err(format!("Failed to read script file: {}", e)),
    };
    
    // Create a VM instance
    let mut vm = match RefCellVM::new() {
        Ok(vm) => vm,
        Err(e) => return Err(format!("Failed to create VM: {:?}", e)),
    };
    
    // Initialize the standard library
    match vm.init_stdlib() {
        Ok(_) => {},
        Err(e) => return Err(format!("Failed to initialize standard library: {:?}", e)),
    };
    
    // Compile the script
    let module = match compile(&script) {
        Ok(module) => module,
        Err(e) => return Err(format!("Compilation error: {:?}", e)),
    };
    
    // Execute the script
    match vm.execute_module(&module, &[]) {
        Ok(value) => Ok(value),
        Err(e) => Err(format!("Execution error: {:?}", e)),
    }
}

/// Print usage information
fn print_usage(program_name: &str) {
    println!("Usage: {} [OPTIONS] SCRIPT_PATH", program_name);
    println!();
    println!("Options:");
    println!("  --help        Show this help message");
    println!("  --run-all     Run all compliance tests");
    println!();
    println!("Examples:");
    println!("  {} tests/lua/comprehensive.lua", program_name);
    println!("  {} --run-all", program_name);
}

/// Run all compliance tests
fn run_all_tests() -> bool {
    let test_files = [
        "tests/lua/comprehensive.lua",
        "tests/lua/table_test.lua",
        "tests/lua/closure_test.lua",
        "tests/lua/nested_refs_test.lua"
    ];
    
    let mut success = true;
    
    for file in test_files {
        println!("Running test: {}", file);
        
        if Path::new(file).exists() {
            match run_script(file) {
                Ok(_) => println!("  Result: SUCCESS"),
                Err(e) => {
                    println!("  Result: FAILURE");
                    println!("  Error: {}", e);
                    success = false;
                }
            }
        } else {
            println!("  Result: SKIPPED (file not found)");
        }
        println!();
    }
    
    if success {
        println!("All tests passed successfully!");
    } else {
        println!("Some tests failed.");
    }
    
    success
}

/// Entry point
fn main() {
    let args: Vec<String> = env::args().collect();
    let program_name = args[0].clone();
    
    if args.len() < 2 {
        print_usage(&program_name);
        process::exit(1);
    }
    
    match args[1].as_str() {
        "--help" => {
            print_usage(&program_name);
            process::exit(0);
        },
        "--run-all" => {
            println!("Running all compliance tests...\n");
            if run_all_tests() {
                process::exit(0);
            } else {
                process::exit(1);
            }
        },
        script_path => {
            if script_path.starts_with("-") {
                println!("Unknown option: {}", script_path);
                print_usage(&program_name);
                process::exit(1);
            }
            
            print!("Running script: {}... ", script_path);
            
            match run_script(script_path) {
                Ok(result) => {
                    println!("SUCCESS");
                    println!("Result: {:?}", result);
                    process::exit(0);
                },
                Err(e) => {
                    println!("FAILURE");
                    println!("Error: {}", e);
                    process::exit(1);
                }
            }
        }
    }
}