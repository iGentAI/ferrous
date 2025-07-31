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
        // CRITICAL: Initialize stdlib BEFORE executing any user code
        self.init_stdlib()?;
        
        // Create main function
        let main_closure = self.load_module(module)?;
        
        // Clear stack and frames
        {
            let mut thread = self.current_thread.borrow_mut();
            thread.stack.clear();
            thread.call_frames.clear();
            thread.top = 0;
        }
        
        // Place main function and arguments on stack
        self.set_register(0, Value::Closure(main_closure.clone()))?;
        for (i, arg) in args.iter().enumerate() {
            self.set_register(i + 1, arg.clone())?;
        }
        
        // CANONICAL LUA 5.1: Calculate main frame_top and reserve window
        let main_max_stack = main_closure.borrow().proto.max_stack_size as usize;
        let main_frame_top = 1 + main_max_stack;
        
        // CRITICAL FIX: Reserve entire main frame window immediately
        self.reserve_frame_window(main_frame_top)?;
        
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
            frame_top: main_frame_top,
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
        
        // Always get the current (top) frame to ensure we execute the most recently called function
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        let base = frame.base_register as usize;
        let pc = frame.pc;
        
        // Debug frame execution context
        if pc == 0 {
            eprintln!("DEBUG STEP: Starting execution of new frame at PC=0, base={}, bytecode_len={}", 
                     base, frame.closure.borrow().proto.bytecode.len());
        }
        
        let instruction = self.get_instruction(&frame.closure, pc)?;
        let inst = Instruction(instruction);
        
        eprintln!("DEBUG STEP: Executing opcode {:?} at PC={}, base={}", inst.get_opcode(), pc, base);
        
        self.heap.increment_pc(&self.current_thread)?;
        
        // Execute instruction DIRECTLY with corrected opcode numbering and StepResult propagation
        let result = match inst.get_opcode() {
            OpCode::Move => { self.op_move(inst, base)?; Ok(StepResult::Continue) }
            OpCode::LoadK => { self.op_loadk(inst, base)?; Ok(StepResult::Continue) }
            OpCode::LoadBool => { self.op_loadbool(inst, base)?; Ok(StepResult::Continue) }
            OpCode::LoadNil => { self.op_loadnil(inst, base)?; Ok(StepResult::Continue) }
            OpCode::GetGlobal => { self.op_getglobal(inst, base)?; Ok(StepResult::Continue) }
            OpCode::SetGlobal => { self.op_setglobal(inst, base)?; Ok(StepResult::Continue) }
            OpCode::GetTable => self.op_gettable(inst, base),
            OpCode::SetTable => self.op_settable(inst, base),
            OpCode::NewTable => { self.op_newtable(inst, base)?; Ok(StepResult::Continue) }
            OpCode::SelfOp => self.op_self(inst, base),
            OpCode::Add => self.op_add(inst, base),
            OpCode::Sub => self.op_sub(inst, base),
            OpCode::Mul => self.op_mul(inst, base),
            OpCode::Div => self.op_div(inst, base),
            OpCode::Mod => self.op_mod(inst, base),
            OpCode::Pow => self.op_pow(inst, base),
            OpCode::Unm => self.op_unm(inst, base),
            OpCode::Not => { self.op_not(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Len => self.op_len(inst, base),
            OpCode::Concat => { self.op_concat(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Jmp => { self.op_jmp(inst)?; Ok(StepResult::Continue) }
            OpCode::Eq => self.op_eq(inst, base),
            OpCode::Lt => self.op_lt(inst, base),
            OpCode::Le => self.op_le(inst, base),
            OpCode::Test => { self.op_test(inst, base)?; Ok(StepResult::Continue) }
            OpCode::TestSet => { self.op_testset(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Call => self.op_call(inst, base),
            OpCode::TailCall => self.op_tailcall(inst, base),
            OpCode::Return => self.op_return(inst, base),
            OpCode::ForPrep => { self.op_forprep(inst, base)?; Ok(StepResult::Continue) }
            OpCode::ForLoop => { self.op_forloop(inst, base)?; Ok(StepResult::Continue) }
            OpCode::TForLoop => self.op_tforloop(inst, base),
            OpCode::VarArg => { self.op_vararg(inst, base)?; Ok(StepResult::Continue) }
            OpCode::GetUpval => { self.op_getupval(inst, base)?; Ok(StepResult::Continue) }
            OpCode::SetUpval => { self.op_setupval(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Closure => { self.op_closure(inst, base)?; Ok(StepResult::Continue) }
            OpCode::Close => { self.op_close(inst, base)?; Ok(StepResult::Continue) }
            OpCode::SetList => { self.op_setlist(inst, base)?; Ok(StepResult::Continue) }
            _ => Err(LuaError::NotImplemented(format!("Opcode {:?}", inst.get_opcode()))),
        };
        
        match &result {
            Ok(StepResult::Continue) => {
                eprintln!("DEBUG STEP: Opcode {:?} completed, continuing execution", inst.get_opcode());
            }
            Ok(StepResult::Completed(_)) => {
                eprintln!("DEBUG STEP: Opcode {:?} completed execution", inst.get_opcode());
            }
            Err(e) => {
                eprintln!("DEBUG STEP: Opcode {:?} failed: {:?}", inst.get_opcode(), e);
            }
        }
        
        result
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
        eprintln!("DEBUG execute_function_call: Called with func type: {}", func.type_name());
        eprintln!("DEBUG execute_function_call: {} args, expected_results={}, result_base={}", 
                 args.len(), expected_results, result_base);
        
        // CRITICAL DEBUG: Show the exact function value to understand type issues
        match &func {
            Value::Closure(closure_handle) => {
                eprintln!("DEBUG execute_function_call: ✓ Recognized as Closure - dispatching to call_lua_function");
                eprintln!("DEBUG execute_function_call: Closure has {} upvalues", 
                         closure_handle.borrow().upvalues.len());
                self.call_lua_function(Rc::clone(closure_handle), args, expected_results, result_base, is_protected, xpcall_handler)
            },
            Value::CFunction(cfunc) => {
                eprintln!("DEBUG execute_function_call: ✓ Recognized as CFunction");
                self.call_c_function(*cfunc, args, expected_results, result_base)
            },
            Value::Table(table_handle) => {
                eprintln!("DEBUG execute_function_call: Recognized as Table - checking for __call metamethod");
                // Check for __call metamethod
                let mm = self.find_metamethod(&Value::Table(Rc::clone(table_handle)), 
                                            &Value::String(Rc::clone(&self.heap.metamethod_names.call)))?;
                
                if let Some(metamethod) = mm {
                    eprintln!("DEBUG execute_function_call: Found __call metamethod");
                    let mut metamethod_args = vec![Value::Table(Rc::clone(table_handle))];
                    metamethod_args.extend(args);
                    
                    return self.execute_function_call(
                        metamethod, 
                        metamethod_args, 
                        expected_results, 
                        result_base, 
                        is_protected, 
                        xpcall_handler
                    );
                } else {
                    eprintln!("DEBUG execute_function_call: ✗ No __call metamethod found");
                    Err(LuaError::TypeError {
                        expected: "function".to_string(),
                        got: "table".to_string(),
                    })
                }
            },
            Value::FunctionProto(proto_handle) => {
                eprintln!("DEBUG execute_function_call: ✓ Recognized as FunctionProto - creating closure on demand");
                
                let current_env = {
                    let frame = self.heap.get_current_frame(&self.current_thread)?;
                    let closure_ref = frame.closure.borrow();
                    Rc::clone(&closure_ref.env)
                };
                
                let upvalues = Vec::new(); // On-demand closures start with empty upvalues
                let on_demand_closure = self.heap.create_closure(Rc::clone(&proto_handle), upvalues, current_env);
                
                self.call_lua_function(on_demand_closure, args, expected_results, result_base, is_protected, xpcall_handler)
            },
            Value::PendingMetamethod(boxed_mm) => {
                // Unwrap the pending metamethod and execute it
                eprintln!("DEBUG execute_function_call: ✓ Recognized as PendingMetamethod - unwrapping and executing");
                let metamethod = *boxed_mm.clone();
                self.execute_function_call(metamethod, args, expected_results, result_base, is_protected, xpcall_handler)
            },
            _ => {
                eprintln!("DEBUG execute_function_call: ✗ ERROR - Non-function type: {}", func.type_name());
                eprintln!("DEBUG execute_function_call: Value details: {:?}", func);
                Err(LuaError::TypeError {
                    expected: "function".to_string(),
                    got: func.type_name().to_string(),
                })
            }
        }
    }
    
    /// Call a Lua function with corrected vararg frame setup
    fn call_lua_function(
        &mut self,
        closure: ClosureHandle,
        args: Vec<Value>,
        expected_results: i32,
        result_base: usize,
        is_protected: bool,
        xpcall_handler: Option<Value>,
    ) -> LuaResult<StepResult> {
        let func_slot = result_base;
        let callee_base = result_base + 1;

        let (num_params, max_stack, is_vararg) = {
            let cl_ref = closure.borrow();
            let proto = &cl_ref.proto;
            (
                proto.num_params as usize,
                proto.max_stack_size as usize,
                proto.is_vararg,
            )
        };

        let frame_top = callee_base + max_stack;
        
        // CRITICAL FIX: Always create vararg frame info when is_vararg=true
        let varargs = if is_vararg {
            // ALWAYS store the vararg slice, even if it's empty
            if args.len() > num_params {
                Some(args[num_params..].to_vec())
            } else {
                Some(Vec::new()) // Empty varargs, not None
            }
        } else {
            None
        };

        let logical_top = if is_vararg {
            callee_base + args.len().max(num_params)
        } else {
            callee_base + num_params
        };

        self.reserve_frame_window(frame_top)?;

        self.set_register(func_slot, Value::Closure(closure.clone()))?;
        for (i, arg) in args.iter().enumerate() {
            self.set_register(callee_base + i, arg.clone())?;
        }
        
        // Fill missing parameters with nil
        for i in args.len()..num_params {
            self.set_register(callee_base + i, Value::Nil)?;
        }

        self.current_thread.borrow_mut().top = logical_top;

        let frame = CallFrame {
            closure: closure.clone(),
            pc: 0,
            base_register: callee_base as u16,
            expected_results: if expected_results >= 0 {
                Some(expected_results as usize)
            } else {
                None
            },
            varargs, // Now correctly stores frame info
            is_protected,
            xpcall_handler,
            result_base,
            frame_top,
        };

        self.heap.push_call_frame(&self.current_thread, frame)?;
        
        Ok(StepResult::Continue)
    }
    
    /// Reserve entire frame window immediately (canonical Lua 5.1 fixed stack semantics)
    fn reserve_frame_window(&self, frame_top: usize) -> LuaResult<()> {
        let mut thread_ref = self.current_thread.borrow_mut();
        
        // CANONICAL LUA 5.1: Ensure backing vector covers the whole fixed window
        if thread_ref.stack.len() < frame_top {
            thread_ref.stack.resize(frame_top, Value::Nil); // Physical growth with nil initialization
        }
        
        // Update logical top to include the reserved frame
        if thread_ref.top < frame_top {
            thread_ref.top = frame_top;
        }
        
        Ok(())
    }

    /// Call a C function with unified register stack per Lua 5.1 specification
    fn call_c_function(
        &mut self,
        func: super::rc_value::CFunction,
        args: Vec<Value>,
        expected_results: i32,
        result_base: usize,
    ) -> LuaResult<StepResult> {
        let func_slot = result_base;           // Function at result_base  
        let arg_base = result_base + 1;        // Arguments at result_base + 1

        self.ensure_stack_space(arg_base + args.len())?;

        self.set_register(func_slot, Value::CFunction(func))?;
        for (i, arg) in args.iter().enumerate() {
            self.set_register(arg_base + i, arg.clone())?;
        }

        self.process_c_function_call(
            func,
            func_slot,        // Function at result_base
            args.len(),
            expected_results,
            result_base,      // Results go to same location
        )
    }
    
    /// Process C function call with unified register stack addressing
    fn process_c_function_call(
        &mut self,
        function: super::rc_value::CFunction,
        func_slot: usize,  // Function position in unified stack
        nargs: usize,
        expected_results: i32,
        result_base: usize,
    ) -> LuaResult<StepResult> {
        let arg_base = func_slot + 1;  // Arguments follow function in unified stack

        let mut ctx = VmExecutionContext {
            vm: self,
            base: arg_base,  // Arguments at func_slot + 1
            nargs,
            results_pushed: 0,
            results: Vec::new(),
        };

        let declared_results = function(&mut ctx)?;
        let mut results = std::mem::take(&mut ctx.results);
        let pushed = results.len();

        let mut final_result_count = if declared_results < 0 { 
            pushed  
        } else { 
            declared_results as usize 
        };

        if final_result_count > pushed {
            results.resize(final_result_count, Value::Nil);
        } else if final_result_count < pushed {
            results.truncate(final_result_count);
        }

        if expected_results >= 0 {
            let expected = expected_results as usize;
            if expected < final_result_count {
                results.truncate(expected);
                final_result_count = expected;
            } else if expected > final_result_count {
                results.resize(expected, Value::Nil);
                final_result_count = expected;
            }
        }

        for (i, val) in results.iter().enumerate() {
            self.set_register(result_base + i, val.clone())?;
        }

        self.current_thread.borrow_mut().top = result_base + final_result_count;

        Ok(StepResult::Continue)
    }

    
    /// Process return with ZERO TOLERANCE - remove accommodations that mask problems
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
        // How many values does the caller expect?
        let result_count = expected_results.unwrap_or_else(|| values.len());

        // Write requested results (or nil padding) into the destination window
        for i in 0..result_count {
            let value = values.get(i).cloned().unwrap_or(Value::Nil);
            self.set_register(result_base + i, value)?;
        }

        // Update logical top to point just past the last visible result
        // For result_count == 0 this correctly collapses top to result_base
        self.current_thread.borrow_mut().top = result_base + result_count;
        
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
    
    /// Load module with proper environment inheritance for main closure
    fn load_module(&mut self, module: &compiler::CompiledModule) -> LuaResult<ClosureHandle> {
        let mut string_handles = Vec::with_capacity(module.strings.len());
        for s in &module.strings {
            string_handles.push(self.heap.create_string(s)?);
        }

        let mut proto_cache: Vec<Option<FunctionProtoHandle>> = vec![None; module.prototypes.len()];
        
        // Build all prototypes recursively
        for i in 0..module.prototypes.len() {
            Self::build_proto_recursive(i, module, &string_handles, &mut proto_cache, self)?;
        }
        
        // Extract all handles (guaranteed to be complete)
        let finalized_proto_handles: Vec<FunctionProtoHandle> = proto_cache
            .into_iter()
            .map(|opt| opt.unwrap()) // Safe: all prototypes built
            .collect();
        
        // Create main function with proper constant handling
        let mut main_constants = Vec::with_capacity(module.constants.len());
        for constant in module.constants.iter() {
            let value = self.create_value_from_constant(constant, &string_handles, &finalized_proto_handles)?;
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
        
        // Main closure uses pure closure.env model (no synthetic upvalue)
        let main_upvalues = vec![];  // No automatic environment upvalue
        let main_env = self.heap.globals();  // Environment as separate field only
        
        let main_closure = self.heap.create_closure(main_proto_handle, main_upvalues, main_env);
        
        Ok(main_closure)
    }
    
    // Helper function for recursive prototype building
    fn build_proto_recursive(
        idx: usize,
        module: &compiler::CompiledModule,
        string_handles: &[StringHandle],
        cache: &mut Vec<Option<FunctionProtoHandle>>,
        vm: &mut RcVM,
    ) -> LuaResult<Value> {
        // Already built? Return cached handle as Value
        if let Some(handle) = &cache[idx] {
            return Ok(Value::FunctionProto(Rc::clone(handle)));
        }
        
        let compiled_proto = &module.prototypes[idx];
        let mut constants = Vec::with_capacity(compiled_proto.constants.len());
        
        // Build all constants - recurse for FunctionProto constants
        for constant in &compiled_proto.constants {
            let value = match constant {
                CompilationConstant::Nil => Value::Nil,
                CompilationConstant::Boolean(b) => Value::Boolean(*b),
                CompilationConstant::Number(n) => Value::Number(*n),
                CompilationConstant::String(string_idx) => {
                    if *string_idx >= string_handles.len() {
                        return Err(LuaError::RuntimeError(format!("Invalid string index: {}", string_idx)));
                    }
                    Value::String(Rc::clone(&string_handles[*string_idx]))
                },
                CompilationConstant::FunctionProto(proto_idx) => {
                    Self::build_proto_recursive(*proto_idx, module, string_handles, cache, vm)?
                },
                CompilationConstant::Table(entries) => {
                    let table = vm.heap.create_table();
                    for (k, v) in entries {
                        let key = Self::build_constant_value(k, string_handles, cache, vm)?;
                        let val = Self::build_constant_value(v, string_handles, cache, vm)?;
                        vm.heap.set_table_field_raw(&table, &key, &val)?;
                    }
                    Value::Table(table)
                },
            };
            constants.push(value);
        }
        
        // Create complete prototype with proper bytecode
        let complete_proto = FunctionProto {
            bytecode: compiled_proto.bytecode.clone(),
            constants,
            num_params: compiled_proto.num_params,
            is_vararg: compiled_proto.is_vararg,
            max_stack_size: compiled_proto.max_stack_size,
            upvalues: compiled_proto.upvalues.iter().map(|u| super::rc_value::UpvalueInfo { 
                in_stack: u.in_stack, 
                index: u.index 
            }).collect(),
        };
        
        let handle = vm.heap.create_function_proto(complete_proto);
        cache[idx] = Some(Rc::clone(&handle));
        Ok(Value::FunctionProto(handle))
    }
    
    // Helper for building individual constant values
    fn build_constant_value(
        constant: &CompilationConstant,
        string_handles: &[StringHandle],
        _cache: &mut Vec<Option<FunctionProtoHandle>>,
        vm: &mut RcVM,
    ) -> LuaResult<Value> {
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
            _ => Ok(Value::Nil),
        }
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
    
    /// Return the logical top of the current thread.
    fn current_top(&self) -> usize {
        self.current_thread.borrow().top
    }

    /// Fetch a register value with SPECIFICATION-COMPLIANT dynamic growth
    fn get_register(&self, index: usize) -> LuaResult<Value> {
        // SPECIFICATION COMPLIANCE: Check against fixed frame_top limit first
        if let Ok(frame) = self.heap.get_current_frame(&self.current_thread) {
            if index >= frame.frame_top {
                return Err(LuaError::StackOverflow);
            }
        }
        
        // SPECIFICATION COMPLIANCE: Dynamic growth on access per Lua 5.1
        let mut thread_ref = self.current_thread.borrow_mut();
        if index >= thread_ref.stack.len() {
            thread_ref.stack.resize(index + 1, Value::Nil);
        }
        Ok(thread_ref.stack[index].clone())
    }

    /// Set register with SPECIFICATION-COMPLIANT bounds checking
    fn set_register(&self, index: usize, value: Value) -> LuaResult<()> {
        // SPECIFICATION COMPLIANCE: Enforce fixed frame_top limit
        if let Ok(frame) = self.heap.get_current_frame(&self.current_thread) {
            if index >= frame.frame_top {
                return Err(LuaError::StackOverflow);
            }
        }

        let mut thread_ref = self.current_thread.borrow_mut();

        if index >= thread_ref.stack.len() {
            thread_ref.stack.resize(index + 1, Value::Nil);
        }

        thread_ref.stack[index] = value;

        if index + 1 > thread_ref.top {
            thread_ref.top = index + 1;
        }
        Ok(())
    }
    
    /// Ensure the physical stack can hold `size` elements.  
    /// Does **not** manipulate `top` — callers decide whether the
    /// slots are logically visible.
    fn ensure_stack_space(&self, size: usize) -> LuaResult<()> {
        let mut thread = self.current_thread.borrow_mut();
        if thread.stack.len() < size {
            thread.stack.resize(size, Value::Nil);
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
    
    /// GETGLOBAL: R(A) := Gbl[Kst(Bx)]
    fn op_getglobal(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let bx = inst.get_bx() as usize;
        
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        let key = self.get_constant(&frame.closure, bx)?;
        
        // Use closure.env field for proper Lua 5.1 environment access
        let globals = {
            let closure_ref = frame.closure.borrow();
            Rc::clone(&closure_ref.env)
        };
        
        let value = self.heap.get_table_field(&globals, &key)?;
        
        // Handle metamethod results with complete integration
        match value {
            Value::PendingMetamethod(boxed_mm) => {
                // Execute __index metamethod for global access
                return self.execute_function_call(*boxed_mm, vec![Value::Table(globals), key], 1, base + a, false, None).map(|_| ());
            },
            other => {
                self.set_register(base + a, other)?;
            }
        }
        
        Ok(())
    }
    
    /// SETGLOBAL: Gbl[Kst(Bx)] := R(A)
    fn op_setglobal(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let bx = inst.get_bx() as usize;
        
        let value = self.get_register(base + a)?;
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        let key = self.get_constant(&frame.closure, bx)?;
        
        // Use closure.env field for proper Lua 5.1 environment access  
        let globals = {
            let closure_ref = frame.closure.borrow();
            Rc::clone(&closure_ref.env)
        };
        
        let metamethod_result = self.heap.set_table_field(&globals, &key, &value)?;
        
        if let Some(metamethod) = metamethod_result {
            // Execute __newindex metamethod for global setting
            self.execute_function_call(metamethod, vec![Value::Table(globals), key, value], 0, 0, false, None)?;
        }
        
        Ok(())
    }
    
    /// GETTABLE: R(A) := R(B)[RK(C)] - SPECIFICATION COMPLIANT with StepResult propagation
    fn op_gettable(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
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

        let value = self.heap.get_table_field(table_handle, &key)?;
        
        // SPECIFICATION COMPLIANCE: Handle metamethods with proper StepResult propagation
        let final_value = match value {
            Value::PendingMetamethod(boxed_mm) => {
                // Execute metamethod and propagate StepResult
                return self.execute_function_call(*boxed_mm, vec![Value::Table(Rc::clone(table_handle)), key.clone()], 1, base + a, false, None);
            },
            other => other,
        };
        
        self.set_register(base + a, final_value)?;
        Ok(StepResult::Continue)
    }
    
    /// SETTABLE: R(A)[RK(B)] := RK(C) - SPECIFICATION COMPLIANT with StepResult propagation
    fn op_settable(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
        let a = inst.get_a() as usize;
        
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
        
        let key = self.read_rk(base, inst.get_b())?;
        let value = self.read_rk(base, inst.get_c())?;
        
        // SPECIFICATION COMPLIANCE: Handle metamethods with proper StepResult propagation
        let metamethod_result = self.heap.set_table_field(table_handle, &key, &value)?;
        
        if let Some(metamethod) = metamethod_result {
            // Execute __newindex metamethod and propagate StepResult
            return self.execute_function_call(metamethod, vec![Value::Table(table_handle.clone()), key.clone(), value.clone()], 0, 0, false, None);
        }
        
        Ok(StepResult::Continue)
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
    
    /// SELF with StepResult propagation for metamethod handling
    fn op_self(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
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
                
                match method {
                    Value::PendingMetamethod(boxed_mm) => {
                        return self.execute_function_call(*boxed_mm, vec![Value::Table(Rc::clone(&table_handle)), key.clone()], 1, base + a, false, None);
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
        
        Ok(StepResult::Continue)
    }
    
    /// Generic arithmetic with SPECIFICATION COMPLIANT StepResult propagation
    fn op_arithmetic(&mut self, inst: Instruction, base: usize, op: ArithOp) -> LuaResult<StepResult> {
        let a = inst.get_a() as usize;
        
        let left = self.read_rk(base, inst.get_b())?;
        let right = self.read_rk(base, inst.get_c())?;
        
        // Try direct arithmetic first
        if let (Value::Number(l), Value::Number(r)) = (&left, &right) {
            let result = match op {
                ArithOp::Add => Value::Number(l + r),
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
            
            self.set_register(base + a, result)?;
            return Ok(StepResult::Continue);
        }

        // SPECIFICATION COMPLIANCE: Try metamethods with proper StepResult propagation
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
            // SPECIFICATION COMPLIANCE: Execute metamethod and propagate StepResult
            return self.execute_function_call(method, vec![left, right], 1, base + a, false, None);
        }
        
        Err(LuaError::TypeError {
            expected: "number".to_string(),
            got: format!("'{}' and '{}'", left.type_name(), right.type_name()),
        })
    }

    /// ADD with StepResult propagation
    fn op_add(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
        self.op_arithmetic(inst, base, ArithOp::Add)
    }
    
    /// SUB with StepResult propagation
    fn op_sub(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
        self.op_arithmetic(inst, base, ArithOp::Sub)
    }
    
    /// MUL with StepResult propagation
    fn op_mul(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
        self.op_arithmetic(inst, base, ArithOp::Mul)
    }
    
    /// DIV with StepResult propagation
    fn op_div(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
        self.op_arithmetic(inst, base, ArithOp::Div)
    }
    
    /// MOD with StepResult propagation  
    fn op_mod(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
        self.op_arithmetic(inst, base, ArithOp::Mod)
    }
    
    /// POW with StepResult propagation
    fn op_pow(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
        self.op_arithmetic(inst, base, ArithOp::Pow)
    }
    
    /// UNM: R(A) := -R(B) with StepResult propagation
    fn op_unm(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        let operand = self.get_register(base + b)?;
        
        let result = match &operand {
            Value::Number(n) => Ok(Value::Number(-*n)),
            _ => {
                let mm = self.find_metamethod(&operand, &Value::String(Rc::clone(&self.heap.metamethod_names.unm)))?;
                
                if let Some(method) = mm {
                    return self.execute_function_call(method, vec![operand.clone()], 1, base + a, false, None);
                }
                
                Err(LuaError::TypeError {
                    expected: "number".to_string(),
                    got: operand.type_name().to_string(),
                })
            }
        }?;
        
        self.set_register(base + a, result)?;
        Ok(StepResult::Continue)
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
    
    /// LEN: R(A) := length of R(B) with SPECIFICATION-COMPLIANT metamethod-first order
    fn op_len(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        let operand = self.get_register(base + b)?;
        
        // SPECIFICATION COMPLIANCE: Check for metamethod FIRST (Lua 5.1 §2.5.5)
        let mm = self.find_metamethod(&operand, &Value::String(Rc::clone(&self.heap.metamethod_names.len)))?;
        
        if let Some(method) = mm {
            return self.execute_function_call(method, vec![operand.clone()], 1, base + a, false, None);
        }
        
        // THEN check primitives if no metamethod
        let result = match &operand {
            Value::String(ref handle) => {
                let string_ref = handle.borrow();
                Value::Number(string_ref.len() as f64)
            },
            Value::Table(ref handle) => {
                let table_ref = handle.borrow();
                Value::Number(table_ref.array_len() as f64)
            },
            // Add userdata support (same as table pattern)
            Value::UserData(_) => {
                // Userdata without metamethod defaults to 0 length
                Value::Number(0.0)
            },
            _ => {
                return Err(LuaError::TypeError {
                    expected: "string, table, userdata, or value with __len metamethod".to_string(),
                    got: operand.type_name().to_string(),
                });
            }
        };
        
        self.set_register(base + a, result)?;
        Ok(StepResult::Continue)
    }
    
    /// CONCAT: R(A) := R(B).. ... ..R(C) (DIRECT execution - MAJOR QUEUE ELIMINATION!)
    fn op_concat(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        let c = inst.get_c() as usize;
        
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
            // Process with DIRECT metamethod execution (NO QUEUE!)
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
                    // Try metamethod with DIRECT execution (NO QUEUE!)
                    let mm = match self.find_metamethod(&temp, &Value::String(Rc::clone(&self.heap.metamethod_names.concat)))? {
                        Some(m) => Some(m),
                        None => self.find_metamethod(&right, &Value::String(Rc::clone(&self.heap.metamethod_names.concat)))?,
                    };
                    
                    if let Some(method) = mm {
                        // Execute metamethod DIRECTLY (NO QUEUE!)
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
    
    /// CALL: R(A), ..., R(A+C-2) := R(A)(R(A+1), ..., R(A+B-1)) - KISS LUA 5.1 SPECIFICATION
    fn op_call(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
        let a = inst.get_a() as usize;
        let b = inst.get_b();
        let c = inst.get_c();
        
        let func = self.get_register(base + a)?;
        
        let arg_count = if b == 0 {
            self.current_top().saturating_sub(base + a + 1)
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
        
        // Execute function call
        let result = self.execute_function_call(func, args, expected_results, base + a, false, None)?;
        
        // KISS LUA 5.1 SPECIFICATION: "L->top = A + nres" - DEAD SIMPLE
        // No complex calculations, no multiple borrows, just the specification requirement
        let nres = if c == 0 {
            // Multi-result: get actual count without complex borrowing
            let current_top = self.current_top();
            current_top.saturating_sub(base + a)
        } else {
            // Fixed result: specification says C-1 results
            (c - 1) as usize
        };
        
        // KISS: Single, simple assignment per specification
        self.current_thread.borrow_mut().top = base + a + nres;
        
        Ok(result)
    }
    
    /// SPECIFICATION COMPLIANT: OP_RETURN with correct upvalue closing order
    fn op_return(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
        let a = inst.get_a() as usize;
        let b = inst.get_b();

        eprintln!("DEBUG RETURN: Executing return with A={}, B={}, base={}", a, b, base);

        // SPECIFICATION COMPLIANCE: Close upvalues from frame base (Lua 5.1 requirement)
        self.heap.close_upvalues(&self.current_thread, base)?;

        let mut values = Vec::new();
        if b == 0 {
            let stack_size = self.current_top();
            let return_start = base + a;
            eprintln!("DEBUG RETURN: Collecting all values from R({}) to top ({})", return_start, stack_size);
            
            if stack_size > return_start {
                for i in return_start..stack_size {
                    let val = self.get_register(i)?;
                    eprintln!("DEBUG RETURN: Collecting R({}) = {:?} (type: {})", i, val, val.type_name());
                    values.push(val);
                }
            }
        } else {
            let num_returns = (b - 1) as usize;
            eprintln!("DEBUG RETURN: Collecting {} specific values from R({})", num_returns, base + a);
            
            for i in 0..num_returns {
                if let Ok(value) = self.get_register(base + a + i) {
                    eprintln!("DEBUG RETURN: Collecting R({}) = {:?} (type: {})", base + a + i, value, value.type_name());
                    values.push(value);
                } else {
                    values.push(Value::Nil);
                }
            }
        }

        eprintln!("DEBUG RETURN: Total return values: {}", values.len());

        // Process return with zero tolerance approach
        self.process_return(values)
    }

    /// SPECIFICATION ALIGNED: TFORLOOP - Complete iterator protocol per Lua 5.1 specification
    fn op_tforloop(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
        let a = inst.get_a() as usize;
        let c = inst.get_c() as usize;
        
        // SPECIFICATION: Call iterator function with state and control as arguments
        let iter_func = self.get_register(base + a)?;
        let state = self.get_register(base + a + 1)?;
        let control = self.get_register(base + a + 2)?;
        
        let args = vec![state, control];
        let result_base = base + a + 3;
        
        // Execute the iterator function call
        let call_step = self.execute_function_call(iter_func.clone(), args, c as i32, result_base, false, None)?;

        // If the iterator was a Lua function, we must yield for the call to complete
        if let Value::Closure(_) = iter_func {
            return Ok(call_step);
        }

        // For C-function iterators (like next, ipairs_iter), the call is synchronous
        // and results are immediately available for testing

        // SPECIFICATION CRITICAL: Check if first result (R(A+3)) is nil
        let first_result = self.get_register(result_base)?;
        
        if !first_result.is_nil() {
            // SUCCESS: Iterator returned a value
            // Update control variable R(A+2) = R(A+3)
            self.set_register(base + a + 2, first_result)?;
            
            // CORRECT BEHAVIOR: Fall through to execute loop body
            // The next instruction will be the loop body, then a JMP back to here
        } else {
            // FAILURE: Iterator returned nil, loop must terminate
            // Skip the following JMP instruction to avoid back-jumping
            self.heap.increment_pc(&self.current_thread)?;
        }
        
        Ok(StepResult::Continue)
    }
    
    /// EQ with StepResult propagation for metamethod handling
    fn op_eq(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
        let a = inst.get_a();
        let left = self.read_rk(base, inst.get_b())?;
        let right = self.read_rk(base, inst.get_c())?;

        if left == right {
            let result = true;
            if result != (a != 0) {
                self.heap.increment_pc(&self.current_thread)?;
            }
            return Ok(StepResult::Continue);
        }

        if !matches!(left, Value::Table(_)) || !matches!(right, Value::Table(_)) {
            let result = false;
            if result != (a != 0) {
                self.heap.increment_pc(&self.current_thread)?;
            }
            return Ok(StepResult::Continue);
        }

        let mt1 = self.get_metatable(&left)?;
        let mt2 = self.get_metatable(&right)?;

        if let (Some(mt1_handle), Some(mt2_handle)) = (mt1, mt2) {
            let eq_key = Value::String(Rc::clone(&self.heap.metamethod_names.eq));
            let mm1 = self.heap.get_table_field(&mt1_handle, &eq_key)?;
            
            if !mm1.is_nil() {
                let mm2 = self.heap.get_table_field(&mt2_handle, &eq_key)?;
                if mm1 == mm2 {
                    let temp = self.heap.get_stack_size(&self.current_thread);
                    self.ensure_stack_space(temp + 1)?;

                    self.execute_function_call(mm1, vec![left, right], 1, temp, false, None)?;
                    
                    let mm_result = self.get_register(temp)?;
                    let result = !mm_result.is_falsey();
                    
                    let mut thread = self.current_thread.borrow_mut();
                    thread.stack.truncate(temp);
                    drop(thread);
                    
                    if result != (a != 0) {
                        self.heap.increment_pc(&self.current_thread)?;
                    }
                    return Ok(StepResult::Continue);
                }
            }
        }

        let result = false;
        if result != (a != 0) {
            self.heap.increment_pc(&self.current_thread)?;
        }
        Ok(StepResult::Continue)
    }
    
    /// LT with StepResult propagation for metamethod handling
    fn op_lt(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
        let a = inst.get_a();
        let left = self.read_rk(base, inst.get_b())?;
        let right = self.read_rk(base, inst.get_c())?;

        let result = match (&left, &right) {
            (Value::Number(l), Value::Number(r)) => *l < *r,
            (Value::String(l), Value::String(r)) => {
                l.borrow().bytes < r.borrow().bytes
            }
            _ => {
                let mm = match self.find_metamethod(&left, &Value::String(Rc::clone(&self.heap.metamethod_names.lt)))? {
                    Some(m) => Some(m),
                    None => self.find_metamethod(&right, &Value::String(Rc::clone(&self.heap.metamethod_names.lt)))?,
                };
                if let Some(method) = mm {
                    let temp = self.heap.get_stack_size(&self.current_thread);
                    self.ensure_stack_space(temp + 1)?;

                    self.execute_function_call(method, vec![left, right], 1, temp, false, None)?;
                    
                    let mm_result = self.get_register(temp)?;
                    let result = !mm_result.is_falsey();
                    
                    let mut thread = self.current_thread.borrow_mut();
                    thread.stack.truncate(temp);
                    drop(thread);
                    
                    if result != (a != 0) {
                        self.heap.increment_pc(&self.current_thread)?;
                    }
                    return Ok(StepResult::Continue);
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
        Ok(StepResult::Continue)
    }
    
    /// LE with StepResult propagation for metamethod handling  
    fn op_le(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
        let a = inst.get_a();
        let left = self.read_rk(base, inst.get_b())?;
        let right = self.read_rk(base, inst.get_c())?;

        let result = match (&left, &right) {
            (Value::Number(l), Value::Number(r)) => *l <= *r,
            (Value::String(l), Value::String(r)) => {
                l.borrow().bytes <= r.borrow().bytes
            }
            _ => {
                let le_key = Value::String(Rc::clone(&self.heap.metamethod_names.le));
                let mm_le = match self.find_metamethod(&left, &le_key)? {
                    Some(m) => Some(m),
                    None => self.find_metamethod(&right, &le_key)?,
                };

                if let Some(method) = mm_le {
                    let temp = self.heap.get_stack_size(&self.current_thread);
                    self.ensure_stack_space(temp + 1)?;

                    self.execute_function_call(method, vec![left, right], 1, temp, false, None)?;
                    
                    let mm_result = self.get_register(temp)?;
                    let result = !mm_result.is_falsey();
                    
                    let mut thread = self.current_thread.borrow_mut();
                    thread.stack.truncate(temp);
                    drop(thread);
                    
                    if result != (a != 0) {
                        self.heap.increment_pc(&self.current_thread)?;
                    }
                    return Ok(StepResult::Continue);
                }

                let lt_key = Value::String(Rc::clone(&self.heap.metamethod_names.lt));
                let mm_lt = match self.find_metamethod(&left, &lt_key)? {
                    Some(m) => Some(m),
                    None => self.find_metamethod(&right, &lt_key)?,
                };

                if let Some(method) = mm_lt {
                    let temp = self.heap.get_stack_size(&self.current_thread);
                    self.ensure_stack_space(temp + 1)?;

                    self.execute_function_call(method, vec![right, left], 1, temp, false, None)?;
                    
                    let mm_result = self.get_register(temp)?;
                    let result = mm_result.is_falsey();
                    
                    let mut thread = self.current_thread.borrow_mut();
                    thread.stack.truncate(temp);
                    drop(thread);
                    
                    if result != (a != 0) {
                        self.heap.increment_pc(&self.current_thread)?;
                    }
                    return Ok(StepResult::Continue);
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
        Ok(StepResult::Continue)
    }
    
    /// FOR loop implementations
    
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
        
        let proto_value = {
            let closure_ref = frame.closure.borrow();
            let constants_len = closure_ref.proto.constants.len();
            
            if bx >= constants_len {
                return Err(LuaError::RuntimeError(
                    format!("Constant index {} out of bounds (constants size: {})", 
                           bx, constants_len)
                ));
            } else {
                match &closure_ref.proto.constants[bx] {
                    Value::FunctionProto(handle) => Rc::clone(handle),
                    _ => {
                        return Err(LuaError::RuntimeError(
                            format!("Constant {} is not a function prototype", bx)
                        ));
                    }
                }
            }
        };
        
        let num_upvalues = proto_value.upvalues.len();
        let mut upvalues = Vec::with_capacity(num_upvalues);
        
        // SPECIFICATION COMPLIANCE: Process exactly num_upvalues pseudo-instructions
        for _i in 0..num_upvalues {
            let pseudo_inst = {
                let pc = self.heap.get_pc(&self.current_thread)?;
                let pseudo_instruction = self.get_instruction(&frame.closure, pc)?;
                self.heap.set_pc(&self.current_thread, pc + 1)?;
                Instruction(pseudo_instruction)
            };
            
            let upvalue = match pseudo_inst.get_opcode() {
                OpCode::Move => {
                    // SPECIFICATION ALIGNED: Use parent function's base for correct index calculation
                    let parent_base = frame.base_register as usize;
                    let local_idx = parent_base + pseudo_inst.get_b() as usize;
                    self.heap.find_or_create_upvalue(&self.current_thread, local_idx)?
                },
                OpCode::GetUpval => {
                    let parent_upval_idx = pseudo_inst.get_b() as usize;
                    let current_frame = self.heap.get_current_frame(&self.current_thread)?;
                    let parent_closure_ref = current_frame.closure.borrow();
                    if parent_upval_idx >= parent_closure_ref.upvalues.len() {
                        return Err(LuaError::RuntimeError(
                            format!("Parent upvalue index {} out of bounds", parent_upval_idx)
                        ));
                    }
                    Rc::clone(&parent_closure_ref.upvalues[parent_upval_idx])
                },
                _ => {
                    return Err(LuaError::RuntimeError(
                        format!("Invalid pseudo-instruction opcode: {:?}", pseudo_inst.get_opcode())
                    ));
                }
            };
            
            upvalues.push(upvalue);
        }
        
        // Inherit environment per Lua 5.1 specification
        let current_env = {
            let current_closure_ref = frame.closure.borrow();
            Rc::clone(&current_closure_ref.env)
        };
        
        let new_closure = self.heap.create_closure(proto_value, upvalues, current_env);
        self.set_register(base + a, Value::Closure(new_closure))?;
        
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
    
    /// SETLIST
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
        
        // Handle C==0 case properly (auxiliary instruction)
        if c == 0 {
            let pc = self.heap.get_pc(&self.current_thread)?;
            let frame = self.heap.get_current_frame(&self.current_thread)?;
            let full_instruction = self.get_instruction(&frame.closure, pc)?;
            c = (full_instruction >> 6) as usize; // Extract Ax field (bits 6-31)
            self.heap.increment_pc(&self.current_thread)?;
        }
        
        // Handle B==0 case AFTER C extraction with special rules
        let count = if b == 0 {
            let top = self.current_top();
            let start = base + a + 1;
            if top > start {
                b = top - start;
                // Special rule when both B==0 and C==0 (last batch flush)
                if c == 0 {
                    c = 1;
                }
                b
            } else {
                0
            }
        } else {
            b
        };
        
        let array_base = if c > 0 { (c - 1) * FIELDS_PER_FLUSH + 1 } else { 1 };
        
        // Set array elements properly
        for i in 0..count {
            let value = self.get_register(base + a + 1 + i)?;
            let index = array_base + i;
            let key = Value::Number(index as f64);
            
            // Use raw table field setting for proper array construction
            self.heap.set_table_field_raw(&table_handle, &key, &value)?;
        }
        
        Ok(())
    }
    
    fn op_tailcall(&mut self, inst: Instruction, base: usize) -> LuaResult<StepResult> {
        let a = inst.get_a() as usize;
        let b = inst.get_b();
        let c = inst.get_c();
        
        let func = self.get_register(base + a)?;
        
        let arg_count = if b == 0 {
            self.current_top().saturating_sub(base + a + 1)
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
        
        let current_frame = self.heap.get_current_frame(&self.current_thread)?;
        let caller_result_base = current_frame.result_base;
        let caller_expected_results = current_frame.expected_results;
        let caller_is_protected = current_frame.is_protected;
        let caller_xpcall_handler = current_frame.xpcall_handler.clone();
        
        self.heap.close_upvalues(&self.current_thread, base)?;
        
        self.heap.pop_call_frame(&self.current_thread)?;
        
        let final_expected_results = if expected_results >= 0 {
            expected_results
        } else if let Some(caller_expected) = caller_expected_results {
            caller_expected as i32
        } else {
            -1
        };
        
        self.execute_function_call(
            func, 
            args, 
            final_expected_results, 
            caller_result_base, 
            caller_is_protected, 
            caller_xpcall_handler
        )
    }
    
    /// VARARG implementation
    fn op_vararg(&self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b();
        
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        
        let declared_nargs = {
            let closure_ref = frame.closure.borrow();
            let num_params = closure_ref.proto.num_params as usize;
            drop(closure_ref);
            
            // Use varargs slice if available, otherwise calculate from frame context
            match &frame.varargs {
                Some(vararg_slice) => vararg_slice.len(),
                None => {
                    let callee_base = frame.base_register as usize;
                    let current_top = self.current_top();
                    if current_top > callee_base + num_params {
                        current_top - callee_base - num_params
                    } else {
                        0
                    }
                }
            }
        };
        
        let copy_count = if b == 0 {
            declared_nargs
        } else {
            (b - 1) as usize
        };
        
        // Copy varargs with proper bounds checking
        for i in 0..copy_count {
            let val = if let Some(vararg_slice) = &frame.varargs {
                vararg_slice.get(i).cloned().unwrap_or(Value::Nil)
            } else {
                // Fallback: read from calculated positions
                let vararg_start = {
                    let closure_ref = frame.closure.borrow();
                    frame.base_register as usize + closure_ref.proto.num_params as usize
                };
                self.get_register(vararg_start + i).unwrap_or(Value::Nil)
            };
            self.set_register(base + a + i, val)?;
        }
        
        // Only adjust top when B == 0 (specification requirement)
        if b == 0 {
            self.current_thread.borrow_mut().top = base + a + declared_nargs;
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
        // DIRECT table setting for C functions (NO QUEUE!)
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
            // Lua 5.1 spec: nil key starts iteration from beginning
            // Try array part first
            for i in 0..table_ref.array.len() {
                if !table_ref.array[i].is_nil() {
                    return Ok(Some((Value::Number((i + 1) as f64), table_ref.array[i].clone())));
                }
            }
            // Then try hash part
            if let Some((k, v)) = table_ref.map.iter().next() {
                return Ok(Some((k.to_value(), v.clone())));
            }
            return Ok(None);
        }

        let mut in_array = false;
        if let Value::Number(n) = key {
            if n.fract() == 0.0 && *n >= 1.0 {
                let index = *n as usize;
                if index > 0 && index <= table_ref.array.len() {
                    in_array = true;
                    // Continue from this array position
                    for i in index..table_ref.array.len() {
                        if !table_ref.array[i].is_nil() {
                            return Ok(Some((Value::Number((i + 1) as f64), table_ref.array[i].clone())));
                        }
                    }
                    // Transition to hash part after array
                    if let Some((k, v)) = table_ref.map.iter().next() {
                        return Ok(Some((k.to_value(), v.clone())));
                    }
                    return Ok(None);
                }
            }
        }

        if !in_array {
            let current_key_hashable = match HashableValue::from_value(key) {
                Ok(h) => h,
                Err(_) => return Ok(None),
            };
            
            let mut found_current = false;
            for (hash_key, hash_value) in table_ref.map.iter() {
                if found_current {
                    // Return the next key-value pair after the current one
                    return Ok(Some((hash_key.to_value(), hash_value.clone())));
                }
                
                if hash_key == &current_key_hashable {
                    found_current = true;
                    // Continue to find the next entry
                }
            }
            
            // If current key was the last or not found, end iteration
            return Ok(None);
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
        // Simplified pcall - NO QUEUE DEPENDENCIES!
        Ok(())
    }
    
    fn xpcall(&mut self, _func: Value, _err_handler: Value) -> LuaResult<()> {
        // Simplified xpcall - NO QUEUE DEPENDENCIES!
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
        Ok(())
    }
}