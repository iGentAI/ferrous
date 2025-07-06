//! Test runner for Lua standard library verification

use ferrous::lua::{compile, LuaVM};
use std::fs;

fn run_test(vm: &mut LuaVM, test_file: &str) -> bool {
    println!("\n=== Running test: {} ===", test_file);
    
    // Read verification script
    match fs::read_to_string(format!("tests/lua/{}.lua", test_file)) {
        Ok(lua_code) => {
            // Compile the script
            match compile(&lua_code) {
                Ok(module) => {
                    // Execute the module
                    match vm.execute_module(&module, &[]) {
                        Ok(result) => {
                            println!("✅ Test passed: {}", test_file);
                            println!("Result: {:?}", result);
                            true
                        }
                        Err(e) => {
                            eprintln!("❌ Test failed: {}", test_file);
                            eprintln!("Error: {}", e);
                            false
                        }
                    }
                },
                Err(e) => {
                    eprintln!("❌ Failed to compile: {}", test_file);
                    eprintln!("Compile error: {}", e);
                    false
                }
            }
        },
        Err(e) => {
            eprintln!("❌ Failed to read test file: {}", test_file);
            eprintln!("IO error: {}", e);
            false
        }
    }
}

fn main() {
    println!("\n=====================================");
    println!("  Lua Standard Library Test Runner");
    println!("=====================================\n");
    
    // Create VM
    let mut vm = LuaVM::new().expect("Failed to create VM");
    
    // Initialize standard library
    println!("Initializing standard library...");
    vm.init_stdlib().expect("Failed to initialize stdlib");
    
    // List of test files to run
    let tests = [
        "minimal_print",
        "minimal_type",
        "minimal_tostring",
        "minimal_metatable",
        "minimal_pairs",
        "minimal_rawops"
    ];
    
    // Track success/failure
    let mut passed = 0;
    let mut failed = 0;
    
    // Run each test
    for test in tests {
        if run_test(&mut vm, test) {
            passed += 1;
        } else {
            failed += 1;
        }
    }
    
    // Print summary
    println!("\n=====================================");
    println!("  Test Summary");
    println!("=====================================");
    println!("Tests passed: {}", passed);
    println!("Tests failed: {}", failed);
    println!("Total tests: {}", tests.len());
    
    // Exit with error if any tests failed
    if failed > 0 {
        std::process::exit(1);
    }
}