//! RefCell-based Lua VM Implementation
//! 
//! This module implements a Lua VM using RefCellHeap for direct memory access,
//! eliminating the transaction system complexity while maintaining memory safety.


use super::codegen::{Instruction, OpCode};
use super::compiler::CompiledModule;
use super::codegen::CompilationConstant;
use super::error::{LuaError, LuaResult};
use super::handle::{StringHandle, TableHandle, ClosureHandle, ThreadHandle, UpvalueHandle, FunctionProtoHandle};
use super::refcell_heap::RefCellHeap;
use super::value::{Value, CallFrame, CFunction, Closure, FunctionProto, UpvalueInfo};
use std::collections::VecDeque;

/// Trait for execution contexts that provide access to VM state during C function calls.
/// 
/// This trait abstracts over different VM implementations, allowing C functions to be
/// written once and work with both transaction-based and RefCell-based VMs.
pub trait ExecutionContext {
    /// Get the number of arguments passed to this C function
    fn arg_count(&self) -> usize;
    
    /// Get the number of arguments (alias for arg_count)
    fn nargs(&self) -> usize {
        self.arg_count()
    }
    
    /// Get an argument value by index (0-based)
    fn get_arg(&self, index: usize) -> LuaResult<Value>;
    
    /// Get an argument value by index (alias for get_arg)
    fn arg(&self, index: usize) -> LuaResult<Value> {
        self.get_arg(index)
    }
    
    /// Push a return value
    fn push_result(&mut self, value: Value) -> LuaResult<()>;
    
    /// Push a return value (alias for push_result)
    fn push_return(&mut self, value: Value) -> LuaResult<()> {
        self.push_result(value)
    }
    
    /// Set a return value at a specific index
    fn set_return(&mut self, index: usize, value: Value) -> LuaResult<()>;
    
    /// Create a new string
    fn create_string(&self, s: &str) -> LuaResult<StringHandle>;
    
    /// Create a new table
    fn create_table(&self) -> LuaResult<TableHandle>;
    
    /// Get a table field
    fn get_table_field(&self, table: TableHandle, key: &Value) -> LuaResult<Value>;
    
    /// Set a table field
    fn set_table_field(&self, table: TableHandle, key: Value, value: Value) -> LuaResult<()>;
    
    /// Get argument as string
    fn get_arg_str(&self, index: usize) -> LuaResult<String>;
    
    /// Get argument as number
    fn get_number_arg(&self, index: usize) -> LuaResult<f64>;
    
    /// Get argument as boolean  
    fn get_bool_arg(&self, index: usize) -> LuaResult<bool>;
    
    /// Get next key-value pair from table
    fn table_next(&self, table: TableHandle, key: Value) -> LuaResult<Option<(Value, Value)>>;
    
    /// Get the globals table handle
    fn globals_handle(&self) -> LuaResult<TableHandle>;
    
    /// Get string value from handle
    fn get_string_from_handle(&self, handle: StringHandle) -> LuaResult<String>;
    
    /// Check for metamethod on a value
    fn check_metamethod(&self, value: &Value, method_name: &str) -> LuaResult<Option<Value>>;
    
    /// Call a metamethod
    fn call_metamethod(&mut self, func: Value, args: Vec<Value>) -> LuaResult<Vec<Value>>;
    
    /// Get table field with metamethod support
    fn table_get(&self, table: TableHandle, key: Value) -> LuaResult<Value>;
    
    /// Get table field without metamethods
    fn table_raw_get(&self, table: TableHandle, key: Value) -> LuaResult<Value>;
    
    /// Set table field without metamethods
    fn table_raw_set(&self, table: TableHandle, key: Value, value: Value) -> LuaResult<()>;
    
    /// Get table length
    fn table_length(&self, table: TableHandle) -> LuaResult<usize>;
    
    /// Set metatable for a table
    fn set_metatable(&self, table: TableHandle, metatable: Option<TableHandle>) -> LuaResult<()>;
    
    /// Get metatable for a table
    fn get_metatable(&self, table: TableHandle) -> LuaResult<Option<TableHandle>>;
    
    /// Get the current thread
    fn get_current_thread(&self) -> LuaResult<ThreadHandle>;
    
    /// Get the base register index
    fn get_base_index(&self) -> LuaResult<usize>;
    
    /// Get the number of results pushed so far
    fn get_results_pushed(&self) -> usize;
    
    /// Get a value from the global table
    fn globals_get(&self, name: &str) -> LuaResult<Value>;
    
    /// Execute a function
    fn execute_function(&mut self, closure: ClosureHandle, args: &[Value]) -> LuaResult<Value>;
    
    /// Evaluate a Lua script
    fn eval_script(&mut self, script: &str) -> LuaResult<Value>;
    
    /// Get table field by integer index
    fn get_table_field_by_int(&self, table: TableHandle, index: isize) -> LuaResult<Value> {
        self.table_raw_get(table, Value::Number(index as f64))
    }
    
    /// Raw table access methods (bypass metamethods)
    fn get_raw_table_field(&self, table: TableHandle, key: &Value) -> LuaResult<Value>;
    fn set_raw_table_field(&mut self, table: TableHandle, key: &Value, value: &Value) -> LuaResult<()>;
}

/// Pending operations for non-recursive VM execution
#[derive(Debug, Clone)]
pub enum PendingOperation {
    /// Function call operation
    FunctionCall {
        func_index: usize,
        nargs: usize,
        expected_results: i32,
    },
    
    /// C function call operation
    CFunctionCall {
        function: CFunction,
        base: u16,
        nargs: usize,
        expected_results: i32,
    },
    
    /// Return from function
    Return {
        values: Vec<Value>,
    },
    
    /// TFORLOOP continuation after iterator function returns
    TForLoopContinuation {
        /// Base register
        base: usize,
        /// A field from instruction
        a: usize,
        /// C field from instruction (result count)
        c: usize,
        /// PC value before the iterator call for proper control flow
        pc_before_call: usize,
        /// Temporary base register used for the function call
        temp_base: usize,
    },
}

/// VM configuration
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

/// RefCell-based Lua VM
pub struct RefCellVM {
    /// The heap with RefCell interior mutability
    heap: RefCellHeap,
    
    /// Operation queue for handling calls/returns
    operation_queue: VecDeque<PendingOperation>,
    
    /// Main thread handle
    main_thread: ThreadHandle,
    
    /// Currently executing thread
    current_thread: ThreadHandle,
    
    /// VM configuration
    config: VMConfig,
}

impl RefCellVM {
    /// Create a new VM instance
    pub fn new() -> LuaResult<Self> {
        Self::with_config(VMConfig::default())
    }
    
    /// Call a metamethod with the given arguments and return the results
    /// This sets up registers properly for the metamethod call and handles return values
    fn call_metamethod(&mut self, method: Value, args: &[Value], expected_results: usize) -> LuaResult<Vec<Value>> {
        eprintln!("DEBUG call_metamethod: Calling {:?} with {} args, expecting {} results", 
                 method, args.len(), expected_results);
        
        // Find a suitable location on the stack for the metamethod call
        let stack_size = self.heap.get_stack_size(self.current_thread)?;
        let call_base = stack_size;
        
        // Ensure we have enough stack space
        self.heap.grow_stack(self.current_thread, call_base + args.len() + 10)?;
        
        // Place the metamethod function
        self.heap.set_thread_register(self.current_thread, call_base, &method)?;
        
        // Place the arguments
        for (i, arg) in args.iter().enumerate() {
            self.heap.set_thread_register(self.current_thread, call_base + 1 + i, arg)?;
        }
        
        // Queue the metamethod call
        match method {
            Value::Closure(_) => {
                self.operation_queue.push_back(PendingOperation::FunctionCall {
                    func_index: call_base,
                    nargs: args.len(),
                    expected_results: expected_results as i32,
                });
            }
            Value::CFunction(cfunc) => {
                self.operation_queue.push_back(PendingOperation::CFunctionCall {
                    function: cfunc,
                    base: call_base as u16,
                    nargs: args.len(),
                    expected_results: expected_results as i32,
                });
            }
            _ => {
                return Err(LuaError::TypeError {
                    expected: "function".to_string(),
                    got: method.type_name().to_string(),
                });
            }
        }
        
        // Process the call immediately
        while let Some(op) = self.operation_queue.pop_front() {
            match self.process_pending_operation(op)? {
                StepResult::Continue => continue,
                StepResult::Completed(_) => break,
            }
        }
        
        // Collect the results
        let mut results = Vec::with_capacity(expected_results);
        for i in 0..expected_results {
            let result = self.heap.get_thread_register(self.current_thread, call_base + i)?;
            results.push(result);
        }
        
        eprintln!("DEBUG call_metamethod: Returned {} results", results.len());
        Ok(results)
    }
    
    /// Create a function prototype in the heap
    pub fn create_function_proto(&mut self, proto: FunctionProto) -> LuaResult<FunctionProtoHandle> {
        self.heap.create_function_proto(proto)
    }
    
    /// Get a copy of a function prototype
    pub fn get_function_proto_copy(&self, handle: FunctionProtoHandle) -> LuaResult<FunctionProto> {
        self.heap.get_function_proto_copy(handle)
    }
    
    /// Replace a function prototype with a new one
    pub fn replace_function_proto(&mut self, handle: FunctionProtoHandle, proto: FunctionProto) -> LuaResult<()> {
        self.heap.replace_function_proto(handle, proto)
    }
    
    /// Create a new VM with custom configuration
    pub fn with_config(config: VMConfig) -> LuaResult<Self> {
        let heap = RefCellHeap::new()?;
        let main_thread = heap.main_thread()?;
        
        Ok(RefCellVM {
            heap,
            operation_queue: VecDeque::new(),
            main_thread,
            current_thread: main_thread,
            config,
        })
    }
    
    /// Execute a closure
    pub fn execute(&mut self, closure: ClosureHandle) -> LuaResult<Vec<Value>> {
        // Clear any previous state
        self.operation_queue.clear();
        
        // Place the closure at position 0 of the main thread
        self.heap.set_thread_register(self.main_thread, 0, &Value::Closure(closure))?;
        
        // Queue initial function call
        self.operation_queue.push_back(PendingOperation::FunctionCall {
            func_index: 0,
            nargs: 0,
            expected_results: -1,
        });
        
        // Execute until completion
        loop {
            // Always process pending operations first
            if let Some(op) = self.operation_queue.pop_front() {
                match self.process_pending_operation(op)? {
                    StepResult::Continue => continue,
                    StepResult::Completed(values) => return Ok(values),
                }
            }
            
            // Then execute next instruction if no pending operations
            match self.step()? {
                StepResult::Continue => continue,
                StepResult::Completed(values) => return Ok(values),
            }
        }
    }
    
    /// Execute a single VM step
    fn step(&mut self) -> LuaResult<StepResult> {
        // Check if we have any call frames
        let depth = self.heap.get_thread_call_depth(self.current_thread)?;
        if depth == 0 {
            eprintln!("DEBUG step: No call frames, execution complete");
            return Ok(StepResult::Completed(vec![]));
        }
        
        // Get current frame info
        let frame = self.heap.get_current_frame(self.current_thread)?;
        let base = frame.base_register;
        let pc = frame.pc;
        
        // Get the instruction
        let instruction = self.heap.get_instruction(frame.closure, pc)?;
        let inst = Instruction(instruction);
        
        // Enhanced debugging for all opcodes during function tests
        match inst.get_opcode() {
            OpCode::ForPrep | OpCode::ForLoop => {
                eprintln!("DEBUG RefCellVM: Executing {:?} at PC={}, base={}", 
                         inst.get_opcode(), pc, base);
            }
            OpCode::Closure | OpCode::Call | OpCode::Return | 
            OpCode::SetGlobal | OpCode::GetGlobal => {
                eprintln!("DEBUG RefCellVM: Executing {:?} at PC={}, base={}, instruction=0x{:08x}", 
                         inst.get_opcode(), pc, base, instruction);
            }
            OpCode::Eq | OpCode::Lt | OpCode::Le => {
                eprintln!("DEBUG RefCellVM: Executing comparison {:?} at PC={}, base={}, instruction=0x{:08x}", 
                         inst.get_opcode(), pc, base, instruction);
                eprintln!("  A={}, B={}, C={}", inst.get_a(), inst.get_b(), inst.get_c());
                let (b_is_const, b_idx) = inst.get_rk_b();
                let (c_is_const, c_idx) = inst.get_rk_c();
                eprintln!("  RK(B): is_const={}, idx={}", b_is_const, b_idx);
                eprintln!("  RK(C): is_const={}, idx={}", c_is_const, c_idx);
            }
            _ => {
                // Uncomment for full trace
                // eprintln!("DEBUG RefCellVM: Executing {:?} at PC={}, base={}", 
                //          inst.get_opcode(), pc, base);
            }
        }
        
        // Increment PC for next instruction
        self.heap.increment_pc(self.current_thread)?;
        
        // Execute the instruction
        match inst.get_opcode() {
            OpCode::Move => self.op_move(inst, base)?,
            OpCode::LoadK => self.op_loadk(inst, base)?,
            OpCode::LoadBool => self.op_loadbool(inst, base)?,
            OpCode::LoadNil => self.op_loadnil(inst, base)?,
            OpCode::GetGlobal => self.op_getglobal(inst, base)?,
            OpCode::SetGlobal => self.op_setglobal(inst, base)?,
            OpCode::GetTable => self.op_gettable(inst, base)?,
            OpCode::SetTable => self.op_settable(inst, base)?,
            OpCode::NewTable => self.op_newtable(inst, base)?,
            OpCode::SelfOp => self.op_self(inst, base)?,
            OpCode::Add => self.op_add(inst, base)?,
            OpCode::Sub => self.op_sub(inst, base)?,
            OpCode::Mul => self.op_mul(inst, base)?,
            OpCode::Div => self.op_div(inst, base)?,
            OpCode::Mod => self.op_mod(inst, base)?,
            OpCode::Pow => self.op_pow(inst, base)?,
            OpCode::Unm => self.op_unm(inst, base)?,
            OpCode::Not => self.op_not(inst, base)?,
            OpCode::Len => self.op_len(inst, base)?,
            OpCode::Concat => self.op_concat(inst, base)?,
            OpCode::Jmp => self.op_jmp(inst)?,
            OpCode::Eq => self.op_eq(inst, base)?,
            OpCode::Lt => self.op_lt(inst, base)?,
            OpCode::Le => self.op_le(inst, base)?,
            OpCode::Test => self.op_test(inst, base)?,
            OpCode::TestSet => self.op_testset(inst, base)?,
            OpCode::Call => self.op_call(inst, base)?,
            OpCode::TailCall => self.op_tailcall(inst, base)?,
            OpCode::Return => self.op_return(inst, base)?,
            OpCode::ForPrep => self.op_forprep(inst, base)?,
            OpCode::ForLoop => self.op_forloop(inst, base)?,
            OpCode::TForLoop => self.op_tforloop(inst, base)?,
            OpCode::VarArg => self.op_vararg(inst, base)?,
            OpCode::GetUpval => self.op_getupval(inst, base)?,
            OpCode::SetUpval => self.op_setupval(inst, base)?,
            OpCode::Closure => self.op_closure(inst, base)?,
            OpCode::Close => self.op_close(inst, base)?,
            OpCode::SetList => self.op_setlist(inst, base)?,
            
            _ => {
                eprintln!("ERROR: Unimplemented opcode {:?}", inst.get_opcode());
                return Err(LuaError::NotImplemented(format!(
                    "Opcode {:?}", inst.get_opcode()
                )));
            }
        }
        
        Ok(StepResult::Continue)
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
            PendingOperation::TForLoopContinuation { base, a, c, pc_before_call, temp_base } => {
                self.process_tforloop_continuation(base, a, c, pc_before_call, temp_base)
            }
        }
    }
    
    /// Process a function call
    fn process_function_call(
        &mut self,
        func_index: usize,
        nargs: usize,
        expected_results: i32,
    ) -> LuaResult<StepResult> {
        eprintln!("DEBUG process_function_call: func_index={}, nargs={}, expected_results={}", 
                 func_index, nargs, expected_results);
        
        // Get the function value
        let func_value = self.heap.get_thread_register(self.current_thread, func_index)?;
        
        eprintln!("DEBUG process_function_call: Function value: {:?}", func_value);
        
        let closure_handle = match func_value {
            Value::Closure(handle) => {
                eprintln!("DEBUG process_function_call: Calling closure with handle: {:?}", handle);
                handle
            },
            _ => {
                eprintln!("ERROR process_function_call: Expected closure, got {:?}", func_value);
                return Err(LuaError::TypeError {
                    expected: "closure".to_string(),
                    got: func_value.type_name().to_string(),
                });
            }
        };
        
        // Phase 1: Extract closure details (release borrow after)
        let (num_params, max_stack, is_vararg, bytecode_len, num_upvalues, upvalue_handles) = {
            let closure = self.heap.get_closure(closure_handle)?;
            eprintln!("DEBUG process_function_call: Closure upvalue handles: {:?}", closure.upvalues);
            (
                closure.proto.num_params as usize,
                closure.proto.max_stack_size as usize,
                closure.proto.is_vararg,
                closure.proto.bytecode.len(),
                closure.upvalues.len(),
                closure.upvalues.clone(),  // Clone the upvalue handles for debugging
            )
        }; // Borrow ends here
        
        eprintln!("DEBUG process_function_call: Closure details:");
        eprintln!("  - num_params: {}", num_params);
        eprintln!("  - max_stack_size: {}", max_stack);
        eprintln!("  - is_vararg: {}", is_vararg);
        eprintln!("  - bytecode length: {}", bytecode_len);
        eprintln!("  - num upvalues: {}", num_upvalues);
        eprintln!("  - upvalue handles: {:?}", upvalue_handles);
        
        // New base is after the function
        let new_base = func_index + 1;
        
        eprintln!("DEBUG process_function_call: new_base={}, num_params={}", new_base, num_params);
        
        // Debug: show current arguments
        for i in 0..nargs {
            let arg = self.heap.get_thread_register(self.current_thread, new_base + i)?;
            eprintln!("DEBUG process_function_call: Arg {} at R({}): {:?}", i, new_base + i, arg);
        }
        
        // Phase 2: Modify stack (no closure borrow active)
        // Ensure stack space
        self.heap.grow_stack(self.current_thread, new_base + max_stack)?;
        
        // Fill missing parameters with nil
        if nargs < num_params {
            eprintln!("DEBUG process_function_call: Filling {} missing params with nil", num_params - nargs);
            for i in nargs..num_params {
                self.heap.set_thread_register(self.current_thread, new_base + i, &Value::Nil)?;
            }
        }
        
        // Create new call frame
        let frame = CallFrame {
            closure: closure_handle,
            pc: 0,
            base_register: new_base as u16,
            expected_results: if expected_results < 0 { None } else { Some(expected_results as usize) },
            varargs: None, // TODO: Handle varargs
        };
        
        eprintln!("DEBUG process_function_call: pushing frame with base_register={}", frame.base_register);
        
        // Show first few instructions of the function (need fresh borrow)
        if bytecode_len > 0 {
            let closure = self.heap.get_closure(closure_handle)?;
            eprintln!("DEBUG process_function_call: First instruction: 0x{:08x}", closure.proto.bytecode[0]);
            let first_inst = Instruction(closure.proto.bytecode[0]);
            eprintln!("  Opcode: {:?}", first_inst.get_opcode());
        }
        
        self.heap.push_call_frame(self.current_thread, frame)?;
        
        // Debug: show call stack depth
        let depth = self.heap.get_thread_call_depth(self.current_thread)?;
        eprintln!("DEBUG process_function_call: Call stack depth now: {}", depth);
        
        Ok(StepResult::Continue)
    }
    
    /// Process a return operation
    fn process_return(&mut self, values: Vec<Value>) -> LuaResult<StepResult> {
        eprintln!("DEBUG process_return: returning {} values", values.len());
        
        // Check call depth
        let depth = self.heap.get_thread_call_depth(self.current_thread)?;
        if depth == 0 {
            return Ok(StepResult::Completed(values));
        }
        
        // Get current frame info
        let frame = self.heap.get_current_frame(self.current_thread)?;
        let func_register = frame.base_register.saturating_sub(1);
        
        // Pop the frame
        self.heap.pop_call_frame(self.current_thread)?;
        
        // Check if this was the last frame
        if self.heap.get_thread_call_depth(self.current_thread)? == 0 {
            return Ok(StepResult::Completed(values));
        }
        
        // Place return values
        let result_count = if let Some(n) = frame.expected_results {
            n.min(values.len())
        } else {
            values.len()
        };
        
        for (i, value) in values.iter().take(result_count).enumerate() {
            self.heap.set_thread_register(self.current_thread, func_register as usize + i, value)?;
        }
        
        // Fill missing expected results with nil
        if let Some(n) = frame.expected_results {
            for i in values.len()..n {
                self.heap.set_thread_register(self.current_thread, func_register as usize + i, &Value::Nil)?;
            }
        }
        
        Ok(StepResult::Continue)
    }
    
    /// Process TFORLOOP continuation after the iterator function returns
    fn process_tforloop_continuation(
        &mut self,
        base: usize,
        a: usize,
        c: usize,
        pc_before_call: usize,
        temp_base: usize,
    ) -> LuaResult<StepResult> {
        eprintln!("DEBUG process_tforloop_continuation: Starting");
        
        let func_idx = base + a;
        
        // Get the first result from the temporary area
        let first_result = self.heap.get_thread_register(self.current_thread, temp_base)?;
        
        eprintln!("DEBUG process_tforloop_continuation: First result = {:?}", first_result);
        
        if first_result.is_nil() {
            // End of iteration - skip the next instruction
            eprintln!("DEBUG process_tforloop_continuation: Nil result - ending loop");
            let pc = self.heap.get_pc(self.current_thread)?;
            self.heap.set_pc(self.current_thread, pc + 1)?;
            return Ok(StepResult::Continue);
        }
        
        // Continue iteration
        eprintln!("DEBUG process_tforloop_continuation: Non-nil result - continuing loop");
        
        // Update control variable with first result
        self.heap.set_thread_register(self.current_thread, func_idx + 2, &first_result)?;
        
        // Copy the results to the loop variables
        for i in 0..c {
            let value = if temp_base + i < self.heap.get_stack_size(self.current_thread)? {
                self.heap.get_thread_register(self.current_thread, temp_base + i)?
            } else {
                Value::Nil
            };
            
            self.heap.set_thread_register(self.current_thread, func_idx + 3 + i, &value)?;
            eprintln!("DEBUG process_tforloop_continuation: Set R({}) = {:?}", func_idx + 3 + i, value);
        }
        
        // Jump back to the TFORLOOP instruction
        self.heap.set_pc(self.current_thread, pc_before_call)?;
        
        Ok(StepResult::Continue)
    }
    
    // Opcode implementations
    
    /// MOVE: R(A) := R(B)
    fn op_move(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        let source_register = base as usize + b;
        let dest_register = base as usize + a;
        
        let value = self.heap.get_thread_register(self.current_thread, source_register)?;
        self.heap.set_thread_register(self.current_thread, dest_register, &value)?;
        
        Ok(())
    }
    
    /// LOADK: R(A) := Kst(Bx)
    fn op_loadk(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let bx = inst.get_bx() as usize;
        
        // Get current function to access constants
        let frame = self.heap.get_current_frame(self.current_thread)?;
        let closure = self.heap.get_closure(frame.closure)?;
        
        if bx >= closure.proto.constants.len() {
            return Err(LuaError::RuntimeError(format!(
                "Constant index {} out of bounds", bx
            )));
        }
        
        let constant = closure.proto.constants[bx].clone();
        self.heap.set_thread_register(self.current_thread, base as usize + a, &constant)?;
        
        Ok(())
    }
    
    /// LOADNIL: R(A) := ... := R(B) := nil
    fn op_loadnil(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        for i in a..=b {
            self.heap.set_thread_register(self.current_thread, base as usize + i, &Value::Nil)?;
        }
        
        Ok(())
    }
    
    /// JMP: pc+=sBx
    fn op_jmp(&mut self, inst: Instruction) -> LuaResult<()> {
        let sbx = inst.get_sbx();
        
        let pc = self.heap.get_pc(self.current_thread)?;
        let new_pc = (pc as i32 + sbx) as usize;
        self.heap.set_pc(self.current_thread, new_pc)?;
        
        Ok(())
    }
    
    /// TEST: if not (R(A) <=> C) then pc++
    fn op_test(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let c = inst.get_c();
        
        let value = self.heap.get_thread_register(self.current_thread, base as usize + a)?;
        let test = !value.is_falsey();
        
        if test != (c != 0) {
            let pc = self.heap.get_pc(self.current_thread)?;
            self.heap.set_pc(self.current_thread, pc + 1)?;
        }
        
        Ok(())
    }
    
    /// FORPREP: R(A)-=R(A+2); pc+=sBx
    /// This is the critical opcode for FOR loop initialization
    fn op_forprep(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let sbx = inst.get_sbx();
        
        let loop_base = base as usize + a;
        
        eprintln!("DEBUG FORPREP: a={}, base={}, loop_base={}", a, base, loop_base);
        
        // Ensure stack space
        self.heap.grow_stack(self.current_thread, loop_base + 4)?;
        
        // Read the three loop control values DIRECTLY from heap
        // This eliminates any transaction boundary issues
        let initial = self.heap.get_thread_register(self.current_thread, loop_base)?;
        let limit = self.heap.get_thread_register(self.current_thread, loop_base + 1)?;
        let step = self.heap.get_thread_register(self.current_thread, loop_base + 2)?;
        
        eprintln!("DEBUG FORPREP: Read values - initial={:?}, limit={:?}, step={:?}", 
                 initial, limit, step);
        
        // Convert to numbers
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
        
        // Handle step value with default
        let step_num = match step {
            Value::Number(n) => n,
            Value::Nil => {
                eprintln!("DEBUG FORPREP: Step is nil, using default 1.0");
                // CRITICAL: Write the default step value immediately
                self.heap.set_thread_register(self.current_thread, loop_base + 2, &Value::Number(1.0))?;
                1.0
            },
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: step.type_name().to_string(),
            }),
        };
        
        if step_num == 0.0 {
            return Err(LuaError::RuntimeError("for loop step cannot be zero".to_string()));
        }
        
        // Prepare initial value (subtract step per Lua spec)
        let prepared_initial = initial_num - step_num;
        
        eprintln!("DEBUG FORPREP: Setting prepared initial {} at R({})", 
                 prepared_initial, loop_base);
        
        // CRITICAL: Write the prepared initial value directly
        self.heap.set_thread_register(self.current_thread, loop_base, &Value::Number(prepared_initial))?;
        
        // Initialize user visible variable to nil
        self.heap.set_thread_register(self.current_thread, loop_base + 3, &Value::Nil)?;
        
        // Verify the write immediately
        let verify = self.heap.get_thread_register(self.current_thread, loop_base)?;
        eprintln!("DEBUG FORPREP: Verification read: {:?}", verify);
        
        // Check if loop should run
        let should_run = if step_num > 0.0 {
            prepared_initial + step_num <= limit_num
        } else {
            prepared_initial + step_num >= limit_num
        };
        
        eprintln!("DEBUG FORPREP: Loop should run: {}", should_run);
        
        if !should_run {
            // Skip the loop
            let pc = self.heap.get_pc(self.current_thread)?;
            let new_pc = (pc as i32 + sbx) as usize;
            self.heap.set_pc(self.current_thread, new_pc)?;
        }
        
        // Final debug dump
        eprintln!("DEBUG FORPREP: Final register state:");
        for i in 0..4 {
            let val = self.heap.get_thread_register(self.current_thread, loop_base + i)?;
            eprintln!("  R({}) = {:?}", loop_base + i, val);
        }
        
        Ok(())
    }
    
    /// FORLOOP: R(A)+=R(A+2); if R(A) <?= R(A+1) then { R(A+3)=R(A); pc-=sBx }
    /// This is the critical opcode for FOR loop iteration
    fn op_forloop(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let sbx = inst.get_sbx();
        
        let loop_base = base as usize + a;
        
        eprintln!("DEBUG FORLOOP: a={}, base={}, loop_base={}", a, base, loop_base);
        
        // Read control values DIRECTLY - no transaction boundaries!
        let loop_var = self.heap.get_thread_register(self.current_thread, loop_base)?;
        let limit = self.heap.get_thread_register(self.current_thread, loop_base + 1)?;
        let step = self.heap.get_thread_register(self.current_thread, loop_base + 2)?;
        
        eprintln!("DEBUG FORLOOP: Initial values - var={:?}, limit={:?}, step={:?}", 
                 loop_var, limit, step);
        
        // Convert to numbers
        let loop_num = match loop_var {
            Value::Number(n) => n,
            _ => {
                eprintln!("ERROR FORLOOP: loop variable is not a number!");
                return Err(LuaError::TypeError {
                    expected: "number".to_string(),
                    got: loop_var.type_name().to_string(),
                });
            }
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
            Value::Nil => {
                eprintln!("WARNING FORLOOP: Step is nil, using 1.0");
                1.0
            },
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: step.type_name().to_string(),
            }),
        };
        
        // Increment the loop variable
        let new_loop_num = loop_num + step_num;
        eprintln!("DEBUG FORLOOP: Incrementing {} + {} = {}", loop_num, step_num, new_loop_num);
        
        // CRITICAL: Write the new value immediately and directly
        self.heap.set_thread_register(self.current_thread, loop_base, &Value::Number(new_loop_num))?;
        
        // Check continuation condition
        let should_continue = if step_num > 0.0 {
            new_loop_num <= limit_num
        } else {
            new_loop_num >= limit_num
        };
        
        eprintln!("DEBUG FORLOOP: Should continue: {}", should_continue);
        
        if should_continue {
            // Update user visible variable
            self.heap.set_thread_register(self.current_thread, loop_base + 3, &Value::Number(new_loop_num))?;
            
            // Jump back
            let pc = self.heap.get_pc(self.current_thread)?;
            let new_pc = (pc as i32 + sbx) as usize;
            eprintln!("DEBUG FORLOOP: Jumping back from {} to {}", pc, new_pc);
            self.heap.set_pc(self.current_thread, new_pc)?;
        } else {
            eprintln!("DEBUG FORLOOP: Loop complete, continuing to next instruction");
        }
        
        // Final verification
        eprintln!("DEBUG FORLOOP: Final register state:");
        for i in 0..4 {
            let val = self.heap.get_thread_register(self.current_thread, loop_base + i)?;
            eprintln!("  R({}) = {:?}", loop_base + i, val);
        }
        
        Ok(())
    }
    
    /// CALL: R(A), ... ,R(A+C-2) := R(A)(R(A+1), ... ,R(A+B-1))
    fn op_call(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b();
        let c = inst.get_c();
        
        eprintln!("DEBUG op_call: A={}, B={}, C={}, base={}", a, b, c, base);
        
        let func_abs = base as usize + a;
        
        // Determine argument count
        let nargs = if b == 0 {
            // Use all values up to stack top
            let stack_size = self.heap.get_stack_size(self.current_thread)?;
            stack_size.saturating_sub(func_abs + 1)
        } else {
            (b - 1) as usize
        };
        
        // Determine expected results
        let expected_results = if c == 0 {
            -1  // Multiple results
        } else {
            (c - 1) as i32
        };
        
        eprintln!("DEBUG op_call: func at R({}), {} args, {} results expected", 
                 func_abs, nargs, expected_results);
        
        // Get the function value to determine its type
        let func_value = self.heap.get_thread_register(self.current_thread, func_abs)?;
        
        eprintln!("DEBUG op_call: Function value at R({}) is: {:?}", func_abs, func_value);
        
        // Debug: print arguments
        for i in 0..nargs {
            if i < nargs {
                let arg = self.heap.get_thread_register(self.current_thread, func_abs + 1 + i)?;
                eprintln!("DEBUG op_call: Arg {}: {:?}", i, arg);
            }
        }

        // Special handling for function calls within TFORLOOP context
        if let Value::CFunction(cfunc) = func_value {
            // Check if this is a call to next() from inside a for loop
            let is_next = cfunc as *const () == super::refcell_stdlib::refcell_next_adapter as *const ();
            let first_arg_is_bad = if nargs >= 1 {
                let arg0 = self.heap.get_thread_register(self.current_thread, func_abs + 1)?;
                !matches!(arg0, Value::Table(_))
            } else {
                false
            };
            
            if is_next && first_arg_is_bad {
                // This is a call to next() with a non-table first argument
                // We need to scan for the for loop state
                
                // Get current frame to access instruction history
                let frame = self.heap.get_current_frame(self.current_thread)?;
                
                // Look back in the bytecode for a TFORLOOP instruction
                let mut pc = frame.pc;
                for offset in 0..20 { // Look back up to 20 instructions
                    if pc > offset {
                        if let Ok(inst_val) = self.heap.get_instruction(frame.closure, pc - offset) {
                            let inst = Instruction(inst_val);
                            if inst.get_opcode() == OpCode::TForLoop {
                                // Found a TFORLOOP instruction, get its state register
                                let tforloop_a = inst.get_a() as usize;
                                let state_reg = base as usize + tforloop_a + 1; // R(A+1) = state
                                
                                // Get the state value
                                if let Ok(state) = self.heap.get_thread_register(self.current_thread, state_reg) {
                                    if let Value::Table(_) = state {
                                        // Found the table state, use it instead
                                        eprintln!("DEBUG op_call: Correcting next() arg from {:?} to {:?}", 
                                                 self.heap.get_thread_register(self.current_thread, func_abs + 1)?, 
                                                 state);
                                        self.heap.set_thread_register(self.current_thread, func_abs + 1, &state)?;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Queue the appropriate operation based on function type
        match func_value {
            Value::Closure(closure_handle) => {
                eprintln!("DEBUG op_call: Queueing Lua function call for closure {:?}", closure_handle);
                self.operation_queue.push_back(PendingOperation::FunctionCall {
                    func_index: func_abs,
                    nargs,
                    expected_results,
                });
            }
            Value::CFunction(cfunc) => {
                eprintln!("DEBUG op_call: Queueing C function call");
                self.operation_queue.push_back(PendingOperation::CFunctionCall {
                    function: cfunc,
                    base: func_abs as u16,
                    nargs,
                    expected_results,
                });
            }
            Value::Table(table_handle) => {
                // Check for __call metamethod
                eprintln!("DEBUG op_call: Attempting to call table, checking for __call metamethod");
                
                if let Some(metatable) = self.heap.get_table_metatable(table_handle)? {
                    let call_key = self.heap.create_string("__call")?;
                    let call_metamethod = self.heap.get_table_field(metatable, &Value::String(call_key))?;
                    
                    if !call_metamethod.is_nil() {
                        eprintln!("DEBUG op_call: Found __call metamethod");
                        
                        // Collect arguments: table + original args
                        let mut args = vec![Value::Table(table_handle)];
                        for i in 0..nargs {
                            let arg = self.heap.get_thread_register(self.current_thread, func_abs + 1 + i)?;
                            args.push(arg);
                        }
                        
                        // Call the metamethod with expected results
                        let results = self.call_metamethod(call_metamethod, &args, 
                                                          if expected_results < 0 { 1 } else { expected_results as usize })?;
                        
                        // Place results in the proper registers
                        for (i, result) in results.iter().enumerate() {
                            self.heap.set_thread_register(self.current_thread, func_abs + i, result)?;
                        }
                        
                        // If we expected a specific number of results, fill with nil
                        if expected_results >= 0 {
                            for i in results.len()..(expected_results as usize) {
                                self.heap.set_thread_register(self.current_thread, func_abs + i, &Value::Nil)?;
                            }
                        }
                        
                        return Ok(());
                    }
                }
                
                eprintln!("ERROR op_call: Attempted to call table without __call metamethod");
                return Err(LuaError::TypeError {
                    expected: "function".to_string(),
                    got: "table".to_string(),
                });
            }
            _ => {
                eprintln!("ERROR op_call: Attempted to call non-function value: {:?}", func_value);
                return Err(LuaError::TypeError {
                    expected: "function".to_string(),
                    got: func_value.type_name().to_string(),
                });
            }
        }
        
        Ok(())
    }
    
    /// RETURN: return R(A), ... ,R(A+B-2)
    /// 
    /// This instruction returns values from the current function and automatically
    /// closes any open upvalues that reference stack positions at or above the
    /// current function's base. This is part of the Lua 5.1 specification - 
    /// RETURN implicitly performs the work of CLOSE for the function's locals.
    fn op_return(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b();
        
        eprintln!("DEBUG op_return: A={}, B={}, base={}", a, b, base);
        
        // Debug: show current call depth
        let depth = self.heap.get_thread_call_depth(self.current_thread)?;
        eprintln!("DEBUG op_return: Current call depth: {}", depth);
        
        // IMPORTANT: RETURN automatically closes upvalues at or above the current 
        // function's base. This ensures that any upvalues referencing local variables
        // in the returning function are properly closed before the stack frame is destroyed.
        eprintln!("DEBUG op_return: Closing upvalues at or above stack position {}", base);
        self.heap.close_thread_upvalues(self.current_thread, base as usize)?;
        
        // Collect return values
        let mut values = Vec::new();
        
        if b == 0 {
            // Return all values from R(A) to stack top
            let stack_size = self.heap.get_stack_size(self.current_thread)?;
            let count = stack_size.saturating_sub(base as usize + a);
            eprintln!("DEBUG op_return: Returning all {} values from R({}) to top", count, base as usize + a);
            
            for i in a..(stack_size - base as usize) {
                let val = self.heap.get_thread_register(self.current_thread, base as usize + i)?;
                eprintln!("DEBUG op_return: Return value {}: {:?}", values.len(), val);
                values.push(val);
            }
        } else {
            // Return specific number of values
            let count = (b - 1) as usize;
            eprintln!("DEBUG op_return: Returning {} specific values starting at R({})", count, base as usize + a);
            
            for i in 0..count {
                let val = self.heap.get_thread_register(self.current_thread, base as usize + a + i)?;
                eprintln!("DEBUG op_return: Return value {}: {:?}", i, val);
                values.push(val);
            }
        }
        
        eprintln!("DEBUG op_return: Total return values: {}", values.len());
        
        // Queue the return
        self.operation_queue.push_back(PendingOperation::Return { values });
        
        Ok(())
    }

    /// UNM: R(A) := -R(B)
    fn op_unm(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        let source_register = base as usize + b;
        let value = self.heap.get_thread_register(self.current_thread, source_register)?;
        
        let result = match value {
            Value::Number(n) => Value::Number(-n),
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: value.type_name().to_string(),
            }),
        };
        
        self.heap.set_thread_register(self.current_thread, base as usize + a, &result)?;
        Ok(())
    }
    
    /// ADD: R(A) := RK(B) + RK(C)
    fn op_add(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        self.op_arithmetic(inst, base, ArithOp::Add)
    }
    
    /// SUB: R(A) := RK(B) - RK(C)
    fn op_sub(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        self.op_arithmetic(inst, base, ArithOp::Sub)
    }
    
    /// MUL: R(A) := RK(B) * RK(C)
    fn op_mul(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        self.op_arithmetic(inst, base, ArithOp::Mul)
    }
    
    /// DIV: R(A) := RK(B) / RK(C)
    fn op_div(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        self.op_arithmetic(inst, base, ArithOp::Div)
    }
    
    /// MOD: R(A) := RK(B) % RK(C)
    fn op_mod(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        self.op_arithmetic(inst, base, ArithOp::Mod)
    }
    
    /// POW: R(A) := RK(B) ^ RK(C)
    fn op_pow(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        self.op_arithmetic(inst, base, ArithOp::Pow)
    }
    
    /// Generic arithmetic operation handler
    fn op_arithmetic(&mut self, inst: Instruction, base: u16, op: ArithOp) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let (b_is_const, b_idx) = inst.get_rk_b();
        let (c_is_const, c_idx) = inst.get_rk_c();
        
        // Get left operand
        let left = if b_is_const {
            self.get_constant(b_idx as usize)?
        } else {
            self.heap.get_thread_register(self.current_thread, base as usize + b_idx as usize)?
        };
        
        // Get right operand
        let right = if c_is_const {
            self.get_constant(c_idx as usize)?
        } else {
            self.heap.get_thread_register(self.current_thread, base as usize + c_idx as usize)?
        };
        
        // Try standard arithmetic first
        let result = match (&left, &right, op) {
            (Value::Number(l), Value::Number(r), ArithOp::Add) => Ok(Value::Number(l + r)),
            (Value::Number(l), Value::Number(r), ArithOp::Sub) => Ok(Value::Number(l - r)),
            (Value::Number(l), Value::Number(r), ArithOp::Mul) => Ok(Value::Number(l * r)),
            (Value::Number(l), Value::Number(r), ArithOp::Div) => {
                if *r == 0.0 {
                    Err(LuaError::RuntimeError("Division by zero".to_string()))
                } else {
                    Ok(Value::Number(l / r))
                }
            }
            (Value::Number(l), Value::Number(r), ArithOp::Mod) => {
                if *r == 0.0 {
                    Err(LuaError::RuntimeError("Modulo by zero".to_string()))
                } else {
                    Ok(Value::Number(l % r))
                }
            }
            (Value::Number(l), Value::Number(r), ArithOp::Pow) => Ok(Value::Number(l.powf(*r))),
            _ => {
                // Check for metamethods
                let metamethod_name = match op {
                    ArithOp::Add => "__add",
                    ArithOp::Sub => "__sub",
                    ArithOp::Mul => "__mul",
                    ArithOp::Div => "__div",
                    ArithOp::Mod => "__mod",
                    ArithOp::Pow => "__pow",
                };
                
                eprintln!("DEBUG op_arithmetic: Checking for {} metamethod", metamethod_name);
                
                // First check left operand
                let metamethod = if let Value::Table(handle) = &left {
                    if let Some(mt) = self.heap.get_table_metatable(*handle)? {
                        let mm_key = self.heap.create_string(metamethod_name)?;
                        self.heap.get_table_field(mt, &Value::String(mm_key))?
                    } else {
                        Value::Nil
                    }
                } else {
                    Value::Nil
                };
                
                // If not found, check right operand
                let metamethod = if metamethod.is_nil() {
                    if let Value::Table(handle) = &right {
                        if let Some(mt) = self.heap.get_table_metatable(*handle)? {
                            let mm_key = self.heap.create_string(metamethod_name)?;
                            self.heap.get_table_field(mt, &Value::String(mm_key))?
                        } else {
                            Value::Nil
                        }
                    } else {
                        Value::Nil
                    }
                } else {
                    metamethod
                };
                
                if !metamethod.is_nil() {
                    eprintln!("DEBUG op_arithmetic: Found {} metamethod", metamethod_name);
                    let args = vec![left.clone(), right.clone()];
                    let results = self.call_metamethod(metamethod, &args, 1)?;
                    Ok(results.get(0).cloned().unwrap_or(Value::Nil))
                } else {
                    Err(LuaError::TypeError {
                        expected: "number".to_string(),
                        got: format!("{} and {}", left.type_name(), right.type_name()),
                    })
                }
            }
        }?;
        
        self.heap.set_thread_register(self.current_thread, base as usize + a, &result)?;
        Ok(())
    }
    
    /// Get constant from current function
    fn get_constant(&mut self, index: usize) -> LuaResult<Value> {
        let frame = self.heap.get_current_frame(self.current_thread)?;
        let closure = self.heap.get_closure(frame.closure)?;
        
        eprintln!("DEBUG get_constant: Getting constant at index {} from closure with {} constants", 
                 index, closure.proto.constants.len());
        
        if index >= closure.proto.constants.len() {
            return Err(LuaError::RuntimeError(format!(
                "Constant index {} out of bounds", index
            )));
        }
        
        let constant = closure.proto.constants[index].clone();
        eprintln!("DEBUG get_constant: Constant[{}] = {:?}", index, constant);
        
        // Extra debug for number constants
        if let Value::Number(n) = &constant {
            eprintln!("DEBUG get_constant: Number constant: {} (bits: {:016x})", n, n.to_bits());
        }
        
        Ok(constant)
    }

    /// EQ: if ((RK(B) == RK(C)) ~= A) then pc++
    fn op_eq(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        self.op_comparison(inst, base, CompOp::Eq)
    }
    
    /// LT: if ((RK(B) < RK(C)) ~= A) then pc++
    fn op_lt(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        self.op_comparison(inst, base, CompOp::Lt)
    }
    
    /// LE: if ((RK(B) <= RK(C)) ~= A) then pc++
    fn op_le(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        self.op_comparison(inst, base, CompOp::Le)
    }
    
    /// Generic comparison operation handler
    fn op_comparison(&mut self, inst: Instruction, base: u16, op: CompOp) -> LuaResult<()> {
        let a = inst.get_a();
        let (b_is_const, b_idx) = inst.get_rk_b();
        let (c_is_const, c_idx) = inst.get_rk_c();
        
        eprintln!("DEBUG op_comparison: op={:?}, A={}, B_is_const={}, B_idx={}, C_is_const={}, C_idx={}", 
                 op, a, b_is_const, b_idx, c_is_const, c_idx);
        
        // Get operands
        let left = if b_is_const {
            let val = self.get_constant(b_idx as usize)?;
            eprintln!("DEBUG op_comparison: Left operand from constant[{}]: {:?}", b_idx, val);
            val
        } else {
            let reg_idx = base as usize + b_idx as usize;
            let val = self.heap.get_thread_register(self.current_thread, reg_idx)?;
            eprintln!("DEBUG op_comparison: Left operand from R[{}]: {:?}", reg_idx, val);
            val
        };
        
        let right = if c_is_const {
            let val = self.get_constant(c_idx as usize)?;
            eprintln!("DEBUG op_comparison: Right operand from constant[{}]: {:?}", c_idx, val);
            val
        } else {
            let reg_idx = base as usize + c_idx as usize;
            let val = self.heap.get_thread_register(self.current_thread, reg_idx)?;
            eprintln!("DEBUG op_comparison: Right operand from R[{}]: {:?}", reg_idx, val);
            val
        };
        
        eprintln!("DEBUG op_comparison: Comparing {:?} {:?} {:?}", left, op, right);
        
        // Perform comparison
        let result = match op {
            CompOp::Eq => {
                let eq_result = self.compare_eq(&left, &right);
                eprintln!("DEBUG op_comparison: Equality result: {}", eq_result);
                
                // Extra debug for number comparisons
                if let (Value::Number(l), Value::Number(r)) = (&left, &right) {
                    eprintln!("DEBUG op_comparison: Number comparison details:");
                    eprintln!("  Left:  {} (bits: {:016x})", l, l.to_bits());
                    eprintln!("  Right: {} (bits: {:016x})", r, r.to_bits());
                    eprintln!("  l == r: {}", l == r);
                    eprintln!("  l.to_bits() == r.to_bits(): {}", l.to_bits() == r.to_bits());
                }
                
                eq_result
            },
            CompOp::Lt => self.compare_lt(&left, &right)?,
            CompOp::Le => self.compare_le(&left, &right)?,
        };
        
        // The Lua comparison opcodes implement: if ((RK(B) op RK(C)) ~= A) then pc++
        // This means:
        // - If A=0 and comparison is true, skip next instruction
        // - If A=0 and comparison is false, continue to next instruction  
        // - If A=1 and comparison is true, continue to next instruction
        // - If A=1 and comparison is false, skip next instruction
        //
        // In other words, A inverts the sense of the test.
        
        // Calculate whether to skip based on the comparison result and A flag
        let a_bool = a != 0;
        let should_skip = result != a_bool;
        
        eprintln!("DEBUG op_comparison: result={}, a={}, a_bool={}, should_skip={}", 
                 result, a, a_bool, should_skip);
        eprintln!("DEBUG op_comparison: Detailed skip logic: {} != {} = {}", 
                 result, a_bool, should_skip);
        
        if should_skip {
            let pc = self.heap.get_pc(self.current_thread)?;
            let new_pc = pc + 1;
            eprintln!("DEBUG op_comparison: Skipping next instruction, PC {} -> {}", pc, new_pc);
            self.heap.set_pc(self.current_thread, new_pc)?;
        } else {
            eprintln!("DEBUG op_comparison: Not skipping, continuing to next instruction");
        }
        
        Ok(())
    }
    
    /// Compare for equality
    fn compare_eq(&mut self, left: &Value, right: &Value) -> bool {
        eprintln!("DEBUG compare_eq: Comparing {:?} == {:?}", left, right);
        
        let result = match (left, right) {
            (Value::Nil, Value::Nil) => true,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => {
                // Lua 5.1 numeric equality semantics:
                // - All numbers in Lua 5.1 are double-precision floating-point
                // - Comparison is by mathematical value, not bit representation
                // - This means 1.0 and 1 should compare as equal
                // 
                // Standard f64 == comparison already provides the correct semantics
                // for Lua, as it compares by value rather than bit pattern.
                // For example: 1.0_f64 == 1.0_f64 returns true even if they
                // were created through different code paths.
                //
                // Note: If OrderedFloat or similar wrappers are used elsewhere,
                // they must preserve these semantics.
                let eq = a == b;
                eprintln!("DEBUG compare_eq: Number comparison: {} == {} = {}", a, b, eq);
                eq
            }
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Table(a), Value::Table(b)) => {
                // First check if they're the same table
                if a == b {
                    true
                } else {
                    // Check for __eq metamethod (only if both have same metamethod)
                    let mt_a = self.heap.get_table_metatable(*a).ok().flatten();
                    let mt_b = self.heap.get_table_metatable(*b).ok().flatten();
                    
                    if let (Some(mt_a), Some(mt_b)) = (mt_a, mt_b) {
                        if mt_a == mt_b {
                            // Both have the same metatable, check for __eq
                            let eq_key = self.heap.create_string("__eq").ok();
                            if let Some(eq_key) = eq_key {
                                let mm = self.heap.get_table_field(mt_a, &Value::String(eq_key)).unwrap_or(Value::Nil);
                                if !mm.is_nil() {
                                    // Call __eq metamethod
                                    eprintln!("DEBUG compare_eq: Calling __eq metamethod");
                                    let args = vec![Value::Table(*a), Value::Table(*b)];
                                    if let Ok(results) = self.call_metamethod(mm, &args, 1) {
                                        return !results.get(0).cloned().unwrap_or(Value::Nil).is_falsey();
                                    }
                                }
                            }
                        }
                    }
                    false
                }
            }
            (Value::Closure(a), Value::Closure(b)) => a == b,
            _ => {
                eprintln!("DEBUG compare_eq: Different types, returning false");
                false
            }
        };
        
        eprintln!("DEBUG compare_eq: Result = {}", result);
        result
    }
    
    /// Compare for less than
    fn compare_lt(&mut self, left: &Value, right: &Value) -> LuaResult<bool> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                // Lua 5.1 numeric comparison: standard floating-point less-than
                // This correctly handles cases like 0.9999999999999999 < 1.0
                Ok(a < b)
            }
            (Value::String(a), Value::String(b)) => {
                let s1 = self.heap.get_string_value(*a)?;
                let s2 = self.heap.get_string_value(*b)?;
                Ok(s1 < s2)
            }
            _ => Err(LuaError::TypeError {
                expected: "number or string".to_string(),
                got: format!("{} and {}", left.type_name(), right.type_name()),
            }),
        }
    }
    
    /// Compare for less than or equal
    fn compare_le(&mut self, left: &Value, right: &Value) -> LuaResult<bool> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                // Lua 5.1 numeric comparison: standard floating-point less-than-or-equal
                // This correctly handles cases like 1.0 <= 1.0
                Ok(a <= b)
            }
            (Value::String(a), Value::String(b)) => {
                let s1 = self.heap.get_string_value(*a)?;
                let s2 = self.heap.get_string_value(*b)?;
                Ok(s1 <= s2)
            }
            _ => Err(LuaError::TypeError {
                expected: "number or string".to_string(),
                got: format!("{} and {}", left.type_name(), right.type_name()),
            }),
        }
    }

    /// NOT: R(A) := not R(B)
    fn op_not(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        let source_register = base as usize + b;
        let value = self.heap.get_thread_register(self.current_thread, source_register)?;
        let result = Value::Boolean(value.is_falsey());
        
        self.heap.set_thread_register(self.current_thread, base as usize + a, &result)?;
        Ok(())
    }
    
    /// LOADBOOL: R(A) := (Bool)B; if (C) pc++
    fn op_loadbool(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b();
        let c = inst.get_c();
        
        let value = Value::Boolean(b != 0);
        self.heap.set_thread_register(self.current_thread, base as usize + a, &value)?;
        
        if c != 0 {
            // Skip next instruction
            let pc = self.heap.get_pc(self.current_thread)?;
            self.heap.set_pc(self.current_thread, pc + 1)?;
        }
        
        Ok(())
    }
    
    /// TESTSET: if (R(B) <=> C) then R(A) := R(B) else pc++
    fn op_testset(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        let c = inst.get_c();
        
        let source_register = base as usize + b;
        let value = self.heap.get_thread_register(self.current_thread, source_register)?;
        let test = !value.is_falsey();
        
        if test == (c != 0) {
            self.heap.set_thread_register(self.current_thread, base as usize + a, &value)?;
        } else {
            let pc = self.heap.get_pc(self.current_thread)?;
            self.heap.set_pc(self.current_thread, pc + 1)?;
        }
        
        Ok(())
    }

    /// NEWTABLE: R(A) := {} (size = B,C)
    fn op_newtable(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b(); // Array size hint
        let c = inst.get_c(); // Hash size hint
        
        let dest_register = base as usize + a;
        eprintln!("DEBUG op_newtable: A={}, B={}, C={}, base={}", a, b, c, base);
        eprintln!("DEBUG op_newtable: Creating new table at R({}) (base {} + A {})", 
                 dest_register, base, a);
        
        let table = self.heap.create_table()?;
        let table_value = Value::Table(table);
        eprintln!("DEBUG op_newtable: Created table handle {:?}", table);
        
        // Show register state before setting
        eprintln!("DEBUG op_newtable: Register state before table creation:");
        let start = dest_register.saturating_sub(2);
        let end = (dest_register + 2).min(self.heap.get_stack_size(self.current_thread).unwrap_or(0));
        for i in start..=end {
            if let Ok(val) = self.heap.get_thread_register(self.current_thread, i) {
                let marker = if i == dest_register { " <-- Will place table here" } else { "" };
                eprintln!("  R({}) = {:?}{}", i, val, marker);
            }
        }
        
        self.heap.set_thread_register(self.current_thread, dest_register, &table_value)?;
        
        // Verify it was stored correctly and show updated state
        let verify = self.heap.get_thread_register(self.current_thread, dest_register)?;
        eprintln!("DEBUG op_newtable: Verification - R({}) now contains: {:?}", dest_register, verify);
        eprintln!("DEBUG op_newtable: ***TABLE CREATED*** at R({})", dest_register);
        
        Ok(())
    }
    
    /// GETTABLE: R(A) := R(B)[RK(C)]
    fn op_gettable(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        let (c_is_const, c_idx) = inst.get_rk_c();
        
        eprintln!("DEBUG op_gettable: A={}, B={}, C_is_const={}, C_idx={}, base={}", 
                 a, b, c_is_const, c_idx, base);
        
        // Get table directly from register B as per Lua 5.1 standard
        let table_register = base as usize + b;
        eprintln!("DEBUG op_gettable: Looking for table at R({})", table_register);
        
        // Debug: Show register state around the expected table location
        eprintln!("DEBUG op_gettable: Register state:");
        let start = table_register.saturating_sub(2);
        let end = (table_register + 2).min(self.heap.get_stack_size(self.current_thread).unwrap_or(0));
        for i in start..=end {
            if let Ok(val) = self.heap.get_thread_register(self.current_thread, i) {
                let marker = if i == table_register { " <-- Expected table location" } else { "" };
                eprintln!("  R({}) = {:?}{}", i, val, marker);
            }
        }
        
        let table_val = self.heap.get_thread_register(self.current_thread, table_register)?;
        eprintln!("DEBUG op_gettable: Value at R({}) is: {:?}", table_register, table_val);
        
        let table_handle = match table_val {
            Value::Table(handle) => handle,
            _ => {
                eprintln!("ERROR op_gettable: Expected table at R({}), but got {:?}", 
                         table_register, table_val);
                return Err(LuaError::TypeError {
                    expected: "table".to_string(),
                    got: table_val.type_name().to_string(),
                });
            }
        };
        
        // Get key
        let key = if c_is_const {
            let k = self.get_constant(c_idx as usize)?;
            eprintln!("DEBUG op_gettable: Key from constant[{}]: {:?}", c_idx, k);
            k
        } else {
            let key_register = base as usize + c_idx as usize;
            let k = self.heap.get_thread_register(self.current_thread, key_register)?;
            eprintln!("DEBUG op_gettable: Key from R({}): {:?}", key_register, k);
            k
        };
        
        // First try to get the field directly
        let value = self.heap.get_table_field(table_handle, &key)?;
        eprintln!("DEBUG op_gettable: Direct table lookup returned: {:?}", value);
        
        let final_value = if value.is_nil() {
            // Check for __index metamethod
            if let Some(metatable) = self.heap.get_table_metatable(table_handle)? {
                eprintln!("DEBUG op_gettable: Table has metatable, checking for __index");
                
                let index_key = self.heap.create_string("__index")?;
                let index_metamethod = self.heap.get_table_field(metatable, &Value::String(index_key))?;
                
                match index_metamethod {
                    Value::Table(index_table) => {
                        // __index is a table, look up the key in it
                        eprintln!("DEBUG op_gettable: __index is a table, looking up key");
                        let indexed_value = self.heap.get_table_field(index_table, &key)?;
                        eprintln!("DEBUG op_gettable: __index table lookup returned: {:?}", indexed_value);
                        indexed_value
                    }
                    Value::Closure(_) | Value::CFunction(_) => {
                        // __index is a function - call it with (table, key)
                        eprintln!("DEBUG op_gettable: __index is a function, calling it");
                        let args = vec![Value::Table(table_handle), key.clone()];
                        let results = self.call_metamethod(index_metamethod, &args, 1)?;
                        if results.is_empty() {
                            Value::Nil
                        } else {
                            results[0].clone()
                        }
                    }
                    _ => {
                        eprintln!("DEBUG op_gettable: No valid __index metamethod found");
                        Value::Nil
                    }
                }
            } else {
                eprintln!("DEBUG op_gettable: No metatable, returning nil");
                Value::Nil
            }
        } else {
            value
        };
        
        let dest_register = base as usize + a;
        eprintln!("DEBUG op_gettable: Storing final value {:?} in R({})", 
                 final_value, dest_register);
        
        self.heap.set_thread_register(self.current_thread, dest_register, &final_value)?;
        
        Ok(())
    }
    
    /// SETTABLE: R(A)[RK(B)] := RK(C)
    fn op_settable(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let (b_is_const, b_idx) = inst.get_rk_b();
        let (c_is_const, c_idx) = inst.get_rk_c();
        
        eprintln!("DEBUG op_settable: A={}, B_is_const={}, B_idx={}, C_is_const={}, C_idx={}, base={}", 
                 a, b_is_const, b_idx, c_is_const, c_idx, base);
        
        // Get table
        let table_register = base as usize + a;
        eprintln!("DEBUG op_settable: Reading table from R({})", table_register);
        
        let table_val = self.heap.get_thread_register(self.current_thread, table_register)?;
        eprintln!("DEBUG op_settable: Value at R({}) is: {:?}", table_register, table_val);
        
        let table_handle = match table_val {
            Value::Table(handle) => {
                eprintln!("DEBUG op_settable: Got table handle: {:?}", handle);
                handle
            },
            _ => {
                eprintln!("ERROR op_settable: Expected table at R({}), but got {:?}", 
                         table_register, table_val);
                return Err(LuaError::TypeError {
                    expected: "table".to_string(),
                    got: table_val.type_name().to_string(),
                });
            }
        };
        
        // Get key
        let key = if b_is_const {
            let k = self.get_constant(b_idx as usize)?;
            eprintln!("DEBUG op_settable: Key from constant[{}]: {:?}", b_idx, k);
            k
        } else {
            let key_register = base as usize + b_idx as usize;
            let k = self.heap.get_thread_register(self.current_thread, key_register)?;
            eprintln!("DEBUG op_settable: Key from R({}): {:?}", key_register, k);
            k
        };
        
        // Get value
        let value = if c_is_const {
            let v = self.get_constant(c_idx as usize)?;
            eprintln!("DEBUG op_settable: Value from constant[{}]: {:?}", c_idx, v);
            v
        } else {
            let value_register = base as usize + c_idx as usize;
            let v = self.heap.get_thread_register(self.current_thread, value_register)?;
            eprintln!("DEBUG op_settable: Value from R({}): {:?}", value_register, v);
            v
        };
        
        eprintln!("DEBUG op_settable: *** SETTABLE OPERATION: table[{:?}] = {:?} ***", key, value);
        
        // Check if table has a metatable
        let has_metatable = if let Some(_) = self.heap.get_table_metatable(table_handle)? {
            eprintln!("DEBUG op_settable: Table HAS metatable");
            true
        } else {
            eprintln!("DEBUG op_settable: Table has NO metatable");
            false
        };
        
        // Check if the key already exists in the table
        eprintln!("DEBUG op_settable: Checking if key exists in table...");
        let existing_value = self.heap.get_table_field(table_handle, &key)?;
        eprintln!("DEBUG op_settable: Existing value for key: {:?}", existing_value);
        
        if existing_value.is_nil() {
            eprintln!("DEBUG op_settable: Key does NOT exist in table (value is nil)");
            
            // Key doesn't exist, check for __newindex metamethod
            if has_metatable {
                if let Some(metatable) = self.heap.get_table_metatable(table_handle)? {
                    eprintln!("DEBUG op_settable: Checking metatable for __newindex metamethod...");
                    
                    let newindex_key = self.heap.create_string("__newindex")?;
                    eprintln!("DEBUG op_settable: Looking up __newindex in metatable");
                    let newindex_metamethod = self.heap.get_table_field(metatable, &Value::String(newindex_key))?;
                    eprintln!("DEBUG op_settable: __newindex metamethod value: {:?}", newindex_metamethod);
                    
                    match newindex_metamethod {
                        Value::Table(newindex_table) => {
                            // __newindex is a table, set the key in it
                            eprintln!("DEBUG op_settable: __newindex is a TABLE, setting key there");
                            eprintln!("DEBUG op_settable: Calling set_table_field on __newindex table");
                            self.heap.set_table_field(newindex_table, &key, &value)?;
                            eprintln!("DEBUG op_settable: Successfully set value in __newindex table");
                            return Ok(());
                        }
                        Value::Closure(_) | Value::CFunction(_) => {
                            // __newindex is a function - call it with (table, key, value)
                            eprintln!("DEBUG op_settable: __newindex is a FUNCTION, calling it");
                            eprintln!("DEBUG op_settable: Preparing to call __newindex(table, key, value)");
                            let args = vec![Value::Table(table_handle), key.clone(), value.clone()];
                            eprintln!("DEBUG op_settable: __newindex args: {:?}", args);
                            self.call_metamethod(newindex_metamethod, &args, 0)?;
                            eprintln!("DEBUG op_settable: __newindex function call completed");
                            return Ok(());
                        }
                        Value::Nil => {
                            eprintln!("DEBUG op_settable: __newindex is NIL, proceeding with normal set");
                        }
                        _ => {
                            // __newindex exists but is not a valid type
                            eprintln!("DEBUG op_settable: __newindex exists but has invalid type: {:?}", 
                                    newindex_metamethod.type_name());
                            eprintln!("DEBUG op_settable: Ignoring invalid __newindex, proceeding with normal set");
                        }
                    }
                }
            }
            
            // No metatable or no valid __newindex, proceed with normal set
            eprintln!("DEBUG op_settable: No __newindex metamethod, doing RAW SET");
            eprintln!("DEBUG op_settable: Calling heap.set_table_field (RAW operation)");
            self.heap.set_table_field(table_handle, &key, &value)?;
            eprintln!("DEBUG op_settable: RAW SET completed");
        } else {
            // Key exists, just update it (no metamethod triggered)
            eprintln!("DEBUG op_settable: Key EXISTS in table, updating directly (NO metamethod)");
            eprintln!("DEBUG op_settable: Calling heap.set_table_field for existing key (RAW operation)");
            self.heap.set_table_field(table_handle, &key, &value)?;
            eprintln!("DEBUG op_settable: RAW UPDATE completed");
        }
        
        eprintln!("DEBUG op_settable: *** SETTABLE OPERATION COMPLETE ***");
        Ok(())
    }
    
    /// SETLIST: R(A)[(C-1)*FPF+i] := R(A+i), 1 <= i <= B
    fn op_setlist(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        let c = inst.get_c() as usize;
        
        // Fields per flush (Lua 5.1 uses 50)
        const FIELDS_PER_FLUSH: usize = 50;
        
        // Get the table
        let table_val = self.heap.get_thread_register(self.current_thread, base as usize + a)?;
        let table_handle = match table_val {
            Value::Table(h) => h,
            _ => return Err(LuaError::TypeError {
                expected: "table".to_string(),
                got: table_val.type_name().to_string(),
            }),
        };
        
        // Calculate base index in table
        let table_base = if c == 0 {
            0 // Special case handled differently in real implementation
        } else {
            (c - 1) * FIELDS_PER_FLUSH
        };
        
        // Number of elements to set
        let count = if b == 0 {
            // Set all values from R(A+1) to top
            let stack_size = self.heap.get_stack_size(self.current_thread)?;
            stack_size.saturating_sub(base as usize + a + 1)
        } else {
            b
        };
        
        // Set elements
        for i in 0..count {
            let source_register = base as usize + a + 1 + i;
            let value = self.heap.get_thread_register(self.current_thread, source_register)?;
            let array_index = table_base + i + 1; // 1-based indexing
            
            self.heap.set_table_field(table_handle, &Value::Number(array_index as f64), &value)?;
        }
        
        Ok(())
    }

    /// GETGLOBAL: R(A) := Gbl[Kst(Bx)]
    fn op_getglobal(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let bx = inst.get_bx() as usize;
        
        // Get constant name
        let key = self.get_constant(bx)?;
        
        eprintln!("DEBUG op_getglobal: Getting global '{}'", 
                 if let Value::String(s) = &key { 
                     self.heap.get_string_value(*s).unwrap_or_else(|_| "<error>".to_string())
                 } else { 
                     "<non-string key>".to_string() 
                 });
        
        // Get globals table
        let globals = self.heap.globals()?;
        
        // Get value from globals
        let value = self.heap.get_table_field(globals, &key)?;
        
        eprintln!("DEBUG op_getglobal: Retrieved value: {:?}, storing in R({})", 
                 value, base as usize + a);
        
        self.heap.set_thread_register(self.current_thread, base as usize + a, &value)?;
        
        Ok(())
    }
    
    /// SETGLOBAL: Gbl[Kst(Bx)] := R(A)
    fn op_setglobal(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let bx = inst.get_bx() as usize;
        
        // Get value to set
        let value = self.heap.get_thread_register(self.current_thread, base as usize + a)?;
        
        // Get constant name
        let key = self.get_constant(bx)?;
        
        eprintln!("DEBUG op_setglobal: Setting global '{}' = {:?}", 
                 if let Value::String(s) = &key { 
                     self.heap.get_string_value(*s).unwrap_or_else(|_| "<error>".to_string())
                 } else { 
                     "<non-string key>".to_string() 
                 }, 
                 value);
        
        // Get globals table
        let globals = self.heap.globals()?;
        
        // Set value in globals
        self.heap.set_table_field(globals, &key, &value)?;
        
        Ok(())
    }
    
    /// LEN: R(A) := length of R(B)
    fn op_len(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        eprintln!("DEBUG op_len: A={}, B={}, base={}", a, b, base);
        
        // Access the operand directly from R(B) as per Lua 5.1 standard
        let source_register = base as usize + b;
        let dest_register = base as usize + a;
        
        eprintln!("DEBUG op_len: Reading from R({}) to get length", source_register);
        
        // Debug register state
        eprintln!("DEBUG op_len: Register state:");
        let start = source_register.saturating_sub(2);
        let end = (source_register + 2).min(self.heap.get_stack_size(self.current_thread).unwrap_or(0));
        for i in start..=end {
            if let Ok(val) = self.heap.get_thread_register(self.current_thread, i) {
                let marker = if i == source_register { " <-- Source" } else { "" };
                eprintln!("  R({}) = {:?}{}", i, val, marker);
            }
        }
        
        let value = self.heap.get_thread_register(self.current_thread, source_register)?;
        eprintln!("DEBUG op_len: Source value at R({}): {:?}", source_register, value);
        
        let length = match &value {
            Value::String(handle) => {
                // Get string value length
                let s = self.heap.get_string_value(*handle)?;
                let len = s.len() as f64;
                eprintln!("DEBUG op_len: String length: {}", len);
                Value::Number(len)
            }
            Value::Table(handle) => {
                // Check for __len metamethod
                if let Some(metatable) = self.heap.get_table_metatable(*handle)? {
                    let len_key = self.heap.create_string("__len")?;
                    let len_metamethod = self.heap.get_table_field(metatable, &Value::String(len_key))?;
                    
                    if !len_metamethod.is_nil() {
                        eprintln!("DEBUG op_len: Found __len metamethod");
                        let args = vec![value.clone()];
                        let results = self.call_metamethod(len_metamethod, &args, 1)?;
                        results.get(0).cloned().unwrap_or(Value::Nil)
                    } else {
                        // No __len metamethod, use default behavior
                        let table = self.heap.get_table(*handle)?;
                        let array_len = table.array.len();
                        eprintln!("DEBUG op_len: Table array length: {}", array_len);
                        Value::Number(array_len as f64)
                    }
                } else {
                    // No metatable, use default behavior
                    let table = self.heap.get_table(*handle)?;
                    let array_len = table.array.len();
                    eprintln!("DEBUG op_len: Table array length: {}", array_len);
                    Value::Number(array_len as f64)
                }
            }
            _ => {
                eprintln!("ERROR op_len: Cannot get length of {:?}", value);
                return Err(LuaError::TypeError {
                    expected: "string or table".to_string(),
                    got: value.type_name().to_string(),
                });
            }
        };
        
        eprintln!("DEBUG op_len: Storing length {} in R({})", 
                 if let Value::Number(n) = length { n } else { 0.0 }, 
                 dest_register);
        
        self.heap.set_thread_register(self.current_thread, dest_register, &length)?;
        
        Ok(())
    }
    
    /// CONCAT: R(A) := R(B).. ... ..R(C)
    fn op_concat(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        let c = inst.get_c() as usize;
        
        let mut result = String::new();
        
        // Collect all values to concatenate
        for i in b..=c {
            let value = self.heap.get_thread_register(self.current_thread, base as usize + i)?;
            match value {
                Value::String(handle) => {
                    let s = self.heap.get_string_value(handle)?;
                    result.push_str(&s);
                }
                Value::Number(n) => {
                    result.push_str(&n.to_string());
                }
                _ => return Err(LuaError::TypeError {
                    expected: "string or number".to_string(),
                    got: value.type_name().to_string(),
                }),
            }
        }
        
        // Create result string
        let handle = self.heap.create_string(&result)?;
        self.heap.set_thread_register(self.current_thread, base as usize + a, &Value::String(handle))?;
        
        Ok(())
    }

    /// SELF: R(A+1) := R(B); R(A) := R(B)[RK(C)]
    fn op_self(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        let (c_is_const, c_idx) = inst.get_rk_c();
        
        // Get the table from R(B)
        let table_val = self.heap.get_thread_register(self.current_thread, base as usize + b)?;
        
        // Get the method name key
        let key = if c_is_const {
            self.get_constant(c_idx as usize)?
        } else {
            self.heap.get_thread_register(self.current_thread, base as usize + c_idx as usize)?
        };
        
        // Verify we have a table
        match &table_val {
            Value::Table(handle) => {
                // Look up the method on the table
                let method = self.heap.get_table_field(*handle, &key)?;
                
                // SELF operation: R(A+1) := R(B); R(A) := R(B)[RK(C)]
                self.heap.set_thread_register(self.current_thread, base as usize + a, &method)?;
                self.heap.set_thread_register(self.current_thread, base as usize + a + 1, &table_val)?;
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
    
    /// TAILCALL: return R(A)(R(A+1), ..., R(A+B-1))
    fn op_tailcall(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        // For simplicity in RefCellVM, implement as regular call + return
        eprintln!("DEBUG: TAILCALL implemented as regular call in RefCellVM");
        self.op_call(inst, base)?;
        
        // Set a flag that next return should be a tail return
        // For now, just return normally
        Ok(())
    }
    
    /// TFORLOOP: R(A+3), R(A+4), ... ,R(A+2+C) := R(A)(R(A+1), R(A+2));
    ///           if R(A+3) ~= nil then R(A+2) := R(A+3) else pc++
    fn op_tforloop(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let c = inst.get_c() as usize;
        
        eprintln!("DEBUG TFORLOOP: Beginning - A={}, C={}, base={}", a, c, base);
        
        // Per the Lua 5.1 specification:
        // R(A)   = iterator function
        // R(A+1) = state
        // R(A+2) = control variable
        // Call the function, placing results at R(A+3), R(A+4), ...
        
        let func_idx = base as usize + a;
        
        // Get the iterator function, state, and control variables
        let func = self.heap.get_thread_register(self.current_thread, func_idx)?;
        let state = self.heap.get_thread_register(self.current_thread, func_idx + 1)?;
        let control = self.heap.get_thread_register(self.current_thread, func_idx + 2)?;
        
        eprintln!("DEBUG TFORLOOP: Function={:?}, State={:?}, Control={:?}", func, state, control);
        
        // We need to call the iterator function with the state and control
        // For C functions, directly call them with a special context
        if let Value::CFunction(cfunc) = func {
            // Create an execution context that places results at R(A+3) onwards
            // This is CRITICAL: we must set up registers so results don't overwrite the state
            
            // First, we'll create temporary copies of the arguments to avoid overwriting them
            let stack_size = self.heap.get_stack_size(self.current_thread)?;
            let temp_base = stack_size;
            
            // Ensure we have enough stack space
            self.heap.grow_stack(self.current_thread, temp_base + 3)?;
            
            // Place arguments at the temporary location
            self.heap.set_thread_register(self.current_thread, temp_base, &func)?;
            self.heap.set_thread_register(self.current_thread, temp_base + 1, &state)?;
            self.heap.set_thread_register(self.current_thread, temp_base + 2, &control)?;
            
            // Create a context for the C function call
            let mut ctx = RefCellExecutionContext::new(&self.heap, self.current_thread, temp_base as u16, 2);
            
            // Call the function
            let result_count = cfunc(&mut ctx)?;
            
            eprintln!("DEBUG TFORLOOP: C iterator returned {} results", result_count);
            
            // Get the first result from the temporary area
            let first_result = if result_count > 0 {
                self.heap.get_thread_register(self.current_thread, temp_base)?
            } else {
                Value::Nil
            };
            
            eprintln!("DEBUG TFORLOOP: First result = {:?}", first_result);
            
            if first_result.is_nil() {
                // End of iteration - skip the next instruction (JMP)
                eprintln!("DEBUG TFORLOOP: Nil result - ending loop");
                let pc = self.heap.get_pc(self.current_thread)?;
                self.heap.set_pc(self.current_thread, pc + 1)?;
                return Ok(());
            }
            
            // Continue iteration - update control variable
            eprintln!("DEBUG TFORLOOP: Non-nil result - continuing loop");
            self.heap.set_thread_register(self.current_thread, func_idx + 2, &first_result)?;
            
            // Copy results from the temporary location to the loop variables
            for i in 0..c.min(result_count as usize) {
                let value = self.heap.get_thread_register(self.current_thread, temp_base + i)?;
                self.heap.set_thread_register(self.current_thread, func_idx + 3 + i, &value)?;
                eprintln!("DEBUG TFORLOOP: Placed result {} at R({})", i, func_idx + 3 + i);
            }
            
            // Fill any remaining expected results with nil
            for i in result_count as usize..c {
                self.heap.set_thread_register(self.current_thread, func_idx + 3 + i, &Value::Nil)?;
                eprintln!("DEBUG TFORLOOP: Set nil at R({})", func_idx + 3 + i);
            }
            
            return Ok(());
        } 
        // For Lua closures, use the operation queue
        else if let Value::Closure(closure_handle) = func {
            eprintln!("DEBUG TFORLOOP: Lua closure iterator - queueing call");
            
            // We'll create a temporary set of registers beyond the current stack
            let stack_size = self.heap.get_stack_size(self.current_thread)?;
            let temp_base = stack_size;
            
            // Ensure we have enough stack space
            self.heap.grow_stack(self.current_thread, temp_base + 3)?;
            
            // Place the function and arguments at the temporary location
            self.heap.set_thread_register(self.current_thread, temp_base, &func)?;
            self.heap.set_thread_register(self.current_thread, temp_base + 1, &state)?;
            self.heap.set_thread_register(self.current_thread, temp_base + 2, &control)?;
            
            // Queue the function call at the temporary location
            self.operation_queue.push_back(PendingOperation::FunctionCall {
                func_index: temp_base,
                nargs: 2,  // state and control
                expected_results: c as i32,
            });
            
            // Queue a continuation to handle the function results
            self.operation_queue.push_back(PendingOperation::TForLoopContinuation {
                base: base as usize,
                a,
                c,
                pc_before_call: self.heap.get_pc(self.current_thread)? - 1,  // PC of TFORLOOP
                temp_base,
            });
            
            return Ok(());
        } else {
            return Err(LuaError::TypeError {
                expected: "function".to_string(),
                got: func.type_name().to_string(),
            });
        }
    }
    
    /// VARARG: R(A), R(A+1), ..., R(A+B-2) = vararg
    fn op_vararg(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        // Get current frame to access varargs
        let frame = self.heap.get_current_frame(self.current_thread)?;
        
        // Get varargs from frame
        let varargs = frame.varargs.as_ref();
        
        // Determine how many values to copy
        let num_to_copy = match varargs {
            Some(va) => {
                if b == 0 {
                    va.len()
                } else {
                    (b - 1).min(va.len())
                }
            }
            None => 0,
        };
        
        // Copy varargs to destination registers
        for i in 0..num_to_copy {
            let value = if let Some(va) = varargs {
                va[i].clone()
            } else {
                Value::Nil
            };
            
            self.heap.set_thread_register(self.current_thread, base as usize + a + i, &value)?;
        }
        
        // Fill remaining with nil if needed
        if b > 0 {
            for i in num_to_copy..(b - 1) {
                self.heap.set_thread_register(self.current_thread, base as usize + a + i, &Value::Nil)?;
            }
        }
        
        Ok(())
    }
    
    /// GETUPVAL: R(A) := UpValue[B]
    fn op_getupval(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        eprintln!("DEBUG op_getupval: A={}, B={}, base={}", a, b, base);
        
        // Get current closure to access its upvalues
        let frame = self.heap.get_current_frame(self.current_thread)?;
        eprintln!("DEBUG op_getupval: Current frame closure handle: {:?}", frame.closure);
        
        let closure = self.heap.get_closure(frame.closure)?;
        eprintln!("DEBUG op_getupval: Closure has {} upvalues", closure.upvalues.len());
        
        if b >= closure.upvalues.len() {
            return Err(LuaError::RuntimeError(format!(
                "Upvalue index {} out of bounds (closure has {} upvalues)", b, closure.upvalues.len()
            )));
        }
        
        let upvalue_handle = closure.upvalues[b];
        eprintln!("DEBUG op_getupval: Using upvalue handle {:?} at index {}", upvalue_handle, b);
        
        // Get the upvalue value (handles open/closed upvalues transparently)
        let value = self.heap.get_upvalue_value(upvalue_handle, self.current_thread)?;
        
        eprintln!("DEBUG op_getupval: Retrieved value {:?} from upvalue {}, storing in R({})", 
                 value, b, base as usize + a);
        
        // Store in destination register R(A)
        self.heap.set_thread_register(self.current_thread, base as usize + a, &value)?;
        
        Ok(())
    }
    
    /// SETUPVAL: UpValue[A] := R(B)
    fn op_setupval(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;  // Upvalue index
        let b = inst.get_b() as usize;  // Source register
        
        eprintln!("DEBUG op_setupval: A={}, B={}, base={}", a, b, base);
        
        // Get value from register R(B)
        let value = self.heap.get_thread_register(self.current_thread, base as usize + b)?;
        
        eprintln!("DEBUG op_setupval: Setting upvalue {} to value {:?} from R({})", 
                 a, value, base as usize + b);
        
        // Get current closure to access its upvalues
        let frame = self.heap.get_current_frame(self.current_thread)?;
        eprintln!("DEBUG op_setupval: Current frame closure handle: {:?}", frame.closure);
        
        let closure = self.heap.get_closure(frame.closure)?;
        eprintln!("DEBUG op_setupval: Closure has {} upvalues", closure.upvalues.len());
        
        if a >= closure.upvalues.len() {
            return Err(LuaError::RuntimeError(format!(
                "Upvalue index {} out of bounds (closure has {} upvalues)", a, closure.upvalues.len()
            )));
        }
        
        let upvalue_handle = closure.upvalues[a];
        eprintln!("DEBUG op_setupval: Using upvalue handle {:?} at index {}", upvalue_handle, a);
        
        // Set the upvalue value (handles open/closed upvalues transparently)
        self.heap.set_upvalue_value(upvalue_handle, &value, self.current_thread)?;
        
        // Verify the value was set
        let verify_value = self.heap.get_upvalue_value(upvalue_handle, self.current_thread)?;
        eprintln!("DEBUG op_setupval: Verification - upvalue {} now contains {:?}", a, verify_value);
        
        Ok(())
    }

    
    /// CLOSURE: R(A) := closure(KPROTO[Bx], R(A), ... ,R(A+n))
    fn op_closure(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let bx = inst.get_bx() as usize;
        
        eprintln!("DEBUG op_closure: A={}, Bx={}, base={}", a, bx, base);
        
        let target_register = base as usize + a;
        eprintln!("DEBUG op_closure: Will store closure at R({})", target_register);
        
        // Phase 1: Extract all needed data from parent closure
        let (parent_proto_constants, parent_upvalues, parent_base, parent_closure_handle) = {
            let frame = self.heap.get_current_frame(self.current_thread)?;
            let parent_closure = self.heap.get_closure(frame.closure)?;
            
            eprintln!("DEBUG op_closure: Parent closure has {} constants, parent frame base={}", 
                     parent_closure.proto.constants.len(), frame.base_register);
            
            // Clone/extract just the data we need, including the closure handle
            (parent_closure.proto.constants.clone(), 
             parent_closure.upvalues.clone(),
             frame.base_register,
             frame.closure)  // Store the closure handle
        }; // Borrow of closures arena ends here
        
        // Phase 2: Get the function prototype from constants
        if bx >= parent_proto_constants.len() {
            return Err(LuaError::RuntimeError(format!(
                "Function prototype index {} out of bounds", bx
            )));
        }
        
        let proto_handle = match &parent_proto_constants[bx] {
            Value::FunctionProto(handle) => {
                eprintln!("DEBUG op_closure: Found function prototype at constant {}", bx);
                *handle
            },
            other => {
                eprintln!("ERROR op_closure: Expected function prototype at constant {}, got {:?}", bx, other);
                return Err(LuaError::RuntimeError(format!(
                    "Expected function prototype at constant index {}", bx
                )));
            }
        };
        
        let proto = self.heap.get_function_proto_copy(proto_handle)?;
        
        eprintln!("DEBUG op_closure: Function has {} params, {} bytecode instructions, {} upvalues", 
                 proto.num_params, proto.bytecode.len(), proto.upvalues.len());
        
        // Phase 3: Create upvalues for the new closure based on pseudo-instructions
        let mut upvalues = Vec::with_capacity(proto.upvalues.len());
        
        // The number of upvalues is determined by the proto
        let num_upvalues = proto.upvalues.len();
        
        // Get current PC to read pseudo-instructions
        let mut pseudo_pc = self.heap.get_pc(self.current_thread)?;
        eprintln!("DEBUG op_closure: Processing {} upvalues, starting at PC {}", num_upvalues, pseudo_pc);
        
        for i in 0..num_upvalues {
            // Read the pseudo-instruction for this upvalue - use the parent closure handle we saved
            let pseudo_inst = self.heap.get_instruction(parent_closure_handle, pseudo_pc)?;
            let pseudo = Instruction(pseudo_inst);
            eprintln!("DEBUG op_closure: Pseudo-instruction {} at PC {}: opcode={:?}, B={}", 
                     i, pseudo_pc, pseudo.get_opcode(), pseudo.get_b());
            
            let upvalue_handle = match pseudo.get_opcode() {
                OpCode::Move => {
                    // MOVE 0 B 0: upvalue refers to local in enclosing function
                    let local_reg = pseudo.get_b() as usize;
                    let stack_index = parent_base as usize + local_reg;
                    eprintln!("DEBUG op_closure: Upvalue {} is local at register {} (stack index {})", 
                             i, local_reg, stack_index);
                    
                    self.heap.find_or_create_upvalue(self.current_thread, stack_index)?
                }
                OpCode::GetUpval => {
                    // GETUPVAL 0 B 0: upvalue refers to upvalue in enclosing function
                    let parent_upval_idx = pseudo.get_b() as usize;
                    if parent_upval_idx >= parent_upvalues.len() {
                        return Err(LuaError::RuntimeError(format!(
                            "Invalid parent upvalue index {} (parent has {} upvalues)",
                            parent_upval_idx, parent_upvalues.len()
                        )));
                    }
                    eprintln!("DEBUG op_closure: Upvalue {} refers to parent upvalue {}", i, parent_upval_idx);
                    parent_upvalues[parent_upval_idx]
                }
                _ => {
                    return Err(LuaError::RuntimeError(format!(
                        "Invalid pseudo-instruction after CLOSURE: {:?}",
                        pseudo.get_opcode()
                    )));
                }
            };
            
            upvalues.push(upvalue_handle);
            pseudo_pc += 1;
        }
        
        // Update PC to skip pseudo-instructions
        self.heap.set_pc(self.current_thread, pseudo_pc)?;
        eprintln!("DEBUG op_closure: Advanced PC to {} after processing pseudo-instructions", pseudo_pc);
        
        // Phase 4: Create new closure (no active borrows now)
        let closure = Closure {
            proto,
            upvalues,
        };
        
        let closure_handle = self.heap.create_closure(closure)?;
        
        eprintln!("DEBUG op_closure: Created closure handle {:?}, storing in R({})", 
                 closure_handle, target_register);
        
        self.heap.set_thread_register(self.current_thread, target_register, &Value::Closure(closure_handle))?;
        
        // Verify it was stored
        let verify = self.heap.get_thread_register(self.current_thread, target_register)?;
        eprintln!("DEBUG op_closure: Verification - R({}) now contains: {:?}", target_register, verify);
        
        Ok(())
    }
    
    /// CLOSE: close all upvalues >= R(A)
    /// 
    /// This instruction closes all open upvalues that reference stack positions
    /// at or above R(A). This is used when local variables go out of scope
    /// (e.g., at the end of a block) to ensure upvalues capture the correct values.
    /// 
    /// Note: CLOSE is NOT needed before RETURN, as RETURN automatically closes
    /// upvalues for the returning function.
    fn op_close(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let threshold = base as usize + a;
        
        eprintln!("DEBUG op_close: Closing upvalues >= R({}) (absolute stack position: {})", a, threshold);
        
        // Close all open upvalues that reference stack positions >= threshold
        // This ensures that any upvalues pointing to locals that are going out of
        // scope are properly closed and capture their current values
        self.heap.close_thread_upvalues(self.current_thread, threshold)?;
        
        Ok(())
    }
    
    /// Process a C function call
    fn process_c_function_call(
        &mut self,
        function: CFunction,
        base: u16,  
        nargs: usize,
        expected_results: i32,
    ) -> LuaResult<StepResult> {
        eprintln!("DEBUG process_c_function_call: base={}, nargs={}, expected_results={}", base, nargs, expected_results);
        
        // Create execution context for C function
        let mut ctx = RefCellExecutionContext::new(&self.heap, self.current_thread, base, nargs);
        
        // Call the C function with our context
        // The CFunction type expects a trait object, so we pass a mutable reference
        // that implements the ExecutionContext trait
        let actual_results = function(&mut ctx)?;
        
        // Validate result count
        if actual_results < 0 {
            return Err(LuaError::RuntimeError(
                "C function returned negative result count".to_string()
            ));
        }
        
        eprintln!("DEBUG process_c_function_call: C function returned {} results", actual_results);
        
        // Get the actual number of results that were pushed  
        let results_pushed = ctx.get_results_pushed();
        eprintln!("DEBUG process_c_function_call: Context reports {} results pushed", results_pushed);
        
        // Debug: Show what was placed in the registers
        for i in 0..results_pushed {
            let val = self.heap.get_thread_register(self.current_thread, base as usize + i)?;
            eprintln!("DEBUG process_c_function_call: Result {}: {:?} at R({})", i, val, base as usize + i);
        }
        
        // Adjust results to expected count if specified
        if expected_results >= 0 {
            let expected = expected_results as usize;
            eprintln!("DEBUG process_c_function_call: Adjusting result count to expected {}", expected);
            
            if results_pushed < expected {
                // Fill missing results with nil
                eprintln!("DEBUG process_c_function_call: Filling {} missing results with nil", expected - results_pushed);
                for i in results_pushed..expected {
                    self.heap.set_thread_register(self.current_thread, base as usize + i, &Value::Nil)?;
                }
            }
            // Note: We don't trim excess results, they're just ignored
        }
        
        Ok(StepResult::Continue)
    }
    
    /// Execute a compiled module
    pub fn execute_module(&mut self, module: &super::compiler::CompiledModule, args: &[Value]) -> LuaResult<Value> {
        eprintln!("DEBUG execute_module: Loading module with {} args", args.len());
        eprintln!("DEBUG execute_module: Module has {} prototypes", module.prototypes.len());
        
        // Clear any previous state
        self.operation_queue.clear();
        
        // Step 1: Create all string handles
        let mut string_handles = Vec::with_capacity(module.strings.len());
        for s in &module.strings {
            string_handles.push(self.heap.create_string(s)?);
        }
        eprintln!("DEBUG execute_module: Created {} string handles", string_handles.len());
        
        // Step 2: Create all function prototypes (two-pass approach like in loader)
        let mut proto_handles = Vec::with_capacity(module.prototypes.len());
        
        // First pass - create prototypes with placeholder constants
        for (idx, proto) in module.prototypes.iter().enumerate() {
            eprintln!("DEBUG execute_module: Creating prototype {} with {} constants, {} params", 
                     idx, proto.constants.len(), proto.num_params);
            
            // Create temporary constants with Nil for unresolved references
            let mut temp_constants = Vec::with_capacity(proto.constants.len());
            for constant in &proto.constants {
                match constant {
                    CompilationConstant::Nil => temp_constants.push(Value::Nil),
                    CompilationConstant::Boolean(b) => temp_constants.push(Value::Boolean(*b)),
                    CompilationConstant::Number(n) => temp_constants.push(Value::Number(*n)),
                    CompilationConstant::String(idx) => {
                        if *idx < string_handles.len() {
                            temp_constants.push(Value::String(string_handles[*idx]));
                        } else {
                            return Err(LuaError::RuntimeError(format!(
                                "Invalid string index: {}", idx
                            )));
                        }
                    }
                    CompilationConstant::FunctionProto(_) | CompilationConstant::Table(_) => {
                        // Use Nil as placeholder for now
                        temp_constants.push(Value::Nil);
                    }
                }
            }
            
            // Convert upvalues
            let vm_upvalues: Vec<UpvalueInfo> = proto.upvalues.iter().map(|u| UpvalueInfo {
                in_stack: u.in_stack,
                index: u.index,
            }).collect();
            
            // Create temporary prototype
            let temp_proto = FunctionProto {
                bytecode: proto.bytecode.clone(),
                constants: temp_constants,
                num_params: proto.num_params,
                is_vararg: proto.is_vararg,
                max_stack_size: proto.max_stack_size,
                upvalues: vm_upvalues,
            };
            
            proto_handles.push(self.heap.create_function_proto(temp_proto)?);
        }
        
        // Second pass - update FunctionProto and Table constants
        for (i, proto) in module.prototypes.iter().enumerate() {
            let proto_handle = proto_handles[i];
            let mut updated_proto = self.heap.get_function_proto_copy(proto_handle)?;
            
            eprintln!("DEBUG execute_module: Updating prototype {} constants", i);
            
            // Update constants that reference other prototypes or tables
            for (j, constant) in proto.constants.iter().enumerate() {
                match constant {
                    CompilationConstant::FunctionProto(idx) => {
                        if *idx < proto_handles.len() {
                            eprintln!("DEBUG execute_module: Proto {} const {}: FunctionProto({})", i, j, idx);
                            updated_proto.constants[j] = Value::FunctionProto(proto_handles[*idx]);
                        } else {
                            return Err(LuaError::RuntimeError(format!(
                                "Invalid function prototype index: {}", idx
                            )));
                        }
                    }
                    CompilationConstant::Table(entries) => {
                        let table_handle = self.create_table_constant(entries, &string_handles, &proto_handles)?;
                        updated_proto.constants[j] = Value::Table(table_handle);
                    }
                    _ => {} // Already set in first pass
                }
            }
            
            // Replace the prototype with updated constants
            self.heap.replace_function_proto(proto_handle, updated_proto)?;
        }
        
        // Step 3: Convert main function constants
        eprintln!("DEBUG execute_module: Creating main function with {} constants", module.constants.len());
        
        let mut main_constants = Vec::with_capacity(module.constants.len());
        for (idx, constant) in module.constants.iter().enumerate() {
            let value = match constant {
                CompilationConstant::Nil => Value::Nil,
                CompilationConstant::Boolean(b) => Value::Boolean(*b),
                CompilationConstant::Number(n) => Value::Number(*n),
                CompilationConstant::String(idx) => {
                    if *idx < string_handles.len() {
                        Value::String(string_handles[*idx])
                    } else {
                        return Err(LuaError::RuntimeError(format!(
                            "Invalid string index: {}", idx
                        )));
                    }
                }
                CompilationConstant::FunctionProto(idx) => {
                    if *idx < proto_handles.len() {
                        eprintln!("DEBUG execute_module: Main const {}: FunctionProto({})", idx, idx);
                        Value::FunctionProto(proto_handles[*idx])
                    } else {
                        return Err(LuaError::RuntimeError(format!(
                            "Invalid function prototype index: {}", idx
                        )));
                    }
                }
                CompilationConstant::Table(entries) => {
                    let table_handle = self.create_table_constant(entries, &string_handles, &proto_handles)?;
                    Value::Table(table_handle)
                }
            };
            main_constants.push(value);
        }
        
        // Convert main upvalues
        let vm_upvalues: Vec<UpvalueInfo> = module.upvalues.iter().map(|u| UpvalueInfo {
            in_stack: u.in_stack,
            index: u.index,
        }).collect();
        
        // Create main function prototype
        let proto = FunctionProto {
            bytecode: module.bytecode.clone(),
            constants: main_constants,
            num_params: module.num_params,
            is_vararg: module.is_vararg,
            max_stack_size: module.max_stack_size,
            upvalues: vm_upvalues,
        };
        
        eprintln!("DEBUG execute_module: Main function has {} bytecode instructions", proto.bytecode.len());
        
        // Create the closure in the heap
        let closure = Closure {
            proto,
            upvalues: Vec::new(), // Top-level functions have no upvalues
        };
        
        let closure_handle = self.heap.create_closure(closure)?;
        
        eprintln!("DEBUG execute_module: Created main closure handle: {:?}", closure_handle);
        
        // Place the closure at position 0 of the main thread
        self.heap.set_thread_register(self.main_thread, 0, &Value::Closure(closure_handle))?;
        
        // Place arguments starting at position 1
        for (i, arg) in args.iter().enumerate() {
            self.heap.set_thread_register(self.main_thread, 1 + i, arg)?;
        }
        
        // Queue initial function call
        self.operation_queue.push_back(PendingOperation::FunctionCall {
            func_index: 0,
            nargs: args.len(),
            expected_results: -1,
        });
        
        eprintln!("DEBUG execute_module: Starting execution...");
        
        // Execute until completion
        loop {
            // Always process pending operations first
            if let Some(op) = self.operation_queue.pop_front() {
                match self.process_pending_operation(op)? {
                    StepResult::Continue => continue,
                    StepResult::Completed(values) => {
                        eprintln!("DEBUG execute_module: Execution completed with {} values", values.len());
                        // Return the first result, or nil if none
                        return Ok(values.get(0).cloned().unwrap_or(Value::Nil));
                    }
                }
            }
            
            // Then execute next instruction if no pending operations
            match self.step()? {
                StepResult::Continue => continue,
                StepResult::Completed(values) => {
                    eprintln!("DEBUG execute_module: Execution completed with {} values", values.len());
                    // Return the first result, or nil if none
                    return Ok(values.get(0).cloned().unwrap_or(Value::Nil));
                }
            }
        }
    }
    
    /// Initialize the standard library
    pub fn init_stdlib(&mut self) -> LuaResult<()> {
        eprintln!("DEBUG init_stdlib: Initializing RefCellVM standard library");
        
        // Use the RefCell-specific standard library initialization
        super::refcell_stdlib::init_refcell_stdlib(self)
    }
    
    /// Set context (for compatibility with LuaGIL interface)
    pub fn set_context(&mut self, _context: super::ScriptContext) -> LuaResult<()> {
        // RefCellVM doesn't use contexts in the same way as the transaction-based VM
        // This is a no-op for compatibility
        Ok(())
    }
    
    /// Evaluate a Lua script
    pub fn eval_script(&mut self, script: &str) -> LuaResult<Value> {
        eprintln!("DEBUG eval_script: Compiling script");
        
        // Parse and compile the script
        let module = super::compiler::compile(script)?;
        
        // Execute the compiled module
        self.execute_module(&module, &[])
    }
    
    /// Get heap reference for library functions
    pub fn heap(&self) -> &RefCellHeap {
        &self.heap
    }
    
    /// Get mutable heap reference for library functions  
    pub fn heap_mut(&mut self) -> &mut RefCellHeap {
        &mut self.heap
    }
    
    /// Set table field
    pub fn set_table_field(&self, table: TableHandle, key: Value, value: Value) -> LuaResult<()> {
        self.heap.set_table_field(table, &key, &value)
    }
    
    /// Helper method to create a table constant
    fn create_table_constant(
        &mut self,
        entries: &[(CompilationConstant, CompilationConstant)],
        string_handles: &[StringHandle],
        proto_handles: &[FunctionProtoHandle],
    ) -> LuaResult<TableHandle> {
        // Create the table
        let table_handle = self.heap.create_table()?;
        
        // Populate the table with entries
        for (key_const, value_const) in entries {
            // Convert key constant to Value
            let key = self.constant_to_value(key_const, string_handles, proto_handles)?;
            
            // Convert value constant to Value
            let value = self.constant_to_value(value_const, string_handles, proto_handles)?;
            
            // Set the field
            self.heap.set_table_field(table_handle, &key, &value)?;
        }
        
        Ok(table_handle)
    }
    
    /// Helper to convert CompilationConstant to Value
    fn constant_to_value(
        &mut self,
        constant: &CompilationConstant,
        string_handles: &[StringHandle],
        proto_handles: &[FunctionProtoHandle],
    ) -> LuaResult<Value> {
        match constant {
            CompilationConstant::Nil => Ok(Value::Nil),
            CompilationConstant::Boolean(b) => Ok(Value::Boolean(*b)),
            CompilationConstant::Number(n) => Ok(Value::Number(*n)),
            CompilationConstant::String(idx) => {
                if *idx < string_handles.len() {
                    Ok(Value::String(string_handles[*idx]))
                } else {
                    Err(LuaError::RuntimeError(format!("Invalid string index: {}", idx)))
                }
            }
            CompilationConstant::FunctionProto(idx) => {
                if *idx < proto_handles.len() {
                    Ok(Value::FunctionProto(proto_handles[*idx]))
                } else {
                    Err(LuaError::RuntimeError(format!("Invalid prototype index: {}", idx)))
                }
            }
            CompilationConstant::Table(entries) => {
                // Recursively create table
                let table = self.create_table_constant(entries, string_handles, proto_handles)?;
                Ok(Value::Table(table))
            }
        }
    }
}

/// Result of a single VM step
enum StepResult {
    /// Continue execution
    Continue,
    /// Execution completed with return values
    Completed(Vec<Value>),
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

/// Execution context for C functions in RefCellVM
/// 
/// This provides the same interface as the transaction-based ExecutionContext
/// but works directly with RefCellHeap's interior mutability pattern
pub struct RefCellExecutionContext<'a> {
    /// Reference to the heap
    heap: &'a RefCellHeap,
    
    /// Current thread
    thread: ThreadHandle,
    
    /// Base register for this C function call
    base: u16,
    
    /// Number of arguments passed
    nargs: usize,
    
    /// Results pushed so far
    results_pushed: usize,
}

impl<'a> ExecutionContext for RefCellExecutionContext<'a> {
    fn arg_count(&self) -> usize {
        self.nargs
    }
    
    fn get_arg(&self, index: usize) -> LuaResult<Value> {
        if index >= self.nargs {
            return Err(LuaError::RuntimeError(format!(
                "Argument {} out of range (passed {})",
                index,
                self.nargs
            )));
        }
        
        // Arguments start at base + 1 (base points to the function)
        let register = self.base as usize + 1 + index;
        self.heap.get_thread_register(self.thread, register)
    }
    
    fn push_result(&mut self, value: Value) -> LuaResult<()> {
        // Results go where the function was (at base), not after arguments
        let register = self.base as usize + self.results_pushed;
        self.heap.set_thread_register(self.thread, register, &value)?;
        self.results_pushed += 1;
        Ok(())
    }
    
    fn set_return(&mut self, index: usize, value: Value) -> LuaResult<()> {
        // Results start at the function's position (base)
        let register = self.base as usize + index;
        self.heap.set_thread_register(self.thread, register, &value)?;
        // Update results_pushed if needed
        if index >= self.results_pushed {
            self.results_pushed = index + 1;
        }
        Ok(())
    }
    
    fn create_string(&self, s: &str) -> LuaResult<StringHandle> {
        self.heap.create_string(s)
    }
    
    fn create_table(&self) -> LuaResult<TableHandle> {
        self.heap.create_table()
    }
    
    fn get_table_field(&self, table: TableHandle, key: &Value) -> LuaResult<Value> {
        self.heap.get_table_field(table, key)
    }
    
    fn set_table_field(&self, table: TableHandle, key: Value, value: Value) -> LuaResult<()> {
        self.heap.set_table_field(table, &key, &value)
    }
    
    fn get_arg_str(&self, index: usize) -> LuaResult<String> {
        let value = self.get_arg(index)?;
        match value {
            Value::String(handle) => self.get_string_from_handle(handle),
            _ => Err(LuaError::TypeError {
                expected: "string".to_string(),
                got: value.type_name().to_string(),
            }),
        }
    }
    
    fn get_number_arg(&self, index: usize) -> LuaResult<f64> {
        let value = self.get_arg(index)?;
        match value {
            Value::Number(n) => Ok(n),
            _ => Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: value.type_name().to_string(),
            }),
        }
    }
    
    fn get_bool_arg(&self, index: usize) -> LuaResult<bool> {
        let value = self.get_arg(index)?;
        match value {
            Value::Boolean(b) => Ok(b),
            _ => Err(LuaError::TypeError {
                expected: "boolean".to_string(),
                got: value.type_name().to_string(),
            }),
        }
    }
    
    fn table_next(&self, table: TableHandle, key: Value) -> LuaResult<Option<(Value, Value)>> {
        self.heap.table_next(table, key)
    }
    
    fn globals_handle(&self) -> LuaResult<TableHandle> {
        self.heap.globals()
    }
    
    fn get_string_from_handle(&self, handle: StringHandle) -> LuaResult<String> {
        self.heap.get_string_value(handle)
    }
    
    fn check_metamethod(&self, value: &Value, method_name: &str) -> LuaResult<Option<Value>> {
        match value {
            Value::Table(handle) => {
                // Get the metatable if any
                let mt_opt = self.heap.get_table_metatable(*handle)?;
                if let Some(mt) = mt_opt {
                    // Look for the metamethod
                    let method_handle = self.heap.create_string(method_name)?;
                    let method_key = Value::String(method_handle);
                    let method = self.heap.get_table_field(mt, &method_key)?;
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
    
    fn call_metamethod(&mut self, _func: Value, _args: Vec<Value>) -> LuaResult<Vec<Value>> {
        Err(LuaError::NotImplemented("metamethod calls in RefCellVM".to_string()))
    }
    
    fn table_get(&self, table: TableHandle, key: Value) -> LuaResult<Value> {
        // For now, just do raw get - metamethods not implemented
        self.heap.get_table_field(table, &key)
    }
    
    fn table_raw_get(&self, table: TableHandle, key: Value) -> LuaResult<Value> {
        self.heap.get_table_field(table, &key)
    }
    
    fn table_raw_set(&self, table: TableHandle, key: Value, value: Value) -> LuaResult<()> {
        self.heap.set_table_field(table, &key, &value)
    }
    
    fn table_length(&self, table: TableHandle) -> LuaResult<usize> {
        let table_obj = self.heap.get_table(table)?;
        Ok(table_obj.array_len())
    }
    
    fn set_metatable(&self, table: TableHandle, metatable: Option<TableHandle>) -> LuaResult<()> {
        self.heap.set_table_metatable(table, metatable)
    }
    
    fn get_metatable(&self, table: TableHandle) -> LuaResult<Option<TableHandle>> {
        self.heap.get_table_metatable(table)
    }
    
    fn get_current_thread(&self) -> LuaResult<ThreadHandle> {
        Ok(self.thread)
    }
    
    fn get_base_index(&self) -> LuaResult<usize> {
        Ok(self.base as usize)
    }
    
    fn get_results_pushed(&self) -> usize {
        self.results_pushed
    }
    
    fn globals_get(&self, name: &str) -> LuaResult<Value> {
        let name_handle = self.heap.create_string(name)?;
        let globals = self.heap.globals()?;
        self.heap.get_table_field(globals, &Value::String(name_handle))
    }
    
    fn execute_function(&mut self, _closure: ClosureHandle, _args: &[Value]) -> LuaResult<Value> {
        Err(LuaError::NotImplemented("execute_function in RefCellVM".to_string()))
    }
    
    fn eval_script(&mut self, _script: &str) -> LuaResult<Value> {
        Err(LuaError::NotImplemented("eval_script in RefCellVM".to_string()))
    }
    
    fn get_raw_table_field(&self, table: TableHandle, key: &Value) -> LuaResult<Value> {
        // Direct table access without metamethods
        self.heap.get_table_field(table, key)
    }
    
    fn set_raw_table_field(&mut self, table: TableHandle, key: &Value, value: &Value) -> LuaResult<()> {
        // Direct table mutation without metamethods
        self.heap.set_table_field(table, key, value)
    }
}

impl<'a> RefCellExecutionContext<'a> {
    /// Create a new execution context
    pub fn new(
        heap: &'a RefCellHeap,
        thread: ThreadHandle,
        base: u16,
        nargs: usize,
    ) -> Self {
        RefCellExecutionContext {
            heap,
            thread,
            base,
            nargs,
            results_pushed: 0,
        }
    }
}

// Type alias for C functions that work with RefCellVM
pub type RefCellCFunction = fn(&mut RefCellExecutionContext) -> LuaResult<i32>;

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_vm_creation() {
        let vm = RefCellVM::new().unwrap();
        assert_eq!(vm.operation_queue.len(), 0);
    }
    
    #[test]
    fn test_number_equality_comparison() {
        let mut vm = RefCellVM::new().unwrap();
        
        // Test direct Value comparison
        let v1 = Value::Number(1.0);
        let v2 = Value::Number(1.0);
        let v3 = Value::Number(1);  // Integer literal converted to f64
        
        assert!(vm.compare_eq(&v1, &v2), "1.0 should equal 1.0");
        assert!(vm.compare_eq(&v1, &v3), "1.0 should equal 1");
        
        // Test with different representations
        let v4 = Value::Number(1.0f64);
        let v5 = Value::Number(1i32 as f64);
        
        assert!(vm.compare_eq(&v4, &v5), "Different representations of 1 should be equal");
        
        // Test that different numbers are not equal
        let v6 = Value::Number(1.1);
        assert!(!vm.compare_eq(&v1, &v6), "1.0 should not equal 1.1");
    }
    
    #[test]
    fn test_simple_for_loop() {
        let mut vm = RefCellVM::new().unwrap();
        
        // Create a simple function with a for loop:
        // for i = 1, 3 do end
        let proto = FunctionProto {
            bytecode: vec![
                // Initialize loop variables
                Instruction::create(OpCode::LoadK, 0, 0, 0).0,     // R(0) = 1 (start)
                Instruction::create(OpCode::LoadK, 1, 1, 0).0,     // R(1) = 3 (limit)
                Instruction::create(OpCode::LoadK, 2, 2, 0).0,     // R(2) = 1 (step)
                
                // For loop
                Instruction::create_sBx(OpCode::ForPrep, 0, 1).0,  // FORPREP R(0), skip 1
                Instruction::create_sBx(OpCode::ForLoop, 0, -1).0, // FORLOOP R(0), jump -1
                
                // Return
                Instruction::create_ABC(OpCode::Return, 0, 1, 0).0,
            ],
            constants: vec![
                Value::Number(1.0),  // start
                Value::Number(3.0),  // limit
                Value::Number(1.0),  // step
            ],
            num_params: 0,
            is_vararg: false,
            max_stack_size: 4, // Need 4 registers for the loop
            upvalues: vec![],
        };
        
        let closure = Closure {
            proto,
            upvalues: vec![],
        };
        
        // Create closure in heap
        let closure_handle = vm.heap.create_closure(closure).unwrap();
        
        // Execute - this should complete without errors
        let results = vm.execute(closure_handle).unwrap();
        assert_eq!(results.len(), 0); // No return values
    }
    
    #[test]
    fn test_tforloop_iterator() {
        let mut vm = RefCellVM::new().unwrap();
        
        // Create a simple iterator function that returns values 1, 2, 3, then nil
        // This simulates what pairs() would return
        let iter_proto = FunctionProto {
            bytecode: vec![
                // R(0) = state (table), R(1) = current key
                // Check if key is nil (first iteration)
                Instruction::create_ABC(OpCode::Eq, 1, 1, 0).0,     // if R(1) == nil (const 0)
                Instruction::create_sBx(OpCode::Jmp, 0, 3).0,      // skip to first key
                
                // Check current key value
                Instruction::create_ABC(OpCode::Eq, 0, 1, 1).0,    // if R(1) == 1
                Instruction::create_ABC(OpCode::LoadK, 0, 2, 0).0, // R(0) = 2 (next key)
                Instruction::create_sBx(OpCode::Jmp, 0, 8).0,      // jump to return
                
                // First iteration - return 1
                Instruction::create_ABC(OpCode::LoadK, 0, 1, 0).0, // R(0) = 1
                Instruction::create_sBx(OpCode::Jmp, 0, 5).0,      // jump to return
                
                // Check if key is 2
                Instruction::create_ABC(OpCode::Eq, 0, 1, 2).0,    // if R(1) == 2
                Instruction::create_ABC(OpCode::LoadK, 0, 3, 0).0, // R(0) = 3
                Instruction::create_sBx(OpCode::Jmp, 0, 2).0,      // jump to return
                
                // Otherwise return nil (end of iteration)
                Instruction::create_ABC(OpCode::LoadNil, 0, 0, 0).0, // R(0) = nil
                
                // Return R(0) and R(0) (key, value)
                Instruction::create_ABC(OpCode::Return, 0, 3, 0).0, // return 2 values
            ],
            constants: vec![
                Value::Nil,
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ],
            num_params: 2, // state and current key
            is_vararg: false,
            max_stack_size: 3,
            upvalues: vec![],
        };
        
        let iter_closure = Closure {
            proto: iter_proto,
            upvalues: vec![],
        };
        
        // Create main function that uses TFORLOOP
        let main_proto = FunctionProto {
            bytecode: vec![
                // Set up iterator (R(0) = iterator, R(1) = state, R(2) = nil)
                Instruction::create_ABC(OpCode::Closure, 0, 0, 0).0, // R(0) = iterator closure
                Instruction::create_ABC(OpCode::NewTable, 1, 0, 0).0, // R(1) = {} (dummy state)
                Instruction::create_ABC(OpCode::LoadNil, 2, 2, 0).0, // R(2) = nil (initial key)
                
                // TFORLOOP
                Instruction::create_ABC(OpCode::TForLoop, 0, 0, 2).0, // 2 results (key, value)
                // Note: The JMP instruction here uses -1, but when it executes, the PC
                // has already been incremented, so it would need -2 to get back to TFORLOOP.
                // However, our TFORLOOP implementation now handles the jump-back directly,
                // so this JMP is only executed when exiting the loop.
                Instruction::create_sBx(OpCode::Jmp, 0, -1).0,        // jump back (handled by TFORLOOP)
                
                // Return
                Instruction::create_ABC(OpCode::Return, 0, 1, 0).0,
            ],
            constants: vec![
                // The iterator function proto should be here
                Value::FunctionProto(vm.heap.create_function_proto(iter_closure.proto).unwrap()),
            ],
            num_params: 0,
            is_vararg: false,
            max_stack_size: 6, // Need space for iterator state and results
            upvalues: vec![],
        };
        
        let main_closure = Closure {
            proto: main_proto,
            upvalues: vec![],
        };
        
        // Create closure in heap
        let closure_handle = vm.heap.create_closure(main_closure).unwrap();
        
        // Execute - this should complete without errors
        let results = vm.execute(closure_handle).unwrap();
        assert_eq!(results.len(), 0); // No return values
    }
}