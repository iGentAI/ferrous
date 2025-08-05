//! Redis Lua Script Commands using pipeline-integrated engine
//!
//! Command handlers that use proper command processing pipeline for redis.call()

use std::sync::Arc;
use std::str;
use std::collections::HashMap;

use crate::error::{Result, FerrousError};
use crate::protocol::resp::RespFrame;
use crate::storage::StorageEngine;
use crate::storage::lua_engine::{get_lua_engine, LuaCommandContext};

/// Process KEYS and ARGV from RESP frames
fn process_keys_and_args(parts: &[RespFrame], start_idx: usize, num_keys: usize) -> std::result::Result<(Vec<Vec<u8>>, Vec<Vec<u8>>), String> {
    if parts.len() < start_idx + num_keys {
        return Err("wrong number of arguments".to_string());
    }
    
    let mut keys = Vec::with_capacity(num_keys);
    for i in 0..num_keys {
        match &parts[start_idx + i] {
            RespFrame::BulkString(Some(bytes)) => {
                keys.push(bytes.to_vec());
            }
            _ => {
                return Err("keys must be strings".to_string());
            }
        }
    }
    
    let mut args = Vec::new();
    for i in start_idx + num_keys..parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => {
                args.push(bytes.to_vec());
            }
            _ => {
                return Err("args must be strings".to_string());
            }
        }
    }
    
    Ok((keys, args))
}

/// Handle EVAL command with proper context passing
pub fn handle_eval_with_db(storage: &Arc<StorageEngine>, parts: &[RespFrame], db_index: usize) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'eval' command"));
    }
    
    let script = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            match str::from_utf8(bytes) {
                Ok(s) => s,
                Err(_) => return Ok(RespFrame::error("ERR invalid script - not valid UTF-8")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid script")),
    };
    
    let num_keys = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            match str::from_utf8(bytes) {
                Ok(s) => match s.parse::<usize>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR invalid number of keys")),
                },
                Err(_) => return Ok(RespFrame::error("ERR invalid number of keys")),
            }
        }
        RespFrame::Integer(n) => {
            if *n < 0 {
                return Ok(RespFrame::error("ERR negative number of keys is invalid"));
            }
            *n as usize
        }
        _ => return Ok(RespFrame::error("ERR invalid number of keys")),
    };
    
    let (keys, args) = match process_keys_and_args(parts, 3, num_keys) {
        Ok((k, a)) => (k, a),
        Err(e) => return Ok(RespFrame::error(format!("ERR {}", e))),
    };
    
    // Create command context with proper database index
    let ctx = LuaCommandContext {
        db_index,
        storage: storage.clone(),
    };
    
    let lua_engine = match get_lua_engine(storage.clone()) {
        Ok(engine) => engine,
        Err(e) => return Ok(RespFrame::error(format!("ERR {}", e))),
    };
    
    match lua_engine.eval(script, keys, args, &ctx) {
        Ok(response) => Ok(response),
        Err(e) => {
            let error_msg = match e {
                FerrousError::LuaError(ref msg) => {
                    // Check if this contains REDIS_CALL_ABORT (should be handled by lua_engine.rs)
                    if msg.contains("REDIS_CALL_ABORT:") {
                        // Extract the clean error from the wrapped message
                        if let Some(pos) = msg.find("REDIS_CALL_ABORT:") {
                            let error_content = &msg[pos + "REDIS_CALL_ABORT:".len()..];
                            let end_pos = error_content.find('\n').unwrap_or(error_content.len());
                            let clean_error = error_content[..end_pos].trim();
                            
                            if clean_error.starts_with("ERR ") {
                                clean_error.to_string()
                            } else {
                                format!("ERR {}", clean_error)
                            }
                        } else {
                            if msg.starts_with("ERR ") { msg.clone() } else { format!("ERR {}", msg) }
                        }
                    } else {
                        if msg.starts_with("ERR ") {
                            msg.clone()
                        } else {
                            format!("ERR {}", msg)
                        }
                    }
                }
                _ => {
                    format!("ERR {}", e)
                },
            };
            
            Ok(RespFrame::error(error_msg))
        }
    }
}

/// Handle EVAL command (wrapper for compatibility)
pub fn handle_eval(storage: &Arc<StorageEngine>, parts: &[RespFrame]) -> Result<RespFrame> {
    handle_eval_with_db(storage, parts, 0) // Default to database 0
}

/// Handle EVALSHA command with proper context
pub fn handle_evalsha_with_db(storage: &Arc<StorageEngine>, parts: &[RespFrame], db_index: usize) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'evalsha' command"));
    }
    
    let sha1 = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            match str::from_utf8(bytes) {
                Ok(s) => s,
                Err(_) => return Ok(RespFrame::error("ERR invalid SHA1 hash")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid SHA1 hash")),
    };
    
    let num_keys = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            match str::from_utf8(bytes) {
                Ok(s) => match s.parse::<usize>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR invalid number of keys")),
                },
                Err(_) => return Ok(RespFrame::error("ERR invalid number of keys")),
            }
        }
        RespFrame::Integer(n) => {
            if *n < 0 {
                return Ok(RespFrame::error("ERR negative number of keys is invalid"));
            }
            *n as usize
        }
        _ => return Ok(RespFrame::error("ERR invalid number of keys")),
    };
    
    let (keys, args) = match process_keys_and_args(parts, 3, num_keys) {
        Ok((k, a)) => (k, a),
        Err(e) => return Ok(RespFrame::error(format!("ERR {}", e))),
    };
    
    // Create command context
    let ctx = LuaCommandContext {
        db_index,
        storage: storage.clone(),
    };
    
    let lua_engine = match get_lua_engine(storage.clone()) {
        Ok(engine) => engine,
        Err(e) => return Ok(RespFrame::error(format!("ERR {}", e))),
    };
    
    match lua_engine.evalsha(sha1, keys, args, &ctx) {
        Ok(response) => Ok(response),
        Err(e) => {
            let error_msg = match e {
                FerrousError::LuaError(ref msg) => {
                    // Check if this contains REDIS_CALL_ABORT (should be handled by lua_engine.rs)
                    if msg.contains("REDIS_CALL_ABORT:") {
                        // Extract the clean error from the wrapped message
                        if let Some(pos) = msg.find("REDIS_CALL_ABORT:") {
                            let error_content = &msg[pos + "REDIS_CALL_ABORT:".len()..];
                            let end_pos = error_content.find('\n').unwrap_or(error_content.len());
                            let clean_error = error_content[..end_pos].trim();
                            
                            if clean_error.starts_with("ERR ") {
                                clean_error.to_string()
                            } else {
                                format!("ERR {}", clean_error)
                            }
                        } else {
                            if msg.starts_with("ERR ") { msg.clone() } else { format!("ERR {}", msg) }
                        }
                    } else {
                        if msg.starts_with("ERR ") {
                            msg.clone()
                        } else {
                            format!("ERR {}", msg)
                        }
                    }
                }
                _ => {
                    format!("ERR {}", e)
                },
            };
            
            Ok(RespFrame::error(error_msg))
        }
    }
}

/// Handle EVALSHA command (wrapper for compatibility) 
pub fn handle_evalsha(storage: &Arc<StorageEngine>, parts: &[RespFrame]) -> Result<RespFrame> {
    handle_evalsha_with_db(storage, parts, 0) // Default to database 0
}

/// Handle SCRIPT LOAD command
pub fn handle_script_load(storage: &Arc<StorageEngine>, parts: &[RespFrame]) -> Result<(String, String)> {
    if parts.len() != 2 {
        return Err(FerrousError::LuaError("wrong number of arguments for 'script load' command".to_string()));
    }
    
    let script = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            match str::from_utf8(bytes) {
                Ok(s) => s,
                Err(_) => return Err(FerrousError::LuaError("invalid script - not valid UTF-8".to_string())),
            }
        }
        _ => return Err(FerrousError::LuaError("invalid script".to_string())),
    };
    
    let lua_engine = match get_lua_engine(storage.clone()) {
        Ok(engine) => engine,
        Err(e) => return Err(e),
    };
    
    match lua_engine.script_load(script) {
        Ok(sha1) => Ok((sha1, script.to_string())),
        Err(e) => Err(e),
    }
}

/// Handle all Lua commands through the pipeline architecture
pub fn handle_lua_command_with_cache(
    storage: &Arc<StorageEngine>, 
    cmd: &str, 
    parts: &[RespFrame],
    _script_cache: &mut HashMap<String, String> // Unused - engine has its own cache
) -> Result<RespFrame> {
    match cmd.to_lowercase().as_str() {
        "eval" => handle_eval(storage, parts),
        "evalsha" => handle_evalsha(storage, parts),
        "script" => {
            if parts.len() < 2 {
                return Ok(RespFrame::error("ERR wrong number of arguments for 'script' command"));
            }
            
            let subcommand = match &parts[1] {
                RespFrame::BulkString(Some(bytes)) => {
                    match str::from_utf8(bytes) {
                        Ok(s) => s.to_lowercase(),
                        Err(_) => return Ok(RespFrame::error("ERR invalid subcommand")),
                    }
                }
                _ => return Ok(RespFrame::error("ERR invalid subcommand")),
            };
            
            let lua_engine = match get_lua_engine(storage.clone()) {
                Ok(engine) => engine,
                Err(e) => return Ok(RespFrame::error(format!("ERR {}", e))),
            };
            
            match subcommand.as_str() {
                "load" => {
                    if parts.len() != 3 {
                        return Ok(RespFrame::error("ERR wrong number of arguments for 'script load' command"));
                    }
                    
                    let script = match &parts[2] {
                        RespFrame::BulkString(Some(bytes)) => {
                            match str::from_utf8(bytes) {
                                Ok(s) => s,
                                Err(_) => return Ok(RespFrame::error("ERR invalid script - not valid UTF-8")),
                            }
                        }
                        _ => return Ok(RespFrame::error("ERR invalid script")),
                    };
                    
                    match lua_engine.script_load(script) {
                        Ok(sha1) => Ok(RespFrame::bulk_string(sha1)),
                        Err(e) => Ok(RespFrame::error(format!("ERR {}", e))),
                    }
                },
                "exists" => {
                    if parts.len() < 3 {
                        return Ok(RespFrame::error("ERR wrong number of arguments for 'script exists' command"));
                    }
                    
                    let mut sha1s = Vec::new();
                    for i in 2..parts.len() {
                        let sha1 = match &parts[i] {
                            RespFrame::BulkString(Some(bytes)) => {
                                match str::from_utf8(bytes) {
                                    Ok(s) => s.to_string(),
                                    Err(_) => continue,
                                }
                            }
                            _ => continue,
                        };
                        sha1s.push(sha1);
                    }
                    
                    match lua_engine.script_exists(&sha1s) {
                        Ok(results) => {
                            let resp_results: Vec<RespFrame> = results.into_iter()
                                .map(|exists| RespFrame::Integer(if exists { 1 } else { 0 }))
                                .collect();
                            Ok(RespFrame::Array(Some(resp_results)))
                        }
                        Err(e) => Ok(RespFrame::error(format!("ERR {}", e))),
                    }
                },
                "flush" => {
                    if parts.len() != 2 {
                        return Ok(RespFrame::error("ERR wrong number of arguments for 'script flush' command"));
                    }
                    
                    match lua_engine.script_flush() {
                        Ok(_) => Ok(RespFrame::SimpleString(Arc::new(b"OK".to_vec()))),
                        Err(e) => Ok(RespFrame::error(format!("ERR {}", e))),
                    }
                },
                "kill" => {
                    if parts.len() != 2 {
                        return Ok(RespFrame::error("ERR wrong number of arguments for 'script kill' command"));
                    }
                    Ok(RespFrame::SimpleString(Arc::new(b"OK".to_vec())))
                },
                _ => Ok(RespFrame::error(format!("ERR Unknown subcommand '{}'", subcommand))),
            }
        },
        _ => Ok(RespFrame::error(format!("ERR unknown command '{}'", cmd))),
    }
}

#[deprecated(note = "Use handle_lua_command_with_cache")]
pub fn handle_lua_command(storage: &Arc<StorageEngine>, cmd: &str, parts: &[RespFrame]) -> Result<RespFrame> {
    let mut unused_cache = HashMap::new();
    handle_lua_command_with_cache(storage, cmd, parts, &mut unused_cache)
}

// Restore test coverage for Lua command handlers
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_eval_with_db() {
        let storage = Arc::new(StorageEngine::new_in_memory());
        let parts = vec![
            RespFrame::BulkString(Some(Arc::new("EVAL".as_bytes().to_vec()))),
            RespFrame::BulkString(Some(Arc::new("return 'hello'".as_bytes().to_vec()))),
            RespFrame::Integer(0),
        ];
        
        let result = handle_eval_with_db(&storage, &parts, 0).unwrap();
        match result {
            RespFrame::BulkString(Some(bytes)) => {
                assert_eq!(String::from_utf8_lossy(&bytes), "hello");
            }
            _ => panic!("Expected bulk string result, got: {:?}", result),
        }
    }
    
    #[test]
    fn test_lua_arithmetic_with_db() {
        let storage = Arc::new(StorageEngine::new_in_memory());
        let parts = vec![
            RespFrame::BulkString(Some(Arc::new("EVAL".as_bytes().to_vec()))),
            RespFrame::BulkString(Some(Arc::new("return 5 + 3".as_bytes().to_vec()))),
            RespFrame::Integer(0),
        ];
        
        let result = handle_eval_with_db(&storage, &parts, 0).unwrap();
        match result {
            RespFrame::Integer(8) => {},
            _ => panic!("Expected integer 8, got: {:?}", result),
        }
    }

    #[test] 
    fn test_keys_and_argv_with_db() {
        let storage = Arc::new(StorageEngine::new_in_memory());
        let parts = vec![
            RespFrame::BulkString(Some(Arc::new("EVAL".as_bytes().to_vec()))),
            RespFrame::BulkString(Some(Arc::new("return KEYS[1] .. ':' .. ARGV[1]".as_bytes().to_vec()))),
            RespFrame::Integer(1),
            RespFrame::BulkString(Some(Arc::new("mykey".as_bytes().to_vec()))),
            RespFrame::BulkString(Some(Arc::new("myvalue".as_bytes().to_vec()))),
        ];
        
        let result = handle_eval_with_db(&storage, &parts, 0).unwrap();
        match result {
            RespFrame::BulkString(Some(bytes)) => {
                assert_eq!(String::from_utf8_lossy(&bytes), "mykey:myvalue");
            }
            _ => panic!("Expected concatenated string result, got: {:?}", result),
        }
    }
    
    #[test]
    fn test_redis_call_pipeline_integration() {
        let storage = Arc::new(StorageEngine::new_in_memory());
        
        // Test that redis.call() operations work with proper context
        let parts = vec![
            RespFrame::BulkString(Some(Arc::new("EVAL".as_bytes().to_vec()))),
            RespFrame::BulkString(Some(Arc::new("return redis.call('SET', 'test_key', 'test_value')".as_bytes().to_vec()))),
            RespFrame::Integer(0),
        ];
        
        let result = handle_eval_with_db(&storage, &parts, 0).unwrap();
        match result {
            RespFrame::BulkString(Some(bytes)) => {
                assert_eq!(String::from_utf8_lossy(&bytes), "OK");
            }
            _ => panic!("Expected 'OK' result from SET, got: {:?}", result),
        }
        
        // Verify the value was actually set
        match storage.get_string(0, b"test_key").unwrap() {
            Some(value) => {
                assert_eq!(String::from_utf8_lossy(&value), "test_value");
            }
            None => panic!("Key was not set by redis.call"),
        }
    }
    
    #[test]
    fn test_atomic_lock_release_with_pipeline() {
        let storage = Arc::new(StorageEngine::new_in_memory());
        
        // Set up the lock
        storage.set_string(0, b"test_lock".to_vec(), b"unique_value".to_vec()).unwrap();
        
        // Test atomic lock release script using pipeline
        let script = r#"
            if redis.call("GET", KEYS[1]) == ARGV[1] then
                return redis.call("DEL", KEYS[1])
            else
                return 0
            end
        "#;
        
        let parts_correct = vec![
            RespFrame::BulkString(Some(Arc::new("EVAL".as_bytes().to_vec()))),
            RespFrame::BulkString(Some(Arc::new(script.as_bytes().to_vec()))),
            RespFrame::Integer(1),
            RespFrame::BulkString(Some(Arc::new("test_lock".as_bytes().to_vec()))),
            RespFrame::BulkString(Some(Arc::new("unique_value".as_bytes().to_vec()))),
        ];
        
        let result = handle_eval_with_db(&storage, &parts_correct, 0).unwrap();
        match result {
            RespFrame::Integer(1) => {}, // Should delete the key
            _ => panic!("Expected atomic lock release to return 1, got: {:?}", result),
        }
        
        // Verify lock is gone
        assert!(storage.get_string(0, b"test_lock").unwrap().is_none());
        
        // Test with wrong value
        storage.set_string(0, b"test_lock".to_vec(), b"unique_value".to_vec()).unwrap();
        
        let parts_wrong = vec![
            RespFrame::BulkString(Some(Arc::new("EVAL".as_bytes().to_vec()))),
            RespFrame::BulkString(Some(Arc::new(script.as_bytes().to_vec()))),
            RespFrame::Integer(1),
            RespFrame::BulkString(Some(Arc::new("test_lock".as_bytes().to_vec()))),
            RespFrame::BulkString(Some(Arc::new("wrong_value".as_bytes().to_vec()))),
        ];
        
        let result = handle_eval_with_db(&storage, &parts_wrong, 0).unwrap();
        match result {
            RespFrame::Integer(0) => {}, // Should not delete when wrong value
            _ => panic!("Expected atomic lock release to return 0 for wrong value, got: {:?}", result),
        }
        
        // Verify lock still exists
        assert!(storage.get_string(0, b"test_lock").unwrap().is_some());
    }
}