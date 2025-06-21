//! Lua script executor for Ferrous
//!
//! This module integrates the Lua interpreter with Redis commands.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
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

/// Redis Lua script executor
pub struct ScriptExecutor {
    /// Script cache (SHA1 -> compiled script)
    script_cache: Arc<Mutex<HashMap<String, CompiledScript>>>,
    
    /// Storage engine reference
    storage: Arc<StorageEngine>,
    
    /// VM pool for reuse
    vm_pool: Arc<Mutex<Vec<LuaVm>>>,
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
            script_cache: Arc::new(Mutex::new(HashMap::new())),
            storage,
            vm_pool: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    /// Execute a script
    pub fn eval(&self, script: &str, keys: Vec<Vec<u8>>, argv: Vec<Vec<u8>>, db: DatabaseIndex) -> std::result::Result<RespFrame, FerrousError> {
        // Compile the script if not already in cache
        let compiled = {
            let sha1 = self.compute_sha1(script);
            let mut cache = self.script_cache.lock().unwrap();
            
            if let Some(compiled) = cache.get(&sha1) {
                compiled.clone()
            } else {
                let compiled = self.compile_script(script, sha1.clone())?;
                cache.insert(sha1, compiled.clone());
                compiled
            }
        };
        
        // Execute the compiled script
        self.execute_compiled(compiled, keys, argv, db)
    }
    
    /// Execute a script by SHA1 hash
    pub fn evalsha(&self, sha1: &str, keys: Vec<Vec<u8>>, argv: Vec<Vec<u8>>, db: DatabaseIndex) -> std::result::Result<RespFrame, FerrousError> {
        // Look up the script in the cache
        let compiled = {
            let cache = self.script_cache.lock().unwrap();
            cache.get(sha1).cloned()
        };
        
        // Execute the script if found
        match compiled {
            Some(script) => self.execute_compiled(script, keys, argv, db),
            None => Err(ScriptError::NotFound.into()),
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
        // Get or create a VM
        let mut vm = self.get_vm();
        
        // Set up the environment
        self.setup_vm_environment(&mut vm, keys.clone(), argv.clone(), db)?;
        
        // We'll use the simplified path for now - the run method will try the full VM first with fallback
        // to pattern matching for reliability
        let result = vm.run_simple(&script.source);
        
        // Process result
        match result {
            Ok(lua_result) => {
                // Convert result to Redis response
                let resp = self.lua_to_resp(lua_result)?;
                
                // Return VM to pool
                self.return_vm(vm);
                
                Ok(resp)
            },
            Err(e) => {
                // Return VM to pool even on error
                self.return_vm(vm);
                
                Err(ScriptError::ExecutionError(format!("Script execution error: {}", e)).into())
            },
        }
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
        
        // Create the redis API
        let redis_api = Box::new(FerrousRedisApi {
            storage: self.storage.clone(),
            db,
            keys,
            argv,
        });
        
        vm.set_redis_api(redis_api);
        
        // Register standard libraries
        self.register_standard_libs(vm)?;
        
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
    
    /// Compute SHA1 hash of a script
    fn compute_sha1(&self, script: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        // Since Rust's standard library doesn't include cryptographic hashes,
        // we'll use a simple hash for now. In a production implementation,
        // you'd use a proper SHA1 implementation from a crate like sha1 or ring.
        let mut hasher = DefaultHasher::new();
        script.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
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
                    Ok(name) => name.to_uppercase(),
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
        
        // Execute the command by directly using the storage engine
        let result = match cmd_name.as_str() {
            // String operations
            "GET" => {
                if resp_args.len() != 2 {
                    return Err(LuaError::Runtime(format!("Wrong number of arguments for '{}'", cmd_name)));
                }
                
                let key = match &resp_args[1] {
                    RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                    _ => return Err(LuaError::Runtime("Invalid key format".to_string())),
                };
                
                match self.storage.get_string(self.db, key) {
                    Ok(Some(bytes)) => Ok(LuaValue::String(LuaString::from_bytes(bytes))),
                    Ok(None) => Ok(LuaValue::Nil),
                    Err(e) => Err(LuaError::Runtime(format!("Error executing GET: {}", e))),
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
                
                match self.storage.set_string(self.db, key, value) {
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
            
            // Add more command implementations as needed...
            
            _ => Err(LuaError::Runtime(format!("Command '{}' not implemented in script mode", cmd_name))),
        };
        
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
    // Call redis.call through the VM helper methods
    vm.set_redis_api_if_missing()?;
    vm.call_redis_api(args, false)
}

/// redis.pcall implementation
fn redis_pcall_impl(vm: &mut LuaVm, args: &[LuaValue]) -> Result<LuaValue> {
    // Call redis.pcall through the VM helper methods
    vm.set_redis_api_if_missing()?;
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