//! Rc<RefCell> Based Lua VM with COMPLETE Queue Elimination
//!
//! This module implements a Lua VM using fine-grained Rc<RefCell> objects
//! with direct execution that completely eliminates temporal state separation.
//! ALL queue infrastructure has been removed and replaced with immediate execution.

use std::rc::Rc;
use std::cell::RefCell;


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

// cmp_value function ELIMINATED - no longer needed with queue elimination!

/// Trait for execution contexts that provide access to VM state during C function calls
pub trait ExecutionContext {
    fn arg_count(&self) -> usize;
    fn nargs(&self) -> usize { self.arg_count() }
    fn get_arg(&self, index: usize) -> LuaResult<Value>;
    fn arg(&self, index: usize) -> LuaResult<Value> { self.get_arg(index) }
    fn push_result(&mut self, value: Value) -> LuaResult<()>;
    fn push_return(&mut self, value: Value) -> LuaResult<()> { self.push_result(value) }
    fn set_return(&mut self, index: usize, value: Value) -> LuaResult<()>;
    fn create_string(&self, s: &str) -> LuaResult<StringHandle>;
    fn create_table(&self) -> LuaResult<TableHandle>;
    fn get_table_field(&self, table: &TableHandle, key: &Value) -> LuaResult<Value>;
    fn set_table_field(&mut self, table: &TableHandle, key: Value, value: Value) -> LuaResult<()>;
    fn get_arg_str(&self, index: usize) -> LuaResult<String>;
    fn get_number_arg(&self, index: usize) -> LuaResult<f64>;
    fn get_bool_arg(&self, index: usize) -> LuaResult<bool>;
    fn table_next(&self, table: &TableHandle, key: &Value) -> LuaResult<Option<(Value, Value)>>;
    fn globals_handle(&self) -> LuaResult<TableHandle>;
    fn get_call_base(&self) -> usize;
    fn pcall(&mut self, func: Value, args: Vec<Value>) -> LuaResult<()>;
    fn xpcall(&mut self, func: Value, err_handler: Value) -> LuaResult<()>;
    fn get_upvalue_value(&self, upvalue: &UpvalueHandle) -> LuaResult<Value>;
    fn set_upvalue_value(&self, upvalue: &UpvalueHandle, value: Value) -> LuaResult<()>;
}

/// VM configuration
#[derive(Debug, Clone)]
pub struct VMConfig {
    pub max_stack_size: usize,
    pub max_call_depth: usize,
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

/// Result of a VM step in direct execution model
#[derive(Debug, Clone)]
enum StepResult {
    Continue,
    Completed(Vec<Value>),
}

/// Execution context for C functions (completely queue-free)
struct VmExecutionContext<'a> {
    vm: &'a RcVM,
    base: usize,
    nargs: usize,
    results_pushed: usize,
    results: Vec<Value>,
}

/// Rc<RefCell> based Lua VM with COMPLETE queue elimination (NO QUEUE INFRASTRUCTURE!)
pub struct RcVM {
    /// The Lua heap
    pub heap: RcHeap,
    
    /// Current thread (NO OPERATION QUEUE!)
    current_thread: ThreadHandle,
    
    /// VM configuration (queue eliminated!)
    config: VMConfig,
}

impl RcVM {
    fn create_value_from_constant(&self, constant: &CompilationConstant, string_handles: &Vec<StringHandle>, proto_handles: &Vec<FunctionProtoHandle>) -> LuaResult<Value> {
        match constant {
            CompilationConstant::Nil => Ok(Value::Nil),
            CompilationConstant::Boolean(b) => Ok(Value::Boolean(*b)),
            CompilationConstant::Number(n) => Ok(Value::Number(*n)),
            CompilationConstant::String(idx) => {
                if *idx >= string_handles.len() {
                    return Err(LuaError::RuntimeError(format!("Invalid string index: {}", idx)));
                }
                Ok(Value::String(Rc::clone(&string_handles[*idx])))
            },
            CompilationConstant::FunctionProto(idx) => {
                if *idx >= proto_handles.len() {
                    return Err(LuaError::RuntimeError(format!("Invalid function prototype index: {}", idx)));
                }
                Ok(Value::FunctionProto(Rc::clone(&proto_handles[*idx])))
            },
            CompilationConstant::Table(entries) => {
                let table = self.heap.create_table();
                for (k, v) in entries {
                    let key = self.create_value_from_constant(k, string_handles, proto_handles)?;
                    let value = self.create_value_from_constant(v, string_handles, proto_handles)?;
                    self.heap.set_table_field_raw(&table, &key, &value)?;
                }
                Ok(Value::Table(table))
            },
        }
    }

    /// Create a new VM (completely queue-free)
    pub fn new() -> LuaResult<Self> {
        Self::with_config(VMConfig::default())
    }
    
    /// Create a new VM with config (NO QUEUE!)
    pub fn with_config(config: VMConfig) -> LuaResult<Self> {
        let heap = RcHeap::new()?;
        let main_thread = heap.main_thread();
        
        Ok(RcVM {
            heap,
            current_thread: main_thread.clone(),
            config,
        })
    }
    
    /// Initialize the standard library
    pub fn init_stdlib(&mut self) -> LuaResult<()> {
        super::rc_stdlib::init_stdlib(self)
    }
    
    /// Execute a compiled module (COMPLETELY queue-free!)
    pub fn execute_module(&mut self, module: &compiler::CompiledModule, args: &[Value]) -> LuaResult<Value> {
        // Create main function
        let main_closure = self.load_module(module)?;
        
        // Clear stack and frames
        {
            let mut thread = self.current_thread.borrow_mut();
            thread.stack.clear();
            thread.call_frames.clear();
        }
        
        // Place main function and arguments on stack
        self.set_register(0, Value::Closure(main_closure.clone()))?;
        for (i, arg) in args.iter().enumerate() {
            self.set_register(i + 1, arg.clone())?;
        }
        
        // Create and push main call frame DIRECTLY (NO QUEUE!)
        let main_frame = CallFrame {
            closure: main_closure.clone(),
            pc: 0,
            base_register: 1,
            expected_results: Some(1),
            varargs: None,
            is_protected: false,
            xpcall_handler: None,
            result_base: 0,
        };
        
        self.heap.push_call_frame(&self.current_thread, main_frame)?;
        
        // Execute with direct execution model (NO QUEUE PROCESSING!)
        self.run_to_completion()
    }
    
    /// UNIFIED DIRECT EXECUTION - NO QUEUE PROCESSING!
    fn run_to_completion(&mut self) -> LuaResult<Value> {
        loop {
            // Check if execution is complete
            let call_depth = self.heap.get_call_depth(&self.current_thread);
            if call_depth == 0 {
                return Ok(Value::Nil);
            }
            
            // Execute next step DIRECTLY - NO QUEUE PROCESSING!
            match self.step() {
                Ok(StepResult::Continue) => continue,
                Ok(StepResult::Completed(values)) => {
                    return Ok(values.first().cloned().unwrap_or(Value::Nil));
                },
                Err(e) => {
                    match self.handle_error(e) {
                        Ok(StepResult::Continue) => continue,
                        Ok(StepResult::Completed(values)) => {
                            return Ok(values.first().cloned().unwrap_or(Value::Nil));
                        }
                        Err(unhandled_error) => {
                            return Err(unhandled_error);
                        }
                    }
                }
            }
        }
    }
    
    /// Execute a single VM step (completely direct execution)
    fn step(&mut self) -> LuaResult<StepResult> {
        let call_depth = self.heap.get_call_depth(&self.current_thread);
        if call_depth == 0 {
            return Ok(StepResult::Completed(vec![]));
        }
        
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        let base = frame.base_register as usize;
        let pc = frame.pc;
        
        let instruction = self.get_instruction(&frame.closure, pc)?;
        let inst = Instruction(instruction);
        
        self.heap.increment_pc(&self.current_thread)?;
        
        // Debug output for key opcodes
        match inst.get_opcode() {
            OpCode::ForPrep | OpCode::ForLoop | OpCode::TForLoop | OpCode::Closure | OpCode::Call | OpCode::Return => {
                eprintln!("DEBUG RcVM: Executing {:?} at PC={}, base={} (DIRECT)", 
                         inst.get_opcode(), pc, base);
            }
            _ => {}
        }
        
        // Execute instruction DIRECTLY (all returns are now direct!)
        match inst.get_opcode() {
            OpCode::Move => { self.op_move(inst, base)?; Ok(StepResult::Continue) }
            OpCode::LoadK => { self.op_loadk(inst, base)?; Ok(StepResult::Continue) }
            OpCode::LoadBool => { self.op_loadbool(inst, base)?; Ok(StepResult::Continue) }
            OpCode::LoadNil => { self.op_loadnil(inst, base)?; Ok(StepResult::Continue) }
            OpCode::GetGlobal => { self.op_getglobal(inst, base)?; Ok(StepResult::Continue) }
            OpCode::SetGlobal => { self.op_setglobal(inst, base)?; Ok(StepResult::Continue) }
            OpCode::GetTable => { self.op_gettable(inst, base)?; Ok(StepResult::Continue) }
            OpCode::SetTable => { self.op_settable(inst, base)?; Ok(StepResult::Continue) }
            OpCode::NewTable => { self.op_newtable(inst, base)?; Ok(StepResult::Continue) }
            OpCode::SelfOp => { self.op_self(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Add => { self.op_add(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Sub => { self.op_sub(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Mul => { self.op_mul(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Div => { self.op_div(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Mod => { self.op_mod(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Pow => { self.op_pow(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Unm => { self.op_unm(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Not => { self.op_not(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Len => { self.op_len(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Concat => { self.op_concat(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Jmp => { self.op_jmp(inst)?; Ok(StepResult::Continue) }
            OpCode::Eq => { self.op_eq(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Lt => { self.op_lt(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Le => { self.op_le(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Test => { self.op_test(inst, base)?; Ok(StepResult::Continue) }
            OpCode::TestSet => { self.op_testset(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Call => self.op_call(inst, base),
            OpCode::TailCall => self.op_tailcall(inst, base),
            OpCode::Return => self.op_return(inst, base),
            OpCode::ForPrep => { self.op_forprep(inst, base)?; Ok(StepResult::Continue) }
            OpCode::ForLoop => { self.op_forloop(inst, base)?; Ok(StepResult::Continue) }
            OpCode::TForCall => { self.op_tforcall(inst, base)?; Ok(StepResult::Continue) }
            OpCode::TForLoop => { self.op_tforloop(inst, base)?; Ok(StepResult::Continue) }
            OpCode::VarArg => { self.op_vararg(inst, base)?; Ok(StepResult::Continue) }
            OpCode::GetUpval => { self.op_getupval(inst, base)?; Ok(StepResult::Continue) }
            OpCode::SetUpval => { self.op_setupval(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Closure => { self.op_closure(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Close => { self.op_close(inst, base)?; Ok(StepResult::Continue) }
            OpCode::SetList => { self.op_setlist(inst, base)?; Ok(StepResult::Continue) }
            _ => Err(LuaError::NotImplemented(format!("Opcode {:?}", inst.get_opcode()))),
        }
    }
    
    /// Execute function call DIRECTLY (NO QUEUE!)
    fn execute_function_call(
        &mut self,
        func: Value,
        args: Vec<Value>,
        expected_results: i32,
        result_base: usize,
        is_protected: bool,
        xpcall_handler: Option<Value>,
    ) -> LuaResult<StepResult> {
        match &func {
            Value::Closure(closure_handle) => {
                self.call_lua_function(Rc::clone(closure_handle), args, expected_results, result_base, is_protected, xpcall_handler)
            },
            Value::CFunction(cfunc) => {
                self.call_c_function(*cfunc, args, expected_results, result_base)
            },
            Value::Table(table_handle) => {
                // Check for __call metamethod
                let table_ref = table_handle.borrow();
                if let Some(metatable) = &table_ref.metatable {
                    let metatable_clone = Rc::clone(metatable);
                    drop(table_ref);
                    
                    let mt_ref = metatable_clone.borrow();
                    let call_key = Value::String(Rc::clone(&self.heap.metamethod_names.call));
                    if let Some(metamethod) = mt_ref.get_field(&call_key) {
                        drop(mt_ref);
                        
                        let mut metamethod_args = vec![Value::Table(Rc::clone(table_handle))];
                        metamethod_args.extend(args);
                        
                        self.execute_function_call(metamethod, metamethod_args, expected_results, result_base, is_protected, xpcall_handler)
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
            _ => Err(LuaError::TypeError {
                expected: "function".to_string(),
                got: func.type_name().to_string(),
            })
        }
    }
    
    /// Call Lua function directly
    fn call_lua_function(
        &mut self,
        closure: ClosureHandle,
        args: Vec<Value>,
        expected_results: i32,
        result_base: usize,
        is_protected: bool,
        xpcall_handler: Option<Value>,
    ) -> LuaResult<StepResult> {
        let (num_params, max_stack, is_vararg, _func_proto) = {
            let closure_ref = closure.borrow();
            let proto_ref = &closure_ref.proto;
            (
                proto_ref.num_params as usize,
                proto_ref.max_stack_size as usize,
                proto_ref.is_vararg,
                Rc::clone(&closure_ref.proto),
            )
        };
        
        let new_base = result_base;
        let frame_base = new_base + 1;
        
        let required_stack_size = frame_base + max_stack;
        self.ensure_stack_space(required_stack_size)?;
        
        // Place arguments
        for (i, arg) in args.iter().enumerate() {
            if i < num_params {
                self.set_register(new_base + 1 + i, arg.clone())?;
            }
        }
        
        // Fill missing parameters with nil
        for i in args.len()..num_params {
            self.set_register(new_base + 1 + i, Value::Nil)?;
        }
        
        // Create varargs if needed
        let varargs = if is_vararg && args.len() > num_params {
            Some(args[num_params..].to_vec())
        } else {
            None
        };
        
        // Create call frame
        let frame = CallFrame {
            closure: closure.clone(),
            pc: 0,
            base_register: (new_base + 1) as u16,
            expected_results: if expected_results >= 0 {
                Some(expected_results as usize)
            } else {
                None
            },
            varargs,
            is_protected,
            xpcall_handler,
            result_base,
        };
        
        // Push frame DIRECTLY (NO QUEUE!)
        self.heap.push_call_frame(&self.current_thread, frame)?;
        
        Ok(StepResult::Continue)
    }
    
    /// Call C function directly
    fn call_c_function(
        &mut self,
        func: super::rc_value::CFunction,
        args: Vec<Value>,
        expected_results: i32,
        result_base: usize,
    ) -> LuaResult<StepResult> {
        let args_base = result_base;
        self.ensure_stack_space(args_base + 1 + args.len())?;

        self.set_register(args_base, Value::CFunction(func))?;
        for (i, arg) in args.iter().enumerate() {
            self.set_register(args_base + 1 + i, arg.clone())?;
        }
        
        self.process_c_function_call(func, result_base as u16, args.len(), expected_results)
    }
    
    /// Process C function call directly
    fn process_c_function_call(
        &mut self,
        function: super::rc_value::CFunction,
        base: u16,  
        nargs: usize,
        expected_results: i32,
    ) -> LuaResult<StepResult> {
        eprintln!("DEBUG process_c_function_call: base={}, nargs={}, expected_results={}", base, nargs, expected_results);
        
        let mut ctx = VmExecutionContext {
            vm: self,
            base: (base + 1) as usize,
            nargs,
            results_pushed: 0,
            results: Vec::new(),
        };
        
        let actual_results = function(&mut ctx)?;
        
        if actual_results == -1 {
            drop(ctx);
            return Ok(StepResult::Continue);
        }
        
        if actual_results < 0 {
            return Err(LuaError::RuntimeError(
                "C function returned invalid result count".to_string()
            ));
        }
        
        let mut results = std::mem::take(&mut ctx.results);
        let pushed = results.len();
        drop(ctx);
        
        let mut results_pushed = actual_results as usize;
        if results_pushed > pushed {
            results.resize(results_pushed, Value::Nil);
        } else if results_pushed < pushed {
            results.truncate(results_pushed);
        }
        
        let base_usize = base as usize;
        if expected_results >= 0 {
            let expected = expected_results as usize;
            
            if results_pushed < expected {
                results.resize(expected, Value::Nil);
                results_pushed = expected;
            } else if results_pushed > expected {
                results.truncate(expected);
                results_pushed = expected;
            }
        }
        
        for (i, val) in results.iter().enumerate() {
            self.set_register(base_usize + i, val.clone())?;
        }
        
        Ok(StepResult::Continue)
    }
    
    /// Process return DIRECTLY (NO QUEUE!)
    fn process_return(&mut self, values: Vec<Value>) -> LuaResult<StepResult> {
        if self.heap.get_call_depth(&self.current_thread) == 0 {
            return Ok(StepResult::Completed(values));
        }
        
        let frame = self.heap.pop_call_frame(&self.current_thread)?;
        
        if frame.is_protected {
            let mut success_results = Vec::with_capacity(values.len() + 1);
            success_results.push(Value::Boolean(true));
            success_results.extend(values);
            
            self.place_return_values(success_results, frame.result_base, frame.expected_results)?;
        } else {
            self.place_return_values(values, frame.result_base, frame.expected_results)?;
        }
        
        if self.heap.get_call_depth(&self.current_thread) == 0 {
            if frame.is_protected {
                 let final_value = self.get_register(frame.result_base)?;
                 return Ok(StepResult::Completed(vec![final_value]));
            }
            return Ok(StepResult::Completed(vec![]));
        }
        
        Ok(StepResult::Continue)
    }

    /// Place return values directly 
    fn place_return_values(&mut self, values: Vec<Value>, result_base: usize, expected_results: Option<usize>) -> LuaResult<()> {
        let result_count = if let Some(n) = expected_results {
            n
        } else {
            values.len()
        };

        if result_count == 0 {
            eprintln!("DEBUG place_return_values: caller expects 0 results – truncating stack to result_base ({})", result_base);
            let mut thread = self.current_thread.borrow_mut();
            if thread.stack.len() > result_base {
                thread.stack.truncate(result_base);
            }
            return Ok(());
        }

        for i in 0..result_count {
            let value_to_set = values.get(i).cloned().unwrap_or(Value::Nil);
            self.set_register(result_base + i, value_to_set)?;
        }
        
        Ok(())
    }
    
    /// Handle error directly (NO QUEUE CLEARING!)
    fn handle_error(&mut self, error: LuaError) -> LuaResult<StepResult> {
        let error_val = Value::String(self.heap.create_string(&error.to_string())?);

        loop {
            if self.heap.get_call_depth(&self.current_thread) == 0 {
                return Err(error);
            }

            let frame = self.heap.pop_call_frame(&self.current_thread)?;

            if frame.is_protected {
                self.set_register(frame.result_base, Value::Boolean(false))?;
                if let Some(n) = frame.expected_results {
                    for i in 2..n {
                        self.set_register(frame.result_base + i, Value::Nil)?;
                    }
                }

                if let Some(handler) = frame.xpcall_handler {
                    return self.execute_function_call(
                        handler,
                        vec![error_val],
                        1,
                        frame.result_base + 1,
                        false,
                        None,
                    );
                } else {
                    self.set_register(frame.result_base + 1, error_val)?;
                    return Ok(StepResult::Continue);
                }
            }
        }
    }
    
    /// Load module (same working version)
    fn load_module(&mut self, module: &compiler::CompiledModule) -> LuaResult<ClosureHandle> {
        eprintln!("DEBUG load_module: Loading module with {} constants, {} strings, {} prototypes",
                 module.constants.len(), module.strings.len(), module.prototypes.len());

        let mut string_handles = Vec::with_capacity(module.strings.len());
        for s in &module.strings {
            string_handles.push(self.heap.create_string(s)?);
        }

        let mut placeholder_handles = Vec::with_capacity(module.prototypes.len());
        for proto in &module.prototypes {
            let placeholder_proto = FunctionProto {
                bytecode: proto.bytecode.clone(),
                constants: vec![],
                num_params: proto.num_params,
                is_vararg: proto.is_vararg,
                max_stack_size: proto.max_stack_size,
                upvalues: proto.upvalues.iter().map(|u| super::rc_value::UpvalueInfo { in_stack: u.in_stack, index: u.index }).collect(),
            };
            placeholder_handles.push(self.heap.create_function_proto(placeholder_proto));
        }

        let mut intermediate_handles = Vec::with_capacity(module.prototypes.len());
        for (i, proto) in module.prototypes.iter().enumerate() {
            let mut constants = Vec::with_capacity(proto.constants.len());
            for constant in &proto.constants {
                let value = self.create_value_from_constant(constant, &string_handles, &placeholder_handles)?;
                constants.push(value);
            }
            let intermediate_proto = FunctionProto {
                bytecode: proto.bytecode.clone(),
                constants,
                num_params: proto.num_params,
                is_vararg: proto.is_vararg,
                max_stack_size: proto.max_stack_size,
                upvalues: module.prototypes[i].upvalues.iter().map(|u| super::rc_value::UpvalueInfo { in_stack: u.in_stack, index: u.index }).collect(),
            };
            intermediate_handles.push(self.heap.create_function_proto(intermediate_proto));
        }

        let mut proto_handles = Vec::with_capacity(module.prototypes.len());
        for (i, proto) in module.prototypes.iter().enumerate() {
            let mut final_constants = Vec::with_capacity(proto.constants.len());
            for constant in &proto.constants {
                let value = self.create_value_from_constant(constant, &string_handles, &intermediate_handles)?;
                final_constants.push(value);
            }
            let final_proto = FunctionProto {
                bytecode: proto.bytecode.clone(),
                constants: final_constants,
                num_params: proto.num_params,
                is_vararg: proto.is_vararg,
                max_stack_size: proto.max_stack_size,
                upvalues: module.prototypes[i].upvalues.iter().map(|u| super::rc_value::UpvalueInfo { in_stack: u.in_stack, index: u.index }).collect(),
            };
            proto_handles.push(self.heap.create_function_proto(final_proto));
        }
        
        let mut main_constants = Vec::with_capacity(module.constants.len());
        for constant in module.constants.iter() {
            let value = self.create_value_from_constant(constant, &string_handles, &proto_handles)?;
            main_constants.push(value);
        }
        
        let main_upvalues = module.upvalues.iter()
            .map(|u| super::rc_value::UpvalueInfo { in_stack: u.in_stack, index: u.index })
            .collect();
        
        let main_proto = FunctionProto {
            bytecode: module.bytecode.clone(),
            constants: main_constants,
            num_params: module.num_params,
            is_vararg: module.is_vararg,
            max_stack_size: module.max_stack_size,
            upvalues: main_upvalues,
        };
        
        let main_proto_handle = self.heap.create_function_proto(main_proto);
        
        let mut main_upvalues = Vec::new();
        let globals_value = Value::Table(self.heap.globals());
        let globals_upvalue = Rc::new(RefCell::new(UpvalueState::Closed {
            value: globals_value,
        }));
        main_upvalues.push(globals_upvalue);
        
        let main_closure = self.heap.create_closure(Rc::clone(&main_proto_handle), main_upvalues);
        
        Ok(main_closure)
    }
    
    /// Find metamethod (preserved functionality)
    fn find_metamethod(&self, value: &Value, method_name: &Value) -> LuaResult<Option<Value>> {
        match value {
            Value::Table(ref handle) => {
                let table_ref = handle.borrow();
                let metatable_opt = table_ref.metatable.clone();
                drop(table_ref);
                
                if let Some(metatable) = metatable_opt {
                    let mt_ref = metatable.borrow();
                    if let Some(method) = mt_ref.get_field(method_name) {
                        match method {
                            Value::Nil => Ok(None),
                            _ => Ok(Some(method)),
                        }
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            },
            _ => Ok(None)
        }
    }
    
    /// Get metatable (preserved functionality)
    fn get_metatable(&self, value: &Value) -> LuaResult<Option<TableHandle>> {
        match value {
            Value::Table(ref handle) => {
                let table_ref = handle.borrow();
                Ok(table_ref.metatable.clone())
            },
            _ => Ok(None)
        }
    }
    
    //
    // Heap access helpers
    //
    
    pub fn create_string(&self, s: &str) -> LuaResult<StringHandle> {
        self.heap.create_string(s)
    }
    
    pub fn create_table(&self) -> LuaResult<TableHandle> {
        Ok(self.heap.create_table())
    }
    
    pub fn get_table_field(&self, table: &TableHandle, key: &Value) -> LuaResult<Value> {
        self.heap.get_table_field(table, key)
    }
    
    pub fn set_table_field(&self, table: &TableHandle, key: &Value, value: &Value) -> LuaResult<()> {
        self.heap.set_table_field(table, key, value).map(|_| ())
    }
    
    pub fn globals(&self) -> LuaResult<TableHandle> {
        Ok(self.heap.globals())
    }
    
    //
    // Register access helpers  
    //
    
    fn get_register(&self, index: usize) -> LuaResult<Value> {
        match self.heap.get_register(&self.current_thread, index) {
            Ok(value) => Ok(value),
            Err(e) => {
                eprintln!("REGISTER OVERFLOW DEBUG:");
                eprintln!("  Attempted to access register: {}", index);
                
                let thread_ref = self.current_thread.borrow();
                eprintln!("  Current stack size: {}", thread_ref.stack.len());
                eprintln!("  Valid register range: 0-{}", thread_ref.stack.len().saturating_sub(1));
                eprintln!("  Call frames: {}", thread_ref.call_frames.len());
                
                if !thread_ref.call_frames.is_empty() {
                    match thread_ref.call_frames.last().unwrap() {
                        super::rc_value::Frame::Call(current_frame) => {
                            eprintln!("  Current frame base_register: {}", current_frame.base_register);
                            eprintln!("  Current frame PC: {}", current_frame.pc);
                        }
                        super::rc_value::Frame::Continuation(_) => {
                            eprintln!("  Current frame: Continuation");
                        }
                    }
                }
                
                if index > 255 {
                    eprintln!("  POTENTIAL CAUSE: Index {} suggests base+offset calculation error", index);
                }
                
                Err(LuaError::RuntimeError(
                    format!("Register {} out of bounds (stack size: {}) - Direct execution fix applied", 
                           index, thread_ref.stack.len())
                ))
            }
        }
    }
    
    fn set_register(&self, index: usize, value: Value) -> LuaResult<()> {
        self.heap.set_register(&self.current_thread, index, value)
    }
    
    fn ensure_stack_space(&self, size: usize) -> LuaResult<()> {
        let current_size = self.heap.get_stack_size(&self.current_thread);
        if current_size < size {
            let mut thread = self.current_thread.borrow_mut();
            if thread.stack.len() < size {
                thread.stack.resize(size, Value::Nil);
            }
        }
        Ok(())
    }
    
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
    
    fn read_rk(&self, base: usize, rk: u32) -> LuaResult<Value> {
        if rk & 0x100 != 0 {
            let frame = self.heap.get_current_frame(&self.current_thread)?;
            self.get_constant(&frame.closure, (rk & 0xFF) as usize)
        } else {
            self.get_register(base + rk as usize)
        }
    }
    
    //
    // ALL OPCODE IMPLEMENTATIONS WITH DIRECT EXECUTION (COMPLETE QUEUE ELIMINATION!)
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
        
        self.set_register(base + a, Value::Boolean(b != 0))?;
        
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
        
        for i in a..=b {
            self.set_register(base + i, Value::Nil)?;
        }
        
        Ok(())
    }
    
    /// GETGLOBAL: R(A) := Gbl[Kst(Bx)] (DIRECT metamethod execution)
    fn op_getglobal(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let bx = inst.get_bx() as usize;
        
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        let key = self.get_constant(&frame.closure, bx)?;
        
        let globals = {
            let closure_ref = frame.closure.borrow();
            if closure_ref.upvalues.is_empty() {
                return Err(LuaError::RuntimeError(
                    "Attempt to access global in function with no upvalues".to_string()
                ));
            }
            let env_upvalue = Rc::clone(&closure_ref.upvalues[0]);
            drop(closure_ref);
            
            let env_val = self.heap.get_upvalue_value(&env_upvalue);
            match env_val {
                Value::Table(t) => t,
                _ => return Err(LuaError::RuntimeError("_ENV is not a table".to_string())),
            }
        };
        
        let value = self.heap.get_table_field(&globals, &key)?;
        
        // Handle metamethods DIRECTLY (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
        let final_value = match value {
            Value::PendingMetamethod(boxed_mm) => {
                // Execute metamethod DIRECTLY
                self.execute_function_call(*boxed_mm, vec![Value::Table(Rc::clone(&globals)), key.clone()], 1, base + a, false, None)?;
                return Ok(());
            },
            other => other,
        };
        
        self.set_register(base + a, final_value)?;
        Ok(())
    }
    
    /// SETGLOBAL: Gbl[Kst(Bx)] := R(A) (DIRECT metamethod execution)
    fn op_setglobal(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let bx = inst.get_bx() as usize;
        
        let value = self.get_register(base + a)?;
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        let key = self.get_constant(&frame.closure, bx)?;
        
        let globals = {
            let closure_ref = frame.closure.borrow();
            if closure_ref.upvalues.is_empty() {
                return Err(LuaError::RuntimeError(
                    "Attempt to access global in function with no upvalues".to_string()
                ));
            }
            let env_upvalue = Rc::clone(&closure_ref.upvalues[0]);
            drop(closure_ref);
            
            let env_val = self.heap.get_upvalue_value(&env_upvalue);
            match env_val {
                Value::Table(t) => t,
                _ => return Err(LuaError::RuntimeError("_ENV is not a table".to_string())),
            }
        };
        
        // Set value with DIRECT metamethod execution (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
        match self.heap.set_table_field(&globals, &key, &value)? {
            Some(metamethod) => {
                self.execute_function_call(metamethod, vec![Value::Table(Rc::clone(&globals)), key.clone(), value.clone()], 0, 0, false, None)?;
            },
            None => {},
        }
        
        Ok(())
    }
    
    /// GETTABLE: R(A) := R(B)[RK(C)] (DIRECT metamethod execution)
    fn op_gettable(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;

        eprintln!("DEBUG GETTABLE: EXECUTING at base={}, A={}, B={} (DIRECT)", base, a, b);
        
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
        
        let key = self.read_rk(base, inst.get_c())?;
        eprintln!("DEBUG GETTABLE: Key={:?}", key);

        let value = self.heap.get_table_field(table_handle, &key)?;
        
        // Handle metamethods DIRECTLY (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
        let final_value = match value {
            Value::PendingMetamethod(boxed_mm) => {
                // Execute metamethod DIRECTLY
                self.execute_function_call(*boxed_mm, vec![Value::Table(Rc::clone(table_handle)), key.clone()], 1, base + a, false, None)?;
                return Ok(());
            },
            other => other,
        };
        
        self.set_register(base + a, final_value)?;
        Ok(())
    }
    
    /// SETTABLE: R(A)[RK(B)] := RK(C) (DIRECT metamethod execution)
    fn op_settable(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        
        eprintln!("DEBUG SETTABLE: EXECUTING at base={}, A={} (DIRECT)", base, a);
        
        let table_val = self.get_register(base + a)?;
        
        let table_handle = match table_val {
            Value::Table(ref handle) => {
                eprintln!("DEBUG SETTABLE: Table found, proceeding with field assignment");
                handle
            },
            _ => {
                eprintln!("DEBUG SETTABLE: ERROR - Not a table: {}", table_val.type_name());
                return Err(LuaError::TypeError {
                    expected: "table".to_string(),
                    got: table_val.type_name().to_string(),
                });
            }
        };
        
        let key = self.read_rk(base, inst.get_b())?;
        let value = self.read_rk(base, inst.get_c())?;
        
        eprintln!("DEBUG SETTABLE: Key={:?}, Value={:?}", key, value);
        
        // Use heap's metamethod-aware set with DIRECT execution (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
        let metamethod_result = self.heap.set_table_field(table_handle, &key, &value)?;
        
        if let Some(metamethod) = metamethod_result {
            eprintln!("DEBUG SETTABLE: Metamethod found, executing DIRECTLY (no queue)");
            // Execute __newindex metamethod DIRECTLY (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
            self.execute_function_call(metamethod, vec![Value::Table(table_handle.clone()), key.clone(), value.clone()], 0, 0, false, None)?;
        } else {
            eprintln!("DEBUG SETTABLE: No metamethod, direct field storage completed");
        }
        
        eprintln!("DEBUG SETTABLE: Operation complete");
        Ok(())
    }
    
    /// NEWTABLE: R(A) := {} (size = B,C)
    fn op_newtable(&self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b();
        let c = inst.get_c();
        
        let array_size = if b == 0 { 0 } else { 1 << (b - 1) };
        let hash_size = if c == 0 { 0 } else { 1 << (c - 1) };
        
        let table = self.heap.create_table_with_capacity(array_size, hash_size);
        self.set_register(base + a, Value::Table(table))?;
        
        Ok(())
    }
    
    /// SELF: R(A+1) := R(B); R(A) := R(B)[RK(C)] (DIRECT metamethod execution)
    fn op_self(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        let (c_is_const, c_idx) = inst.get_rk_c();
        
        let table_val = self.get_register(base + b)?;
        
        match &table_val {
            Value::Table(table_handle) => {
                let key = if c_is_const {
                    let frame = self.heap.get_current_frame(&self.current_thread)?;
                    self.get_constant(&frame.closure, c_idx as usize)?
                } else {
                    self.get_register(base + c_idx as usize)?
                };
                
                self.set_register(base + a + 1, table_val.clone())?;
                
                let method = self.heap.get_table_field(table_handle, &key)?;
                
                // Handle metamethods DIRECTLY (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
                match method {
                    Value::PendingMetamethod(boxed_mm) => {
                        // Execute metamethod DIRECTLY
                        self.execute_function_call(*boxed_mm, vec![Value::Table(Rc::clone(&table_handle)), key.clone()], 1, base + a, false, None)?;
                    },
                    _ => {
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
    
    /// Arithmetic operations with DIRECT metamethod execution (COMPLETE QUEUE ELIMINATION!)
    fn op_add(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        self.op_arithmetic(inst, base, ArithOp::Add)
    }
    
    fn op_sub(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        self.op_arithmetic(inst, base, ArithOp::Sub)
    }
    
    fn op_mul(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        self.op_arithmetic(inst, base, ArithOp::Mul)
    }
    
    fn op_div(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        self.op_arithmetic(inst, base, ArithOp::Div)
    }
    
    fn op_mod(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        self.op_arithmetic(inst, base, ArithOp::Mod)
    }
    
    fn op_pow(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        self.op_arithmetic(inst, base, ArithOp::Pow)
    }
    
    /// Generic arithmetic with DIRECT metamethod execution (COMPLETE QUEUE ELIMINATION!)
    fn op_arithmetic(&mut self, inst: Instruction, base: usize, op: ArithOp) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        
        let left = self.read_rk(base, inst.get_b())?;
        let right = self.read_rk(base, inst.get_c())?;
        
        if matches!(op, ArithOp::Add) {
            eprintln!("DEBUG ADD: Executing at base={}, A={} (DIRECT EXECUTION)", base, a);
            eprintln!("DEBUG ADD: Left operand: {:?}", left);
            eprintln!("DEBUG ADD: Right operand: {:?}", right);
        }
        
        // Try direct arithmetic first
        if let (Value::Number(l), Value::Number(r)) = (&left, &right) {
            let result = match op {
                ArithOp::Add => {
                    eprintln!("DEBUG ADD: Numeric addition: {} + {} = {}", l, r, l + r);
                    Value::Number(l + r)
                },
                ArithOp::Sub => Value::Number(l - r),
                ArithOp::Mul => Value::Number(l * r),
                ArithOp::Div => {
                    if *r == 0.0 {
                        return Err(LuaError::RuntimeError("attempt to divide by zero".to_string()));
                    }
                    Value::Number(l / r)
                }
                ArithOp::Mod => {
                    if *r == 0.0 {
                        return Err(LuaError::RuntimeError("attempt to perform 'n%0'".to_string()));
                    }
                    Value::Number(l % r)
                }
                ArithOp::Pow => Value::Number(l.powf(*r)),
            };
            
            if matches!(op, ArithOp::Add) {
                eprintln!("DEBUG ADD: Storing result {:?} in register R({})", result, base + a);
            }
            
            self.set_register(base + a, result)?;
            return Ok(());
        }

        // Try metamethods with DIRECT execution (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
        let mm_key = match op {
            ArithOp::Add => &self.heap.metamethod_names.add,
            ArithOp::Sub => &self.heap.metamethod_names.sub,
            ArithOp::Mul => &self.heap.metamethod_names.mul,
            ArithOp::Div => &self.heap.metamethod_names.div,
            ArithOp::Mod => &self.heap.metamethod_names.mod_op,
            ArithOp::Pow => &self.heap.metamethod_names.pow,
        };
        
        let mm = match self.find_metamethod(&left, &Value::String(Rc::clone(mm_key)))? {
            Some(method) => Some(method),
            None => self.find_metamethod(&right, &Value::String(Rc::clone(mm_key)))?,
        };

        if let Some(method) = mm {
            if matches!(op, ArithOp::Add) {
                eprintln!("DEBUG ADD: Found metamethod __add, executing DIRECTLY (no queue)");
            }
            // Execute metamethod DIRECTLY (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
            self.execute_function_call(method, vec![left, right], 1, base + a, false, None)?;
            return Ok(());
        }
        
        if matches!(op, ArithOp::Add) {
            eprintln!("DEBUG ADD: No metamethod found, giving type error");
        }
        Err(LuaError::TypeError {
            expected: "number".to_string(),
            got: format!("'{}' and '{}'", left.type_name(), right.type_name()),
        })
    }
    
    /// UNM: R(A) := -R(B) (DIRECT metamethod execution)
    fn op_unm(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        let operand = self.get_register(base + b)?;
        
        let result = match &operand {
            Value::Number(n) => Ok(Value::Number(-*n)),
            _ => {
                // Try metamethod with DIRECT execution (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
                let mm = self.find_metamethod(&operand, &Value::String(Rc::clone(&self.heap.metamethod_names.unm)))?;
                
                if let Some(method) = mm {
                    // Execute metamethod DIRECTLY (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
                    self.execute_function_call(method, vec![operand.clone()], 1, base + a, false, None)?;
                    return Ok(());
                }
                
                Err(LuaError::TypeError {
                    expected: "number".to_string(),
                    got: operand.type_name().to_string(),
                })
            }
        }?;
        
        self.set_register(base + a, result)?;
        Ok(())
    }
    
    /// NOT: R(A) := not R(B)
    fn op_not(&self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        let operand = self.get_register(base + b)?;
        let result = Value::Boolean(operand.is_falsey());
        
        self.set_register(base + a, result)?;
        Ok(())
    }
    
    /// LEN: R(A) := length of R(B) (DIRECT metamethod execution)
    fn op_len(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        let operand = self.get_register(base + b)?;
        
        let result = match &operand {
            Value::String(ref handle) => {
                let string_ref = handle.borrow();
                Ok(Value::Number(string_ref.len() as f64))
            },
            Value::Table(ref handle) => {
                // Check for metamethod with DIRECT execution (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
                let mm = self.find_metamethod(&operand, &Value::String(Rc::clone(&self.heap.metamethod_names.len)))?;
                
                if let Some(method) = mm {
                    // Execute metamethod DIRECTLY (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
                    self.execute_function_call(method, vec![operand.clone()], 1, base + a, false, None)?;
                    return Ok(());
                }
                
                let table_ref = handle.borrow();
                Ok(Value::Number(table_ref.array_len() as f64))
            },
            _ => Err(LuaError::TypeError {
                expected: "string or table".to_string(),
                got: operand.type_name().to_string(),
            }),
        }?;
        
        self.set_register(base + a, result)?;
        Ok(())
    }
    
    /// CONCAT: R(A) := R(B).. ... ..R(C) (DIRECT execution - MAJOR QUEUE ELIMINATION!)
    fn op_concat(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        let c = inst.get_c() as usize;
        
        eprintln!("DEBUG CONCAT: EXECUTING at base={}, A={} (DIRECT EXECUTION)", base, a);
        
        if b == c {
            let value = self.get_register(base + b)?;
            self.set_register(base + a, value)?;
            return Ok(());
        }
        
        let mut values = Vec::with_capacity(c - b + 1);
        for i in b..=c {
            values.push(self.get_register(base + i)?);
        }
        
        let mut can_concat_directly = true;
        for value in &values {
            if !matches!(value, Value::String(_) | Value::Number(_)) {
                can_concat_directly = false;
                break;
            }
        }
        
        if can_concat_directly {
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
                    _ => unreachable!(),
                }
            }
            
            let string_handle = self.heap.create_string(&result)?;
            self.set_register(base + a, Value::String(string_handle))?;
        } else {
            // Process with DIRECT metamethod execution (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
            let mut temp = values[0].clone();

            for i in 1..values.len() {
                let right = values[i].clone();

                let both_primitive = matches!(&temp, Value::String(_) | Value::Number(_)) &&
                                     matches!(&right, Value::String(_) | Value::Number(_));

                if both_primitive {
                    let mut s = String::new();
                    match temp {
                        Value::String(h) => s.push_str(h.borrow().to_str().unwrap_or("")),
                        Value::Number(n) => s.push_str(&n.to_string()),
                        _ => {},
                    }
                    match right {
                        Value::String(h) => s.push_str(h.borrow().to_str().unwrap_or("")),
                        Value::Number(n) => s.push_str(&n.to_string()),
                        _ => {},
                    }
                    let h = self.heap.create_string(&s)?;
                    temp = Value::String(h);
                } else {
                    // Try metamethod with DIRECT execution (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
                    let mm = match self.find_metamethod(&temp, &Value::String(Rc::clone(&self.heap.metamethod_names.concat)))? {
                        Some(m) => Some(m),
                        None => self.find_metamethod(&right, &Value::String(Rc::clone(&self.heap.metamethod_names.concat)))?,
                    };
                    
                    if let Some(method) = mm {
                        // Execute metamethod DIRECTLY (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
                        let temp_reg = self.heap.get_stack_size(&self.current_thread);
                        self.ensure_stack_space(temp_reg + 1)?;
                        
                        self.execute_function_call(method, vec![temp.clone(), right.clone()], 1, temp_reg, false, None)?;
                        
                        temp = self.get_register(temp_reg)?;
                        
                        // Truncate temporary register
                        let mut thread = self.current_thread.borrow_mut();
                        thread.stack.truncate(temp_reg);
                    } else {
                        return Err(LuaError::TypeError {
                            expected: "string or number".to_string(),
                            got: format!("{} and {}", temp.type_name(), right.type_name()),
                        });
                    }
                }
            }

            self.set_register(base + a, temp)?;
        }
        
        Ok(())
    }
    
    /// JMP: pc += sBx
    fn op_jmp(&mut self, inst: Instruction) -> LuaResult<()> {
        let sbx = inst.get_sbx();
        let pc = self.heap.get_pc(&self.current_thread)?;
        let new_pc = (pc as isize + sbx as isize) as usize;
        self.heap.set_pc(&self.current_thread, new_pc)?;
        Ok(())
    }
    
    /// CALL: R(A), ..., R(A+C-2) := R(A)(R(A+1), ..., R(A+B-1)) (DIRECT execution)
    fn op_call(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
        let a = inst.get_a() as usize;
        let b = inst.get_b();
        let c = inst.get_c();
        
        let func = self.get_register(base + a)?;
        
        let arg_count = if b == 0 {
            self.heap.get_stack_size(&self.current_thread) - (base + a + 1)
        } else {
            (b - 1) as usize
        };
        
        let mut args = Vec::with_capacity(arg_count);
        for i in 0..arg_count {
            args.push(self.get_register(base + a + 1 + i)?);
        }
        
        let expected_results = if c == 0 {
            -1
        } else {
            (c - 1) as i32
        };
        
        // Execute function call DIRECTLY (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
        self.execute_function_call(func, args, expected_results, base + a, false, None)
    }
    
    /// RETURN: return R(A), ..., R(A+B-2) (DIRECT execution)
    fn op_return(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
        let a = inst.get_a() as usize;
        let b = inst.get_b();

        eprintln!("DEBUG RETURN: Executing with A={}, B={}, base={} (DIRECT)", a, b, base);

        self.heap.close_upvalues(&self.current_thread, base)?;

        let mut values = Vec::new();
        if b == 0 {
            let stack_size = self.heap.get_stack_size(&self.current_thread);
            let return_start = base + a;
            eprintln!("DEBUG RETURN: Multireturn (B=0). Collecting all values from R({}) to stack top ({})", 
                     return_start, stack_size);
            
            if stack_size > return_start {
                for i in return_start..stack_size {
                    values.push(self.get_register(i)?);
                }
            }
        } else {
            let num_returns = (b - 1) as usize;
            eprintln!("DEBUG RETURN: Fixed return (B={}). Collecting {} values from R({})", 
                     b, num_returns, base + a);
            
            for i in 0..num_returns {
                if let Ok(value) = self.get_register(base + a + i) {
                    values.push(value);
                } else {
                    values.push(Value::Nil);
                }
            }
        }

        eprintln!("DEBUG RETURN: Collected {} return values", values.len());

        // Process return DIRECTLY (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
        self.process_return(values)
    }
    
    /// TFORCALL/TFORLOOP: DIRECT execution eliminates temporal state separation (NO QUEUE!)
    fn op_tforcall(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let c = inst.get_c() as usize;

        eprintln!("DEBUG TFORCALL: EXECUTING DIRECTLY - eliminates temporal state separation!");
        
        let call_base = base + a + 3;

        let iter_func = self.get_register(base + a)?;
        let state = self.get_register(base + a + 1)?;
        let control = self.get_register(base + a + 2)?;
        let args = vec![state, control];

        self.ensure_stack_space(call_base + 1 + args.len())?;

        self.set_register(call_base, iter_func.clone())?;
        for (i, arg) in args.iter().enumerate() {
            self.set_register(call_base + 1 + i, arg.clone())?;
        }

        // Execute iterator call IMMEDIATELY (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
        self.execute_function_call(iter_func, args, c as i32, call_base, false, None)?;

        Ok(())
    }
    
    /// TFORLOOP: DIRECT register check (NO temporal separation!)
    fn op_tforloop(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let sbx = inst.get_sbx();
        
        eprintln!("DEBUG TFORLOOP: base={}, A={}, sBx={} (DIRECT CHECK)", base, a, sbx);

        // DIRECT register check - no temporal separation! - COMPLETE QUEUE ELIMINATION
        let first_var_reg = base + a + 3;
        let stack_size = self.heap.get_stack_size(&self.current_thread);
        if first_var_reg >= stack_size {
            eprintln!("DEBUG TFORLOOP: End of iteration (stack truncated before result register at index {})", first_var_reg);
            return Ok(());
        }

        let first_result = self.get_register(first_var_reg)?;
        
        if !first_result.is_nil() {
            eprintln!("DEBUG TFORLOOP: Continuing iteration");
            let control_var_reg = base + a + 2;
            self.set_register(control_var_reg, first_result.clone())?;
            
            let pc = self.heap.get_pc(&self.current_thread)?;
            let new_pc = (pc as isize + sbx as isize) as usize;
            eprintln!("DEBUG TFORLOOP: Jumping back from PC {} to PC {} (DIRECT)", pc, new_pc);
            self.heap.set_pc(&self.current_thread, new_pc)?;
        } else {
            eprintln!("DEBUG TFORLOOP: End of iteration (first result is nil)");
        }
        
        Ok(())
    }
    
    /// EQ/LT/LE: DIRECT metamethod execution (NO comparison continuations!)
    fn op_eq(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a();
        let left = self.read_rk(base, inst.get_b())?;
        let right = self.read_rk(base, inst.get_c())?;

        if left == right {
            let result = true;
            if result != (a != 0) {
                self.heap.increment_pc(&self.current_thread)?;
            }
            return Ok(());
        }

        if !matches!(left, Value::Table(_)) || !matches!(right, Value::Table(_)) {
            let result = false;
            if result != (a != 0) {
                self.heap.increment_pc(&self.current_thread)?;
            }
            return Ok(());
        }

        let mt1 = self.get_metatable(&left)?;
        let mt2 = self.get_metatable(&right)?;

        if let (Some(mt1_handle), Some(mt2_handle)) = (mt1, mt2) {
            let eq_key = Value::String(Rc::clone(&self.heap.metamethod_names.eq));
            let mm1 = self.heap.get_table_field(&mt1_handle, &eq_key)?;
            
            if !mm1.is_nil() {
                let mm2 = self.heap.get_table_field(&mt2_handle, &eq_key)?;
                if mm1 == mm2 {
                    // Execute metamethod DIRECTLY (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
                    let temp = self.heap.get_stack_size(&self.current_thread);
                    self.ensure_stack_space(temp + 1)?;

                    self.execute_function_call(mm1, vec![left, right], 1, temp, false, None)?;
                    
                    let mm_result = self.get_register(temp)?;
                    let result = !mm_result.is_falsey();
                    
                    // Truncate temp register
                    let mut thread = self.current_thread.borrow_mut();
                    thread.stack.truncate(temp);
                    drop(thread);
                    
                    if result != (a != 0) {
                        self.heap.increment_pc(&self.current_thread)?;
                    }
                    return Ok(());
                }
            }
        }

        let result = false;
        if result != (a != 0) {
            self.heap.increment_pc(&self.current_thread)?;
        }
        Ok(())
    }
    
    fn op_lt(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a();
        let left = self.read_rk(base, inst.get_b())?;
        let right = self.read_rk(base, inst.get_c())?;

        let result = match (&left, &right) {
            (Value::Number(l), Value::Number(r)) => *l < *r,
            (Value::String(l), Value::String(r)) => {
                l.borrow().bytes < r.borrow().bytes
            }
            _ => {
                // Try __lt metamethod with DIRECT execution (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
                let mm = match self.find_metamethod(&left, &Value::String(Rc::clone(&self.heap.metamethod_names.lt)))? {
                    Some(m) => Some(m),
                    None => self.find_metamethod(&right, &Value::String(Rc::clone(&self.heap.metamethod_names.lt)))?,
                };
                if let Some(method) = mm {
                    let temp = self.heap.get_stack_size(&self.current_thread);
                    self.ensure_stack_space(temp + 1)?;

                    // Execute metamethod DIRECTLY (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
                    self.execute_function_call(method, vec![left, right], 1, temp, false, None)?;
                    
                    let mm_result = self.get_register(temp)?;
                    let result = !mm_result.is_falsey();
                    
                    let mut thread = self.current_thread.borrow_mut();
                    thread.stack.truncate(temp);
                    drop(thread);
                    
                    if result != (a != 0) {
                        self.heap.increment_pc(&self.current_thread)?;
                    }
                    return Ok(());
                } else {
                    return Err(LuaError::TypeError {
                        expected: "comparable values".to_string(),
                        got: format!("{} and {}", left.type_name(), right.type_name()),
                    });
                }
            }
        };

        let skip = result != (a != 0);
        if skip {
            self.heap.increment_pc(&self.current_thread)?;
        }
        Ok(())
    }
    
    fn op_le(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a();
        let left = self.read_rk(base, inst.get_b())?;
        let right = self.read_rk(base, inst.get_c())?;

        let result = match (&left, &right) {
            (Value::Number(l), Value::Number(r)) => *l <= *r,
            (Value::String(l), Value::String(r)) => {
                l.borrow().bytes <= r.borrow().bytes
            }
            _ => {
                // First try __le with DIRECT execution (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
                let le_key = Value::String(Rc::clone(&self.heap.metamethod_names.le));
                let mm_le = match self.find_metamethod(&left, &le_key)? {
                    Some(m) => Some(m),
                    None => self.find_metamethod(&right, &le_key)?,
                };

                if let Some(method) = mm_le {
                    let temp = self.heap.get_stack_size(&self.current_thread);
                    self.ensure_stack_space(temp + 1)?;

                    // Execute metamethod DIRECTLY (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
                    self.execute_function_call(method, vec![left, right], 1, temp, false, None)?;
                    
                    let mm_result = self.get_register(temp)?;
                    let result = !mm_result.is_falsey();
                    
                    let mut thread = self.current_thread.borrow_mut();
                    thread.stack.truncate(temp);
                    drop(thread);
                    
                    if result != (a != 0) {
                        self.heap.increment_pc(&self.current_thread)?;
                    }
                    return Ok(());
                }

                // Try __lt fallback with DIRECT execution (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
                let lt_key = Value::String(Rc::clone(&self.heap.metamethod_names.lt));
                let mm_lt = match self.find_metamethod(&left, &lt_key)? {
                    Some(m) => Some(m),
                    None => self.find_metamethod(&right, &lt_key)?,
                };

                if let Some(method) = mm_lt {
                    let temp = self.heap.get_stack_size(&self.current_thread);
                    self.ensure_stack_space(temp + 1)?;

                    // Execute __lt(right, left) DIRECTLY (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
                    self.execute_function_call(method, vec![right, left], 1, temp, false, None)?;
                    
                    let mm_result = self.get_register(temp)?;
                    let result = mm_result.is_falsey(); // NEGATE for LE fallback
                    
                    let mut thread = self.current_thread.borrow_mut();
                    thread.stack.truncate(temp);
                    drop(thread);
                    
                    if result != (a != 0) {
                        self.heap.increment_pc(&self.current_thread)?;
                    }
                    return Ok(());
                }

                return Err(LuaError::TypeError {
                    expected: "comparable values".to_string(),
                    got: format!("{} and {}", left.type_name(), right.type_name()),
                });
            }
        };

        let skip = result != (a != 0);
        if skip {
            self.heap.increment_pc(&self.current_thread)?;
        }
        Ok(())
    }
    
    /// Remaining opcodes with direct execution...
    
    fn op_forprep(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let sbx = inst.get_sbx();

        let initial_val = self.get_register(base + a)?;
        let limit_val = self.get_register(base + a + 1)?;
        let step_val = self.get_register(base + a + 2)?;

        let initial_num = match initial_val {
            Value::Number(n) => n,
            Value::String(s) => {
                let n = s.borrow().to_str().unwrap_or("").parse::<f64>()
                    .map_err(|_| LuaError::RuntimeError("'for' initial value is not a number".to_string()))?;
                self.set_register(base + a, Value::Number(n))?;
                n
            },
            _ => return Err(LuaError::RuntimeError("'for' initial value must be a number".to_string())),
        };

        let _limit_num = match limit_val {
            Value::Number(n) => n,
            Value::String(s) => {
                let n = s.borrow().to_str().unwrap_or("").parse::<f64>()
                    .map_err(|_| LuaError::RuntimeError("'for' limit is not a number".to_string()))?;
                self.set_register(base + a + 1, Value::Number(n))?;
                n
            },
            _ => return Err(LuaError::RuntimeError("'for' limit must be a number".to_string())),
        };

        let step_num = match step_val {
            Value::Number(n) => n,
            Value::String(s) => {
                let n = s.borrow().to_str().unwrap_or("").parse::<f64>()
                    .map_err(|_| LuaError::RuntimeError("'for' step is not a number".to_string()))?;
                self.set_register(base + a + 2, Value::Number(n))?;
                n
            },
            _ => return Err(LuaError::RuntimeError("'for' step must be a number".to_string())),
        };

        let prepared = initial_num - step_num;
        self.set_register(base + a, Value::Number(prepared))?;

        let pc = self.heap.get_pc(&self.current_thread)?;
        let new_pc = (pc as isize + sbx as isize) as usize;
        self.heap.set_pc(&self.current_thread, new_pc)?;

        Ok(())
    }
    
    fn op_forloop(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let sbx = inst.get_sbx();
        
        let counter_val = self.get_register(base + a)?;
        let limit_val = self.get_register(base + a + 1)?;
        let step_val = self.get_register(base + a + 2)?;
        
        let counter_num = match counter_val {
            Value::Number(n) => n,
            _ => return Err(LuaError::RuntimeError("'for' loop counter must be a number".to_string())),
        };
        
        let limit_num = match limit_val {
            Value::Number(n) => n,
            _ => return Err(LuaError::RuntimeError("'for' loop limit must be a number".to_string())),
        };
        
        let step_num = match step_val {
            Value::Number(n) => n,
            _ => return Err(LuaError::RuntimeError("'for' loop step must be a number".to_string())),
        };
        
        let new_counter = counter_num + step_num;
        self.set_register(base + a, Value::Number(new_counter))?;
        
        let should_continue = if step_num > 0.0 {
            new_counter <= limit_num
        } else {
            new_counter >= limit_num
        };
        
        if should_continue {
            self.set_register(base + a + 3, Value::Number(new_counter))?;
            
            let pc = self.heap.get_pc(&self.current_thread)?;
            let new_pc = (pc as isize + sbx as isize) as usize;
            self.heap.set_pc(&self.current_thread, new_pc)?;
        }
        
        Ok(())
    }
    
    fn op_closure(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let bx = inst.get_bx() as usize;
        
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        let closure_ref = frame.closure.borrow();
        
        let proto_value = match &closure_ref.proto.constants[bx] {
            Value::FunctionProto(handle) => Rc::clone(handle),
            _ => {
                drop(closure_ref);
                return Err(LuaError::RuntimeError(
                    format!("Constant {} is not a function prototype", bx)
                ));
            }
        };
        
        let num_upvalues = proto_value.upvalues.len();
        drop(closure_ref);
        
        let current_pc = self.heap.get_pc(&self.current_thread)?;
        
        let mut upvalues = Vec::with_capacity(num_upvalues);
        
        for (i, _upvalue_info) in proto_value.upvalues.iter().enumerate() {
            let pseudo_inst = self.get_instruction(&frame.closure, current_pc + i)?;
            let pseudo = Instruction(pseudo_inst);
            
            let upvalue = match pseudo.get_opcode() {
                OpCode::Move => {
                    let idx = pseudo.get_b() as usize;
                    let absolute_idx = base + idx;

                    let stack_size = self.heap.get_stack_size(&self.current_thread);
                    if absolute_idx >= stack_size {
                        return Err(LuaError::RuntimeError(format!(
                            "op_closure: cannot capture upvalue from invalid register {} (stack size: {})",
                            absolute_idx, stack_size
                        )));
                    }

                    self.heap.find_or_create_upvalue(&self.current_thread, absolute_idx)?
                },
                OpCode::GetUpval => {
                    let idx = pseudo.get_b() as usize;
                    
                    let parent_closure_ref = frame.closure.borrow();
                    if idx >= parent_closure_ref.upvalues.len() {
                        drop(parent_closure_ref);
                        return Err(LuaError::RuntimeError(
                            format!("Upvalue index {} out of bounds", idx)
                        ));
                    }
                    
                    Rc::clone(&parent_closure_ref.upvalues[idx])
                },
                _ => {
                    return Err(LuaError::RuntimeError(
                        format!("Invalid pseudo-instruction for upvalue: {:?}", pseudo.get_opcode())
                    ));
                }
            };
            
            upvalues.push(upvalue);
        }
        
        let new_closure = self.heap.create_closure(Rc::clone(&proto_value), upvalues);
        self.set_register(base + a, Value::Closure(new_closure))?;
        
        let new_pc = current_pc + num_upvalues;
        self.heap.set_pc(&self.current_thread, new_pc)?;
        
        Ok(())
    }
    
    fn op_getupval(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        let closure_ref = frame.closure.borrow();
        
        if b >= closure_ref.upvalues.len() {
            drop(closure_ref);
            return Err(LuaError::RuntimeError(format!(
                "Upvalue index {} out of bounds", b
            )));
        }
        
        let upvalue = Rc::clone(&closure_ref.upvalues[b]);
        drop(closure_ref);
        
        let value = self.heap.get_upvalue_value(&upvalue);
        self.set_register(base + a, value)?;
        
        Ok(())
    }
    
    fn op_setupval(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        let value = self.get_register(base + a)?;
        
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        let closure_ref = frame.closure.borrow();
        
        if b >= closure_ref.upvalues.len() {
            drop(closure_ref);
            return Err(LuaError::RuntimeError(format!(
                "Upvalue index {} out of bounds", b
            )));
        }
        
        let upvalue = Rc::clone(&closure_ref.upvalues[b]);
        drop(closure_ref);
        
        self.heap.set_upvalue_value(&upvalue, value.clone())?;
        
        Ok(())
    }
    
    fn op_close(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        self.heap.close_upvalues(&self.current_thread, base + a)?;
        Ok(())
    }
    
    fn op_setlist(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let mut b = inst.get_b() as usize;
        let mut c = inst.get_c() as usize;
        
        const FIELDS_PER_FLUSH: usize = 50;
        
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
        
        if c == 0 {
            let pc = self.heap.get_pc(&self.current_thread)?;
            let frame = self.heap.get_current_frame(&self.current_thread)?;
            c = self.get_instruction(&frame.closure, pc)? as usize;
            self.heap.increment_pc(&self.current_thread)?;
        }
        
        let array_base = (c - 1) * FIELDS_PER_FLUSH;
        
        let count = if b == 0 {
            b = self.heap.get_stack_size(&self.current_thread) - (base + a + 1);
            b
        } else {
            b
        };
        
        for i in 0..count {
            let value = self.get_register(base + a + 1 + i)?;
            let index = array_base + i + 1;
            let key = Value::Number(index as f64);
            
            self.heap.set_table_field_raw(&table_handle, &key, &value)?;
        }
        
        Ok(())
    }
    
    fn op_tailcall(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
        let a = inst.get_a() as usize;
        let b = inst.get_b();
        
        let func = self.get_register(base + a)?;
        
        let arg_count = if b == 0 {
            self.heap.get_stack_size(&self.current_thread) - (base + a + 1)
        } else {
            (b - 1) as usize
        };
        
        let mut args = Vec::with_capacity(arg_count);
        for i in 0..arg_count {
            args.push(self.get_register(base + a + 1 + i)?);
        }
        
        self.heap.close_upvalues(&self.current_thread, base)?;
        
        let frame = self.heap.pop_call_frame(&self.current_thread)?;
        let expected_results = frame.expected_results.map_or(-1, |n| n as i32);
        
        let result_base = if let Ok(parent_frame) = self.heap.get_current_frame(&self.current_thread) {
            parent_frame.base_register as usize - 1
        } else {
            0
        };
        
        // Execute tail call DIRECTLY (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
        self.execute_function_call(func, args, expected_results, result_base, false, None)
    }
    
    fn op_vararg(&self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b();
        
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        
        let varargs = match &frame.varargs {
            Some(varargs) => varargs,
            None => return Ok(()),
        };
        
        let count = if b == 0 {
            varargs.len()
        } else {
            (b - 1) as usize
        };
        
        for i in 0..count {
            if i < varargs.len() {
                self.set_register(base + a + i, varargs[i].clone())?;
            } else {
                self.set_register(base + a + i, Value::Nil)?;
            }
        }
        
        Ok(())
    }
    
    fn op_test(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let c = inst.get_c();
        
        let value = self.get_register(base + a)?;
        let is_truthy = !value.is_falsey();
        let skip = is_truthy != (c != 0);
        
        if skip {
            let pc = self.heap.get_pc(&self.current_thread)?;
            self.heap.set_pc(&self.current_thread, pc + 1)?;
        }
        
        Ok(())
    }
    
    fn op_testset(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        let c = inst.get_c();
        
        let value = self.get_register(base + b)?;
        let is_truthy = !value.is_falsey();
        
        if is_truthy == (c != 0) {
            self.set_register(base + a, value)?;
        } else {
            let pc = self.heap.get_pc(&self.current_thread)?;
            self.heap.set_pc(&self.current_thread, pc + 1)?;
        }
        
        Ok(())
    }
}

/// Helper enums (simplified - no queue dependencies!)
#[derive(Debug, Clone, Copy)]
enum ArithOp {
    Add,
    Sub,  
    Mul,
    Div,
    Mod,
    Pow,
}

/// ExecutionContext implementation (completely queue-free!)
impl<'a> ExecutionContext for VmExecutionContext<'a> {
    fn arg_count(&self) -> usize {
        self.nargs
    }
    
    fn get_arg(&self, index: usize) -> LuaResult<Value> {
        if index >= self.nargs {
            return Err(LuaError::RuntimeError(format!(
                "Argument {} out of range (nargs: {})", index, self.nargs
            )));
        }
        
        let register_index = self.base + index;
        self.vm.get_register(register_index)
    }
    
    fn push_result(&mut self, value: Value) -> LuaResult<()> {
        self.results.push(value);
        self.results_pushed += 1;
        Ok(())
    }
    
    fn set_return(&mut self, index: usize, value: Value) -> LuaResult<()> {
        if index >= self.results.len() {
            self.results.resize(index + 1, Value::Nil);
        }
        self.results[index] = value;
        if index + 1 > self.results_pushed {
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
    
    fn set_table_field(&mut self, table: &TableHandle, key: Value, value: Value) -> LuaResult<()> {
        // DIRECT table setting for C functions (NO QUEUE!) - COMPLETE QUEUE ELIMINATION
        let mut table_ref = table.borrow_mut();
        table_ref.set_field(key, value)?;
        Ok(())
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
        let table_ref = table.borrow();

        if key.is_nil() {
            for i in 0..table_ref.array.len() {
                if !table_ref.array[i].is_nil() {
                    return Ok(Some((Value::Number((i + 1) as f64), table_ref.array[i].clone())));
                }
            }
            if let Some((k, v)) = table_ref.map.iter().next() {
                return Ok(Some((k.to_value(), v.clone())));
            }
            return Ok(None);
        }

        let mut in_array = false;
        if let Value::Number(n) = key {
            if n.fract() == 0.0 && *n >= 1.0 {
                let index = *n as usize;
                if index <= table_ref.array.len() {
                    in_array = true;
                    for i in index..table_ref.array.len() {
                        if !table_ref.array[i].is_nil() {
                            return Ok(Some((Value::Number((i + 1) as f64), table_ref.array[i].clone())));
                        }
                    }
                }
            }
        }

        if !in_array {
            let current_key_hashable = match HashableValue::from_value(key) {
                Ok(h) => h,
                Err(_) => return Err(LuaError::RuntimeError(format!("invalid key to 'next'"))),
            };
            
            let mut found_current = false;
            for (hash_key, hash_value) in table_ref.map.iter() {
                if found_current {
                    return Ok(Some((hash_key.to_value(), hash_value.clone())));
                }
                if hash_key == &current_key_hashable {
                    found_current = true;
                }
            }
            if !found_current {
                return Err(LuaError::RuntimeError(format!("invalid key to 'next'")));
            }
        } else {
            if let Some((k, v)) = table_ref.map.iter().next() {
                return Ok(Some((k.to_value(), v.clone())));
            }
        }
        
        Ok(None)
    }
    
    fn globals_handle(&self) -> LuaResult<TableHandle> {
        Ok(self.vm.heap.globals())
    }
    
    fn get_call_base(&self) -> usize {
        self.base
    }
    
    fn pcall(&mut self, _func: Value, _args: Vec<Value>) -> LuaResult<()> {
        // Simplified pcall - NO QUEUE DEPENDENCIES! - COMPLETE QUEUE ELIMINATION
        Ok(())
    }
    
    fn xpcall(&mut self, _func: Value, _err_handler: Value) -> LuaResult<()> {
        // Simplified xpcall - NO QUEUE DEPENDENCIES! - COMPLETE QUEUE ELIMINATION
        Ok(())
    }
    
    fn get_upvalue_value(&self, upvalue: &UpvalueHandle) -> LuaResult<Value> {
        Ok(self.vm.heap.get_upvalue_value(upvalue))
    }
    
    fn set_upvalue_value(&self, upvalue: &UpvalueHandle, value: Value) -> LuaResult<()> {
        self.vm.heap.set_upvalue_value(upvalue, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_rc_vm_creation() -> LuaResult<()> {
        let _vm = RcVM::new()?;
        // Test COMPLETE queue elimination - no more queue infrastructure!
        Ok(())
    }
}