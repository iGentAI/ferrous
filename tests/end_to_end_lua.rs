//! End-to-end integration tests for MLua Redis Lua scripting
//! 
//! Tests the complete pipeline from command parsing to script execution

use std::sync::Arc;
use ferrous::storage::engine::StorageEngine;
use ferrous::network::server::Server;
use ferrous::protocol::resp::RespFrame;
use ferrous::config::Config;

/// Test the complete Redis EVAL command pipeline
#[test]
fn test_eval_command_pipeline() {
    // This would test the full server integration
    // For now, test the core functionality
    let storage = Arc::new(StorageEngine::new_in_memory());
    
    // Simulate a complete Redis EVAL command
    let script = r#"
        -- Redis Lua script example
        local key = KEYS[1]
        local value = ARGV[1]
        
        return "Key: " .. key .. ", Value: " .. value
    "#;
    
    let parts = vec![
        RespFrame::BulkString(Some(Arc::new("EVAL".as_bytes().to_vec()))),
        RespFrame::BulkString(Some(Arc::new(script.as_bytes().to_vec()))),
        RespFrame::Integer(1), // 1 key
        RespFrame::BulkString(Some(Arc::new("mykey".as_bytes().to_vec()))),
        RespFrame::BulkString(Some(Arc::new("myvalue".as_bytes().to_vec()))),
    ];
    
    let result = ferrous::storage::commands::lua::handle_lua_command(&storage, "eval", &parts).unwrap();
    
    match result {
        RespFrame::BulkString(Some(bytes)) => {
            let response = String::from_utf8_lossy(&bytes);
            assert_eq!(response, "Key: mykey, Value: myvalue");
        }
        _ => panic!("Expected concatenated string result"),
    }
}

/// Test Redis command validation and error handling
#[test]
fn test_command_validation() {
    let storage = Arc::new(StorageEngine::new_in_memory());
    
    // Test invalid command
    let parts = vec![RespFrame::BulkString(Some(Arc::new("INVALID".as_bytes().to_vec())))];
    let result = ferrous::storage::commands::lua::handle_lua_command(&storage, "invalid", &parts).unwrap();
    
    match result {
        RespFrame::Error(bytes) => {
            assert!(String::from_utf8_lossy(&bytes).contains("unknown command"));
        }
        _ => panic!("Expected error for invalid command"),
    }
    
    // Test malformed EVAL
    let parts = vec![
        RespFrame::BulkString(Some(Arc::new("EVAL".as_bytes().to_vec()))),
        // Missing required parameters
    ];
    
    let result = ferrous::storage::commands::lua::handle_lua_command(&storage, "eval", &parts).unwrap();
    match result {
        RespFrame::Error(bytes) => {
            assert!(String::from_utf8_lossy(&bytes).contains("wrong number"));
        }
        _ => panic!("Expected error for malformed EVAL"),
    }
}

/// Test performance and resource management
#[test]
fn test_performance_characteristics() {
    let storage = Arc::new(StorageEngine::new_in_memory());
    
    // Test script performance
    let script = r#"
        local sum = 0
        for i = 1, 1000 do
            sum = sum + i
        end
        return sum
    "#;
    
    let parts = vec![
        RespFrame::BulkString(Some(Arc::new("EVAL".as_bytes().to_vec()))),
        RespFrame::BulkString(Some(Arc::new(script.as_bytes().to_vec()))),
        RespFrame::Integer(0),
    ];
    
    let start = std::time::Instant::now();
    let result = ferrous::storage::commands::lua::handle_lua_command(&storage, "eval", &parts).unwrap();
    let elapsed = start.elapsed();
    
    // Should complete quickly
    assert!(elapsed.as_millis() < 100, "Script execution too slow: {:?}", elapsed);
    
    match result {
        RespFrame::Integer(500500) => {}, // Sum 1 to 1000
        _ => panic!("Expected correct sum calculation"),
    }
}

/// Test various Lua data types and Redis conversion
#[test]
fn test_lua_redis_type_conversion() {
    let storage = Arc::new(StorageEngine::new_in_memory());
    
    let test_cases = vec![
        // Test different return types
        ("return nil", "nil"),
        ("return true", "true->1"),
        ("return false", "false->nil"),
        ("return 42", "integer"),
        ("return 3.14", "float->string"),
        ("return 'string'", "string"),
        ("return {1, 2, 3}", "array"),
        ("return {}", "empty_table->nil"),
    ];
    
    for (script, description) in test_cases {
        let parts = vec![
            RespFrame::BulkString(Some(Arc::new("EVAL".as_bytes().to_vec()))),
            RespFrame::BulkString(Some(Arc::new(script.as_bytes().to_vec()))),
            RespFrame::Integer(0),
        ];
        
        let result = ferrous::storage::commands::lua::handle_lua_command(&storage, "eval", &parts);
        
        // All should succeed without errors
        assert!(result.is_ok(), "Script '{}' ({}) failed: {:?}", script, description, result.err());
        
        println!("✓ {} - {}: {:?}", script, description, result.unwrap());
    }
}

#[test]
fn test_concurrent_script_execution() {
    // Simple test to ensure scripts can be executed concurrently
    let storage = Arc::new(StorageEngine::new_in_memory());
    
    let handles: Vec<_> = (0..5).map(|i| {
        let storage = storage.clone();
        std::thread::spawn(move || {
            let script = format!("return {}", i);
            let parts = vec![
                RespFrame::BulkString(Some(Arc::new("EVAL".as_bytes().to_vec()))),
                RespFrame::BulkString(Some(Arc::new(script.as_bytes().to_vec()))),
                RespFrame::Integer(0),
            ];
            
            ferrous::storage::commands::lua::handle_lua_command(&storage, "eval", &parts)
        })
    }).collect();
    
    for handle in handles {
        let result = handle.join().unwrap().unwrap();
        // Should all succeed
        match result {
            RespFrame::Integer(_) => {},
            _ => panic!("Expected integer result"),
        }
    }
}