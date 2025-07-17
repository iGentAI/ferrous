//! Lua Virtual Machine Implementation
//! 
//! This module implements the Lua 5.1 VM using a unified stack architecture
//! with transaction-based safety and non-recursive execution.

use super::codegen::{Instruction, OpCode};
use super::error::{LuaError, LuaResult};
use super::handle::{StringHandle, TableHandle, ClosureHandle, ThreadHandle, UpvalueHandle};
use super::heap::LuaHeap;
use super::metamethod::{MetamethodType, MetamethodContext, MetamethodContinuation};
use super::resource::{ResourceLimits, ResourceTracker, ConcatContext};
use super::transaction::{HeapTransaction, TransactionState};
use super::value::{Value, CallFrame, CFunction, Closure, FunctionProto, UpvalueInfo};
use std::collections::VecDeque;

/// Pending operations for non-recursive VM execution
#[derive(Debug, Clone)]
pub enum PendingOperation {
    /// Function call operation
    FunctionCall {
        /// Function position on stack (absolute index)
        func_index: usize,
        /// Number of arguments
        nargs: usize,
        /// Expected number of results (-1 for multiple)
        expected_results: i32,
    },
    
    /// C function call operation
    CFunctionCall {
        /// C function to call
        function: CFunction,
        /// Base register for the call
        base: u16,
        /// Number of arguments
        nargs: usize,
        /// Expected number of results
        expected_results: i32,
    },
    
    /// Return from function
    Return {
        /// Return values
        values: Vec<Value>,
    },
    
    /// Metamethod call
    MetamethodCall {
        /// Metamethod name
        method: StringHandle,
        /// Target object
        target: Value,
        /// Arguments
        args: Vec<Value>,
        /// Return context
        context: ReturnContext,
    },
    
    /// Resume execution after metamethod
    ResumeAfterMetamethod {
        /// Result of metamethod call
        result: Value,
        /// What to do with result
        context: ReturnContext,
    },
    
    /// Table iteration operation
    TableNext {
        /// Table handle
        table: TableHandle,
        /// Current key
        key: Value,
        /// Where to store results
        base: u16,
        /// Target register offset
        offset: usize,
    },
    
    /// TFORLOOP continuation after iterator function returns
    TForLoopContinuation {
        /// Base register
        base: usize,
        /// A field from instruction
        a: usize,
        /// C field from instruction (result count)
        c: usize,
        /// PC value of the TFORLOOP instruction for loop back
        pc_before_tforloop: usize,
    },
    
    /// Tail call operation
    TailCall {
        /// Function position on stack
        func_index: usize,
        /// Number of arguments
        nargs: usize,
    },
    
    /// Complete a tail call to a C function
    /// This is needed because C functions don't update the PC directly,
    /// so we need to handle their return values specially.
    TailCallReturn,
}

/// Context for tracking where to return results
#[derive(Debug, Clone)]
pub enum ReturnContext {
    /// Normal register storage
    Register {
        /// Base register
        base: u16,
        /// Offset from base
        offset: usize,
    },
    
    /// Store as table field
    TableField {
        /// Table to store in
        table: TableHandle,
        /// Key to use
        key: Value,
    },
    
    /// Conditional jump based on result
    ConditionalJump {
        /// Thread for PC manipulation
        thread: ThreadHandle,
        /// Jump if result matches expected
        expected: bool,
        /// How many instructions to skip
        skip_count: usize,
    },
    
    /// Metamethod-specific context
    Metamethod {
        /// Metamethod context details
        context: MetamethodContext,
    },
    
    /// Discard result
    Discard,
}

/// VM access wrapper for ExecutionContext
pub struct VMAccess {
    /// Reference to heap
    pub heap: *mut LuaHeap,
}

impl VMAccess {
    /// Create a new VMAccess with proper heap reference
    pub fn new(heap: *mut LuaHeap) -> Self {
        VMAccess {
            heap,
        }
    }
    
    /// Get the heap reference, checking for null pointers
    pub fn get_heap(&mut self) -> LuaResult<&mut LuaHeap> {
        if self.heap.is_null() {
            Err(LuaError::InternalError("VM access not properly initialized".to_string()))
        } else {
            // SAFETY: We just checked the pointer is not null, and we trust
            // that the pointer has a valid lifetime through ExecutionContext
            unsafe { Ok(&mut *self.heap) }
        }
    }
}

// Implement Send and Sync for VMAccess to allow it to be used across threads if needed
unsafe impl Send for VMAccess {}
unsafe impl Sync for VMAccess {}

/// Execution context for C functions
pub struct ExecutionContext<'a> {
    /// Transaction for heap access
    tx: &'a mut HeapTransaction<'a>,
    
    /// Current thread
    thread: ThreadHandle,
    
    /// Base register for this C function call
    base: u16,
    
    /// Number of arguments passed
    nargs: usize,
    
    /// Reference to VM for advanced operations
    pub vm_access: VMAccess,
}

impl<'a> ExecutionContext<'a> {
    /// Create a new execution context
    pub fn new(
        tx: &'a mut HeapTransaction<'a>,
        thread: ThreadHandle,
        base: u16,
        nargs: usize,
    ) -> Self {
        ExecutionContext {
            tx,
            thread,
            base,
            nargs,
            vm_access: VMAccess::new(std::ptr::null_mut()),
        }
    }
    
    /// Create a new execution context with VM access
    pub fn new_with_vm(
        tx: &'a mut HeapTransaction<'a>,
        thread: ThreadHandle,
        base: u16,
        nargs: usize,
        vm_access: VMAccess,
    ) -> Self {
        ExecutionContext {
            tx,
            thread,
            base,
            nargs,
            vm_access,
        }
    }
    
    /// Get the number of arguments
    pub fn nargs(&self) -> usize {
        self.nargs
    }
    
    /// Get an argument value (0-indexed) - renamed for stdlib compatibility
    pub fn get_arg(&mut self, index: usize) -> LuaResult<Value> {
        if index >= self.nargs {
            return Err(LuaError::RuntimeError(format!(
                "Argument {} out of range (passed {})",
                index,
                self.nargs
            )));
        }
        
        // Arguments start at base + 1 (base points to the function)
        let register = self.base as usize + 1 + index;
        self.tx.read_register(self.thread, register)
    }
    
    /// Get an argument value (0-indexed)
    pub fn arg(&mut self, index: usize) -> LuaResult<Value> {
        self.get_arg(index)
    }
    
    /// Push a return value (alias for stdlib compatibility)
    pub fn push_result(&mut self, value: Value) -> LuaResult<()> {
        // Results go where the function was (at base), not after arguments
        let register = self.base as usize;
        self.tx.set_register(self.thread, register, value)
    }
    
    /// Push a return value
    pub fn push_return(&mut self, value: Value) -> LuaResult<()> {
        self.push_result(value)
    }
    
    /// Set return value at specific index
    pub fn set_return(&mut self, index: usize, value: Value) -> LuaResult<()> {
        // Results start at the function's position (base)
        let register = self.base as usize + index;
        self.tx.set_register(self.thread, register, value)
    }
    
    /// Access the transaction for complex operations
    pub fn transaction(&mut self) -> &mut HeapTransaction<'a> {
        self.tx
    }
    
    /// Create a string
    pub fn create_string(&mut self, s: &str) -> LuaResult<StringHandle> {
        self.tx.create_string(s)
    }
    
    /// Create a table
    pub fn create_table(&mut self) -> LuaResult<TableHandle> {
        self.tx.create_table()
    }
    
    /// Read a table field
    pub fn get_table_field(&mut self, table: TableHandle, key: &Value) -> LuaResult<Value> {
        self.tx.read_table_field(table, &key)
    }
    
    /// Set a table field
    pub fn set_table_field(&mut self, table: TableHandle, key: Value, value: Value) -> LuaResult<()> {
        self.tx.set_table_field(table, key, value)
    }
    
    /// Get the number of arguments
    pub fn arg_count(&self) -> usize {
        self.nargs
    }
    
    /// Get argument as a string
    pub fn get_arg_str(&mut self, index: usize) -> LuaResult<String> {
        let value = self.get_arg(index)?;
        match value {
            Value::String(handle) => self.get_string_from_handle(handle),
            _ => Err(LuaError::TypeError {
                expected: "string".to_string(),
                got: value.type_name().to_string(),
            }),
        }
    }
    
    /// Get argument as a number
    pub fn get_number_arg(&mut self, index: usize) -> LuaResult<f64> {
        let value = self.get_arg(index)?;
        match value {
            Value::Number(n) => Ok(n),
            _ => Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: value.type_name().to_string(),
            }),
        }
    }
    
    /// Get argument as a boolean
    pub fn get_bool_arg(&mut self, index: usize) -> LuaResult<bool> {
        let value = self.get_arg(index)?;
        match value {
            Value::Boolean(b) => Ok(b),
            _ => Err(LuaError::TypeError {
                expected: "boolean".to_string(),
                got: value.type_name().to_string(),
            }),
        }
    }
    
    /// Get table next key-value pair
    pub fn table_next(&mut self, table: TableHandle, key: Value) -> LuaResult<Option<(Value, Value)>> {
        self.tx.table_next(table, key)
    }
    
    /// Execute a function (for eval/pcall)
    pub fn execute_function(&mut self, closure: ClosureHandle, args: &[Value]) -> LuaResult<Value> {
        // This is a simplified stub - in reality would queue the call
        Err(LuaError::NotImplemented("execute_function".to_string()))
    }
    
    /// Evaluate a Lua script
    pub fn eval_script(&mut self, script: &str) -> LuaResult<Value> {
        // This is a simplified stub - in reality would compile and run script
        Err(LuaError::NotImplemented("eval_script".to_string()))
    }
    
    /// Get string from handle
    pub fn get_string_from_handle(&mut self, handle: StringHandle) -> LuaResult<String> {
        self.tx.get_string_value(handle)
    }
    
    /// Check for metamethod on a value
    pub fn check_metamethod(&mut self, value: &Value, method_name: &str) -> LuaResult<Option<Value>> {
        match value {
            Value::Table(handle) => {
                // Get the metatable if any
                let mt_opt = self.tx.get_table_metatable(*handle)?;
                if let Some(mt) = mt_opt {
                    // Look for the metamethod
                    let method_handle = self.tx.create_string(method_name)?;
                    let method_key = Value::String(method_handle);
                    let method = self.tx.read_table_field(mt, &method_key)?;
                    if method.is_nil() {
                        Ok(None)
                    } else {
                        Ok(Some(method))
                    }
                } else {
                    Ok(None)
                }
            },
            _ => Ok(None), // Non-table values don't have metatables yet
        }
    }
    
    /// Call a metamethod
    pub fn call_metamethod(&mut self, _func: Value, _args: Vec<Value>) -> LuaResult<Vec<Value>> {
        // This would be implemented to call the metamethod
        // For now, return an empty result
        Err(LuaError::NotImplemented("metamethod calls".to_string()))
    }
    
    /// Get table with metamethod support
    pub fn table_get(&mut self, table: TableHandle, key: Value) -> LuaResult<Value> {
        self.tx.get_table_with_metamethods(table, &key)
    }
    
    /// Get table raw (no metamethods)
    pub fn table_raw_get(&mut self, table: TableHandle, key: Value) -> LuaResult<Value> {
        self.tx.read_table_field(table, &key)
    }
    
    /// Set table raw (no metamethods)
    pub fn table_raw_set(&mut self, table: TableHandle, key: Value, value: Value) -> LuaResult<()> {
        self.tx.set_table_field(table, key, value)
    }
    
    /// Get table length
    pub fn table_length(&mut self, table: TableHandle) -> LuaResult<usize> {
        // This is a simplified implementation that only counts the array part
        // A real implementation would follow Lua's length calculation algorithm
        let table_obj = self.tx.get_table(table)?;
        Ok(table_obj.array_len())
    }
    
    /// Set metatable for a table
    pub fn set_metatable(&mut self, table: TableHandle, metatable: Option<TableHandle>) -> LuaResult<()> {
        self.tx.set_table_metatable(table, metatable)
    }
    
    /// Get metatable for a table
    pub fn get_metatable(&mut self, table: TableHandle) -> LuaResult<Option<TableHandle>> {
        self.tx.get_table_metatable(table)
    }
    
    /// Get the current thread
    pub fn get_current_thread(&self) -> LuaResult<ThreadHandle> {
        Ok(self.thread)
    }
    
    /// Get the base register index
    pub fn get_base_index(&self) -> LuaResult<usize> {
        Ok(self.base as usize)
    }
    
    /// Get the number of results pushed
    pub fn get_results_pushed(&self) -> usize {
        // This is a placeholder - a real implementation would track this
        0
    }
    
    /// Get a value from globals
    pub fn globals_get(&mut self, name: &str) -> LuaResult<Value> {
        let name_handle = self.tx.create_string(name)?;
        let globals = self.tx.get_globals_table()?;
        self.tx.read_table_field(globals, &Value::String(name_handle))
    }
}

/// Main Lua VM structure
pub struct LuaVM {
    /// Lua heap
    heap: LuaHeap,
    
    /// Operation queue for non-recursive execution
    operation_queue: VecDeque<PendingOperation>,
    
    /// Main thread handle
    main_thread: ThreadHandle,
    
    /// Currently executing thread
    current_thread: ThreadHandle,
    
    /// VM configuration
    config: VMConfig,
    
    /// Track if execution has completed
    execution_completed: bool,
}

/// VM configuration options
#[derive(Debug, Clone)]
pub struct VMConfig {
    /// Maximum stack size per thread
    pub max_stack_size: usize,
    
    /// Maximum call depth
    pub max_call_depth: usize,
    
    /// Enable debug mode
    pub debug_mode: bool,
    
    /// Resource limits for VM operations
    pub resource_limits: ResourceLimits,
}

impl Default for VMConfig {
    fn default() -> Self {
        VMConfig {
            max_stack_size: 1_000_000,
            max_call_depth: 1000,
            debug_mode: false,
            resource_limits: ResourceLimits::default(),
        }
    }
}

/// Result of a single VM step
#[derive(Debug)]
pub enum StepResult {
    /// Continue execution normally
    Continue,
    
    /// Yield to caller (coroutine yield)
    Yield(Vec<Value>),
    
    /// Execution completed
    Completed(Vec<Value>),
    
    /// Error occurred
    Error(LuaError),
}



impl LuaVM {
    /// Helper to initialize the _G global properly
    fn initialize_globals(&mut self) -> LuaResult<()> {
        let mut tx = HeapTransaction::new(&mut self.heap);
        let globals = tx.get_globals_table()?;
        
        // Create "_G" string
        let g_str = tx.create_string("_G")?;
        let g_key = Value::String(g_str);
        
        // Set _G._G = _G
        tx.set_table_field(globals, g_key.clone(), Value::Table(globals))?;
        
        // Debug: Verify it was set
        let verify = tx.read_table_field(globals, &g_key)?;
        eprintln!("DEBUG: Initial _G._G setup, verification: {:?}", verify);
        
        tx.commit()?;
        
        Ok(())
    }
    
    /// Create a debug transaction for testing and debugging
    pub fn create_debug_transaction(&mut self, max_apply_changes: usize) -> HeapTransaction {
        let debug_config = super::transaction::TransactionDebugConfig {
            max_apply_changes,
            verbose_logging: true,
            log_string_creation: true,
        };
        HeapTransaction::new_with_debug(&mut self.heap, debug_config)
    }
    
    /// Create a new VM instance
    pub fn new() -> LuaResult<Self> {
        Self::with_config(VMConfig::default())
    }
    
    /// Create a new VM with custom configuration
    pub fn with_config(config: VMConfig) -> LuaResult<Self> {
        let mut heap = LuaHeap::new()?;
        let main_thread = heap.main_thread()?;
        
        let mut vm = LuaVM {
            heap,
            operation_queue: VecDeque::new(),
            main_thread,
            current_thread: main_thread,
            config,
            execution_completed: false,
        };
        
        // Ensure globals are properly set up
        vm.initialize_globals()?;
        
        Ok(vm)
    }
    
    /// Execute a closure on the main thread
    pub fn execute(&mut self, closure: ClosureHandle) -> LuaResult<Vec<Value>> {
        // Reset execution state for new execution
        self.execution_completed = false;
        self.operation_queue.clear();
        
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Place the closure at position 0 of the main thread
        tx.set_register(self.main_thread, 0, Value::Closure(closure))?;
        
        // Set up initial call - function at position 0, no arguments
        self.operation_queue.push_back(PendingOperation::FunctionCall {
            func_index: 0,
            nargs: 0,
            expected_results: -1,
        });
        
        tx.commit()?;
        
        // Run until completion
        loop {
            match self.step()? {
                StepResult::Continue => continue,
                StepResult::Completed(values) => {
                    // Ensure clean state on completion
                    self.execution_completed = true;
                    self.operation_queue.clear();
                    return Ok(values);
                },
                StepResult::Error(e) => return Err(e),
                StepResult::Yield(_) => {
                    return Err(LuaError::RuntimeError(
                        "Unexpected yield in main thread".to_string()
                    ));
                }
            }
        }
    }
    
    /// Execute a compiled module
    pub fn execute_module(&mut self, module: &super::compiler::CompiledModule, args: &[Value]) -> LuaResult<Value> {
        // Reset execution state for new execution
        self.execution_completed = false;
        self.operation_queue.clear();
        
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Load module and get function prototype handle
        let proto_handle = super::compiler::loader::load_module(&mut tx, module)?;
        
        // Get the actual FunctionProto from the handle
        let proto = tx.get_function_proto_copy(proto_handle)?;
        
        // Create a closure from the prototype with no upvalues (top-level function)
        let closure = Closure {
            proto,
            upvalues: Vec::new(),  // Top-level functions have no upvalues
        };
        
        // Create the closure in the heap
        let closure_handle = tx.create_closure(closure)?;
        
        // Place the closure at position 0 of the main thread
        tx.set_register(self.main_thread, 0, Value::Closure(closure_handle))?;
        
        // Place arguments starting at position 1
        for (i, arg) in args.iter().enumerate() {
            tx.set_register(self.main_thread, 1 + i, arg.clone())?;
        }
        
        // Queue initial call - function at position 0, with specified arguments
        self.operation_queue.push_back(PendingOperation::FunctionCall { 
            func_index: 0,
            nargs: args.len(),
            expected_results: -1 
        });
        
        tx.commit()?;
        
        // Execute until completion
        loop {
            match self.step()? {
                StepResult::Continue => {
                    // Continue execution
                },
                StepResult::Completed(values) => {
                    // Ensure clean state on completion
                    self.execution_completed = true;
                    self.operation_queue.clear();
                    // Return the first result, or nil if none
                    return Ok(values.get(0).cloned().unwrap_or(Value::Nil));
                },
                StepResult::Error(e) => return Err(e),
                StepResult::Yield(_) => {
                    return Err(LuaError::RuntimeError(
                        "Unexpected yield in main thread".to_string()
                    ));
                }
            }
        }
    }
    
    /// Set the execution context for this VM (used by Redis integration)
    pub fn set_context(&mut self, _context: super::ScriptContext) -> LuaResult<()> {
        // Store the context for use during script execution
        // For now, this is a no-op as we don't use the context yet
        Ok(())
    }
    
    /// Create a transaction with custom debug configuration
    pub fn create_transaction_with_config(
        &mut self, 
        config: super::transaction::TransactionDebugConfig
    ) -> HeapTransaction {
        HeapTransaction::new_with_debug(&mut self.heap, config)
    }
    
    /// Create a transaction configured to catch infinite loops
    pub fn create_loop_detection_transaction(&mut self, max_operations: usize) -> HeapTransaction {
        let config = super::transaction::TransactionDebugConfig {
            max_apply_changes: max_operations,
            verbose_logging: true,
            log_string_creation: true,
        };
        HeapTransaction::new_with_debug(&mut self.heap, config)
    }
    
    /// Create a transaction configured for safe string operations
    pub fn create_string_safe_transaction(&mut self) -> HeapTransaction {
        let config = super::transaction::TransactionDebugConfig {
            // Limit to 1000 operations to catch infinite loops in string operations
            max_apply_changes: 1000,
            verbose_logging: true,
            log_string_creation: true,
        };
        HeapTransaction::new_with_debug(&mut self.heap, config)
    }
    
    /// Evaluate a Lua script string and return the result
    pub fn eval_script(&mut self, script: &str) -> LuaResult<Value> {
        use super::compiler;
        
        // Compile the script
        let module = compiler::compile(script)?;
        
        // Execute the compiled module
        self.execute_module(&module, &[])
    }
    
    /// Initialize the standard library
    pub fn init_stdlib(&mut self) -> LuaResult<()> {
        // Initialize all standard libraries
        crate::lua::stdlib::init_all(self)?;
        
        // Re-initialize globals to ensure _G.G is properly set
        self.initialize_globals()?;
        
        Ok(())
    }
    
    /// Execute a single step of the VM
    pub fn step(&mut self) -> LuaResult<StepResult> {
        // Check if execution has already completed
        if self.execution_completed {
            eprintln!("WARNING: Attempted to step VM after execution completed");
            return Ok(StepResult::Completed(vec![]));
        }
        
        // Process pending operations first
        if let Some(op) = self.operation_queue.pop_front() {
            let result = self.process_pending_operation(op)?;
            
            // Track completion state
            if matches!(result, StepResult::Completed(_)) {
                self.execution_completed = true;
            }
            
            return Ok(result);
        }
        
        // Check if there are any call frames before trying to execute instructions
        {
            let mut tx = HeapTransaction::new(&mut self.heap);
            let depth = tx.get_thread_call_depth(self.current_thread)?;
            
            if depth == 0 {
                // No frames left, execution is complete
                self.execution_completed = true;
                tx.commit()?;
                return Ok(StepResult::Completed(vec![]));
            }
        }
        
        // Store current_thread to avoid borrowing issues
        let current_thread = self.current_thread;
        
        // Create transaction for this step
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Get current execution state
        let frame = tx.get_current_frame(current_thread)?;
        let base = frame.base_register;
        let pc = frame.pc;
        
        // Get instruction
        let instruction = tx.get_instruction(frame.closure, pc)?;
        let inst = Instruction(instruction);
        
        // Debug: Log frame info for FORPREP and FORLOOP
        match inst.get_opcode() {
            OpCode::ForPrep | OpCode::ForLoop => {
                eprintln!("DEBUG step(): Executing {:?} with frame base_register={}, pc={}", 
                         inst.get_opcode(), base, pc);
            },
            _ => {}
        }
        
        // Increment PC for next instruction
        tx.increment_pc(current_thread)?;
        
        // Execute instruction
        let result = match inst.get_opcode() {
            OpCode::Move => Self::op_move(&mut tx, inst, base, current_thread),
            OpCode::LoadK => Self::op_loadk(&mut tx, inst, base, current_thread),
            OpCode::LoadBool => Self::op_loadbool(&mut tx, inst, base, current_thread),
            OpCode::LoadNil => Self::op_loadnil(&mut tx, inst, base, current_thread),
            OpCode::GetGlobal => Self::op_getglobal(&mut tx, inst, base, current_thread),
            OpCode::SetGlobal => Self::op_setglobal(&mut tx, inst, base, current_thread),
            OpCode::GetTable => Self::op_gettable(&mut tx, inst, base, current_thread),
            OpCode::SetTable => Self::op_settable(&mut tx, inst, base, current_thread),
            OpCode::NewTable => Self::op_newtable(&mut tx, inst, base, current_thread),
            OpCode::SelfOp => Self::op_self(&mut tx, inst, base, current_thread),
            OpCode::Add => Self::op_arithmetic(&mut tx, inst, base, ArithOp::Add, current_thread),
            OpCode::Sub => Self::op_arithmetic(&mut tx, inst, base, ArithOp::Sub, current_thread),
            OpCode::Mul => Self::op_arithmetic(&mut tx, inst, base, ArithOp::Mul, current_thread),
            OpCode::Div => Self::op_arithmetic(&mut tx, inst, base, ArithOp::Div, current_thread),
            OpCode::Mod => Self::op_arithmetic(&mut tx, inst, base, ArithOp::Mod, current_thread),
            OpCode::Pow => Self::op_arithmetic(&mut tx, inst, base, ArithOp::Pow, current_thread),
            OpCode::Unm => Self::op_unm(&mut tx, inst, base, current_thread),
            OpCode::Not => Self::op_not(&mut tx, inst, base, current_thread),
            OpCode::Len => Self::op_len(&mut tx, inst, base, current_thread),
            OpCode::Concat => Self::op_concat(&mut tx, inst, base, current_thread),
            OpCode::Jmp => Self::op_jmp(&mut tx, inst, current_thread),
            OpCode::Eq => Self::op_comparison(&mut tx, inst, base, CompOp::Eq, current_thread),
            OpCode::Lt => Self::op_comparison(&mut tx, inst, base, CompOp::Lt, current_thread),
            OpCode::Le => Self::op_comparison(&mut tx, inst, base, CompOp::Le, current_thread),
            OpCode::Test => Self::op_test(&mut tx, inst, base, current_thread),
            OpCode::TestSet => Self::op_testset(&mut tx, inst, base, current_thread),
            OpCode::Call => Self::op_call(&mut tx, inst, base, current_thread),
            OpCode::TailCall => Self::op_tailcall(&mut tx, inst, base, current_thread),
            OpCode::Return => Self::op_return(&mut tx, inst, base, current_thread),
            OpCode::ForPrep => Self::op_forprep(&mut tx, inst, base, current_thread),
            OpCode::ForLoop => {
                eprintln!("DEBUG step(): Executing FORLOOP opcode");
                Self::op_forloop(&mut tx, inst, base, current_thread)
            },
            OpCode::TForLoop => Self::op_tforloop(&mut tx, inst, base, current_thread),
            OpCode::VarArg => Self::op_vararg(&mut tx, inst, base, current_thread),
            OpCode::GetUpval => Self::op_getupval(&mut tx, inst, base, current_thread),
            OpCode::SetUpval => Self::op_setupval(&mut tx, inst, base, current_thread),
            OpCode::Closure => Self::op_closure(&mut tx, inst, base, current_thread),
            OpCode::Close => Self::op_close(&mut tx, inst, base, current_thread),
            OpCode::SetList => Self::op_setlist(&mut tx, inst, base, current_thread),
            _ => Err(LuaError::NotImplemented(format!(
                "Opcode {:?}",
                inst.get_opcode()
            ))),
        };
        
        // Handle result
        match result {
            Ok(_) => {
                // Commit transaction and process any pending operations
                match tx.commit() {
                    Ok(pending) => {
                        for op in pending {
                            self.operation_queue.push_back(op);
                        }
                        
                        // Always continue - let pending operations determine completion
                        Ok(StepResult::Continue)
                    }
                    Err(e) => {
                        // If commit failed due to resource exhaustion or other error, propagate it
                        eprintln!("ERROR: Transaction commit failed: {}", e);
                        Ok(StepResult::Error(e))
                    }
                }
            }
            Err(e) => {
                // Don't commit on error
                Ok(StepResult::Error(e))
            }
        }
    }
    
    /// Process a pending operation
    fn process_pending_operation(&mut self, op: PendingOperation) -> LuaResult<StepResult> {
        match op {
            PendingOperation::FunctionCall { func_index, nargs, expected_results } => {
                self.process_function_call(func_index, nargs, expected_results)
            }
            PendingOperation::CFunctionCall { function, base, nargs, expected_results } => {
                self.process_c_function_call(function, base, nargs, expected_results)
            }
            PendingOperation::Return { values } => {
                self.process_return(values)
            }
            PendingOperation::TForLoopContinuation { base, a, c, pc_before_tforloop } => {
                self.process_tforloop_continuation(base, a, c, pc_before_tforloop)
            }
            PendingOperation::TailCall { func_index, nargs } => {
                self.process_tail_call(func_index, nargs)
            }
            PendingOperation::TailCallReturn => {
                // For tail calls to C functions, we need to propagate the return values
                // We can collect the values from the stack and return them
                let mut tx = HeapTransaction::new(&mut self.heap);
                let frame = tx.get_current_frame(self.current_thread)?;
                let base = frame.base_register as usize;
                
                // Collect all values from the call's base to the stack top
                let mut values = Vec::new();
                let top = tx.get_stack_top(self.current_thread)?;
                
                for i in base..=top {
                    let value = tx.read_register(self.current_thread, i)?;
                    values.push(value);
                }
                
                tx.commit()?;
                
                // Return the values
                self.process_return(values)
            }
            _ => Err(LuaError::NotImplemented(format!(
                "Pending operation: {:?}",
                op
            ))),
        }
    }
    
    /// Process a function call
    fn process_function_call(
        &mut self,
        func_index: usize,
        nargs: usize,
        expected_results: i32,
    ) -> LuaResult<StepResult> {
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        eprintln!("DEBUG: process_function_call - func_index: {}, nargs: {}, expected_results: {}", 
                 func_index, nargs, expected_results);
        
        // Get the function value
        let func_value = tx.read_register(self.current_thread, func_index)?;
        let closure_handle = match func_value {
            Value::Closure(handle) => handle,
            _ => return Err(LuaError::RuntimeError(
                format!("Function call with non-closure value: {}", func_value.type_name())
            )),
        };
        
        // Debug: Log the current register state before setting up the call
        eprintln!("DEBUG: process_function_call - Register state before call setup:");
        for i in 0..=nargs+1 {
            let abs_idx = func_index + i;
            if let Ok(val) = tx.read_register(self.current_thread, abs_idx) {
                eprintln!("  R({}) = {:?}", abs_idx, val);
            }
        }
        
        // Get closure details
        let closure = tx.get_closure(closure_handle)?;
        let num_params = closure.proto.num_params as usize;
        let is_vararg = closure.proto.is_vararg;
        let max_stack = closure.proto.max_stack_size as usize;
        
        // The new base is the first argument position (func_index + 1)
        let new_base = func_index + 1;
        
        eprintln!("DEBUG: process_function_call - Function expects {} params, got {} args", 
                 num_params, nargs);
        eprintln!("DEBUG: process_function_call - New base will be at position {}", new_base);
        
        // Calculate needed stack space
        let needed_size = new_base + max_stack;
        
        // Ensure we have enough stack space
        Self::ensure_stack_size(&mut tx, self.current_thread, needed_size, self.config.max_stack_size)?;
        
        // Detect method call with swapped arguments
        // This happens when SELF sets up R(A+1) = table, but somehow the arguments get swapped
        let needs_method_swap = nargs == 2 && {
            if let (Ok(arg0), Ok(arg1)) = (
                tx.read_register(self.current_thread, func_index + 1),
                tx.read_register(self.current_thread, func_index + 2)
            ) {
                // Check if arguments appear swapped: first is not table, second is table
                // This indicates a method call where self should be first
                !matches!(arg0, Value::Table(_)) && matches!(arg1, Value::Table(_))
            } else {
                false
            }
        };
        
        if needs_method_swap {
            eprintln!("DEBUG: process_function_call - Detected method call with swapped arguments, fixing order");
            // For method calls, swap the arguments to ensure self is first
            let arg0 = tx.read_register(self.current_thread, func_index + 1)?;
            let arg1 = tx.read_register(self.current_thread, func_index + 2)?;
            
            // Clone for debug output before moving
            let arg0_type = arg0.type_name().to_string();
            let arg1_type = arg1.type_name().to_string();
            
            // Write them back in the correct order for the new frame
            // Clone the values since we need to move them
            tx.set_register(self.current_thread, new_base, arg1.clone())?;  // Table (self) goes first  
            tx.set_register(self.current_thread, new_base + 1, arg0.clone())?;  // Other arg goes second
            
            eprintln!("DEBUG: process_function_call - After swap:");
            eprintln!("  R({}) = {} (was second arg)", new_base, arg1_type);
            eprintln!("  R({}) = {} (was first arg)", new_base + 1, arg0_type);
        }
        
        // Debug: Show how arguments map to the new function's registers
        eprintln!("DEBUG: process_function_call - Argument mapping for new frame:");
        for i in 0..nargs.max(num_params) {
            let caller_idx = func_index + 1 + i;
            let callee_idx = i;
            if i < nargs {
                // For method calls that were swapped, read from the new positions
                let val = if needs_method_swap && i < 2 {
                    tx.read_register(self.current_thread, new_base + i)?
                } else {
                    tx.read_register(self.current_thread, caller_idx)?
                };
                eprintln!("  Caller R({}) -> Callee R({}) = {:?}", caller_idx, callee_idx, val);
            } else {
                eprintln!("  Caller R({}) -> Callee R({}) = nil (missing argument)", caller_idx, callee_idx);
            }
        }
        
        // Handle parameter adjustment - Fill missing parameters with nil
        if nargs < num_params {
            eprintln!("DEBUG: process_function_call - Filling {} missing parameters with nil", 
                     num_params - nargs);
            for i in nargs..num_params {
                let fill_pos = new_base + i;
                tx.set_register(self.current_thread, fill_pos, Value::Nil)?;
            }
        }
        
        // Handle varargs collection if needed
        let varargs = if is_vararg && nargs > num_params {
            let mut va = Vec::new();
            eprintln!("DEBUG: process_function_call - Collecting {} varargs", nargs - num_params);
            for i in num_params..nargs {
                let arg = tx.read_register(self.current_thread, new_base + i)?;
                va.push(arg);
            }
            Some(va)
        } else {
            None
        };
        
        // Create new call frame - this establishes the register window
        let frame = CallFrame {
            closure: closure_handle,
            pc: 0,
            base_register: new_base as u16,
            expected_results: if expected_results < 0 {
                None
            } else {
                Some(expected_results as usize)
            },
            varargs,
        };
        
        eprintln!("DEBUG: process_function_call - Creating call frame with base_register: {}", 
                 frame.base_register);
        
        tx.push_call_frame(self.current_thread, frame)?;
        
        // Commit and continue
        let pending = tx.commit()?;
        for op in pending {
            self.operation_queue.push_back(op);
        }
        
        Ok(StepResult::Continue)
    }
    
    /// Process a C function call
    fn process_c_function_call(
        &mut self,
        function: CFunction,
        base: u16,
        nargs: usize,
        expected_results: i32,
    ) -> LuaResult<StepResult> {
        // Take a raw pointer to the heap before creating any mutable borrows
        let heap_ptr = &mut self.heap as *mut LuaHeap;
        
        // Now create the transaction using the heap
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Create execution context without VM access initially
        let mut ctx = ExecutionContext::new(&mut tx, self.current_thread, base, nargs);
        
        // Set up VM access safely using the raw pointer we captured earlier
        // This avoids the overlapping mutable borrow problem
        ctx.vm_access = VMAccess::new(heap_ptr);
        
        // Call the C function
        let actual_results = function(&mut ctx)?;
        
        // Validate result count
        if actual_results < 0 {
            return Err(LuaError::RuntimeError(
                "C function returned negative result count".to_string()
            ));
        }
        
        // Adjust results to expected count
        if expected_results >= 0 {
            let expected = expected_results as usize;
            if actual_results < expected as i32 {
                // Fill missing results with nil
                for i in actual_results as usize..expected {
                    ctx.set_return(i, Value::Nil)?;
                }
            }
        }
        
        // Commit transaction
        let tx = ctx.tx;
        let pending = tx.commit()?;
        for op in pending {
            self.operation_queue.push_back(op);
        }
        
        Ok(StepResult::Continue)
    }
    
    /// Process a return operation
    fn process_return(&mut self, values: Vec<Value>) -> LuaResult<StepResult> {
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Check if there's an active call frame
        let depth = tx.get_thread_call_depth(self.current_thread)?;
        if depth == 0 {
            // No active frames - this is likely returning from the main chunk
            // In this case, we're done with execution
            tx.commit()?;
            // Clear any remaining operations to prevent further execution attempts
            self.operation_queue.clear();
            return Ok(StepResult::Completed(values));
        }
        
        // Get current frame to find where to place results
        let frame = tx.get_current_frame(self.current_thread)?;
        let func_register = frame.base_register.saturating_sub(1);
        let frame_base = frame.base_register as usize;
        
        // Close all upvalues that reference stack positions in this frame
        // This must happen before popping the frame to ensure closures can capture locals
        tx.close_thread_upvalues(self.current_thread, frame_base)?;
        
        // Pop the call frame
        tx.pop_call_frame(self.current_thread)?;
        
        // Check if this was the last frame
        if tx.get_thread_call_depth(self.current_thread)? == 0 {
            // Main function returned
            tx.commit()?;
            // Clear the operation queue to ensure clean termination
            self.operation_queue.clear();
            return Ok(StepResult::Completed(values));
        }
        
        // Get the parent frame to check expected results
        let parent_frame = tx.get_current_frame(self.current_thread)?;
        
        let expected = frame.expected_results;
        
        // Place results starting at function's register
        let result_count = if let Some(n) = expected {
            n.min(values.len())
        } else {
            values.len()
        };
        
        eprintln!("DEBUG: process_return - placing {} results starting at register {}", 
                 result_count, func_register);
        
        for (i, value) in values.iter().take(result_count).enumerate() {
            tx.set_register(self.current_thread, func_register as usize + i, value.clone())?;
        }
        
        // Fill any missing expected results with nil
        if let Some(n) = expected {
            for i in values.len()..n {
                tx.set_register(self.current_thread, func_register as usize + i, Value::Nil)?;
            }
        }
        
        // Commit and continue
        let pending = tx.commit()?;
        for op in pending {
            self.operation_queue.push_back(op);
        }
        
        Ok(StepResult::Continue)
    }
    
    /// Process TFORLOOP continuation after iterator function returns
    /// 
    /// This handles the iterator protocol after the iterator function has returned.
    /// If the first result is not nil, it updates the control variable and jumps
    /// back to continue the loop. If the first result is nil, it skips to the
    /// instruction after the JMP that follows the TFORLOOP.
    fn process_tforloop_continuation(
        &mut self,
        base: usize,
        a: usize,
        c: usize,
        pc_before_tforloop: usize,
    ) -> LuaResult<StepResult> {
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        eprintln!("DEBUG process_tforloop_continuation: base={}, a={}, c={}, pc_before_tforloop={}",
                 base, a, c, pc_before_tforloop);
        
        // First result will be at R(A+3)
        let first_result_idx = base + a + 3;
        
        // Read first result (nil means iterator is done)
        let first_result = tx.read_register(self.current_thread, first_result_idx)?;
        
        eprintln!("DEBUG process_tforloop_continuation: First result: {:?}", first_result);
        
        if !first_result.is_nil() {
            // Continue iteration - copy first result to control variable
            let control_var_idx = base + a + 2;
            eprintln!("DEBUG process_tforloop_continuation: Updating control variable at R({}) = {:?}", 
                     control_var_idx, first_result);
            
            tx.set_register(self.current_thread, control_var_idx, first_result)?;
            
            // Get the current frame to check the current PC
            let frame = tx.get_current_frame(self.current_thread)?;
            
            // In Lua, TFORLOOP is normally followed by a JMP instruction that jumps back
            // to the start of the loop. When the iteration continues, we want the JMP to
            // execute so the loop body runs again.
            
            // We need to keep the current PC as is since execution will naturally
            // continue to the JMP instruction after this operation

            let pending = tx.commit()?;
            for op in pending {
                self.operation_queue.push_back(op);
            }
            
            Ok(StepResult::Continue)
        } else {
            // Iterator is done - TFORLOOP spec says to skip next instruction in this case,
            // which would be the JMP instruction that loops back.
            
            // Get current PC
            let pc = tx.get_pc(self.current_thread)?;
            
            eprintln!("DEBUG process_tforloop_continuation: Iterator done, current PC: {}, skipping to PC {}", 
                     pc, pc + 1);
            
            // Skip the JMP instruction
            tx.set_pc(self.current_thread, pc + 1)?;
            
            let pending = tx.commit()?;
            for op in pending {
                self.operation_queue.push_back(op);
            }
            
            Ok(StepResult::Continue)
        }
    }
    
    /// Process a tail call operation
    /// 
    /// Tail calls are a critical optimization for recursive functions, allowing
    /// them to run without consuming additional stack space. Instead of creating
    /// a new stack frame, we reuse the current one.
    fn process_tail_call(
        &mut self,
        func_index: usize,
        nargs: usize,
    ) -> LuaResult<StepResult> {
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        eprintln!("DEBUG process_tail_call: func_index={}, nargs={}", func_index, nargs);
        
        // Get function and current frame information
        let func_value = tx.read_register(self.current_thread, func_index)?;
        let frame = tx.get_current_frame(self.current_thread)?;
        let frame_base = frame.base_register as usize;
        
        // Save expected_results to maintain across frame reuse
        let expected_results = frame.expected_results;
        
        eprintln!("DEBUG process_tail_call: Function type: {}, current frame base: {}", 
                 func_value.type_name(), frame_base);
        
        // Critically important: close upvalues before we overwrite the stack
        // This ensures any locals captured by upvalues have their values preserved
        tx.close_thread_upvalues(self.current_thread, frame_base)?;
        
        // For tail calls, we need to move the function and arguments to the beginning
        // of the current frame, replacing the current function
        
        // The destination is actually one position before the current base
        // This is where the function being called is stored in Lua's calling convention
        let dest_base = frame_base - 1; 
        
        eprintln!("DEBUG process_tail_call: Moving function and args to position {}", dest_base);
        
        // Move function to its new position
        if dest_base != func_index {
            tx.set_register(self.current_thread, dest_base, func_value.clone())?;
        }
        
        // Move arguments to their new positions
        for i in 0..nargs {
            let src = func_index + 1 + i;  // Argument position
            let dst = dest_base + 1 + i;   // New position (after function)
            
            if src != dst {  // Avoid unnecessary moves
                let arg = tx.read_register(self.current_thread, src)?;
                tx.set_register(self.current_thread, dst, arg)?;
            }
        }
        
        // Handle based on function type
        match func_value {
            Value::Closure(closure_handle) => {
                // Get closure details
                let closure = tx.get_closure(closure_handle)?;
                let num_params = closure.proto.num_params as usize;
                let is_vararg = closure.proto.is_vararg;
                let max_stack = closure.proto.max_stack_size as usize;
                
                eprintln!("DEBUG process_tail_call: Closure expecting {} params, is_vararg={}", 
                         num_params, is_vararg);
                
                // Handle parameter count mismatch
                if nargs < num_params {
                    // Fill missing parameters with nil
                    for i in nargs..num_params {
                        tx.set_register(self.current_thread, dest_base + 1 + i, Value::Nil)?;
                    }
                }
                
                // Handle varargs if needed
                let varargs = if is_vararg && nargs > num_params {
                    // Collect varargs
                    let mut va = Vec::new();
                    for i in num_params..nargs {
                        let arg = tx.read_register(self.current_thread, dest_base + 1 + i)?;
                        va.push(arg);
                    }
                    Some(va)
                } else {
                    None
                };
                
                // Ensure we have enough stack space
                let needed_size = dest_base + 1 + max_stack;
                tx.grow_stack(self.current_thread, needed_size)?;
                
                // Update current frame to call the new function
                // Pop the frame from call_frames
                tx.pop_call_frame(self.current_thread)?;
                
                // Create a new frame for the tail-called function
                let new_frame = CallFrame {
                    closure: closure_handle,
                    pc: 0,  // Start at the beginning of the new function
                    base_register: (dest_base + 1) as u16,  // Base is after function
                    expected_results,  // Preserve expected results from caller
                    varargs,  // Pass collected varargs if any
                };
                
                // Push the new frame
                tx.push_call_frame(self.current_thread, new_frame)?;
                
                eprintln!("DEBUG process_tail_call: Created new frame with base_register={}", 
                         dest_base + 1);
            },
            Value::CFunction(cfunc) => {
                // For C functions, we can't reuse the frame directly
                // Queue a normal C function call and pop the current frame
                
                eprintln!("DEBUG process_tail_call: Handling C function tail call");
                
                // Pop current frame since it won't be reused for C functions
                tx.pop_call_frame(self.current_thread)?;
                
                // Convert expected results to form used by CFunctionCall
                let expected = match expected_results {
                    Some(n) => n as i32,
                    None => -1  // Multiple results
                };
                
                // Queue C function call
                tx.queue_operation(PendingOperation::CFunctionCall {
                    function: cfunc,
                    base: dest_base as u16,
                    nargs,
                    expected_results: expected,
                })?;
                
                // Queue a return operation for after the C function call
                // This is needed because C functions don't update PC directly
                tx.queue_operation(PendingOperation::TailCallReturn)?;
            },
            _ => {
                return Err(LuaError::TypeError {
                    expected: "function".to_string(),
                    got: func_value.type_name().to_string(),
                });
            }
        }
        
        // Commit the transaction
        let pending = tx.commit()?;
        for op in pending {
            self.operation_queue.push_back(op);
        }
        
        Ok(StepResult::Continue)
    }
    
    // Opcode implementations
    
    /// MOVE: R(A) := R(B)
    fn op_move(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        // Ensure we have enough stack space for the destination register
        let needed_size = base as usize + a.max(b) + 1;
        tx.grow_stack(current_thread, needed_size)?;
        
        let value = tx.read_register(current_thread, base as usize + b)?;
        tx.set_register(current_thread, base as usize + a, value)?;
        
        Ok(())
    }
    
    /// LOADK: R(A) := Kst(Bx)
    fn op_loadk(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let bx = inst.get_bx() as usize;
        
        // Ensure we have enough stack space for the destination register
        let needed_size = base as usize + a + 1;
        tx.grow_stack(current_thread, needed_size)?;
        
        // Get constant from current function
        let frame = tx.get_current_frame(current_thread)?;
        let closure = tx.get_closure(frame.closure)?;
        
        if bx >= closure.proto.constants.len() {
            return Err(LuaError::RuntimeError(format!(
                "Constant index {} out of bounds",
                bx
            )));
        }
        
        let constant = closure.proto.constants[bx].clone();
        
        // Debug: Log string constants being loaded
        if let Value::String(s) = &constant {
            let str_val = tx.get_string_value(*s)?;
            eprintln!("DEBUG LOADK: Loading string constant '{}' into register R({})", str_val, base as usize + a);
        }
        
        tx.set_register(current_thread, base as usize + a, constant)?;
        
        Ok(())
    }
    
    /// LOADBOOL: R(A) := (Bool)B; if (C) pc++
    fn op_loadbool(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b();
        let c = inst.get_c();
        
        let value = Value::Boolean(b != 0);
        tx.set_register(current_thread, base as usize + a, value)?;
        
        if c != 0 {
            // Skip next instruction
            let pc = tx.get_pc(current_thread)?;
            tx.set_pc(current_thread, pc + 1)?;
        }
        
        Ok(())
    }
    
    /// LOADNIL: R(A) := ... := R(B) := nil
    fn op_loadnil(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        for i in a..=b {
            tx.set_register(current_thread, base as usize + i, Value::Nil)?;
        }
        
        Ok(())
    }
    
    /// GETGLOBAL: R(A) := Gbl[Kst(Bx)]
    fn op_getglobal(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let bx = inst.get_bx() as usize;
        
        // Get constant name - extract it completely before next borrow
        let key = {
            let frame = tx.get_current_frame(current_thread)?;
            let closure = tx.get_closure(frame.closure)?;
            
            if bx >= closure.proto.constants.len() {
                return Err(LuaError::RuntimeError(format!(
                    "Constant index {} out of bounds (max: {})",
                    bx, closure.proto.constants.len() - 1
                )));
            }
            
            // Clone the key to avoid holding a reference into closure
            closure.proto.constants[bx].clone()
        }; // Drop the borrow of tx here
        
        // Debug: Log what we're looking up
        let global_name = if let Value::String(s) = &key {
            let key_str = tx.get_string_value(*s)?;
            eprintln!("DEBUG GETGLOBAL: Looking up global '{}'", key_str);
            
            // Special handling for "_G" lookup
            if key_str == "_G" {
                eprintln!("DEBUG GETGLOBAL: Special handling for _G lookup");
                let globals = tx.get_globals_table()?;
                tx.set_register(current_thread, base as usize + a, Value::Table(globals))?;
                return Ok(());
            }
            
            Some(key_str)
        } else {
            eprintln!("DEBUG GETGLOBAL: Non-string key type: {}", key.type_name());
            None
        };
        
        // Get globals table
        let globals = tx.get_globals_table()?;
        eprintln!("DEBUG GETGLOBAL: Got globals table handle: {:?}", globals);
        
        // Get value from globals
        let value = tx.get_table_with_metamethods(globals, &key)?;
        eprintln!("DEBUG GETGLOBAL: Result: {:?}", value);
        
        // If the global doesn't exist (nil), provide a helpful error context
        if value.is_nil() {
            if let Some(name) = global_name {
                eprintln!("WARNING GETGLOBAL: Global '{}' is nil/undefined", name);
                // Don't error out - just set nil and continue
                // This matches Lua's behavior
            }
        }
        
        tx.set_register(current_thread, base as usize + a, value)?;
        
        Ok(())
    }
    
    /// SETGLOBAL: Gbl[Kst(Bx)] := R(A)
    fn op_setglobal(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let bx = inst.get_bx() as usize;
        
        // Get value to set
        let value = tx.read_register(current_thread, base as usize + a)?;
        
        // Get constant name
        let frame = tx.get_current_frame(current_thread)?;
        let closure = tx.get_closure(frame.closure)?;
        
        if bx >= closure.proto.constants.len() {
            return Err(LuaError::RuntimeError(format!(
                "Constant index {} out of bounds",
                bx
            )));
        }
        
        let key = closure.proto.constants[bx].clone();
        
        // Get globals table
        let globals = tx.get_globals_table()?;
        
        // Set value in globals
        tx.set_table_field(globals, key, value)?;
        
        Ok(())
    }
    
    /// GETTABLE: R(A) := R(B)[RK(C)]
    fn op_gettable(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        let (c_is_const, c_idx) = inst.get_rk_c();
        
        // Get table
        let table_val = tx.read_register(current_thread, base as usize + b)?;
        
        // Get key - make sure to fully extract it if it's a constant
        let key = if c_is_const {
            // Extract constant completely to avoid holding tx borrow
            let frame = tx.get_current_frame(current_thread)?;
            let closure = tx.get_closure(frame.closure)?;
            
            if c_idx as usize >= closure.proto.constants.len() {
                return Err(LuaError::RuntimeError(format!(
                    "Constant index {} out of bounds",
                    c_idx
                )));
            }
            
            closure.proto.constants[c_idx as usize].clone()
        } else {
            tx.read_register(current_thread, base as usize + c_idx as usize)?
        };
        
        // Debug logging for table access
        if let Value::String(k) = &key {
            let key_str = tx.get_string_value(*k)?;
            eprintln!("DEBUG GETTABLE: Accessing table[{}], table value: {:?}", key_str, table_val);
        }
        
        // Handle table access
        match table_val {
            Value::Table(handle) => {
                let value = tx.get_table_with_metamethods(handle, &key)?;
                tx.set_register(current_thread, base as usize + a, value)?;
            }
            _ => {
                return Err(LuaError::TypeError {
                    expected: "table".to_string(),
                    got: table_val.type_name().to_string(),
                });
            }
        }
        
        Ok(())
    }
    
    /// SETTABLE: R(A)[RK(B)] := RK(C)
    fn op_settable(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let (b_is_const, b_idx) = inst.get_rk_b();
        let (c_is_const, c_idx) = inst.get_rk_c();
        
        // Get table
        let table_val = tx.read_register(current_thread, base as usize + a)?;
        
        // Get key
        let key = if b_is_const {
            Self::get_constant(tx, b_idx as usize, current_thread)?
        } else {
            tx.read_register(current_thread, base as usize + b_idx as usize)?
        };
        
        // Get value
        let value = if c_is_const {
            Self::get_constant(tx, c_idx as usize, current_thread)?
        } else {
            tx.read_register(current_thread, base as usize + c_idx as usize)?
        };
        
        // Handle table assignment
        match table_val {
            Value::Table(handle) => {
                tx.set_table_field(handle, key, value)?;
            }
            _ => {
                return Err(LuaError::TypeError {
                    expected: "table".to_string(),
                    got: table_val.type_name().to_string(),
                });
            }
        }
        
        Ok(())
    }
    
    /// NEWTABLE: R(A) := {} (size = B,C)
    fn op_newtable(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let _b = inst.get_b(); // Array size hint (unused for now)
        let _c = inst.get_c(); // Hash size hint (unused for now)
        
        // Create new table
        let table = tx.create_table()?;
        tx.set_register(current_thread, base as usize + a, Value::Table(table))?;
        
        Ok(())
    }
    
    /// SELF: R(A+1) := R(B); R(A) := R(B)[RK(C)]
    fn op_self(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        let (c_is_const, c_idx) = inst.get_rk_c();
        
        eprintln!("DEBUG SELF: A={}, B={}, C(const={}, idx={}), base={}", 
                 a, b, c_is_const, c_idx, base);
        
        // Ensure we have enough stack space for R(A) and R(A+1)
        let needed_size = base as usize + a + 2; // +2 to cover both R(A) and R(A+1)
        tx.grow_stack(current_thread, needed_size)?;
        
        // Get the table from R(B)
        let table_val = tx.read_register(current_thread, base as usize + b)?;
        eprintln!("DEBUG SELF: Table from R({}) = {:?}", base as usize + b, table_val);
        
        // Get the method name key - either from constant or register
        let key = if c_is_const {
            Self::get_constant(tx, c_idx as usize, current_thread)?
        } else {
            tx.read_register(current_thread, base as usize + c_idx as usize)?
        };
        
        // Debug logging
        if let Value::String(k) = &key {
            let key_str = tx.get_string_value(*k)?;
            eprintln!("DEBUG SELF: Looking up method '{}' on table {:?}", key_str, table_val);
        }
        
        // Verify we have a table
        match &table_val {
            Value::Table(handle) => {
                eprintln!("DEBUG SELF: Table handle: {:?}", handle);
                
                // Look up the method on the table
                let method = tx.get_table_with_metamethods(*handle, &key)?;
                eprintln!("DEBUG SELF: Found method: {:?}", method);
                
                // SELF operation: R(A+1) := R(B); R(A) := R(B)[RK(C)]
                // This sets up for method calls where:
                // - R(A) contains the method function
                // - R(A+1) contains the table (self parameter)
                tx.set_register(current_thread, base as usize + a, method)?;
                tx.set_register(current_thread, base as usize + a + 1, table_val)?;
                
                eprintln!("DEBUG SELF: Set R({}) = method, R({}) = table (self)", 
                         base as usize + a, base as usize + a + 1);
                
                // Extra debug: verify what we just set
                let verify_method = tx.read_register(current_thread, base as usize + a)?;
                let verify_self = tx.read_register(current_thread, base as usize + a + 1)?;
                eprintln!("DEBUG SELF: Verification - R({}) = {:?}, R({}) = {:?}",
                         base as usize + a, verify_method, base as usize + a + 1, verify_self);
            }
            _ => {
                return Err(LuaError::TypeError {
                    expected: "table".to_string(),
                    got: table_val.type_name().to_string(),
                });
            }
        }
        
        Ok(())
    }
    
    /// Arithmetic operations
    fn op_arithmetic(
        tx: &mut HeapTransaction,
        inst: Instruction,
        base: u16,
        op: ArithOp,
        current_thread: ThreadHandle,
    ) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let (b_is_const, b_idx) = inst.get_rk_b();
        let (c_is_const, c_idx) = inst.get_rk_c();
        
        // Get operands
        let left = if b_is_const {
            Self::get_constant(tx, b_idx as usize, current_thread)?
        } else {
            tx.read_register(current_thread, base as usize + b_idx as usize)?
        };
        
        let right = if c_is_const {
            Self::get_constant(tx, c_idx as usize, current_thread)?
        } else {
            tx.read_register(current_thread, base as usize + c_idx as usize)?
        };
        
        // Get type names before the match (to avoid move issues)
        let left_type = left.type_name().to_string();
        let right_type = right.type_name().to_string();
        
        // Try to perform arithmetic
        let result = match (left, right, op) {
            (Value::Number(l), Value::Number(r), ArithOp::Add) => Value::Number(l + r),
            (Value::Number(l), Value::Number(r), ArithOp::Sub) => Value::Number(l - r),
            (Value::Number(l), Value::Number(r), ArithOp::Mul) => Value::Number(l * r),
            (Value::Number(l), Value::Number(r), ArithOp::Div) => {
                if r == 0.0 {
                    return Err(LuaError::RuntimeError("Division by zero".to_string()));
                }
                Value::Number(l / r)
            }
            (Value::Number(l), Value::Number(r), ArithOp::Mod) => {
                if r == 0.0 {
                    return Err(LuaError::RuntimeError("Modulo by zero".to_string()));
                }
                Value::Number(l % r)
            }
            (Value::Number(l), Value::Number(r), ArithOp::Pow) => Value::Number(l.powf(r)),
            _ => {
                return Err(LuaError::TypeError {
                    expected: "number".to_string(),
                    got: format!("{} and {}", left_type, right_type),
                });
            }
        };
        
        tx.set_register(current_thread, base as usize + a, result)?;
        
        Ok(())
    }
    
    /// UNM: R(A) := -R(B)
    fn op_unm(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        let value = tx.read_register(current_thread, base as usize + b)?;
        
        let result = match value {
            Value::Number(n) => Value::Number(-n),
            _ => {
                return Err(LuaError::TypeError {
                    expected: "number".to_string(),
                    got: value.type_name().to_string(),
                });
            }
        };
        
        tx.set_register(current_thread, base as usize + a, result)?;
        
        Ok(())
    }
    
    /// NOT: R(A) := not R(B)
    fn op_not(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        let value = tx.read_register(current_thread, base as usize + b)?;
        let result = Value::Boolean(value.is_falsey());
        
        tx.set_register(current_thread, base as usize + a, result)?;
        
        Ok(())
    }
    
    /// LEN: R(A) := length of R(B)
    fn op_len(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        let value = tx.read_register(current_thread, base as usize + b)?;
        
        let length = match value {
            Value::String(handle) => {
                let s = tx.get_string_bytes(handle)?;
                Value::Number(s.len() as f64)
            }
            Value::Table(handle) => {
                // For tables, count array part
                let table = tx.get_table(handle)?;
                let mut len = 0;
                for i in 1.. {
                    if let Some(v) = table.get_array(i) {
                        if !v.is_nil() {
                            len = i;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                Value::Number(len as f64)
            }
            _ => {
                return Err(LuaError::TypeError {
                    expected: "string or table".to_string(),
                    got: value.type_name().to_string(),
                });
            }
        };
        
        tx.set_register(current_thread, base as usize + a, length)?;
        
        Ok(())
    }
    
    /// CONCAT: R(A) := R(B).. ... ..R(C)
    fn op_concat(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        let c = inst.get_c() as usize;
        
        eprintln!("DEBUG CONCAT: Starting concatenation R({}) := R({})..R({})", a, b, c);
        eprintln!("DEBUG CONCAT: Base register: {}, Absolute registers: {}..{}", 
                 base, base as usize + b, base as usize + c);
        
        // Track this as a concatenation operation to prevent infinite loops
        tx.resource_tracker().track_operation()?;
        
        // Create a concat context to collect parts
        let mut concat_ctx = ConcatContext::new();
        
        // Collect all values to concatenate
        for i in b..=c {
            let abs_register = base as usize + i;
            let value = tx.read_register(current_thread, abs_register)?;
            eprintln!("DEBUG CONCAT: Reading R({}) (absolute: {}) = {:?}", 
                     i, abs_register, value.type_name());
            
            match value {
                Value::String(handle) => {
                    let s = tx.get_string_value(handle)?;
                    let len = s.len();
                    eprintln!("DEBUG CONCAT: String part: '{}' (len: {})", 
                             if len < 50 { &s } else { "<long string>" }, len);
                    
                    // Track memory for this string part
                    tx.resource_tracker().track_string_allocation(len)?;
                    
                    concat_ctx.add_part(s);
                }
                Value::Number(n) => {
                    let s = n.to_string();
                    let len = s.len();
                    eprintln!("DEBUG CONCAT: Number part: {} (len: {})", n, len);
                    
                    // Track memory for number strings too
                    tx.resource_tracker().track_string_allocation(len)?;
                    
                    concat_ctx.add_part(s);
                }
                _ => {
                    eprintln!("DEBUG CONCAT: Type error - got {} at R({})", 
                             value.type_name(), abs_register);
                    return Err(LuaError::TypeError {
                        expected: "string or number".to_string(),
                        got: value.type_name().to_string(),
                    });
                }
            }
        }
        
        let total_length = concat_ctx.total_length;
        eprintln!("DEBUG CONCAT: Total parts: {}, Total length: {}", concat_ctx.parts.len(), total_length);
        
        // Check if total length would exceed string memory limits
        // This prevents runaway string growth (not circular references in data)
        let limits = tx.resource_tracker().limits();
        if total_length > limits.max_string_memory {
            return Err(LuaError::ResourceLimit {
                resource: "string size".to_string(),
                limit: limits.max_string_memory,
                used: total_length,
                context: "String concatenation would create a string larger than the memory limit. This may indicate runaway string growth.".to_string(),
            });
        }
        
        // Concatenate all parts
        let result = concat_ctx.finish();
        eprintln!("DEBUG CONCAT: Concatenated result: '{}' (actual len: {})", 
                 if result.len() < 50 { &result } else { "<long string>" }, 
                 result.len());
        
        // Create the string in the heap - this will also track the allocation
        eprintln!("DEBUG CONCAT: Creating string in heap");
        let handle = tx.create_string(&result)?;
        eprintln!("DEBUG CONCAT: Created string handle: {:?}", handle);
        
        let target_register = base as usize + a;
        tx.set_register(current_thread, target_register, Value::String(handle))?;
        eprintln!("DEBUG CONCAT: Set result in R({}) (absolute: {})", a, target_register);
        eprintln!("DEBUG CONCAT: Operation complete");
        
        Ok(())
    }
    
    /// JMP: pc+=sBx
    fn op_jmp(tx: &mut HeapTransaction, inst: Instruction, current_thread: ThreadHandle) -> LuaResult<()> {
        let sbx = inst.get_sbx();
        
        let pc = tx.get_pc(current_thread)?;
        let new_pc = (pc as i32 + sbx) as usize;
        tx.set_pc(current_thread, new_pc)?;
        
        Ok(())
    }
    
    /// Comparison operations
    fn op_comparison(
        tx: &mut HeapTransaction,
        inst: Instruction,
        base: u16,
        op: CompOp,
        current_thread: ThreadHandle,
    ) -> LuaResult<()> {
        let a = inst.get_a();
        let b = inst.get_b() as usize;
        let c = inst.get_c() as usize;
        
        let left = tx.read_register(current_thread, base as usize + b)?;
        let right = tx.read_register(current_thread, base as usize + c)?;
        
        let result = match op {
            CompOp::Eq => Self::compare_eq(&left, &right),
            CompOp::Lt => Self::compare_lt(&left, &right)?,
            CompOp::Le => Self::compare_le(&left, &right)?,
        };
        
        // If result doesn't match A, skip next instruction
        if result != (a != 0) {
            let pc = tx.get_pc(current_thread)?;
            tx.set_pc(current_thread, pc + 1)?;
        }
        
        Ok(())
    }
    
    /// TEST: if not (R(A) <=> C) then pc++
    fn op_test(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let c = inst.get_c();
        
        let value = tx.read_register(current_thread, base as usize + a)?;
        let test = !value.is_falsey();
        
        if test != (c != 0) {
            let pc = tx.get_pc(current_thread)?;
            tx.set_pc(current_thread, pc + 1)?;
        }
        
        Ok(())
    }
    
    /// TESTSET: if (R(B) <=> C) then R(A) := R(B) else pc++
    fn op_testset(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        let c = inst.get_c();
        
        let value = tx.read_register(current_thread, base as usize + b)?;
        let test = !value.is_falsey();
        
        if test == (c != 0) {
            tx.set_register(current_thread, base as usize + a, value)?;
        } else {
            let pc = tx.get_pc(current_thread)?;
            tx.set_pc(current_thread, pc + 1)?;
        }
        
        Ok(())
    }
    
    /// CALL: R(A), ... ,R(A+C-2) := R(A)(R(A+1), ... ,R(A+B-1))
    fn op_call(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b();
        let c = inst.get_c();
        
        eprintln!("DEBUG: op_call - A: {}, B: {}, C: {}, base: {}", a, b, c, base);
        
        // Get function
        let func_abs = base as usize + a;
        eprintln!("DEBUG: op_call - Reading function from absolute position: {}", func_abs);
        let func = tx.read_register(current_thread, func_abs)?;
        eprintln!("DEBUG: op_call - Function type: {:?}", func.type_name());
        
        // Determine argument count
        let nargs = if b == 0 {
            // Use all values up to stack top
            let stack_top = tx.get_stack_top(current_thread)?;
            eprintln!("DEBUG: op_call - Using all values, stack_top: {}, func_abs: {}", stack_top, func_abs);
            stack_top - func_abs - 1  // Subtract 1 for the function itself
        } else {
            (b - 1) as usize
        };
        
        eprintln!("DEBUG: op_call - Number of arguments: {}", nargs);
        
        // For method calls, we need to detect if a SELF instruction preceded this CALL
        // In that case, the self parameter is already in R(A+1) and should be preserved
        // This is a bit tricky since we don't have direct access to previous instructions
        // However, we can check if this looks like a method call pattern
        let is_method_call = nargs >= 1 && {
            // Check if R(A+1) contains a table that might be the self parameter
            // This is a heuristic but should work for typical method call patterns
            if let Ok(first_arg) = tx.read_register(current_thread, func_abs + 1) {
                matches!(first_arg, Value::Table(_))
            } else {
                false
            }
        };
        
        eprintln!("DEBUG: op_call - Detected method call pattern: {}", is_method_call);
        
        // Log argument positions AND VALUES for debugging
        for i in 0..nargs {
            let arg_pos = func_abs + 1 + i;
            let arg_value = tx.read_register(current_thread, arg_pos)?;
            eprintln!("DEBUG: op_call - Argument {} at position {} = {:?}", i, arg_pos, arg_value);
            
            // Special logging for table values to help debug method calls
            if let Value::Table(handle) = &arg_value {
                eprintln!("DEBUG: op_call - Argument {} is table with handle {:?}", i, handle);
                // Try to get some identifying information about the table
                let table = tx.get_table(*handle)?;
                eprintln!("DEBUG: op_call - Table has {} array elements and {} map entries", 
                         table.array_len(), table.map_len());
            }
        }
        
        // Determine expected results
        let expected_results = if c == 0 {
            -1  // Multiple results
        } else {
            (c - 1) as i32
        };
        
        eprintln!("DEBUG: op_call - Expected results: {}", expected_results);
        
        // Queue the call based on function type
        match func {
            Value::Closure(_) => {
                eprintln!("DEBUG: op_call - Queueing Lua function call with func_index: {}", func_abs);
                tx.queue_operation(PendingOperation::FunctionCall {
                    func_index: func_abs,
                    nargs,
                    expected_results,
                })?;
            }
            Value::CFunction(cfunc) => {
                eprintln!("DEBUG: op_call - Queueing C function call");
                tx.queue_operation(PendingOperation::CFunctionCall {
                    function: cfunc,
                    base: func_abs as u16,
                    nargs,
                    expected_results,
                })?;
            }
            _ => {
                eprintln!("ERROR: op_call - Attempt to call non-function value: {}", func.type_name());
                return Err(LuaError::TypeError {
                    expected: "function".to_string(),
                    got: func.type_name().to_string(),
                });
            }
        }
        
        Ok(())
    }
    
    /// TAILCALL: return R(A)(R(A+1), ..., R(A+B-1))
    /// 
    /// This performs a tail call - a function call that reuses the current stack frame.
    /// Tail calls are a critical optimization in Lua that enables proper recursion without stack growth.
    fn op_tailcall(
        tx: &mut HeapTransaction,
        inst: Instruction,
        base: u16,
        current_thread: ThreadHandle
    ) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b();
        
        eprintln!("DEBUG TAILCALL: A={}, B={}, base={}", a, b, base);
        
        // Get function from register
        let func_abs = base as usize + a;
        let func = tx.read_register(current_thread, func_abs)?;
        
        eprintln!("DEBUG TAILCALL: Function at register R({}) = {:?}", func_abs, func);
        
        // Determine number of arguments (same logic as CALL)
        let nargs = if b == 0 {
            // Use all values up to stack top
            let stack_top = tx.get_stack_top(current_thread)?;
            eprintln!("DEBUG TAILCALL: Using all values, stack_top: {}, func_abs: {}", stack_top, func_abs);
            stack_top - func_abs - 1  // Subtract function position
        } else {
            (b - 1) as usize  // B includes the function itself, so subtract 1
        };
        
        eprintln!("DEBUG TAILCALL: Number of arguments: {}", nargs);
        
        // Log each argument for debugging
        for i in 0..nargs {
            let arg_pos = func_abs + 1 + i;
            if let Ok(arg) = tx.read_register(current_thread, arg_pos) {
                eprintln!("DEBUG TAILCALL: Argument {} at position {} = {:?}", i, arg_pos, arg);
            }
        }
        
        // Queue the tail call (will be handled by process_tail_call)
        tx.queue_operation(PendingOperation::TailCall {
            func_index: func_abs,
            nargs,
        })?;
        
        Ok(())
    }
    
    /// RETURN: return R(A), ... ,R(A+B-2)
    fn op_return(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b();
        
        // Collect return values
        let mut values = Vec::new();
        
        if b == 0 {
            // Return all values from R(A) to stack top
            let stack_top = tx.get_stack_top(current_thread)?;
            for i in a..(stack_top - base as usize + 1) {
                values.push(tx.read_register(current_thread, base as usize + i)?);
            }
        } else {
            // Return specific number of values
            for i in 0..(b - 1) as usize {
                values.push(tx.read_register(current_thread, base as usize + a + i)?);
            }
        }
        
        // Queue return operation
        tx.queue_operation(PendingOperation::Return { values })?;
        
        Ok(())
    }
    
    /// GETUPVAL: R(A) := UpValue[B]
    fn op_getupval(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        eprintln!("DEBUG GETUPVAL: A={}, B={}, base={}", a, b, base);
        
        // Get current closure
        let frame = tx.get_current_frame(current_thread)?;
        let closure = tx.get_closure(frame.closure)?;
        
        eprintln!("DEBUG GETUPVAL: Closure has {} upvalues", closure.upvalues.len());
        
        if b >= closure.upvalues.len() {
            return Err(LuaError::RuntimeError(format!(
                "Upvalue index {} out of bounds (closure has {} upvalues)",
                b, closure.upvalues.len()
            )));
        }
        
        let upvalue_handle = closure.upvalues[b];
        eprintln!("DEBUG GETUPVAL: Reading upvalue handle {:?}", upvalue_handle);
        
        // Validate the upvalue handle
        match tx.validate_handle(&upvalue_handle) {
            Ok(_) => {},
            Err(e) => {
                eprintln!("ERROR GETUPVAL: Invalid upvalue handle: {}", e);
                return Err(LuaError::RuntimeError(format!(
                    "Invalid upvalue handle at index {}: {}",
                    b, e
                )));
            }
        }
        
        let upvalue = tx.get_upvalue(upvalue_handle)?;
        eprintln!("DEBUG GETUPVAL: Upvalue state - stack_index: {:?}, value: {:?}", 
                 upvalue.stack_index, upvalue.value);
        
        // Get value from upvalue
        let value = if let Some(stack_idx) = upvalue.stack_index {
            // Open upvalue - read from stack
            eprintln!("DEBUG GETUPVAL: Open upvalue, reading from stack position {}", stack_idx);
            match tx.read_register(current_thread, stack_idx) {
                Ok(val) => {
                    eprintln!("DEBUG GETUPVAL: Stack position {} contains: {:?}", stack_idx, val);
                    val
                },
                Err(e) => {
                    eprintln!("ERROR GETUPVAL: Cannot read stack position {}: {}", stack_idx, e);
                    // If we can't read from the stack position, return nil instead of erroring
                    // This can happen if the stack has been unwound but the upvalue wasn't closed
                    Value::Nil
                }
            }
        } else if let Some(ref val) = upvalue.value {
            // Closed upvalue - use stored value
            eprintln!("DEBUG GETUPVAL: Closed upvalue, value: {:?}", val);
            val.clone()
        } else {
            // Invalid state - both stack_index and value are None
            // This shouldn't happen, but handle gracefully by returning nil
            eprintln!("WARNING GETUPVAL: Upvalue in invalid state (no stack_index or value), returning nil");
            Value::Nil
        };
        
        eprintln!("DEBUG GETUPVAL: Setting R({}) = {:?}", base as usize + a, value);
        tx.set_register(current_thread, base as usize + a, value)?;
        
        Ok(())
    }
    
    /// SETUPVAL: UpValue[A] := R(B)
    fn op_setupval(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        let value = tx.read_register(current_thread, base as usize + b)?;
        
        // Get current closure
        let frame = tx.get_current_frame(current_thread)?;
        let closure = tx.get_closure(frame.closure)?;
        
        if a >= closure.upvalues.len() {
            return Err(LuaError::RuntimeError(format!(
                "Upvalue index {} out of bounds",
                a
            )));
        }
        
        let upvalue_handle = closure.upvalues[a];
        tx.set_upvalue(upvalue_handle, value, current_thread)?;
        
        Ok(())
    }
    
    /// CLOSURE: R(A) := closure(KPROTO[Bx], R(A), ... ,R(A+n))
    fn op_closure(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let bx = inst.get_bx() as usize;
        
        eprintln!("DEBUG CLOSURE: A={}, Bx={}, base={}", a, bx, base);
        
        // Extract all needed parent closure info first
        let (proto_handle, parent_upvalues, parent_frame) = {
            let frame = tx.get_current_frame(current_thread)?;
            let parent_closure = tx.get_closure(frame.closure)?;
            
            eprintln!("DEBUG CLOSURE: Parent closure has {} constants and {} upvalues", 
                     parent_closure.proto.constants.len(), parent_closure.upvalues.len());
            
            // Get the function prototype from constants
            if bx >= parent_closure.proto.constants.len() {
                return Err(LuaError::RuntimeError(format!(
                    "Function prototype index {} out of bounds (max: {})",
                    bx, parent_closure.proto.constants.len() - 1
                )));
            }
            
            // Extract the function prototype handle from the constant
            let proto_handle = match &parent_closure.proto.constants[bx] {
                Value::FunctionProto(handle) => {
                    eprintln!("DEBUG CLOSURE: Found function prototype at constant index {}", bx);
                    *handle
                },
                _ => {
                    eprintln!("ERROR CLOSURE: Expected function prototype at constant index {}, got {}", 
                             bx, parent_closure.proto.constants[bx].type_name());
                    return Err(LuaError::RuntimeError(format!(
                        "Expected function prototype at constant index {}, got {}",
                        bx,
                        parent_closure.proto.constants[bx].type_name()
                    )));
                }
            };
            
            // Clone the parent upvalues to avoid holding the reference
            let parent_upvalues = parent_closure.upvalues.clone();
            
            (proto_handle, parent_upvalues, frame)
        }; // Drop the borrow of tx here
        
        // Get the prototype's upvalue information
        let proto = tx.get_function_proto(proto_handle)?;
        let upvalue_infos = proto.upvalues.clone();
        
        eprintln!("DEBUG CLOSURE: Creating closure with {} upvalues", upvalue_infos.len());
        
        // Create upvalues for the new closure with proper validation
        let mut upvalues = Vec::new();
        
        for (i, &upval_info) in upvalue_infos.iter().enumerate() {
            eprintln!("DEBUG CLOSURE: Processing upvalue {}: in_stack={}, index={}", 
                     i, upval_info.in_stack, upval_info.index);
            
            let upvalue_handle = if upval_info.in_stack {
                // Upvalue refers to local variable in enclosing function
                let stack_index = base as usize + upval_info.index as usize;
                eprintln!("DEBUG CLOSURE: Creating upvalue for stack position {} (base={}, index={})", 
                         stack_index, base, upval_info.index);
                
                // Validate stack index is within bounds
                let stack_size = tx.get_stack_size(current_thread)?;
                if stack_index >= stack_size {
                    eprintln!("ERROR CLOSURE: Stack index {} out of bounds (stack size: {})", 
                             stack_index, stack_size);
                    return Err(LuaError::RuntimeError(format!(
                        "Invalid upvalue reference to stack position {} (stack size: {})",
                        stack_index, stack_size
                    )));
                }
                
                // Check what value is at this stack position for debugging
                match tx.read_register(current_thread, stack_index) {
                    Ok(value) => {
                        eprintln!("DEBUG CLOSURE: Stack position {} contains: {:?}", stack_index, value);
                    },
                    Err(e) => {
                        eprintln!("ERROR CLOSURE: Cannot read stack position {}: {}", stack_index, e);
                        return Err(LuaError::RuntimeError(format!(
                            "Cannot access stack position {} for upvalue: {}",
                            stack_index, e
                        )));
                    }
                }
                
                tx.find_or_create_upvalue(
                    current_thread,
                    stack_index
                )?
            } else {
                // Upvalue refers to upvalue of enclosing function
                eprintln!("DEBUG CLOSURE: Referencing parent upvalue at index {}", upval_info.index);
                
                if upval_info.index as usize >= parent_upvalues.len() {
                    eprintln!("ERROR CLOSURE: Parent upvalue index {} out of bounds (parent has {} upvalues)", 
                             upval_info.index, parent_upvalues.len());
                    return Err(LuaError::RuntimeError(format!(
                        "Invalid upvalue reference: parent closure has {} upvalues, but tried to access index {}",
                        parent_upvalues.len(),
                        upval_info.index
                    )));
                }
                
                let parent_upval = parent_upvalues[upval_info.index as usize];
                
                // Validate the parent upvalue handle
                match tx.validate_handle(&parent_upval) {
                    Ok(_) => {
                        eprintln!("DEBUG CLOSURE: Parent upvalue handle {:?} is valid", parent_upval);
                    },
                    Err(e) => {
                        eprintln!("ERROR CLOSURE: Parent upvalue handle {:?} is invalid: {}", parent_upval, e);
                        return Err(LuaError::RuntimeError(format!(
                            "Invalid parent upvalue at index {}: {}",
                            upval_info.index, e
                        )));
                    }
                }
                
                parent_upval
            };
            
            upvalues.push(upvalue_handle);
        }
        
        // Create new closure using the convenience method
        let closure_handle = tx.create_closure_from_proto(proto_handle, upvalues)?;
        
        eprintln!("DEBUG CLOSURE: Created closure handle {:?}, storing in R({})", 
                 closure_handle, base as usize + a);
        
        tx.set_register(current_thread, base as usize + a, Value::Closure(closure_handle))?;
        
        Ok(())
    }
    
    /// CLOSE: close all upvalues >= R(A)
    fn op_close(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let threshold = base as usize + a;
        
        // Close upvalues that reference stack positions >= threshold
        tx.close_thread_upvalues(current_thread, threshold)?;
        
        Ok(())
    }
    
    /// FORPREP: R(A)-=R(A+2); pc+=sBx
    /// Prepares for a numeric for loop by subtracting step from initial and checking if the loop should run
    fn op_forprep(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let sbx = inst.get_sbx();
        
        let loop_base = base as usize + a;
        
        // Ensure stack space for all loop registers
        tx.grow_stack(current_thread, loop_base + 4)?;  // Need 4 registers
        
        eprintln!("DEBUG FORPREP: A={}, sbx={}, base={}, loop_base={}", a, sbx, base, loop_base);
        
        // CRITICAL DEBUG: Validate we're not trying to access registers before the function's base
        if loop_base < base as usize {
            eprintln!("ERROR FORPREP: loop_base {} is less than function base {}", loop_base, base);
            return Err(LuaError::RuntimeError(format!(
                "FORPREP: Invalid register access - loop_base {} < base {}",
                loop_base, base
            )));
        }
        
        // Get the three values with detailed logging
        eprintln!("DEBUG FORPREP: Reading initial at position {}", loop_base);
        let initial = tx.read_register(current_thread, loop_base)?;
        eprintln!("DEBUG FORPREP: Initial value: {:?}", initial);
        
        eprintln!("DEBUG FORPREP: Reading limit at position {}", loop_base + 1);
        let limit = tx.read_register(current_thread, loop_base + 1)?;
        eprintln!("DEBUG FORPREP: Limit value: {:?}", limit);
        
        eprintln!("DEBUG FORPREP: Reading step at position {}", loop_base + 2);
        let step = tx.read_register(current_thread, loop_base + 2)?;
        eprintln!("DEBUG FORPREP: Step value before processing: {:?}", step);
        
        // Convert to numbers with error handling
        let initial_num = match &initial {
            Value::Number(n) => *n,
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: format!("for initial value: {}", initial.type_name()),
            }),
        };
        
        // CRITICAL: Check if we're about to write to an invalid register
        if base > 0 && loop_base == 0 {
            eprintln!("WARNING FORPREP: Attempting to write to R(0) when function base is {}. This may be outside the function's register window!", base);
            eprintln!("WARNING FORPREP: The compiler may have generated incorrect bytecode, or there's a register calculation issue.");
        }
        
        let limit_num = match &limit {
            Value::Number(n) => *n,
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: format!("for limit value: {}", limit.type_name()),
            }),
        };
        
        // Critical: Ensure step exists, providing default if needed
        let step_num = match &step {
            Value::Number(n) => *n,
            Value::Nil => {
                eprintln!("DEBUG FORPREP: Step is nil, initializing with default 1.0");
                1.0
            },
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: format!("for step value: {}", step.type_name()),
            }),
        };
        
        // Ensure step is not zero
        if step_num == 0.0 {
            return Err(LuaError::RuntimeError("for loop step must be different from 0".to_string()));
        }
        
        // CRITICAL FIX: Always write the step value back to ensure it's properly initialized
        // This ensures FORLOOP will see a valid step value even across transaction boundaries
        eprintln!("DEBUG FORPREP: Writing step value {} to register {}", step_num, loop_base + 2);
        tx.set_register(current_thread, loop_base + 2, Value::Number(step_num))?;
        eprintln!("DEBUG FORPREP: Successfully wrote step value");
        
        // Verify step write
        let verify_step = tx.read_register(current_thread, loop_base + 2)?;
        eprintln!("DEBUG FORPREP: Verified step register now contains: {:?}", verify_step);
        
        // Per Lua 5.1 spec: Subtract step from initial counter 
        let prepared_initial = initial_num - step_num;
        eprintln!("DEBUG FORPREP: Prepared initial value = {} (original {} - step {})", 
                 prepared_initial, initial_num, step_num);
        
        // CRITICAL: Write prepared initial value with verification
        eprintln!("DEBUG FORPREP: About to write prepared initial {} to register {}", prepared_initial, loop_base);
        match tx.set_register(current_thread, loop_base, Value::Number(prepared_initial)) {
            Ok(_) => {
                eprintln!("DEBUG FORPREP: Successfully wrote prepared initial value");
                
                // Immediately verify the write
                match tx.read_register(current_thread, loop_base) {
                    Ok(verify_val) => {
                        eprintln!("DEBUG FORPREP: Verification read of R({}) = {:?}", loop_base, verify_val);
                        if let Value::Number(n) = verify_val {
                            if (n - prepared_initial).abs() > f64::EPSILON {
                                eprintln!("ERROR FORPREP: Write verification failed! Wrote {} but read {}", prepared_initial, n);
                            }
                        } else {
                            eprintln!("ERROR FORPREP: Write verification failed! Expected Number but got {:?}", verify_val);
                        }
                    },
                    Err(e) => {
                        eprintln!("ERROR FORPREP: Failed to verify write: {}", e);
                    }
                }
            },
            Err(e) => {
                eprintln!("ERROR FORPREP: Failed to write prepared initial: {}", e);
                return Err(e);
            }
        }
        
        // Initialize R(A+3) to nil - FORLOOP will set it when loop runs
        eprintln!("DEBUG FORPREP: Initializing user variable R({}) to nil", loop_base + 3);
        tx.set_register(current_thread, loop_base + 3, Value::Nil)?;
        
        // Check if loop should run at all
        let should_run = if step_num > 0.0 {
            prepared_initial + step_num <= limit_num // For positive step
        } else {
            prepared_initial + step_num >= limit_num // For negative step
        };
        
        eprintln!("DEBUG FORPREP: Loop should run: {} (prepared {} + step {} vs limit {})", 
                 should_run, prepared_initial, step_num, limit_num);
        
        if !should_run {
            // Skip the loop entirely
            let pc = tx.get_pc(current_thread)?;
            let new_pc = (pc as i32 + sbx) as usize;
            eprintln!("DEBUG FORPREP: Loop will not run, jumping to PC={}", new_pc);
            tx.set_pc(current_thread, new_pc)?;
        } else {
            eprintln!("DEBUG FORPREP: Loop will run, continuing to next instruction");
        }
        
        // Additional validation for debugging
        eprintln!("DEBUG FORPREP: Final register state after setup:");
        for i in 0..4 {
            match tx.read_register(current_thread, loop_base + i) {
                Ok(reg_val) => eprintln!("  R({}) = {:?}", loop_base + i, reg_val),
                Err(e) => eprintln!("  R({}) = ERROR: {}", loop_base + i, e),
            }
        }
        
        Ok(())
    }
    
    /// FORLOOP: R(A)+=R(A+2); if R(A) <?= R(A+1) then { R(A+3)=R(A); pc-=sBx }
    /// Increments the loop variable and tests if the loop should continue
    fn op_forloop(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let sbx = inst.get_sbx();
        
        let loop_base = base as usize + a;
        
        eprintln!("DEBUG FORLOOP [START]: A={}, sbx={}, base={}, loop_base={}", a, sbx, base, loop_base);
        
        // CRITICAL: Ensure stack has space for all loop registers
        tx.grow_stack(current_thread, loop_base + 4)?;  // Ensure space for all 4 loop registers
        
        // Dump the register state for debugging
        eprintln!("DEBUG FORLOOP [REGISTERS]: Initial state");
        for i in 0..4 {
            let abs_index = loop_base + i;
            match tx.read_register(current_thread, abs_index) {
                Ok(val) => eprintln!("  Register R({}) = {:?}", abs_index, val),
                Err(e) => eprintln!("  Register R({}) = Error: {}", abs_index, e),
            }
        }
        
        // Step 1: Read the internal counter value
        eprintln!("DEBUG FORLOOP [STEP 1]: Reading internal counter at R({})", loop_base);
        let loop_var = match tx.read_register(current_thread, loop_base) {
            Ok(val) => val,
            Err(e) => {
                eprintln!("DEBUG FORLOOP [ERROR]: Failed to read counter: {}", e);
                return Err(e);
            }
        };
        eprintln!("DEBUG FORLOOP [STEP 1]: Internal counter = {:?}", loop_var);
        
        // Step 2: Read the limit value
        eprintln!("DEBUG FORLOOP [STEP 2]: Reading limit at R({})", loop_base + 1);
        let limit = match tx.read_register(current_thread, loop_base + 1) {
            Ok(val) => val,
            Err(e) => {
                eprintln!("DEBUG FORLOOP [ERROR]: Failed to read limit: {}", e);
                return Err(e);
            }
        };
        eprintln!("DEBUG FORLOOP [STEP 2]: Limit = {:?}", limit);
        
        // Step 3: Read the step value
        eprintln!("DEBUG FORLOOP [STEP 3]: Reading step at R({})", loop_base + 2);
        let step = match tx.read_register(current_thread, loop_base + 2) {
            Ok(val) => val,
            Err(e) => {
                eprintln!("DEBUG FORLOOP [ERROR]: Failed to read step: {}", e);
                return Err(e);
            }
        };
        eprintln!("DEBUG FORLOOP [STEP 3]: Step value read = {:?}", step);
        
        // Step 4: Convert values to numbers (with specific type error messages)
        let loop_num = match &loop_var {
            Value::Number(n) => *n,
            _ => {
                let error_msg = format!("for loop variable is not a number (got: {})", loop_var.type_name());
                eprintln!("DEBUG FORLOOP [ERROR]: {}", error_msg);
                return Err(LuaError::TypeError {
                    expected: "number".to_string(),
                    got: loop_var.type_name().to_string(),
                });
            }
        };
        
        let limit_num = match &limit {
            Value::Number(n) => *n,
            _ => {
                let error_msg = format!("for loop limit is not a number (got: {})", limit.type_name());
                eprintln!("DEBUG FORLOOP [ERROR]: {}", error_msg);
                return Err(LuaError::TypeError {
                    expected: "number".to_string(),
                    got: limit.type_name().to_string(),
                });
            }
        };
        
        // CRITICAL FIX: Defensive handling of nil/invalid step values
        // This shouldn't happen if FORPREP initialized correctly, but adds safety
        let step_num = match &step {
            Value::Number(n) => {
                eprintln!("DEBUG FORLOOP [STEP 4]: Step is a valid number: {}", n);
                *n
            },
            Value::Nil => {
                eprintln!("WARNING FORLOOP [STEP 4]: Step is nil, defaulting to 1.0 (FORPREP should have initialized this!)");
                // Write the default value back to the register for consistency
                tx.set_register(current_thread, loop_base + 2, Value::Number(1.0))?;
                1.0
            },
            _ => {
                let error_msg = format!("for loop step is not a number (got: {})", step.type_name());
                eprintln!("DEBUG FORLOOP [ERROR]: {}", error_msg);
                return Err(LuaError::TypeError {
                    expected: "number".to_string(),
                    got: format!("for step value: {}", step.type_name()),
                });
            }
        };
        
        // Step 5: Check for zero step (defensive check)
        if step_num == 0.0 {
            let error_msg = "for loop step cannot be zero";
            eprintln!("DEBUG FORLOOP [ERROR]: {}", error_msg);
            return Err(LuaError::RuntimeError(error_msg.to_string()));
        }
        
        // Step 6: Increment loop counter
        let new_loop_num = loop_num + step_num;
        eprintln!("DEBUG FORLOOP [STEP 6]: Incrementing counter {} + {} = {}", loop_num, step_num, new_loop_num);
        match tx.set_register(current_thread, loop_base, Value::Number(new_loop_num)) {
            Ok(_) => {
                eprintln!("DEBUG FORLOOP [STEP 6]: Successfully set new counter value");
            },
            Err(e) => {
                eprintln!("DEBUG FORLOOP [ERROR]: Failed to set incremented counter: {}", e);
                return Err(e);
            }
        }
        
        // Step 7: Check if loop should continue
        let should_continue = if step_num > 0.0 {
            new_loop_num <= limit_num  // For positive step, continue if <= limit
        } else {
            new_loop_num >= limit_num  // For negative step, continue if >= limit
        };
        
        eprintln!("DEBUG FORLOOP [STEP 7]: Should continue = {} (new value {} vs limit {})", 
                 should_continue, new_loop_num, limit_num);
        
        if should_continue {
            // Step 8a: Set user variable to new loop value
            eprintln!("DEBUG FORLOOP [STEP 8a]: Setting user variable R({}) = {}", loop_base + 3, new_loop_num);
            match tx.set_register(current_thread, loop_base + 3, Value::Number(new_loop_num)) {
                Ok(_) => {
                    eprintln!("DEBUG FORLOOP [STEP 8a]: Successfully set user variable");
                },
                Err(e) => {
                    eprintln!("DEBUG FORLOOP [ERROR]: Failed to set user variable: {}", e);
                    return Err(e);
                }
            }
            
            // Step 9a: Jump back to loop start
            let pc = tx.get_pc(current_thread)?;
            let new_pc = (pc as i32 + sbx) as usize;
            eprintln!("DEBUG FORLOOP [STEP 9a]: Jumping back from PC {} to PC {}", pc, new_pc);
            match tx.set_pc(current_thread, new_pc) {
                Ok(_) => {
                    eprintln!("DEBUG FORLOOP [STEP 9a]: Successfully jumped back");
                },
                Err(e) => {
                    eprintln!("DEBUG FORLOOP [ERROR]: Failed to jump back: {}", e);
                    return Err(e);
                }
            }
        } else {
            // Step 8b/9b: Loop complete, just continue to next instruction
            eprintln!("DEBUG FORLOOP [STEP 8b/9b]: Loop complete, continuing to next instruction");
        }
        
        // Final state dump for debugging
        eprintln!("DEBUG FORLOOP [COMPLETE]: Final register state:");
        for i in 0..4 {
            let abs_index = loop_base + i;
            match tx.read_register(current_thread, abs_index) {
                Ok(val) => eprintln!("  Register R({}) = {:?}", abs_index, val),
                Err(_) => {}, // Ignore errors in debug output
            }
        }
        
        eprintln!("DEBUG FORLOOP [COMPLETE]: Successfully executed FORLOOP");
        Ok(())
    }
    
    /// TForLoop: R(A+3), R(A+4), ... ,R(A+2+C) := R(A)(R(A+1), R(A+2));
    ///           if R(A+3) ~= nil then R(A+2) := R(A+3) else pc++
    /// 
    /// This implements Lua's generic iteration protocol for 'for k,v in pairs(t) do' style loops.
    /// A points to the stack location containing the iterator function.
    fn op_tforloop(
        tx: &mut HeapTransaction,
        inst: Instruction,
        base: u16,
        current_thread: ThreadHandle
    ) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let c = inst.get_c() as usize;
        
        eprintln!("DEBUG TFORLOOP: A={}, C={}, base={}", a, c, base);
        
        // TFORLOOP uses these registers:
        // R(A)   = iterator function
        // R(A+1) = state
        // R(A+2) = control variable (current key)
        // Results will be placed in R(A+3) to R(A+2+C)
        
        // Read the iterator function and arguments
        let func_idx = base as usize + a;
        let state_idx = base as usize + a + 1;
        let control_idx = base as usize + a + 2;
        
        let func = tx.read_register(current_thread, func_idx)?;
        let state = tx.read_register(current_thread, state_idx)?;
        let control = tx.read_register(current_thread, control_idx)?;
        
        eprintln!("DEBUG TFORLOOP: Iterator function: {:?}", func);
        eprintln!("DEBUG TFORLOOP: State: {:?}", state);
        eprintln!("DEBUG TFORLOOP: Control variable: {:?}", control);
        
        // Set up function call: R(A)(R(A+1), R(A+2))
        // The stack is arranged as:
        // [base+a] = function
        // [base+a+1] = state
        // [base+a+2] = control variable
        // [base+a+3] = first result (will be placed here)
        
        // Ensure we have enough stack space for results
        let needed_size = base as usize + a + 3 + c;
        tx.grow_stack(current_thread, needed_size)?;
        
        // Place parameters for the call at the same position (no need to copy)
        // The function takes state and control as parameters
        
        // Queue the call operation based on function type
        match func {
            Value::Closure(closure) => {
                eprintln!("DEBUG TFORLOOP: Queueing Lua function call");
                tx.queue_operation(PendingOperation::FunctionCall {
                    func_index: func_idx,
                    nargs: 2,  // state and control as arguments
                    expected_results: c as i32,  // Number of results needed
                })?;
            },
            Value::CFunction(cfunc) => {
                eprintln!("DEBUG TFORLOOP: Queueing C function call");
                tx.queue_operation(PendingOperation::CFunctionCall {
                    function: cfunc,
                    base: func_idx as u16,
                    nargs: 2,  // state and control as arguments
                    expected_results: c as i32,  // Number of results needed 
                })?;
            },
            _ => {
                return Err(LuaError::TypeError {
                    expected: "function".to_string(),
                    got: func.type_name().to_string()
                });
            }
        }
        
        // Store current PC for reuse by continuation
        let pc = tx.get_pc(current_thread)?;
        
        // Queue the continuation to check loop condition after call completes
        tx.queue_operation(PendingOperation::TForLoopContinuation {
            base: base as usize,
            a,
            c,
            pc_before_tforloop: pc - 1, // Storing the PC value of the TFORLOOP instruction
        })?;
        
        Ok(())
    }
    
    /// VARARG: R(A), R(A+1), ..., R(A+B-2) = vararg
    /// 
    /// This opcode loads variable arguments passed to a vararg function.
    /// If B is 0, it loads all varargs. Otherwise, it loads B-1 varargs.
    /// If there are fewer varargs than requested, the remaining registers are filled with nil.
    fn op_vararg(
        tx: &mut HeapTransaction,
        inst: Instruction,
        base: u16,
        current_thread: ThreadHandle
    ) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        eprintln!("DEBUG VARARG: A={}, B={}, base={}", a, b, base);
        
        // Get current frame to access varargs
        let frame = tx.get_current_frame(current_thread)?;
        
        // Get varargs from frame (may be None if function is not vararg)
        let varargs = frame.varargs.as_ref();
        
        eprintln!("DEBUG VARARG: Has varargs: {}", varargs.is_some());
        
        // Determine how many values to copy
        let (num_to_copy, total_varargs) = match varargs {
            Some(va) => {
                eprintln!("DEBUG VARARG: Available varargs: {}", va.len());
                if b == 0 {
                    // Copy all varargs
                    (va.len(), va.len())
                } else {
                    // Copy specific number (B-1)
                    let requested = b - 1;
                    eprintln!("DEBUG VARARG: Requested {} values", requested);
                    (requested, va.len())
                }
            }
            None => {
                // No varargs available
                eprintln!("DEBUG VARARG: No varargs available");
                if b == 0 {
                    (0, 0)  // No varargs to copy
                } else {
                    (b - 1, 0)  // Will fill with nils
                }
            }
        };
        
        eprintln!("DEBUG VARARG: Will copy {} values (total available: {})", num_to_copy, total_varargs);
        
        // Ensure stack has space
        let needed_size = base as usize + a + num_to_copy;
        tx.grow_stack(current_thread, needed_size)?;
        
        // Copy varargs to destination registers
        for i in 0..num_to_copy {
            let value = if let Some(va) = varargs {
                if i < va.len() {
                    eprintln!("DEBUG VARARG: Copying vararg {} to register R({})", i, base as usize + a + i);
                    va[i].clone()
                } else {
                    eprintln!("DEBUG VARARG: Filling register R({}) with nil (not enough varargs)", base as usize + a + i);
                    Value::Nil  // Fill with nil if not enough varargs
                }
            } else {
                eprintln!("DEBUG VARARG: Filling register R({}) with nil (no varargs)", base as usize + a + i);
                Value::Nil  // No varargs available
            };
            
            tx.set_register(current_thread, base as usize + a + i, value)?;
        }
        
        Ok(())
    }
    
    /// SETLIST: R(A)[(C-1)*FPF+i] := R(A+i), 1 <= i <= B
    /// 
    /// Bulk assignment to table array part
    fn op_setlist(
        tx: &mut HeapTransaction,
        inst: Instruction,
        base: u16,
        current_thread: ThreadHandle
    ) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        let c = inst.get_c() as usize;
        
        // Fields per flush (Lua 5.1 uses 50)
        const FIELDS_PER_FLUSH: usize = 50;
        
        eprintln!("DEBUG SETLIST: A={}, B={}, C={}, base={}", a, b, c, base);
        
        // Get the table
        let table_val = tx.read_register(current_thread, base as usize + a)?;
        let table_handle = match table_val {
            Value::Table(h) => h,
            _ => return Err(LuaError::TypeError {
                expected: "table".to_string(),
                got: table_val.type_name().to_string(),
            }),
        };
        
        // Calculate base index in table
        let table_base = if c == 0 {
            // Special case: next instruction contains actual C value
            // For now, we'll handle the common case
            0
        } else {
            (c - 1) * FIELDS_PER_FLUSH
        };
        
        // Number of elements to set
        let count = if b == 0 {
            // Set all values from R(A+1) to top
            let top = tx.get_stack_top(current_thread)?;
            let top_rel = top.saturating_sub(base as usize + a + 1);
            eprintln!("DEBUG SETLIST: B=0, using all values up to stack top. Found {} values", top_rel + 1);
            top_rel + 1
        } else {
            b
        };
        
        eprintln!("DEBUG SETLIST: Setting {} elements starting at table index {}", 
                 count, table_base + 1);
        
        // Set elements one by one using the optimized array element setter
        for i in 0..count {
            let source_register = base as usize + a + 1 + i;
            let value = tx.read_register(current_thread, source_register)?;
            let array_index = table_base + i + 1; // +1 for 1-based indexing
            
            eprintln!("DEBUG SETLIST: Setting table element [{}] = {:?} from register R({})",
                     array_index, value, source_register);
            
            // Use the dedicated array element setter
            tx.set_table_array_element(table_handle, array_index, value)?;
        }
        
        // Debug check if indices were set correctly
        if count > 0 {
            let check_idx = table_base + 1; // First index in this batch
            let check_key = Value::Number(check_idx as f64);
            let check_value = tx.read_table_field(table_handle, &check_key)?;
            eprintln!("DEBUG SETLIST: Verification - table[{}] = {:?}", check_idx, check_value);
        }
        
        Ok(())
    }
    
    // Helper methods
    
    /// Get constant from current function - safely gets a constant with proper borrow management
    fn get_constant(tx: &mut HeapTransaction, index: usize, current_thread: ThreadHandle) -> LuaResult<Value> {
        let frame = tx.get_current_frame(current_thread)?;
        let closure = tx.get_closure(frame.closure)?;
        
        if index >= closure.proto.constants.len() {
            return Err(LuaError::RuntimeError(format!(
                "Constant index {} out of bounds (max: {})",
                index,
                closure.proto.constants.len()
            )));
        }
        
        // Clone the constant
        Ok(closure.proto.constants[index].clone())
    }
    
    /// Ensure thread has enough stack space
    fn ensure_stack_size(
        tx: &mut HeapTransaction,
        thread: ThreadHandle,
        needed: usize,
        max_stack_size: usize,
    ) -> LuaResult<()> {
        if needed > max_stack_size {
            return Err(LuaError::RuntimeError(format!(
                "Stack overflow (needed {} > max {})",
                needed,
                max_stack_size
            )));
        }
        
        // Actually grow the stack to the needed size
        tx.grow_stack(thread, needed)?;
        
        Ok(())
    }
    
    /// Compare for equality
    fn compare_eq(left: &Value, right: &Value) -> bool {
        match (left, right) {
            (Value::Nil, Value::Nil) => true,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            _ => false,
        }
    }
    
    /// Compare for less than
    fn compare_lt(left: &Value, right: &Value) -> LuaResult<bool> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => Ok(a < b),
            (Value::String(_a), Value::String(_b)) => {
                // String comparison would require heap access
                Err(LuaError::NotImplemented("String comparison".to_string()))
            }
            _ => Err(LuaError::TypeError {
                expected: "number or string".to_string(),
                got: format!("{} and {}", left.type_name(), right.type_name()),
            }),
        }
    }
    
    /// Compare for less than or equal
    fn compare_le(left: &Value, right: &Value) -> LuaResult<bool> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => Ok(a <= b),
            (Value::String(_a), Value::String(_b)) => {
                // String comparison would require heap access
                Err(LuaError::NotImplemented("String comparison".to_string()))
            }
            _ => Err(LuaError::TypeError {
                expected: "number or string".to_string(),
                got: format!("{} and {}", left.type_name(), right.type_name()),
            }),
        }
    }
    
    /// Get heap reference (for library functions)
    pub fn heap(&self) -> &LuaHeap {
        &self.heap
    }
    
    /// Get mutable heap reference (for library functions)
    pub fn heap_mut(&mut self) -> &mut LuaHeap {
        &mut self.heap
    }
    
    /// Update resource limits for this VM
    pub fn set_resource_limits(&mut self, limits: ResourceLimits) {
        self.config.resource_limits = limits;
    }
    
    /// Get current resource limits
    pub fn get_resource_limits(&self) -> &ResourceLimits {
        &self.config.resource_limits
    }
}

/// Arithmetic operation types
#[derive(Debug, Clone, Copy)]
enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
}

/// Comparison operation types
#[derive(Debug, Clone, Copy)]
enum CompOp {
    Eq,
    Lt,
    Le,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lua::value::FunctionProto;
    
    #[test]
    fn test_vm_creation() {
        let vm = LuaVM::new().unwrap();
        assert_eq!(vm.operation_queue.len(), 0);
    }
    
    #[test]
    fn test_circular_table_handling() {
        let mut vm = LuaVM::new().unwrap();
        vm.init_stdlib().unwrap();
        
        // Create a circular table structure
        let script = r#"
            local t1 = {name = "table1"}
            local t2 = {name = "table2"}
            t1.next = t2
            t2.next = t1  -- Circular reference
            
            -- This should NOT error - circular data structures are valid
            return t1
        "#;
        
        // Should execute successfully
        let result = vm.eval_script(script).unwrap();
        assert!(matches!(result, Value::Table(_)));
    }
    
    #[test]
    fn test_simple_execution() {
        let mut vm = LuaVM::new().unwrap();
        
        // Create a simple function that returns nil
        let proto = FunctionProto {
            bytecode: vec![
                Instruction::create_ABC(OpCode::Return, 0, 1, 0).0,
            ],
            constants: vec![],
            num_params: 0,
            is_vararg: false,
            max_stack_size: 2,
            upvalues: vec![],
        };
        
        let closure = Closure {
            proto,
            upvalues: vec![],
        };
        
        // Create closure in heap
        let closure_handle = {
            let mut tx = HeapTransaction::new(&mut vm.heap);
            let handle = tx.create_closure(closure).unwrap();
            tx.commit().unwrap();
            handle
        };
        
        // Execute
        let results = vm.execute(closure_handle).unwrap();
        assert_eq!(results.len(), 0);
    }
}