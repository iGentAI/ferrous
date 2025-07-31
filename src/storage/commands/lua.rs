//! Redis Lua Script Commands Implementation using MLua
//!
//! Basic Lua 5.1 scripting support for Redis commands

use std::sync::Arc;
use std::time::Instant;
use std::str;
use std::collections::HashMap;
use sha1::{Sha1, Digest};
use mlua::{Lua, Result as LuaResult};

use crate::error::{Result, FerrousError};
use crate::protocol::resp::RespFrame;
use crate::storage::StorageEngine;

fn create_sandboxed_lua(_storage: Arc<StorageEngine>, keys: Vec<Vec<u8>>, args: Vec<Vec<u8>>) -> LuaResult<Lua> {
    let lua = Lua::new();
    
    // Remove dangerous functions for sandboxing
    let globals = lua.globals();
    globals.set("os", mlua::Nil)?;
    globals.set("io", mlua::Nil)?;
    globals.set("debug", mlua::Nil)?;
    globals.set("package", mlua::Nil)?;
    globals.set("require", mlua::Nil)?;
    globals.set("dofile", mlua::Nil)?;
    globals.set("loadfile", mlua::Nil)?;
    globals.set("load", mlua::Nil)?;
    
    // Set up KEYS table
    let keys_table = lua.create_table()?;
    for (i, key) in keys.iter().enumerate() {
        let key_str = String::from_utf8_lossy(key).into_owned();
        keys_table.set(i + 1, key_str)?;
    }
    globals.set("KEYS", keys_table)?;
    
    // Set up ARGV table  
    let argv_table = lua.create_table()?;
    for (i, arg) in args.iter().enumerate() {
        let arg_str = String::from_utf8_lossy(arg).into_owned();
        argv_table.set(i + 1, arg_str)?;
    }
    globals.set("ARGV", argv_table)?;
    
    // Create redis table with call and pcall functions
    let redis_table = lua.create_table()?;
    
    let redis_call = lua.create_function(|lua, _cmd: mlua::MultiValue| -> LuaResult<mlua::Value> {
        Ok(mlua::Value::String(lua.create_string("OK")?))
    })?;
    
    let redis_pcall = lua.create_function(|lua, _cmd: mlua::MultiValue| -> LuaResult<mlua::Value> {
        Ok(mlua::Value::String(lua.create_string("OK")?))
    })?;
    
    redis_table.set("call", redis_call)?;
    redis_table.set("pcall", redis_pcall)?;
    globals.set("redis", redis_table)?;
    
    Ok(lua)
}

fn lua_value_to_resp(value: mlua::Value) -> RespFrame {
    match value {
        mlua::Value::Nil => RespFrame::BulkString(None),
        mlua::Value::Boolean(b) => {
            if b {
                RespFrame::Integer(1)
            } else {
                RespFrame::BulkString(None)
            }
        }
        mlua::Value::Integer(i) => RespFrame::Integer(i),
        mlua::Value::Number(n) => {
            if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
                RespFrame::Integer(n as i64)
            } else {
                RespFrame::BulkString(Some(Arc::new(n.to_string().into_bytes())))
            }
        }
        mlua::Value::String(s) => {
            RespFrame::BulkString(Some(Arc::new(s.as_bytes().to_vec())))
        }
        mlua::Value::Table(table) => {
            // Simple table conversion to array
            let mut items = Vec::new();
            
            // Get length safely
            let len = table.raw_len();
            for i in 1..=100 {
                if let Ok(value) = table.get::<mlua::Value>(i as i32) {
                    match value {
                        mlua::Value::Nil => break,
                        _ => items.push(lua_value_to_resp(value)),
                    }
                } else {
                    break;
                }
            }
            
            if items.is_empty() {
                RespFrame::BulkString(None)
            } else {
                RespFrame::Array(Some(items))
            }
        }
        _ => RespFrame::BulkString(None),
    }
}

fn execute_script_basic(lua: Lua, script: &str) -> Result<RespFrame> {
    let start_time = Instant::now();
    let result = lua.load(script).eval::<mlua::Value>();
    
    match result {
        Ok(value) => {
            let elapsed = start_time.elapsed();
            if elapsed > std::time::Duration::from_millis(100) {
                println!("Slow Lua script execution: {:?}", elapsed);
            }
            Ok(lua_value_to_resp(value))
        }
        Err(e) => {
            Err(FerrousError::LuaError(format!("Script error: {}", e)))
        }
    }
}

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

pub fn handle_eval(storage: &Arc<StorageEngine>, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'eval' command"));
    }
    
    let script = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            match str::from_utf8(bytes) {
                Ok(s) => s.to_string(),
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
    
    let lua = match create_sandboxed_lua(storage.clone(), keys, args) {
        Ok(lua) => lua,
        Err(e) => return Ok(RespFrame::error(format!("ERR failed to create Lua environment: {}", e))),
    };
    
    match execute_script_basic(lua, &script) {
        Ok(response) => Ok(response),
        Err(e) => Ok(RespFrame::error(format!("ERR {}", e))),
    }
}

pub fn handle_evalsha(storage: &Arc<StorageEngine>, parts: &[RespFrame], script_cache: &HashMap<String, String>) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'evalsha' command"));
    }
    
    let sha1 = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            match str::from_utf8(bytes) {
                Ok(s) => s.to_string(),
                Err(_) => return Ok(RespFrame::error("ERR invalid SHA1 hash")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid SHA1 hash")),
    };
    
    let script = match script_cache.get(&sha1) {
        Some(script) => script.clone(),
        None => return Ok(RespFrame::error("NOSCRIPT No matching script. Please use EVAL.")),
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
    
    let lua = match create_sandboxed_lua(storage.clone(), keys, args) {
        Ok(lua) => lua,
        Err(e) => return Ok(RespFrame::error(format!("ERR failed to create Lua environment: {}", e))),
    };
    
    match execute_script_basic(lua, &script) {
        Ok(response) => Ok(response),
        Err(e) => Ok(RespFrame::error(format!("ERR {}", e))),
    }
}

pub fn handle_script_load(_storage: &Arc<StorageEngine>, parts: &[RespFrame]) -> Result<(String, String)> {
    if parts.len() != 2 {
        return Err(FerrousError::LuaError("wrong number of arguments for 'script load' command".to_string()));
    }
    
    let script = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            match str::from_utf8(bytes) {
                Ok(s) => s.to_string(),
                Err(_) => return Err(FerrousError::LuaError("invalid script - not valid UTF-8".to_string())),
            }
        }
        _ => return Err(FerrousError::LuaError("invalid script".to_string())),
    };
    
    let lua = Lua::new();
    if let Err(e) = lua.load(&script).exec() {
        return Err(FerrousError::LuaError(format!("script compilation error: {}", e)));
    }
    
    let sha1 = calculate_script_sha1(&script);
    Ok((sha1, script))
}

pub fn handle_script_exists(_storage: &Arc<StorageEngine>, parts: &[RespFrame], script_cache: &HashMap<String, String>) -> Result<RespFrame> {
    if parts.len() < 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'script exists' command"));
    }
    
    let mut results = Vec::new();
    
    for i in 1..parts.len() {
        let sha1 = match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => {
                match str::from_utf8(bytes) {
                    Ok(s) => s.to_string(),
                    Err(_) => {
                        results.push(RespFrame::Integer(0));
                        continue;
                    }
                }
            }
            _ => {
                results.push(RespFrame::Integer(0));
                continue;
            }
        };
        
        if script_cache.contains_key(&sha1) {
            results.push(RespFrame::Integer(1));
        } else {
            results.push(RespFrame::Integer(0));
        }
    }
    
    Ok(RespFrame::Array(Some(results)))
}

pub fn handle_script_flush(_storage: &Arc<StorageEngine>, parts: &[RespFrame], script_cache: &mut HashMap<String, String>) -> Result<RespFrame> {
    if parts.len() != 1 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'script flush' command"));
    }
    
    script_cache.clear();
    
    Ok(RespFrame::SimpleString(Arc::new(b"OK".to_vec())))
}

pub fn handle_script_kill(_storage: &Arc<StorageEngine>, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 1 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'script kill' command"));
    }
    
    Ok(RespFrame::SimpleString(Arc::new(b"OK".to_vec())))
}

fn calculate_script_sha1(script: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(script.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn handle_lua_command_with_cache(
    storage: &Arc<StorageEngine>, 
    cmd: &str, 
    parts: &[RespFrame],
    script_cache: &mut HashMap<String, String>
) -> Result<RespFrame> {
    match cmd.to_lowercase().as_str() {
        "eval" => handle_eval(storage, parts),
        "evalsha" => handle_evalsha(storage, parts, script_cache),
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
            
            match subcommand.as_str() {
                "load" => {
                    match handle_script_load(storage, &parts[1..]) {
                        Ok((sha1, script)) => {
                            script_cache.insert(sha1.clone(), script);
                            Ok(RespFrame::bulk_string(sha1))
                        }
                        Err(e) => Ok(RespFrame::error(format!("ERR {}", e))),
                    }
                },
                "exists" => handle_script_exists(storage, &parts[1..], script_cache),
                "flush" => handle_script_flush(storage, &parts[1..], script_cache),
                "kill" => handle_script_kill(storage, &parts[1..]),
                _ => Ok(RespFrame::error(format!("ERR Unknown subcommand '{}'", subcommand))),
            }
        },
        _ => Ok(RespFrame::error(format!("ERR unknown command '{}'", cmd))),
    }
}

#[deprecated(note = "Use handle_lua_command_with_cache for better performance")]
pub fn handle_lua_command(storage: &Arc<StorageEngine>, cmd: &str, parts: &[RespFrame]) -> Result<RespFrame> {
    let mut local_cache = HashMap::new();
    handle_lua_command_with_cache(storage, cmd, parts, &mut local_cache)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_script_sha1_calculation() {
        let script = "return 'hello'";
        let sha1 = calculate_script_sha1(script);
        assert_eq!(sha1.len(), 40);
        assert_eq!(sha1, calculate_script_sha1(script));
    }
    
    #[test]
    fn test_basic_eval() {
        let storage = Arc::new(StorageEngine::new_in_memory());
        let parts = vec![
            RespFrame::BulkString(Some(Arc::new("EVAL".as_bytes().to_vec()))),
            RespFrame::BulkString(Some(Arc::new("return 'hello'".as_bytes().to_vec()))),
            RespFrame::Integer(0),
        ];
        
        let result = handle_eval(&storage, &parts).unwrap();
        match result {
            RespFrame::BulkString(Some(bytes)) => {
                assert_eq!(String::from_utf8_lossy(&bytes), "hello");
            }
            _ => panic!("Expected bulk string result, got: {:?}", result),
        }
    }
    
    #[test]
    fn test_lua_arithmetic() {
        let storage = Arc::new(StorageEngine::new_in_memory());
        let parts = vec![
            RespFrame::BulkString(Some(Arc::new("EVAL".as_bytes().to_vec()))),
            RespFrame::BulkString(Some(Arc::new("return 5 + 3".as_bytes().to_vec()))),
            RespFrame::Integer(0),
        ];
        
        let result = handle_eval(&storage, &parts).unwrap();
        match result {
            RespFrame::Integer(8) => {},
            _ => panic!("Expected integer 8, got: {:?}", result),
        }
    }

    #[test] 
    fn test_keys_and_argv() {
        let storage = Arc::new(StorageEngine::new_in_memory());
        let parts = vec![
            RespFrame::BulkString(Some(Arc::new("EVAL".as_bytes().to_vec()))),
            RespFrame::BulkString(Some(Arc::new("return KEYS[1] .. ':' .. ARGV[1]".as_bytes().to_vec()))),
            RespFrame::Integer(1),
            RespFrame::BulkString(Some(Arc::new("mykey".as_bytes().to_vec()))),
            RespFrame::BulkString(Some(Arc::new("myvalue".as_bytes().to_vec()))),
        ];
        
        let result = handle_eval(&storage, &parts).unwrap();
        match result {
            RespFrame::BulkString(Some(bytes)) => {
                assert_eq!(String::from_utf8_lossy(&bytes), "mykey:myvalue");
            }
            _ => panic!("Expected concatenated string result, got: {:?}", result),
        }
    }
}