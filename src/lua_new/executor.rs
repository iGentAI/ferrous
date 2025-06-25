use crate::lua_new::error::{LuaError, Result};
use crate::lua_new::parser::Parser;
use crate::lua_new::compiler::Compiler;
use crate::lua_new::vm::LuaVM;
use crate::lua_new::redis_api::RedisApiContext;
use crate::lua_new::sandbox::LuaSandbox;
use crate::lua_new::gil::LuaGIL;
use crate::lua_new::value::Value;
use crate::storage::engine::StorageEngine;
use crate::protocol::resp::RespFrame;
use crate::error::FerrousError;
use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use std::collections::HashMap;

/// A compiled Lua script - modified to NOT store handles
#[derive(Clone)]
pub struct CompiledScript {
    /// Original source code
    pub source: String,
    
    /// SHA1 hash of the script
    pub sha1: String,
}

/// Script executor for managing Lua script execution
pub struct ScriptExecutor {
    /// Script cache by SHA1
    cache: Arc<RwLock<HashMap<String, CompiledScript>>>,
    
    /// Global Interpreter Lock
    gil: Arc<LuaGIL>,
    
    /// Storage engine reference
    storage: Arc<StorageEngine>,
    
    /// Execution statistics
    stats: Arc<Mutex<ExecutionStats>>,
    
    /// Configuration
    config: crate::lua_new::VMConfig,
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
    
    /// Script timed out
    Timeout,
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScriptError::NotFound => write!(f, "NOSCRIPT No matching script. Please use EVAL."),
            ScriptError::CompilationFailed(msg) => write!(f, "ERR Error compiling script: {}", msg),
            ScriptError::ExecutionFailed(msg) => write!(f, "ERR {}", msg),
            ScriptError::Killed => write!(f, "ERR Script killed by user with SCRIPT KILL"),
            ScriptError::Timeout => write!(f, "ERR Script execution timeout"),
        }
    }
}

impl std::error::Error for ScriptError {}

impl ScriptExecutor {
    /// Create a new script executor
    pub fn new(storage: Arc<StorageEngine>) -> Self {
        let config = crate::lua_new::VMConfig::default();
        
        ScriptExecutor {
            cache: Arc::new(RwLock::new(HashMap::new())),
            gil: Arc::new(LuaGIL::new(storage.clone())),
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
        println!("[LUA_GIL] Executing EVAL source={} keys={} args={}", 
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
        
        // Execute script using the GIL
        let result = self.gil.execute_script(
            script,
            keys,
            args, 
            db,
            self.config.script_timeout,
        );
        
        match result {
            Ok(resp) => Ok(resp),
            Err(e) => {
                match e {
                    LuaError::Timeout => {
                        Ok(RespFrame::Error(Arc::new(
                            b"ERR Script execution timeout".to_vec()
                        )))
                    }
                    LuaError::ScriptKilled => {
                        Ok(RespFrame::Error(Arc::new(
                            b"ERR Script killed by user with SCRIPT KILL".to_vec()
                        )))
                    }
                    _ => {
                        Ok(RespFrame::Error(Arc::new(
                            format!("ERR {}", e).into_bytes()
                        )))
                    }
                }
            }
        }
    }
    
    /// Execute a script with EVALSHA
    pub fn evalsha(
        &self,
        sha1: &str,
        keys: Vec<Vec<u8>>,
        args: Vec<Vec<u8>>,
        db: usize,
    ) -> std::result::Result<RespFrame, FerrousError> {
        println!("[LUA_GIL] Executing EVALSHA sha1={} keys={} args={}", 
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
        
        // Execute script using the GIL
        let result = self.gil.execute_script(
            script,
            keys,
            args, 
            db,
            self.config.script_timeout,
        );
        
        match result {
            Ok(resp) => Ok(resp),
            Err(e) => {
                match e {
                    LuaError::Timeout => {
                        Ok(RespFrame::Error(Arc::new(
                            b"ERR Script execution timeout".to_vec()
                        )))
                    }
                    LuaError::ScriptKilled => {
                        Ok(RespFrame::Error(Arc::new(
                            b"ERR Script killed by user with SCRIPT KILL".to_vec()
                        )))
                    }
                    _ => {
                        Ok(RespFrame::Error(Arc::new(
                            format!("ERR {}", e).into_bytes()
                        )))
                    }
                }
            }
        }
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
        self.gil.kill_current_script().map(|result| result).unwrap_or(false)
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
        })
    }

}

/// Compute SHA1 hash of a script
fn compute_sha1(script: &str) -> String {
    crate::lua_new::sha1::compute_sha1(script)
}

/// Extension trait for converting standard errors
impl From<ScriptError> for FerrousError {
    fn from(err: ScriptError) -> Self {
        FerrousError::Internal(err.to_string())
    }
}