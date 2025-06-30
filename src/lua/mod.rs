//! Lua scripting integration for Ferrous
//!
//! This module implements a Redis-compatible Lua scripting engine using
//! a state-machine architecture with non-recursive execution paths.

pub mod arena;
pub mod value;
pub mod heap;
pub mod compiler;
pub mod vm;
pub mod parser;
pub mod stdlib;
pub mod redis_api;
pub mod error;
pub mod transaction;

use std::sync::{Arc, Mutex, RwLock};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant, SystemTime};
use std::sync::atomic::{AtomicBool, Ordering};

pub use error::{LuaError, Result};
pub use value::{Value, HandleType};
pub use vm::LuaVM;

pub use crate::protocol::resp::RespFrame;
pub use crate::storage::StorageEngine;
pub use crate::error::FerrousError;

/// Represents a compiled Lua script
#[derive(Clone, Debug)]
pub struct CompiledScript {
    /// Source code
    pub source: String,
    
    /// SHA1 hash for caching
    pub sha1: String,
    
    /// Compiled bytecode
    bytecode: Vec<u8>,
}

/// Context for script execution
pub struct ScriptContext {
    /// Storage engine
    pub storage: Arc<StorageEngine>,
    
    /// Database ID
    pub db: usize,
    
    /// KEYS array
    pub keys: Vec<Vec<u8>>,
    
    /// ARGV array
    pub args: Vec<Vec<u8>>,
    
    /// Timeout for script execution
    pub timeout: Duration,
}

/// Global Lua Interpreter Lock for thread safety
pub struct LuaGIL {
    /// VM instance
    vm: Arc<Mutex<LuaVM>>,
    
    /// Script cache
    script_cache: Arc<RwLock<HashMap<String, CompiledScript>>>,
    
    /// Execution state
    execution_state: Arc<RwLock<ExecutionState>>,
    
    /// Kill flag
    kill_flag: Arc<AtomicBool>,
}

/// Execution state
#[derive(Debug, Clone, PartialEq)]
enum ExecutionState {
    /// Not executing
    Idle,
    
    /// Currently executing a script
    Running(String), // script SHA
    
    /// Killed by SCRIPT KILL command
    Killed,
}

impl LuaGIL {
    /// Create a new Lua GIL
    pub fn new() -> Result<Self> {
        let vm = LuaVM::new()?;
        
        Ok(LuaGIL {
            vm: Arc::new(Mutex::new(vm)),
            script_cache: Arc::new(RwLock::new(HashMap::new())),
            execution_state: Arc::new(RwLock::new(ExecutionState::Idle)),
            kill_flag: Arc::new(AtomicBool::new(false)),
        })
    }
    
    /// Execute a script
    pub fn eval(&self, source: &str, context: ScriptContext) -> std::result::Result<RespFrame, FerrousError> {
        // Compute SHA1 for caching
        let sha1 = self.compute_sha1(source);
        
        // Start timing
        let start_time = Instant::now();
        
        // Set execution state
        {
            let mut state = self.execution_state.write().map_err(|_| {
                FerrousError::LuaError("Failed to lock execution state".to_string())
            })?;
            *state = ExecutionState::Running(sha1.clone());
        }
        
        // Reset kill flag
        self.kill_flag.store(false, Ordering::SeqCst);
        
        let result = self.execute_script(source, &sha1, context, start_time);
        
        // Reset execution state
        {
            let mut state = self.execution_state.write().map_err(|_| {
                FerrousError::LuaError("Failed to lock execution state".to_string())
            })?;
            *state = ExecutionState::Idle;
        }
        
        result
    }
    
    /// Execute a script using its SHA1 hash
    pub fn evalsha(&self, sha1: &str, context: ScriptContext) -> std::result::Result<RespFrame, FerrousError> {
        // Get script from cache
        let script = {
            let cache = self.script_cache.read().map_err(|_| {
                FerrousError::LuaError("Failed to read script cache".to_string())
            })?;
            cache.get(sha1).cloned()
        };
        
        match script {
            Some(script) => {
                // Start timing
                let start_time = Instant::now();
                
                // Set execution state
                {
                    let mut state = self.execution_state.write().map_err(|_| {
                        FerrousError::LuaError("Failed to lock execution state".to_string())
                    })?;
                    *state = ExecutionState::Running(sha1.to_string());
                }
                
                // Reset kill flag
                self.kill_flag.store(false, Ordering::SeqCst);
                
                let result = self.execute_cached_script(script, context, start_time);
                
                // Reset execution state
                {
                    let mut state = self.execution_state.write().map_err(|_| {
                        FerrousError::LuaError("Failed to lock execution state".to_string())
                    })?;
                    *state = ExecutionState::Idle;
                }
                
                result
            }
            None => Err(FerrousError::ScriptNotFound(sha1.to_string())),
        }
    }
    
    /// Execute a script
    fn execute_script(
        &self,
        source: &str,
        sha1: &str,
        context: ScriptContext,
        start_time: Instant,
    ) -> std::result::Result<RespFrame, FerrousError> {
        // Get lock on VM
        let mut vm = self.vm.lock().map_err(|_| {
            FerrousError::LuaError("Failed to lock Lua VM".to_string())
        })?;
        
        // Set kill flag for this VM
        vm.set_kill_flag(self.kill_flag.clone());
        
        // Compile or get from cache
        let result = {
            let cache = self.script_cache.read().map_err(|_| {
                FerrousError::LuaError("Failed to read script cache".to_string())
            })?;
            
            if let Some(_script) = cache.get(sha1) {
                // Already compiled, just load from cache
                // This is a simplification, we would actually deserialize the
                // bytecode into a Module and then load it into the VM
                let module = compiler::compile(source)?;
                
                // Load KEYS and ARGV tables
                self.setup_context(&mut vm, &context)?;
                
                // Execute module
                vm.execute_module(&module, &[])
            } else {
                // Not in cache, compile it
                let module = compiler::compile(source)?;
                
                // Cache the compiled script
                let script = CompiledScript {
                    source: source.to_string(),
                    sha1: sha1.to_string(),
                    bytecode: Vec::new(), // Placeholder, would be serialized bytecode
                };
                
                // Store in cache
                let mut cache = self.script_cache.write().map_err(|_| {
                    FerrousError::LuaError("Failed to write script cache".to_string())
                })?;
                cache.insert(sha1.to_string(), script);
                
                // Load KEYS and ARGV tables
                self.setup_context(&mut vm, &context)?;
                
                // Execute module
                vm.execute_module(&module, &[])
            }
        };
        
        // Handle result
        self.handle_execution_result(result, start_time, context.timeout)
    }
    
    /// Execute a cached script
    fn execute_cached_script(
        &self,
        _script: CompiledScript,
        context: ScriptContext,
        start_time: Instant,
    ) -> std::result::Result<RespFrame, FerrousError> {
        // Get lock on VM
        let mut vm = self.vm.lock().map_err(|_| {
            FerrousError::LuaError("Failed to lock Lua VM".to_string())
        })?;
        
        // Set kill flag for this VM
        vm.set_kill_flag(self.kill_flag.clone());
        
        // This is a simplification - would deserialize the bytecode
        // and load it into the VM in a real implementation
        let module = compiler::compile(&_script.source)?;
        
        // Load KEYS and ARGV tables
        self.setup_context(&mut vm, &context)?;
        
        // Execute the compiled bytecode
        let result = vm.execute_module(&module, &[]);
        
        // Handle result
        self.handle_execution_result(result, start_time, context.timeout)
    }
    
    /// Handle execution result
    fn handle_execution_result(
        &self,
        result: Result<Value>,
        start_time: Instant,
        timeout: Duration,
    ) -> std::result::Result<RespFrame, FerrousError> {
        // Check for timeout
        let elapsed = start_time.elapsed();
        if elapsed > timeout {
            return Err(FerrousError::ScriptTimeout);
        }
        
        // Check for kill
        if self.kill_flag.load(Ordering::SeqCst) {
            return Err(FerrousError::ScriptKilled);
        }
        
        // Process result
        match result {
            Ok(value) => {
                // Convert Lua value to RESP
                self.lua_value_to_resp(value)
            }
            Err(e) => {
                Err(FerrousError::LuaError(e.to_string()))
            }
        }
    }
    
    /// Set up execution context
    fn setup_context(&self, vm: &mut LuaVM, context: &ScriptContext) -> Result<()> {
        // Register Redis API
        redis_api::register_redis_api(vm, redis_api::RedisContext {
            storage: context.storage.clone(),
            db: context.db,
            keys: context.keys.clone(),
            args: context.args.clone(),
        })?;
        
        Ok(())
    }
    
    /// Convert Lua value to RESP frame
    fn lua_value_to_resp(&self, value: Value) -> std::result::Result<RespFrame, FerrousError> {
        match value {
            Value::Nil => Ok(RespFrame::Null),
            Value::Boolean(b) => Ok(RespFrame::Boolean(b)),
            Value::Number(n) => {
                if n.fract() == 0.0 {
                    Ok(RespFrame::Integer(n as i64))
                } else {
                    Ok(RespFrame::Double(n))
                }
            }
            Value::String(handle) => {
                let vm = self.vm.lock().map_err(|_| {
                    FerrousError::LuaError("Failed to lock Lua VM".to_string())
                })?;
                
                let bytes = vm.heap.get_string_bytes(handle)?;
                Ok(RespFrame::BulkString(Some(bytes.to_vec())))
            }
            Value::Table(handle) => {
                let vm = self.vm.lock().map_err(|_| {
                    FerrousError::LuaError("Failed to lock Lua VM".to_string())
                })?;
                
                let table = vm.heap.get_table(handle)?;
                
                // Check if it's a special reply type (status or error)
                let type_key = vm.create_string("__redis_type")?;
                if let Ok(Value::String(type_handle)) = vm.get_table(handle, &Value::String(type_key)) {
                    let type_str = vm.heap.get_string_value(type_handle)?;
                    
                    if type_str == "status" {
                        let msg_key = vm.create_string("__status_msg")?;
                        if let Ok(Value::String(msg_handle)) = vm.get_table(handle, &Value::String(msg_key)) {
                            let msg = vm.heap.get_string_value(msg_handle)?;
                            let bytes = Arc::new(msg.into_bytes());
                            return Ok(RespFrame::SimpleString(bytes));
                        }
                    } else if type_str == "error" {
                        let msg_key = vm.create_string("__error_msg")?;
                        if let Ok(Value::String(msg_handle)) = vm.get_table(handle, &Value::String(msg_key)) {
                            let msg = vm.heap.get_string_value(msg_handle)?;
                            let msg_bytes = Arc::new(msg.into_bytes());
                            return Ok(RespFrame::Error(msg_bytes));
                        }
                    }
                }
                
                // Convert array part to RESP array
                let mut items = Vec::new();
                for i in 0..table.array.len() {
                    let item = table.array[i].clone();
                    let resp = self.lua_value_to_resp(item)?;
                    items.push(resp);
                }
                
                Ok(RespFrame::Array(Some(items)))
            }
            _ => Err(FerrousError::LuaError(format!(
                "Cannot convert {} to RESP", value.type_name()
            ))),
        }
    }
    
    /// Check if a script exists in the cache
    pub fn script_exists(&self, sha1: &str) -> bool {
        let cache = self.script_cache.read().unwrap();
        cache.contains_key(sha1)
    }
    
    /// Flush the script cache
    pub fn script_flush(&self) {
        let mut cache = self.script_cache.write().unwrap();
        cache.clear();
    }
    
    /// Load a script into the cache
    pub fn script_load(&self, source: &str) -> std::result::Result<String, FerrousError> {
        // Compute SHA1
        let sha1 = self.compute_sha1(source);
        
        // Compile script
        match compiler::compile(source) {
            Ok(_module) => {
                // Cache the script
                let script = CompiledScript {
                    source: source.to_string(),
                    sha1: sha1.clone(),
                    bytecode: Vec::new(), // Placeholder, would be serialized bytecode
                };
                
                let mut cache = self.script_cache.write().map_err(|_| {
                    FerrousError::LuaError("Failed to write script cache".to_string())
                })?;
                
                cache.insert(sha1.clone(), script);
                
                Ok(sha1)
            }
            Err(e) => Err(FerrousError::LuaError(format!("Compilation error: {:?}", e))),
        }
    }
    
    /// Kill a running script
    pub fn kill_script(&self) -> std::result::Result<(), FerrousError> {
        let mut state = self.execution_state.write().map_err(|_| {
            FerrousError::LuaError("Failed to lock execution state".to_string())
        })?;
        
        match *state {
            ExecutionState::Running(_) => {
                // Kill the script
                *state = ExecutionState::Killed;
                
                // Set the kill flag to stop execution
                self.kill_flag.store(true, Ordering::SeqCst);
                
                Ok(())
            }
            _ => Err(FerrousError::NoScriptRunning),
        }
    }
    
    /// Compute SHA1 hash of a script
    fn compute_sha1(&self, source: &str) -> String {
        use sha1::{Sha1, Digest};
        
        let mut hasher = Sha1::new();
        hasher.update(source.as_bytes());
        let result = hasher.finalize();
        
        hex::encode(result)
    }
}

impl ScriptContext {
    /// Create a new script context
    pub fn new(storage: Arc<StorageEngine>, db: usize, keys: Vec<Vec<u8>>, args: Vec<Vec<u8>>) -> Self {
        ScriptContext {
            storage,
            db,
            keys,
            args,
            timeout: Duration::from_secs(5),
        }
    }
    
    /// Set a custom timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

/// Handle a Lua script command from Redis
pub fn handle_lua_command(storage: &Arc<StorageEngine>, cmd: &str, parts: &[RespFrame]) -> crate::error::Result<RespFrame> {
    // Get the Lua interpreter
    let gil = LuaGIL::new().map_err(|e| FerrousError::LuaError(e.to_string()))?;
    
    // Delegate command handling based on the command
    match cmd.to_lowercase().as_str() {
        "eval" => handle_eval(storage, &gil, parts),
        "evalsha" => handle_evalsha(storage, &gil, parts),
        "script" => handle_script_command(storage, &gil, parts),
        _ => Ok(RespFrame::error(format!("ERR unknown command '{}'", cmd))),
    }
}

/// Handle EVAL command
fn handle_eval(storage: &Arc<StorageEngine>, gil: &LuaGIL, parts: &[RespFrame]) -> crate::error::Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'eval' command"));
    }
    
    // Get script string
    let script = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            match std::str::from_utf8(bytes) {
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
            match std::str::from_utf8(bytes) {
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
    
    // Check if we have enough arguments
    if parts.len() < 3 + num_keys {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'eval' command"));
    }
    
    // Extract keys
    let mut keys = Vec::with_capacity(num_keys);
    for i in 0..num_keys {
        match &parts[3 + i] {
            RespFrame::BulkString(Some(bytes)) => {
                keys.push(bytes.to_vec());
            }
            _ => {
                return Ok(RespFrame::error("ERR keys must be strings"));
            }
        }
    }
    
    // Extract args
    let mut args = Vec::with_capacity(parts.len() - 3 - num_keys);
    for i in 3 + num_keys..parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => {
                args.push(bytes.to_vec());
            }
            _ => {
                return Ok(RespFrame::error("ERR args must be strings"));
            }
        }
    }
    
    // Create script context
    let context = ScriptContext {
        storage: storage.clone(),
        db: storage.get_current_db(),
        keys,
        args,
        timeout: Duration::from_secs(5), // Default timeout (should come from config)
    };
    
    // Evaluate script
    match gil.eval(&script, context) {
        Ok(resp) => Ok(resp),
        Err(e) => Ok(RespFrame::error(format!("ERR {}", e))),
    }
}

/// Handle EVALSHA command
fn handle_evalsha(storage: &Arc<StorageEngine>, gil: &LuaGIL, parts: &[RespFrame]) -> crate::error::Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'evalsha' command"));
    }
    
    // Get SHA1
    let sha1 = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            match std::str::from_utf8(bytes) {
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
            match std::str::from_utf8(bytes) {
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
    
    // Check if we have enough arguments
    if parts.len() < 3 + num_keys {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'evalsha' command"));
    }
    
    // Extract keys
    let mut keys = Vec::with_capacity(num_keys);
    for i in 0..num_keys {
        match &parts[3 + i] {
            RespFrame::BulkString(Some(bytes)) => {
                keys.push(bytes.to_vec());
            }
            _ => {
                return Ok(RespFrame::error("ERR keys must be strings"));
            }
        }
    }
    
    // Extract args
    let mut args = Vec::with_capacity(parts.len() - 3 - num_keys);
    for i in 3 + num_keys..parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => {
                args.push(bytes.to_vec());
            }
            _ => {
                return Ok(RespFrame::error("ERR args must be strings"));
            }
        }
    }
    
    // Create script context
    let context = ScriptContext {
        storage: storage.clone(),
        db: storage.get_current_db(),
        keys,
        args,
        timeout: Duration::from_secs(5), // Default timeout
    };
    
    // Evaluate script by SHA
    match gil.evalsha(&sha1, context) {
        Ok(resp) => Ok(resp),
        Err(FerrousError::ScriptNotFound(_)) => {
            Ok(RespFrame::error("NOSCRIPT No matching script. Please use EVAL."))
        }
        Err(e) => Ok(RespFrame::error(format!("ERR {}", e))),
    }
}

/// Handle SCRIPT LOAD/EXISTS/FLUSH/KILL commands
fn handle_script_command(_storage: &Arc<StorageEngine>, gil: &LuaGIL, parts: &[RespFrame]) -> crate::error::Result<RespFrame> {
    if parts.len() < 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'script' command"));
    }
    
    // Get subcommand
    let subcommand = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            match std::str::from_utf8(bytes) {
                Ok(s) => s.to_lowercase(),
                Err(_) => return Ok(RespFrame::error("ERR invalid subcommand")),
            }
        }
        _ => {
            return Ok(RespFrame::error("ERR invalid subcommand"));
        }
    };
    
    match subcommand.as_str() {
        "load" => handle_script_load(gil, &parts[1..]),
        "exists" => handle_script_exists(gil, &parts[1..]),
        "flush" => handle_script_flush(gil, &parts[1..]),
        "kill" => handle_script_kill(gil, &parts[1..]),
        _ => Ok(RespFrame::error(format!("ERR unknown subcommand '{}'", subcommand))),
    }
}

/// Handle SCRIPT LOAD command
fn handle_script_load(gil: &LuaGIL, parts: &[RespFrame]) -> crate::error::Result<RespFrame> {
    if parts.len() != 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'script load' command"));
    }
    
    // Get script string
    let script = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            match std::str::from_utf8(bytes) {
                Ok(s) => s.to_string(),
                Err(_) => return Ok(RespFrame::error("ERR invalid script - not valid UTF-8")),
            }
        }
        _ => {
            return Ok(RespFrame::error("ERR invalid script"));
        }
    };
    
    // Load script
    match gil.script_load(&script) {
        Ok(sha1) => Ok(RespFrame::bulk_string(sha1)),
        Err(e) => Ok(RespFrame::error(format!("ERR {}", e))),
    }
}

/// Handle SCRIPT EXISTS command
fn handle_script_exists(gil: &LuaGIL, parts: &[RespFrame]) -> crate::error::Result<RespFrame> {
    if parts.len() < 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'script exists' command"));
    }
    
    // Check all hashes
    let mut results = Vec::new();
    
    for i in 1..parts.len() {
        let sha1 = match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => {
                match std::str::from_utf8(bytes) {
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
        results.push(RespFrame::Integer(if gil.script_exists(&sha1) { 1 } else { 0 }));
    }
    
    Ok(RespFrame::Array(Some(results)))
}

/// Handle SCRIPT FLUSH command
fn handle_script_flush(gil: &LuaGIL, parts: &[RespFrame]) -> crate::error::Result<RespFrame> {
    if parts.len() != 1 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'script flush' command"));
    }
    
    // Flush scripts
    gil.script_flush();
    
    let bytes = Arc::new("OK".as_bytes().to_vec());
    Ok(RespFrame::SimpleString(bytes))
}

/// Handle SCRIPT KILL command
fn handle_script_kill(gil: &LuaGIL, parts: &[RespFrame]) -> crate::error::Result<RespFrame> {
    if parts.len() != 1 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'script kill' command"));
    }
    
    // Kill script
    match gil.kill_script() {
        Ok(_) => {
            let bytes = Arc::new("OK".as_bytes().to_vec());
            Ok(RespFrame::SimpleString(bytes))
        },
        Err(e) => {
            let msg_bytes = Arc::new(format!("ERR {}", e).into_bytes());
            Ok(RespFrame::Error(msg_bytes))
        }
    }
}