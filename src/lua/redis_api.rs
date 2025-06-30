//! Redis API Integration for Lua
//!
//! This module implements the Redis API for Lua scripts, including
//! redis.call, redis.pcall, and other Redis-specific functions.

use lazy_static::lazy_static;
use std::sync::{Arc, Once};

use crate::error::FerrousError;
use crate::protocol::resp::RespFrame;
use crate::storage::StorageEngine;

use super::error::{LuaError, Result};
use super::value::{Value, TableHandle};
use super::vm::ExecutionContext;

// Create a global storage engine and lazy initialization
lazy_static! {
    static ref GLOBAL_STORAGE: Arc<StorageEngine> = Arc::new(StorageEngine::new());
}

static INIT_ONCE: Once = Once::new();

/// A Redis API context for Lua scripts
pub struct RedisApiContext {
    /// Storage engine
    pub storage: Arc<StorageEngine>,
    
    /// Database ID
    pub db: usize,
}

impl RedisApiContext {
    /// Create a new Redis API context
    pub fn new(storage: Arc<StorageEngine>, db: usize) -> Self {
        RedisApiContext { storage, db }
    }
    
    /// Get a global dummy context for testing
    fn get_dummy_context() -> &'static RedisApiContext {
        // Initialize the dummy context once
        static mut DUMMY_CONTEXT: Option<RedisApiContext> = None;
        
        INIT_ONCE.call_once(|| {
            // Safety: This is only called once during initialization
            unsafe {
                DUMMY_CONTEXT = Some(RedisApiContext {
                    storage: GLOBAL_STORAGE.clone(),
                    db: 0,
                });
            }
        });
        
        // Safety: After initialization, this is read-only
        unsafe { DUMMY_CONTEXT.as_ref().unwrap() }
    }
    
    /// Register Redis API with a VM
    pub fn register_with_context(vm: &mut super::vm::LuaVM, _context: RedisApiContext) -> Result<()> {
        // Create redis table
        let redis_table = vm.create_table()?;
        
        // Register functions
        let functions = [
            ("call", redis_call as super::value::CFunction),
            ("pcall", redis_pcall as super::value::CFunction),
            ("log", redis_log_impl as super::value::CFunction),
            ("sha1hex", redis_sha1hex_impl as super::value::CFunction),
            ("error_reply", redis_error_reply_impl as super::value::CFunction),
            ("status_reply", redis_status_reply_impl as super::value::CFunction),
        ];
        
        for (name, func) in &functions {
            let name_handle = vm.create_string(name)?;
            vm.set_table(redis_table, Value::String(name_handle), Value::CFunction(*func))?;
        }
        
        // Register constants
        let constants = [
            ("LOG_DEBUG", 0),
            ("LOG_VERBOSE", 1),
            ("LOG_NOTICE", 2),
            ("LOG_WARNING", 3),
        ];
        
        for (name, value) in &constants {
            let name_handle = vm.create_string(name)?;
            vm.set_table(redis_table, Value::String(name_handle), Value::Number(*value as f64))?;
        }
        
        // Add redis table to globals
        let globals = vm.globals();
        let redis_key = vm.create_string("redis")?;
        vm.set_table(globals, Value::String(redis_key), Value::Table(redis_table))?;
        
        // In a real implementation, we would store the context for command usage
        // but we'll skip that for now
        
        Ok(())
    }

    
    /// Execute a Redis command
    fn execute_command(&self, command: &str, args: &[Vec<u8>]) -> std::result::Result<RespFrame, FerrousError> {
        // This is a simplified implementation
        // A full implementation would use the StorageEngine to execute the command
        
        println!("Executing Redis command: {} with {} args", command, args.len());
        
        // Return a placeholder result
        Ok(RespFrame::SimpleString(Arc::new(command.as_bytes().to_vec())))
    }
}

/// Get the context from the VM
fn get_context<'a>(_ctx: &'a mut ExecutionContext) -> Result<&'static RedisApiContext> {
    // Get the dummy context
    Ok(RedisApiContext::get_dummy_context())
}

/// Implementation of redis.call
pub fn redis_call(ctx: &mut ExecutionContext) -> Result<i32> {
    execute_redis_command(ctx, false)
}

/// Implementation of redis.pcall
pub fn redis_pcall(ctx: &mut ExecutionContext) -> Result<i32> {
    match execute_redis_command(ctx, true) {
        Ok(count) => Ok(count),
        Err(e) => {
            // For pcall, we don't propagate the error, but return it as a value
            let err_str = format!("{}", e);
            let err_handle = ctx.vm.create_string(&err_str)?;
            
            // Create error table
            let error_table = ctx.vm.create_table()?;
            
            // Set err field
            let err_key = ctx.vm.create_string("err")?;
            ctx.vm.set_table(error_table, Value::String(err_key), Value::String(err_handle))?;
            
            // Push error table as result
            ctx.push_result(Value::Table(error_table))?;
            
            Ok(1) // Return 1 value
        }
    }
}

/// Common implementation for redis.call and redis.pcall
fn execute_redis_command(ctx: &mut ExecutionContext, protected: bool) -> Result<i32> {
    let arg_count = ctx.get_arg_count();
    
    // Need at least command name
    if arg_count < 1 {
        return Err(LuaError::ArgError(1, "command name required".to_string()));
    }
    
    // Get command name
    let cmd_val = ctx.get_arg(0)?;
    let cmd_str = match cmd_val {
        Value::String(handle) => {
            let bytes = ctx.heap().get_string_bytes(handle)?;
            std::str::from_utf8(bytes)
                .map_err(|_| LuaError::InvalidEncoding)?
                .to_string()
        },
        _ => {
            return Err(LuaError::ArgError(1, format!("command name must be a string, got {}", cmd_val.type_name())));
        }
    };
    
    // Collect arguments
    let mut args = Vec::with_capacity(arg_count - 1);
    for i in 1..arg_count {
        let arg_val = ctx.get_arg(i)?;
        
        // Convert argument to bytes
        let arg_bytes = match arg_val {
            Value::String(handle) => {
                ctx.heap().get_string_bytes(handle)?.to_vec()
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
            Value::Table(_) => {
                // Tables get special handling for Redis commands
                // For simplicity, just convert to string representation
                "table".to_string().into_bytes()
            },
            _ => {
                return Err(LuaError::ArgError(i, format!("unsupported argument type: {}", arg_val.type_name())));
            }
        };
        
        args.push(arg_bytes);
    }
    
    // Get Redis context
    let redis_ctx = get_context(ctx)?;
    
    // Execute command
    let result = redis_ctx.execute_command(&cmd_str, &args);
    
    // Process result
    match result {
        Ok(resp) => {
            // Convert RESP frame to Lua value
            let lua_val = resp_to_lua(ctx, &resp)?;
            
            // Push result
            ctx.push_result(lua_val)?;
            
            Ok(1) // Return 1 value
        },
        Err(e) => {
            if protected {
                // For pcall, handled in the caller
                Err(LuaError::RuntimeError(format!("{}", e)))
            } else {
                // For call, propagate error
                Err(LuaError::RuntimeError(format!("{}", e)))
            }
        }
    }
}

/// Convert RESP frame to Lua value
fn resp_to_lua(ctx: &mut ExecutionContext, resp: &RespFrame) -> Result<Value> {
    match resp {
        RespFrame::SimpleString(s) => {
            let s_str = std::str::from_utf8(s).map_err(|_| LuaError::InvalidEncoding)?;
            let handle = ctx.vm.create_string(s_str)?;
            Ok(Value::String(handle))
        },
        RespFrame::Error(e) => {
            // Create error table
            let error_table = ctx.vm.create_table()?;
            
            // Set err field
            let err_key = ctx.vm.create_string("err")?;
            let e_str = std::str::from_utf8(e).map_err(|_| LuaError::InvalidEncoding)?;
            let err_handle = ctx.vm.create_string(e_str)?;
            ctx.vm.set_table(error_table, Value::String(err_key), Value::String(err_handle))?;
            
            Ok(Value::Table(error_table))
        },
        RespFrame::Integer(n) => {
            Ok(Value::Number(*n as f64))
        },
        RespFrame::BulkString(Some(s)) => {
            let s_str = std::str::from_utf8(s).map_err(|_| LuaError::InvalidEncoding)?;
            let handle = ctx.vm.create_string(s_str)?;
            Ok(Value::String(handle))
        },
        RespFrame::BulkString(None) => {
            Ok(Value::Nil)
        },
        RespFrame::Array(Some(items)) => {
            // Create a table
            let table = ctx.vm.create_table()?;
            
            // Fill table with array elements
            for (i, item) in items.iter().enumerate() {
                let value = resp_to_lua(ctx, item)?;
                ctx.vm.set_table_index(table, i + 1, value)?; // Lua arrays are 1-based
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
            // Create a table
            let table = ctx.vm.create_table()?;
            
            // Fill table with key-value pairs
            for (k, v) in pairs {
                let key = resp_to_lua(ctx, k)?;
                let value = resp_to_lua(ctx, v)?;
                ctx.vm.set_table(table, key, value)?;
            }
            
            Ok(Value::Table(table))
        },
        RespFrame::Set(items) => {
            // Create a table
            let table = ctx.vm.create_table()?;
            
            // Fill table with set elements
            for (i, item) in items.iter().enumerate() {
                let value = resp_to_lua(ctx, item)?;
                ctx.vm.set_table_index(table, i + 1, value)?; // Lua arrays are 1-based
            }
            
            Ok(Value::Table(table))
        },
    }
}

/// Implementation of redis.log
pub fn redis_log_impl(ctx: &mut ExecutionContext) -> Result<i32> {
    // Need at least level and message
    if ctx.get_arg_count() < 2 {
        return Err(LuaError::ArgError(1, "level and message required".to_string()));
    }
    
    // Get level
    let level = match ctx.get_arg(0)? {
        Value::Number(n) => n as i32,
        _ => {
            return Err(LuaError::ArgError(1, "level must be a number".to_string()));
        }
    };
    
    // Get message
    let message = match ctx.get_arg(1)? {
        Value::String(handle) => {
            let bytes = ctx.heap().get_string_bytes(handle)?;
            std::str::from_utf8(bytes)
                .map_err(|_| LuaError::InvalidEncoding)?
                .to_string()
        },
        _ => {
            return Err(LuaError::ArgError(2, "message must be a string".to_string()));
        }
    };
    
    // Log message (in a real implementation, this would go to Redis's logging system)
    println!("[REDIS_LOG] Level {}: {}", level, message);
    
    Ok(0) // No return values
}

/// Implementation of redis.sha1hex
pub fn redis_sha1hex_impl(ctx: &mut ExecutionContext) -> Result<i32> {
    // Need at least one argument
    if ctx.get_arg_count() < 1 {
        return Err(LuaError::ArgError(1, "string required".to_string()));
    }
    
    // Get string
    let input = match ctx.get_arg(0)? {
        Value::String(handle) => {
            let bytes = ctx.heap().get_string_bytes(handle)?;
            bytes.to_vec()
        },
        _ => {
            return Err(LuaError::ArgError(1, "string required".to_string()));
        }
    };
    
    // Calculate SHA1
    use sha1::{Sha1, Digest};
    let mut hasher = Sha1::new();
    hasher.update(&input);
    let result = hasher.finalize();
    let hex = hex::encode(result);
    
    // Return result
    let handle = ctx.vm.create_string(&hex)?;
    ctx.push_result(Value::String(handle))?;
    
    Ok(1) // One return value
}

/// Implementation of redis.error_reply
pub fn redis_error_reply_impl(ctx: &mut ExecutionContext) -> Result<i32> {
    // Need at least one argument
    if ctx.get_arg_count() < 1 {
        return Err(LuaError::ArgError(1, "error message required".to_string()));
    }
    
    // Get error message
    let message = match ctx.get_arg(0)? {
        Value::String(handle) => {
            let bytes = ctx.heap().get_string_bytes(handle)?;
            std::str::from_utf8(bytes)
                .map_err(|_| LuaError::InvalidEncoding)?
                .to_string()
        },
        _ => {
            return Err(LuaError::ArgError(1, "error message must be a string".to_string()));
        }
    };
    
    // Create error table
    let error_table = ctx.vm.create_table()?;
    
    // Set err field
    let err_key = ctx.vm.create_string("err")?;
    let err_value = ctx.vm.create_string(&message)?;
    ctx.vm.set_table(error_table, Value::String(err_key), Value::String(err_value))?;
    
    // Return error table
    ctx.push_result(Value::Table(error_table))?;
    
    Ok(1) // One return value
}

/// Implementation of redis.status_reply
pub fn redis_status_reply_impl(ctx: &mut ExecutionContext) -> Result<i32> {
    // Need at least one argument
    if ctx.get_arg_count() < 1 {
        return Err(LuaError::ArgError(1, "status message required".to_string()));
    }
    
    // Get status message
    let message = match ctx.get_arg(0)? {
        Value::String(handle) => {
            let bytes = ctx.heap().get_string_bytes(handle)?;
            std::str::from_utf8(bytes)
                .map_err(|_| LuaError::InvalidEncoding)?
                .to_string()
        },
        _ => {
            return Err(LuaError::ArgError(1, "status message must be a string".to_string()));
        }
    };
    
    // Create status table
    let status_table = ctx.vm.create_table()?;
    
    // Set ok field
    let ok_key = ctx.vm.create_string("ok")?;
    let ok_value = ctx.vm.create_string(&message)?;
    ctx.vm.set_table(status_table, Value::String(ok_key), Value::String(ok_value))?;
    
    // Return status table
    ctx.push_result(Value::Table(status_table))?;
    
    Ok(1) // One return value
}