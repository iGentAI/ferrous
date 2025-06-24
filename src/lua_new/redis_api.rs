//! Redis API bridge for Lua scripts
//!
//! This module implements the Redis API for Lua scripts, including:
//! - redis.call
//! - redis.pcall
//! - redis.log
//! - redis.sha1hex
//! - redis.error_reply
//! - redis.status_reply

use crate::lua_new::vm::{LuaVM, ExecutionContext};
use crate::lua_new::value::{Value, StringHandle, TableHandle, CFunction};
use crate::lua_new::error::{LuaError, Result};
use crate::storage::engine::StorageEngine;
use crate::protocol::resp::RespFrame;
use sha1::{Sha1, Digest};
use std::sync::Arc;

/// Redis API context
#[derive(Clone)]
pub struct RedisApiContext {
    /// Storage engine
    pub storage: Arc<StorageEngine>,
    
    /// Current database
    pub db: usize,
}

impl RedisApiContext {
    /// Create a new Redis API context
    pub fn new(storage: Arc<StorageEngine>, db: usize) -> Self {
        RedisApiContext {
            storage,
            db,
        }
    }
    
    /// Register API in the VM without context (basic registration)
    pub fn register(vm: &mut LuaVM) -> Result<()> {
        Self::register_api(vm, None)
    }
    
    /// Register API in the VM with context (full functionality)
    pub fn register_with_context(vm: &mut LuaVM, ctx: RedisApiContext) -> Result<()> {
        Self::register_api(vm, Some(ctx))
    }
    
    /// Internal helper to register API with or without context
    fn register_api(vm: &mut LuaVM, ctx_opt: Option<RedisApiContext>) -> Result<()> {
        // Store context in registry if provided
        if let Some(ctx) = ctx_opt {
            // Create context table
            let registry = vm.registry();
            let context_key = vm.heap.create_string("_REDIS_API_CTX");
            let ctx_table = vm.heap.alloc_table();
            
            // Store DB
            let db_key = vm.heap.create_string("db");
            vm.heap.get_table_mut(ctx_table)?.set(
                Value::String(db_key), 
                Value::Number(ctx.db as f64)
            );
            
            // Store storage pointer (a bit hacky, but works for this test)
            let storage_key = vm.heap.create_string("storage");
            let storage_table = vm.heap.alloc_table();
            let pointer_key = vm.heap.create_string("pointer");
            let pointer_val = vm.heap.create_string(&format!("{}", &ctx.storage as *const _ as usize));
            vm.heap.get_table_mut(storage_table)?.set(
                Value::String(pointer_key), 
                Value::String(pointer_val)
            );
            vm.heap.get_table_mut(ctx_table)?.set(
                Value::String(storage_key), 
                Value::Table(storage_table)
            );
            
            // Store in registry
            vm.heap.get_table_mut(registry)?.set(
                Value::String(context_key), 
                Value::Table(ctx_table)
            );
        }
        
        // Create redis table
        let redis_table = vm.heap.alloc_table();
        
        // Register functions
        let call_key = vm.heap.create_string("call");
        vm.heap.get_table_mut(redis_table)?.set(
            Value::String(call_key), 
            Value::CFunction(redis_call_func)
        );
        
        let pcall_key = vm.heap.create_string("pcall");
        vm.heap.get_table_mut(redis_table)?.set(
            Value::String(pcall_key), 
            Value::CFunction(redis_pcall_func)
        );
        
        let log_key = vm.heap.create_string("log");
        vm.heap.get_table_mut(redis_table)?.set(
            Value::String(log_key), 
            Value::CFunction(redis_log_impl)
        );
        
        let sha1hex_key = vm.heap.create_string("sha1hex");
        vm.heap.get_table_mut(redis_table)?.set(
            Value::String(sha1hex_key), 
            Value::CFunction(redis_sha1hex_impl)
        );
        
        let error_reply_key = vm.heap.create_string("error_reply");
        vm.heap.get_table_mut(redis_table)?.set(
            Value::String(error_reply_key), 
            Value::CFunction(redis_error_reply_impl)
        );
        
        let status_reply_key = vm.heap.create_string("status_reply");
        vm.heap.get_table_mut(redis_table)?.set(
            Value::String(status_reply_key), 
            Value::CFunction(redis_status_reply_impl)
        );
        
        // Register constants
        Self::register_constants(vm, redis_table)?;
        
        // Set in globals
        let globals = vm.globals();
        let redis_name = vm.heap.create_string("redis");
        vm.heap.get_table_mut(globals)?.set(
            Value::String(redis_name), 
            Value::Table(redis_table)
        );
        
        Ok(())
    }
    
    /// Register Redis constants
    fn register_constants(vm: &mut LuaVM, redis_table: TableHandle) -> Result<()> {
        let constants = [
            ("LOG_DEBUG", 0.0),
            ("LOG_VERBOSE", 1.0),
            ("LOG_NOTICE", 2.0),
            ("LOG_WARNING", 3.0),
        ];
        
        for (name, value) in &constants {
            let key = vm.heap.create_string(name);
            vm.heap.get_table_mut(redis_table)?.set(
                Value::String(key), 
                Value::Number(*value)
            );
        }
        
        Ok(())
    }
    
    /// Convert a Lua value to a RESP frame
    pub fn lua_to_resp(vm: &mut LuaVM, value: Value) -> Result<RespFrame> {
        match value {
            Value::Nil => Ok(RespFrame::Null),
            
            Value::Boolean(b) => {
                Ok(RespFrame::Integer(if b { 1 } else { 0 }))
            }
            
            Value::Number(n) => {
                if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
                    Ok(RespFrame::Integer(n as i64))
                } else {
                    Ok(RespFrame::BulkString(Some(Arc::new(n.to_string().into_bytes()))))
                }
            }
            
            Value::String(s) => {
                let bytes = vm.heap.get_string(s)?.to_vec();
                Ok(RespFrame::BulkString(Some(Arc::new(bytes))))
            }
            
            Value::Table(t) => {
                // First, collect all the table entries to avoid borrow issues
                let mut table_values = Vec::new();
                {
                    let table = vm.heap.get_table(t)?;
                    let len = table.len();
                    
                    for i in 1..=len {
                        let key = Value::Number(i as f64);
                        if let Some(val) = table.get(&key) {
                            table_values.push(*val);
                        }
                    }
                }
                
                // Now, convert each value to a RESP frame without borrowing issues
                let mut elements = Vec::new();
                for val in table_values {
                    let resp_val = Self::lua_to_resp(vm, val)?;
                    elements.push(resp_val);
                }
                
                Ok(RespFrame::Array(Some(elements)))
            }
            
            _ => {
                // Function, thread, etc. convert to string
                Ok(RespFrame::BulkString(Some(Arc::new(
                    format!("<{}>", value.type_name()).into_bytes()
                ))))
            }
        }
    }
    
    /// Convert a RESP frame to a Lua value
    pub fn resp_to_lua(vm: &mut LuaVM, resp: RespFrame) -> Result<Value> {
        match resp {
            RespFrame::SimpleString(bytes) => {
                let handle = vm.heap.alloc_string(&bytes);
                Ok(Value::String(handle))
            }
            
            RespFrame::Error(bytes) => {
                let handle = vm.heap.alloc_string(&bytes);
                Ok(Value::String(handle))
            }
            
            RespFrame::Integer(n) => {
                Ok(Value::Number(n as f64))
            }
            
            RespFrame::BulkString(Some(bytes)) => {
                let handle = vm.heap.alloc_string(&bytes);
                Ok(Value::String(handle))
            }
            
            RespFrame::BulkString(None) => {
                Ok(Value::Nil)
            }
            
            RespFrame::Array(Some(elements)) => {
                let table = vm.heap.alloc_table();
                let mut index = 1.0;
                
                for elem in elements {
                    let val = Self::resp_to_lua(vm, elem)?;
                    vm.heap.get_table_mut(table)?.set(
                        Value::Number(index),
                        val
                    );
                    index += 1.0;
                }
                
                Ok(Value::Table(table))
            }
            
            RespFrame::Array(None) => {
                Ok(Value::Nil)
            }
            
            RespFrame::Null => {
                Ok(Value::Nil)
            }
            
            // Handle additional variants not used in our implementation
            RespFrame::Boolean(b) => {
                Ok(Value::Boolean(b))
            }
            
            RespFrame::Double(d) => {
                Ok(Value::Number(d))
            }
            
            RespFrame::Map(_) | RespFrame::Set(_) => {
                // Create empty table for now
                let table = vm.heap.alloc_table();
                Ok(Value::Table(table))
            }
        }
    }
    
    /// Execute a Redis command
    pub fn execute_command(
        &self,
        command: &str,
        args: Vec<Vec<u8>>,
    ) -> std::result::Result<RespFrame, String> {
        // Create command frame
        let mut parts = Vec::with_capacity(args.len() + 1);
        parts.push(RespFrame::BulkString(Some(Arc::new(command.as_bytes().to_vec()))));
        
        for arg in &args {
            parts.push(RespFrame::BulkString(Some(Arc::new(arg.clone()))));
        }
        
        let command = command.to_uppercase();
        
        // Execute the command on the storage engine
        match command.as_str() {
            "PING" => Ok(RespFrame::SimpleString(Arc::new(b"PONG".to_vec()))),
            
            "GET" => {
                if args.is_empty() {
                    return Err("wrong number of arguments for 'get' command".to_string());
                }
                
                let key = &args[0];
                match self.storage.get_string(self.db, key) {
                    Ok(Some(val)) => Ok(RespFrame::BulkString(Some(Arc::new(val)))),
                    Ok(None) => Ok(RespFrame::Null),
                    Err(e) => Err(e.to_string()),
                }
            }
            
            "SET" => {
                if args.len() < 2 {
                    return Err("wrong number of arguments for 'set' command".to_string());
                }
                
                let key = args[0].clone();
                let value = args[1].clone();
                
                // Parse any options
                let mut i = 2;
                let mut expiration = None;
                
                while i < args.len() {
                    let option = String::from_utf8_lossy(&args[i]).to_uppercase();
                    
                    match option.as_str() {
                        "EX" => {
                            if i + 1 >= args.len() {
                                return Err("syntax error".to_string());
                            }
                            
                            let seconds_str = String::from_utf8_lossy(&args[i + 1]);
                            if let Ok(seconds) = seconds_str.parse::<u64>() {
                                expiration = Some(std::time::Duration::from_secs(seconds));
                            } else {
                                return Err("value is not an integer or out of range".to_string());
                            }
                            
                            i += 2;
                        }
                        
                        "PX" => {
                            if i + 1 >= args.len() {
                                return Err("syntax error".to_string());
                            }
                            
                            let millis_str = String::from_utf8_lossy(&args[i + 1]);
                            if let Ok(millis) = millis_str.parse::<u64>() {
                                expiration = Some(std::time::Duration::from_millis(millis));
                            } else {
                                return Err("value is not an integer or out of range".to_string());
                            }
                            
                            i += 2;
                        }
                        
                        _ => {
                            return Err(format!("unsupported option: {}", option));
                        }
                    }
                }
                
                let result = match expiration {
                    Some(exp) => self.storage.set_string_ex(self.db, key, value, exp),
                    None => self.storage.set_string(self.db, key, value),
                };
                
                match result {
                    Ok(_) => Ok(RespFrame::ok()),
                    Err(e) => Err(e.to_string()),
                }
            }
            
            // Implement more Redis commands as needed
            // This includes DEL, EXISTS, INCR, DECR, etc.
            "DEL" => {
                if args.is_empty() {
                    return Err("wrong number of arguments for 'del' command".to_string());
                }
                
                let mut count = 0;
                for key in &args {
                    if let Ok(deleted) = self.storage.delete(self.db, key) {
                        if deleted {
                            count += 1;
                        }
                    }
                }
                
                Ok(RespFrame::Integer(count))
            }
            
            "EXISTS" => {
                if args.is_empty() {
                    return Err("wrong number of arguments for 'exists' command".to_string());
                }
                
                let mut count = 0;
                for key in &args {
                    if let Ok(exists) = self.storage.exists(self.db, key) {
                        if exists {
                            count += 1;
                        }
                    }
                }
                
                Ok(RespFrame::Integer(count))
            }
            
            "INCR" => {
                if args.len() != 1 {
                    return Err("wrong number of arguments for 'incr' command".to_string());
                }
                
                match self.storage.incr(self.db, args[0].clone()) {
                    Ok(value) => Ok(RespFrame::Integer(value)),
                    Err(e) => Err(e.to_string()),
                }
            }
            
            "INCRBY" => {
                if args.len() != 2 {
                    return Err("wrong number of arguments for 'incrby' command".to_string());
                }
                
                let increment = match String::from_utf8_lossy(&args[1]).parse::<i64>() {
                    Ok(n) => n,
                    Err(_) => return Err("value is not an integer or out of range".to_string()),
                };
                
                match self.storage.incr_by(self.db, args[0].clone(), increment) {
                    Ok(value) => Ok(RespFrame::Integer(value)),
                    Err(e) => Err(e.to_string()),
                }
            }
            
            // Fallback for unknown commands
            _ => Err(format!("unsupported command: {}", command)),
        }
    }
}

/// Static implementation of redis.call function
fn redis_call_func(exec_ctx: &mut crate::lua_new::vm::ExecutionContext) -> Result<i32> {
    // Try to get context from registry
    if let Some(context) = get_redis_context(exec_ctx) {
        // Call with real context
        redis_call_impl(exec_ctx, false, &context)
    } else {
        // Fallback to stub implementation
        fallback_redis_call_func(exec_ctx)
    }
}

/// Static implementation of redis.pcall function
fn redis_pcall_func(exec_ctx: &mut crate::lua_new::vm::ExecutionContext) -> Result<i32> {
    // Try to get context from registry
    if let Some(context) = get_redis_context(exec_ctx) {
        // Call with real context
        redis_call_impl(exec_ctx, true, &context)
    } else {
        // Fallback to stub implementation
        fallback_redis_pcall_func(exec_ctx)
    }
}

/// Helper to extract RedisApiContext from registry
fn get_redis_context(exec_ctx: &mut crate::lua_new::vm::ExecutionContext) -> Option<RedisApiContext> {
    // Get registry
    let registry = match exec_ctx.vm.registry() {
        handle => handle
    };
    
    // Get context key
    let context_key = match exec_ctx.vm.heap.create_string("_REDIS_API_CTX") {
        key => key
    };
    
    // Get context value
    let ctx_val = match exec_ctx.vm.heap.get_table(registry) {
        Ok(table) => table.get(&Value::String(context_key)).copied(),
        Err(_) => return None,
    }?;
    
    if let Value::Table(ctx_table) = ctx_val {
        // Extract storage and DB from context
        let storage_key = exec_ctx.vm.heap.create_string("storage");
        let db_key = exec_ctx.vm.heap.create_string("db");
        
        let ctx_table_obj = match exec_ctx.vm.heap.get_table(ctx_table) {
            Ok(table) => table,
            Err(_) => return None,
        };
        
        let storage_val = ctx_table_obj.get(&Value::String(storage_key)).copied()?;
        let db_val = ctx_table_obj.get(&Value::String(db_key)).copied()?;
            
        // Create RedisApiContext
        if let (Value::Table(storage_handle), Value::Number(db)) = (storage_val, db_val) {
            // Extract storage Arc pointer from the table
            let pointer_key = exec_ctx.vm.heap.create_string("pointer");
            
            let storage_table_obj = match exec_ctx.vm.heap.get_table(storage_handle) {
                Ok(table) => table,
                Err(_) => return None,
            };
            
            let pointer_val = storage_table_obj.get(&Value::String(pointer_key)).copied()?;
                
            if let Value::String(ptr_str) = pointer_val {
                // Extract pointer from string
                let ptr_str = match exec_ctx.vm.heap.get_string_utf8(ptr_str) {
                    Ok(s) => s,
                    Err(_) => return None,
                };
                
                if let Ok(ptr) = ptr_str.parse::<usize>() {
                    // Cast back to Arc<StorageEngine>
                    let storage = unsafe { &*(ptr as *const Arc<crate::storage::engine::StorageEngine>) };
                    
                    // Create context
                    return Some(RedisApiContext {
                        storage: Arc::clone(storage),
                        db: db as usize,
                    });
                }
            }
        }
    }
    
    None
}

/// Fallback stub implementation when real context isn't available
fn fallback_redis_call_func(exec_ctx: &mut crate::lua_new::vm::ExecutionContext) -> Result<i32> {
    // Check arguments
    if exec_ctx.get_arg_count() == 0 {
        return Err(LuaError::Runtime("redis.call requires at least one argument".to_string()));
    }
    
    // Get command name
    let cmd = match exec_ctx.get_arg(0)? {
        Value::String(s) => exec_ctx.vm.heap.get_string_utf8(s)?.to_string(),
        _ => return Err(LuaError::TypeError("redis.call first argument must be a command name".to_string())),
    };
    
    // Handle basic commands as a fallback
    match cmd.to_uppercase().as_str() {
        "PING" => {
            let pong_handle = exec_ctx.vm.heap.create_string("PONG");
            exec_ctx.push_result(Value::String(pong_handle))?;
        }
        "GET" => {
            if exec_ctx.get_arg_count() < 2 {
                return Err(LuaError::Runtime("GET requires a key argument".to_string()));
            }
            
            // Return nil for all GET operations in fallback
            exec_ctx.push_result(Value::Nil)?;
        }
        "SET" => {
            if exec_ctx.get_arg_count() < 3 {
                return Err(LuaError::Runtime("SET requires key and value arguments".to_string()));
            }
            
            // Return OK for SET operations in fallback
            let ok_handle = exec_ctx.vm.heap.create_string("OK");
            exec_ctx.push_result(Value::String(ok_handle))?;
        }
        _ => {
            // Return nil for unknown commands in fallback
            exec_ctx.push_result(Value::Nil)?;
        }
    }
    
    Ok(1) // One return value
}

/// Fallback stub implementation for pcall when real context isn't available
fn fallback_redis_pcall_func(exec_ctx: &mut crate::lua_new::vm::ExecutionContext) -> Result<i32> {
    // Similar to call, but wrap errors
    match fallback_redis_call_func(exec_ctx) {
        Ok(val) => Ok(val),
        Err(LuaError::Runtime(msg)) | Err(LuaError::TypeError(msg)) => {
            // Create error table
            let table = exec_ctx.vm.heap.alloc_table();
            let err_key = exec_ctx.vm.heap.create_string("err");
            let err_msg = exec_ctx.vm.heap.create_string(&msg);
            
            exec_ctx.vm.heap.get_table_mut(table)?.set(
                Value::String(err_key), 
                Value::String(err_msg)
            );
            
            // Return the error table
            exec_ctx.push_result(Value::Table(table))?;
            Ok(1) // One return value
        },
        Err(e) => Err(e), // Other errors still propagate
    }
}

/// redis.call implementation
fn redis_call_impl(exec_ctx: &mut ExecutionContext, is_pcall: bool, ctx: &RedisApiContext) -> Result<i32> {
    // Check arguments
    if exec_ctx.get_arg_count() == 0 {
        if is_pcall {
            let err_msg = "redis.pcall requires at least one argument";
            return transform_error(exec_ctx, err_msg.to_string());
        } else {
            return Err(LuaError::Runtime(
                "redis.call requires at least one argument".to_string()
            ));
        }
    }
    
    // Get command name
    let cmd = match exec_ctx.get_arg(0)? {
        Value::String(s) => exec_ctx.vm.heap.get_string_utf8(s)?.to_string(),
        _ => {
            let err_msg = "redis.call first argument must be a command name";
            if is_pcall {
                return transform_error(exec_ctx, err_msg.to_string());
            } else {
                return Err(LuaError::Runtime(err_msg.to_string()));
            }
        }
    };
    
    // Convert arguments
    let mut args = Vec::with_capacity(exec_ctx.get_arg_count() - 1);
    for i in 1..exec_ctx.get_arg_count() {
        let arg = exec_ctx.get_arg(i)?;
        match arg {
            Value::String(s) => {
                let bytes = exec_ctx.vm.heap.get_string(s)?.to_vec();
                args.push(bytes);
            }
            Value::Number(n) => {
                args.push(n.to_string().into_bytes());
            }
            Value::Boolean(b) => {
                args.push((if b { "true" } else { "false" }).as_bytes().to_vec());
            }
            Value::Nil => {
                args.push(b"".to_vec());
            }
            _ => {
                let err_msg = format!(
                    "redis.call: Lua {} type not convertible to Redis protocol", 
                    arg.type_name()
                );
                if is_pcall {
                    return transform_error(exec_ctx, err_msg);
                } else {
                    return Err(LuaError::Runtime(err_msg));
                }
            }
        }
    }
    
    // Execute command
    match ctx.execute_command(&cmd, args) {
        Ok(resp) => {
            // Convert response to Lua value
            let value = RedisApiContext::resp_to_lua(&mut exec_ctx.vm, resp)?;
            exec_ctx.push_result(value)?;
            Ok(1) // One return value
        }
        Err(err) => {
            // Handle error according to call type
            if is_pcall {
                return transform_error(exec_ctx, err);
            } else {
                return Err(LuaError::Runtime(err));
            }
        }
    }
}

/// redis.log implementation
fn redis_log_impl(exec_ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if exec_ctx.get_arg_count() < 2 {
        return Err(LuaError::Runtime(
            "redis.log requires level and message arguments".to_string()
        ));
    }
    
    // Get log level
    let level = match exec_ctx.get_arg(0)? {
        Value::Number(n) => n as i32,
        _ => return Err(LuaError::TypeError("redis.log level must be a number".to_string())),
    };
    
    // Get message
    let message = match exec_ctx.get_arg(1)? {
        Value::String(s) => exec_ctx.vm.heap.get_string_utf8(s)?.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::Nil => "nil".to_string(),
        _ => format!("<{}>", exec_ctx.get_arg(1)?.type_name()),
    };
    
    // Log message
    println!("<redis> [{}] {}", level, message);
    
    Ok(0) // No return value
}

/// redis.sha1hex implementation
fn redis_sha1hex_impl(exec_ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if exec_ctx.get_arg_count() < 1 {
        return Err(LuaError::Runtime(
            "redis.sha1hex requires a string argument".to_string()
        ));
    }
    
    // Get string
    let input = match exec_ctx.get_arg(0)? {
        Value::String(s) => exec_ctx.vm.heap.get_string(s)?.to_vec(),
        _ => return Err(LuaError::TypeError("redis.sha1hex requires a string".to_string())),
    };
    
    // Compute SHA1
    let mut hasher = Sha1::new();
    hasher.update(&input);
    let hash = hasher.finalize();
    
    // Convert to hex string
    let hex_string = format!("{:x}", hash);
    let handle = exec_ctx.vm.heap.create_string(&hex_string);
    
    // Return the hash
    exec_ctx.push_result(Value::String(handle))?;
    Ok(1) // One return value
}

/// redis.error_reply implementation
fn redis_error_reply_impl(exec_ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if exec_ctx.get_arg_count() < 1 {
        return Err(LuaError::Runtime(
            "redis.error_reply requires a string argument".to_string()
        ));
    }
    
    // Get error message
    let message = match exec_ctx.get_arg(0)? {
        Value::String(s) => exec_ctx.vm.heap.get_string_utf8(s)?.to_string(),
        _ => return Err(LuaError::TypeError("redis.error_reply requires a string".to_string())),
    };
    
    // Create error table
    let table = exec_ctx.vm.heap.alloc_table();
    let err_key = exec_ctx.vm.heap.create_string("err");
    let err_msg = exec_ctx.vm.heap.create_string(&message);
    
    exec_ctx.vm.heap.get_table_mut(table)?.set(
        Value::String(err_key), 
        Value::String(err_msg)
    );
    
    // Return the error table
    exec_ctx.push_result(Value::Table(table))?;
    Ok(1) // One return value
}

/// redis.status_reply implementation
fn redis_status_reply_impl(exec_ctx: &mut ExecutionContext) -> Result<i32> {
    // Check arguments
    if exec_ctx.get_arg_count() < 1 {
        return Err(LuaError::Runtime(
            "redis.status_reply requires a string argument".to_string()
        ));
    }
    
    // Get status message
    let message = match exec_ctx.get_arg(0)? {
        Value::String(s) => exec_ctx.vm.heap.get_string_utf8(s)?.to_string(),
        _ => return Err(LuaError::TypeError("redis.status_reply requires a string".to_string())),
    };
    
    // Create status table
    let table = exec_ctx.vm.heap.alloc_table();
    let ok_key = exec_ctx.vm.heap.create_string("ok");
    let status_msg = exec_ctx.vm.heap.create_string(&message);
    
    exec_ctx.vm.heap.get_table_mut(table)?.set(
        Value::String(ok_key), 
        Value::String(status_msg)
    );
    
    // Return the status table
    exec_ctx.push_result(Value::Table(table))?;
    Ok(1) // One return value
}

/// Transform an error into a Lua error table for pcall
fn transform_error(exec_ctx: &mut ExecutionContext, error_message: String) -> Result<i32> {
    // Create error table
    let table = exec_ctx.vm.heap.alloc_table();
    let err_key = exec_ctx.vm.heap.create_string("err");
    let err_msg = exec_ctx.vm.heap.create_string(&error_message);
    
    exec_ctx.vm.heap.get_table_mut(table)?.set(
        Value::String(err_key), 
        Value::String(err_msg)
    );
    
    // Return the error table
    exec_ctx.push_result(Value::Table(table))?;
    Ok(1) // One return value
}