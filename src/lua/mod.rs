//! Lua Virtual Machine for Ferrous
//! 
//! This module provides a Redis-compatible Lua 5.1 VM implementation
//! using fine-grained Rc<RefCell> for proper Lua semantics.

use std::sync::Arc;

// Core modules
pub mod arena;
pub mod handle;
pub mod value;
pub mod metamethod;
pub mod resource;
pub mod error;
pub mod compiler;
pub mod codegen;
pub mod lexer;
pub mod parser;
pub mod ast;

// RC-based implementation (Current)
pub mod rc_handle;
pub mod rc_value;
pub mod rc_heap;
pub mod rc_vm;
pub mod rc_stdlib;

// Tests
#[cfg(test)]
mod test_basic;

// Re-exports
pub use self::error::{LuaError, LuaResult};
pub use self::value::Value;
pub use self::compiler::{compile, CompiledModule};
pub use self::codegen::OpCode;
pub use self::handle::{StringHandle, TableHandle, ClosureHandle, ThreadHandle};

// Current VM type
pub use self::rc_vm::RcVM;

// Implement handle_lua_command directly in the lua module
pub fn handle_lua_command(
    storage: &Arc<crate::storage::StorageEngine>,
    cmd: &str, 
    parts: &[crate::protocol::resp::RespFrame]
) -> crate::error::Result<crate::protocol::resp::RespFrame> {
    use crate::protocol::resp::RespFrame;
    use std::str;
    
    match cmd.to_lowercase().as_str() {
        "eval" => {
            if parts.len() < 3 {
                return Ok(RespFrame::error("ERR wrong number of arguments for 'eval' command"));
            }
            
            // Extract script
            let script = match &parts[1] {
                RespFrame::BulkString(Some(bytes)) => {
                    match str::from_utf8(bytes) {
                        Ok(s) => s,
                        Err(_) => return Ok(RespFrame::error("ERR invalid script")),
                    }
                }
                _ => return Ok(RespFrame::error("ERR invalid script"))
            };
            
            // Extract number of keys
            let num_keys = match &parts[2] {
                RespFrame::BulkString(Some(bytes)) => {
                    match str::from_utf8(bytes) {
                        Ok(s) => match s.parse::<usize>() {
                            Ok(n) => n,
                            Err(_) => return Ok(RespFrame::error("ERR invalid number of keys"))
                        },
                        Err(_) => return Ok(RespFrame::error("ERR invalid number of keys")),
                    }
                }
                _ => return Ok(RespFrame::error("ERR invalid number of keys"))
            };
            
            // Extract keys and arguments
            let mut keys = Vec::new();
            let mut args = Vec::new();
            
            // Basic validation
            if parts.len() < 3 + num_keys {
                return Ok(RespFrame::error("ERR wrong number of arguments for 'eval' command"));
            }
            
            // Extract keys
            for i in 0..num_keys {
                match &parts[3 + i] {
                    RespFrame::BulkString(Some(bytes)) => keys.push(bytes.to_vec()),
                    _ => return Ok(RespFrame::error("ERR invalid key"))
                }
            }
            
            // Extract args
            for i in 3 + num_keys..parts.len() {
                match &parts[i] {
                    RespFrame::BulkString(Some(bytes)) => args.push(bytes.to_vec()),
                    _ => return Ok(RespFrame::error("ERR invalid argument"))
                }
            }
            
            // Create context
            let context = ScriptContext {
                storage: storage.clone(),
                db: storage.get_current_db(),
                keys,
                args,
                timeout: std::time::Duration::from_secs(5), // Default timeout
            };
            
            // Create a new LuaGIL
            let engine = match LuaGIL::new() {
                Ok(gil) => gil,
                Err(e) => return Ok(RespFrame::error(format!("ERR {}", e)))
            };
            
            // Evaluate script and return result
            match engine.eval(script, context) {
                Ok(result) => Ok(result),
                Err(e) => Ok(RespFrame::error(format!("ERR {}", e)))
            }
        },
        "evalsha" => {
            Ok(RespFrame::error("ERR EVALSHA not implemented yet"))
        },
        "script" => {
            if parts.len() < 2 {
                return Ok(RespFrame::error("ERR wrong number of arguments for 'script' command"));
            }
            
            // Get subcommand
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
                "load" | "exists" | "flush" | "kill" => {
                    Ok(RespFrame::error("ERR SCRIPT subcommand not implemented yet"))
                }
                _ => Ok(RespFrame::error(format!("ERR Unknown subcommand '{}'", subcommand))),
            }
        },
        _ => Ok(RespFrame::error(format!("ERR unknown command '{}'", cmd))),
    }
}

use std::sync::{Mutex, RwLock};
use std::collections::HashMap;
use std::time::Duration;
use crate::error::FerrousError;
use crate::protocol::resp::RespFrame;
use crate::storage::StorageEngine;
use std::sync::atomic::{AtomicBool, Ordering};

/// Script execution context containing Redis state and arguments
#[derive(Clone)]
pub struct ScriptContext {
    pub storage: Arc<StorageEngine>,
    pub db: usize,
    pub keys: Vec<Vec<u8>>,
    pub args: Vec<Vec<u8>>,
    pub timeout: Duration,
}

/// Common interface for Lua VMs
pub trait LuaVMInterface {
    /// Set script execution context
    fn set_context(&mut self, context: ScriptContext) -> LuaResult<()>;
    
    /// Evaluate a Lua script
    fn eval_script(&mut self, script: &str) -> LuaResult<Value>;
}

// Implement trait for RcVM
impl LuaVMInterface for rc_vm::RcVM {
    fn set_context(&mut self, _context: ScriptContext) -> LuaResult<()> {
        // The RcVM doesn't yet have a set_context method
        // This is a placeholder implementation
        Ok(())
    }
    
    fn eval_script(&mut self, script: &str) -> LuaResult<value::Value> {
        // Compile and execute the script
        let module = compile(script)?;
        // Execute the module and convert the rc_value::Value to lua::value::Value
        let rc_result = self.execute_module(&module, &[])?;
        
        // Convert to legacy Value type for compatibility
        let legacy_result = match rc_result {
            rc_value::Value::Nil => value::Value::Nil,
            rc_value::Value::Boolean(b) => value::Value::Boolean(b),
            rc_value::Value::Number(n) => value::Value::Number(n),
            // For more complex types, just return Nil for now
            _ => value::Value::Nil,
        };
        
        Ok(legacy_result)
    }
}

/// Global Interpreter Lock for Lua VMs
/// Manages a pool of Lua VMs and script caching
pub struct LuaGIL {
    /// Pool of available RC-based VMs
    rc_vm_pool: Arc<Mutex<Vec<RcVM>>>,
    
    /// Script cache mapping SHA1 hash to compiled scripts
    script_cache: Arc<RwLock<HashMap<String, String>>>,
    
    /// Kill flag for currently executing script
    kill_flag: Arc<AtomicBool>,
}

impl LuaGIL {
    /// Create a new Lua Global Interpreter Lock
    pub fn new() -> Result<Self, FerrousError> {
        // Initialize RC VM pool
        let mut rc_pool = Vec::new();
        
        // Create initial RC-based VM
        match RcVM::new() {
            Ok(mut vm) => {
                // Initialize standard library
                if let Err(e) = vm.init_stdlib() {
                    return Err(FerrousError::LuaError(format!("Failed to initialize stdlib: {}", e)));
                }
                rc_pool.push(vm);
            },
            Err(e) => return Err(FerrousError::LuaError(format!("Failed to create Lua VM: {}", e))),
        }
        
        Ok(LuaGIL {
            rc_vm_pool: Arc::new(Mutex::new(rc_pool)),
            script_cache: Arc::new(RwLock::new(HashMap::new())),
            kill_flag: Arc::new(AtomicBool::new(false)),
        })
    }
    
    /// Evaluate a Lua script
    pub fn eval(&self, script: &str, context: ScriptContext) -> Result<RespFrame, FerrousError> {
        // Use RC-based VM
        let mut vm = self.get_rc_vm()?;
        
        // Set up context
        if let Err(e) = vm.set_context(context) {
            self.return_rc_vm(vm);
            return Err(FerrousError::LuaError(format!("Failed to set context: {}", e)));
        }
        
        // Execute script
        let result = match vm.eval_script(script) {
            Ok(val) => {
                let resp = self.lua_to_resp_rc(val)?;
                self.return_rc_vm(vm);
                resp
            },
            Err(e) => {
                self.return_rc_vm(vm);
                return Err(FerrousError::LuaError(format!("Script error: {}", e)));
            }
        };
        
        Ok(result)
    }
    
    /// Evaluate a cached script by SHA1 hash
    pub fn evalsha(&self, sha1: &str, context: ScriptContext) -> Result<RespFrame, FerrousError> {
        // Check cache
        let script = {
            let cache = self.script_cache.read().map_err(|_| {
                FerrousError::Internal("Failed to acquire script cache read lock".to_string())
            })?;
            
            cache.get(sha1).cloned()
        };
        
        match script {
            Some(script) => self.eval(&script, context),
            None => Err(FerrousError::ScriptNotFound(sha1.to_string())),
        }
    }
    
    /// Load a script into the cache
    pub fn script_load(&self, script: &str) -> Result<String, FerrousError> {
        // Calculate SHA1 hash
        use sha1::{Sha1, Digest};
        let mut hasher = Sha1::new();
        hasher.update(script.as_bytes());
        let result = hasher.finalize();
        let sha1 = format!("{:x}", result);
        
        // Add to cache
        {
            let mut cache = self.script_cache.write().map_err(|_| {
                FerrousError::Internal("Failed to acquire script cache write lock".to_string())
            })?;
            cache.insert(sha1.clone(), script.to_string());
        }
        
        Ok(sha1)
    }
    
    /// Check if a script exists in the cache
    pub fn script_exists(&self, sha1: &str) -> bool {
        if let Ok(cache) = self.script_cache.read() {
            cache.contains_key(sha1)
        } else {
            false
        }
    }
    
    /// Flush all scripts from the cache
    pub fn script_flush(&self) {
        if let Ok(mut cache) = self.script_cache.write() {
            cache.clear();
        }
    }
    
    /// Kill the currently running script
    pub fn kill_script(&self) -> Result<(), FerrousError> {
        // Set kill flag
        self.kill_flag.store(true, Ordering::Relaxed);
        
        // TODO: Interrupt running VM
        
        Ok(())
    }
    
    /// Get an RC-based VM from the pool
    fn get_rc_vm(&self) -> Result<RcVM, FerrousError> {
        let mut pool = self.rc_vm_pool.lock().map_err(|_| {
            FerrousError::Internal("Failed to acquire VM pool lock".to_string())
        })?;
        
        if let Some(vm) = pool.pop() {
            Ok(vm)
        } else {
            // Create a new VM if pool is empty
            let mut new_vm = RcVM::new().map_err(|e| {
                FerrousError::LuaError(format!("Failed to create Lua VM: {}", e))
            })?;
            
            // Initialize standard library
            new_vm.init_stdlib().map_err(|e| {
                FerrousError::LuaError(format!("Failed to initialize stdlib: {}", e))
            })?;
            
            Ok(new_vm)
        }
    }
    
    /// Return an RC-based VM to the pool
    fn return_rc_vm(&self, vm: RcVM) {
        if let Ok(mut pool) = self.rc_vm_pool.lock() {
            // Reset VM state before returning to pool
            // TODO: Implement VM reset
            pool.push(vm);
        }
    }
    
    /// Convert Rc-based Value to RESP
    fn lua_to_resp_rc(&self, value: Value) -> Result<RespFrame, FerrousError> {
        match value {
            Value::Nil => Ok(RespFrame::Null),
            Value::Boolean(b) => Ok(RespFrame::Integer(if b { 1 } else { 0 })),
            Value::Number(n) => {
                if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
                    Ok(RespFrame::Integer(n as i64))
                } else {
                    Ok(RespFrame::bulk_string(n.to_string()))
                }
            },
            Value::String(handle) => {
                // For now, return a placeholder for strings
                Ok(RespFrame::bulk_string("<string>".to_string()))
            },
            Value::Table(_) => {
                Ok(RespFrame::bulk_string("<table>".to_string()))
            },
            _ => Err(FerrousError::LuaError("Cannot convert Lua value to RESP".to_string())),
        }
    }
}