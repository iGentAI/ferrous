//! Lua script executor for Ferrous
//!
//! This module integrates the Lua interpreter with Redis commands.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use std::rc::Rc;
use std::cell::RefCell;

use super::compiler::Compiler;
use super::error::{LuaError, Result};
use super::parser::Parser;
use super::value::{LuaValue, LuaString, LuaTable, LuaFunction, Instruction, FunctionProto};
use super::vm::{LuaVm, RedisApi, LuaRustFunction};

use crate::error::{FerrousError, ScriptError};
use crate::protocol::RespFrame;
use crate::storage::{StorageEngine, DatabaseIndex};

/// Script executor for managing Lua scripts
pub struct ScriptExecutor {
    /// Script cache (SHA1 -> compiled script)
    cache: Arc<Mutex<HashMap<String, CompiledScript>>>,
    
    /// Storage engine reference
    storage: Arc<StorageEngine>,
    
    /// VM pool for reuse
    vm_pool: Arc<Mutex<Vec<LuaVm>>>,
    
    /// Currently running script, if any
    running: Arc<Mutex<Option<ScriptExecution>>>,
}

/// Information about a running script
struct ScriptExecution {
    /// Script SHA1
    sha1: String,
    
    /// Time the script started
    start_time: Instant,
    
    /// Flag to indicate script should be killed
    should_kill: Arc<AtomicBool>,
}

/// A compiled script
#[derive(Clone)]
pub struct CompiledScript {
    /// The original script
    source: String,
    
    /// The SHA1 hash of the script
    sha1: String,
    
    /// The compiled function prototype - using Rc to match VM expectations
    proto: Rc<FunctionProto>,
}

/// Redis API implementation for Ferrous
struct FerrousRedisApi {
    /// Storage engine for executing commands
    storage: Arc<StorageEngine>,
    
    /// Current database
    db: DatabaseIndex,
    
    /// KEYS array
    keys: Vec<Vec<u8>>,
    
    /// ARGV array
    argv: Vec<Vec<u8>>,
}

impl ScriptExecutor {
    /// Create a new script executor
    pub fn new(storage: Arc<StorageEngine>) -> Self {
        ScriptExecutor {
            cache: Arc::new(Mutex::new(HashMap::new())),
            storage,
            vm_pool: Arc::new(Mutex::new(Vec::new())),
            running: Arc::new(Mutex::new(None)),
        }
    }
    
    /// Load a script and return its SHA1 hash
    pub fn load_script(&self, script: &str) -> std::result::Result<String, FerrousError> {
        let sha = compute_sha1(script);
        
        // Check if the script is already in the cache
        {
            let cache = self.cache.lock().unwrap();
            if cache.contains_key(&sha) {
                return Ok(sha);
            }
        }
        
        // Compile the script
        let compiled = match self.compile_script(script, sha.clone()) {
            Ok(compiled) => compiled,
            Err(e) => return Err(FerrousError::Script(ScriptError::CompilationError(e.to_string()))),
        };
        
        // Cache the compiled script
        {
            let mut cache = self.cache.lock().unwrap();
            cache.insert(sha.clone(), compiled);
        }
        
        Ok(sha)
    }
    
    /// Check if a script exists in the cache
    pub fn script_exists(&self, sha: &str) -> bool {
        let cache = self.cache.lock().unwrap();
        cache.contains_key(sha)
    }
    
    /// Flush all scripts from the cache
    pub fn flush_scripts(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
    }
    
    /// Get a script from the cache
    pub fn get_cached(&self, sha: &str) -> Option<CompiledScript> {
        let cache = self.cache.lock().unwrap();
        cache.get(sha).cloned()
    }
    
    /// Execute a script
    pub fn eval(&self, script: &str, keys: Vec<Vec<u8>>, argv: Vec<Vec<u8>>, db: DatabaseIndex) -> std::result::Result<RespFrame, FerrousError> {
        // Compile the script if not already in cache
        let compiled = {
            let sha1 = compute_sha1(script);
            let mut cache = self.cache.lock().unwrap();
            
            if let Some(compiled) = cache.get(&sha1) {
                compiled.clone()
            } else {
                let compiled = self.compile_script(script, sha1.clone())?;
                cache.insert(sha1, compiled.clone());
                compiled
            }
        };
        
        // Mark script as running
        let should_kill = Arc::new(AtomicBool::new(false));
        {
            let mut running = self.running.lock().unwrap();
            *running = Some(ScriptExecution {
                sha1: compiled.sha1.clone(),
                start_time: Instant::now(),
                should_kill: should_kill.clone(),
            });
        }
        
        // Execute the script
        let result = self.execute_compiled_with_kill_flag(compiled, keys, argv, db, should_kill);
        
        // Mark script as no longer running
        {
            let mut running = self.running.lock().unwrap();
            *running = None;
        }
        
        result
    }
    
    /// Execute a script that's already in the cache
    pub fn evalsha(&self, sha: &str, keys: Vec<Vec<u8>>, argv: Vec<Vec<u8>>, db: DatabaseIndex) -> std::result::Result<RespFrame, FerrousError> {
        // Get the script from the cache
        let script = match self.get_cached(sha) {
            Some(s) => s,
            None => return Err(FerrousError::Script(ScriptError::NotFound)),
        };
        
        // Execute the script
        self.execute_compiled(script, keys, argv, db)
    }
    
    /// Kill the currently running script (if any)
    pub fn kill_running_script(&self) -> bool {
        let mut running = self.running.lock().unwrap();
        
        if let Some(execution) = running.as_ref() {
            execution.should_kill.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }
    
    /// Compile a script
    fn compile_script(&self, script: &str, sha1: String) -> std::result::Result<CompiledScript, FerrousError> {
        // We'll store the original script source for direct execution
        // The compiled bytecode isn't used directly yet (we're using interpreter mode)
        // but we still go through the compilation process to validate syntax
        
        // Parse the script
        let mut parser = match Parser::new(script) {
            Ok(parser) => parser,
            Err(e) => return Err(ScriptError::CompilationError(e.to_string()).into()),
        };
        
        let chunk = match parser.parse() {
            Ok(chunk) => chunk,
            Err(e) => return Err(ScriptError::CompilationError(e.to_string()).into()),
        };
        
        // Compile the chunk (mostly for validation)
        let mut compiler = Compiler::new();
        let proto = match compiler.compile_chunk(&chunk) {
            Ok(proto) => proto,
            Err(e) => return Err(ScriptError::CompilationError(e.to_string()).into()),
        };
        
        Ok(CompiledScript {
            source: script.to_string(),
            sha1,
            proto: Rc::new(proto),
        })
    }
    
    /// Execute a compiled script
    fn execute_compiled(&self, script: CompiledScript, keys: Vec<Vec<u8>>, argv: Vec<Vec<u8>>, db: DatabaseIndex) -> std::result::Result<RespFrame, FerrousError> {
        let should_kill = Arc::new(AtomicBool::new(false));
        self.execute_compiled_with_kill_flag(script, keys, argv, db, should_kill)
    }
    
    /// Execute a compiled script with a kill flag
    fn execute_compiled_with_kill_flag(&self, script: CompiledScript, keys: Vec<Vec<u8>>, argv: Vec<Vec<u8>>, db: DatabaseIndex, 
                                        should_kill: Arc<AtomicBool>) -> std::result::Result<RespFrame, FerrousError> {
        // Get or create a VM
        let mut vm = self.get_vm();
        
        // Set up the environment
        self.setup_vm_environment(&mut vm, keys.clone(), argv.clone(), db)?;
        
        // Log that we're going to execute the script
        println!("[LUA EXEC] Executing script: {}", script.source);
        
        // Set a custom check_limits function that also checks the kill flag
        let should_kill_clone = should_kill.clone();
        let check_limits_with_kill = move |vm: &mut LuaVm| -> Result<()> {
            // First do regular limit checks
            vm.check_limits()?;
            
            // Then check if script should be killed
            if should_kill_clone.load(Ordering::SeqCst) {
                return Err(LuaError::Runtime("Script execution aborted".to_string()));
            }
            
            Ok(())
        };
        
        // We'll use the simplified path for now - the run method will try the full VM first with fallback
        // to pattern matching for reliability
        let result = match vm.run_with_kill_check(&script.source, &check_limits_with_kill) {
            Ok(lua_result) => {
                // Convert result to Redis response
                println!("[LUA EXEC] Script executed successfully, result: {:?}", lua_result);
                self.lua_to_resp(lua_result)
            },
            Err(e) => {
                if should_kill.load(Ordering::SeqCst) {
                    // The script was killed
                    println!("[LUA EXEC] Script execution was aborted");
                    Ok(RespFrame::error("ERR Script execution aborted"))
                } else {
                    // Log the error and return as a Redis error
                    println!("[LUA EXEC] Script execution error: {}", e);
                    Ok(RespFrame::error(format!("ERR Lua execution error: {}", e)))
                }
            }
        };
        
        // Return VM to pool
        self.return_vm(vm);
        
        // Return the result or error
        result
    }
    
    /// Set up the VM environment for script execution
    fn setup_vm_environment(&self, vm: &mut LuaVm, keys: Vec<Vec<u8>>, argv: Vec<Vec<u8>>, db: DatabaseIndex) -> std::result::Result<(), FerrousError> {
        // Create the KEYS table
        let mut keys_table = LuaTable::new();
        for (i, k) in keys.iter().enumerate() {
            keys_table.set(
                LuaValue::Number((i + 1) as f64),
                LuaValue::String(LuaString::from_bytes(k.clone()))
            );
        }
        vm.set_global("KEYS", LuaValue::Table(Rc::new(RefCell::new(keys_table))));
        
        // Create the ARGV table
        let mut argv_table = LuaTable::new();
        for (i, a) in argv.iter().enumerate() {
            argv_table.set(
                LuaValue::Number((i + 1) as f64),
                LuaValue::String(LuaString::from_bytes(a.clone()))
            );
        }
        vm.set_global("ARGV", LuaValue::Table(Rc::new(RefCell::new(argv_table))));
        
        // Make sure Redis API is initialized in the VM
        // This will set up redis.call and other Redis functions
        match vm.ensure_redis_environment() {
            Ok(_) => {},
            Err(e) => return Err(FerrousError::Script(ScriptError::ExecutionError(e.to_string())))
        }
        
        // Create the Redis API
        let redis_api = Box::new(FerrousRedisApi {
            storage: self.storage.clone(),
            db,
            keys,
            argv,
        });
        
        vm.set_redis_api(redis_api);
        
        Ok(())
    }
    
    /// Register standard libraries
    fn register_standard_libs(&self, vm: &mut LuaVm) -> std::result::Result<(), FerrousError> {
        // Create the 'redis' table with redis.call and redis.pcall functions
        let table = LuaTable::new();
        let redis_table = Rc::new(RefCell::new(table));
        
        // Create Lua table and add functions directly
        {
            let mut table = redis_table.borrow_mut();
            
            // Add redis.call function
            table.set(
                LuaValue::String(LuaString::from_str("call")),
                LuaValue::Function(LuaFunction::Rust(redis_call_impl))
            );
            
            // Add redis.pcall function
            table.set(
                LuaValue::String(LuaString::from_str("pcall")),
                LuaValue::Function(LuaFunction::Rust(redis_pcall_impl))
            );
            
            // Add redis.log function
            table.set(
                LuaValue::String(LuaString::from_str("log")),
                LuaValue::Function(LuaFunction::Rust(redis_log_impl))
            );
            
            // Add constants
            table.set(
                LuaValue::String(LuaString::from_str("LOG_DEBUG")),
                LuaValue::Number(0.0)
            );
            
            table.set(
                LuaValue::String(LuaString::from_str("LOG_VERBOSE")),
                LuaValue::Number(1.0)
            );
            
            table.set(
                LuaValue::String(LuaString::from_str("LOG_NOTICE")),
                LuaValue::Number(2.0)
            );
            
            table.set(
                LuaValue::String(LuaString::from_str("LOG_WARNING")),
                LuaValue::Number(3.0)
            );
        }
        
        // Set the 'redis' table in the global environment
        vm.set_global("redis", LuaValue::Table(redis_table));
        
        Ok(())
    }
    
    /// Get a VM from the pool or create a new one
    fn get_vm(&self) -> LuaVm {
        if let Ok(mut pool) = self.vm_pool.lock() {
            if let Some(vm) = pool.pop() {
                return vm;
            }
        }
        
        LuaVm::new()
    }
    
    /// Return a VM to the pool
    fn return_vm(&self, vm: LuaVm) {
        if let Ok(mut pool) = self.vm_pool.lock() {
            if pool.len() < 8 { // Limit pool size
                pool.push(vm);
            }
        }
    }
    
    /// Convert a Lua value to a Redis response
    fn lua_to_resp(&self, value: LuaValue) -> std::result::Result<RespFrame, FerrousError> {
        match value {
            LuaValue::Nil => Ok(RespFrame::Null),
            LuaValue::Boolean(false) => Ok(RespFrame::Null),
            LuaValue::Boolean(true) => Ok(RespFrame::Integer(1)),
            LuaValue::Number(n) => {
                if n.fract() == 0.0 && n >= std::i64::MIN as f64 && n <= std::i64::MAX as f64 {
                    Ok(RespFrame::Integer(n as i64))
                } else {
                    let s = n.to_string();
                    Ok(RespFrame::BulkString(Some(Arc::new(s.into_bytes()))))
                }
            },
            LuaValue::String(s) => {
                Ok(RespFrame::BulkString(Some(Arc::new(s.as_bytes().to_vec()))))
            },
            LuaValue::Table(t) => {
                // Check if table is an array
                let t = t.borrow();
                if t.is_array() {
                    // Convert to Redis array
                    let mut frames = Vec::new();
                    for i in 1..=t.len() {
                        let key = LuaValue::Number(i as f64);
                        if let Some(value) = t.get(&key) {
                            frames.push(self.lua_to_resp(value.clone())?);
                        }
                    }
                    Ok(RespFrame::Array(Some(frames)))
                } else {
                    // Check if this is an error table (has 'err' field)
                    let err_key = LuaValue::String(LuaString::from_str("err"));
                    if let Some(LuaValue::String(err_msg)) = t.get(&err_key) {
                        // This is an error response from redis.pcall
                        return Ok(RespFrame::Error(Arc::new(err_msg.as_bytes().to_vec())));
                    }
                    
                    // Check if this is a status reply table (has 'ok' field)
                    let ok_key = LuaValue::String(LuaString::from_str("ok"));
                    if let Some(LuaValue::String(ok_msg)) = t.get(&ok_key) {
                        // This is a status response
                        return Ok(RespFrame::SimpleString(Arc::new(ok_msg.as_bytes().to_vec())));
                    }
                    
                    // Non-array tables not supported in Redis Lua
                    Err(ScriptError::ExecutionError("cannot convert a non-array table to Redis response".to_string()).into())
                }
            },
            _ => Err(ScriptError::ExecutionError("cannot convert Lua value to Redis response".to_string()).into()),
        }
    }
    
    /// Convert a Redis response to a Lua value
    fn resp_to_lua(&self, frame: &RespFrame) -> LuaValue {
        match frame {
            RespFrame::SimpleString(s) => LuaValue::String(LuaString::from_bytes(s.to_vec())),
            RespFrame::Error(e) => {
                // Create an error table
                let mut table = LuaTable::new();
                table.set(
                    LuaValue::String(LuaString::from_str("err")),
                    LuaValue::String(LuaString::from_bytes(e.to_vec())),
                );
                LuaValue::Table(Rc::new(RefCell::new(table)))
            },
            RespFrame::Integer(i) => LuaValue::Number(*i as f64),
            RespFrame::BulkString(Some(s)) => LuaValue::String(LuaString::from_bytes(s.to_vec())),
            RespFrame::BulkString(None) => LuaValue::Boolean(false),
            RespFrame::Array(Some(items)) => {
                // Convert to Lua table
                let mut table = LuaTable::new();
                for (i, item) in items.iter().enumerate() {
                    table.set(
                        LuaValue::Number((i + 1) as f64),
                        self.resp_to_lua(item),
                    );
                }
                LuaValue::Table(Rc::new(RefCell::new(table)))
            },
            RespFrame::Array(None) => LuaValue::Boolean(false),
            RespFrame::Null => LuaValue::Boolean(false),
            RespFrame::Boolean(b) => LuaValue::Boolean(*b),
            RespFrame::Double(d) => LuaValue::Number(*d),
            RespFrame::Map(_) => {
                // Create an empty table for now - maps not fully supported
                LuaValue::Table(Rc::new(RefCell::new(LuaTable::new())))
            },
            RespFrame::Set(_) => {
                // Create an empty table for now - sets not fully supported
                LuaValue::Table(Rc::new(RefCell::new(LuaTable::new())))
            },
        }
    }
}

impl RedisApi for FerrousRedisApi {
    fn call(&self, args: &[LuaValue]) -> Result<LuaValue> {
        if args.len() < 1 {
            return Err(LuaError::Runtime("redis.call requires at least a command name".to_string()));
        }
        
        // Get command name
        let cmd_name = match &args[0] {
            LuaValue::String(s) => {
                match s.to_str() {
                    Ok(name) => {
                        println!("[LUA DEBUG] Command: {}", name.to_uppercase());
                        name.to_uppercase()
                    },
                    Err(_) => return Err(LuaError::Runtime("invalid command name".to_string())),
                }
            },
            _ => return Err(LuaError::Runtime("command name must be a string".to_string())),
        };
        
        // Convert arguments to RESP frames
        let mut resp_args = Vec::with_capacity(args.len());
        
        // Add command name as first argument
        resp_args.push(RespFrame::BulkString(Some(Arc::new(cmd_name.as_bytes().to_vec()))));
        
        // Add remaining arguments
        for arg in &args[1..] {
            match arg {
                LuaValue::Nil => resp_args.push(RespFrame::Null),
                LuaValue::Boolean(b) => resp_args.push(RespFrame::Integer(if *b { 1 } else { 0 })),
                LuaValue::Number(n) => {
                    if n.fract() == 0.0 && *n >= std::i64::MIN as f64 && *n <= std::i64::MAX as f64 {
                        resp_args.push(RespFrame::Integer(*n as i64));
                    } else {
                        let s = n.to_string();
                        resp_args.push(RespFrame::BulkString(Some(Arc::new(s.into_bytes()))));
                    }
                },
                LuaValue::String(s) => {
                    resp_args.push(RespFrame::BulkString(Some(Arc::new(s.as_bytes().to_vec()))));
                },
                LuaValue::Table(t) => {
                    // Convert table to string representation
                    resp_args.push(RespFrame::BulkString(Some(Arc::new(format!("{:?}", t).into_bytes()))));
                },
                _ => return Err(LuaError::Runtime("unsupported argument type".to_string())),
            }
        }
        
        println!("[LUA DEBUG] Processing command: {}", cmd_name);
        
        // Execute the command by directly using the storage engine
        let result = match cmd_name.as_str() {
            // Special case for PING - just return PONG
            "PING" => {
                println!("[LUA DEBUG] Executing PING command");
                Ok(LuaValue::String(LuaString::from_str("PONG")))
            },
            
            // String operations
            "GET" => {
                if resp_args.len() != 2 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                println!("[LUA DEBUG] Executing GET for key: {:?}", key);
                
                match self.storage.get_string(self.db, key) {
                    Ok(Some(bytes)) => {
                        println!("[LUA DEBUG] GET found value, length: {}", bytes.len());
                        Ok(LuaValue::String(LuaString::from_bytes(bytes)))
                    },
                    Ok(None) => {
                        println!("[LUA DEBUG] GET key not found");
                        Ok(LuaValue::Nil)
                    },
                    Err(e) => {
                        println!("[LUA ERROR] GET error: {}", e);
                        Err(LuaError::Runtime(format!("Error executing GET: {}", e)))
                    },
                }
            },
            
            "SET" => {
                if resp_args.len() < 3 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref().to_vec(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                let value = match &resp_args[2] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref().to_vec(),
                    RespFrame::Integer(n) => n.to_string().into_bytes(), // Support numeric values
                    _ => return Err(LuaError::Runtime("Invalid value format".to_string())),
                };
                
                println!("[LUA DEBUG] Executing SET for key: {:?}", key);
                
                // Check for expiration options
                let mut expiry = None;
                let mut i = 3;
                while i < resp_args.len() {
                    match &resp_args[i] {
                        RespFrame::BulkString(Some(bytes)) => {
                            let opt = String::from_utf8_lossy(bytes).to_uppercase();
                            match opt.as_str() {
                                "EX" => {
                                    if i + 1 < resp_args.len() {
                                        if let RespFrame::BulkString(Some(sec_bytes)) = &resp_args[i + 1] {
                                            if let Ok(sec_str) = String::from_utf8(sec_bytes.to_vec()) {
                                                if let Ok(seconds) = sec_str.parse::<u64>() {
                                                    expiry = Some(Duration::from_secs(seconds));
                                                    i += 2;
                                                    continue;
                                                }
                                            }
                                        }
                                    }
                                },
                                "PX" => {
                                    if i + 1 < resp_args.len() {
                                        if let RespFrame::BulkString(Some(ms_bytes)) = &resp_args[i + 1] {
                                            if let Ok(ms_str) = String::from_utf8(ms_bytes.to_vec()) {
                                                if let Ok(millis) = ms_str.parse::<u64>() {
                                                    expiry = Some(Duration::from_millis(millis));
                                                    i += 2;
                                                    continue;
                                                }
                                            }
                                        }
                                    }
                                },
                                _ => {}
                            }
                        },
                        _ => {}
                    }
                    i += 1;
                }
                
                // Set with or without expiration
                let result = if let Some(expires) = expiry {
                    self.storage.set_string_ex(self.db, key, value, expires)
                } else {
                    self.storage.set_string(self.db, key, value)
                };
                
                match result {
                    Ok(_) => Ok(LuaValue::String(LuaString::from_str("OK"))),
                    Err(e) => Err(LuaError::Runtime(format!("Error executing SET: {}", e))),
                }
            },
            
            "DEL" => {
                if resp_args.len() < 2 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let mut deleted = 0;
                
                for i in 1..resp_args.len() {
                    let key = match &resp_args[i] {
                        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                        _ => continue, // Skip invalid keys
                    };
                    
                    if self.storage.delete(self.db, key).unwrap_or(false) {
                        deleted += 1;
                    }
                }
                
                Ok(LuaValue::Number(deleted as f64))
            },
            
            "EXISTS" => {
                if resp_args.len() < 2 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let mut count = 0;
                
                for i in 1..resp_args.len() {
                    let key = match &resp_args[i] {
                        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                        _ => continue, // Skip invalid keys
                    };
                    
                    if self.storage.exists(self.db, key).unwrap_or(false) {
                        count += 1;
                    }
                }
                
                Ok(LuaValue::Number(count as f64))
            },
            
            "INCR" => {
                if resp_args.len() != 2 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref().to_vec(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                match self.storage.incr(self.db, key) {
                    Ok(new_value) => Ok(LuaValue::Number(new_value as f64)),
                    Err(e) => Err(LuaError::Runtime(format!("Error executing INCR: {}", e))),
                }
            },
            
            "INCRBY" => {
                if resp_args.len() != 3 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref().to_vec(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                let increment = match &resp_args[2] {
                    RespFrame::Integer(n) => *n,
                    RespFrame::BulkString(Some(bytes)) => {
                        match String::from_utf8_lossy(bytes).parse::<i64>() {
                            Ok(n) => n,
                            Err(_) => return Err(LuaError::Runtime("Invalid increment value".to_string())),
                        }
                    },
                    _ => return Err(LuaError::Runtime("Invalid increment format".to_string())),
                };
                
                match self.storage.incr_by(self.db, key, increment) {
                    Ok(new_value) => Ok(LuaValue::Number(new_value as f64)),
                    Err(e) => Err(LuaError::Runtime(format!("Error executing INCRBY: {}", e))),
                }
            },
            
            "HSET" => {
                if resp_args.len() < 4 || resp_args.len() % 2 != 0 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref().to_vec(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                let mut field_count = 0;
                
                // Process field-value pairs
                let mut field_values = Vec::new();
                for i in (2..resp_args.len()).step_by(2) {
                    let field = match &resp_args[i] {
                        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().to_vec(),
                        _ => return Err(LuaError::Runtime("Invalid field format".to_string())),
                    };
                    
                    let value = match &resp_args[i+1] {
                        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().to_vec(),
                        RespFrame::Integer(n) => n.to_string().into_bytes(),
                        _ => return Err(LuaError::Runtime("Invalid value format".to_string())),
                    };
                    
                    field_values.push((field, value));
                }
                
                // Call the storage engine's hset with field_values
                match self.storage.hset(self.db, key, field_values) {
                    Ok(count) => Ok(LuaValue::Number(count as f64)),
                    Err(e) => Err(LuaError::Runtime(format!("Error executing HSET: {}", e)))
                }
            },
            
            "HGET" => {
                if resp_args.len() != 3 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                let field = match &resp_args[2] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                    _ => return Err(LuaError::Runtime("Invalid field format".to_string())),
                };
                
                match self.storage.hget(self.db, key, field) {
                    Ok(Some(value)) => Ok(LuaValue::String(LuaString::from_bytes(value))),
                    Ok(None) => Ok(LuaValue::Nil),
                    Err(e) => Err(LuaError::Runtime(format!("Error executing HGET: {}", e))),
                }
            },
            
            // List operations
            "LPUSH" => {
                if resp_args.len() < 3 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref().to_vec(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                let mut values = Vec::new();
                for i in 2..resp_args.len() {
                    match &resp_args[i] {
                        RespFrame::BulkString(Some(bytes)) => values.push(bytes.as_ref().to_vec()),
                        RespFrame::Integer(n) => values.push(n.to_string().into_bytes()),
                        _ => return Err(LuaError::Runtime("Invalid value format".to_string())),
                    }
                }
                
                match self.storage.lpush(self.db, key, values) {
                    Ok(new_len) => Ok(LuaValue::Number(new_len as f64)),
                    Err(e) => Err(LuaError::Runtime(format!("Error executing LPUSH: {}", e))),
                }
            },
            
            "RPUSH" => {
                if resp_args.len() < 3 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref().to_vec(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                let mut values = Vec::new();
                for i in 2..resp_args.len() {
                    match &resp_args[i] {
                        RespFrame::BulkString(Some(bytes)) => values.push(bytes.as_ref().to_vec()),
                        RespFrame::Integer(n) => values.push(n.to_string().into_bytes()),
                        _ => return Err(LuaError::Runtime("Invalid value format".to_string())),
                    }
                }
                
                match self.storage.rpush(self.db, key, values) {
                    Ok(new_len) => Ok(LuaValue::Number(new_len as f64)),
                    Err(e) => Err(LuaError::Runtime(format!("Error executing RPUSH: {}", e))),
                }
            },
            
            "LPOP" => {
                if resp_args.len() != 2 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                match self.storage.lpop(self.db, key) {
                    Ok(Some(value)) => Ok(LuaValue::String(LuaString::from_bytes(value))),
                    Ok(None) => Ok(LuaValue::Nil),
                    Err(e) => Err(LuaError::Runtime(format!("Error executing LPOP: {}", e))),
                }
            },
            
            "RPOP" => {
                if resp_args.len() != 2 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                match self.storage.rpop(self.db, key) {
                    Ok(Some(value)) => Ok(LuaValue::String(LuaString::from_bytes(value))),
                    Ok(None) => Ok(LuaValue::Nil),
                    Err(e) => Err(LuaError::Runtime(format!("Error executing RPOP: {}", e))),
                }
            },
            
            "LLEN" => {
                if resp_args.len() != 2 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                match self.storage.llen(self.db, key) {
                    Ok(len) => Ok(LuaValue::Number(len as f64)),
                    Err(e) => Err(LuaError::Runtime(format!("Error executing LLEN: {}", e))),
                }
            },
            
            // Set operations
            "SADD" => {
                if resp_args.len() < 3 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref().to_vec(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                let mut members = Vec::new();
                for i in 2..resp_args.len() {
                    match &resp_args[i] {
                        RespFrame::BulkString(Some(bytes)) => members.push(bytes.as_ref().to_vec()),
                        RespFrame::Integer(n) => members.push(n.to_string().into_bytes()),
                        _ => return Err(LuaError::Runtime("Invalid member format".to_string())),
                    }
                }
                
                match self.storage.sadd(self.db, key, members) {
                    Ok(added) => {
                        // Explicitly cast usize to f64
                        let result: f64 = added as f64;
                        Ok(LuaValue::Number(result)) 
                    },
                    Err(e) => Err(LuaError::Runtime(format!("Error executing SADD: {}", e))),
                }
            },
            
            "SREM" => {
                if resp_args.len() < 3 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                // Convert all elements to bytes vectors
                let mut member_values = Vec::new();
                for i in 2..resp_args.len() {
                    match &resp_args[i] {
                        RespFrame::BulkString(Some(bytes)) => member_values.push(bytes.as_ref().to_vec()),
                        RespFrame::Integer(n) => member_values.push(n.to_string().into_bytes()),
                        _ => return Err(LuaError::Runtime("Invalid member format".to_string())),
                    }
                }
                
                match self.storage.srem(self.db, key, &member_values) {
                    Ok(removed) => Ok(LuaValue::Number(removed as f64)),
                    Err(e) => Err(LuaError::Runtime(format!("Error executing SREM: {}", e))),
                }
            },
            
            "SISMEMBER" => {
                if resp_args.len() != 3 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                let member = match &resp_args[2] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                    _ => return Err(LuaError::Runtime("Invalid member format".to_string())),
                };
                
                match self.storage.sismember(self.db, key, member) {
                    Ok(is_member) => Ok(LuaValue::Number(if is_member { 1.0 } else { 0.0 })),
                    Err(e) => Err(LuaError::Runtime(format!("Error executing SISMEMBER: {}", e))),
                }
            },
            
            "SCARD" => {
                if resp_args.len() != 2 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                match self.storage.scard(self.db, key) {
                    Ok(count) => Ok(LuaValue::Number(count as f64)),
                    Err(e) => Err(LuaError::Runtime(format!("Error executing SCARD: {}", e))),
                }
            },
            
            // Sorted set operations  
            "ZADD" => {
                if resp_args.len() < 4 || (resp_args.len() - 2) % 2 != 0 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref().to_vec(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                let mut added_count = 0;
                
                // Process score/member pairs one at a time
                for i in (2..resp_args.len()).step_by(2) {
                    let score = match &resp_args[i] {
                        RespFrame::Integer(n) => *n as f64,
                        RespFrame::BulkString(Some(bytes)) => {
                            match String::from_utf8_lossy(bytes).parse::<f64>() {
                                Ok(f) => f,
                                Err(_) => return Err(LuaError::Runtime("Invalid score format".to_string())),
                            }
                        },
                        _ => return Err(LuaError::Runtime("Invalid score format".to_string())),
                    };
                    
                    let member = match &resp_args[i+1] {
                        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().to_vec(),
                        _ => return Err(LuaError::Runtime("Invalid member format".to_string())),
                    };
                    
                    // Call ZADD with a single member/score
                    match self.storage.zadd(self.db, key.clone(), member, score) {
                        Ok(added) => added_count += added as usize,
                        Err(e) => return Err(LuaError::Runtime(format!("Error executing ZADD: {}", e))),
                    }
                }
                
                Ok(LuaValue::Number(added_count as f64))
            },
            
            "ZSCORE" => {
                if resp_args.len() != 3 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                let member = match &resp_args[2] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                    _ => return Err(LuaError::Runtime("Invalid member format".to_string())),
                };
                
                match self.storage.zscore(self.db, key, member) {
                    Ok(Some(score)) => Ok(LuaValue::Number(score)),
                    Ok(None) => Ok(LuaValue::Nil),
                    Err(e) => Err(LuaError::Runtime(format!("Error executing ZSCORE: {}", e))),
                }
            },
            
            "ZCARD" => {
                if resp_args.len() != 2 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                match self.storage.scard(self.db, key) {
                    Ok(count) => Ok(LuaValue::Number(count as f64)),
                    Err(e) => Err(LuaError::Runtime(format!("Error executing ZCARD: {}", e))),
                }
            },
            
            "KEYS" => {
                if resp_args.len() != 2 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let pattern = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                    _ => return Err(LuaError::Runtime("Invalid pattern format".to_string())),
                };
                
                match self.storage.keys(self.db, pattern) {
                    Ok(keys) => {
                        // Create a Lua table from the keys
                        let mut table = LuaTable::new();
                        for (i, key) in keys.into_iter().enumerate() {
                            table.set(
                                LuaValue::Number((i + 1) as f64),
                                LuaValue::String(LuaString::from_bytes(key))
                            );
                        }
                        Ok(LuaValue::Table(Rc::new(RefCell::new(table))))
                    },
                    Err(e) => Err(LuaError::Runtime(format!("Error executing KEYS: {}", e))),
                }
            },
            
            "TYPE" => {
                if resp_args.len() != 2 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                match self.storage.key_type(self.db, key) {
                    Ok(type_name) => {
                        Ok(LuaValue::String(LuaString::from_str(&type_name)))
                    },
                    Err(e) => Err(LuaError::Runtime(format!("Error executing TYPE: {}", e))),
                }
            },
            
            "EXPIRE" => {
                if resp_args.len() != 3 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                let seconds = match &resp_args[2] {
                    RespFrame::Integer(n) => *n as u64,
                    RespFrame::BulkString(Some(bytes)) => {
                        match String::from_utf8_lossy(bytes).parse::<u64>() {
                            Ok(n) => n,
                            Err(_) => return Err(LuaError::Runtime("Invalid seconds format".to_string())),
                        }
                    },
                    _ => return Err(LuaError::Runtime("Invalid seconds format".to_string())),
                };
                
                match self.storage.expire(self.db, key, Duration::from_secs(seconds)) {
                    Ok(set) => Ok(LuaValue::Number(if set { 1.0 } else { 0.0 })),
                    Err(e) => Err(LuaError::Runtime(format!("Error executing EXPIRE: {}", e))),
                }
            },
            
            "TTL" => {
                if resp_args.len() != 2 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                match self.storage.ttl(self.db, key) {
                    Ok(Some(ttl)) => Ok(LuaValue::Number(ttl.as_secs() as f64)),
                    Ok(None) => {
                        // Check if key exists without expiry
                        if self.storage.exists(self.db, key).unwrap_or(false) {
                            Ok(LuaValue::Number(-1.0))
                        } else {
                            Ok(LuaValue::Number(-2.0))
                        }
                    },
                    Err(e) => Err(LuaError::Runtime(format!("Error executing TTL: {}", e))),
                }
            },
            
            // Add any other commands you want to support...
            
            _ => {
                println!("[LUA ERROR] Command not implemented in script mode: {}", cmd_name);
                Err(LuaError::Runtime(format!("Command '{}' not supported in script mode", cmd_name)))
            },
        };
        
        match &result {
            Ok(val) => println!("[LUA DEBUG] Command {} returned: {:?}", cmd_name, val),
            Err(e) => println!("[LUA ERROR] Command {} error: {}", cmd_name, e),
        }
        
        result
    }
    
    fn pcall(&self, args: &[LuaValue]) -> Result<LuaValue> {
        // Call through redis.call but catch errors
        match self.call(args) {
            Ok(result) => Ok(result),
            Err(err) => {
                // Create an error table
                let mut table = LuaTable::new();
                table.set(
                    LuaValue::String(LuaString::from_str("err")),
                    LuaValue::String(LuaString::from_str(&err.to_string())),
                );
                Ok(LuaValue::Table(Rc::new(RefCell::new(table))))
            }
        }
    }
    
    fn log(&self, level: i32, message: &str) -> Result<()> {
        // Log message to server log
        println!("[LUA] [{}] {}", level, message);
        Ok(())
    }
}

// Redis API function implementations

/// redis.call implementation
fn redis_call_impl(vm: &mut LuaVm, args: &[LuaValue]) -> Result<LuaValue> {
    // Add debug logging to help diagnose issues
    if let Some(cmd) = args.get(0) {
        if let LuaValue::String(ref s) = cmd {
            if let Ok(cmd_str) = s.to_str() {
                println!("[DEBUG LUA] redis.call: {}", cmd_str);
            }
        }
    }
    
    // Call redis.call through the VM helper methods, with error handling
    vm.set_redis_api_if_missing()?;
    
    match vm.call_redis_api(args, false) {
        Ok(value) => Ok(value),
        Err(e) => {
            // Log the error but avoid panicking the server
            println!("[ERROR LUA] redis.call error: {}", e);
            Err(e)
        }
    }
}

/// redis.pcall implementation
fn redis_pcall_impl(vm: &mut LuaVm, args: &[LuaValue]) -> Result<LuaValue> {
    // Add debug logging to help diagnose issues
    if let Some(cmd) = args.get(0) {
        if let LuaValue::String(ref s) = cmd {
            if let Ok(cmd_str) = s.to_str() {
                println!("[DEBUG LUA] redis.pcall: {}", cmd_str);
            }
        }
    }
    
    // Call redis.pcall through the VM helper methods
    vm.set_redis_api_if_missing()?;
    
    // pcall catches errors and returns them as values rather than throwing
    vm.call_redis_api(args, true)
}

/// redis.log implementation
fn redis_log_impl(vm: &mut LuaVm, args: &[LuaValue]) -> Result<LuaValue> {
    if args.len() < 2 {
        return Err(LuaError::Runtime("redis.log requires at least 2 arguments".to_string()));
    }
    
    // Extract log level
    let level = match &args[0] {
        LuaValue::Number(n) => *n as i32,
        _ => return Err(LuaError::Runtime("log level must be a number".to_string())),
    };
    
    // Extract message
    let message = match &args[1] {
        LuaValue::String(s) => {
            match s.to_str() {
                Ok(msg) => msg.to_string(),
                Err(_) => return Err(LuaError::Runtime("invalid message encoding".to_string())),
            }
        },
        LuaValue::Number(n) => n.to_string(),
        LuaValue::Boolean(b) => b.to_string(),
        LuaValue::Nil => "nil".to_string(),
        _ => return Err(LuaError::Runtime("cannot convert value to string".to_string())),
    };
    
    // Log the message through the VM
    vm.log_message(level, &message)?;
    
    // Redis log returns nil
    Ok(LuaValue::Nil)
}

// Helper extension for LuaTable
impl LuaTable {
    /// Create a table with initial values
    fn new_with_values(entries: &[(LuaValue, LuaValue)]) -> Rc<RefCell<Self>> {
        let mut table = Self::new();
        for (k, v) in entries {
            table.set(k.clone(), v.clone());
        }
        Rc::new(RefCell::new(table))
    }
}

/// Compute the SHA1 hash of a script
fn compute_sha1(script: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    // In a production implementation, we would use a proper SHA1 library
    // For this version focused on Redis compatibility, we'll implement a simple SHA1-like hash
    
    // Create two hashers for increased collision resistance
    let mut hasher1 = DefaultHasher::new();
    let mut hasher2 = DefaultHasher::new();
    
    // Hash the script with both hashers
    script.hash(&mut hasher1);
    (script.len() as u64).hash(&mut hasher2);
    for (i, c) in script.chars().enumerate() {
        if i % 2 == 0 {
            c.hash(&mut hasher1);
        } else {
            c.hash(&mut hasher2);
        }
    }
    
    // Combine the hashes
    let hash1 = hasher1.finish();
    let hash2 = hasher2.finish();
    let combined = hash1 ^ (hash2 << 1) ^ (hash2 >> 1);
    
    // Format the hash to look like a SHA1 (40 hex digits)
    format!("{:016x}{:016x}{:08x}", hash1, hash2, combined & 0xFFFFFFFF)
}