//! Lua Virtual Machine for Ferrous
//! 
//! This module provides a Redis-compatible Lua 5.1 VM implementation
//! designed to work harmoniously with Rust's ownership model.

use std::sync::Arc;  // Ensure Arc is imported

mod arena;
mod handle;
mod value;
mod heap;
mod transaction;
mod vm;
mod error;
mod metamethod;

#[cfg(test)]
mod test_basic;

pub use self::error::{LuaError, LuaResult};

// Implement handle_lua_command directly in the lua module
pub fn handle_lua_command(
    storage: &Arc<crate::storage::StorageEngine>,
    cmd: &str, 
    parts: &[crate::protocol::resp::RespFrame]
) -> crate::error::Result<crate::protocol::resp::RespFrame> {
    use crate::protocol::resp::RespFrame;
    use crate::error::{Result, FerrousError};
    use std::str;
    
    // A simplified implementation that will be expanded later
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
            
            // Create a new LuaGIL - in a real implementation, we'd use a singleton
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
            // Simplified implementation
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
            
            // Simplified implementation
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

/// Global Interpreter Lock for Lua VMs
/// Manages a pool of Lua VMs and script caching
pub struct LuaGIL {
    /// Pool of available VMs
    vm_pool: Arc<Mutex<Vec<vm::LuaVM>>>,
    
    /// Script cache mapping SHA1 hash to compiled scripts
    script_cache: Arc<RwLock<HashMap<String, String>>>,
    
    /// Kill flag for currently executing script
    kill_flag: Arc<AtomicBool>,
}

impl LuaGIL {
    /// Create a new Lua Global Interpreter Lock
    pub fn new() -> Result<Self, FerrousError> {
        // Initialize VM pool with one VM for now
        let mut pool = Vec::new();
        
        // Create initial VM
        match vm::LuaVM::new() {
            Ok(vm) => pool.push(vm),
            Err(e) => return Err(FerrousError::LuaError(format!("Failed to create Lua VM: {}", e))),
        }
        
        Ok(LuaGIL {
            vm_pool: Arc::new(Mutex::new(pool)),
            script_cache: Arc::new(RwLock::new(HashMap::new())),
            kill_flag: Arc::new(AtomicBool::new(false)),
        })
    }
    
    /// Evaluate a Lua script
    pub fn eval(&self, script: &str, context: ScriptContext) -> Result<RespFrame, FerrousError> {
        // Get VM from pool
        let mut vm = self.get_vm()?;
        
        // Set up context
        vm.set_context(context)?;
        
        // Compile and execute script
        let result = vm.eval_script(script)?;
        
        // Convert result to RESP
        let resp = self.lua_to_resp(result)?;
        
        // Return VM to pool
        self.return_vm(vm);
        
        Ok(resp)
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
    
    /// Get a VM from the pool
    fn get_vm(&self) -> Result<vm::LuaVM, FerrousError> {
        let mut pool = self.vm_pool.lock().map_err(|_| {
            FerrousError::Internal("Failed to acquire VM pool lock".to_string())
        })?;
        
        if let Some(vm) = pool.pop() {
            Ok(vm)
        } else {
            // Create a new VM if pool is empty
            vm::LuaVM::new().map_err(|e| {
                FerrousError::LuaError(format!("Failed to create Lua VM: {}", e))
            })
        }
    }
    
    /// Return a VM to the pool
    fn return_vm(&self, vm: vm::LuaVM) {
        if let Ok(mut pool) = self.vm_pool.lock() {
            // Reset VM state before returning to pool
            // TODO: Implement VM reset
            pool.push(vm);
        }
    }
    
    /// Convert Lua value to RESP frame
    fn lua_to_resp(&self, value: value::Value) -> Result<RespFrame, FerrousError> {
        match value {
            value::Value::Nil => Ok(RespFrame::Null),
            value::Value::Boolean(b) => Ok(RespFrame::Integer(if b { 1 } else { 0 })),
            value::Value::Number(n) => {
                // Check if it's an integer
                if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
                    Ok(RespFrame::Integer(n as i64))
                } else {
                    // Return as bulk string
                    Ok(RespFrame::bulk_string(n.to_string()))
                }
            },
            value::Value::String(handle) => {
                // Get a VM to access the string value
                let mut vm = self.get_vm()?;
                
                // Use transaction to get the string value
                let string_value = {
                    let mut tx = transaction::HeapTransaction::new(vm.heap_mut());
                    let value = tx.get_string_value(handle.clone())?;
                    tx.commit()?;
                    value
                };
                
                // Return the VM to the pool
                self.return_vm(vm);
                
                Ok(RespFrame::bulk_string(string_value))
            },
            value::Value::Table(_) => {
                // For now, return a placeholder for tables
                Ok(RespFrame::bulk_string("<table>".to_string()))
            },
            _ => Err(FerrousError::LuaError("Cannot convert Lua value to RESP".to_string())),
        }
    }
}