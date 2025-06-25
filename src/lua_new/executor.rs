//! Script executor for Redis Lua scripts

use crate::lua_new::heap::{LuaHeap, ClosureObject};
use crate::lua_new::value::{Value, ClosureHandle, StringHandle};
use crate::lua_new::vm::{LuaVM, ExecutionContext};
use crate::lua_new::error::{LuaError, Result};
use crate::lua_new::{VMConfig, LuaLimits};
use crate::lua_new::redis_api::RedisApiContext;
use crate::lua_new::parser::Parser;
use crate::lua_new::compiler::Compiler;
use crate::lua_new::sandbox::LuaSandbox;
use crate::storage::engine::StorageEngine;
use crate::protocol::resp::RespFrame;
use crate::error::FerrousError;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// A compiled Lua script - modified to NOT store handles
#[derive(Clone)]
pub struct CompiledScript {
    /// Original source code
    pub source: String,
    
    /// SHA1 hash of the script
    pub sha1: String,
    
    // Remove the closure handle field which was causing cross-VM issues
    // pub closure: ClosureHandle, 
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
        // We no longer need to create a temporary VM just for compilation
        // Just compute the SHA1 and store the source
        
        Ok(CompiledScript {
            source: source.to_string(),
            sha1,
            // No longer storing the closure
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
        
        // Get a VM from the pool
        let mut vm = match self.get_vm() {
            Ok(mut vm) => {
                // Explicitly mark vm as mutable to fix the borrow error
                vm.reset();  // Clean state for reuse
                vm
            },
            Err(e) => return Err(e),
        };
        
        // Set up killable execution
        let kill_flag = Arc::new(AtomicBool::new(false));
        vm.set_kill_flag(Arc::clone(&kill_flag));
        
        // CRITICAL: Set up environment before scripts execute
        match self.setup_environment(&mut vm, &keys, &args, db) {
            Ok(()) => {},
            Err(e) => {
                self.return_vm(vm);
                return Err(e);
            }
        }      
        
        // Track execution
        {
            let mut current = self.current_script.lock().unwrap();
            *current = Some(RunningScript {
                sha1: script.sha1.clone(),
                start_time,
                kill_flag: Arc::clone(&kill_flag),
            });
        }
        
        // Run script
        let result = match self.compile_and_execute(&mut vm, &script.source) {
            Ok(value) => {
                println!("[LUA_NEW] Script executed successfully with value: {:?}", value);
                // Convert to Redis response
                match RedisApiContext::lua_to_resp(&mut vm, value) {
                    Ok(resp) => {
                        println!("[LUA_NEW] Converted Lua result to RESP frame: {:?}", resp);
                        Ok(resp)
                    },
                    Err(e) => {
                        println!("[LUA_ERROR] Error converting Lua value to RESP: {}", e);
                        Ok(RespFrame::Error(Arc::new(format!("ERR {}", e).into_bytes())))
                    }
                }
            }
            Err(e) => {
                // Map error to Redis format
                println!("[LUA_ERROR] Script execution error: {}", e);
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
        
        // Clean up
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
        
        println!("[LUA_NEW] Script execution complete in {:?}", start_time.elapsed());
        result
    }
    
    /// Execute compiled bytecode
    fn compile_and_execute(&self, vm: &mut LuaVM, source: &str) -> Result<Value> {
        let start = Instant::now();
        
        // Parse the script source into an AST
        let mut parser = Parser::new(source, &mut vm.heap)?;
        let ast = parser.parse()?;
        
        // Create a compiler and set heap reference
        let mut compiler = Compiler::new();
        compiler.set_heap(&mut vm.heap as *mut _);
        
        // Compile the AST to a function prototype
        let proto = compiler.compile_chunk(&ast)?;
        
        // Create a closure from the prototype
        let closure = vm.heap.alloc_closure(proto, Vec::new());
        
        // Execute the function
        println!("[LUA_EXECUTOR] Executing script (took {}µs to compile)",
                 start.elapsed().as_micros());
        
        let result = vm.execute_function(closure, &[]);
        
        println!("[LUA_EXECUTOR] Script execution complete (took {}µs total)",
                 start.elapsed().as_micros());
        
        result
    }
    
    /// Get a VM from the pool or create a new one
    fn get_vm(&self) -> std::result::Result<LuaVM, FerrousError> {
        let mut pool = self.vm_pool.lock().unwrap();
        
        if let Some(vm) = pool.pop() {
            // Return the VM without trying to reset it here
            // The reset will happen in execute_script
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
        
        // First, apply sandbox to register ALL standard library functions
        // This MUST happen before any other initialization
        if let Err(e) = LuaSandbox::redis_compatible().apply(vm) {
            println!("[LUA_ERROR] Sandbox application failed: {}", e);
            return Err(FerrousError::Internal(format!("Failed to set up Lua environment: {}", e)));
        }
        
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
        
        // Register cjson library
        if let Err(e) = crate::lua_new::cjson::register(vm) {
            return Err(FerrousError::Internal(e.to_string()));
        }
        
        // Last thing: perform a simple test of the type function to validate environment
        let test_script = "local t = {}; return type(t)";
        let mut parser = Parser::new(test_script, &mut vm.heap).map_err(|e| {
            FerrousError::Internal(format!("Failed to verify environment: {}", e))
        })?;
        let ast = parser.parse().map_err(|e| {
            FerrousError::Internal(format!("Failed to verify environment: {}", e))
        })?;
        let mut compiler = Compiler::new();
        compiler.set_heap(&mut vm.heap as *mut _);
        let proto = compiler.compile_chunk(&ast).map_err(|e| {
            FerrousError::Internal(format!("Failed to verify environment: {}", e))
        })?;
        let closure = vm.heap.alloc_closure(proto, Vec::new());
        match vm.execute_function(closure, &[]) {
            Ok(result) => {
                println!("[LUA_ENV] Type function test result: {:?}", result);
            },
            Err(e) => {
                println!("[LUA_ERROR] Type function verification failed: {}", e);
                return Err(FerrousError::Internal(format!("Type function verification failed: {}", e)));
            }
        };
        
        Ok(())
    }
    
    /// Register standard Lua global functions
    fn register_globals(&self, vm: &mut LuaVM) -> Result<()> {
        let globals = vm.globals();
        
        // Register core functions
        let type_key = vm.heap.create_string("type");
        vm.heap.get_table_mut(globals)?.set(
            Value::String(type_key),
            Value::CFunction(crate::lua_new::sandbox::lua_type)
        );
        
        let print_key = vm.heap.create_string("print");
        vm.heap.get_table_mut(globals)?.set(
            Value::String(print_key),
            Value::CFunction(crate::lua_new::sandbox::lua_print)
        );
        
        Ok(())
    }

}

/// Compute SHA1 hash of a script
fn compute_sha1(script: &str) -> String {
    crate::lua_new::sha1::compute_sha1(script)
}

/// Extension trait for converting Ferrous errors
impl From<ScriptError> for FerrousError {
    fn from(err: ScriptError) -> Self {
        FerrousError::Internal(err.to_string())
    }
}