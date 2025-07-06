//! Minimal Test for the New Lua VM Implementation
//!
//! This test validates that our non-recursive Lua VM can compile and execute
//! simple Lua code correctly.

use ferrous::lua::{LuaVM, Value, compiler};
use ferrous::error::FerrousError;
use std::sync::Arc;
use std::time::Duration;
use ferrous::storage::StorageEngine;
use ferrous::protocol::resp::RespFrame;

#[test]
fn test_minimal_vm_execution() {
    // Create a new Lua VM
    let mut vm = LuaVM::new().unwrap();
    
    // Initialize standard library
    vm.init_stdlib().unwrap();
    
    // Compile a simple Lua script
    let source = "return 'hello world'";
    let module = compiler::compile(source).unwrap();
    
    // Execute the module
    let result = vm.execute_module(&module, &[]).unwrap();
    
    // Check result
    if let Value::String(handle) = result {
        let value = vm.heap.get_string_value(handle).unwrap();
        assert_eq!(value, "hello world");
    } else {
        panic!("Expected string result, got {:?}", result);
    }
}

#[test]
fn test_arithmetic() {
    // Create a new Lua VM
    let mut vm = LuaVM::new().unwrap();
    
    // Initialize standard library
    vm.init_stdlib().unwrap();
    
    // Test addition
    let source = "return 40 + 2";
    let module = compiler::compile(source).unwrap();
    let result = vm.execute_module(&module, &[]).unwrap();
    
    if let Value::Number(n) = result {
        assert_eq!(n, 42.0);
    } else {
        panic!("Expected number result, got {:?}", result);
    }
    
    // Test multiplication
    let source = "return 6 * 7";
    let module = compiler::compile(source).unwrap();
    let result = vm.execute_module(&module, &[]).unwrap();
    
    if let Value::Number(n) = result {
        assert_eq!(n, 42.0);
    } else {
        panic!("Expected number result, got {:?}", result);
    }
}

#[test]
fn test_string_functions() {
    // Create a new Lua VM
    let mut vm = LuaVM::new().unwrap();
    
    // Initialize standard library
    vm.init_stdlib().unwrap();
    
    // Test string.upper
    let source = "return string.upper('hello')";
    let module = compiler::compile(source).unwrap();
    let result = vm.execute_module(&module, &[]).unwrap();
    
    if let Value::String(handle) = result {
        let value = vm.heap.get_string_value(handle).unwrap();
        assert_eq!(value, "HELLO");
    } else {
        panic!("Expected string result, got {:?}", result);
    }
}

#[test]
fn test_global_variables() {
    // Create a new Lua VM
    let mut vm = LuaVM::new().unwrap();
    
    // Initialize standard library
    vm.init_stdlib().unwrap();
    
    // Execute code to set a global variable
    let source = "x = 42; return x";
    let module = compiler::compile(source).unwrap();
    let result = vm.execute_module(&module, &[]).unwrap();
    
    if let Value::Number(n) = result {
        assert_eq!(n, 42.0);
    } else {
        panic!("Expected number result, got {:?}", result);
    }
}

#[test]
fn test_table_operations() {
    // Create a new Lua VM
    let mut vm = LuaVM::new().unwrap();
    
    // Initialize standard library
    vm.init_stdlib().unwrap();
    
    // Test table creation and access
    let source = "local t = {10, 20, 30}; return t[2]";
    let module = compiler::compile(source).unwrap();
    let result = vm.execute_module(&module, &[]).unwrap();
    
    if let Value::Number(n) = result {
        assert_eq!(n, 20.0);
    } else {
        panic!("Expected number result, got {:?}", result);
    }
}

#[test]
fn test_script_timeout() {
    // Create a test storage engine
    let storage = Arc::new(StorageEngine::new_in_memory());
    
    // Create script context with a very short timeout
    let context = ferrous::lua::ScriptContext {
        storage: storage.clone(),
        db: 0,
        keys: vec![],
        args: vec![],
        timeout: Duration::from_nanos(1), // Extremely short timeout
    };
    
    // Create Lua interpreter
    let gil = ferrous::lua::LuaGIL::new().unwrap();
    
    // Execute an infinite loop script
    let source = "while true do end";
    let result = gil.eval(source, context);
    
    // Should timeout
    assert!(matches!(result, Err(FerrousError::ScriptTimeout)));
}

#[test]
fn test_redis_call() {
    // Create a new Lua VM
    let mut vm = LuaVM::new().unwrap();
    
    // Initialize standard library
    vm.init_stdlib().unwrap();
    
    // Create test storage engine
    let storage = Arc::new(StorageEngine::new_in_memory());
    
    // Create Redis context
    let redis_ctx = ferrous::lua::redis_api::RedisContext {
        storage: storage.clone(),
        db: 0,
        keys: vec![b"key1".to_vec()],
        args: vec![b"arg1".to_vec()],
    };
    
    // Register Redis API
    ferrous::lua::redis_api::register_redis_api(&mut vm, redis_ctx).unwrap();
    
    // Test accessing KEYS table
    let source = "return KEYS[1]";
    let module = compiler::compile(source).unwrap();
    let result = vm.execute_module(&module, &[]).unwrap();
    
    if let Value::String(handle) = result {
        let value = vm.heap.get_string_value(handle).unwrap();
        assert_eq!(value, "key1");
    } else {
        panic!("Expected string result, got {:?}", result);
    }
}