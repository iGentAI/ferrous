//! Simple test program for the Lua implementation

use crate::lua_new::{ScriptExecutor, LuaVM, Value, VMConfig};
use crate::storage::engine::StorageEngine;
use crate::protocol::resp::RespFrame;
use std::sync::Arc;

/// Run some simple Lua tests to verify the implementation
pub fn run_simple_lua_tests() -> Result<(), String> {
    println!("Running Lua implementation tests...");
    
    // Create storage engine
    let storage = StorageEngine::new();
    
    // Create script executor
    let script_executor = Arc::new(ScriptExecutor::new(Arc::clone(&storage)));
    
    // Set some test data in Redis
    storage.set_string(0, b"test_key".to_vec(), b"test_value".to_vec())
        .map_err(|e| e.to_string())?;
    
    // Test cases
    let test_cases = [
        // Simple return value
        (
            "return 'hello world'", 
            vec![], 
            vec![], 
            |resp: &RespFrame| -> bool {
                match resp {
                    RespFrame::BulkString(Some(bytes)) => {
                        String::from_utf8_lossy(bytes) == "hello world"
                    },
                    _ => false,
                }
            },
            "Simple string return"
        ),
        
        // Keys and ARGV access
        (
            "return {KEYS[1], ARGV[1]}", 
            vec![b"key1".to_vec()], 
            vec![b"arg1".to_vec()], 
            |resp: &RespFrame| -> bool {
                match resp {
                    RespFrame::Array(Some(items)) => {
                        items.len() == 2 &&
                        match &items[0] {
                            RespFrame::BulkString(Some(bytes)) => {
                                String::from_utf8_lossy(bytes) == "key1"
                            },
                            _ => false,
                        } &&
                        match &items[1] {
                            RespFrame::BulkString(Some(bytes)) => {
                                String::from_utf8_lossy(bytes) == "arg1"
                            },
                            _ => false,
                        }
                    },
                    _ => false,
                }
            },
            "KEYS and ARGV access"
        ),
        
        // Redis API call
        (
            "return redis.call('GET', KEYS[1])", 
            vec![b"test_key".to_vec()], 
            vec![], 
            |resp: &RespFrame| -> bool {
                match resp {
                    RespFrame::BulkString(Some(bytes)) => {
                        String::from_utf8_lossy(bytes) == "test_value"
                    },
                    _ => false,
                }
            },
            "redis.call('GET')"
        ),
        
        // Redis API error handling with pcall
        (
            "return redis.pcall('INVALID_COMMAND')", 
            vec![], 
            vec![], 
            |resp: &RespFrame| -> bool {
                // Should return an error object with err field
                match resp {
                    RespFrame::Array(Some(items)) => {
                        if items.len() != 2 { return false; }
                        
                        match (&items[0], &items[1]) {
                            (
                                RespFrame::BulkString(Some(k)), 
                                RespFrame::BulkString(Some(v))
                            ) => {
                                String::from_utf8_lossy(k) == "err" &&
                                String::from_utf8_lossy(v).contains("unsupported command")
                            },
                            _ => false,
                        }
                    },
                    _ => false,
                }
            },
            "redis.pcall error handling"
        ),
    ];
    
    // Run tests
    let mut passed = 0;
    for (i, (script, keys, args, validator, name)) in test_cases.iter().enumerate() {
        print!("Test {}: {} - ", i+1, name);
        
        // Run script
        match script_executor.eval(script, keys.clone(), args.clone(), 0) {
            Ok(resp) => {
                if validator(&resp) {
                    println!("✅ PASSED");
                    passed += 1;
                } else {
                    println!("❌ FAILED - Unexpected response: {:?}", resp);
                    return Err(format!("Test {} failed: unexpected response", i+1));
                }
            },
            Err(e) => {
                println!("❌ FAILED - Error: {}", e);
                return Err(format!("Test {} failed: {}", i+1, e));
            }
        }
    }
    
    println!("All tests passed! ({}/{})", passed, test_cases.len());
    Ok(())
}

/// Add this to your main.rs to run the tests
pub fn run_lua_tests() {
    match run_simple_lua_tests() {
        Ok(_) => println!("Lua tests completed successfully"),
        Err(e) => eprintln!("Lua tests failed: {}", e),
    }
}