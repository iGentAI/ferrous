//! Rc<RefCell> Based Lua VM
//!
//! This module implements a Lua VM using fine-grained Rc<RefCell> objects
//! instead of a global RefCell, providing proper shared mutable state semantics
//! and resolving borrow checker issues.

use std::rc::Rc;
use std::cell::RefCell;
use std::collections::VecDeque;

use super::codegen::{self, Instruction, OpCode, CompilationConstant};
use super::compiler;
use super::error::{LuaError, LuaResult};
use super::rc_value::{
    Value, LuaString, Table, Closure, Thread, UpvalueState,
    StringHandle, TableHandle, ClosureHandle, ThreadHandle,
    UpvalueHandle, FunctionProtoHandle, CallFrame, ThreadStatus,
    HashableValue, FunctionProto
};
use super::rc_heap::RcHeap;

/// Trait for execution contexts that provide access to VM state during C function calls
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
    fn get_table_field(&self, table: &TableHandle, key: &Value) -> LuaResult<Value>;
    
    /// Set a table field
    fn set_table_field(&self, table: &TableHandle, key: Value, value: Value) -> LuaResult<()>;
    
    /// Get argument as string
    fn get_arg_str(&self, index: usize) -> LuaResult<String>;
    
    /// Get argument as number
    fn get_number_arg(&self, index: usize) -> LuaResult<f64>;
    
    /// Get argument as boolean
    fn get_bool_arg(&self, index: usize) -> LuaResult<bool>;
    
    /// Get next key-value pair from table
    fn table_next(&self, table: &TableHandle, key: &Value) -> LuaResult<Option<(Value, Value)>>;
    
    /// Get the globals table handle
    fn globals_handle(&self) -> LuaResult<TableHandle>;
    
    // Additional context methods will be added as needed
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

/// Pending VM operations queue for non-recursive execution
#[derive(Debug, Clone)]
enum PendingOperation {
    /// Function call
    FunctionCall {
        /// Function to call
        func: Value,
        /// Arguments to pass
        args: Vec<Value>,
        /// Expected number of results (-1 for multiple)
        expected_results: i32,
        /// Stack location to store results
        result_base: usize,
    },
    
    /// Return from function
    Return {
        /// Return values
        values: Vec<Value>,
    },
    
    /// TFORLOOP continuation
    TForLoopContinuation {
        /// Base register index
        base: usize,
        /// A field from TFORLOOP instruction
        a: usize,
        /// C field from TFORLOOP instruction
        c: usize,
        /// PC value before iterator call
        pc_before_call: usize,
    },
    
    /// Metamethod call
    MetamethodCall {
        /// Metamethod to call
        method: Value,
        /// Arguments for metamethod
        args: Vec<Value>,
        /// Expected number of results
        expected_results: i32,
        /// Stack location to store results
        result_base: usize,
    },
}

/// Result of a VM step
#[derive(Debug, Clone)]
enum StepResult {
    /// Continue execution
    Continue,
    
    /// Completed with return values
    Completed(Vec<Value>),
}

/// Execution context for C functions
struct VmExecutionContext<'a> {
    /// Reference to the VM
    vm: &'a mut RcVM,
    
    /// Arguments base index
    base: usize,
    
    /// Number of arguments
    nargs: usize,
    
    /// Results pushed so far
    results_pushed: usize,
}

/// Rc<RefCell> based Lua VM
pub struct RcVM {
    /// The Lua heap
    pub heap: RcHeap,
    
    /// Operation queue for non-recursive execution
    operation_queue: VecDeque<PendingOperation>,
    
    /// Current thread
    current_thread: ThreadHandle,
    
    /// VM configuration
    config: VMConfig,
}

impl RcVM {
    /// Create a new VM
    pub fn new() -> LuaResult<Self> {
        Self::with_config(VMConfig::default())
    }
    
    /// Create a new VM with config
    pub fn with_config(config: VMConfig) -> LuaResult<Self> {
        // Create heap
        let heap = RcHeap::new()?;
        
        // Get main thread
        let main_thread = heap.main_thread();
        
        Ok(RcVM {
            heap,
            operation_queue: VecDeque::new(),
            current_thread: main_thread.clone(),
            config,
        })
    }
    
    /// Initialize the standard library
    pub fn init_stdlib(&mut self) -> LuaResult<()> {
        super::rc_stdlib::init_stdlib(self)
    }
    
    /// Execute a compiled module
    pub fn execute_module(&mut self, module: &compiler::CompiledModule, args: &[Value]) -> LuaResult<Value> {
        // Set up operations queue
        self.operation_queue.clear();
        
        // Create main function
        let main_closure = self.load_module(module)?;
        
        // Clear stack
        {
            let mut thread = self.current_thread.borrow_mut();
            thread.stack.clear();
            thread.call_frames.clear();
        }
        
        // Place the main function and arguments on the stack
        self.set_register(0, Value::Closure(main_closure.clone()))?;
        
        for (i, arg) in args.iter().enumerate() {
            self.set_register(i + 1, arg.clone())?;
        }
        
        // Queue the main function call
        self.operation_queue.push_back(PendingOperation::FunctionCall {
            func: Value::Closure(main_closure.clone()),
            args: args.to_vec(),
            expected_results: 1, // Expect one result from main chunk
            result_base: 0,
        });
        
        // Execute until completion
        self.run_to_completion()
    }
    
    /// Run VM until completion
    fn run_to_completion(&mut self) -> LuaResult<Value> {
        loop {
            // Process pending operations first
            if !self.operation_queue.is_empty() {
                let op = self.operation_queue.pop_front().unwrap();
                match self.process_operation(op)? {
                    StepResult::Continue => continue,
                    StepResult::Completed(values) => {
                        return Ok(values.first().cloned().unwrap_or(Value::Nil));
                    }
                }
            }
            
            // Execute next instruction
            match self.step()? {
                StepResult::Continue => continue,
                StepResult::Completed(values) => {
                    return Ok(values.first().cloned().unwrap_or(Value::Nil));
                }
            }
        }
    }
    
    /// Execute a single VM step
    fn step(&mut self) -> LuaResult<StepResult> {
        // Check if we have call frames
        let call_depth = self.heap.get_call_depth(&self.current_thread);
        if call_depth == 0 {
            return Ok(StepResult::Completed(vec![]));
        }
        
        // Get the current frame
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        let base = frame.base_register as usize;
        let pc = frame.pc;
        
        // Get the current instruction
        let instruction = self.get_instruction(&frame.closure, pc)?;
        let inst = Instruction(instruction);
        
        // Increment PC for next instruction
        self.heap.increment_pc(&self.current_thread)?;
        
        // Debug output for opcodes of interest
        match inst.get_opcode() {
            OpCode::ForPrep | OpCode::ForLoop | OpCode::TForLoop => {
                eprintln!("DEBUG RcVM: Executing {:?} at PC={}, base={}", 
                         inst.get_opcode(), pc, base);
            }
            OpCode::Closure | OpCode::Call | OpCode::Return => {
                eprintln!("DEBUG RcVM: Executing {:?} at PC={}, base={}", 
                         inst.get_opcode(), pc, base);
            }
            _ => {}
        }
        
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
                return Err(LuaError::NotImplemented(format!("Opcode {:?}", inst.get_opcode())));
            }
        }
        
        Ok(StepResult::Continue)
    }
    
    /// Process a pending operation
    fn process_operation(&mut self, operation: PendingOperation) -> LuaResult<StepResult> {
        match operation {
            PendingOperation::FunctionCall { func, args, expected_results, result_base } => {
                self.execute_function_call(func, args, expected_results, result_base)
            },
            PendingOperation::Return { values } => {
                self.process_return(values)
            },
            PendingOperation::TForLoopContinuation { base, a, c, pc_before_call } => {
                self.process_tforloop_continuation(base, a, c, pc_before_call)
            },
            PendingOperation::MetamethodCall { method, args, expected_results, result_base } => {
                self.execute_function_call(method, args, expected_results, result_base)
            }
        }
    }
    
    /// Load a compiled module
    fn load_module(&mut self, module: &compiler::CompiledModule) -> LuaResult<ClosureHandle> {
        eprintln!("DEBUG load_module: Loading module with {} constants, {} strings, {} prototypes", 
                 module.constants.len(), module.strings.len(), module.prototypes.len());
        
        // Step 1: Create string handles
        let mut string_handles = Vec::with_capacity(module.strings.len());
        for s in &module.strings {
            string_handles.push(self.heap.create_string(s)?);
        }
        
        // Step 2: Create function prototype handles (placeholder pass)
        let mut proto_handles = Vec::with_capacity(module.prototypes.len());
        
        for proto in &module.prototypes {
            // Create constants with placeholders
            let mut constants = Vec::with_capacity(proto.constants.len());
            for _ in &proto.constants {
                constants.push(Value::Nil);
            }
            
            // Create upvalues
            let upvalues = proto.upvalues.iter()
                .map(|u| super::rc_value::UpvalueInfo { in_stack: u.in_stack, index: u.index })
                .collect();
            
            // Create prototype
            let func_proto = FunctionProto {
                bytecode: proto.bytecode.clone(),
                constants,
                num_params: proto.num_params,
                is_vararg: proto.is_vararg,
                max_stack_size: proto.max_stack_size,
                upvalues,
            };
            
            let handle = self.heap.create_function_proto(func_proto);
            proto_handles.push(handle);
        }
        
        // Step 3: Fill in constants
        for (i, proto) in module.prototypes.iter().enumerate() {
            let proto_handle = Rc::clone(&proto_handles[i]);
            
            // Need to create a mutable clone
            let mut updated_proto = (*proto_handle).clone();
            
            // Update constants
            for (j, constant) in proto.constants.iter().enumerate() {
                match constant {
                    codegen::CompilationConstant::Nil => {
                        updated_proto.constants[j] = Value::Nil;
                    },
                    codegen::CompilationConstant::Boolean(b) => {
                        updated_proto.constants[j] = Value::Boolean(*b);
                    },
                    codegen::CompilationConstant::Number(n) => {
                        updated_proto.constants[j] = Value::Number(*n);
                    },
                    codegen::CompilationConstant::String(idx) => {
                        if *idx < string_handles.len() {
                            updated_proto.constants[j] = Value::String(Rc::clone(&string_handles[*idx]));
                        } else {
                            return Err(LuaError::RuntimeError(format!("Invalid string index: {}", idx)));
                        }
                    },
                    codegen::CompilationConstant::FunctionProto(idx) => {
                        if *idx < proto_handles.len() {
                            updated_proto.constants[j] = Value::FunctionProto(Rc::clone(&proto_handles[*idx]));
                        } else {
                            return Err(LuaError::RuntimeError(format!("Invalid function prototype index: {}", idx)));
                        }
                    },
                    codegen::CompilationConstant::Table(_entries) => {
                        // Tables need to be created and filled
                        let table = self.heap.create_table();
                        updated_proto.constants[j] = Value::Table(table);
                    },
                }
            }
            
            // Replace the prototype
            let new_handle = self.heap.create_function_proto(updated_proto);
            proto_handles[i] = new_handle;
        }
        
        // Step 4: Create main function
        let mut main_constants = Vec::with_capacity(module.constants.len());
        
        // Fill main constants
        for constant in &module.constants {
            match constant {
                codegen::CompilationConstant::Nil => {
                    main_constants.push(Value::Nil);
                },
                codegen::CompilationConstant::Boolean(b) => {
                    main_constants.push(Value::Boolean(*b));
                },
                codegen::CompilationConstant::Number(n) => {
                    main_constants.push(Value::Number(*n));
                },
                codegen::CompilationConstant::String(idx) => {
                    if *idx < string_handles.len() {
                        main_constants.push(Value::String(Rc::clone(&string_handles[*idx])));
                    } else {
                        return Err(LuaError::RuntimeError(format!("Invalid string index: {}", idx)));
                    }
                },
                codegen::CompilationConstant::FunctionProto(idx) => {
                    if *idx < proto_handles.len() {
                        main_constants.push(Value::FunctionProto(Rc::clone(&proto_handles[*idx])));
                    } else {
                        return Err(LuaError::RuntimeError(format!("Invalid function prototype index: {}", idx)));
                    }
                },
                codegen::CompilationConstant::Table(_entries) => {
                    // Tables need to be created and filled
                    let table = self.heap.create_table();
                    main_constants.push(Value::Table(table));
                },
            }
        }
        
        // Create upvalues for main function based on its prototype
        let main_upvalues = module.upvalues.iter()
            .map(|u| super::rc_value::UpvalueInfo { in_stack: u.in_stack, index: u.index })
            .collect();
        
        // Create main function prototype
        let main_proto = FunctionProto {
            bytecode: module.bytecode.clone(),
            constants: main_constants,
            num_params: module.num_params,
            is_vararg: module.is_vararg,
            max_stack_size: module.max_stack_size,
            upvalues: main_upvalues,
        };
        
        let main_proto_handle = self.heap.create_function_proto(main_proto);
        
        // Initialize the upvalues for the main closure
        // according to Lua 5.1 spec, which requires _ENV (globals) as upvalue 0
        let mut main_upvalues = Vec::new();
        
        let main_proto_handle_for_upvalue_len = Rc::clone(&main_proto_handle);
        
        eprintln!("DEBUG load_module: Creating upvalues for main function");
        for upvalue_info in &main_proto_handle_for_upvalue_len.upvalues {
            eprintln!("DEBUG load_module: Processing upvalue: in_stack={}, index={}", 
                     upvalue_info.in_stack, upvalue_info.index);
            
            // In Lua 5.1, the main function typically has _ENV (globals table) as upvalue 0
            if upvalue_info.index == 0 && !upvalue_info.in_stack {
                // This is the _ENV upvalue - global environment
                eprintln!("DEBUG load_module: Creating _ENV upvalue with globals");
                let globals_value = Value::Table(self.heap.globals());
                let globals_upvalue = Rc::new(RefCell::new(UpvalueState::Closed { 
                    value: globals_value 
                }));
                main_upvalues.push(globals_upvalue);
            } else {
                // For any other upvalue in main function
                // Create as closed nil value as per Lua 5.1 spec
                eprintln!("DEBUG load_module: Creating closed nil upvalue");
                let nil_upvalue = Rc::new(RefCell::new(UpvalueState::Closed { 
                    value: Value::Nil 
                }));
                main_upvalues.push(nil_upvalue);
            }
        }
        
        // Create main closure with proper upvalues
        let main_closure = self.heap.create_closure(Rc::clone(&main_proto_handle), main_upvalues);
        
        eprintln!("DEBUG load_module: Created main closure with {} upvalues", 
                 main_proto_handle.upvalues.len());
        
        Ok(main_closure)
    }
    
    /// Execute a function call
    fn execute_function_call(
        &mut self,
        func: Value,
        args: Vec<Value>,
        expected_results: i32,
        result_base: usize,
    ) -> LuaResult<StepResult> {
        match &func {
            &Value::Closure(ref closure_handle) => {
                self.call_lua_function(Rc::clone(closure_handle), args, expected_results, result_base)
            },
            &Value::CFunction(cfunc) => {
                self.call_c_function(cfunc, args, expected_results, result_base)
            },
            &Value::Table(ref table_handle) => {
                // Check for __call metamethod
                let table_ref = table_handle.borrow();
                if let Some(metatable) = &table_ref.metatable {
                    let metatable_clone = Rc::clone(metatable);
                    drop(table_ref);
                    
                    let mt_ref = metatable_clone.borrow();
                    let call_key = Value::String(Rc::clone(&self.heap.metamethod_names.call));
                    if let Some(metamethod) = mt_ref.get_field(&call_key) {
                        drop(mt_ref);
                        
                        // Call the metamethod with the table as first argument
                        let mut metamethod_args = vec![Value::Table(Rc::clone(table_handle))];
                        metamethod_args.extend(args);
                        
                        self.execute_function_call(metamethod, metamethod_args, expected_results, result_base)
                    } else {
                        Err(LuaError::TypeError {
                            expected: "function".to_string(),
                            got: "table".to_string(),
                        })
                    }
                } else {
                    Err(LuaError::TypeError {
                        expected: "function".to_string(),
                        got: "table".to_string(),
                    })
                }
            },
            _ => {
                Err(LuaError::TypeError {
                    expected: "function".to_string(),
                    got: func.type_name().to_string(),
                })
            }
        }
    }
    
    /// Call a Lua function
    fn call_lua_function(
        &mut self,
        closure: ClosureHandle,
        args: Vec<Value>,
        expected_results: i32,
        result_base: usize,
    ) -> LuaResult<StepResult> {
        // Get prototype information
        let (num_params, max_stack, is_vararg, func_proto) = {
            let closure_ref = closure.borrow();
            let proto_ref = &closure_ref.proto;
            (
                proto_ref.num_params as usize,
                proto_ref.max_stack_size as usize,
                proto_ref.is_vararg,
                Rc::clone(&closure_ref.proto),
            )
        };
        
        // Prepare the stack
        let new_base = result_base + 1; // Skip the function
        
        // Ensure stack space
        self.ensure_stack_space(new_base + max_stack)?;
        
        // Place arguments on stack
        for (i, arg) in args.iter().enumerate() {
            if i < num_params {
                self.set_register(new_base + i, arg.clone())?;
            }
        }
        
        // Fill missing parameters with nil
        for i in args.len()..num_params {
            self.set_register(new_base + i, Value::Nil)?;
        }
        
        // Create varargs if needed
        let varargs = if is_vararg && args.len() > num_params {
            Some(args[num_params..].to_vec())
        } else {
            None
        };
        
        // Create new call frame
        let frame = CallFrame {
            closure: closure.clone(),
            pc: 0,
            base_register: new_base as u16,
            expected_results: if expected_results >= 0 {
                Some(expected_results as usize)
            } else {
                None
            },
            varargs,
        };
        
        // Push the frame
        self.heap.push_call_frame(&self.current_thread, frame)?;
        
        Ok(StepResult::Continue)
    }
    
    /// Call a C function
    fn call_c_function(
        &mut self,
        func: super::rc_value::CFunction,
        args: Vec<Value>,
        expected_results: i32,
        result_base: usize,
    ) -> LuaResult<StepResult> {
        // Place arguments in the stack at result_base+1
        let args_base = result_base + 1;
        
        // Ensure stack space
        self.ensure_stack_space(args_base + args.len())?;
        
        // Place arguments
        for (i, arg) in args.iter().enumerate() {
            self.set_register(args_base + i, arg.clone())?;
        }
        
        // Call process_c_function_call
        self.process_c_function_call(func, result_base as u16, args.len(), expected_results)
    }
    
    /// Process a C function call
    fn process_c_function_call(
        &mut self,
        function: super::rc_value::CFunction,
        base: u16,  
        nargs: usize,
        expected_results: i32,
    ) -> LuaResult<StepResult> {
        eprintln!("DEBUG process_c_function_call: base={}, nargs={}, expected_results={}", base, nargs, expected_results);
        
        // Create execution context for C function
        let mut ctx = VmExecutionContext {
            vm: self,
            base: base as usize,
            nargs,
            results_pushed: 0,
        };
        
        // Call the C function with our context
        let actual_results = function(&mut ctx)?;
        
        // Validate result count
        if actual_results < 0 {
            return Err(LuaError::RuntimeError(
                "C function returned negative result count".to_string()
            ));
        }
        
        eprintln!("DEBUG process_c_function_call: C function returned {} results", actual_results);
        
        // Get the actual number of results that were pushed  
        let results_pushed = ctx.results_pushed;
        eprintln!("DEBUG process_c_function_call: Context reports {} results pushed", results_pushed);
        
        // Adjust results to expected count if specified
        if expected_results >= 0 {
            let expected = expected_results as usize;
            eprintln!("DEBUG process_c_function_call: Adjusting result count to expected {}", expected);
            
            if results_pushed < expected {
                // Fill missing results with nil
                eprintln!("DEBUG process_c_function_call: Filling {} missing results with nil", expected - results_pushed);
                for i in results_pushed..expected {
                    self.set_register(base as usize + i, Value::Nil)?;
                }
            }
            // Note: We don't trim excess results, they're just ignored
        }
        
        Ok(StepResult::Continue)
    }
    
    /// Process a return operation
    fn process_return(&mut self, values: Vec<Value>) -> LuaResult<StepResult> {
        // If no call frames, return to caller
        if self.heap.get_call_depth(&self.current_thread) == 0 {
            return Ok(StepResult::Completed(values));
        }
        
        // Get current frame
        let frame = self.heap.pop_call_frame(&self.current_thread)?;
        let func_register = (frame.base_register as usize).saturating_sub(1);
        
        // Check if this was the last frame
        if self.heap.get_call_depth(&self.current_thread) == 0 {
            return Ok(StepResult::Completed(values));
        }
        
        // Place return values
        let result_count = if let Some(n) = frame.expected_results {
            n.min(values.len())
        } else {
            values.len()
        };
        
        for (i, value) in values.iter().take(result_count).enumerate() {
            self.set_register(func_register + i, value.clone())?;
        }
        
        // Fill missing expected results with nil
        if let Some(n) = frame.expected_results {
            for i in values.len()..n {
                self.set_register(func_register + i, Value::Nil)?;
            }
        }
        
        Ok(StepResult::Continue)
    }
    
    /// Process a TFORLOOP continuation after iterator call
    fn process_tforloop_continuation(
        &mut self,
        base: usize,
        a: usize,
        c: usize,
        pc_before_call: usize,
    ) -> LuaResult<StepResult> {
        // Get result of iterator call
        let first_result = self.get_register(base)?;
        
        if first_result.is_nil() {
            // End of iteration - skip the JMP instruction
            let pc = self.heap.get_pc(&self.current_thread)?;
            self.heap.set_pc(&self.current_thread, pc + 1)?;
            return Ok(StepResult::Continue);
        }
        
        // Continue iteration
        let iter_base = base - 3 - a;
        
        // Update control variable with first result
        self.set_register(iter_base + a + 2, first_result.clone())?;
        
        // Copy results to loop variables
        for i in 0..c {
            // Get result at index i, or nil if not enough results
            let value = self.get_register(base + i).unwrap_or(Value::Nil);
            self.set_register(iter_base + a + 3 + i, value)?;
        }
        
        // Jump back to TFORLOOP instruction
        self.heap.set_pc(&self.current_thread, pc_before_call)?;
        
        Ok(StepResult::Continue)
    }
    
    //
    // Register access helpers
    //
    
    /// Get a register value
    fn get_register(&self, index: usize) -> LuaResult<Value> {
        self.heap.get_register(&self.current_thread, index)
    }
    
    /// Set a register value
    fn set_register(&self, index: usize, value: Value) -> LuaResult<()> {
        self.heap.set_register(&self.current_thread, index, value)
    }
    
    /// Ensure sufficient stack space
    fn ensure_stack_space(&self, size: usize) -> LuaResult<()> {
        let current_size = self.heap.get_stack_size(&self.current_thread);
        if current_size < size {
            // Resize stack
            let mut thread = self.current_thread.borrow_mut();
            if thread.stack.len() < size {
                thread.stack.resize(size, Value::Nil);
            }
        }
        Ok(())
    }
    
    /// Helper method to safely set a register with bounds checking
    fn try_set_register(&self, index: usize, value: Value, allow_expansion: bool) -> LuaResult<()> {
        const MAX_STACK_SIZE: usize = 1_000_000; // Same limit as in heap
        
        if index >= MAX_STACK_SIZE {
            return Err(LuaError::StackOverflow);
        }
        
        let current_size = self.heap.get_stack_size(&self.current_thread);
        
        if index >= current_size && !allow_expansion {
            return Err(LuaError::RuntimeError(format!(
                "Register index {} out of bounds (stack size: {})", 
                index, current_size
            )));
        }
        
        // Set register with proper bounds checking
        self.heap.set_register(&self.current_thread, index, value)
    }
    
    /// Get instruction from closure
    fn get_instruction(&self, closure: &ClosureHandle, pc: usize) -> LuaResult<u32> {
        let closure_ref = closure.borrow();
        let proto_ref = &closure_ref.proto;
        
        if pc >= proto_ref.bytecode.len() {
            return Err(LuaError::RuntimeError(format!(
                "PC {} out of bounds (bytecode size: {})",
                pc, proto_ref.bytecode.len()
            )));
        }
        
        Ok(proto_ref.bytecode[pc])
    }
    
    /// Get constant from closure
    fn get_constant(&self, closure: &ClosureHandle, index: usize) -> LuaResult<Value> {
        let closure_ref = closure.borrow();
        let proto_ref = &closure_ref.proto;
        
        if index >= proto_ref.constants.len() {
            return Err(LuaError::RuntimeError(format!(
                "Constant index {} out of bounds (constants size: {})",
                index, proto_ref.constants.len()
            )));
        }
        
        Ok(proto_ref.constants[index].clone())
    }
    
    /// Read RK value (register or constant)
    fn read_rk(&self, base: usize, rk: u32) -> LuaResult<Value> {
        if rk & 0x100 != 0 {
            // Constant
            let frame = self.heap.get_current_frame(&self.current_thread)?;
            self.get_constant(&frame.closure, (rk & 0xFF) as usize)
        } else {
            // Register
            self.get_register(base + rk as usize)
        }
    }
    
    //
    // Opcode Implementations
    //
    
    /// MOVE: R(A) := R(B)
    fn op_move(&self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        let value = self.get_register(base + b)?;
        self.set_register(base + a, value)?;
        
        Ok(())
    }
    
    /// LOADK: R(A) := Kst(Bx)
    fn op_loadk(&self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let bx = inst.get_bx() as usize;
        
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        let constant = self.get_constant(&frame.closure, bx)?;
        
        self.set_register(base + a, constant)?;
        
        Ok(())
    }
    
    /// LOADBOOL: R(A) := (Bool)B; if (C) pc++
    fn op_loadbool(&self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b();
        let c = inst.get_c();
        
        // Set boolean value
        self.set_register(base + a, Value::Boolean(b != 0))?;
        
        // Skip next instruction if C is non-zero
        if c != 0 {
            let pc = self.heap.get_pc(&self.current_thread)?;
            self.heap.set_pc(&self.current_thread, pc + 1)?;
        }
        
        Ok(())
    }
    
    /// LOADNIL: R(A) := ... := R(B) := nil
    fn op_loadnil(&self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        // Set range to nil
        for i in a..=b {
            self.set_register(base + i, Value::Nil)?;
        }
        
        Ok(())
    }
    
    /// GETGLOBAL: R(A) := Gbl[Kst(Bx)]
    fn op_getglobal(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let bx = inst.get_bx() as usize;
        
        // Get key from constants
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        let key = self.get_constant(&frame.closure, bx)?;
        
        // Get globals table
        let globals = self.heap.globals();
        
        // Get value from globals
        let value = self.heap.get_table_field(&globals, &key)?;
        
        // Handle metamethods
        let final_value = match value {
            Value::PendingMetamethod(boxed_mm) => {
                // Queue metamethod call
                self.operation_queue.push_back(PendingOperation::MetamethodCall {
                    method: *boxed_mm,
                    args: vec![Value::Table(Rc::clone(&globals)), key.clone()],
                    expected_results: 1,
                    result_base: base + a,
                });
                return Ok(());
            },
            other => other,
        };
        
        // Store result
        self.set_register(base + a, final_value)?;
        
        Ok(())
    }
    
    /// SETGLOBAL: Gbl[Kst(Bx)] := R(A)
    fn op_setglobal(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let bx = inst.get_bx() as usize;
        
        // Get value to set
        let value = self.get_register(base + a)?;
        
        // Get key from constants
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        let key = self.get_constant(&frame.closure, bx)?;
        
        // Get globals table
        let globals = self.heap.globals();
        
        // Set value in globals
        self.heap.set_table_field(&globals, &key, &value)?;
        
        Ok(())
    }
    
    /// GETTABLE: R(A) := R(B)[RK(C)]
    fn op_gettable(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        let (c_is_const, c_idx) = inst.get_rk_c();
        
        // Get table
        let table_val = self.get_register(base + b)?;
        
        let table_handle = match table_val {
            Value::Table(ref handle) => handle,
            _ => {
                return Err(LuaError::TypeError {
                    expected: "table".to_string(),
                    got: table_val.type_name().to_string(),
                });
            }
        };
        
        // Get key
        let key = if c_is_const {
            let frame = self.heap.get_current_frame(&self.current_thread)?;
            self.get_constant(&frame.closure, c_idx as usize)?
        } else {
            self.get_register(base + c_idx as usize)?
        };
        
        // Get value from table
        let value = self.heap.get_table_field(table_handle, &key)?;
        
        // Handle metamethods
        let final_value = match value {
            Value::PendingMetamethod(boxed_mm) => {
                // Queue metamethod call
                self.operation_queue.push_back(PendingOperation::MetamethodCall {
                    method: *boxed_mm,
                    args: vec![Value::Table(Rc::clone(table_handle)), key.clone()],
                    expected_results: 1,
                    result_base: base + a,
                });
                return Ok(());
            },
            other => other,
        };
        
        // Store result
        self.set_register(base + a, final_value)?;
        
        Ok(())
    }
    
    /// SETTABLE: R(A)[RK(B)] := RK(C)
    fn op_settable(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let (b_is_const, b_idx) = inst.get_rk_b();
        let (c_is_const, c_idx) = inst.get_rk_c();
        
        // Get table
        let table_val = self.get_register(base + a)?;
        let table_handle = match table_val {
            Value::Table(ref handle) => handle,
            _ => {
                return Err(LuaError::TypeError {
                    expected: "table".to_string(),
                    got: table_val.type_name().to_string(),
                });
            }
        };
        
        // Get key
        let key = if b_is_const {
            let frame = self.heap.get_current_frame(&self.current_thread)?;
            self.get_constant(&frame.closure, b_idx as usize)?
        } else {
            self.get_register(base + b_idx as usize)?
        };
        
        // Get value
        let value = if c_is_const {
            let frame = self.heap.get_current_frame(&self.current_thread)?;
            self.get_constant(&frame.closure, c_idx as usize)?
        } else {
            self.get_register(base + c_idx as usize)?
        };
        
        // Check if key exists
        let exists = {
            let table_ref = table_handle.borrow();
            table_ref.get_field(&key).is_some()
        };
        
        if exists {
            self.heap.set_table_field(&table_handle, &key, &value)?;
        } else {
            let metatable_opt = {
                let table_ref = table_handle.borrow();
                table_ref.metatable.clone()
            };
            if let Some(metatable) = metatable_opt {
                let mt_ref = metatable.borrow();
                let newindex_key = Value::String(Rc::clone(&self.heap.metamethod_names.newindex));
                if let Some(newindex_mm) = mt_ref.get_field(&newindex_key) {
                    drop(mt_ref);
                    match newindex_mm {
                        Value::Table(newindex_table) => {
                            self.heap.set_table_field(&newindex_table, &key, &value)?;
                        }
                        Value::Closure(_) | Value::CFunction(_) => {
                            self.operation_queue.push_back(PendingOperation::MetamethodCall {
                                method: newindex_mm,
                                args: vec![Value::Table(Rc::clone(table_handle)), key, value],
                                expected_results: 0,
                                result_base: 0,
                            });
                        }
                        _ => {
                            self.heap.set_table_field(&table_handle, &key, &value)?;
                        }
                    }
                } else {
                    self.heap.set_table_field(&table_handle, &key, &value)?;
                }
            } else {
                self.heap.set_table_field(&table_handle, &key, &value)?;
            }
        }
        
        Ok(())
    }
    
    /// NEWTABLE: R(A) := {} (size = B,C)
    fn op_newtable(&self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b(); // Array size hint
        let c = inst.get_c(); // Hash size hint
        
        // Calculate array and hash sizes from the hints
        let array_size = if b == 0 { 0 } else { 1 << (b - 1) };
        let hash_size = if c == 0 { 0 } else { 1 << (c - 1) };
        
        // Create new table with capacity
        let table = self.heap.create_table_with_capacity(array_size, hash_size);
        
        // Store in register
        self.set_register(base + a, Value::Table(table))?;
        
        Ok(())
    }
    
    /// Additional opcode implementations would follow...
    // For brevity, I've included just a few essential opcodes here.
    // The complete implementation would include all opcodes as specified in the
    // LUA_OPCODE_REGISTER_CONVENTIONS_UPDATED.md file.
    
    // MOVE, LOADK, LOADBOOL, LOADNIL, GETGLOBAL, SETGLOBAL, GETTABLE, SETTABLE, NEWTABLE
    // are already implemented above.

    /// SELF: R(A+1) := R(B); R(A) := R(B)[RK(C)]
    fn op_self(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        let (c_is_const, c_idx) = inst.get_rk_c();
        
        // Get table
        let table_val = self.get_register(base + b)?;
        
        match &table_val {
            Value::Table(table_handle) => {
                // Get key
                let key = if c_is_const {
                    let frame = self.heap.get_current_frame(&self.current_thread)?;
                    self.get_constant(&frame.closure, c_idx as usize)?
                } else {
                    self.get_register(base + c_idx as usize)?
                };
                
                // Set self (R(A+1) = R(B))
                self.set_register(base + a + 1, table_val.clone())?;
                
                // Get method (R(A) = R(B)[RK(C)])
                let method = self.heap.get_table_field(table_handle, &key)?;
                
                // Handle metamethods
                match method {
                    Value::PendingMetamethod(boxed_mm) => {
                        // Queue metamethod call
                        self.operation_queue.push_back(PendingOperation::MetamethodCall {
                            method: *boxed_mm,
                            args: vec![Value::Table(Rc::clone(&table_handle)), key.clone()],
                            expected_results: 1,
                            result_base: base + a,
                        });
                    },
                    _ => {
                        // Store method
                        self.set_register(base + a, method)?;
                    }
                }
            },
            _ => {
                return Err(LuaError::TypeError {
                    expected: "table".to_string(),
                    got: table_val.type_name().to_string(),
                });
            }
        }
        
        Ok(())
    }
    
    /// ADD: R(A) := RK(B) + RK(C)
    fn op_add(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let (b_is_const, b_idx) = inst.get_rk_b();
        let (c_is_const, c_idx) = inst.get_rk_c();
        
        eprintln!("DEBUG ADD: Executing at base={}, A={}, B_is_const={}, B_idx={}, C_is_const={}, C_idx={}",
                 base, a, b_is_const, b_idx, c_is_const, c_idx);
        
        // Get operands with detailed tracing
        let left = if b_is_const {
            let frame = self.heap.get_current_frame(&self.current_thread)?;
            let constant = self.get_constant(&frame.closure, b_idx as usize)?;
            eprintln!("DEBUG ADD: Left operand from constant[{}]: {:?}", b_idx, constant);
            constant
        } else {
            let register_idx = base + b_idx as usize;
            eprintln!("DEBUG ADD: Reading left operand from register R({}) (absolute index {})", 
                     b_idx, register_idx);
            let value = self.get_register(register_idx)?;
            eprintln!("DEBUG ADD: Left operand value: {:?}", value);
            value
        };
        
        let right = if c_is_const {
            let frame = self.heap.get_current_frame(&self.current_thread)?;
            let constant = self.get_constant(&frame.closure, c_idx as usize)?;
            eprintln!("DEBUG ADD: Right operand from constant[{}]: {:?}", c_idx, constant);
            constant
        } else {
            let register_idx = base + c_idx as usize;
            eprintln!("DEBUG ADD: Reading right operand from register R({}) (absolute index {})", 
                     c_idx, register_idx);
            let value = self.get_register(register_idx)?;
            eprintln!("DEBUG ADD: Right operand value: {:?}", value);
            value
        };
        
        eprintln!("DEBUG ADD: Performing addition between: {:?} and {:?}", left, right);
        
        // Verify both operands are numbers before proceeding
        let result = match (&left, &right) {
            (Value::Number(l), Value::Number(r)) => {
                eprintln!("DEBUG ADD: Numeric addition: {} + {} = {}", l, r, l + r);
                Ok(Value::Number(l + r))
            },
            _ => {
                // If either operand is not a number, try metamethods first
                let metamethod = self.find_metamethod(&left, &Value::String(Rc::clone(&self.heap.metamethod_names.add)))?;
                
                if let Some(mm) = metamethod {
                    eprintln!("DEBUG ADD: Found metamethod __add, queuing call");
                    // Queue metamethod call
                    self.operation_queue.push_back(PendingOperation::MetamethodCall {
                        method: mm,
                        args: vec![left.clone(), right.clone()],
                        expected_results: 1,
                        result_base: base + a,
                    });
                    return Ok(());
                }
                
                eprintln!("DEBUG ADD: No metamethod found, checking types");
                // Detailed error for debugging
                if !left.is_number() {
                    eprintln!("DEBUG ADD: Left operand is not a number: {:?} (type: {})", 
                             left, left.type_name());
                    return Err(LuaError::TypeError {
                        expected: "number".to_string(),
                        got: format!("{} ({})", left.type_name(), if left.is_nil() { "nil" } else { "not nil" }),
                    });
                } else {
                    eprintln!("DEBUG ADD: Right operand is not a number: {:?} (type: {})", 
                             right, right.type_name());
                    return Err(LuaError::TypeError {
                        expected: "number".to_string(),
                        got: right.type_name().to_string(),
                    });
                }
            }
        }?;
        
        // Store result
        eprintln!("DEBUG ADD: Storing result {:?} in register R({})", result, base + a);
        self.set_register(base + a, result)?;
        
        Ok(())
    }
    
    /// SUB: R(A) := RK(B) - RK(C)
    fn op_sub(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        self.op_arithmetic(inst, base, ArithOp::Sub)
    }
    
    /// MUL: R(A) := RK(B) * RK(C)
    fn op_mul(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        self.op_arithmetic(inst, base, ArithOp::Mul)
    }
    
    /// DIV: R(A) := RK(B) / RK(C)
    fn op_div(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        self.op_arithmetic(inst, base, ArithOp::Div)
    }
    
    /// MOD: R(A) := RK(B) % RK(C)
    fn op_mod(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        self.op_arithmetic(inst, base, ArithOp::Mod)
    }
    
    /// POW: R(A) := RK(B) ^ RK(C)
    fn op_pow(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        self.op_arithmetic(inst, base, ArithOp::Pow)
    }
    
    /// Generic arithmetic operation handler
    fn op_arithmetic(&mut self, inst: Instruction, base: usize, op: ArithOp) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let (b_is_const, b_idx) = inst.get_rk_b();
        let (c_is_const, c_idx) = inst.get_rk_c();
        
        eprintln!("DEBUG op_arithmetic: Executing {:?} with A={}, B_is_const={}, B_idx={}, C_is_const={}, C_idx={}, base={}",
                 op, a, b_is_const, b_idx, c_is_const, c_idx, base);
        
        // Get full current context for better understanding during debug
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        let closure = frame.closure.clone();
        let closure_ref = closure.borrow();
        eprintln!("DEBUG op_arithmetic: Current function has {} upvalues, {} constants, bytecode length {}", 
                 closure_ref.upvalues.len(),
                 closure_ref.proto.constants.len(),
                 closure_ref.proto.bytecode.len());
        
        // Get operands with detailed debugging
        let left = if b_is_const {
            let constant = self.get_constant(&frame.closure, b_idx as usize)?;
            eprintln!("DEBUG op_arithmetic: Left operand from constant[{}]: {:?}", b_idx, constant);
            constant
        } else {
            let register_idx = base + b_idx as usize;
            let value = self.get_register(register_idx)?;
            eprintln!("DEBUG op_arithmetic: Left operand from register R({}) (absolute {}): {:?}", 
                     b_idx, register_idx, value);
            
            // If it's Nil and we don't expect it to be, something is wrong with our register allocation
            if value.is_nil() {
                eprintln!("DEBUG op_arithmetic: WARNING - Left operand is nil, but expected a number. Check register allocation.");
            }
            
            value
        };
        
        let right = if c_is_const {
            let constant = self.get_constant(&frame.closure, c_idx as usize)?;
            eprintln!("DEBUG op_arithmetic: Right operand from constant[{}]: {:?}", c_idx, constant);
            constant
        } else {
            let register_idx = base + c_idx as usize;
            let value = self.get_register(register_idx)?;
            eprintln!("DEBUG op_arithmetic: Right operand from register R({}) (absolute {}): {:?}", 
                     c_idx, register_idx, value);
            
            // If it's Nil and we don't expect it to be, something is wrong with our register allocation
            if value.is_nil() {
                eprintln!("DEBUG op_arithmetic: WARNING - Right operand is nil, but expected a number. Check register allocation.");
            }
            
            value
        };
        
        // Register debug dump for better understanding
        let stack_size = self.heap.get_stack_size(&self.current_thread);
        eprintln!("DEBUG op_arithmetic: Stack dump (size {}, base {})", stack_size, base);
        for i in 0..(stack_size.min(base + 10)) {
            if let Ok(value) = self.get_register(i) {
                let is_base = if i == base { " (BASE)" } else { "" };
                let operand_marker = if !b_is_const && i == base + b_idx as usize {
                    " <- LEFT OPERAND"
                } else if !c_is_const && i == base + c_idx as usize {
                    " <- RIGHT OPERAND"
                } else if i == base + a {
                    " <- RESULT TARGET"
                } else {
                    ""
                };
                
                eprintln!("  R({}) = {:?}{}{}", i, value, is_base, operand_marker);
            }
        }
        
        eprintln!("DEBUG op_arithmetic: Comparing {:?} {:?} {:?}", left, op, right);
        
        // Try to perform the operation according to Lua 5.1 semantics
        let result = match (&left, &right, op) {
            // First, check for nil values (Lua gives clear errors for these)
            (Value::Nil, _, _) => {
                eprintln!("DEBUG op_arithmetic: Left operand is nil");
                
                // Try metamethod first before giving up
                let mm_key = match op {
                    ArithOp::Add => &self.heap.metamethod_names.add,
                    ArithOp::Sub => &self.heap.metamethod_names.sub,
                    ArithOp::Mul => &self.heap.metamethod_names.mul,
                    ArithOp::Div => &self.heap.metamethod_names.div,
                    ArithOp::Mod => &self.heap.metamethod_names.mod_op,
                    ArithOp::Pow => &self.heap.metamethod_names.pow,
                };
                
                // Try metamethods on both operands
                if let Some(mm) = self.find_metamethod(&left, &Value::String(Rc::clone(mm_key)))? {
                    eprintln!("DEBUG op_arithmetic: Found metamethod for left operand");
                    self.operation_queue.push_back(PendingOperation::MetamethodCall {
                        method: mm,
                        args: vec![left.clone(), right.clone()],
                        expected_results: 1,
                        result_base: base + a,
                    });
                    return Ok(());
                }
                
                if let Some(mm) = self.find_metamethod(&right, &Value::String(Rc::clone(mm_key)))? {
                    eprintln!("DEBUG op_arithmetic: Found metamethod for right operand");
                    self.operation_queue.push_back(PendingOperation::MetamethodCall {
                        method: mm,
                        args: vec![left.clone(), right.clone()],
                        expected_results: 1,
                        result_base: base + a,
                    });
                    return Ok(());
                }
                
                return Err(LuaError::TypeError {
                    expected: "number".to_string(),
                    got: "nil".to_string(),
                });
            },
            (_, Value::Nil, _) => {
                eprintln!("DEBUG op_arithmetic: Right operand is nil");
                
                // Try metamethod first before giving up
                let mm_key = match op {
                    ArithOp::Add => &self.heap.metamethod_names.add,
                    ArithOp::Sub => &self.heap.metamethod_names.sub,
                    ArithOp::Mul => &self.heap.metamethod_names.mul,
                    ArithOp::Div => &self.heap.metamethod_names.div,
                    ArithOp::Mod => &self.heap.metamethod_names.mod_op,
                    ArithOp::Pow => &self.heap.metamethod_names.pow,
                };
                
                // Try metamethods on both operands
                if let Some(mm) = self.find_metamethod(&left, &Value::String(Rc::clone(mm_key)))? {
                    eprintln!("DEBUG op_arithmetic: Found metamethod for left operand");
                    self.operation_queue.push_back(PendingOperation::MetamethodCall {
                        method: mm,
                        args: vec![left.clone(), right.clone()],
                        expected_results: 1,
                        result_base: base + a,
                    });
                    return Ok(());
                }
                
                if let Some(mm) = self.find_metamethod(&right, &Value::String(Rc::clone(mm_key)))? {
                    eprintln!("DEBUG op_arithmetic: Found metamethod for right operand");
                    self.operation_queue.push_back(PendingOperation::MetamethodCall {
                        method: mm,
                        args: vec![left.clone(), right.clone()],
                        expected_results: 1,
                        result_base: base + a,
                    });
                    return Ok(());
                }
                
                return Err(LuaError::TypeError {
                    expected: "number".to_string(),
                    got: "nil".to_string(),
                });
            },
            
            // Normal number operations
            (Value::Number(l), Value::Number(r), ArithOp::Add) => {
                eprintln!("DEBUG op_arithmetic: Numeric addition: {} + {}", l, r);
                Ok(Value::Number(l + r))
            },
            (Value::Number(l), Value::Number(r), ArithOp::Sub) => Ok(Value::Number(l - r)),
            (Value::Number(l), Value::Number(r), ArithOp::Mul) => Ok(Value::Number(l * r)),
            (Value::Number(l), Value::Number(r), ArithOp::Div) => {
                if *r == 0.0 {
                    Err(LuaError::RuntimeError("Division by zero".to_string()))
                } else {
                    Ok(Value::Number(l / r))
                }
            },
            (Value::Number(l), Value::Number(r), ArithOp::Mod) => {
                if *r == 0.0 {
                    Err(LuaError::RuntimeError("Modulo by zero".to_string()))
                } else {
                    Ok(Value::Number(l % r))
                }
            },
            (Value::Number(l), Value::Number(r), ArithOp::Pow) => Ok(Value::Number(l.powf(*r))),
            
            // If not matching any of the above, try metamethods before giving type error
            _ => {
                eprintln!("DEBUG op_arithmetic: Non-numeric operands, checking for metamethods");
                
                // Try metamethods
                let mm_key = match op {
                    ArithOp::Add => &self.heap.metamethod_names.add,
                    ArithOp::Sub => &self.heap.metamethod_names.sub,
                    ArithOp::Mul => &self.heap.metamethod_names.mul,
                    ArithOp::Div => &self.heap.metamethod_names.div,
                    ArithOp::Mod => &self.heap.metamethod_names.mod_op,
                    ArithOp::Pow => &self.heap.metamethod_names.pow,
                };
                
                // Try metamethods on both operands
                if let Some(mm) = self.find_metamethod(&left, &Value::String(Rc::clone(mm_key)))? {
                    eprintln!("DEBUG op_arithmetic: Found metamethod for left operand");
                    self.operation_queue.push_back(PendingOperation::MetamethodCall {
                        method: mm,
                        args: vec![left.clone(), right.clone()],
                        expected_results: 1,
                        result_base: base + a,
                    });
                    return Ok(());
                }
                
                if let Some(mm) = self.find_metamethod(&right, &Value::String(Rc::clone(mm_key)))? {
                    eprintln!("DEBUG op_arithmetic: Found metamethod for right operand");
                    self.operation_queue.push_back(PendingOperation::MetamethodCall {
                        method: mm,
                        args: vec![left.clone(), right.clone()],
                        expected_results: 1,
                        result_base: base + a,
                    });
                    return Ok(());
                }
                
                // No metamethods, give type error
                eprintln!("DEBUG op_arithmetic: No valid metamethods, giving type error");
                let left_type = left.type_name();
                let right_type = right.type_name();
                Err(LuaError::TypeError {
                    expected: "number".to_string(),
                    got: format!("{} and {}", left_type, right_type),
                })
            }
        }?;
        
        // Store result
        eprintln!("DEBUG op_arithmetic: Storing result {:?} in register R({}) (absolute {})", 
                 result, a, base + a);
        self.set_register(base + a, result)?;
        
        Ok(())
    }
    
    /// UNM: R(A) := -R(B)
    fn op_unm(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        // Get operand
        let operand = self.get_register(base + b)?;
        
        // Try to negate
        let result = match &operand {
            Value::Number(n) => Ok(Value::Number(-*n)),
            _ => {
                // Try metamethod
                let metamethod = self.find_metamethod(&operand, &Value::String(Rc::clone(&self.heap.metamethod_names.unm)))?;
                
                if let Some(mm) = metamethod {
                    // Queue metamethod call
                    self.operation_queue.push_back(PendingOperation::MetamethodCall {
                        method: mm,
                        args: vec![operand.clone()],
                        expected_results: 1,
                        result_base: base + a,
                    });
                    return Ok(());
                }
                
                // No metamethod found
                Err(LuaError::TypeError {
                    expected: "number".to_string(),
                    got: operand.type_name().to_string(),
                })
            }
        }?;
        
        // Store result
        self.set_register(base + a, result)?;
        
        Ok(())
    }
    
    /// NOT: R(A) := not R(B)
    fn op_not(&self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        // Get operand
        let operand = self.get_register(base + b)?;
        
        // Perform logical not
        let result = Value::Boolean(operand.is_falsey());
        
        // Store result
        self.set_register(base + a, result)?;
        
        Ok(())
    }
    
    /// LEN: R(A) := length of R(B)
    fn op_len(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        // Get operand
        let operand = self.get_register(base + b)?;
        
        // Get length
        let result = match &operand {
            Value::String(ref handle) => {
                let string_ref = handle.borrow();
                Ok(Value::Number(string_ref.len() as f64))
            },
            Value::Table(ref handle) => {
                // Check for metamethod
                let metamethod = self.find_metamethod(&operand, &Value::String(Rc::clone(&self.heap.metamethod_names.len)))?;
                
                if let Some(mm) = metamethod {
                    // Queue metamethod call
                    self.operation_queue.push_back(PendingOperation::MetamethodCall {
                        method: mm,
                        args: vec![operand.clone()],
                        expected_results: 1,
                        result_base: base + a,
                    });
                    return Ok(());
                }
                
                // No metamethod, use array length
                let table_ref = handle.borrow();
                Ok(Value::Number(table_ref.array_len() as f64))
            },
            _ => Err(LuaError::TypeError {
                expected: "string or table".to_string(),
                got: operand.type_name().to_string(),
            }),
        }?;
        
        // Store result
        self.set_register(base + a, result)?;
        
        Ok(())
    }
    
    /// CONCAT: R(A) := R(B).. ... ..R(C)
    fn op_concat(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        let c = inst.get_c() as usize;
        
        // If only one value, just copy it
        if b == c {
            let value = self.get_register(base + b)?;
            self.set_register(base + a, value)?;
            return Ok(());
        }
        
        // Collect values to concatenate
        let mut values = Vec::with_capacity(c - b + 1);
        for i in b..=c {
            values.push(self.get_register(base + i)?);
        }
        
        // Check if all values can be converted to strings
        let mut can_concat_directly = true;
        let mut first_non_stringable_idx = 0;
        
        for (idx, value) in values.iter().enumerate() {
            match value {
                Value::String(_) | Value::Number(_) => {
                    // These can be concatenated directly
                },
                _ => {
                    // Found a value that needs metamethod checking
                    can_concat_directly = false;
                    first_non_stringable_idx = idx;
                    break;
                }
            }
        }
        
        if can_concat_directly {
            // All values are strings or numbers, concatenate directly
            let mut result = String::new();
            
            for value in values {
                match value {
                    Value::String(handle) => {
                        let string_ref = handle.borrow();
                        match string_ref.to_str() {
                            Ok(s) => result.push_str(s),
                            Err(_) => return Err(LuaError::RuntimeError("Invalid UTF-8 in string".to_string())),
                        }
                    },
                    Value::Number(n) => {
                        result.push_str(&n.to_string());
                    },
                    _ => unreachable!(), // We checked above
                }
            }
            
            // Create result string
            let string_handle = self.heap.create_string(&result)?;
            
            // Store result
            self.set_register(base + a, Value::String(string_handle))?;
        } else {
            // Need to handle metamethods
            // In Lua 5.1, concatenation with metamethods is done pairwise
            
            // For now, handle the simple case of exactly 2 values
            if values.len() == 2 {
                let left = &values[0];
                let right = &values[1];
                
                // Check if either value is not string/number
                let left_needs_mm = !matches!(left, Value::String(_) | Value::Number(_));
                let right_needs_mm = !matches!(right, Value::String(_) | Value::Number(_));
                
                if left_needs_mm || right_needs_mm {
                    // Check for __concat metamethod
                    let concat_key = Value::String(Rc::clone(&self.heap.metamethod_names.concat));
                    
                    // Try left operand first
                    if let Some(mm) = self.find_metamethod(left, &concat_key)? {
                        self.operation_queue.push_back(PendingOperation::MetamethodCall {
                            method: mm,
                            args: vec![left.clone(), right.clone()],
                            expected_results: 1,
                            result_base: base + a,
                        });
                        return Ok(());
                    }
                    
                    // Try right operand
                    if let Some(mm) = self.find_metamethod(right, &concat_key)? {
                        self.operation_queue.push_back(PendingOperation::MetamethodCall {
                            method: mm,
                            args: vec![left.clone(), right.clone()],
                            expected_results: 1,
                            result_base: base + a,
                        });
                        return Ok(());
                    }
                    
                    // No metamethod found, give appropriate error
                    let problematic = if left_needs_mm { left } else { right };
                    return Err(LuaError::TypeError {
                        expected: "string or number".to_string(),
                        got: problematic.type_name().to_string(),
                    });
                }
            } else {
                // Multiple values with at least one non-string/number
                // For a complete implementation, we would need to handle pairwise concatenation
                // For now, we'll check the first non-stringable value for a metamethod
                
                if first_non_stringable_idx > 0 {
                    // We have some stringable values before the problematic one
                    // In a full implementation, we'd concatenate the stringable ones first
                }
                
                let problematic_value = &values[first_non_stringable_idx];
                let next_value = values.get(first_non_stringable_idx + 1)
                    .unwrap_or(problematic_value);
                
                // Check for metamethod on the problematic value
                let concat_key = Value::String(Rc::clone(&self.heap.metamethod_names.concat));
                
                if let Some(mm) = self.find_metamethod(problematic_value, &concat_key)? {
                    // For simplicity, just handle the pair for now
                    self.operation_queue.push_back(PendingOperation::MetamethodCall {
                        method: mm,
                        args: vec![problematic_value.clone(), next_value.clone()],
                        expected_results: 1,
                        result_base: base + a,
                    });
                    return Ok(());
                }
                
                // Check the next value if different
                if first_non_stringable_idx + 1 < values.len() {
                    if let Some(mm) = self.find_metamethod(next_value, &concat_key)? {
                        self.operation_queue.push_back(PendingOperation::MetamethodCall {
                            method: mm,
                            args: vec![problematic_value.clone(), next_value.clone()],
                            expected_results: 1,
                            result_base: base + a,
                        });
                        return Ok(());
                    }
                }
                
                // No metamethod found
                return Err(LuaError::TypeError {
                    expected: "string or number".to_string(),
                    got: problematic_value.type_name().to_string(),
                });
            }
        }
        
        Ok(())
    }
    
    /// JMP: pc += sBx
    fn op_jmp(&mut self, inst: Instruction) -> LuaResult<()> {
        let sbx = inst.get_sbx();
        
        // Get current PC
        let pc = self.heap.get_pc(&self.current_thread)?;
        
        // Add offset
        let new_pc = (pc as isize + sbx as isize) as usize;
        
        // Set new PC
        self.heap.set_pc(&self.current_thread, new_pc)?;
        
        Ok(())
    }
    
    /// CALL: R(A), ..., R(A+C-2) := R(A)(R(A+1), ..., R(A+B-1))
    fn op_call(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b();
        let c = inst.get_c();
        
        // Get function
        let func = self.get_register(base + a)?;
        
        // Determine argument count
        let arg_count = if b == 0 {
            // All values above the function
            self.heap.get_stack_size(&self.current_thread) - (base + a + 1)
        } else {
            (b - 1) as usize
        };
        
        // Collect arguments
        let mut args = Vec::with_capacity(arg_count);
        for i in 0..arg_count {
            args.push(self.get_register(base + a + 1 + i)?);
        }
        
        // Determine expected results
        let expected_results = if c == 0 {
            -1 // All results
        } else {
            (c - 1) as i32
        };
        
        // Queue function call
        self.operation_queue.push_back(PendingOperation::FunctionCall {
            func,
            args,
            expected_results,
            result_base: base + a,
        });
        
        Ok(())
    }
    
    /// RETURN: return R(A), ..., R(A+B-2)
    fn op_return(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b();
        
        eprintln!("DEBUG RETURN: Executing with A={}, B={}, base={}", a, b, base);
        
        // Get current frame info for debugging
        let frame_info = {
            let frame = self.heap.get_current_frame(&self.current_thread)?;
            let closure_ref = frame.closure.borrow();
            eprintln!("DEBUG RETURN: Returning from function with {} upvalues", closure_ref.upvalues.len());
            eprintln!("DEBUG RETURN: Current PC: {}, base_register: {}", frame.pc, frame.base_register);
            drop(closure_ref);
            frame.base_register
        };
        
        eprintln!("DEBUG RETURN: Closing all upvalues at or above stack index {}", base);
        
        // First, close all upvalues at or above base
        self.heap.close_upvalues(&self.current_thread, base)?;
        
        eprintln!("DEBUG RETURN: Finished closing upvalues");
        
        // Collect return values
        let mut values = Vec::new();
        
        if b == 0 {
            // Return all values from R(A) to stack top
            let stack_size = self.heap.get_stack_size(&self.current_thread);
            eprintln!("DEBUG RETURN: Returning all values from R({}) to stack top ({})", 
                     base + a, stack_size);
            
            for i in 0..(stack_size - base - a) {
                values.push(self.get_register(base + a + i)?);
            }
        } else {
            // Return specific number of values
            eprintln!("DEBUG RETURN: Returning {} values starting from R({})", b - 1, base + a);
            
            for i in 0..((b - 1) as usize) {
                if let Ok(value) = self.get_register(base + a + i) {
                    values.push(value);
                } else {
                    values.push(Value::Nil);
                }
            }
        }
        
        eprintln!("DEBUG RETURN: Collected {} return values", values.len());
        for (i, val) in values.iter().enumerate() {
            eprintln!("  Return[{}]: {:?}", i, val);
        }
        
        // Queue return operation
        self.operation_queue.push_back(PendingOperation::Return { values });
        
        Ok(())
    }
    
    /// CLOSURE: R(A) := closure(KPROTO[Bx], upvalues...)
    fn op_closure(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let bx = inst.get_bx() as usize;
        
        eprintln!("DEBUG CLOSURE: a={}, bx={}, base={}", a, bx, base);
        
        // Get current frame
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        let closure_ref = frame.closure.borrow();
        
        eprintln!("DEBUG CLOSURE: Current PC: {}", frame.pc);
        
        // Get function prototype
        let proto_value = match &closure_ref.proto.constants[bx] {
            Value::FunctionProto(handle) => Rc::clone(handle),
            _ => {
                drop(closure_ref);
                return Err(LuaError::RuntimeError(
                    format!("Constant {} is not a function prototype", bx)
                ));
            }
        };
        
        // Get number of upvalues
        let num_upvalues = proto_value.upvalues.len();
        eprintln!("DEBUG CLOSURE: Function has {} upvalues", num_upvalues);
        
        // Drop current closure borrow
        drop(closure_ref);
        
        // Get the CURRENT PC (already incremented, pointing to first pseudo-instruction)
        let current_pc = self.heap.get_pc(&self.current_thread)?;
        
        // Create upvalues list
        let mut upvalues = Vec::with_capacity(num_upvalues);
        
        // Process each upvalue
        for (i, upvalue_info) in proto_value.upvalues.iter().enumerate() {
            // Read the pseudo-instruction for this upvalue
            let pseudo_inst = self.get_instruction(&frame.closure, current_pc + i)?;
            let pseudo = Instruction(pseudo_inst);
            
            eprintln!("DEBUG CLOSURE: Processing upvalue {} with pseudo-instruction {:?} at PC={}", 
                     i, pseudo.get_opcode(), current_pc + i);
            
            // Create upvalue based on type
            let upvalue = match pseudo.get_opcode() {
                OpCode::Move => {
                    // Local variable - get its register index
                    let idx = pseudo.get_b() as usize;
                    eprintln!("DEBUG CLOSURE: Upvalue {} is local var at parent register {}", i, idx);
                    eprintln!("DEBUG CLOSURE: Will look for stack value at absolute index {}", base + idx);
                    
                    // Debug - dump the stack value at this location
                    let stack_value = self.get_register(base + idx)?;
                    eprintln!("DEBUG CLOSURE: Stack value at R({}) (abs: {}): {:?}", 
                             idx, base + idx, stack_value);
                    
                    // Create upvalue for this local
                    self.heap.find_or_create_upvalue(&self.current_thread, base + idx)?
                },
                OpCode::GetUpval => {
                    // Upvalue from parent
                    let idx = pseudo.get_b() as usize;
                    eprintln!("DEBUG CLOSURE: Upvalue {} refers to parent upvalue {}", i, idx);
                    
                    let parent_closure_ref = frame.closure.borrow();
                    if idx >= parent_closure_ref.upvalues.len() {
                        drop(parent_closure_ref);
                        return Err(LuaError::RuntimeError(
                            format!("Upvalue index {} out of bounds", idx)
                        ));
                    }
                    
                    // Share the parent's upvalue
                    Rc::clone(&parent_closure_ref.upvalues[idx])
                },
                _ => {
                    eprintln!("DEBUG CLOSURE: Invalid pseudo-instruction: {:?}", pseudo.get_opcode());
                    return Err(LuaError::RuntimeError(
                        format!("Invalid pseudo-instruction for upvalue: {:?}", pseudo.get_opcode())
                    ));
                }
            };
            
            upvalues.push(upvalue);
        }
        
        // Create the closure
        let new_closure = self.heap.create_closure(Rc::clone(&proto_value), upvalues);
        
        // Store in register
        eprintln!("DEBUG CLOSURE: Storing closure in register R({}) (absolute: {})", 
                 a, base + a);
        self.set_register(base + a, Value::Closure(new_closure))?;
        
        // Update PC to skip all the pseudo-instructions we just read
        let new_pc = current_pc + num_upvalues;
        eprintln!("DEBUG CLOSURE: Updating PC: {} -> {}", current_pc, new_pc);
        self.heap.set_pc(&self.current_thread, new_pc)?;
        
        Ok(())
    }
    
    /// FORPREP: R(A) -= R(A+2); pc += sBx
    fn op_forprep(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let sbx = inst.get_sbx();
        
        // Get loop values
        let initial = self.get_register(base + a)?;
        let limit = self.get_register(base + a + 1)?;
        let step = self.get_register(base + a + 2)?;
        
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
        
        // Step defaults to 1.0 if nil
        let step_num = match step {
            Value::Number(n) => n,
            Value::Nil => {
                self.set_register(base + a + 2, Value::Number(1.0))?;
                1.0
            },
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: step.type_name().to_string(),
            }),
        };
        
        // Check step != 0
        if step_num == 0.0 {
            return Err(LuaError::RuntimeError("For loop step cannot be zero".to_string()));
        }
        
        // Subtract step from initial value
        let prepared = initial_num - step_num;
        self.set_register(base + a, Value::Number(prepared))?;
        
        // ALWAYS jump to FORLOOP (sBx points to it)
        let pc = self.heap.get_pc(&self.current_thread)?;
        let new_pc = (pc as isize + sbx as isize) as usize;
        self.heap.set_pc(&self.current_thread, new_pc)?;
        
        Ok(())
    }
    
    /// FORLOOP: R(A) += R(A+2); if R(A) <?= R(A+1) then { R(A+3) = R(A); pc -= sBx }
    fn op_forloop(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let sbx = inst.get_sbx();
        
        // Get loop values
        let counter = self.get_register(base + a)?;
        let limit = self.get_register(base + a + 1)?;
        let step = self.get_register(base + a + 2)?;
        
        // Convert to numbers
        let counter_num = match counter {
            Value::Number(n) => n,
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: counter.type_name().to_string(),
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
        
        // Increment counter by step
        let new_counter = counter_num + step_num;
        self.set_register(base + a, Value::Number(new_counter))?;
        
        // Check loop condition based on step sign
        let should_continue = if step_num > 0.0 {
            new_counter <= limit_num
        } else {
            new_counter >= limit_num
        };
        
        if should_continue {
            // Update user variable
            self.set_register(base + a + 3, Value::Number(new_counter))?;
            
            // Jump back to the start of the loop
            let pc = self.heap.get_pc(&self.current_thread)?;
            let new_pc = (pc as isize + sbx as isize) as usize;
            self.heap.set_pc(&self.current_thread, new_pc)?;
        }
        
        Ok(())
    }
    
    /// TFORLOOP: R(A+3), ... ,R(A+2+C) := R(A)(R(A+1), R(A+2));
    fn op_tforloop(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let c = inst.get_c() as usize;
        
        // Get iterator function, state, and control variable
        let iter_func = self.get_register(base + a)?;
        let state = self.get_register(base + a + 1)?;
        let control = self.get_register(base + a + 2)?;
        
        // Queue function call with continuation
        let result_base = base + a + 3;
        
        self.operation_queue.push_back(PendingOperation::FunctionCall {
            func: iter_func,
            args: vec![state, control],
            expected_results: c as i32,
            result_base,
        });
        
        // Queue continuation
        self.operation_queue.push_back(PendingOperation::TForLoopContinuation {
            base: result_base,
            a,
            c,
            pc_before_call: self.heap.get_pc(&self.current_thread)? - 1, // PC of TFORLOOP
        });
        
        Ok(())
    }
    
    /// GETUPVAL: R(A) := UpValue[B]
    fn op_getupval(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        eprintln!("DEBUG GETUPVAL: Executing with A={}, B={}, base={}", a, b, base);
        
        // Get current frame and closure
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        let closure_ref = frame.closure.borrow();
        
        eprintln!("DEBUG GETUPVAL: Closure has {} upvalues", closure_ref.upvalues.len());
        
        // Check upvalue index
        if b >= closure_ref.upvalues.len() {
            eprintln!("DEBUG GETUPVAL: Upvalue index {} out of bounds", b);
            drop(closure_ref);
            return Err(LuaError::RuntimeError(format!(
                "Upvalue index {} out of bounds", b
            )));
        }
        
        // Get upvalue handle
        let upvalue = Rc::clone(&closure_ref.upvalues[b]);
        drop(closure_ref);
        
        // Dump upvalue state for debugging
        {
            let uv_ref = upvalue.borrow();
            match &*uv_ref {
                UpvalueState::Open { stack_index, .. } => {
                    eprintln!("DEBUG GETUPVAL: Upvalue is OPEN, points to stack index {}", stack_index);
                    
                    // Debug dump the stack value and surrounding context
                    let thread_ref = self.current_thread.borrow();
                    eprintln!("DEBUG GETUPVAL: Current stack size: {}", thread_ref.stack.len());
                    eprintln!("DEBUG GETUPVAL: Current base: {}", base);
                    
                    // Dump stack around the upvalue index
                    let start = stack_index.saturating_sub(2);
                    let end = (*stack_index + 3).min(thread_ref.stack.len());
                    
                    eprintln!("DEBUG GETUPVAL: Stack dump around upvalue:");
                    for i in start..end {
                        let marker = if i == *stack_index { " <-- UPVALUE" } else { "" };
                        eprintln!("  stack[{}]: {:?}{}", i, thread_ref.stack.get(i), marker);
                    }
                },
                UpvalueState::Closed { value } => {
                    eprintln!("DEBUG GETUPVAL: Upvalue is CLOSED with value: {:?}", value);
                }
            }
        }
        
        // Get the value from upvalue
        let value = self.heap.get_upvalue_value(&upvalue);
        
        eprintln!("DEBUG GETUPVAL: Retrieved upvalue value: {:?}", value);
        eprintln!("DEBUG GETUPVAL: Setting register {} (absolute {}) to value: {:?}", 
                 a, base + a, value);
        
        // Set the target register
        self.set_register(base + a, value)?;
        
        eprintln!("DEBUG GETUPVAL: Operation complete");
        
        Ok(())
    }
    
    /// SETUPVAL: UpValue[A] := R(B)
    fn op_setupval(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        eprintln!("DEBUG SETUPVAL: Executing with A={}, B={}, base={}", a, b, base);
        
        // Get value to store
        let value = self.get_register(base + b)?;
        eprintln!("DEBUG SETUPVAL: Value from R({}) (absolute {}): {:?}", b, base + b, value);
        
        // Get current frame and closure
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        let closure_ref = frame.closure.borrow();
        
        eprintln!("DEBUG SETUPVAL: Closure has {} upvalues", closure_ref.upvalues.len());
        
        // Check upvalue index
        if a >= closure_ref.upvalues.len() {
            eprintln!("DEBUG SETUPVAL: Upvalue index {} out of bounds", a);
            drop(closure_ref);
            return Err(LuaError::RuntimeError(format!(
                "Upvalue index {} out of bounds", a
            )));
        }
        
        // Get upvalue
        let upvalue = Rc::clone(&closure_ref.upvalues[a]);
        drop(closure_ref);
        
        // Debug dump upvalue state before setting
        {
            let uv_ref = upvalue.borrow();
            match &*uv_ref {
                UpvalueState::Open { stack_index, .. } => {
                    eprintln!("DEBUG SETUPVAL: Upvalue is OPEN, points to stack index {}", stack_index);
                },
                UpvalueState::Closed { value } => {
                    eprintln!("DEBUG SETUPVAL: Upvalue is CLOSED with current value: {:?}", value);
                }
            }
        }
        
        // Set upvalue value
        eprintln!("DEBUG SETUPVAL: Setting upvalue to value: {:?}", value);
        self.heap.set_upvalue_value(&upvalue, value.clone())?;
        
        // Verify it was set correctly
        {
            let value_after = self.heap.get_upvalue_value(&upvalue);
            eprintln!("DEBUG SETUPVAL: Verified upvalue now contains: {:?}", value_after);
        }
        
        Ok(())
    }
    
    /// CLOSE: close all upvalues for locals >= R(A)
    fn op_close(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        
        // Close upvalues
        self.heap.close_upvalues(&self.current_thread, base + a)?;
        
        Ok(())
    }
    
    /// SETLIST: R(A)[(C-1)*FPF+i] := R(A+i), 1 <= i <= B
    fn op_setlist(&self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        let c = inst.get_c() as usize;
        
        const FIELDS_PER_FLUSH: usize = 50;
        
        // Get table
        let table_val = self.get_register(base + a)?;
        
        let table_handle = match table_val {
            Value::Table(handle) => handle,
            _ => {
                return Err(LuaError::TypeError {
                    expected: "table".to_string(),
                    got: table_val.type_name().to_string(),
                });
            }
        };
        
        // Calculate base array index
        let array_base = (c - 1) * FIELDS_PER_FLUSH;
        
        // Determine number of elements to set
        let count = if b == 0 {
            // Use all values up to stack top
            self.heap.get_stack_size(&self.current_thread) - (base + a + 1)
        } else {
            b
        };
        
        // Set elements
        for i in 0..count {
            let value = self.get_register(base + a + 1 + i)?;
            let index = array_base + i + 1; // +1 for 1-based indexing
            
            // Create key
            let key = Value::Number(index as f64);
            
            // Set field
            self.heap.set_table_field_raw(&table_handle, &key, &value)?;
        }
        
        Ok(())
    }
    
    /// TAILCALL: return R(A)(R(A+1), ..., R(A+B-1))
    fn op_tailcall(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        // TAILCALL is like CALL followed by RETURN
        
        let a = inst.get_a() as usize;
        let b = inst.get_b();
        
        // Get function
        let func = self.get_register(base + a)?;
        
        // Determine argument count
        let arg_count = if b == 0 {
            // All values above the function
            self.heap.get_stack_size(&self.current_thread) - (base + a + 1)
        } else {
            (b - 1) as usize
        };
        
        // Collect arguments
        let mut args = Vec::with_capacity(arg_count);
        for i in 0..arg_count {
            args.push(self.get_register(base + a + 1 + i)?);
        }
        
        // Close all upvalues
        self.heap.close_upvalues(&self.current_thread, base)?;
        
        // Pop current frame to get expected return count
        let frame = self.heap.pop_call_frame(&self.current_thread)?;
        let expected_results = frame.expected_results.map_or(-1, |n| n as i32);
        
        // Get base of calling function
        let result_base = if let Ok(parent_frame) = self.heap.get_current_frame(&self.current_thread) {
            parent_frame.base_register as usize - 1 // -1 to get the function position
        } else {
            0 // Main chunk
        };
        
        // Queue function call to caller's result position
        self.operation_queue.push_back(PendingOperation::FunctionCall {
            func,
            args,
            expected_results,
            result_base,
        });
        
        Ok(())
    }
    
    /// VARARG: R(A), ..., R(A+B-2) := vararg
    fn op_vararg(&self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b();
        
        // Get current frame
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        
        // Check if varargs are available
        let varargs = match &frame.varargs {
            Some(varargs) => varargs,
            None => return Ok(()), // No varargs, do nothing
        };
        
        // Determine number of values to copy
        let count = if b == 0 {
            // All varargs
            varargs.len()
        } else {
            // Specific number
            (b - 1) as usize
        };
        
        // Copy varargs to registers
        for i in 0..count {
            if i < varargs.len() {
                self.set_register(base + a + i, varargs[i].clone())?;
            } else {
                self.set_register(base + a + i, Value::Nil)?;
            }
        }
        
        Ok(())
    }
    
    /// EQ: if ((RK(B) == RK(C)) ~= A) then pc++
    fn op_eq(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        // This is a comparison instruction
        self.op_comparison(inst, base, CompOp::Eq)
    }
    
    /// LT: if ((RK(B) < RK(C)) ~= A) then pc++
    fn op_lt(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        self.op_comparison(inst, base, CompOp::Lt)
    }
    
    /// LE: if ((RK(B) <= RK(C)) ~= A) then pc++
    fn op_le(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        self.op_comparison(inst, base, CompOp::Le)
    }
    
    /// Generic comparison operator
    fn op_comparison(&mut self, inst: Instruction, base: usize, op: CompOp) -> LuaResult<()> {
        let a = inst.get_a();
        let (b_is_const, b_idx) = inst.get_rk_b();
        let (c_is_const, c_idx) = inst.get_rk_c();
        
        // Get operands
        let left = if b_is_const {
            let frame = self.heap.get_current_frame(&self.current_thread)?;
            self.get_constant(&frame.closure, b_idx as usize)?
        } else {
            self.get_register(base + b_idx as usize)?
        };
        
        let right = if c_is_const {
            let frame = self.heap.get_current_frame(&self.current_thread)?;
            self.get_constant(&frame.closure, c_idx as usize)?
        } else {
            self.get_register(base + c_idx as usize)?
        };
        
        // Try direct comparison
        let result = match (op, &left, &right) {
            (CompOp::Eq, _, _) => left == right,
            (CompOp::Lt, &Value::Number(l), &Value::Number(r)) => l < r,
            (CompOp::Lt, &Value::String(ref l), &Value::String(ref r)) => {
                let l_ref = l.borrow();
                let r_ref = r.borrow();
                l_ref.bytes < r_ref.bytes
            },
            (CompOp::Le, &Value::Number(l), &Value::Number(r)) => l <= r,
            (CompOp::Le, &Value::String(ref l), &Value::String(ref r)) => {
                let l_ref = l.borrow();
                let r_ref = r.borrow();
                l_ref.bytes <= r_ref.bytes
            },
            _ => {
                // Metamethod lookup would go here
                return Err(LuaError::TypeError {
                    expected: "comparable values".to_string(),
                    got: format!("{} and {}", left.type_name(), right.type_name()),
                });
            }
        };
        
        // Skip next instruction if (result ~= A)
        let skip = result != (a != 0);
        
        if skip {
            let pc = self.heap.get_pc(&self.current_thread)?;
            self.heap.set_pc(&self.current_thread, pc + 1)?;
        }
        
        Ok(())
    }
    
    /// TEST: if not (R(A) <=> C) then pc++
    fn op_test(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let c = inst.get_c();
        
        // Get value
        let value = self.get_register(base + a)?;
        
        // Determine truthiness
        let is_truthy = !value.is_falsey();
        
        // Skip if condition not met
        let skip = is_truthy != (c != 0);
        
        if skip {
            let pc = self.heap.get_pc(&self.current_thread)?;
            self.heap.set_pc(&self.current_thread, pc + 1)?;
        }
        
        Ok(())
    }
    
    /// TESTSET: if (R(B) <=> C) then R(A) := R(B) else pc++
    fn op_testset(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        let c = inst.get_c();
        
        // Get test value
        let value = self.get_register(base + b)?;
        
        // Determine truthiness
        let is_truthy = !value.is_falsey();
        
        // Check condition
        if is_truthy == (c != 0) {
            // Condition met, assign
            self.set_register(base + a, value)?;
        } else {
            // Condition not met, skip
            let pc = self.heap.get_pc(&self.current_thread)?;
            self.heap.set_pc(&self.current_thread, pc + 1)?;
        }
        
        Ok(())
    }
    
    /// Find a metamethod for a value
    fn find_metamethod(&self, value: &Value, method_name: &Value) -> LuaResult<Option<Value>> {
        match value {
            Value::Table(ref handle) => {
                let table_ref = handle.borrow();
                
                // Check if table has a metatable
                let metatable_opt = table_ref.metatable.clone();
                
                // Drop the table borrow before accessing metatable
                drop(table_ref);
                
                if let Some(metatable) = metatable_opt {
                    let mt_ref = metatable.borrow();
                    if let Some(method) = mt_ref.get_field(method_name) {
                        return Ok(Some(method));
                    }
                }
            },
            // Other types with metatables would be handled here
            _ => {}
        }
        
        Ok(None)
    }
    
    // More opcode implementations would follow...
}

/// Helper enums

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

/// Implementation of ExecutionContext for C functions
impl<'a> ExecutionContext for VmExecutionContext<'a> {
    fn arg_count(&self) -> usize {
        self.nargs
    }
    
    fn get_arg(&self, index: usize) -> LuaResult<Value> {
        if index >= self.nargs {
            return Err(LuaError::RuntimeError(format!(
                "Argument {} out of range", index
            )));
        }
        
        self.vm.get_register(self.base + 1 + index)
    }
    
    fn push_result(&mut self, value: Value) -> LuaResult<()> {
        self.vm.set_register(self.base + self.results_pushed, value)?;
        self.results_pushed += 1;
        Ok(())
    }
    
    fn set_return(&mut self, index: usize, value: Value) -> LuaResult<()> {
        self.vm.set_register(self.base + index, value)?;
        
        if index >= self.results_pushed {
            self.results_pushed = index + 1;
        }
        
        Ok(())
    }
    
    fn create_string(&self, s: &str) -> LuaResult<StringHandle> {
        self.vm.heap.create_string(s)
    }
    
    fn create_table(&self) -> LuaResult<TableHandle> {
        Ok(self.vm.heap.create_table())
    }
    
    fn get_table_field(&self, table: &TableHandle, key: &Value) -> LuaResult<Value> {
        self.vm.heap.get_table_field(table, key)
    }
    
    fn set_table_field(&self, table: &TableHandle, key: Value, value: Value) -> LuaResult<()> {
        self.vm.heap.set_table_field(table, &key, &value)
    }
    
    fn get_arg_str(&self, index: usize) -> LuaResult<String> {
        let value = self.get_arg(index)?;
        match value {
            Value::String(handle) => {
                let string_ref = handle.borrow();
                match string_ref.to_str() {
                    Ok(s) => Ok(s.to_string()),
                    Err(_) => Err(LuaError::TypeError {
                        expected: "UTF-8 string".to_string(),
                        got: "binary data".to_string(),
                    }),
                }
            },
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
    
    fn table_next(&self, table: &TableHandle, key: &Value) -> LuaResult<Option<(Value, Value)>> {
        // Table traversal logic would go here
        Ok(None) // Placeholder
    }
    
    fn globals_handle(&self) -> LuaResult<TableHandle> {
        Ok(self.vm.heap.globals())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_rc_vm_creation() -> LuaResult<()> {
        let vm = RcVM::new()?;
        assert!(vm.operation_queue.is_empty());
        Ok(())
    }
    
    // Add more tests as needed
}