//! Script executor for Redis Lua scripts

use crate::lua_new::heap::{LuaHeap, ClosureObject};
use crate::lua_new::value::{Value, ClosureHandle, StringHandle};
use crate::lua_new::vm::{LuaVM, ExecutionContext};
use crate::lua_new::error::{LuaError, Result};
use crate::lua_new::{VMConfig, LuaLimits};
use crate::lua_new::redis_api::RedisApiContext;
use crate::storage::engine::StorageEngine;
use crate::protocol::resp::RespFrame;
use crate::error::FerrousError;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// A compiled Lua script
#[derive(Clone)]
pub struct CompiledScript {
    /// Original source code
    pub source: String,
    
    /// SHA1 hash of the script
    pub sha1: String,
    
    /// Compiled closure handle (in a shared heap)
    pub closure: ClosureHandle,
}

/// Runtime information for a running script
pub struct RunningScript {
    /// SHA1 of the script
    pub sha1: String,
    
    /// Start time
    pub start_time: Instant,
    
    /// Kill flag
    pub kill_flag: Arc<AtomicBool>,
}

/// Execution statistics
#[derive(Default)]
pub struct ExecutionStats {
    /// Total scripts executed
    pub total_executed: u64,
    
    /// Total execution time (microseconds)
    pub total_time_us: u64,
    
    /// Cache hits
    pub cache_hits: u64,
    
    /// Cache misses
    pub cache_misses: u64,
}

/// Script executor for managing Lua script execution
pub struct ScriptExecutor {
    /// Script cache by SHA1
    cache: Arc<RwLock<HashMap<String, CompiledScript>>>,
    
    /// VM pool for reuse
    vm_pool: Arc<Mutex<Vec<LuaVM>>>,
    
    /// Currently running script
    current_script: Arc<Mutex<Option<RunningScript>>>,
    
    /// Storage engine reference
    storage: Arc<StorageEngine>,
    
    /// Execution statistics
    stats: Arc<Mutex<ExecutionStats>>,
    
    /// Configuration
    config: VMConfig,
}

/// Script-specific errors
#[derive(Debug, Clone)]
pub enum ScriptError {
    /// Script not found in cache
    NotFound,
    
    /// Script compilation failed
    CompilationFailed(String),
    
    /// Script execution failed
    ExecutionFailed(String),
    
    /// Script was killed
    Killed,
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScriptError::NotFound => write!(f, "NOSCRIPT No matching script. Please use EVAL."),
            ScriptError::CompilationFailed(msg) => write!(f, "ERR Error compiling script: {}", msg),
            ScriptError::ExecutionFailed(msg) => write!(f, "ERR {}", msg),
            ScriptError::Killed => write!(f, "ERR Script killed by user with SCRIPT KILL"),
        }
    }
}

impl std::error::Error for ScriptError {}

impl ScriptExecutor {
    /// Create a new script executor
    pub fn new(storage: Arc<StorageEngine>) -> Self {
        let config = VMConfig::default();
        
        ScriptExecutor {
            cache: Arc::new(RwLock::new(HashMap::new())),
            vm_pool: Arc::new(Mutex::new(Vec::new())),
            current_script: Arc::new(Mutex::new(None)),
            storage,
            stats: Arc::new(Mutex::new(ExecutionStats::default())),
            config,
        }
    }
    
    /// Execute a script with EVAL
    pub fn eval(
        &self,
        source: &str,
        keys: Vec<Vec<u8>>,
        args: Vec<Vec<u8>>,
        db: usize,
    ) -> std::result::Result<RespFrame, FerrousError> {
        println!("[LUA_NEW] Executing EVAL source={} keys={} args={}", 
                source, keys.len(), args.len());
        
        // Compute SHA1
        let sha1 = compute_sha1(source);
        
        // Try cache first
        let script = match self.get_cached(&sha1) {
            Some(script) => {
                self.stats.lock().unwrap().cache_hits += 1;
                script
            }
            None => {
                self.stats.lock().unwrap().cache_misses += 1;
                
                // Compile new script
                match self.compile_script(source, sha1.clone()) {
                    Ok(script) => {
                        // Add to cache
                        self.add_to_cache(script.clone());
                        script
                    }
                    Err(e) => {
                        return Ok(RespFrame::Error(Arc::new(
                            format!("ERR {}", e).into_bytes()
                        )));
                    }
                }
            }
        };
        
        // Execute script
        self.execute_script(script, keys, args, db)
    }
    
    /// Execute a script with EVALSHA
    pub fn evalsha(
        &self,
        sha1: &str,
        keys: Vec<Vec<u8>>,
        args: Vec<Vec<u8>>,
        db: usize,
    ) -> std::result::Result<RespFrame, FerrousError> {
        println!("[LUA_NEW] Executing EVALSHA sha1={} keys={} args={}", 
                sha1, keys.len(), args.len());
                
        // Look up in cache
        let script = match self.get_cached(sha1) {
            Some(script) => {
                self.stats.lock().unwrap().cache_hits += 1;
                script
            }
            None => {
                self.stats.lock().unwrap().cache_misses += 1;
                return Ok(RespFrame::Error(Arc::new(
                    b"NOSCRIPT No matching script. Please use EVAL.".to_vec()
                )));
            }
        };
        
        // Execute script
        self.execute_script(script, keys, args, db)
    }
    
    /// Load a script without executing (SCRIPT LOAD)
    pub fn load(&self, source: &str) -> std::result::Result<String, ScriptError> {
        let sha1 = compute_sha1(source);
        
        // Check if already loaded
        if self.get_cached(&sha1).is_some() {
            return Ok(sha1);
        }
        
        // Compile and cache
        let script = self.compile_script(source, sha1.clone())?;
        self.add_to_cache(script);
        
        Ok(sha1)
    }
    
    /// Check if scripts exist (SCRIPT EXISTS)
    pub fn exists(&self, sha1s: &[String]) -> Vec<bool> {
        let cache = self.cache.read().unwrap();
        sha1s.iter().map(|sha1| cache.contains_key(sha1)).collect()
    }
    
    /// Flush the script cache (SCRIPT FLUSH)
    pub fn flush(&self) {
        self.cache.write().unwrap().clear();
    }
    
    /// Kill the currently running script (SCRIPT KILL)
    pub fn kill(&self) -> bool {
        let mut current = self.current_script.lock().unwrap();
        if let Some(ref script) = *current {
            script.kill_flag.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }
    
    /// Get a cached script
    fn get_cached(&self, sha1: &str) -> Option<CompiledScript> {
        self.cache.read().unwrap().get(sha1).cloned()
    }
    
    /// Add a script to cache
    fn add_to_cache(&self, script: CompiledScript) {
        self.cache.write().unwrap().insert(script.sha1.clone(), script);
    }
    
    /// Compile a script
    fn compile_script(&self, source: &str, sha1: String) -> std::result::Result<CompiledScript, ScriptError> {
        // For now, we'll create a simple stub
        // In a full implementation, this would parse and compile Lua code
        
        // Create a temporary VM for compilation
        let mut vm = LuaVM::new(self.config.clone());
        
        // Create a dummy closure
        // In real implementation, this would be the compiled bytecode
        let proto = crate::lua_new::value::FunctionProto::default();
        let closure = vm.heap.alloc_closure(proto, Vec::new());
        
        Ok(CompiledScript {
            source: source.to_string(),
            sha1,
            closure,
        })
    }
    
    /// Execute a compiled script
    fn execute_script(
        &self,
        script: CompiledScript,
        keys: Vec<Vec<u8>>,
        args: Vec<Vec<u8>>,
        db: usize,
    ) -> std::result::Result<RespFrame, FerrousError> {
        let start_time = Instant::now();
        
        println!("[LUA_NEW] Starting script execution");
        
        // Get or create a VM
        let mut vm = match self.get_vm() {
            Ok(vm) => vm,
            Err(e) => return Err(e),
        };
        
        // Set up killable execution
        let kill_flag = Arc::new(AtomicBool::new(false));
        vm.set_kill_flag(Arc::clone(&kill_flag));
        
        // Set up environment
        match self.setup_environment(&mut vm, &keys, &args, db) {
            Ok(()) => {},
            Err(e) => {
                self.return_vm(vm);
                return Err(e);
            }
        }
        
        println!("[LUA_NEW] Environment setup complete, executing script");
        
        // Track execution
        {
            let mut current = self.current_script.lock().unwrap();
            *current = Some(RunningScript {
                sha1: script.sha1.clone(),
                start_time,
                kill_flag: Arc::clone(&kill_flag),
            });
        }
        
        // Execute script
        let result = match vm.execute_with_limits(script.closure, &[], Arc::clone(&kill_flag)) {
            Ok(value) => {
                println!("[LUA_NEW] Script executed successfully, converting result to RESP");
                // Convert to Redis response
                match RedisApiContext::lua_to_resp(&mut vm, value) {
                    Ok(resp) => Ok(resp),
                    Err(e) => Ok(RespFrame::Error(Arc::new(format!("ERR {}", e).into_bytes()))),
                }
            }
            Err(e) => {
                // Map error to Redis format
                println!("[LUA_NEW] Script execution error: {}", e);
                if matches!(e, LuaError::ScriptKilled) {
                    Ok(RespFrame::Error(Arc::new(
                        b"ERR Script killed by user with SCRIPT KILL".to_vec()
                    )))
                } else {
                    Ok(RespFrame::Error(Arc::new(
                        format!("ERR {}", e).into_bytes()
                    )))
                }
            }
        };
        
        // Clear current script
        {
            let mut current = self.current_script.lock().unwrap();
            *current = None;
        }
        
        // Update stats
        {
            let mut stats = self.stats.lock().unwrap();
            stats.total_executed += 1;
            stats.total_time_us += start_time.elapsed().as_micros() as u64;
        }
        
        // Return VM to pool
        self.return_vm(vm);
        
        println!("[LUA_NEW] Script execution complete");
        result
    }
    
    /// Get a VM from the pool or create a new one
    fn get_vm(&self) -> std::result::Result<LuaVM, FerrousError> {
        let mut pool = self.vm_pool.lock().unwrap();
        
        if let Some(mut vm) = pool.pop() {
            // Reset the VM for reuse
            vm.instruction_count = 0;
            Ok(vm)
        } else {
            // Create new VM
            let vm = LuaVM::new(self.config.clone());
            Ok(vm)
        }
    }
    
    /// Return a VM to the pool
    fn return_vm(&self, vm: LuaVM) {
        let mut pool = self.vm_pool.lock().unwrap();
        
        // Keep pool size reasonable
        if pool.len() < 10 {
            pool.push(vm);
        }
    }
    
    /// Setup the Lua environment
    fn setup_environment(
        &self,
        vm: &mut LuaVM,
        keys: &[Vec<u8>],
        args: &[Vec<u8>],
        db: usize,
    ) -> std::result::Result<(), FerrousError> {
        println!("[LUA_NEW] Setting up environment with {} keys and {} args", 
                keys.len(), args.len());
        
        // Create KEYS table
        let keys_table = vm.heap.alloc_table();
        for (i, key) in keys.iter().enumerate() {
            // Lua tables are 1-indexed
            let idx = Value::Number((i + 1) as f64);
            let val = Value::String(vm.heap.alloc_string(key));
            vm.heap.get_table_mut(keys_table)
                .map_err(|e| FerrousError::Internal(e.to_string()))?
                .set(idx, val);
        }
        
        // Create ARGV table
        let argv_table = vm.heap.alloc_table();
        for (i, arg) in args.iter().enumerate() {
            // Lua tables are 1-indexed
            let idx = Value::Number((i + 1) as f64);
            let val = Value::String(vm.heap.alloc_string(arg));
            vm.heap.get_table_mut(argv_table)
                .map_err(|e| FerrousError::Internal(e.to_string()))?
                .set(idx, val);
        }
        
        // Set in globals
        let globals = vm.globals();
        let keys_name = vm.heap.create_string("KEYS");
        let argv_name = vm.heap.create_string("ARGV");
        
        // Set KEYS global
        vm.heap.get_table_mut(globals)
            .map_err(|e| FerrousError::Internal(e.to_string()))?
            .set(Value::String(keys_name), Value::Table(keys_table));
        
        // Set ARGV global
        vm.heap.get_table_mut(globals)
            .map_err(|e| FerrousError::Internal(e.to_string()))?
            .set(Value::String(argv_name), Value::Table(argv_table));
        
        // Register Redis API
        let redis_ctx = RedisApiContext::new(Arc::clone(&self.storage), db);
        if let Err(e) = RedisApiContext::register_with_context(vm, redis_ctx) {
            return Err(FerrousError::Internal(e.to_string()));
        }
        
        // Apply sandbox
        let sandbox = crate::lua_new::sandbox::LuaSandbox::redis_compatible();
        if let Err(e) = sandbox.apply(vm) {
            return Err(FerrousError::Internal(e.to_string()));
        }
        
        Ok(())
    }

}

/// Compute SHA1 hash of a script
fn compute_sha1(script: &str) -> String {
    use std::fmt::Write;
    
    // Simple hash function for now
    // In real implementation, use proper SHA1
    let mut hash = 0u64;
    for byte in script.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
    }
    
    let mut result = String::new();
    write!(&mut result, "{:040x}", hash).unwrap();
    result
}

/// Extension trait for converting Ferrous errors
impl From<ScriptError> for FerrousError {
    fn from(err: ScriptError) -> Self {
        FerrousError::Internal(err.to_string())
    }
}