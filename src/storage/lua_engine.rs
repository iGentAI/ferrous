//! Redis-compatible Lua execution engine with proper error semantics
//!
//! This module implements proper Redis Lua behavior where:
//! - redis.call errors immediately terminate the script and become the RESP response
//! - redis.pcall errors return nil and allow the script to continue

use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::time::{Instant, Duration};
use mlua::{Lua, Result as LuaResult, MultiValue, Value as LuaValue};
use sha1::{Sha1, Digest};

use crate::error::{Result, FerrousError};
use crate::protocol::resp::RespFrame;
use crate::storage::StorageEngine;

/// Command execution context passed from server to Lua engine
pub struct LuaCommandContext {
    pub db_index: usize,
    pub storage: Arc<StorageEngine>,
}

/// Special error type to handle redis.call immediate termination
#[derive(Debug)]
pub struct RedisCallError {
    pub error_msg: String,
}

impl std::fmt::Display for RedisCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.error_msg)
    }
}

impl std::error::Error for RedisCallError {}

/// Single-threaded Lua execution engine with proper Redis semantics
pub struct LuaEngine {
    script_cache: Arc<Mutex<HashMap<String, String>>>,
}

impl LuaEngine {
    pub fn new(_storage: Arc<StorageEngine>) -> Result<Self> {
        Ok(LuaEngine {
            script_cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    
    /// Execute a Lua script with proper command context
    pub fn eval(&self, script: &str, keys: Vec<Vec<u8>>, args: Vec<Vec<u8>>, ctx: &LuaCommandContext) -> Result<RespFrame> {
        let lua = self.create_lua_context(ctx)?;
        self.setup_keys_and_args(&lua, keys, args)?;
        
        let start_time = Instant::now();
        let result = lua.load(script).eval::<LuaValue>();
        
        match result {
            Ok(value) => {
                let elapsed = start_time.elapsed();
                // if elapsed > std::time::Duration::from_millis(100) {
                //     eprintln!("Slow Lua script execution: {:?}", elapsed);
                // }
                Ok(self.lua_value_to_resp(value))
            }
            Err(e) => {
                match e {
                    mlua::Error::RuntimeError(ref msg) => {
                        // Robust extraction for REDIS_CALL_ABORT errors per comprehensive analysis
                        if let Some(pos) = msg.find("REDIS_CALL_ABORT:") {
                            let mut error_content = &msg[pos + "REDIS_CALL_ABORT:".len()..];
                            // Skip any non-alphabetic chars after prefix (e.g., spaces, colons, numbers)
                            error_content = error_content.trim_start_matches(|c: char| !c.is_alphabetic());
                            // Find end: Up to first \n or end (strip suffixes like stack traces)
                            let end_pos = error_content.find('\n').unwrap_or(error_content.len());
                            let clean_error = error_content[..end_pos].trim().to_string();
                            // Ensure starts with "ERR " (add if missing)
                            let final_error = if clean_error.starts_with("ERR ") { 
                                clean_error 
                            } else { 
                                format!("ERR {}", clean_error) 
                            };
                            Err(FerrousError::LuaError(final_error))
                        } else {
                            // This is a regular Lua runtime error
                            Err(FerrousError::LuaError(format!("ERR Error running script: {}", msg)))
                        }
                    }
                    mlua::Error::SyntaxError { message, .. } => {
                        Err(FerrousError::LuaError(format!("ERR Error compiling script: {}", message)))
                    }
                    _ => {
                        Err(FerrousError::LuaError(format!("ERR Script execution failed: {}", e)))
                    }
                }
            }
        }
    }
    
    pub fn evalsha(&self, sha1: &str, keys: Vec<Vec<u8>>, args: Vec<Vec<u8>>, ctx: &LuaCommandContext) -> Result<RespFrame> {
        let script = match self.script_cache.try_lock() {
            Ok(cache) => {
                cache.get(sha1).cloned().ok_or_else(|| {
                    FerrousError::LuaError("NOSCRIPT No matching script. Please use EVAL.".to_string())
                })?
            }
            Err(_) => {
                return Err(FerrousError::LuaError("Script cache temporarily unavailable".to_string()));
            }
        };
        
        self.eval(&script, keys, args, ctx)
    }
    
    pub fn script_load(&self, script: &str) -> Result<String> {
        let lua = Lua::new();
        lua.load(script).exec().map_err(|e| {
            FerrousError::LuaError(format!("Script compilation error: {}", e))
        })?;
        
        let sha1 = self.calculate_script_sha1(script);
        
        if let Ok(mut cache) = self.script_cache.try_lock() {
            cache.insert(sha1.clone(), script.to_string());
        }
        
        Ok(sha1)
    }
    
    pub fn script_exists(&self, sha1s: &[String]) -> Result<Vec<bool>> {
        match self.script_cache.try_lock() {
            Ok(cache) => {
                Ok(sha1s.iter().map(|sha1| cache.contains_key(sha1)).collect())
            }
            Err(_) => {
                Ok(vec![false; sha1s.len()])
            }
        }
    }
    
    pub fn script_flush(&self) -> Result<()> {
        match self.script_cache.try_lock() {
            Ok(mut cache) => {
                cache.clear();
                Ok(())
            }
            Err(_) => {
                Err(FerrousError::LuaError("Script cache temporarily unavailable".to_string()))
            }
        }
    }
    
    fn create_lua_context(&self, ctx: &LuaCommandContext) -> Result<Lua> {
        let lua = Lua::new();
        
        // Remove dangerous functions for sandboxing
        let globals = lua.globals();
        let dangerous_functions = ["os", "io", "debug", "package", "require", "dofile", "loadfile", "load"];
        for func in &dangerous_functions {
            globals.set(*func, mlua::Nil).map_err(|e| FerrousError::LuaError(e.to_string()))?;
        }
        
        // Create Redis API with proper error semantics
        let redis_table = lua.create_table().map_err(|e| FerrousError::LuaError(e.to_string()))?;
        
        let storage_ref = ctx.storage.clone();
        let db_index = ctx.db_index;
        
        // redis.call: Errors terminate the script immediately
        let redis_call = lua.create_function(move |lua_ctx, cmd: MultiValue| -> LuaResult<LuaValue> {
            Self::execute_redis_command(&storage_ref, lua_ctx, cmd, db_index, false)
        }).map_err(|e| FerrousError::LuaError(e.to_string()))?;
        
        let storage_ref_pcall = ctx.storage.clone();
        // redis.pcall: Errors return nil, script continues
        let redis_pcall = lua.create_function(move |lua_ctx, cmd: MultiValue| -> LuaResult<LuaValue> {
            Self::execute_redis_command(&storage_ref_pcall, lua_ctx, cmd, db_index, true)
        }).map_err(|e| FerrousError::LuaError(e.to_string()))?;
        
        redis_table.set("call", redis_call).map_err(|e| FerrousError::LuaError(e.to_string()))?;
        redis_table.set("pcall", redis_pcall).map_err(|e| FerrousError::LuaError(e.to_string()))?;
        globals.set("redis", redis_table).map_err(|e| FerrousError::LuaError(e.to_string()))?;
        
        Ok(lua)
    }
    
    fn setup_keys_and_args(&self, lua: &Lua, keys: Vec<Vec<u8>>, args: Vec<Vec<u8>>) -> Result<()> {
        let globals = lua.globals();
        
        let keys_table = lua.create_table().map_err(|e| FerrousError::LuaError(e.to_string()))?;
        for (i, key) in keys.iter().enumerate() {
            let key_str = String::from_utf8_lossy(key).into_owned();
            keys_table.set(i + 1, key_str).map_err(|e| FerrousError::LuaError(e.to_string()))?;
        }
        globals.set("KEYS", keys_table).map_err(|e| FerrousError::LuaError(e.to_string()))?;
        
        let argv_table = lua.create_table().map_err(|e| FerrousError::LuaError(e.to_string()))?;
        for (i, arg) in args.iter().enumerate() {
            let arg_str = String::from_utf8_lossy(arg).into_owned();
            argv_table.set(i + 1, arg_str).map_err(|e| FerrousError::LuaError(e.to_string()))?;
        }
        globals.set("ARGV", argv_table).map_err(|e| FerrousError::LuaError(e.to_string()))?;
        
        Ok(())
    }
    
    /// Execute Redis command through proper server handlers with Lua-specific context
    fn execute_redis_command(
        storage: &Arc<StorageEngine>,
        lua_ctx: &Lua,
        cmd: MultiValue,
        db_index: usize,
        is_pcall: bool
    ) -> LuaResult<LuaValue> {
        // Parse command arguments
        let mut args = Vec::new();
        for value in cmd {
            match value {
                LuaValue::String(s) => {
                    match s.to_str() {
                        Ok(string_val) => args.push(string_val.to_string()),
                        Err(_) => {
                            return Self::handle_command_error_with_context(lua_ctx, "Invalid UTF-8 in command argument".to_string(), is_pcall);
                        }
                    }
                }
                LuaValue::Integer(i) => args.push(i.to_string()),
                LuaValue::Number(n) => args.push(n.to_string()),
                _ => {
                    return Self::handle_command_error_with_context(lua_ctx, "Invalid argument type".to_string(), is_pcall);
                }
            }
        }
        
        if args.is_empty() {
            return Self::handle_command_error_with_context(lua_ctx, "No command specified".to_string(), is_pcall);
        }
        
        let cmd_name = args[0].to_uppercase();
        
        // Convert to RESP frames for server handler routing
        let mut resp_parts = Vec::new();
        for arg in &args {
            resp_parts.push(RespFrame::from_string(arg.clone()));
        }
        
        // Route through appropriate handler based on command type
        let result = match cmd_name.as_str() {
            // Commands forbidden in Lua scripts
            "EVAL" | "EVALSHA" | "SCRIPT" => {
                return Self::handle_command_error_with_context(
                    lua_ctx, 
                    "Redis scripting commands are not allowed inside Lua scripts".to_string(), 
                    is_pcall
                );
            }
            "SELECT" | "MULTI" | "EXEC" | "DISCARD" | "WATCH" | "UNWATCH" => {
                return Self::handle_command_error_with_context(
                    lua_ctx,
                    format!("'{}' command is not allowed inside Lua scripts", cmd_name),
                    is_pcall
                );
            }
            "BLPOP" | "BRPOP" | "SUBSCRIBE" | "UNSUBSCRIBE" | "PSUBSCRIBE" | "PUNSUBSCRIBE" => {
                return Self::handle_command_error_with_context(
                    lua_ctx,
                    format!("'{}' command is not allowed inside Lua scripts", cmd_name),
                    is_pcall
                );
            }
            
            // Route data manipulation commands to proper handlers
            "SET" => {
                if args.len() < 3 {
                    return Self::handle_command_error_with_context(lua_ctx, "wrong number of arguments for 'set' command".to_string(), is_pcall);
                }
                
                // Parse SET options (NX, XX, EX, PX)
                let key = args[1].as_bytes().to_vec();
                let value = args[2].as_bytes().to_vec();
                
                let mut nx = false;
                let mut xx = false;
                let mut ex_seconds: Option<u64> = None;
                let mut px_millis: Option<u64> = None;
                
                let mut i = 3;
                while i < args.len() {
                    match args[i].to_uppercase().as_str() {
                        "NX" => {
                            nx = true;
                            i += 1;
                        }
                        "XX" => {
                            xx = true;
                            i += 1;
                        }
                        "EX" => {
                            if i + 1 < args.len() {
                                if let Ok(seconds) = args[i + 1].parse::<u64>() {
                                    ex_seconds = Some(seconds);
                                    i += 2;
                                } else {
                                    return Self::handle_command_error_with_context(lua_ctx, "invalid expire time".to_string(), is_pcall);
                                }
                            } else {
                                return Self::handle_command_error_with_context(lua_ctx, "syntax error".to_string(), is_pcall);
                            }
                        }
                        "PX" => {
                            if i + 1 < args.len() {
                                if let Ok(millis) = args[i + 1].parse::<u64>() {
                                    px_millis = Some(millis);
                                    i += 2;
                                } else {
                                    return Self::handle_command_error_with_context(lua_ctx, "invalid expire time".to_string(), is_pcall);
                                }
                            } else {
                                return Self::handle_command_error_with_context(lua_ctx, "syntax error".to_string(), is_pcall);
                            }
                        }
                        _ => {
                            return Self::handle_command_error_with_context(lua_ctx, "syntax error".to_string(), is_pcall);
                        }
                    }
                }
                
                // Handle NX condition (only set if key doesn't exist)
                if nx {
                    match storage.exists(db_index, &key) {
                        Ok(true) => {
                            // Key exists, NX should fail - return nil
                            return Ok(LuaValue::Nil);
                        }
                        Ok(false) => {
                            // Key doesn't exist, proceed with set
                        }
                        Err(e) => {
                            return Self::handle_command_error_with_context(lua_ctx, format!("EXISTS check failed: {}", e), is_pcall);
                        }
                    }
                }
                
                // Handle XX condition (only set if key exists)
                if xx {
                    match storage.exists(db_index, &key) {
                        Ok(false) => {
                            // Key doesn't exist, XX should fail - return nil
                            return Ok(LuaValue::Nil);
                        }
                        Ok(true) => {
                            // Key exists, proceed with set
                        }
                        Err(e) => {
                            return Self::handle_command_error_with_context(lua_ctx, format!("EXISTS check failed: {}", e), is_pcall);
                        }
                    }
                }
                
                // Execute the SET operation with expiration if specified
                let result = if let Some(seconds) = ex_seconds {
                    storage.set_string_ex(db_index, key, value, std::time::Duration::from_secs(seconds))
                } else if let Some(millis) = px_millis {
                    storage.set_string_ex(db_index, key, value, std::time::Duration::from_millis(millis))
                } else {
                    storage.set_string(db_index, key, value)
                };
                
                match result {
                    Ok(_) => {
                        match lua_ctx.create_string("OK") {
                            Ok(lua_string) => Ok(LuaValue::String(lua_string)),
                            Err(e) => Self::handle_command_error_with_context(lua_ctx, format!("String creation failed: {}", e), is_pcall),
                        }
                    }
                    Err(e) => Self::handle_command_error_with_context(lua_ctx, format!("SET operation failed: {}", e), is_pcall),
                }
            }
            "GET" => {
                crate::storage::commands::strings::handle_mget(storage, db_index, &resp_parts)
                    .map_err(|e| FerrousError::LuaError(e.to_string()))
                    .and_then(|resp| Ok(resp))
            }
            "DEL" => {
                Self::route_to_server_handler(storage, &resp_parts, db_index, "DEL")
            }
            "EXISTS" => {
                Self::route_to_server_handler(storage, &resp_parts, db_index, "EXISTS")
            }
            "INCR" => {
                Self::route_to_server_handler(storage, &resp_parts, db_index, "INCR")
            }
            "DECR" => {
                Self::route_to_server_handler(storage, &resp_parts, db_index, "DECR")
            }
            "INCRBY" => {
                Self::route_to_server_handler(storage, &resp_parts, db_index, "INCRBY")
            }
            "DECRBY" => {
                Self::route_to_server_handler(storage, &resp_parts, db_index, "DECRBY")
            }
            
            // List operations
            "LPUSH" => {
                crate::storage::commands::lists::handle_lpush(storage, db_index, &resp_parts)
                    .map_err(|e| FerrousError::LuaError(e.to_string()))
                    .and_then(|resp| Ok(resp))
            }
            "RPUSH" => {
                crate::storage::commands::lists::handle_rpush(storage, db_index, &resp_parts)
                    .map_err(|e| FerrousError::LuaError(e.to_string()))
                    .and_then(|resp| Ok(resp))
            }
            "LPOP" => {
                crate::storage::commands::lists::handle_lpop(storage, db_index, &resp_parts)
                    .map_err(|e| FerrousError::LuaError(e.to_string()))
                    .and_then(|resp| Ok(resp))
            }
            "RPOP" => {
                crate::storage::commands::lists::handle_rpop(storage, db_index, &resp_parts)
                    .map_err(|e| FerrousError::LuaError(e.to_string()))
                    .and_then(|resp| Ok(resp))
            }
            "LLEN" => {
                crate::storage::commands::lists::handle_llen(storage, db_index, &resp_parts)
                    .map_err(|e| FerrousError::LuaError(e.to_string()))
                    .and_then(|resp| Ok(resp))
            }
            "LRANGE" => {
                crate::storage::commands::lists::handle_lrange(storage, db_index, &resp_parts)
                    .map_err(|e| FerrousError::LuaError(e.to_string()))
                    .and_then(|resp| Ok(resp))
            }
            
            // Set operations  
            "SADD" => {
                crate::storage::commands::sets::handle_sadd(storage, db_index, &resp_parts)
                    .map_err(|e| FerrousError::LuaError(e.to_string()))
                    .and_then(|resp| Ok(resp))
            }
            "SREM" => {
                crate::storage::commands::sets::handle_srem(storage, db_index, &resp_parts)
                    .map_err(|e| FerrousError::LuaError(e.to_string()))
                    .and_then(|resp| Ok(resp))
            }
            "SCARD" => {
                crate::storage::commands::sets::handle_scard(storage, db_index, &resp_parts)
                    .map_err(|e| FerrousError::LuaError(e.to_string()))
                    .and_then(|resp| Ok(resp))
            }
            "SMEMBERS" => {
                crate::storage::commands::sets::handle_smembers(storage, db_index, &resp_parts)
                    .map_err(|e| FerrousError::LuaError(e.to_string()))
                    .and_then(|resp| Ok(resp))
            }
            "SISMEMBER" => {
                crate::storage::commands::sets::handle_sismember(storage, db_index, &resp_parts)
                    .map_err(|e| FerrousError::LuaError(e.to_string()))
                    .and_then(|resp| Ok(resp))
            }
            
            // Hash operations
            "HSET" => {
                crate::storage::commands::hashes::handle_hset(storage, db_index, &resp_parts)
                    .map_err(|e| FerrousError::LuaError(e.to_string()))
                    .and_then(|resp| Ok(resp))
            }
            "HGET" => {
                crate::storage::commands::hashes::handle_hget(storage, db_index, &resp_parts)
                    .map_err(|e| FerrousError::LuaError(e.to_string()))
                    .and_then(|resp| Ok(resp))
            }
            "HDEL" => {
                crate::storage::commands::hashes::handle_hdel(storage, db_index, &resp_parts)
                    .map_err(|e| FerrousError::LuaError(e.to_string()))
                    .and_then(|resp| Ok(resp))
            }
            "HLEN" => {
                crate::storage::commands::hashes::handle_hlen(storage, db_index, &resp_parts)
                    .map_err(|e| FerrousError::LuaError(e.to_string()))
                    .and_then(|resp| Ok(resp))
            }
            "HGETALL" => {
                crate::storage::commands::hashes::handle_hgetall(storage, db_index, &resp_parts)
                    .map_err(|e| FerrousError::LuaError(e.to_string()))
                    .and_then(|resp| Ok(resp))
            }
            "HEXISTS" => {
                crate::storage::commands::hashes::handle_hexists(storage, db_index, &resp_parts)
                    .map_err(|e| FerrousError::LuaError(e.to_string()))
                    .and_then(|resp| Ok(resp))
            }
            "HKEYS" => {
                crate::storage::commands::hashes::handle_hkeys(storage, db_index, &resp_parts)
                    .map_err(|e| FerrousError::LuaError(e.to_string()))
                    .and_then(|resp| Ok(resp))
            }
            "HVALS" => {
                crate::storage::commands::hashes::handle_hvals(storage, db_index, &resp_parts)
                    .map_err(|e| FerrousError::LuaError(e.to_string()))
                    .and_then(|resp| Ok(resp))
            }
            "HINCRBY" => {
                crate::storage::commands::hashes::handle_hincrby(storage, db_index, &resp_parts)
                    .map_err(|e| FerrousError::LuaError(e.to_string()))
                    .and_then(|resp| Ok(resp))
            }
            
            "PING" => {
                if args.len() == 1 {
                    RespFrame::SimpleString(Arc::new(b"PONG".to_vec()))
                } else if args.len() == 2 {
                    RespFrame::from_string(args[1].clone())
                } else {
                    return Self::handle_command_error_with_context(lua_ctx, "wrong number of arguments for 'ping' command".to_string(), is_pcall);
                }
            }
            
            _ => {
                return Self::handle_command_error_with_context(lua_ctx, format!("unknown command '{}'", cmd_name), is_pcall);
            }
        };
        
        // Convert server handler result to Lua value
        Self::resp_to_lua_value(lua_ctx, result, is_pcall)
    }
    
    /// Route command to appropriate server handler
    fn route_to_server_handler(
        storage: &Arc<StorageEngine>, 
        parts: &[RespFrame], 
        db_index: usize, 
        cmd_name: &str
    ) -> Result<RespFrame> {
        match cmd_name {
            "SET" => {
                Self::handle_lua_set(storage, db_index, parts)
            }
            "DEL" => {
                if parts.len() < 2 {
                    return Ok(RespFrame::error("ERR wrong number of arguments for 'del' command"));
                }
                
                let mut deleted = 0;
                for i in 1..parts.len() {
                    if let RespFrame::BulkString(Some(key_bytes)) = &parts[i] {
                        if storage.delete(db_index, key_bytes.as_ref())? {
                            deleted += 1;
                        }
                    }
                }
                Ok(RespFrame::Integer(deleted))
            }
            "EXISTS" => {
                if parts.len() < 2 {
                    return Ok(RespFrame::error("ERR wrong number of arguments for 'exists' command"));
                }
                
                let mut count = 0;
                for i in 1..parts.len() {
                    if let RespFrame::BulkString(Some(key_bytes)) = &parts[i] {
                        if storage.exists(db_index, key_bytes.as_ref())? {
                            count += 1;
                        }
                    }
                }
                Ok(RespFrame::Integer(count))
            }
            "INCR" => {
                if parts.len() != 2 {
                    return Ok(RespFrame::error("ERR wrong number of arguments for 'incr' command"));
                }
                
                if let RespFrame::BulkString(Some(key_bytes)) = &parts[1] {
                    match storage.incr(db_index, key_bytes.as_ref().clone()) {
                        Ok(new_value) => Ok(RespFrame::Integer(new_value)),
                        Err(e) => Ok(RespFrame::error(e.to_string())),
                    }
                } else {
                    Ok(RespFrame::error("ERR invalid key format"))
                }
            }
            _ => Ok(RespFrame::error(format!("ERR unknown command '{}'", cmd_name))),
        }
    }
    
    /// Handle SET command with ALL options for Lua context
    fn handle_lua_set(storage: &Arc<StorageEngine>, db_index: usize, parts: &[RespFrame]) -> Result<RespFrame> {
        if parts.len() < 3 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'set' command"));
        }
        
        // Extract key and value
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        let value = match &parts[2] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
            _ => return Ok(RespFrame::error("ERR invalid value format")),
        };
        
        // Parse SET options (EX, PX, NX, XX)
        let mut expiration = None;
        let mut nx = false;
        let mut xx = false;
        
        let mut i = 3;
        while i < parts.len() {
            match &parts[i] {
                RespFrame::BulkString(Some(option_bytes)) => {
                    let option_str = String::from_utf8_lossy(option_bytes).to_uppercase();
                    match option_str.as_str() {
                        "EX" => {
                            if i + 1 >= parts.len() {
                                return Ok(RespFrame::error("ERR syntax error"));
                            }
                            if let RespFrame::BulkString(Some(seconds_bytes)) = &parts[i + 1] {
                                if let Ok(seconds) = String::from_utf8_lossy(seconds_bytes).parse::<u64>() {
                                    expiration = Some(Duration::from_secs(seconds));
                                    i += 2;
                                    continue;
                                }
                            }
                            return Ok(RespFrame::error("ERR invalid expire time"));
                        }
                        "PX" => {
                            if i + 1 >= parts.len() {
                                return Ok(RespFrame::error("ERR syntax error"));
                            }
                            if let RespFrame::BulkString(Some(millis_bytes)) = &parts[i + 1] {
                                if let Ok(millis) = String::from_utf8_lossy(millis_bytes).parse::<u64>() {
                                    expiration = Some(Duration::from_millis(millis));
                                    i += 2;
                                    continue;
                                }
                            }
                            return Ok(RespFrame::error("ERR invalid expire time"));
                        }
                        "NX" => {
                            nx = true;
                            i += 1;
                        }
                        "XX" => {
                            xx = true;
                            i += 1;
                        }
                        _ => return Ok(RespFrame::error("ERR syntax error")),
                    }
                }
                _ => return Ok(RespFrame::error("ERR syntax error")),
            }
        }
        
        // Handle NX option (only set if key doesn't exist)
        if nx {
            if storage.exists(db_index, &key)? {
                return Ok(RespFrame::BulkString(None)); // Key exists, return nil
            }
        }
        
        // Handle XX option (only set if key exists)
        if xx {
            if !storage.exists(db_index, &key)? {
                return Ok(RespFrame::BulkString(None)); // Key doesn't exist, return nil
            }
        }
        
        // Execute the SET operation
        match expiration {
            Some(expires_in) => {
                storage.set_string_ex(db_index, key, value, expires_in)?;
            }
            None => {
                storage.set_string(db_index, key, value)?;
            }
        }
        
        Ok(RespFrame::SimpleString(Arc::new(b"OK".to_vec())))
    }
    
    /// Convert server RESP response to Lua value
    fn resp_to_lua_value(lua_ctx: &Lua, resp: Result<RespFrame>, is_pcall: bool) -> LuaResult<LuaValue> {
        match resp {
            Ok(frame) => {
                match frame {
                    RespFrame::SimpleString(bytes) => {
                        let string_val = String::from_utf8_lossy(&bytes);
                        match lua_ctx.create_string(&string_val) {
                            Ok(lua_string) => Ok(LuaValue::String(lua_string)),
                            Err(e) => Self::handle_command_error_with_context(lua_ctx, format!("String creation failed: {}", e), is_pcall),
                        }
                    }
                    RespFrame::BulkString(Some(bytes)) => {
                        let string_val = String::from_utf8_lossy(&bytes);
                        match lua_ctx.create_string(&string_val) {
                            Ok(lua_string) => Ok(LuaValue::String(lua_string)),
                            Err(e) => Self::handle_command_error_with_context(lua_ctx, format!("String creation failed: {}", e), is_pcall),
                        }
                    }
                    RespFrame::BulkString(None) => Ok(LuaValue::Nil),
                    RespFrame::Integer(i) => Ok(LuaValue::Integer(i)),
                    RespFrame::Array(Some(frames)) => {
                        let table = lua_ctx.create_table().map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                        for (idx, frame) in frames.iter().enumerate() {
                            let lua_val = Self::resp_to_lua_value(lua_ctx, Ok(frame.clone()), is_pcall)?;
                            table.set(idx + 1, lua_val).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                        }
                        Ok(LuaValue::Table(table))
                    }
                    RespFrame::Array(None) => Ok(LuaValue::Nil),
                    RespFrame::Error(bytes) => {
                        let error_msg = String::from_utf8_lossy(&bytes);
                        Self::handle_command_error_with_context(lua_ctx, error_msg.to_string(), is_pcall)
                    }
                    _ => Ok(LuaValue::Nil),
                }
            }
            Err(e) => {
                Self::handle_command_error_with_context(lua_ctx, e.to_string(), is_pcall)
            }
        }
    }
    
    /// Handle command errors with proper Redis semantics using lua context
    fn handle_command_error_with_context(lua_ctx: &Lua, error_msg: String, is_pcall: bool) -> LuaResult<LuaValue> {
        let formatted_error = if error_msg.starts_with("ERR ") {
            error_msg
        } else {
            format!("ERR {}", error_msg)
        };
        
        if is_pcall {
            // redis.pcall: Return nil, script continues - CORRECT Redis behavior
            Ok(LuaValue::Nil)
        } else {
            // redis.call: MUST abort script execution immediately - throw runtime error
            // This ensures multi-statement scripts terminate instead of continuing
            Err(mlua::Error::RuntimeError(format!("REDIS_CALL_ABORT:{}", formatted_error)))
        }
    }
    
    fn lua_value_to_resp(&self, value: LuaValue) -> RespFrame {
        match value {
            LuaValue::Nil => RespFrame::BulkString(None),
            LuaValue::Boolean(b) => {
                if b {
                    RespFrame::Integer(1)
                } else {
                    RespFrame::Integer(0)
                }
            }
            LuaValue::Integer(i) => RespFrame::Integer(i),
            LuaValue::Number(n) => {
                if n.is_nan() {
                    RespFrame::BulkString(None)
                } else if n.is_infinite() {
                    let inf_str = if n.is_sign_positive() { "inf" } else { "-inf" };
                    RespFrame::BulkString(Some(Arc::new(inf_str.as_bytes().to_vec())))
                } else if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
                    RespFrame::Integer(n as i64)
                } else {
                    let formatted = format!("{:.17}", n);
                    let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
                    RespFrame::BulkString(Some(Arc::new(trimmed.as_bytes().to_vec())))
                }
            }
            LuaValue::String(s) => {
                RespFrame::BulkString(Some(Arc::new(s.as_bytes().to_vec())))
            }
            LuaValue::Table(table) => {
                if let Ok(error_msg) = table.get::<String>("err") {
                    return RespFrame::error(error_msg);
                }
                
                let mut has_sequential_keys = false;
                let mut has_associative_keys = false;
                let mut max_sequential = 0i32;
                let mut associative_pairs = Vec::new();
                
                for pair in table.pairs::<LuaValue, LuaValue>() {
                    if let Ok((key, value)) = pair {
                        match key {
                            LuaValue::Integer(i) if i > 0 && i <= 1000 => {
                                has_sequential_keys = true;
                                max_sequential = max_sequential.max(i as i32);
                            }
                            LuaValue::String(_) | LuaValue::Integer(_) => {
                                has_associative_keys = true;
                                associative_pairs.push((key, value));
                            }
                            _ => {
                                has_associative_keys = true;
                                associative_pairs.push((key, value));
                            }
                        }
                    }
                }
                
                if has_associative_keys && !has_sequential_keys {
                    let mut items = Vec::new();
                    for (key, value) in associative_pairs {
                        items.push(self.lua_value_to_resp(key));
                        items.push(self.lua_value_to_resp(value));
                    }
                    
                    if items.is_empty() {
                        RespFrame::BulkString(None)
                    } else {
                        RespFrame::Array(Some(items))
                    }
                } else if has_sequential_keys {
                    let mut items = Vec::new();
                    
                    for i in 1..=max_sequential {
                        if let Ok(value) = table.get::<LuaValue>(i) {
                            match value {
                                LuaValue::Nil => break,
                                _ => items.push(self.lua_value_to_resp(value)),
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
                } else {
                    RespFrame::BulkString(None)
                }
            }
            _ => RespFrame::BulkString(None),
        }
    }
    
    fn calculate_script_sha1(&self, script: &str) -> String {
        let mut hasher = Sha1::new();
        hasher.update(script.as_bytes());
        hex::encode(hasher.finalize())
    }
}

/// Global singleton Lua engine
static mut LUA_ENGINE: Option<Arc<LuaEngine>> = None;
static INIT: std::sync::Once = std::sync::Once::new();

/// Get the global singleton Lua engine instance
pub fn get_lua_engine(storage: Arc<StorageEngine>) -> Result<Arc<LuaEngine>> {
    unsafe {
        INIT.call_once(|| {
            match LuaEngine::new(storage.clone()) {
                Ok(engine) => LUA_ENGINE = Some(Arc::new(engine)),
                Err(e) => eprintln!("Failed to initialize Lua engine: {}", e),
            }
        });
        
        LUA_ENGINE.clone().ok_or_else(|| {
            FerrousError::LuaError("Lua engine not initialized".to_string())
        })
    }
}