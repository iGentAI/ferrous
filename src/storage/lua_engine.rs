//! Redis-compatible Lua execution engine using unified command architecture
//!
//! This module implements proper Redis Lua behavior by routing all redis.call()
//! commands through the unified command executor, ensuring atomic operations
//! and complete Redis compatibility.

use std::sync::Arc;
use std::time::Instant;
use mlua::{Lua, Result as LuaResult, MultiValue, Value as LuaValue};
use sha1::{Sha1, Digest};

use crate::error::{Result, FerrousError};
use crate::protocol::resp::RespFrame;
use crate::storage::StorageEngine;
use crate::storage::commands::executor::{UnifiedCommandExecutor, LuaCommandAdapter};

/// Command execution context passed from server to Lua engine
pub struct LuaCommandContext {
    pub db_index: usize,
    pub storage: Arc<StorageEngine>,
}

/// Single-threaded Lua execution engine with unified command processing
pub struct LuaEngine {
    // Removed local script_cache - using global cache at server level
}

impl LuaEngine {
    pub fn new(_storage: Arc<StorageEngine>) -> Result<Self> {
        Ok(LuaEngine {
            // No local cache needed
        })
    }
    
    /// Execute a Lua script using unified command processing
    pub fn eval(&self, script: &str, keys: Vec<Vec<u8>>, args: Vec<Vec<u8>>, ctx: &LuaCommandContext) -> Result<RespFrame> {
        let lua = self.create_lua_context(ctx)?;
        self.setup_keys_and_args(&lua, keys, args)?;
        
        let start_time = Instant::now();
        let result = lua.load(script).eval::<LuaValue>();
        
        match result {
            Ok(value) => Ok(self.lua_value_to_resp(value)),
            Err(e) => {
                match e {
                    mlua::Error::RuntimeError(ref msg) => {
                        if let Some(pos) = msg.find("REDIS_CALL_ABORT:") {
                            let mut error_content = &msg[pos + "REDIS_CALL_ABORT:".len()..];
                            error_content = error_content.trim_start_matches(|c: char| !c.is_alphabetic());
                            let end_pos = error_content.find('\n').unwrap_or(error_content.len());
                            let clean_error = error_content[..end_pos].trim().to_string();
                            let final_error = if clean_error.starts_with("ERR ") { 
                                clean_error 
                            } else { 
                                format!("ERR {}", clean_error) 
                            };
                            Err(FerrousError::LuaError(final_error))
                        } else {
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
    
    pub fn script_load(&self, script: &str) -> Result<String> {
        let lua = Lua::new();
        lua.load(script).exec().map_err(|e| {
            FerrousError::LuaError(format!("Script compilation error: {}", e))
        })?;
        
        let sha1 = self.calculate_script_sha1(script);
        
        Ok(sha1)
    }
    
    /// Create Lua context with unified redis.call implementation
    fn create_lua_context(&self, ctx: &LuaCommandContext) -> Result<Lua> {
        let lua = Lua::new();
        
        // Remove dangerous functions for sandboxing
        let globals = lua.globals();
        let dangerous_functions = ["os", "io", "debug", "package", "require", "dofile", "loadfile", "load"];
        for func in &dangerous_functions {
            globals.set(*func, mlua::Nil).map_err(|e| FerrousError::LuaError(e.to_string()))?;
        }
        
        // Create Redis API using unified command processing
        let redis_table = lua.create_table().map_err(|e| FerrousError::LuaError(e.to_string()))?;
        
        let storage_ref = ctx.storage.clone();
        let db_index = ctx.db_index;
        
        // redis.call: Errors terminate the script immediately
        let redis_call = lua.create_function(move |lua_ctx, cmd: MultiValue| -> LuaResult<LuaValue> {
            Self::execute_unified_redis_command(&storage_ref, lua_ctx, cmd, db_index, false)
        }).map_err(|e| FerrousError::LuaError(e.to_string()))?;
        
        let storage_ref_pcall = ctx.storage.clone();
        // redis.pcall: Errors return nil, script continues
        let redis_pcall = lua.create_function(move |lua_ctx, cmd: MultiValue| -> LuaResult<LuaValue> {
            Self::execute_unified_redis_command(&storage_ref_pcall, lua_ctx, cmd, db_index, true)
        }).map_err(|e| FerrousError::LuaError(e.to_string()))?;
        
        redis_table.set("call", redis_call).map_err(|e| FerrousError::LuaError(e.to_string()))?;
        redis_table.set("pcall", redis_pcall).map_err(|e| FerrousError::LuaError(e.to_string()))?;
        globals.set("redis", redis_table).map_err(|e| FerrousError::LuaError(e.to_string()))?;
        
        Ok(lua)
    }
    
    /// Execute Redis command using unified command processor
    fn execute_unified_redis_command(
        storage: &Arc<StorageEngine>,
        lua_ctx: &Lua,
        cmd: MultiValue,
        db_index: usize,
        is_pcall: bool,
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
        
        // Block commands that shouldn't be available in Lua scripts
        match cmd_name.as_str() {
            "EVAL" | "EVALSHA" | "SCRIPT" => {
                return Self::handle_command_error_with_context(
                    lua_ctx, 
                    "Redis scripting commands are not allowed inside Lua scripts".to_string(), 
                    is_pcall
                );
            }
            "SELECT" | "AUTH" | "QUIT" | "CLIENT" => {
                return Self::handle_command_error_with_context(
                    lua_ctx,
                    format!("'{}' command requires connection context not available in Lua scripts", cmd_name),
                    is_pcall
                );
            }
            "MULTI" | "EXEC" | "DISCARD" | "WATCH" | "UNWATCH" => {
                return Self::handle_command_error_with_context(
                    lua_ctx,
                    format!("'{}' command is not allowed inside Lua scripts - scripts are inherently atomic", cmd_name),
                    is_pcall
                );
            }
            "BLPOP" | "BRPOP" | "BZPOPMIN" | "BZPOPMAX" => {
                return Self::handle_command_error_with_context(
                    lua_ctx,
                    format!("'{}' blocking command is not allowed inside Lua scripts", cmd_name),
                    is_pcall
                );
            }
            "SUBSCRIBE" | "UNSUBSCRIBE" | "PSUBSCRIBE" | "PUNSUBSCRIBE" | "PUBSUB" => {
                return Self::handle_command_error_with_context(
                    lua_ctx,
                    format!("'{}' pub/sub command is not allowed inside Lua scripts", cmd_name),
                    is_pcall
                );
            }
            "MONITOR" | "RESET" => {
                return Self::handle_command_error_with_context(
                    lua_ctx,
                    format!("'{}' monitoring command is not allowed inside Lua scripts", cmd_name),
                    is_pcall
                );
            }
            "CONFIG" | "SHUTDOWN" | "DEBUG" | "ACL" => {
                return Self::handle_command_error_with_context(
                    lua_ctx,
                    format!("'{}' administrative command is not allowed inside Lua scripts", cmd_name),
                    is_pcall
                );
            }
            _ => {
                // Route through unified command processor
                let lua_adapter = LuaCommandAdapter::new(storage.clone());
                match lua_adapter.execute_lua_command(args, db_index) {
                    Ok(resp_frame) => Self::resp_frame_to_lua_value(lua_ctx, resp_frame, is_pcall),
                    Err(e) => Self::handle_command_error_with_context(lua_ctx, e.to_string(), is_pcall),
                }
            }
        }
    }
    
    /// Convert RESP frame to Lua value
    fn resp_frame_to_lua_value(lua_ctx: &Lua, frame: RespFrame, is_pcall: bool) -> LuaResult<LuaValue> {
        match frame {
            RespFrame::SimpleString(bytes) => {
                let string_val = String::from_utf8_lossy(&bytes).into_owned();
                match lua_ctx.create_string(&string_val) {
                    Ok(lua_string) => Ok(LuaValue::String(lua_string)),
                    Err(e) => Self::handle_command_error_with_context(lua_ctx, e.to_string(), is_pcall),
                }
            }
            RespFrame::BulkString(Some(bytes)) => {
                let string_val = String::from_utf8_lossy(&bytes).into_owned();
                match lua_ctx.create_string(&string_val) {
                    Ok(lua_string) => Ok(LuaValue::String(lua_string)),
                    Err(e) => Self::handle_command_error_with_context(lua_ctx, e.to_string(), is_pcall),
                }
            }
            RespFrame::BulkString(None) => Ok(LuaValue::Nil),
            RespFrame::Integer(i) => Ok(LuaValue::Integer(i)),
            RespFrame::Error(bytes) => {
                let error_msg = String::from_utf8_lossy(&bytes).into_owned();
                Self::handle_command_error_with_context(lua_ctx, error_msg, is_pcall)
            }
            RespFrame::Array(Some(frames)) => {
                // Convert Redis array to Lua table
                match lua_ctx.create_table() {
                    Ok(table) => {
                        for (idx, frame) in frames.iter().enumerate() {
                            let lua_val = Self::resp_frame_to_lua_value(lua_ctx, frame.clone(), is_pcall)?;
                            table.set(idx + 1, lua_val).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                        }
                        Ok(LuaValue::Table(table))
                    }
                    Err(e) => Self::handle_command_error_with_context(lua_ctx, e.to_string(), is_pcall),
                }
            }
            RespFrame::Array(None) => Ok(LuaValue::Nil),
            _ => Ok(LuaValue::Nil),
        }
    }
    
    /// Handle command errors with proper Redis semantics
    fn handle_command_error_with_context(_lua_ctx: &Lua, error_msg: String, is_pcall: bool) -> LuaResult<LuaValue> {
        let formatted_error = if error_msg.starts_with("ERR ") {
            error_msg
        } else {
            format!("ERR {}", error_msg)
        };
        
        if is_pcall {
            // redis.pcall: Return nil, script continues
            Ok(LuaValue::Nil)
        } else {
            // redis.call: Abort script execution immediately
            Err(mlua::Error::RuntimeError(format!("REDIS_CALL_ABORT:{}", formatted_error)))
        }
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
                // Convert Lua table to Redis array
                let mut items = Vec::new();
                for i in 1.. {
                    match table.get::<LuaValue>(i) {
                        Ok(LuaValue::Nil) => break,
                        Ok(value) => items.push(self.lua_value_to_resp(value)),
                        Err(_) => break,
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