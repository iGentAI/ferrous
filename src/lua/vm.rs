//! Lua Virtual Machine Implementation
//! 
//! This module implements the Lua 5.1 VM using a unified stack architecture
//! with transaction-based safety and non-recursive execution.

use super::codegen::{Instruction, OpCode};
use super::error::{LuaError, LuaResult};
use super::handle::{StringHandle, TableHandle, ClosureHandle, ThreadHandle, UpvalueHandle};
use super::heap::LuaHeap;
use super::metamethod::{MetamethodType, MetamethodContext, MetamethodContinuation};
use super::transaction::{HeapTransaction, TransactionState};
use super::value::{Value, CallFrame, CFunction, Closure, FunctionProto, UpvalueInfo};
use std::collections::VecDeque;

/// Pending operations for non-recursive VM execution
#[derive(Debug, Clone)]
pub enum PendingOperation {
    /// Function call operation
    FunctionCall {
        /// Closure to call
        closure: ClosureHandle,
        /// Arguments for the call
        args: Vec<Value>,
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
pub struct VMAccess<'a> {
    /// Reference to heap
    pub heap: &'a mut LuaHeap,
}

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
    pub vm_access: VMAccess<'a>,
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
            vm_access: unsafe { std::mem::zeroed() }, // This will be set properly later
        }
    }
    
    /// Create a new execution context with VM access
    pub fn new_with_vm(
        tx: &'a mut HeapTransaction<'a>,
        thread: ThreadHandle,
        base: u16,
        nargs: usize,
        vm_access: VMAccess<'a>,
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
        
        let register = self.base as usize + index;
        self.tx.read_register(self.thread, register)
    }
    
    /// Get an argument value (0-indexed)
    pub fn arg(&mut self, index: usize) -> LuaResult<Value> {
        self.get_arg(index)
    }
    
    /// Push a return value (alias for stdlib compatibility)
    pub fn push_result(&mut self, value: Value) -> LuaResult<()> {
        let register = self.base as usize + self.nargs;
        self.tx.set_register(self.thread, register, value)
    }
    
    /// Push a return value
    pub fn push_return(&mut self, value: Value) -> LuaResult<()> {
        self.push_result(value)
    }
    
    /// Set return value at specific index
    pub fn set_return(&mut self, index: usize, value: Value) -> LuaResult<()> {
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
}

impl Default for VMConfig {
    fn default() -> Self {
        VMConfig {
            max_stack_size: 1_000_000,
            max_call_depth: 1000,
            debug_mode: false,
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
    /// Create a new VM instance
    pub fn new() -> LuaResult<Self> {
        Self::with_config(VMConfig::default())
    }
    
    /// Create a new VM with custom configuration
    pub fn with_config(config: VMConfig) -> LuaResult<Self> {
        let heap = LuaHeap::new()?;
        let main_thread = heap.main_thread()?;
        
        Ok(LuaVM {
            heap,
            operation_queue: VecDeque::new(),
            main_thread,
            current_thread: main_thread,
            config,
        })
    }
    
    /// Execute a closure on the main thread
    pub fn execute(&mut self, closure: ClosureHandle) -> LuaResult<Vec<Value>> {
        // Set up initial call
        self.operation_queue.push_back(PendingOperation::FunctionCall {
            closure,
            args: vec![],
            expected_results: -1,
        });
        
        // Run until completion
        loop {
            match self.step()? {
                StepResult::Continue => continue,
                StepResult::Completed(values) => return Ok(values),
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
        
        // Queue initial call with arguments
        let mut call_args = Vec::with_capacity(args.len());
        for arg in args {
            call_args.push(arg.clone());
        }
        
        self.operation_queue.push_back(PendingOperation::FunctionCall { 
            closure: closure_handle, 
            args: call_args, 
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
        super::stdlib::init_stdlib(self)
    }
    
    /// Execute a single step of the VM
    pub fn step(&mut self) -> LuaResult<StepResult> {
        // Process pending operations first
        if let Some(op) = self.operation_queue.pop_front() {
            return self.process_pending_operation(op);
        }
        
        // Store current_thread to avoid borrowing issues
        let current_thread = self.current_thread;
        
        // Create transaction for this step
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Check if there are any call frames left
        let depth = tx.get_thread_call_depth(current_thread)?;
        if depth == 0 {
            // No more frames and no pending operations - execution complete
            tx.commit()?;
            return Ok(StepResult::Completed(vec![]));
        }
        
        // Get current execution state
        let frame = tx.get_current_frame(current_thread)?;
        let base = frame.base_register;
        let pc = frame.pc;
        
        // Get instruction
        let instruction = tx.get_instruction(frame.closure, pc)?;
        let inst = Instruction(instruction);
        
        // Increment PC for next instruction
        tx.increment_pc(current_thread)?;
        
        // Execute instruction - pass current_thread as parameter
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
            OpCode::Return => Self::op_return(&mut tx, inst, base, current_thread),
            OpCode::ForPrep => Self::op_forprep(&mut tx, inst, base, current_thread),
            OpCode::ForLoop => Self::op_forloop(&mut tx, inst, base, current_thread),
            OpCode::GetUpval => Self::op_getupval(&mut tx, inst, base, current_thread),
            OpCode::SetUpval => Self::op_setupval(&mut tx, inst, base, current_thread),
            OpCode::Closure => Self::op_closure(&mut tx, inst, base, current_thread),
            OpCode::Close => Self::op_close(&mut tx, inst, base, current_thread),
            _ => Err(LuaError::NotImplemented(format!(
                "Opcode {:?}",
                inst.get_opcode()
            ))),
        };
        
        // Handle result
        match result {
            Ok(_) => {
                // Commit transaction and process any pending operations
                let pending = tx.commit()?;
                for op in pending {
                    self.operation_queue.push_back(op);
                }
                
                // Always continue - let pending operations determine completion
                Ok(StepResult::Continue)
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
            PendingOperation::FunctionCall { closure, args, expected_results } => {
                self.process_function_call(closure, args, expected_results)
            }
            PendingOperation::CFunctionCall { function, base, nargs, expected_results } => {
                self.process_c_function_call(function, base, nargs, expected_results)
            }
            PendingOperation::Return { values } => {
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
        closure_handle: ClosureHandle,
        args: Vec<Value>,
        expected_results: i32,
    ) -> LuaResult<StepResult> {
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Get closure details
        let closure = tx.get_closure(closure_handle)?;
        let num_params = closure.proto.num_params as usize;
        let is_vararg = closure.proto.is_vararg;
        let max_stack = closure.proto.max_stack_size as usize;
        
        // Get current stack state
        let stack_top = tx.get_stack_top(self.current_thread)?;
        let new_base = stack_top + 1;
        
        // Prepare stack for new function
        let needed_size = new_base + max_stack;
        
        // Use static ensure_stack_size method
        Self::ensure_stack_size(&mut tx, self.current_thread, needed_size, self.config.max_stack_size)?;
        
        // Push function and arguments
        tx.set_register(self.current_thread, stack_top + 1, Value::Closure(closure_handle))?;
        for (i, arg) in args.iter().enumerate() {
            tx.set_register(self.current_thread, new_base + 1 + i, arg.clone())?;
        }
        
        // Handle parameter adjustment
        let varargs = if is_vararg && args.len() > num_params {
            Some(args[num_params..].to_vec())
        } else {
            None
        };
        
        // Fill missing parameters with nil
        for i in args.len()..num_params {
            tx.set_register(self.current_thread, new_base + 1 + i, Value::Nil)?;
        }
        
        // Create new call frame
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
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Create execution context without VM access initially
        let mut ctx = ExecutionContext::new(&mut tx, self.current_thread, base, nargs);
        
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
            // No active frames - this shouldn't happen normally
            return Err(LuaError::RuntimeError("No active call frame".to_string()));
        }
        
        // Get current frame to find where to place results
        let frame = tx.get_current_frame(self.current_thread)?;
        let func_register = frame.base_register.saturating_sub(1);
        
        // Pop the call frame
        tx.pop_call_frame(self.current_thread)?;
        
        // Check if this was the last frame
        if tx.get_thread_call_depth(self.current_thread)? == 0 {
            // Main function returned
            tx.commit()?;
            return Ok(StepResult::Completed(values));
        }
        
        // Get the parent frame's expected results
        let parent_frame = tx.get_current_frame(self.current_thread)?;
        let expected = parent_frame.expected_results;
        
        // Place results starting at function's register
        let result_count = if let Some(n) = expected {
            n.min(values.len())
        } else {
            values.len()
        };
        
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
    
    // Opcode implementations
    
    /// MOVE: R(A) := R(B)
    fn op_move(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        let value = tx.read_register(current_thread, base as usize + b)?;
        tx.set_register(current_thread, base as usize + a, value)?;
        
        Ok(())
    }
    
    /// LOADK: R(A) := Kst(Bx)
    fn op_loadk(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let bx = inst.get_bx() as usize;
        
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
                    "Constant index {} out of bounds",
                    bx
                )));
            }
            
            // Clone the key to avoid holding a reference into closure
            closure.proto.constants[bx].clone()
        }; // Drop the borrow of tx here
        
        // Get globals table
        let globals = tx.get_globals_table()?;
        
        // Get value from globals
        let value = tx.get_table_with_metamethods(globals, &key)?;
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
        
        // Collect all values to concatenate
        let mut parts = Vec::new();
        for i in b..=c {
            let value = tx.read_register(current_thread, base as usize + i)?;
            match value {
                Value::String(handle) => {
                    let s = tx.get_string_value(handle)?;
                    parts.push(s);
                }
                Value::Number(n) => {
                    parts.push(n.to_string());
                }
                _ => {
                    return Err(LuaError::TypeError {
                        expected: "string or number".to_string(),
                        got: value.type_name().to_string(),
                    });
                }
            }
        }
        
        // Concatenate
        let result = parts.join("");
        let handle = tx.create_string(&result)?;
        tx.set_register(current_thread, base as usize + a, Value::String(handle))?;
        
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
        
        // Get function
        let func = tx.read_register(current_thread, base as usize + a)?;
        
        // Determine argument count
        let nargs = if b == 0 {
            // Use all values up to stack top
            let stack_top = tx.get_stack_top(current_thread)?;
            stack_top - (base as usize + a)
        } else {
            (b - 1) as usize
        };
        
        // Collect arguments
        let mut args = Vec::with_capacity(nargs);
        for i in 0..nargs {
            args.push(tx.read_register(current_thread, base as usize + a + 1 + i)?);
        }
        
        // Determine expected results
        let expected_results = if c == 0 {
            -1  // Multiple results
        } else {
            (c - 1) as i32
        };
        
        // Queue the call based on function type
        match func {
            Value::Closure(closure) => {
                tx.queue_operation(PendingOperation::FunctionCall {
                    closure,
                    args,
                    expected_results,
                })?;
            }
            Value::CFunction(cfunc) => {
                tx.queue_operation(PendingOperation::CFunctionCall {
                    function: cfunc,
                    base: (base as usize + a) as u16,
                    nargs,
                    expected_results,
                })?;
            }
            _ => {
                return Err(LuaError::TypeError {
                    expected: "function".to_string(),
                    got: func.type_name().to_string(),
                });
            }
        }
        
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
        
        // Get current closure
        let frame = tx.get_current_frame(current_thread)?;
        let closure = tx.get_closure(frame.closure)?;
        
        if b >= closure.upvalues.len() {
            return Err(LuaError::RuntimeError(format!(
                "Upvalue index {} out of bounds",
                b
            )));
        }
        
        let upvalue_handle = closure.upvalues[b];
        let upvalue = tx.get_upvalue(upvalue_handle)?;
        
        // Get value from upvalue
        let value = if let Some(stack_idx) = upvalue.stack_index {
            tx.read_register(current_thread, stack_idx)?
        } else if let Some(ref val) = upvalue.value {
            val.clone()
        } else {
            return Err(LuaError::RuntimeError("Invalid upvalue state".to_string()));
        };
        
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
        
        // Extract all needed parent closure info first
        let (proto_handle, parent_upvalues) = {
            let frame = tx.get_current_frame(current_thread)?;
            let parent_closure = tx.get_closure(frame.closure)?;
            
            // Get the function prototype from constants
            if bx >= parent_closure.proto.constants.len() {
                return Err(LuaError::RuntimeError(format!(
                    "Function prototype index {} out of bounds",
                    bx
                )));
            }
            
            // Extract the function prototype handle from the constant
            let proto_handle = match &parent_closure.proto.constants[bx] {
                Value::FunctionProto(handle) => *handle,
                _ => {
                    return Err(LuaError::RuntimeError(format!(
                        "Expected function prototype at constant index {}, got {}",
                        bx,
                        parent_closure.proto.constants[bx].type_name()
                    )));
                }
            };
            
            // Clone the parent upvalues to avoid holding the reference
            let parent_upvalues = parent_closure.upvalues.clone();
            
            (proto_handle, parent_upvalues)
        }; // Drop the borrow of tx here
        
        // Get the prototype's upvalue information
        let proto = tx.get_function_proto(proto_handle)?;
        let upvalue_infos = proto.upvalues.clone();
        
        // Create upvalues for the new closure
        let mut upvalues = Vec::new();
        
        for &upval_info in &upvalue_infos {
            let upvalue_handle = if upval_info.in_stack {
                // Upvalue refers to local variable in enclosing function
                tx.find_or_create_upvalue(
                    current_thread,
                    base as usize + upval_info.index as usize
                )?
            } else {
                // Upvalue refers to upvalue of enclosing function
                if upval_info.index as usize >= parent_upvalues.len() {
                    return Err(LuaError::RuntimeError(
                        "Invalid upvalue reference".to_string()
                    ));
                }
                parent_upvalues[upval_info.index as usize]
            };
            upvalues.push(upvalue_handle);
        }
        
        // Create new closure using the convenience method
        let closure_handle = tx.create_closure_from_proto(proto_handle, upvalues)?;
        
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
    fn op_forprep(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let sbx = inst.get_sbx();
        
        // Get the three values: initial, limit, step
        let initial = tx.read_register(current_thread, base as usize + a)?;
        let limit = tx.read_register(current_thread, base as usize + a + 1)?;
        let step = tx.read_register(current_thread, base as usize + a + 2)?;
        
        // All values must be numbers
        let initial_num = match initial {
            Value::Number(n) => n,
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: initial.type_name().to_string(),
            }),
        };
        
        let limit_num = match limit {
            Value::Number(n) => n,
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: limit.type_name().to_string(),
            }),
        };
        
        let step_num = match step {
            Value::Number(n) => n,
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: step.type_name().to_string(),
            }),
        };
        
        // Check for invalid step
        if step_num == 0.0 {
            return Err(LuaError::RuntimeError("for loop step is zero".to_string()));
        }
        
        // Prepare loop: subtract step from initial value
        let prepared_initial = initial_num - step_num;
        tx.set_register(current_thread, base as usize + a, Value::Number(prepared_initial))?;
        
        // Check if loop should run at all
        let should_run = if step_num > 0.0 {
            prepared_initial + step_num <= limit_num
        } else {
            prepared_initial + step_num >= limit_num
        };
        
        if !should_run {
            // Skip the loop entirely
            let pc = tx.get_pc(current_thread)?;
            let new_pc = (pc as i32 + sbx) as usize;
            tx.set_pc(current_thread, new_pc)?;
        }
        
        Ok(())
    }
    
    /// FORLOOP: R(A)+=R(A+2); if R(A) <= R(A+1) then { R(A+3)=R(A); pc+=sBx }
    fn op_forloop(tx: &mut HeapTransaction, inst: Instruction, base: u16, current_thread: ThreadHandle) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let sbx = inst.get_sbx();
        
        // Get loop variable, limit, and step
        let loop_var = tx.read_register(current_thread, base as usize + a)?;
        let limit = tx.read_register(current_thread, base as usize + a + 1)?;
        let step = tx.read_register(current_thread, base as usize + a + 2)?;
        
        // All must be numbers
        let loop_num = match loop_var {
            Value::Number(n) => n,
            _ => return Err(LuaError::RuntimeError("for loop variable is not a number".to_string())),
        };
        
        let limit_num = match limit {
            Value::Number(n) => n,
            _ => return Err(LuaError::RuntimeError("for loop limit is not a number".to_string())),
        };
        
        let step_num = match step {
            Value::Number(n) => n,
            _ => return Err(LuaError::RuntimeError("for loop step is not a number".to_string())),
        };
        
        // Increment loop variable
        let new_loop_num = loop_num + step_num;
        tx.set_register(current_thread, base as usize + a, Value::Number(new_loop_num))?;
        
        // Check if we should continue
        let should_continue = if step_num > 0.0 {
            new_loop_num <= limit_num
        } else {
            new_loop_num >= limit_num
        };
        
        if should_continue {
            // Copy internal loop variable to external (user-visible) variable
            tx.set_register(current_thread, base as usize + a + 3, Value::Number(new_loop_num))?;
            
            // Jump back
            let pc = tx.get_pc(current_thread)?;
            let new_pc = (pc as i32 + sbx) as usize;
            tx.set_pc(current_thread, new_pc)?;
        }
        
        Ok(())
    }
    
    // Helper methods
    
    /// Get a constant from current function
    fn get_constant(tx: &mut HeapTransaction, index: usize, current_thread: ThreadHandle) -> LuaResult<Value> {
        let frame = tx.get_current_frame(current_thread)?;
        let closure = tx.get_closure(frame.closure)?;
        
        if index >= closure.proto.constants.len() {
            return Err(LuaError::RuntimeError(format!(
                "Constant index {} out of bounds",
                index
            )));
        }
        
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
        
        // In a real implementation, we'd grow the stack here
        // For now, we assume the stack is large enough
        
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