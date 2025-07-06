//! Redis Lua Script Commands Implementation
//!
//! This module implements the Redis commands for running Lua scripts:
//! - EVAL
//! - EVALSHA
//! - SCRIPT LOAD
//! - SCRIPT FLUSH
//! - SCRIPT EXISTS
//! - SCRIPT KILL

use std::sync::Arc;
use std::time::Duration;
use std::str;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Instant;
use std::sync::Mutex;
use std::sync::Once;

use crate::error::{Result, FerrousError};
use crate::lua::{LuaGIL, ScriptContext};
use crate::protocol::resp::RespFrame;
use crate::storage::StorageEngine;

// Singleton Lua interpreter with lazy initialization
static LUA_INIT: Once = Once::new();
static mut LUA_INTERPRETER: Option<Arc<LuaGIL>> = None;

/// Get or initialize the Lua interpreter singleton
fn get_lua_interpreter() -> std::result::Result<Arc<LuaGIL>, FerrousError> {
    unsafe {
        LUA_INIT.call_once(|| {
            match LuaGIL::new() {
                Ok(gil) => {
                    LUA_INTERPRETER = Some(Arc::new(gil));
                }
                Err(e) => {
                    eprintln!("Failed to initialize Lua interpreter: {}", e);
                    // We can't return an error from a Once callback, so we leave the interpreter as None
                    // and handle this case below
                }
            }
        });
        
        match &LUA_INTERPRETER {
            Some(gil) => Ok(gil.clone()),
            None => Err(FerrousError::LuaError("Failed to initialize Lua interpreter".to_string())),
        }
    }
}

/// Process key and args from command arguments
fn process_keys_and_args(parts: &[RespFrame], start_idx: usize, num_keys: usize) -> std::result::Result<(Vec<Vec<u8>>, Vec<Vec<u8>>), FerrousError> {
    // Check if we have enough arguments
    if parts.len() < start_idx + num_keys {
        return Err(FerrousError::WrongNumberOfArguments("eval/evalsha".to_string()));
    }
    
    // Extract keys
    let mut keys = Vec::with_capacity(num_keys);
    for i in 0..num_keys {
        match &parts[start_idx + i] {
            RespFrame::BulkString(Some(bytes)) => {
                keys.push(bytes.to_vec());
            }
            _ => {
                return Err(FerrousError::InvalidArgument("keys must be strings".to_string()));
            }
        }
    }
    
    // Extract args
    let mut args = Vec::new();
    for i in start_idx + num_keys..parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => {
                args.push(bytes.to_vec());
            }
            _ => {
                return Err(FerrousError::InvalidArgument("args must be strings".to_string()));
            }
        }
    }
    
    Ok((keys, args))
}

/// EVAL script numkeys [key1...] [arg1...]
/// 
/// Evaluate a Lua script server side
pub fn handle_eval(storage: &Arc<StorageEngine>, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'eval' command"));
    }
    
    // Get script string
    let script = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            match str::from_utf8(bytes) {
                Ok(s) => s.to_string(),
                Err(_) => return Ok(RespFrame::error("ERR invalid script - not valid UTF-8")),
            }
        }
        _ => {
            return Ok(RespFrame::error("ERR invalid script"));
        }
    };
    
    // Get number of keys
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
        _ => {
            return Ok(RespFrame::error("ERR invalid number of keys"));
        }
    };
    
    // Process keys and args
    let (keys, args) = match process_keys_and_args(parts, 3, num_keys) {
        Ok((k, a)) => (k, a),
        Err(e) => return Ok(RespFrame::error(format!("ERR {}", e))),
    };
    
    // Get Lua interpreter
    let gil = match get_lua_interpreter() {
        Ok(gil) => gil,
        Err(e) => return Ok(RespFrame::error(format!("ERR {}", e))),
    };
    
    // Create execution context
    let context = ScriptContext {
        storage: storage.clone(),
        db: storage.get_current_db(),
        keys,
        args,
        timeout: Duration::from_secs(5), // Default timeout (should come from config)
    };
    
    // Evaluate script
    let start_time = Instant::now();
    let result = match gil.eval(&script, context) {
        Ok(resp) => resp,
        Err(e) => return Ok(RespFrame::error(format!("ERR {}", e))),
    };
    
    // Log slow scripts
    let elapsed = start_time.elapsed();
    if elapsed > Duration::from_millis(100) {
        println!("Slow Lua script execution: {:?}", elapsed);
    }
    
    Ok(result)
}

/// EVALSHA sha1 numkeys [key1...] [arg1...]
/// 
/// Evaluate a cached Lua script server side
pub fn handle_evalsha(storage: &Arc<StorageEngine>, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'evalsha' command"));
    }
    
    // Get SHA1
    let sha1 = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            match str::from_utf8(bytes) {
                Ok(s) => s.to_string(),
                Err(_) => return Ok(RespFrame::error("ERR invalid SHA1 hash - not valid UTF-8")),
            }
        }
        _ => {
            return Ok(RespFrame::error("ERR invalid SHA1 hash"));
        }
    };
    
    // Get number of keys
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
        _ => {
            return Ok(RespFrame::error("ERR invalid number of keys"));
        }
    };
    
    // Process keys and args
    let (keys, args) = match process_keys_and_args(parts, 3, num_keys) {
        Ok((k, a)) => (k, a),
        Err(e) => return Ok(RespFrame::error(format!("ERR {}", e))),
    };
    
    // Get Lua interpreter
    let gil = match get_lua_interpreter() {
        Ok(gil) => gil,
        Err(e) => return Ok(RespFrame::error(format!("ERR {}", e))),
    };
    
    // Create execution context
    let context = ScriptContext {
        storage: storage.clone(),
        db: storage.get_current_db(),
        keys,
        args,
        timeout: Duration::from_secs(5), // Default timeout (should come from config)
    };
    
    // Evaluate script
    let start_time = Instant::now();
    let result = match gil.evalsha(&sha1, context) {
        Ok(resp) => resp,
        Err(e) => {
            match e {
                FerrousError::ScriptNotFound(_) => {
                    return Ok(RespFrame::error("NOSCRIPT No matching script. Please use EVAL."))
                }
                _ => return Ok(RespFrame::error(format!("ERR {}", e))),
            }
        }
    };
    
    // Log slow scripts
    let elapsed = start_time.elapsed();
    if elapsed > Duration::from_millis(100) {
        println!("Slow Lua script execution: {:?}", elapsed);
    }
    
    Ok(result)
}

/// SCRIPT LOAD script
/// 
/// Load a script into the script cache, but don't execute it
pub fn handle_script_load(storage: &Arc<StorageEngine>, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'script load' command"));
    }
    
    // Get script string
    let script = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            match str::from_utf8(bytes) {
                Ok(s) => s.to_string(),
                Err(_) => return Ok(RespFrame::error("ERR invalid script - not valid UTF-8")),
            }
        }
        _ => {
            return Ok(RespFrame::error("ERR invalid script"));
        }
    };
    
    // Get Lua interpreter
    let gil = match get_lua_interpreter() {
        Ok(gil) => gil,
        Err(e) => return Ok(RespFrame::error(format!("ERR {}", e))),
    };
    
    // Load script
    match gil.script_load(&script) {
        Ok(sha1) => Ok(RespFrame::bulk_string(sha1)),
        Err(e) => Ok(RespFrame::error(format!("ERR {}", e))),
    }
}

/// SCRIPT EXISTS sha1 [sha1...]
/// 
/// Check if scripts exist in the script cache
pub fn handle_script_exists(storage: &Arc<StorageEngine>, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'script exists' command"));
    }
    
    // Get Lua interpreter
    let gil = match get_lua_interpreter() {
        Ok(gil) => gil,
        Err(e) => return Ok(RespFrame::error(format!("ERR {}", e))),
    };
    
    // Check all hashes
    let mut results = Vec::new();
    
    for i in 1..parts.len() {
        let sha1 = match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => {
                match str::from_utf8(bytes) {
                    Ok(s) => s.to_string(),
                    Err(_) => {
                        // Invalid UTF-8, cannot be a valid SHA1
                        results.push(RespFrame::Integer(0));
                        continue;
                    }
                }
            }
            _ => {
                // Invalid type, cannot be a valid SHA1
                results.push(RespFrame::Integer(0));
                continue;
            }
        };
        
        // Check if script exists
        if gil.script_exists(&sha1) {
            results.push(RespFrame::Integer(1));
        } else {
            results.push(RespFrame::Integer(0));
        }
    }
    
    Ok(RespFrame::Array(Some(results)))
}

/// SCRIPT FLUSH
/// 
/// Remove all scripts from the script cache
pub fn handle_script_flush(storage: &Arc<StorageEngine>, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 1 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'script flush' command"));
    }
    
    // Get Lua interpreter
    let gil = match get_lua_interpreter() {
        Ok(gil) => gil,
        Err(e) => return Ok(RespFrame::error(format!("ERR {}", e))),
    };
    
    // Flush scripts
    gil.script_flush();
    
    Ok(RespFrame::SimpleString(b"OK".to_vec()))
}

/// SCRIPT KILL
/// 
/// Kill the currently running script
pub fn handle_script_kill(storage: &Arc<StorageEngine>, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 1 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'script kill' command"));
    }
    
    // Get Lua interpreter
    let gil = match get_lua_interpreter() {
        Ok(gil) => gil,
        Err(e) => return Ok(RespFrame::error(format!("ERR {}", e))),
    };
    
    // Kill script
    match gil.kill_script() {
        Ok(_) => Ok(RespFrame::SimpleString(b"OK".to_vec())),
        Err(e) => Ok(RespFrame::error(format!("ERR {}", e))),
    }
}

/// Handle a Lua script command
pub fn handle_lua_command(storage: &Arc<StorageEngine>, cmd: &str, parts: &[RespFrame]) -> Result<RespFrame> {
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
                _ => {
                    return Ok(RespFrame::error("ERR invalid subcommand"));
                }
            };
            
            match subcommand.as_str() {
                "load" => handle_script_load(storage, &parts[1..]),
                "exists" => handle_script_exists(storage, &parts[1..]),
                "flush" => handle_script_flush(storage, &parts[1..]),
                "kill" => handle_script_kill(storage, &parts[1..]),
                _ => Ok(RespFrame::error(format!("ERR Unknown subcommand '{}'", subcommand))),
            }
        },
        _ => Ok(RespFrame::error(format!("ERR unknown command '{}'", cmd))),
    }
}

/// Register script commands
pub fn register_script_commands(storage: &Arc<StorageEngine>) {
    // This was called during server startup - initialize Lua interpreter
    let _ = get_lua_interpreter();
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_eval_simple() {
        let storage = Arc::new(StorageEngine::new_in_memory());
        
        // Simple script
        let script = "return 'hello'".as_bytes().to_vec();
        let parts = vec![
            RespFrame::BulkString(Some("EVAL".as_bytes().to_vec())),
            RespFrame::BulkString(Some(script)),
            RespFrame::Integer(0),
        ];
        
        let result = handle_eval(&storage, &parts).unwrap();
        
        // Check result
        if let RespFrame::BulkString(Some(bytes)) = result {
            assert_eq!(String::from_utf8_lossy(&bytes), "hello");
        } else {
            panic!("Expected bulk string result");
        }
    }
    
    #[test]
    fn test_eval_with_keys_and_args() {
        let storage = Arc::new(StorageEngine::new_in_memory());
        
        // Script using KEYS and ARGV
        let script = "return {KEYS[1], ARGV[1]}".as_bytes().to_vec();
        let parts = vec![
            RespFrame::BulkString(Some("EVAL".as_bytes().to_vec())),
            RespFrame::BulkString(Some(script)),
            RespFrame::Integer(1),
            RespFrame::BulkString(Some("key1".as_bytes().to_vec())),
            RespFrame::BulkString(Some("arg1".as_bytes().to_vec())),
        ];
        
        let result = handle_eval(&storage, &parts).unwrap();
        
        // Should be an array with two elements
        if let RespFrame::Array(Some(items)) = result {
            assert_eq!(items.len(), 2);
            if let RespFrame::BulkString(Some(bytes)) = &items[0] {
                assert_eq!(String::from_utf8_lossy(bytes), "key1");
            } else {
                panic!("Expected bulk string for first element");
            }
            if let RespFrame::BulkString(Some(bytes)) = &items[1] {
                assert_eq!(String::from_utf8_lossy(bytes), "arg1");
            } else {
                panic!("Expected bulk string for second element");
            }
        } else {
            panic!("Expected array result");
        }
    }
    
    #[test]
    fn test_script_load_evalsha_cycle() {
        let storage = Arc::new(StorageEngine::new_in_memory());
        
        // Load a script
        let script = "return 'hello from cached script'".as_bytes().to_vec();
        let load_parts = vec![
            RespFrame::BulkString(Some("SCRIPT".as_bytes().to_vec())),
            RespFrame::BulkString(Some("LOAD".as_bytes().to_vec())),
            RespFrame::BulkString(Some(script)),
        ];
        
        let sha1_result = handle_script_load(&storage, &load_parts).unwrap();
        
        // Get the SHA1
        let sha1_bytes = match sha1_result {
            RespFrame::BulkString(Some(bytes)) => bytes,
            _ => panic!("Expected bulk string for SHA1"),
        };
        
        // Try to execute with EVALSHA
        let evalsha_parts = vec![
            RespFrame::BulkString(Some("EVALSHA".as_bytes().to_vec())),
            RespFrame::BulkString(Some(sha1_bytes)),
            RespFrame::Integer(0),
        ];
        
        let result = handle_evalsha(&storage, &evalsha_parts).unwrap();
        
        // Check result
        if let RespFrame::BulkString(Some(bytes)) = result {
            assert_eq!(String::from_utf8_lossy(&bytes), "hello from cached script");
        } else {
            panic!("Expected bulk string result");
        }
    }
    
    #[test]
    fn test_evalsha_nonexistent() {
        let storage = Arc::new(StorageEngine::new_in_memory());
        
        // Try to execute a nonexistent script
        let evalsha_parts = vec![
            RespFrame::BulkString(Some("EVALSHA".as_bytes().to_vec())),
            RespFrame::BulkString(Some("123456789abcdef123456789abcdef123456789a".as_bytes().to_vec())),
            RespFrame::Integer(0),
        ];
        
        let result = handle_evalsha(&storage, &evalsha_parts).unwrap();
        
        // Should be an error
        if let RespFrame::Error(bytes) = result {
            assert!(String::from_utf8_lossy(&bytes).contains("NOSCRIPT"));
        } else {
            panic!("Expected error result");
        }
    }
    
    #[test]
    fn test_script_exists() {
        let storage = Arc::new(StorageEngine::new_in_memory());
        
        // Load a script
        let script = "return 'test'".as_bytes().to_vec();
        let load_parts = vec![
            RespFrame::BulkString(Some("SCRIPT".as_bytes().to_vec())),
            RespFrame::BulkString(Some("LOAD".as_bytes().to_vec())),
            RespFrame::BulkString(Some(script)),
        ];
        
        let sha1_result = handle_script_load(&storage, &load_parts).unwrap();
        
        // Get the SHA1
        let sha1_bytes = match sha1_result {
            RespFrame::BulkString(Some(bytes)) => bytes,
            _ => panic!("Expected bulk string for SHA1"),
        };
        
        // Check if script exists
        let exists_parts = vec![
            RespFrame::BulkString(Some("SCRIPT".as_bytes().to_vec())),
            RespFrame::BulkString(Some("EXISTS".as_bytes().to_vec())),
            RespFrame::BulkString(Some(sha1_bytes.clone())),
            RespFrame::BulkString(Some("nonexistent".as_bytes().to_vec())),
        ];
        
        let result = handle_script_exists(&storage, &exists_parts).unwrap();
        
        // Should be an array with [1, 0]
        if let RespFrame::Array(Some(items)) = result {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0], RespFrame::Integer(1)); // Exists
            assert_eq!(items[1], RespFrame::Integer(0)); // Doesn't exist
        } else {
            panic!("Expected array result");
        }
    }
}