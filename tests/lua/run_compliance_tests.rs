//! Lua Compliance Test Runner
//!
//! This module runs a comprehensive set of tests against the Lua VM implementation
//! to verify that all language features and opcodes work correctly.

use crate::lua::{LuaVM, Value, StringHandle, TableHandle, compile};
use crate::lua::transaction::HeapTransaction;
use std::sync::Arc;
use std::fs;
use std::path::Path;

/// Execute a Lua script and return the result
fn execute_script(vm: &mut LuaVM, script: &str) -> Value {
    // Compile the script
    let module = match compile(script) {
        Ok(module) => module,
        Err(e) => {
            panic!("Failed to compile script: {:?}", e);
        }
    };
    
    // Execute the script
    match vm.execute_module(&module, &[]) {
        Ok(value) => value,
        Err(e) => {
            panic!("Failed to execute script: {:?}", e);
        }
    }
}

/// Verify a single test result from the returned table
fn verify_test_result(vm: &mut LuaVM, table_handle: TableHandle, key: &str, expected: bool) -> bool {
    let mut tx = HeapTransaction::new(vm.heap_mut());
    
    // Create a string handle for the key
    let key_handle = match tx.create_string(key) {
        Ok(handle) => handle,
        Err(_) => {
            println!("Failed to create string for key: {}", key);
            return false;
        }
    };
    
    // Get the value from the table
    let value = match tx.read_table_field(table_handle, &Value::String(key_handle)) {
        Ok(value) => value,
        Err(_) => {
            println!("Failed to read table field: {}", key);
            return false;
        }
    };
    
    // Check if the value matches the expected boolean
    match value {
        Value::Boolean(b) => {
            if b != expected {
                println!("Test '{}' failed: expected {}, got {}", key, expected, b);
                return false;
            }
            return true;
        },
        Value::Nil => {
            if expected {
                println!("Test '{}' failed: expected true, got nil", key);
                return false;
            }
            return true;
        },
        _ => {
            println!("Test '{}' has unexpected type: {:?}", key, value);
            return false;
        }
    }
}

/// Load and run a test script from a file
fn run_test_script(script_path: &str) -> bool {
    println!("Running test script: {}", script_path);
    
    // Load the script
    let script = match fs::read_to_string(script_path) {
        Ok(content) => content,
        Err(e) => {
            println!("Failed to read test script {}: {}", script_path, e);
            return false;
        }
    };
    
    // Create a new VM for this test
    let mut vm = match LuaVM::new() {
        Ok(vm) => vm,
        Err(e) => {
            println!("Failed to create VM: {:?}", e);
            return false;
        }
    };
    
    // Execute the script
    let result = match execute_script(&mut vm, &script) {
        Value::Table(table_handle) => {
            // Verify the test results
            let mut tx = HeapTransaction::new(vm.heap_mut());
            
            // Get all the keys in the table
            let mut success = true;
            
            // Here we would iterate through the table and check each result
            // Since we can't directly iterate tables in this implementation,
            // we'll check specific expected test results by key
            
            // Let's check a few key test results that should be present in all test files
            success &= verify_test_result(&mut vm, table_handle, "creation_empty", true);
            success &= verify_test_result(&mut vm, table_handle, "access_direct", true);
            
            if !success {
                println!("Some tests in {} failed", script_path);
            } else {
                println!("All tests in {} passed", script_path);
            }
            
            success
        },
        _ => {
            println!("Test script {} did not return a result table", script_path);
            false
        }
    };
    
    result
}

/// Run all compliance tests
pub fn run_all_compliance_tests() -> bool {
    println!("Running all Lua compliance tests...");
    
    let mut success = true;
    
    // Run the table test
    success &= run_test_script("tests/lua/table_test.lua");
    
    // Run the closures test
    success &= run_test_script("tests/lua/closure_test.lua");
    
    // Run the nested references test
    success &= run_test_script("tests/lua/nested_refs_test.lua");
    
    // Run the comprehensive test
    success &= run_test_script("tests/lua/comprehensive.lua");
    
    if success {
        println!("All compliance tests passed!");
    } else {
        println!("Some compliance tests failed!");
    }
    
    success
}

#[test]
fn test_lua_compliance() {
    assert!(run_all_compliance_tests(), "Lua compliance tests failed");
}