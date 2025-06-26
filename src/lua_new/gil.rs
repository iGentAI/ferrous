//! Global Interpreter Lock for Lua script execution
//!
//! This module implements a GIL (Global Interpreter Lock) for Lua script
//! execution, ensuring atomic execution and transaction-like semantics.

use crate::lua_new::error::{LuaError, Result};
use crate::lua_new::executor::CompiledScript;
use crate::lua_new::vm::LuaVM;
use crate::protocol::resp::RespFrame;
use crate::storage::engine::StorageEngine;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Execution information for a running script
#[derive(Clone)]
pub struct ExecutionInfo {
    /// Script SHA1 hash
    pub script_sha: String,
    
    /// Start time
    pub start_time: Instant,
    
    /// Database index
    pub db_index: usize,
    
    /// Kill flag 
    pub kill_flag: Arc<AtomicBool>,
    
    /// Transaction ID
    pub transaction_id: Uuid,
}

/// Scripting execution request
pub struct ExecutionRequest {
    /// Script to execute
    pub script: CompiledScript,
    
    /// Keys for the script
    pub keys: Vec<Vec<u8>>,
    
    /// Arguments for the script
    pub args: Vec<Vec<u8>>,
    
    /// Database index
    pub db: usize,
    
    /// Timeout for execution
    pub timeout: Duration,
    
    /// Response callback
    pub response_cb: Option<Box<dyn FnOnce(std::result::Result<RespFrame, LuaError>) + Send>>,
}

/// Global Interpreter Lock
pub struct LuaGIL {
    /// Mutex protecting script execution
    execution_lock: Arc<Mutex<()>>,
    
    /// Currently executing script info
    current_execution: Arc<RwLock<Option<ExecutionInfo>>>,
    
    /// Script execution queue
    execution_queue: Arc<Mutex<VecDeque<ExecutionRequest>>>,
    
    /// VM instance
    vm_instance: Arc<Mutex<LuaVMInstance>>,
    
    /// Transaction manager
    transaction_manager: Arc<TransactionManager>,
    
    /// Storage engine reference
    storage_engine: Arc<StorageEngine>,
}

impl LuaGIL {
    /// Create a new GIL
    pub fn new(storage: Arc<StorageEngine>) -> Self {
        let vm_instance = LuaVMInstance::new();
        let transaction_manager = TransactionManager::new(storage.clone());
        
        LuaGIL {
            execution_lock: Arc::new(Mutex::new(())),
            current_execution: Arc::new(RwLock::new(None)),
            execution_queue: Arc::new(Mutex::new(VecDeque::new())),
            vm_instance: Arc::new(Mutex::new(vm_instance)),
            transaction_manager: Arc::new(transaction_manager),
            storage_engine: storage,
        }
    }
    
    /// Execute a script with the GIL
    pub fn execute_script(
        &self,
        script: CompiledScript,
        keys: Vec<Vec<u8>>,
        args: Vec<Vec<u8>>,
        db: usize,
        timeout: Duration,
    ) -> std::result::Result<RespFrame, LuaError> {
        // Acquire GIL
        let _gil_guard = self.execution_lock.lock().unwrap();
        
        // Create execution context
        let execution_info = ExecutionInfo {
            script_sha: script.sha1.clone(),
            start_time: Instant::now(),
            db_index: db,
            kill_flag: Arc::new(AtomicBool::new(false)),
            transaction_id: Uuid::new_v4(),
        };
        
        // Set current execution
        {
            let mut current = self.current_execution.write().unwrap();
            *current = Some(execution_info.clone());
        }
        
        // Start transaction
        let transaction = self.transaction_manager.begin_transaction(
            execution_info.transaction_id, 
            db
        )?;
        
        // Execute with timeout protection
        let execution_result = {
            // Use a timeout for execution if provided
            if timeout.as_secs() > 0 {
                // Since we don't have tokio, spawn a thread that will set the kill flag after timeout
                let kill_flag = execution_info.kill_flag.clone();
                let timeout_thread = std::thread::spawn(move || {
                    std::thread::sleep(timeout);
                    kill_flag.store(true, Ordering::Relaxed);
                });
                
                // Execute script
                let result = self.execute_with_context(
                    script,
                    keys,
                    args,
                    execution_info.clone(),
                    transaction.clone(),
                );
                
                // Cleanup timeout thread if completed before timeout
                timeout_thread.join().ok();
                
                // Check if timeout occurred
                if execution_info.kill_flag.load(Ordering::Relaxed) {
                    Err(LuaError::Timeout)
                } else {
                    result
                }
            } else {
                // No timeout, just execute
                self.execute_with_context(
                    script,
                    keys,
                    args,
                    execution_info.clone(),
                    transaction.clone(),
                )
            }
        };
        
        // Clean up execution info
        {
            let mut current = self.current_execution.write().unwrap();
            *current = None;
        }
        
        // Handle result and transaction
        match execution_result {
            Ok(resp) => {
                // Commit transaction
                self.transaction_manager.commit()?;
                Ok(resp)
            }
            Err(e) => {
                // Rollback on script error
                self.transaction_manager.rollback()?;
                Err(e)
            }
        }
    }
    
    /// Execute with context
    fn execute_with_context(
        &self,
        script: CompiledScript,
        keys: Vec<Vec<u8>>,
        args: Vec<Vec<u8>>,
        exec_info: ExecutionInfo,
        transaction: Transaction,
    ) -> std::result::Result<RespFrame, LuaError> {
        let mut vm_guard = self.vm_instance.lock().unwrap();
        
        // Create a new script context
        let context = ScriptContext {
            keys,
            args,
            db: exec_info.db_index,
            transaction_id: exec_info.transaction_id,
            storage_proxy: TransactionalStorageProxy::new(
                &self.transaction_manager,
                transaction,
            ),
        };
        
        // Push script context to stack
        vm_guard.context_stack.push(context);
        
        // Set kill flag
        vm_guard.vm.set_kill_flag(exec_info.kill_flag.clone());
        
        // Execute script
        let result = vm_guard.execute_script(&script.source);
        
        // Pop context (even if error)
        vm_guard.context_stack.pop();
        
        result
    }
    
    /// Kill the currently running script
    pub fn kill_current_script(&self) -> std::result::Result<bool, LuaError> {
        let current = self.current_execution.read().unwrap();
        if let Some(exec_info) = &*current {
            // Set kill flag
            exec_info.kill_flag.store(true, Ordering::Relaxed);
            
            // Force rollback after a grace period - using a separate thread instead of tokio
            let transaction_manager = self.transaction_manager.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(100));
                let _ = transaction_manager.rollback();
            });
            
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// VM instance for script execution
pub struct LuaVMInstance {
    /// The LuaVM
    pub vm: LuaVM,
    
    /// Script context stack
    pub context_stack: Vec<ScriptContext>,
    
    /// Global VM state
    pub global_state: GlobalVMState,
}

impl LuaVMInstance {
    /// Create a new VM instance
    pub fn new() -> Self {
        LuaVMInstance {
            vm: LuaVM::new(crate::lua_new::VMConfig::default()),
            context_stack: Vec::new(),
            global_state: GlobalVMState::default(),
        }
    }
    
    /// Execute a script
    pub fn execute_script(&mut self, source: &str) -> std::result::Result<RespFrame, LuaError> {
        // Get current context
        let context_idx = self.context_stack.len().checked_sub(1).ok_or(LuaError::NoContext)?;
        
        // Shadow borrow the context to avoid borrow checker issues
        let context_clone = {
            let ctx = &self.context_stack[context_idx];
            // We need to make a separate copy of the context to work around borrow checker
            ScriptContext {
                keys: ctx.keys.clone(),
                args: ctx.args.clone(),
                db: ctx.db,
                transaction_id: ctx.transaction_id,
                storage_proxy: TransactionalStorageProxy {
                    transaction_manager: ctx.storage_proxy.transaction_manager.clone(),
                    transaction: ctx.storage_proxy.transaction.clone(),
                    storage: ctx.storage_proxy.storage.clone(),
                },
            }
        };
        
        // Setup transactional environment
        self.setup_transactional_env(&context_clone)?;
        
        // Compile and run
        let result = self.compile_and_run(source)?;
        
        // Convert to RESP
        crate::lua_new::redis_api::RedisApiContext::lua_to_resp(&mut self.vm, result)
            .map_err(|e| LuaError::Runtime(e.to_string()))
    }
    
    /// Setup transactional environment
    fn setup_transactional_env(&mut self, context: &ScriptContext) -> Result<()> {
        // Reset VM to clean state
        self.vm.full_reset();
        
        // Apply sandbox
        crate::lua_new::sandbox::LuaSandbox::redis_compatible().apply(&mut self.vm)?;
        
        // Register cjson library
        crate::lua_new::cjson::register(&mut self.vm)?;
        
        // Setup KEYS and ARGV
        self.setup_script_arrays(&context.keys, &context.args)?;
        
        // Register transactional Redis API
        self.register_transactional_api(context)?;
        
        Ok(())
    }
    
    /// Setup KEYS and ARGV arrays
    fn setup_script_arrays(&mut self, keys: &[Vec<u8>], args: &[Vec<u8>]) -> Result<()> {
        // Create KEYS table
        let keys_table = self.vm.heap.alloc_table();
        
        // Create ARGV table
        let argv_table = self.vm.heap.alloc_table();
        
        // Populate KEYS table
        for (i, key) in keys.iter().enumerate() {
            let idx = crate::lua_new::value::Value::Number((i + 1) as f64);
            let val = crate::lua_new::value::Value::String(self.vm.heap.alloc_string(key));
            
            {
                let table = self.vm.heap.get_table_mut(keys_table)?;
                table.set(idx, val);
            }
        }
        
        // Populate ARGV table
        for (i, arg) in args.iter().enumerate() {
            let idx = crate::lua_new::value::Value::Number((i + 1) as f64);
            let val = crate::lua_new::value::Value::String(self.vm.heap.alloc_string(arg));
            
            {
                let table = self.vm.heap.get_table_mut(argv_table)?;
                table.set(idx, val);
            }
        }
        
        // Set tables in globals
        let globals = self.vm.globals();
        let keys_name = self.vm.heap.create_string("KEYS");
        let argv_name = self.vm.heap.create_string("ARGV");
        
        // Set KEYS global
        {
            let globals_table = self.vm.heap.get_table_mut(globals)?;
            globals_table.set(
                crate::lua_new::value::Value::String(keys_name), 
                crate::lua_new::value::Value::Table(keys_table)
            );
        }
        
        // Set ARGV global
        {
            let globals_table = self.vm.heap.get_table_mut(globals)?;
            globals_table.set(
                crate::lua_new::value::Value::String(argv_name), 
                crate::lua_new::value::Value::Table(argv_table)
            );
        }
        
        Ok(())
    }
    
    /// Register transactional Redis API
    fn register_transactional_api(&mut self, context: &ScriptContext) -> Result<()> {
        // Create redis table
        let redis_table = self.vm.heap.alloc_table();
        
        // Create redis function names
        let call_key = self.vm.heap.create_string("call");
        let pcall_key = self.vm.heap.create_string("pcall");
        // These are unused but kept for completeness
        let _log_key = self.vm.heap.create_string("log");
        let _sha1hex_key = self.vm.heap.create_string("sha1hex");
        let _error_reply_key = self.vm.heap.create_string("error_reply");
        let _status_reply_key = self.vm.heap.create_string("status_reply");
        
        // Register functions
        {
            let table = self.vm.heap.get_table_mut(redis_table)?;
            table.set(
                crate::lua_new::value::Value::String(call_key),
                crate::lua_new::value::Value::CFunction(redis_call_transaction)
            );
        }
        
        {
            let table = self.vm.heap.get_table_mut(redis_table)?;
            table.set(
                crate::lua_new::value::Value::String(pcall_key),
                crate::lua_new::value::Value::CFunction(redis_pcall_transaction)
            );
        }
        
        // Register other Redis functions here
        
        // Store context in VM registry
        let registry = self.vm.registry();
        let context_key = self.vm.heap.create_string("_SCRIPT_CONTEXT");
        
        // Create a handle that will be stored in registry for script context
        let context_ptr = context as *const ScriptContext as usize;
        let context_val = crate::lua_new::value::Value::Number(context_ptr as f64);
        
        {
            let registry_table = self.vm.heap.get_table_mut(registry)?;
            registry_table.set(
                crate::lua_new::value::Value::String(context_key),
                context_val
            );
        }
        
        // Set redis table in globals
        let globals = self.vm.globals();
        let redis_name = self.vm.heap.create_string("redis");
        
        {
            let globals_table = self.vm.heap.get_table_mut(globals)?;
            globals_table.set(
                crate::lua_new::value::Value::String(redis_name),
                crate::lua_new::value::Value::Table(redis_table)
            );
        }
        
        Ok(())
    }
    
    /// Compile and run a script
    fn compile_and_run(&mut self, source: &str) -> Result<crate::lua_new::value::Value> {
        // Parse
        let mut parser = crate::lua_new::parser::Parser::new(source, &mut self.vm.heap)?;
        let ast = parser.parse()?;
        
        // Compile
        let mut compiler = crate::lua_new::compiler::Compiler::new();
        compiler.set_heap(&mut self.vm.heap as *mut _);
        let proto = compiler.compile_chunk(&ast)?;
        
        // Create closure
        let closure = self.vm.heap.alloc_closure(proto, Vec::new());
        
        // Execute
        self.vm.execute_function(closure, &[])
    }
}

/// Global VM state
pub struct GlobalVMState {
    /// Cached scripts
    pub script_cache: HashMap<String, CompiledBytecode>,
    
    /// VM statistics
    pub stats: VMStatistics,
}

impl Default for GlobalVMState {
    fn default() -> Self {
        GlobalVMState {
            script_cache: HashMap::new(),
            stats: VMStatistics::default(),
        }
    }
}

/// VM statistics
#[derive(Default)]
pub struct VMStatistics {
    /// Total scripts executed
    pub total_scripts: u64,
    
    /// Total execution time
    pub total_time_ns: u64,
    
    /// Script timeouts
    pub timeouts: u64,
    
    /// Script kills
    pub kills: u64,
}

/// Compiled bytecode
#[derive(Default)]
pub struct CompiledBytecode {
    /// Bytecode
    pub bytecode: Vec<u8>,
}

/// Transaction manager
pub struct TransactionManager {
    /// Active transaction
    active_transaction: Arc<RwLock<Option<Transaction>>>,
    
    /// Storage engine
    storage_engine: Arc<StorageEngine>,
    
    /// Operation log
    operation_log: Arc<Mutex<Vec<Operation>>>,
}

impl TransactionManager {
    /// Create a new transaction manager
    pub fn new(storage: Arc<StorageEngine>) -> Self {
        TransactionManager {
            active_transaction: Arc::new(RwLock::new(None)),
            storage_engine: storage,
            operation_log: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    /// Begin a transaction
    pub fn begin_transaction(
        &self,
        id: Uuid,
        db: usize,
    ) -> std::result::Result<Transaction, LuaError> {
        let transaction = Transaction {
            id,
            db,
            operations: Vec::new(),
            start_time: Instant::now(),
        };
        
        let mut active = self.active_transaction.write().unwrap();
        *active = Some(transaction.clone());
        
        Ok(transaction)
    }
    
    /// Record an operation
    pub fn record_operation(
        &self,
        op: Operation,
    ) -> std::result::Result<(), LuaError> {
        let mut active = self.active_transaction.write().unwrap();
        if let Some(ref mut transaction) = *active {
            transaction.operations.push(op.clone());
        }
        
        let mut log = self.operation_log.lock().unwrap();
        log.push(op);
        
        Ok(())
    }
    
    /// Commit the transaction
    pub fn commit(&self) -> std::result::Result<(), LuaError> {
        let mut active = self.active_transaction.write().unwrap();
        active.take();
        
        // Clear operation log
        self.operation_log.lock().unwrap().clear();
        
        Ok(())
    }
    
    /// Rollback the transaction
    pub fn rollback(&self) -> std::result::Result<(), LuaError> {
        let mut active = self.active_transaction.write().unwrap();
        if let Some(transaction) = active.take() {
            // Reverse all operations
            for op in transaction.operations.iter().rev() {
                self.reverse_operation(op)?;
            }
        }
        
        // Clear operation log
        self.operation_log.lock().unwrap().clear();
        
        Ok(())
    }
    
    /// Reverse an operation
    fn reverse_operation(&self, op: &Operation) -> std::result::Result<(), LuaError> {
        match op {
            Operation::Set { db, key, old_value, .. } => {
                if let Some(old) = old_value {
                    self.storage_engine.set_string(*db, key.clone(), old.clone()).map_err(|e| LuaError::Runtime(e.to_string()))?;
                } else {
                    self.storage_engine.delete(*db, key).map_err(|e| LuaError::Runtime(e.to_string()))?;
                }
            }
            Operation::Delete { db, key, old_value } => {
                if let Some(old) = old_value {
                    self.storage_engine.set_string(*db, key.clone(), old.clone()).map_err(|e| LuaError::Runtime(e.to_string()))?;
                }
            }
            Operation::Incr { db, key, old_value, .. } => {
                let value = old_value.to_string().into_bytes();
                self.storage_engine.set_string(*db, key.clone(), value).map_err(|e| LuaError::Runtime(e.to_string()))?;
            }
            // ... handle other operations
        }
        Ok(())
    }
}

/// Transaction
#[derive(Clone)]
pub struct Transaction {
    /// Transaction ID
    pub id: Uuid,
    
    /// Database index
    pub db: usize,
    
    /// Operations
    pub operations: Vec<Operation>,
    
    /// Start time
    pub start_time: Instant,
}

/// Operation
#[derive(Clone)]
pub enum Operation {
    /// Set operation
    Set {
        /// Database index
        db: usize,
        
        /// Key
        key: Vec<u8>,
        
        /// Value
        value: Vec<u8>,
        
        /// Old value for rollback
        old_value: Option<Vec<u8>>,
    },
    
    /// Delete operation
    Delete {
        /// Database index
        db: usize,
        
        /// Key
        key: Vec<u8>,
        
        /// Old value for rollback
        old_value: Option<Vec<u8>>,
    },
    
    /// Increment operation
    Incr {
        /// Database index
        db: usize,
        
        /// Key
        key: Vec<u8>,
        
        /// Delta
        delta: i64,
        
        /// Old value for rollback
        old_value: i64,
    },
    
    // Add more operation types as needed
}

/// Script context
pub struct ScriptContext {
    /// KEYS
    pub keys: Vec<Vec<u8>>,
    
    /// ARGV
    pub args: Vec<Vec<u8>>,
    
    /// Database index
    pub db: usize,
    
    /// Transaction ID
    pub transaction_id: Uuid,
    
    /// Storage proxy
    pub storage_proxy: TransactionalStorageProxy,
}

/// Transactional storage proxy
pub struct TransactionalStorageProxy {
    /// Transaction manager
    pub transaction_manager: Arc<TransactionManager>,
    
    /// Transaction
    pub transaction: Transaction,
    
    /// Storage engine
    pub storage: Arc<StorageEngine>,
}

impl TransactionalStorageProxy {
    /// Create a new transactional storage proxy
    pub fn new(
        transaction_manager: &Arc<TransactionManager>,
        transaction: Transaction,
    ) -> Self {
        TransactionalStorageProxy {
            transaction_manager: Arc::clone(transaction_manager),
            transaction,
            storage: Arc::clone(&transaction_manager.storage_engine),
        }
    }
    
    /// Execute a command
    pub fn execute_command(
        &self,
        cmd: &str,
        args: Vec<Vec<u8>>,
    ) -> std::result::Result<RespFrame, LuaError> {
        match cmd.to_uppercase().as_str() {
            "GET" => self.handle_get(&args),
            "SET" => self.handle_set(&args),
            "DEL" => self.handle_del(&args),
            "EXISTS" => self.handle_exists(&args),
            "INCR" => self.handle_incr(&args),
            "PING" => self.handle_ping(),
            // ... other commands
            _ => Err(LuaError::Runtime(format!("Unsupported command: {}", cmd))),
        }
    }
    
    /// Handle PING command
    fn handle_ping(&self) -> std::result::Result<RespFrame, LuaError> {
        Ok(RespFrame::SimpleString(Arc::new(b"PONG".to_vec())))
    }
    
    /// Handle GET command
    fn handle_get(&self, args: &[Vec<u8>]) -> std::result::Result<RespFrame, LuaError> {
        if args.is_empty() {
            return Err(LuaError::Runtime("GET requires a key".to_string()));
        }
        
        let key = &args[0];
        
        match self.storage.get_string(self.transaction.db, key) {
            Ok(Some(val)) => Ok(RespFrame::BulkString(Some(Arc::new(val)))),
            Ok(None) => Ok(RespFrame::Null),
            Err(e) => Err(LuaError::Runtime(e.to_string())),
        }
    }
    
    /// Handle SET command
    fn handle_set(&self, args: &[Vec<u8>]) -> std::result::Result<RespFrame, LuaError> {
        if args.len() < 2 {
            return Err(LuaError::Runtime("SET requires key and value".to_string()));
        }
        
        let key = args[0].clone();
        let value = args[1].clone();
        
        // Get old value for rollback
        let old_value = self.storage.get_string(self.transaction.db, &key).ok().flatten();
        
        // Record operation
        let op = Operation::Set {
            db: self.transaction.db,
            key: key.clone(),
            value: value.clone(),
            old_value,
        };
        
        self.transaction_manager.record_operation(op)?;
        
        // Apply operation
        match self.storage.set_string(self.transaction.db, key, value) {
            Ok(_) => Ok(RespFrame::SimpleString(Arc::new(b"OK".to_vec()))),
            Err(e) => Err(LuaError::Runtime(e.to_string())),
        }
    }
    
    /// Handle DEL command
    fn handle_del(&self, args: &[Vec<u8>]) -> std::result::Result<RespFrame, LuaError> {
        if args.is_empty() {
            return Err(LuaError::Runtime("DEL requires at least one key".to_string()));
        }
        
        let mut deleted = 0;
        
        for key in args {
            // Get old value for rollback
            let old_value = self.storage.get_string(self.transaction.db, key).ok().flatten();
            
            // Record operation
            let op = Operation::Delete {
                db: self.transaction.db,
                key: key.clone(),
                old_value,
            };
            
            self.transaction_manager.record_operation(op)?;
            
            // Apply operation
            if let Ok(true) = self.storage.delete(self.transaction.db, key) {
                deleted += 1;
            }
        }
        
        Ok(RespFrame::Integer(deleted))
    }
    
    /// Handle EXISTS command
    fn handle_exists(&self, args: &[Vec<u8>]) -> std::result::Result<RespFrame, LuaError> {
        if args.is_empty() {
            return Err(LuaError::Runtime("EXISTS requires at least one key".to_string()));
        }
        
        let mut count = 0;
        
        for key in args {
            if let Ok(true) = self.storage.exists(self.transaction.db, key) {
                count += 1;
            }
        }
        
        Ok(RespFrame::Integer(count))
    }
    
    /// Handle INCR command
    fn handle_incr(&self, args: &[Vec<u8>]) -> std::result::Result<RespFrame, LuaError> {
        if args.is_empty() {
            return Err(LuaError::Runtime("INCR requires a key".to_string()));
        }
        
        let key = args[0].clone();
        
        // Get old value for rollback
        let current = match self.storage.get_string(self.transaction.db, &key) {
            Ok(Some(bytes)) => {
                match std::str::from_utf8(&bytes) {
                    Ok(s) => s.parse::<i64>().unwrap_or(0),
                    Err(_) => 0,
                }
            }
            _ => 0,
        };
        
        // Record operation
        let op = Operation::Incr {
            db: self.transaction.db,
            key: key.clone(),
            delta: 1,
            old_value: current,
        };
        
        self.transaction_manager.record_operation(op)?;
        
        // Apply operation
        match self.storage.incr(self.transaction.db, key) {
            Ok(value) => Ok(RespFrame::Integer(value)),
            Err(e) => Err(LuaError::Runtime(e.to_string())),
        }
    }
    
    // Implement other Redis commands as needed
}

/// Redis call function with transaction support
pub fn redis_call_transaction(ctx: &mut crate::lua_new::vm::ExecutionContext) -> Result<i32> {
    redis_call_impl(ctx, false)
}

/// Redis pcall function with transaction support
pub fn redis_pcall_transaction(ctx: &mut crate::lua_new::vm::ExecutionContext) -> Result<i32> {
    redis_call_impl(ctx, true)
}

/// Redis call implementation
fn redis_call_impl(ctx: &mut crate::lua_new::vm::ExecutionContext, is_pcall: bool) -> Result<i32> {
    // Get script context from the registry
    let registry = ctx.vm.registry();
    let context_key = ctx.vm.heap.create_string("_SCRIPT_CONTEXT");
    
    let context_ptr = match ctx.vm.heap.get_table(registry)? {
        table => {
            match table.get(&crate::lua_new::value::Value::String(context_key)) {
                Some(&crate::lua_new::value::Value::Number(n)) => n as usize,
                _ => {
                    let error = LuaError::Runtime("Script context not found".to_string());
                    if is_pcall {
                        return transform_error_to_table(ctx, error);
                    } else {
                        return Err(error);
                    }
                }
            }
        }
    };
    
    // This is unsafe but controlled - we're getting back exactly what we stored
    // and it's guaranteed to be valid for the duration of the script execution
    let context = unsafe { &*(context_ptr as *const ScriptContext) };
    
    // Check arguments
    if ctx.get_arg_count() == 0 {
        let error = LuaError::Runtime("redis.call requires at least one argument".to_string());
        if is_pcall {
            return transform_error_to_table(ctx, error);
        } else {
            return Err(error);
        }
    }
    
    // Get command
    let cmd = match ctx.get_arg(0)? {
        crate::lua_new::value::Value::String(s) => ctx.vm.heap.get_string_utf8(s)?.to_string(),
        _ => {
            let error = LuaError::TypeError("redis.call first argument must be a command name".to_string());
            if is_pcall {
                return transform_error_to_table(ctx, error);
            } else {
                return Err(error);
            }
        }
    };
    
    // Build arguments
    let mut args = Vec::with_capacity(ctx.get_arg_count() - 1);
    for i in 1..ctx.get_arg_count() {
        let arg = ctx.get_arg(i)?;
        match arg {
            crate::lua_new::value::Value::String(s) => {
                args.push(ctx.vm.heap.get_string(s)?.to_vec());
            }
            crate::lua_new::value::Value::Number(n) => {
                args.push(n.to_string().into_bytes());
            }
            crate::lua_new::value::Value::Boolean(b) => {
                args.push(b.to_string().into_bytes());
            }
            crate::lua_new::value::Value::Nil => {
                args.push(b"".to_vec());
            }
            _ => {
                let error = LuaError::TypeError(format!(
                    "Unsupported argument type: {}",
                    arg.type_name()
                ));
                if is_pcall {
                    return transform_error_to_table(ctx, error);
                } else {
                    return Err(error);
                }
            }
        }
    }
    
    // Execute command via storage proxy
    let result = match context.storage_proxy.execute_command(&cmd, args) {
        Ok(resp) => {
            // Convert to Lua value
            let value = crate::lua_new::redis_api::RedisApiContext::resp_to_lua(
                &mut ctx.vm,
                resp
            )?;
            ctx.push_result(value)?;
            Ok(1) // One return value
        }
        Err(e) => {
            if is_pcall {
                transform_error_to_table(ctx, e)
            } else {
                Err(e)
            }
        }
    };
    
    result
}

/// Transform error to table
fn transform_error_to_table(
    ctx: &mut crate::lua_new::vm::ExecutionContext,
    error: LuaError,
) -> Result<i32> {
    // Create error table
    let table = ctx.vm.heap.alloc_table();
    let err_key = ctx.vm.heap.create_string("err");
    let err_msg = ctx.vm.heap.create_string(&error.to_string());
    
    {
        let table_obj = ctx.vm.heap.get_table_mut(table)?;
        table_obj.set(
            crate::lua_new::value::Value::String(err_key),
            crate::lua_new::value::Value::String(err_msg)
        );
    }
    
    // Return the error table
    ctx.push_result(crate::lua_new::value::Value::Table(table))?;
    Ok(1) // One return value
}