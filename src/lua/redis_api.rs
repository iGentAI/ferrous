//! Redis API for Lua VM
//!
//! This module implements the Redis API for the Lua VM, allowing Lua scripts
//! to execute Redis commands through redis.call() and redis.pcall().

use std::sync::Arc;

use crate::error::FerrousError;
use crate::protocol::resp::RespFrame;
use crate::storage::StorageEngine;

use super::error::{LuaError, Result};
use super::value::{Value, TableHandle, StringHandle, CFunction};
use super::vm::{LuaVM, ExecutionContext};

/// Context for Redis command execution
pub struct RedisContext {
    /// The storage engine
    pub storage: Arc<StorageEngine>,
    /// Current database index
    pub db: usize,
    /// Keys array
    pub keys: Vec<Vec<u8>>,
    /// Arguments array
    pub args: Vec<Vec<u8>>,
}

impl RedisContext {
    /// Execute a Redis command
    pub fn execute_command(&self, _cmd: &str, _args: &[Vec<u8>]) -> std::result::Result<RespFrame, FerrousError> {
        // This is a stub implementation - in a real implementation, this would dispatch the command
        // For now, just return OK
        Ok(RespFrame::SimpleString(Arc::new("OK".as_bytes().to_vec())))
    }
}

/// Register Redis API with a VM
pub fn register_redis_api(vm: &mut LuaVM, ctx: RedisContext) -> Result<()> {
    // Create Redis table
    let redis_table = vm.create_table()?;
    
    // Register redis.call
    let call_name = vm.create_string("call")?;
    vm.set_table(
        redis_table.clone(),
        Value::String(call_name),
        Value::CFunction(redis_call),
    )?;
    
    // Register redis.pcall
    let pcall_name = vm.create_string("pcall")?;
    vm.set_table(
        redis_table.clone(),
        Value::String(pcall_name),
        Value::CFunction(redis_pcall),
    )?;
    
    // Register redis.sha1hex
    let sha1hex_name = vm.create_string("sha1hex")?;
    vm.set_table(
        redis_table.clone(),
        Value::String(sha1hex_name),
        Value::CFunction(redis_sha1hex),
    )?;
    
    // Register redis.status_reply
    let status_reply_name = vm.create_string("status_reply")?;
    vm.set_table(
        redis_table.clone(),
        Value::String(status_reply_name),
        Value::CFunction(redis_status_reply),
    )?;
    
    // Register redis.error_reply
    let error_reply_name = vm.create_string("error_reply")?;
    vm.set_table(
        redis_table.clone(),
        Value::String(error_reply_name),
        Value::CFunction(redis_error_reply),
    )?;
    
    // Register redis.log
    let log_name = vm.create_string("log")?;
    vm.set_table(
        redis_table.clone(),
        Value::String(log_name),
        Value::CFunction(redis_log),
    )?;
    
    // Register constants
    let constants = [
        ("LOG_DEBUG", 0),
        ("LOG_VERBOSE", 1),
        ("LOG_NOTICE", 2),
        ("LOG_WARNING", 3),
    ];
    
    for (name, value) in &constants {
        let name_handle = vm.create_string(name)?;
        vm.set_table(redis_table.clone(), Value::String(name_handle), Value::Number(*value as f64))?;
    }
    
    // Register redis table in globals
    let globals = vm.globals();
    let redis_name = vm.create_string("redis")?;
    vm.set_table(globals, Value::String(redis_name), Value::Table(redis_table))?;
    
    // Setup KEYS and ARGV tables
    setup_keys_argv_tables(vm, &ctx)?;
    
    // Store Redis context
    store_redis_context(vm, ctx)?;
    
    Ok(())
}

/// Setup KEYS and ARGV tables
fn setup_keys_argv_tables(vm: &mut LuaVM, ctx: &RedisContext) -> Result<()> {
    // Create KEYS table
    let keys_table = vm.create_table()?;
    
    // Fill KEYS table
    for (i, key) in ctx.keys.iter().enumerate() {
        let key_str = vm.create_string(&String::from_utf8_lossy(key))?;
        vm.set_table_index(keys_table.clone(), i + 1, Value::String(key_str))?;
    }
    
    // Create ARGV table
    let argv_table = vm.create_table()?;
    
    // Fill ARGV table
    for (i, arg) in ctx.args.iter().enumerate() {
        let arg_str = vm.create_string(&String::from_utf8_lossy(arg))?;
        vm.set_table_index(argv_table.clone(), i + 1, Value::String(arg_str))?;
    }
    
    // Register in globals
    let globals = vm.globals();
    let keys_name = vm.create_string("KEYS")?;
    vm.set_table(globals.clone(), Value::String(keys_name), Value::Table(keys_table))?;
    
    let argv_name = vm.create_string("ARGV")?;
    vm.set_table(globals, Value::String(argv_name), Value::Table(argv_table))?;
    
    Ok(())
}

/// Store Redis context in registry
fn store_redis_context(vm: &mut LuaVM, ctx: RedisContext) -> Result<()> {
    // In a real implementation, we would store the context in the registry
    // for use by redis.call and redis.pcall. For now, we'll just create a
    // marker to indicate the context exists.
    
    let registry = vm.heap.get_registry()?;
    let ctx_key = vm.create_string("__redis_context")?;
    
    // Create a table to store context info
    let ctx_table = vm.create_table()?;
    
    // Set db field
    let db_key = vm.create_string("db")?;
    vm.set_table(ctx_table.clone(), Value::String(db_key), Value::Number(ctx.db as f64))?;
    
    // Set marker
    let marker_key = vm.create_string("has_context")?;
    vm.set_table(ctx_table.clone(), Value::String(marker_key), Value::Boolean(true))?;
    
    // Store in registry
    vm.set_table(registry, Value::String(ctx_key), Value::Table(ctx_table))?;
    
    Ok(())
}

/// Get Redis context from VM
fn get_redis_context(_ctx: &ExecutionContext) -> std::result::Result<&'static RedisContext, FerrousError> {
    // In a real implementation, we would retrieve the context from the registry
    // For now, just return a dummy context for demonstration
    
    static mut DUMMY_CONTEXT: Option<RedisContext> = None;
    
    // Initialize dummy context if needed
    unsafe {
        if DUMMY_CONTEXT.is_none() {
            DUMMY_CONTEXT = Some(RedisContext {
                storage: Arc::new(StorageEngine::new_in_memory()),
                db: 0,
                keys: vec![],
                args: vec![],
            });
        }
        
        Ok(DUMMY_CONTEXT.as_ref().unwrap())
    }
}

/// Convert a Lua value to a Redis RESP frame
fn lua_value_to_resp(ctx: &mut ExecutionContext, value: &Value) -> std::result::Result<RespFrame, FerrousError> {
    match value {
        Value::Nil => Ok(RespFrame::Null),
        Value::Boolean(b) => Ok(RespFrame::Boolean(*b)),
        Value::Number(n) => {
            if n.fract() == 0.0 {
                Ok(RespFrame::Integer(*n as i64))
            } else {
                Ok(RespFrame::Double(*n))
            }
        },
        Value::String(handle) => {
            let bytes = ctx.vm.heap.get_string_bytes(handle.clone())?;
            Ok(RespFrame::BulkString(Some(Arc::new(bytes.to_vec()))))
        },
        Value::Table(handle) => {
            // Extract all the data we need first
            let table_data = {
                let table = ctx.vm.heap.get_table(handle.clone())?;
                
                // Collect array data separately
                let array_values = table.array.clone();
                
                // Check if it's a special reply type
                let is_error = false;
                let is_status = false;
                let mut error_message = None;
                let mut status_message = None;
                
                (array_values, is_error, is_status, error_message, status_message)
            };
            
            // Special reply types
            let (array_values, is_error, is_status, error_message, status_message) = table_data;
            
            if is_error && error_message.is_some() {
                return Ok(RespFrame::Error(Arc::new(error_message.unwrap().into_bytes())));
            } else if is_status && status_message.is_some() {
                return Ok(RespFrame::SimpleString(Arc::new(status_message.unwrap().into_bytes())));
            }
            
            // Convert array items
            let mut items = Vec::new();
            for item in array_values {
                let resp = lua_value_to_resp(ctx, &item)?;
                items.push(resp);
            }
            
            Ok(RespFrame::Array(Some(items)))
        },
        _ => Err(FerrousError::LuaError(format!("Cannot convert {} to Redis type", value.type_name())))
    }
}

/// Convert a Redis RESP frame to a Lua value
fn resp_to_lua_value(ctx: &mut ExecutionContext, frame: &RespFrame) -> Result<Value> {
    match frame {
        RespFrame::SimpleString(bytes) => {
            let s = String::from_utf8_lossy(bytes);
            let handle = ctx.vm.create_string(&s)?;
            Ok(Value::String(handle))
        },
        RespFrame::Error(bytes) => {
            // Create error reply table
            let table = ctx.vm.create_table()?;
            
            // Set type field
            let type_key = ctx.vm.create_string("__redis_type")?;
            let type_val = ctx.vm.create_string("error")?;
            ctx.vm.set_table(table.clone(), Value::String(type_key), Value::String(type_val))?;
            
            // Set message field
            let msg_key = ctx.vm.create_string("__error_msg")?;
            let s = String::from_utf8_lossy(bytes);
            let msg_val = ctx.vm.create_string(&s)?;
            ctx.vm.set_table(table.clone(), Value::String(msg_key), Value::String(msg_val))?;
            
            Ok(Value::Table(table))
        },
        RespFrame::Integer(n) => {
            Ok(Value::Number(*n as f64))
        },
        RespFrame::BulkString(Some(bytes)) => {
            let s = String::from_utf8_lossy(bytes);
            let handle = ctx.vm.create_string(&s)?;
            Ok(Value::String(handle))
        },
        RespFrame::BulkString(None) => {
            Ok(Value::Nil)
        },
        RespFrame::Array(Some(items)) => {
            let table = ctx.vm.create_table()?;
            
            for (i, item) in items.iter().enumerate() {
                let value = resp_to_lua_value(ctx, item)?;
                ctx.vm.set_table_index(table.clone(), i + 1, value)?;
            }
            
            Ok(Value::Table(table))
        },
        RespFrame::Array(None) => {
            Ok(Value::Nil)
        },
        RespFrame::Null => {
            Ok(Value::Nil)
        },
        RespFrame::Boolean(b) => {
            Ok(Value::Boolean(*b))
        },
        RespFrame::Double(d) => {
            Ok(Value::Number(*d))
        },
        RespFrame::Map(pairs) => {
            let table = ctx.vm.create_table()?;
            
            for (key_frame, val_frame) in pairs {
                let key = resp_to_lua_value(ctx, key_frame)?;
                let val = resp_to_lua_value(ctx, val_frame)?;
                ctx.vm.set_table(table.clone(), key, val)?;
            }
            
            Ok(Value::Table(table))
        },
        RespFrame::Set(items) => {
            // Convert to a regular array table
            let table = ctx.vm.create_table()?;
            
            for (i, item) in items.iter().enumerate() {
                let value = resp_to_lua_value(ctx, item)?;
                ctx.vm.set_table_index(table.clone(), i + 1, value)?;
            }
            
            Ok(Value::Table(table))
        },
    }
}

//
// Redis API Function Implementations
//

/// Implement redis.call
fn redis_call(ctx: &mut ExecutionContext) -> Result<i32> {
    match execute_redis_command(ctx, false) {
        Ok(count) => Ok(count),
        Err(e) => {
            Err(LuaError::RuntimeError(e.to_string()))
        }
    }
}

/// Implement redis.pcall
fn redis_pcall(ctx: &mut ExecutionContext) -> Result<i32> {
    // Wrap in protected call
    match execute_redis_command(ctx, true) {
        Ok(count) => Ok(count),
        Err(err) => {
            // Convert error to error reply
            let error_table = ctx.vm.create_table()?;
            
            // Set type field
            let type_key = ctx.vm.create_string("__redis_type")?;
            let type_val = ctx.vm.create_string("error")?;
            ctx.vm.set_table(error_table.clone(), Value::String(type_key), Value::String(type_val))?;
            
            // Set message field
            let msg_key = ctx.vm.create_string("__error_msg")?;
            let msg_val = ctx.vm.create_string(&err.to_string())?;
            ctx.vm.set_table(error_table.clone(), Value::String(msg_key), Value::String(msg_val))?;
            
            // Return error table as value
            ctx.push_result(Value::Table(error_table))?;
            
            Ok(1) // One result
        }
    }
}

/// Shared implementation for redis.call and redis.pcall
fn execute_redis_command(ctx: &mut ExecutionContext, protected: bool) -> Result<i32> {
    // Verify arguments
    if ctx.arg_count() < 1 {
        return Err(LuaError::ArgError(1, "command name required".to_string()));
    }
    
    // Get command name
    let cmd_arg = ctx.get_arg(0)?;
    let cmd_name = match cmd_arg {
        Value::String(handle) => {
            ctx.vm.heap.get_string_value(handle)?
        },
        _ => {
            return Err(LuaError::ArgError(1, format!(
                "command name must be a string, got {}", cmd_arg.type_name()
            )));
        }
    };
    
    // Collect arguments
    let mut args = Vec::with_capacity(ctx.arg_count() - 1);
    for i in 1..ctx.arg_count() {
        let arg = ctx.get_arg(i)?;
        let bytes = match arg {
            Value::String(handle) => {
                ctx.vm.heap.get_string_bytes(handle)?.to_vec()
            },
            Value::Number(n) => {
                n.to_string().into_bytes()
            },
            Value::Boolean(b) => {
                (if b { "1" } else { "0" }).to_string().into_bytes()
            },
            Value::Nil => {
                "".to_string().into_bytes()
            },
            _ => {
                return Err(LuaError::ArgError(i + 1, format!(
                    "cannot convert {} to Redis value", arg.type_name()
                )));
            }
        };
        
        args.push(bytes);
    }
    
    // Get Redis context
    let redis_ctx = match get_redis_context(ctx) {
        Ok(ctx) => ctx,
        Err(e) => return Err(LuaError::RuntimeError(format!("Redis context error: {}", e))),
    };
    
    // Execute command
    let result = match redis_ctx.execute_command(&cmd_name, &args) {
        Ok(resp) => resp,
        Err(e) => {
            return Err(LuaError::RuntimeError(format!("Redis command error: {}", e)));
        }
    };
    
    // Convert RESP to Lua value
    let lua_val = match resp_to_lua_value(ctx, &result) {
        Ok(val) => val,
        Err(e) => return Err(LuaError::RuntimeError(format!("Response conversion error: {}", e))),
    };
    
    // Return value
    ctx.push_result(lua_val)?;
    Ok(1) // One result
}

/// Implement redis.sha1hex
fn redis_sha1hex(ctx: &mut ExecutionContext) -> Result<i32> {
    // Verify arguments
    if ctx.arg_count() < 1 {
        return Err(LuaError::ArgError(1, "string argument required".to_string()));
    }
    
    // Get input string
    let input = ctx.get_arg(0)?;
    let bytes = match input {
        Value::String(handle) => {
            ctx.vm.heap.get_string_bytes(handle)?.to_vec()
        },
        _ => {
            return Err(LuaError::ArgError(1, format!(
                "expected string, got {}", input.type_name()
            )));
        }
    };
    
    // Calculate SHA1 hash
    use sha1::{Sha1, Digest};
    let mut hasher = Sha1::new();
    hasher.update(&bytes);
    let hash = hasher.finalize();
    
    // Convert to hex string
    let hex = hex::encode(hash);
    let handle = ctx.vm.create_string(&hex)?;
    
    // Return result
    ctx.push_result(Value::String(handle))?;
    
    Ok(1) // One result
}

/// Implement redis.status_reply
fn redis_status_reply(ctx: &mut ExecutionContext) -> Result<i32> {
    // Verify arguments
    if ctx.arg_count() < 1 {
        return Err(LuaError::ArgError(1, "status message required".to_string()));
    }
    
    // Get status message
    let msg = ctx.get_arg(0)?;
    let msg_str = match msg {
        Value::String(handle) => {
            ctx.vm.heap.get_string_value(handle)?
        },
        _ => {
            return Err(LuaError::ArgError(1, format!(
                "expected string, got {}", msg.type_name()
            )));
        }
    };
    
    // Create status reply table
    let table = ctx.vm.create_table()?;
    
    // Set type field
    let type_key = ctx.vm.create_string("__redis_type")?;
    let type_val = ctx.vm.create_string("status")?;
    ctx.vm.set_table(table.clone(), Value::String(type_key), Value::String(type_val))?;
    
    // Set message field
    let msg_key = ctx.vm.create_string("__status_msg")?;
    let msg_val = ctx.vm.create_string(&msg_str)?;
    ctx.vm.set_table(table.clone(), Value::String(msg_key), Value::String(msg_val))?;
    
    // Return status table
    ctx.push_result(Value::Table(table))?;
    
    Ok(1) // One result
}

/// Implement redis.error_reply
fn redis_error_reply(ctx: &mut ExecutionContext) -> Result<i32> {
    // Verify arguments
    if ctx.arg_count() < 1 {
        return Err(LuaError::ArgError(1, "error message required".to_string()));
    }
    
    // Get error message
    let msg = ctx.get_arg(0)?;
    let msg_str = match msg {
        Value::String(handle) => {
            ctx.vm.heap.get_string_value(handle)?
        },
        _ => {
            return Err(LuaError::ArgError(1, format!(
                "expected string, got {}", msg.type_name()
            )));
        }
    };
    
    // Create error reply table
    let table = ctx.vm.create_table()?;
    
    // Set type field
    let type_key = ctx.vm.create_string("__redis_type")?;
    let type_val = ctx.vm.create_string("error")?;
    ctx.vm.set_table(table.clone(), Value::String(type_key), Value::String(type_val))?;
    
    // Set message field
    let msg_key = ctx.vm.create_string("__error_msg")?;
    let msg_val = ctx.vm.create_string(&msg_str)?;
    ctx.vm.set_table(table.clone(), Value::String(msg_key), Value::String(msg_val))?;
    
    // Return error table
    ctx.push_result(Value::Table(table))?;
    
    Ok(1) // One result
}

/// Implement redis.log
fn redis_log(ctx: &mut ExecutionContext) -> Result<i32> {
    // Verify arguments
    if ctx.arg_count() < 2 {
        return Err(LuaError::ArgError(1, "log level and message required".to_string()));
    }
    
    // Get log level
    let level = ctx.get_arg(0)?;
    let level_num = match level {
        Value::Number(n) => n as i32,
        _ => {
            return Err(LuaError::ArgError(1, format!(
                "expected number for level, got {}", level.type_name()
            )));
        }
    };
    
    // Get log message
    let msg = ctx.get_arg(1)?;
    let msg_str = match msg {
        Value::String(handle) => {
            ctx.vm.heap.get_string_value(handle)?
        },
        _ => {
            return Err(LuaError::ArgError(2, format!(
                "expected string for message, got {}", msg.type_name()
            )));
        }
    };
    
    // Log message (would use Redis's real logging system in production)
    println!("[Redis Lua] [Level {}] {}", level_num, msg_str);
    
    Ok(0) // No return values
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_redis_api_compiles() {
        // Just a placeholder to ensure the module compiles
        assert!(true);
    }
}