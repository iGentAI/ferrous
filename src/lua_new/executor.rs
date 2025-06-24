//! Script executor for Redis Lua scripts

use crate::lua_new::heap::{LuaHeap, ClosureObject};
use crate::lua_new::value::{Value, ClosureHandle, StringHandle};
use crate::lua_new::vm::{LuaVM, ExecutionContext};
use crate::lua_new::error::{LuaError, Result};
use crate::lua_new::{VMConfig, LuaLimits};
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
        
        // Get or create a VM
        let mut vm = self.get_vm()?;
        
        // Set up killable execution
        let kill_flag = Arc::new(AtomicBool::new(false));
        vm.set_kill_flag(Arc::clone(&kill_flag));
        
        // Set up environment
        self.setup_environment(&mut vm, &keys, &args, db)?;
        
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
        let result = match vm.execute_with_limits(script.closure, &[], kill_flag) {
            Ok(value) => {
                // Convert to Redis response
                self.value_to_resp(value, &vm.heap)
            }
            Err(e) => {
                // Map error to Redis format
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
    
    /// Set up the Lua environment with KEYS and ARGV
    fn setup_environment(
        &self,
        vm: &mut LuaVM,
        keys: &[Vec<u8>],
        args: &[Vec<u8>],
        _db: usize,
    ) -> std::result::Result<(), FerrousError> {
        // Create KEYS table
        let keys_table = vm.heap.alloc_table();
        for (i, key) in keys.iter().enumerate() {
            let idx = Value::Number((i + 1) as f64); // 1-indexed
            let val = Value::String(vm.heap.alloc_string(key));
            vm.heap.get_table_mut(keys_table)
                .map_err(|e| FerrousError::Internal(e.to_string()))?
                .set(idx, val);
        }
        
        // Create ARGV table
        let argv_table = vm.heap.alloc_table();
        for (i, arg) in args.iter().enumerate() {
            let idx = Value::Number((i + 1) as f64); // 1-indexed
            let val = Value::String(vm.heap.alloc_string(arg));
            vm.heap.get_table_mut(argv_table)
                .map_err(|e| FerrousError::Internal(e.to_string()))?
                .set(idx, val);
        }
        
        // Set in globals
        let globals = vm.globals();
        let keys_name = vm.heap.create_string("KEYS");
        let argv_name = vm.heap.create_string("ARGV");
        
        vm.heap.get_table_mut(globals)
            .map_err(|e| FerrousError::Internal(e.to_string()))?
            .set(Value::String(keys_name), Value::Table(keys_table));
            
        vm.heap.get_table_mut(globals)
            .map_err(|e| FerrousError::Internal(e.to_string()))?
            .set(Value::String(argv_name), Value::Table(argv_table));
        
        // Register Redis API
        self.register_redis_api(vm)?;
        
        Ok(())
    }
    
    /// Register Redis API functions
    fn register_redis_api(&self, vm: &mut LuaVM) -> std::result::Result<(), FerrousError> {
        // Create redis table
        let redis_table = vm.heap.alloc_table();
        
        // Register functions - for now, these are stubs
        // In full implementation, these would call into Redis
        
        // Set redis table in globals
        let globals = vm.globals();
        let redis_name = vm.heap.create_string("redis");
        
        vm.heap.get_table_mut(globals)
            .map_err(|e| FerrousError::Internal(e.to_string()))?
            .set(Value::String(redis_name), Value::Table(redis_table));
        
        Ok(())
    }
    
    /// Convert a Lua value to a Redis response
    fn value_to_resp(&self, value: Value, heap: &LuaHeap) -> std::result::Result<RespFrame, FerrousError> {
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
                let bytes = heap.get_string(s)
                    .map_err(|e| FerrousError::Internal(e.to_string()))?
                    .to_vec();
                Ok(RespFrame::BulkString(Some(Arc::new(bytes))))
            }
            
            Value::Table(t) => {
                // First, collect all the table entries to avoid borrow issues
                let mut table_values = Vec::new();
                {
                    let table = heap.get_table(t)
                        .map_err(|e| FerrousError::Internal(e.to_string()))?;
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
                    let resp_val = self.value_to_resp(val, heap)?;
                    elements.push(resp_val);
                }
                
                Ok(RespFrame::Array(Some(elements)))
            }
            
            _ => {
                // Function, thread, etc. - convert to string representation
                Ok(RespFrame::BulkString(Some(Arc::new(
                    format!("<{}>", value.type_name()).into_bytes()
                ))))
            }
        }
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