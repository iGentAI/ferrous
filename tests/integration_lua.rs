//! Integration tests for MLua-based Redis Lua scripting
//! 
//! These tests validate Redis compatibility of our MLua Lua 5.1 implementation

use std::sync::Arc;
use ferrous::storage::engine::StorageEngine;
use ferrous::storage::commands::lua::handle_eval;
use ferrous::protocol::resp::RespFrame;

/// Test Redis EVAL command compatibility
#[test]
fn test_redis_eval_compatibility() {
    let storage = Arc::new(StorageEngine::new_in_memory());
    
    // Test cases that Redis Lua should handle
    let test_cases = vec![
        // Basic return values
        ("return 42", RespFrame::Integer(42)),
        ("return 'hello'", RespFrame::BulkString(Some(Arc::new(b"hello".to_vec())))),
        ("return true", RespFrame::Integer(1)), 
        ("return false", RespFrame::Integer(0)),
        ("return nil", RespFrame::BulkString(None)),
        
        // Arithmetic
        ("return 10 + 5", RespFrame::Integer(15)),
        ("return 20 - 3", RespFrame::Integer(17)),
        ("return 4 * 5", RespFrame::Integer(20)),
        ("return 15 / 3", RespFrame::Integer(5)),
        
        // String operations
        ("return 'hello' .. ' world'", RespFrame::BulkString(Some(Arc::new(b"hello world".to_vec())))),
        ("return string.len('test')", RespFrame::Integer(4)),
        
        // Table operations
        ("local t = {1, 2, 3}; return t[2]", RespFrame::Integer(2)),
        ("local t = {a=5}; return t.a", RespFrame::Integer(5)),
    ];
    
    for (script, expected) in test_cases {
        let parts = create_eval_parts(script, 0, &[], &[]);
        let result = handle_eval(&storage, &parts).unwrap();
        
        match (&result, &expected) {
            (RespFrame::Integer(a), RespFrame::Integer(b)) => assert_eq!(a, b, "Script: {}", script),
            (RespFrame::BulkString(Some(a)), RespFrame::BulkString(Some(b))) => {
                assert_eq!(a.as_ref(), b.as_ref(), "Script: {}", script);
            }
            (RespFrame::BulkString(None), RespFrame::BulkString(None)) => {},
            _ => panic!("Type mismatch for script '{}': got {:?}, expected {:?}", script, result, expected),
        }
    }
}

/// Test KEYS and ARGV Redis compatibility
#[test] 
fn test_redis_keys_argv_compatibility() {
    let storage = Arc::new(StorageEngine::new_in_memory());
    
    // Test KEYS functionality
    let parts = create_eval_parts("return KEYS[1]", 1, &["mykey"], &[]);
    let result = handle_eval(&storage, &parts).unwrap();
    
    match result {
        RespFrame::BulkString(Some(bytes)) => {
            assert_eq!(bytes.as_ref(), b"mykey");
        }
        _ => panic!("Expected KEYS[1] to return 'mykey'"),
    }
    
    // Test ARGV functionality
    let parts = create_eval_parts("return ARGV[1]", 0, &[], &["myvalue"]);
    let result = handle_eval(&storage, &parts).unwrap();
    
    match result {
        RespFrame::BulkString(Some(bytes)) => {
            assert_eq!(bytes.as_ref(), b"myvalue");
        }
        _ => panic!("Expected ARGV[1] to return 'myvalue'"),
    }
    
    // Test multiple KEYS and ARGV
    let parts = create_eval_parts(
        "return {KEYS[1], KEYS[2], ARGV[1], ARGV[2]}",
        2, 
        &["key1", "key2"], 
        &["arg1", "arg2"]
    );
    let result = handle_eval(&storage, &parts).unwrap();
    
    match result {
        RespFrame::Array(Some(items)) => {
            assert_eq!(items.len(), 4);
            // Verify each element
            for (i, expected) in ["key1", "key2", "arg1", "arg2"].iter().enumerate() {
                if let RespFrame::BulkString(Some(bytes)) = &items[i] {
                    assert_eq!(bytes.as_ref(), expected.as_bytes());
                } else {
                    panic!("Expected string at index {}", i);
                }
            }
        }
        _ => panic!("Expected array result"),
    }
}

/// Test Redis Lua sandboxing compliance
#[test] 
fn test_redis_sandboxing_compliance() {
    let storage = Arc::new(StorageEngine::new_in_memory());
    
    // Test that dangerous functions are properly blocked
    let blocked_functions = vec![
        "os.execute('ls')",           // OS access
        "io.open('/tmp/test.txt')",   // File I/O 
        "debug.getinfo(1)",           // Debug introspection
        "package.loadlib('lib')",     // Dynamic loading
        "require('socket')",          // Module loading
        "dofile('/etc/passwd')",      // File execution
        "loadfile('/etc/passwd')",    // File loading
        "load('print(1)')",           // String compilation
    ];
    
    for script in blocked_functions {
        let parts = create_eval_parts(script, 0, &[], &[]);
        let result = handle_eval(&storage, &parts).unwrap();
        
        // All should result in errors due to sandboxing
        match result {
            RespFrame::Error(_) => {}, // Expected - function should be blocked
            _ => panic!("Dangerous function should be blocked: {}", script),
        }
    }
}

/// Test allowed Lua standard library functions
#[test]
fn test_allowed_stdlib_functions() {
    let storage = Arc::new(StorageEngine::new_in_memory());
    
    // Test math library functions that should work (return integers)
    let math_tests = vec![
        ("return math.abs(-5)", 5),
        ("return math.max(1, 3, 2)", 3),
        ("return math.min(1, 3, 2)", 1),
        ("return math.floor(3.7)", 3),
        ("return math.ceil(3.2)", 4),
        ("return string.len('hello')", 5),
    ];
    
    for (script, expected) in math_tests {
        let parts = create_eval_parts(script, 0, &[], &[]);
        let result = handle_eval(&storage, &parts).unwrap();
        
        match result {
            RespFrame::Integer(n) => assert_eq!(n, expected, "Script: {}", script),
            _ => panic!("Expected integer result for: {}", script),
        }
    }
    
    // Test string library functions that return strings
    let string_tests = vec![
        ("return string.upper('test')", "TEST"),
        ("return string.lower('TEST')", "test"),
    ];
    
    for (script, expected) in string_tests {
        let parts = create_eval_parts(script, 0, &[], &[]);
        let result = handle_eval(&storage, &parts).unwrap();
        
        match result {
            RespFrame::BulkString(Some(bytes)) => {
                assert_eq!(String::from_utf8_lossy(&bytes), expected, "Script: {}", script);
            }
            _ => panic!("Expected string '{}' for: {}", expected, script),
        }
    }
}

/// Test redis.call and redis.pcall functionality
#[test]
fn test_redis_call_functions() {
    let storage = Arc::new(StorageEngine::new_in_memory());
    
    // Test redis.call with PING (returns "PONG")
    let parts = create_eval_parts("return redis.call('ping')", 0, &[], &[]);
    let result = handle_eval(&storage, &parts).unwrap();
    
    match result {
        RespFrame::BulkString(Some(bytes)) => {
            assert_eq!(bytes.as_ref(), b"PONG");
        }
        _ => panic!("Expected redis.call to work"),
    }
    
    // Test redis.pcall
    let parts = create_eval_parts("return redis.pcall('ping')", 0, &[], &[]);
    let result = handle_eval(&storage, &parts).unwrap();
    
    match result {
        RespFrame::BulkString(Some(bytes)) => {
            assert_eq!(bytes.as_ref(), b"PONG");
        }
        _ => panic!("Expected redis.pcall to work"),
    }
}

/// Test complex Lua scripts like Redis would encounter
#[test]
fn test_complex_lua_scenarios() {
    let storage = Arc::new(StorageEngine::new_in_memory());
    
    // Test function definitions
    let script = r#"
        local function fibonacci(n)
            if n <= 1 then return n end
            return fibonacci(n-1) + fibonacci(n-2)
        end
        
        return fibonacci(6)
    "#;
    
    let parts = create_eval_parts(script, 0, &[], &[]);
    let result = handle_eval(&storage, &parts).unwrap();
    
    match result {
        RespFrame::Integer(8) => {}, // fibonacci(6) = 8
        _ => panic!("Expected fibonacci(6) = 8"),
    }
    
    // Test table manipulation  
    let script = r#"
        local data = {}
        for i = 1, 5 do
            data[i] = i * i
        end
        
        local sum = 0
        for k, v in pairs(data) do
            sum = sum + v
        end
        
        return sum
    "#;
    
    let parts = create_eval_parts(script, 0, &[], &[]);
    let result = handle_eval(&storage, &parts).unwrap();
    
    match result {
        RespFrame::Integer(55) => {}, // 1^2 + 2^2 + 3^2 + 4^2 + 5^2 = 55
        _ => panic!("Expected sum of squares = 55"),
    }
}

/// Test error handling in Lua scripts
#[test]
fn test_lua_error_handling() {
    let storage = Arc::new(StorageEngine::new_in_memory());
    
    // Test syntax error
    let parts = create_eval_parts("invalid lua syntax {{{", 0, &[], &[]);
    let result = handle_eval(&storage, &parts).unwrap();
    
    match result {
        RespFrame::Error(_) => {}, // Expected
        _ => panic!("Expected syntax error"),
    }
    
    // Test runtime error
    let parts = create_eval_parts("error('test error')", 0, &[], &[]);
    let result = handle_eval(&storage, &parts).unwrap();
    
    match result {
        RespFrame::Error(_) => {}, // Expected
        _ => panic!("Expected runtime error"),
    }
}

/// Test edge cases and Redis-specific behaviors
#[test]
fn test_redis_edge_cases() {
    let storage = Arc::new(StorageEngine::new_in_memory());
    
    // Test empty script
    let parts = create_eval_parts("", 0, &[], &[]);
    let result = handle_eval(&storage, &parts).unwrap();
    
    match result {
        RespFrame::BulkString(None) => {}, // Empty script should return nil
        _ => panic!("Empty script should return nil"),
    }
    
    // Test negative number of keys
    let parts = vec![
        RespFrame::BulkString(Some(Arc::new("EVAL".as_bytes().to_vec()))),
        RespFrame::BulkString(Some(Arc::new("return 1".as_bytes().to_vec()))),
        RespFrame::Integer(-1), // Negative keys
    ];
    
    let result = handle_eval(&storage, &parts).unwrap();
    match result {
        RespFrame::Error(bytes) => {
            assert!(String::from_utf8_lossy(&bytes).contains("negative"));
        }
        _ => panic!("Expected error for negative keys"),
    }
    
    // Test wrong number of arguments
    let parts = vec![
        RespFrame::BulkString(Some(Arc::new("EVAL".as_bytes().to_vec()))),
        // Missing script and numkeys
    ];
    
    let result = handle_eval(&storage, &parts).unwrap();
    match result {
        RespFrame::Error(bytes) => {
            assert!(String::from_utf8_lossy(&bytes).contains("wrong number"));
        }
        _ => panic!("Expected error for wrong arguments"),
    }
}

/// Helper function to create EVAL command parts
fn create_eval_parts(script: &str, num_keys: i64, keys: &[&str], args: &[&str]) -> Vec<RespFrame> {
    let mut parts = vec![
        RespFrame::BulkString(Some(Arc::new("EVAL".as_bytes().to_vec()))),
        RespFrame::BulkString(Some(Arc::new(script.as_bytes().to_vec()))),
        RespFrame::Integer(num_keys),
    ];
    
    for key in keys {
        parts.push(RespFrame::BulkString(Some(Arc::new(key.as_bytes().to_vec()))));
    }
    
    for arg in args {
        parts.push(RespFrame::BulkString(Some(Arc::new(arg.as_bytes().to_vec()))));
    }
    
    parts
}