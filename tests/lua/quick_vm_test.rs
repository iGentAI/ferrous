//! Quick Test for the Lua Unified Stack VM Implementation
//!
//! This test validates that our VM implementation can compile and execute
//! the simplest possible Lua scripts.

use std::fs;
use std::path::Path;
use ferrous::lua::{LuaVM, Value};
use ferrous::lua::compiler;

#[test]
fn test_minimal_script() {
    // Load the minimal.lua test script
    let script_path = Path::new("tests/lua/minimal.lua");
    let source = fs::read_to_string(script_path).expect("Failed to read test script");
    
    // Create and initialize VM
    let mut vm = LuaVM::new().unwrap();
    vm.init_stdlib().unwrap();
    
    // Compile script
    let module = compiler::compile(&source).unwrap();
    
    // Execute module
    let result = vm.execute_module(&module, &[]).unwrap();
    
    // Verify result
    match result {
        Value::Number(n) => {
            assert_eq!(n, 42.0, "Expected result to be 42");
        },
        _ => panic!("Expected number result, got {:?}", result),
    }
}

#[test]
fn test_simple_arithmetic() {
    // Load the simple_test.lua script
    let script_path = Path::new("tests/lua/simple_test.lua");
    let source = fs::read_to_string(script_path).expect("Failed to read test script");
    
    // Create and initialize VM
    let mut vm = LuaVM::new().unwrap();
    vm.init_stdlib().unwrap();
    
    // Compile script
    let module = compiler::compile(&source).unwrap();
    
    // Execute module
    let result = vm.execute_module(&module, &[]).unwrap();
    
    // Verify result
    match result {
        Value::Number(n) => {
            assert_eq!(n, 142.0, "Expected result to be 142 (42 + 100)");
        },
        _ => panic!("Expected number result, got {:?}", result),
    }
}