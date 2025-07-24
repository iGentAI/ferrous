//! Rc<RefCell> Based Lua VM
//!
//! This module implements a Lua VM using fine-grained Rc<RefCell> objects
//! instead of a global RefCell, providing proper shared mutable state semantics
//! and resolving borrow checker issues.

use std::rc::Rc;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::cmp::Ordering;

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

/// Helper function to compare two Values for sorting keys
fn cmp_value(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Number(na), Value::Number(nb)) => na.partial_cmp(nb).unwrap_or(Ordering::Equal),
        (Value::String(sa), Value::String(sb)) => {
            sa.borrow().bytes.cmp(&sb.borrow().bytes)
        },
        (Value::Number(_), Value::String(_)) => Ordering::Less,
        (Value::String(_), Value::Number(_)) => Ordering::Greater,
        _ => Ordering::Equal, // Fallback for other types
    }
}

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
    fn set_table_field(&mut self, table: &TableHandle, key: Value, value: Value) -> LuaResult<()>;
    
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
    
    /// Get the call base register
    fn get_call_base(&self) -> usize;
    
    /// Queue a protected call (pcall)
    fn pcall(&mut self, func: Value, args: Vec<Value>) -> LuaResult<()>;
    
    /// Queue a protected call with error handler (xpcall)
    fn xpcall(&mut self, func: Value, err_handler: Value) -> LuaResult<()>;
    
    /// Get an upvalue's value safely (for getfenv implementation)
    fn get_upvalue_value(&self, upvalue: &UpvalueHandle) -> LuaResult<Value>;
    
    /// Set an upvalue's value safely (for setfenv implementation)
    fn set_upvalue_value(&self, upvalue: &UpvalueHandle, value: Value) -> LuaResult<()>;
    
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
        /// Is this a protected call?
        is_protected: bool,
        /// Optional error handler for xpcall
        xpcall_handler: Option<Value>,
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
        /// sBx field for jump calculation
        sbx: i32,
    },
    
    /// Comparison continuation for metamethods
    ComparisonContinuation {
        /// Comparison operation type
        comp_op: CompOp,
        /// A field from comparison instruction
        a: u32,
        /// Temporary register for result
        temp: usize,
        /// PC to continue from
        pc: usize,
        /// Whether to negate the result
        negate: bool,
    },
    
    /// Concatenation continuation for multi-value cases
    ConcatContinuation {
        /// Base register
        base: usize,
        /// Start index
        start: usize,
        /// End index
        end: usize,
        /// Current index in concatenation
        current: usize,
        /// Temporary register
        temp: usize,
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
    /// Reference to the VM (immutable to avoid borrow conflicts)
    vm: &'a RcVM,
    
    /// Arguments base index
    base: usize,
    
    /// Number of arguments
    nargs: usize,
    
    /// Results pushed so far
    results_pushed: usize,
    
    /// Results vector to prevent stack overwrite
    results: Vec<Value>,
}

/// Rc<RefCell> based Lua VM
pub struct RcVM {
    /// The Lua heap
    pub heap: RcHeap,
    
    /// Operation queue for non-recursive execution (wrapped in RefCell for interior mutability)
    operation_queue: RefCell<VecDeque<PendingOperation>>,
    
    /// Current thread
    current_thread: ThreadHandle,
    
    /// VM configuration
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
                    self.heap.set_table_field(&table, &key, &value)?;
                }
                Ok(Value::Table(table))
            },
        }
    }

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
            operation_queue: RefCell::new(VecDeque::new()),
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
        self.operation_queue.borrow_mut().clear();
        
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
        self.operation_queue.borrow_mut().push_back(PendingOperation::FunctionCall {
            func: Value::Closure(main_closure.clone()),
            args: args.to_vec(),
            expected_results: 1, // Expect one result from main chunk
            result_base: 0,
            is_protected: false,
            xpcall_handler: None,
        });
        
        // Execute until completion
        self.run_to_completion()
    }
    
    /// Run VM until completion
    fn run_to_completion(&mut self) -> LuaResult<Value> {
        loop {
            // Process pending operations first
            let op_result = if !self.operation_queue.borrow().is_empty() {
                let op = self.operation_queue.borrow_mut().pop_front().unwrap();
                self.process_operation(op)
            } else {
                // Execute next instruction
                self.step()
            };

            match op_result {
                Ok(StepResult::Continue) => continue,
                Ok(StepResult::Completed(values)) => {
                    return Ok(values.first().cloned().unwrap_or(Value::Nil));
                },
                Err(e) => {
                    // Error occurred, try to handle it
                    match self.handle_error(e) {
                        Ok(StepResult::Continue) => continue, // Error was handled, continue execution
                        Ok(StepResult::Completed(values)) => {
                            // Should not happen from handle_error, but handle gracefully
                            return Ok(values.first().cloned().unwrap_or(Value::Nil));
                        },
                        Err(unhandled_error) => {
                            // Error was not handled by a protected call, propagate it
                            return Err(unhandled_error);
                        }
                    }
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
                let c = inst.get_c();
                if inst.get_opcode() == OpCode::TForLoop {
                    let opcode_name = if c > 0 { "TFORCALL" } else { "TFORLOOP" };
                    eprintln!("DEBUG RcVM: Executing {} at PC={}, base={}", 
                             opcode_name, pc, base);
                } else {
                    eprintln!("DEBUG RcVM: Executing {:?} at PC={}, base={}", 
                             inst.get_opcode(), pc, base);
                }
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
            OpCode::TForCall => self.op_tforcall(inst, base)?,
            OpCode::TForLoop => {
                let a = inst.get_a() as usize;
                let sbx = inst.get_sbx();
                self.operation_queue.borrow_mut().push_back(PendingOperation::TForLoopContinuation { base, a, sbx });
            },
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
            PendingOperation::FunctionCall { func, args, expected_results, result_base, is_protected, xpcall_handler } => {
                self.execute_function_call(func, args, expected_results, result_base, is_protected, xpcall_handler)
            },
            PendingOperation::Return { values } => {
                self.process_return(values)
            },
            PendingOperation::TForLoopContinuation { base, a, sbx } => {
                eprintln!("DEBUG TFORLOOP Continuation:");
                eprintln!("  Base: {}, A: {}, sBx: {}", base, a, sbx);

                // SYSTEMATIC FIX: The preceding iterator call (from TFORCALL) might have
                // returned 0 results, causing the stack to be truncated. We must check
                // if the target result register R(A+3) is even valid before reading it.
                // If it's not, it's equivalent to the iterator returning nil, so the
                // loop must terminate.
                let first_var_reg = base + a + 3;
                let stack_size = self.heap.get_stack_size(&self.current_thread);
                if first_var_reg >= stack_size {
                    eprintln!(
                        "  TFORLOOP: End of iteration (stack truncated before result register at index {})",
                        first_var_reg
                    );
                    return Ok(StepResult::Continue);
                }

                // The first variable is at R(A+3) and access is now safe.
                let first_result = self.get_register(first_var_reg)?;
                
                eprintln!("  Reading first result from register: {}", first_var_reg);
                
                if first_result.is_nil() {
                    eprintln!("  TFORLOOP: End of iteration (first result is nil)");
                    // Loop is finished, just continue to the next instruction.
                    // The PC is already pointing to the instruction after TFORLOOP.
                    Ok(StepResult::Continue)
                } else {
                    eprintln!("  TFORLOOP: Continuing iteration, copying first result to control variable");
                    // The control variable is R(A+2)
                    let control_var_reg = base + a + 2;
                    self.set_register(control_var_reg, first_result.clone())?;
                    
                    // The loop body is jumped to via the sBx offset.
                    // The TFORLOOP instruction is followed by a JMP. The sBx in our
                    // custom TFORLOOP opcode is the jump offset.
                    let pc = self.heap.get_pc(&self.current_thread)?;
                    let new_pc = (pc as isize + sbx as isize) as usize;
                    eprintln!("  TFORLOOP: Jumping back from PC {} to PC {}", pc, new_pc);
                    self.heap.set_pc(&self.current_thread, new_pc)?;
                    Ok(StepResult::Continue)
                }
            },
            PendingOperation::ComparisonContinuation { comp_op, a, temp, pc, negate } => {
                let result_val = self.get_register(temp)?;
                let mut result = !result_val.is_falsey();
                
                if negate {
                    result = !result;
                }
                
                let skip = match comp_op {
                    CompOp::Eq => result != (a != 0),
                    CompOp::Lt | CompOp::Le => result != (a != 0),
                };
                
                if skip {
                    self.heap.set_pc(&self.current_thread, pc + 1)?;
                } else {
                    self.heap.set_pc(&self.current_thread, pc)?;
                }
                
                // Trim the temp register
                let mut thread = self.current_thread.borrow_mut();
                thread.stack.truncate(temp);
                
                Ok(StepResult::Continue)
            },
            PendingOperation::ConcatContinuation { base, start, end, current, temp } => {
                // This continuation is activated after a pairwise concatenation (often a metamethod call)
                // has completed. The accumulated result is in the `temp` register. The `current` field
                // indicates the index of the right-hand operand of the operation that just finished.
                // We now need to continue concatenating with the values from `current + 1` to `end`.

                // We can loop here, processing all subsequent concatenations until we either finish
                // or need to invoke another metamethod.
                for i in (current + 1)..=end {
                    let left = self.get_register(temp)?;
                    let right = self.get_register(base + i)?;

                    // Attempt direct concatenation if both operands are strings or numbers.
                    let both_primitive = (matches!(left, Value::String(_)) || matches!(left, Value::Number(_))) &&
                                           (matches!(right, Value::String(_)) || matches!(right, Value::Number(_)));

                    if both_primitive {
                        let mut s = String::new();
                        match left {
                            Value::String(h) => s.push_str(h.borrow().to_str().unwrap_or("")),
                            Value::Number(n) => s.push_str(&n.to_string()),
                            _ => unreachable!(), // Checked by both_primitive
                        }
                        match right {
                            Value::String(h) => s.push_str(h.borrow().to_str().unwrap_or("")),
                            Value::Number(n) => s.push_str(&n.to_string()),
                            _ => unreachable!(), // Checked by both_primitive
                        }
                        let result_handle = self.heap.create_string(&s)?;
                        self.set_register(temp, Value::String(result_handle))?;
                        // Continue the loop to the next operand.
                    } else {
                        // One or both operands are not primitive, so we must try to find a metamethod.
                        let concat_mm_key = Value::String(Rc::clone(&self.heap.metamethod_names.concat));
                        let mm = match self.find_metamethod(&left, &concat_mm_key)? {
                            Some(method) => Some(method),
                            None => self.find_metamethod(&right, &concat_mm_key)?,
                        };

                        if let Some(method) = mm {
                            // A metamethod was found. We must stop this loop and queue two new operations:
                            // 1. The call to the metamethod itself.
                            // 2. A new continuation to pick up the process after the metamethod returns.
                            self.operation_queue.borrow_mut().push_back(PendingOperation::MetamethodCall {
                                method,
                                args: vec![left, right],
                                expected_results: 1,
                                result_base: temp, // The result will be stored back in our accumulator register.
                            });

                            // This new continuation will be processed *after* the metamethod call.
                            // Its `current` index will be `i`, because that's the operand we are now processing.
                            self.operation_queue.borrow_mut().push_back(PendingOperation::ConcatContinuation {
                                base,
                                start,
                                end,
                                current: i,
                                temp,
                            });

                            // Yield control back to the VM's main loop to process the metamethod call.
                            return Ok(StepResult::Continue);
                        } else {
                            // No metamethod found, which is a runtime error.
                            return Err(LuaError::TypeError {
                                expected: "string or number".to_string(),
                                got: format!("'{}' and '{}'", left.type_name(), right.type_name()),
                            });
                        }
                    }
                }

                // If the loop completes without breaking, the entire concatenation is finished.
                // The final result is in R(A) (which is `temp`), so we just continue execution.
                Ok(StepResult::Continue)
            },
            PendingOperation::MetamethodCall { method, args, expected_results, result_base } => {
                self.execute_function_call(method, args, expected_results, result_base, false, None)
            }
        }
    }
    
    /// Load a compiled module
    fn load_module(&mut self, module: &compiler::CompiledModule) -> LuaResult<ClosureHandle> {
        eprintln!("DEBUG load_module: Loading module with {} constants, {} strings, {} prototypes",
                 module.constants.len(), module.strings.len(), module.prototypes.len());

        // Step 1: Create string handles for the entire module.
        let mut string_handles = Vec::with_capacity(module.strings.len());
        for s in &module.strings {
            string_handles.push(self.heap.create_string(s)?);
        }

        // Step 2: Create placeholder prototype handles.
        // These are needed to break potential circular dependencies between functions.
        let mut placeholder_handles = Vec::with_capacity(module.prototypes.len());
        for proto in &module.prototypes {
            let placeholder_proto = FunctionProto {
                bytecode: proto.bytecode.clone(),
                constants: vec![], // Empty, will be populated later
                num_params: proto.num_params,
                is_vararg: proto.is_vararg,
                max_stack_size: proto.max_stack_size,
                upvalues: proto.upvalues.iter().map(|u| super::rc_value::UpvalueInfo { in_stack: u.in_stack, index: u.index }).collect(),
            };
            placeholder_handles.push(self.heap.create_function_proto(placeholder_proto));
        }

        // Step 3: Create intermediate prototypes.
        // These will have their constant tables populated, but any `FunctionProto` constants
        // inside them will point to the placeholders.
        let mut intermediate_handles = Vec::with_capacity(module.prototypes.len());
        for (i, proto) in module.prototypes.iter().enumerate() {
            let mut constants = Vec::with_capacity(proto.constants.len());
            for constant in &proto.constants {
                // Resolve constants using the placeholder handles.
                let value = self.create_value_from_constant(constant, &string_handles, &placeholder_handles)?;
                constants.push(value);
            }
            let intermediate_proto = FunctionProto {
                bytecode: proto.bytecode.clone(),
                constants, // Contains stale handles to placeholders
                num_params: proto.num_params,
                is_vararg: proto.is_vararg,
                max_stack_size: proto.max_stack_size,
                upvalues: module.prototypes[i].upvalues.iter().map(|u| super::rc_value::UpvalueInfo { in_stack: u.in_stack, index: u.index }).collect(),
            };
            intermediate_handles.push(self.heap.create_function_proto(intermediate_proto));
        }

        // Step 4: Fix-up pass. Create the final, correct prototypes.
        // We re-resolve the constants for each prototype, but this time, we use the
        // `intermediate_handles`. This ensures that any `FunctionProto` constant
        // points to a handle from the final set, guaranteeing handle consistency.
        let mut proto_handles = Vec::with_capacity(module.prototypes.len());
        for (i, proto) in module.prototypes.iter().enumerate() {
            let mut final_constants = Vec::with_capacity(proto.constants.len());
            for constant in &proto.constants {
                // Resolve constants using the intermediate handles, which are the final ones.
                let value = self.create_value_from_constant(constant, &string_handles, &intermediate_handles)?;
                final_constants.push(value);
            }
            let final_proto = FunctionProto {
                bytecode: proto.bytecode.clone(),
                constants: final_constants, // Correct, final constants
                num_params: proto.num_params,
                is_vararg: proto.is_vararg,
                max_stack_size: proto.max_stack_size,
                upvalues: module.prototypes[i].upvalues.iter().map(|u| super::rc_value::UpvalueInfo { in_stack: u.in_stack, index: u.index }).collect(),
            };
            proto_handles.push(self.heap.create_function_proto(final_proto));
        }

        eprintln!("DEBUG load_module: Completed prototype loading and fix-up.");
        
        // Step 5: Create main function constants using the now-correct prototype handles.
        let mut main_constants = Vec::with_capacity(module.constants.len());
        eprintln!("DEBUG load_module: Creating main function constants from {} constants",
                 module.constants.len());
        for (const_idx, constant) in module.constants.iter().enumerate() {
            eprintln!("DEBUG load_module: Processing main constant {}: {:?}", const_idx, constant);
            let value = self.create_value_from_constant(constant, &string_handles, &proto_handles)?;
            eprintln!("DEBUG load_module: Resolved main constant {} to: {:?}", const_idx, value);
            main_constants.push(value);
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
        
        // Initialize the main chunk's global environment (_ENV).
        // The main chunk always has exactly one upvalue, _ENV, which is a closed upvalue containing
        // the globals table. This provides the foundation for reliable global variable resolution.
        let mut main_upvalues = Vec::new();
        eprintln!("DEBUG load_module: Systematically creating single _ENV upvalue for main chunk's global access");
        
        let globals_value = Value::Table(self.heap.globals());
        let globals_upvalue = Rc::new(RefCell::new(UpvalueState::Closed {
            value: globals_value,
        }));
        main_upvalues.push(globals_upvalue);
        
        // Create main closure with proper upvalues including _ENV
        let main_closure = self.heap.create_closure(Rc::clone(&main_proto_handle), main_upvalues);
        
        Ok(main_closure)
    }
    
    /// Execute a function call
    fn execute_function_call(
        &mut self,
        func: Value,
        args: Vec<Value>,
        expected_results: i32,
        result_base: usize,
        is_protected: bool,
        xpcall_handler: Option<Value>,
    ) -> LuaResult<StepResult> {
        // ARCHITECTURAL FIX: Ensure all function calls are executed regardless of the number
        // of expected results. This unified execution path is critical for Lua 5.1
        // specification compliance, guaranteeing that functions are always run for their
        // side-effects. This resolves a class of bugs related to incorrect call-skipping.
        match &func {
            &Value::Closure(ref closure_handle) => {
                self.call_lua_function(Rc::clone(closure_handle), args, expected_results, result_base, is_protected, xpcall_handler)
            },
            &Value::CFunction(cfunc) => {
                // C functions also must always be called for side-effects.
                self.call_c_function(cfunc, args, expected_results, result_base)
            },
            &Value::Table(ref table_handle) => {
                // Check for __call metamethod. The logic here is inherently recursive,
                // as the metamethod itself needs to be executed.
                let table_ref = table_handle.borrow();
                if let Some(metatable) = &table_ref.metatable {
                    let metatable_clone = Rc::clone(metatable);
                    drop(table_ref);
                    
                    let mt_ref = metatable_clone.borrow();
                    let call_key = Value::String(Rc::clone(&self.heap.metamethod_names.call));
                    if let Some(metamethod) = mt_ref.get_field(&call_key) {
                        drop(mt_ref);
                        
                        // Call the metamethod with the table as first argument.
                        // This recursive call ensures the metamethod is also always executed.
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
        is_protected: bool,
        xpcall_handler: Option<Value>,
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
        let new_base = result_base; // Lua 5.1: base points to the function slot
        let frame_base = new_base + 1; // R(0) is at new_base + 1
        
        // The required size is `frame_base + max_stack`.
        let required_stack_size = frame_base + max_stack;
        
        // Ensure stack space
        self.ensure_stack_space(required_stack_size)?;
        
        // Place arguments on stack
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
        
        // Create new call frame with protection info
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
        let args_base = result_base;
        
        // Ensure stack space for function + args
        self.ensure_stack_space(args_base + 1 + args.len())?;

        // Place function and arguments
        self.set_register(args_base, Value::CFunction(func))?;
        for (i, arg) in args.iter().enumerate() {
            self.set_register(args_base + 1 + i, arg.clone())?;
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
            base: (base + 1) as usize,
            nargs,
            results_pushed: 0,
            results: Vec::new(),
        };
        
        // Call the C function with our context
        let actual_results = function(&mut ctx)?;
        
        // Handle special return values for queued operations (pcall/xpcall)
        if actual_results == -1 {
            // Special case: C function queued its own operations (pcall/xpcall)
            // Drop ctx to release borrows and continue execution
            drop(ctx);
            return Ok(StepResult::Continue);
        }
        
        // Validate result count
        if actual_results < 0 {
            return Err(LuaError::RuntimeError(
                "C function returned invalid result count".to_string()
            ));
        }
        
        eprintln!("DEBUG process_c_function_call: C function returned {} results", actual_results);
        
        // Extract results and drop ctx to end borrow
        let mut results = std::mem::take(&mut ctx.results);
        let pushed = results.len();
        drop(ctx);
        
        eprintln!("DEBUG process_c_function_call: Context reports {} results pushed", pushed);
        
        // Adjust results to the returned count
        let mut results_pushed = actual_results as usize;
        if results_pushed > pushed {
            results.resize(results_pushed, Value::Nil);
        } else if results_pushed < pushed {
            results.truncate(results_pushed);
        }
        
        // Adjust results to expected count if specified
        let base_usize = base as usize;
        if expected_results >= 0 {
            let expected = expected_results as usize;
            eprintln!("DEBUG process_c_function_call: Adjusting result count to expected {}", expected);
            
            if results_pushed < expected {
                // Fill missing results with nil
                let filled_count = expected - results_pushed;
                results.resize(expected, Value::Nil);
                results_pushed = expected;
                eprintln!("DEBUG process_c_function_call: Filled {} missing results with nil", filled_count);
            } else if results_pushed > expected {
                // Trim excess results
                let trimmed_count = results_pushed - expected;
                results.truncate(expected);
                results_pushed = expected;
                eprintln!("DEBUG process_c_function_call: Trimmed {} excess results", trimmed_count);
            }
        }
        
        // Copy final results to stack (overwriting function and args)
        for (i, val) in results.iter().enumerate() {
            self.set_register(base_usize + i, val.clone())?;
        }
        
        eprintln!("DEBUG process_c_function_call: Placed {} results at base {}, stack maintained for continued execution", results_pushed, base_usize);
        
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
        
        // Handle protected call success
        if frame.is_protected {
            let mut success_results = Vec::with_capacity(values.len() + 1);
            success_results.push(Value::Boolean(true));
            success_results.extend(values);
            
            self.place_return_values(success_results, frame.result_base, frame.expected_results)?;
        } else {
            self.place_return_values(values, frame.result_base, frame.expected_results)?;
        }
        
        // Check if this was the last frame
        if self.heap.get_call_depth(&self.current_thread) == 0 {
            // If the last frame was protected, we need to return the value from the stack
            if frame.is_protected {
                 let final_value = self.get_register(frame.result_base)?;
                 return Ok(StepResult::Completed(vec![final_value]));
            }
            // Otherwise, there are no results to complete with
            return Ok(StepResult::Completed(vec![]));
        }
        
        Ok(StepResult::Continue)
    }

    /// Helper to place return values on the stack, with specification-compliant logic.
    fn place_return_values(&mut self, values: Vec<Value>, result_base: usize, expected_results: Option<usize>) -> LuaResult<()> {
        // ARCHITECTURAL FIX: Check function identity preservation BEFORE placing any values
        // This resolves the contradictory logic where we claimed to preserve function identity
        // but actually overwrote the register with return values first
        
        // Determine the exact number of results to place based on the CALLER's expectations.
        // If expected_results is Some(n), we must place exactly n values.
        // If it is None (from C=0), we place all returned values (LUA_MULTRET).
        let result_count = if let Some(n) = expected_results {
            n
        } else {
            values.len()
        };

        // CRITICAL SPECIFICATION FIX: If result_count is 0, place ZERO values
        // This preserves function identity when CALL uses C=1 (expecting 0 results)
        //
        // However, the implementation previously *left the function object and its
        // arguments alive on the caller's stack*, corrupting later register reads.
        // Per Lua 5.1 the caller's stack must be rolled back so that the slot that
        // formerly contained the function is no longer visible.  Therefore we must
        // truncate the stack to `result_base`.
        if result_count == 0 {
            eprintln!("DEBUG place_return_values: caller expects 0 results – truncating stack to result_base ({})", result_base);

            // SAFETY: result_base is always within the current stack because it
            // pointed to the function slot for this very call.
            let mut thread = self.current_thread.borrow_mut();
            if thread.stack.len() > result_base {
                thread.stack.truncate(result_base);
            }
            return Ok(());
        }

        // Place the required number of results only when result_count > 0
        for i in 0..result_count {
            // Get the value to place, or nil if the function returned fewer values than expected.
            let value_to_set = values.get(i).cloned().unwrap_or(Value::Nil);
            self.set_register(result_base + i, value_to_set)?;
        }
        
        Ok(())
    }
    
    /// Handle a runtime error, unwinding the stack if necessary
    fn handle_error(&mut self, error: LuaError) -> LuaResult<StepResult> {
        let error_val = Value::String(self.heap.create_string(&error.to_string())?);

        loop {
            if self.heap.get_call_depth(&self.current_thread) == 0 {
                // No more frames, unhandled error
                return Err(error);
            }

            let frame = self.heap.pop_call_frame(&self.current_thread)?;

            if frame.is_protected {
                // Found a protected frame, handle the error.
                
                // 1. Clear operations from the failed call.
                self.operation_queue.borrow_mut().clear();

                // 2. Set up the common parts of the return value: `false` and trailing `nil`s.
                self.set_register(frame.result_base, Value::Boolean(false))?;
                if let Some(n) = frame.expected_results {
                    for i in 2..n {
                        self.set_register(frame.result_base + i, Value::Nil)?;
                    }
                }

                // 3. Either call the handler or place the error message directly.
                if let Some(handler) = frame.xpcall_handler {
                    // Call the handler. Its result will be placed at `result_base + 1` by the VM.
                    // This returns a StepResult that continues execution into the handler.
                    return self.execute_function_call(
                        handler,
                        vec![error_val],
                        1, // Expect one result
                        frame.result_base + 1, // Place it after `false`
                        false, // Errors in handler are not caught
                        None,
                    );
                } else {
                    // No handler, so place the error message at `result_base + 1`.
                    self.set_register(frame.result_base + 1, error_val)?;
                    return Ok(StepResult::Continue);
                }
            }
        }
    }
    
    //
    // Heap access helpers (moved from RcVMExt trait)
    //
    
    /// Create a string
    pub fn create_string(&self, s: &str) -> LuaResult<StringHandle> {
        self.heap.create_string(s)
    }
    
    /// Create a table
    pub fn create_table(&self) -> LuaResult<TableHandle> {
        Ok(self.heap.create_table())
    }
    
    /// Get table field
    pub fn get_table_field(&self, table: &TableHandle, key: &Value) -> LuaResult<Value> {
        self.heap.get_table_field(table, key)
    }
    
    /// Set table field
    pub fn set_table_field(&self, table: &TableHandle, key: &Value, value: &Value) -> LuaResult<()> {
        // Use the heap's comprehensive Two-Phase Commit architecture for all table operations
        self.heap.set_table_field(table, key, value).map(|_| ())
    }
    
    /// Get globals table
    pub fn globals(&self) -> LuaResult<TableHandle> {
        Ok(self.heap.globals())
    }
    
    //
    // Register access helpers
    //
    
    /// Get a register value
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
                    let current_frame = &thread_ref.call_frames[thread_ref.call_frames.len() - 1];
                    eprintln!("  Current frame base_register: {}", current_frame.base_register);
                    eprintln!("  Current frame PC: {}", current_frame.pc);
                }
                
                if index > 255 {
                    eprintln!("  POTENTIAL CAUSE: Index {} suggests base+offset calculation error", index);
                    eprintln!("  If base=1, offset calculation might be wrong");
                }
                
                Err(LuaError::RuntimeError(
                    format!("Register {} out of bounds (stack size: {}) - Root cause investigation needed", 
                           index, thread_ref.stack.len())
                ))
            }
        }
    }
    
    /// Set a register value
    fn set_register(&self, index: usize, value: Value) -> LuaResult<()> {
        self.heap.set_register(&self.current_thread, index, value)
    }
    
    /// Ensure sufficient stack space.
    fn ensure_stack_space(&self, size: usize) -> LuaResult<()> {
        // The total stack can grow much larger than 256. The limit of 255 registers
        // applies to the number of registers a single function can use (its frame size),
        // which is enforced by codegen's `max_stack_size: u8`.
        let current_size = self.heap.get_stack_size(&self.current_thread);
        if current_size < size {
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
        
        // Get globals table from _ENV upvalue (upvalue 0)
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
        
        // Get value from globals
        let value = self.heap.get_table_field(&globals, &key)?;
        
        // Handle metamethods
        let final_value = match value {
            Value::PendingMetamethod(boxed_mm) => {
                // Queue metamethod call
                self.operation_queue.borrow_mut().push_back(PendingOperation::MetamethodCall {
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
        
        // Get globals table from _ENV upvalue (upvalue 0)
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
        
        // Set value in globals
        self.heap.set_table_field(&globals, &key, &value)?;
        
        Ok(())
    }
    
    /// GETTABLE: R(A) := R(B)[RK(C)]
    fn op_gettable(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;

        eprintln!("DEBUG GETTABLE: EXECUTING at base={}, A={}, B={}", base, a, b);
        
        // Get table
        let table_val = self.get_register(base + b)?;
        
        let table_handle = match table_val {
            Value::Table(ref handle) => {
                handle
            },
            _ => {
                return Err(LuaError::TypeError {
                    expected: "table".to_string(),
                    got: table_val.type_name().to_string(),
                });
            }
        };
        
        // Get key
        let key = self.read_rk(base, inst.get_c())?;
        eprintln!("DEBUG GETTABLE: Key={:?}", key);

        // Get value from table
        eprintln!("DEBUG GETTABLE: About to call heap.get_table_field");
        let value = self.heap.get_table_field(table_handle, &key)?;
        
        // Handle metamethods
        let final_value = match value {
            Value::PendingMetamethod(boxed_mm) => {
                // Queue metamethod call
                self.operation_queue.borrow_mut().push_back(PendingOperation::MetamethodCall {
                    method: *boxed_mm,
                    args: vec![Value::Table(Rc::clone(table_handle)), key.clone()],
                    expected_results: 1,
                    result_base: base + a,
                });
                return Ok(());
            },
            other => {
                other
            }
        };
        
        // Store result
        self.set_register(base + a, final_value)?;
        
        Ok(())
    }
    
    /// SETTABLE: R(A)[RK(B)] := RK(C)
    fn op_settable(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        
        eprintln!("DEBUG SETTABLE: EXECUTING at base={}, A={}", base, a);
        eprintln!("DEBUG SETTABLE: Instruction={:08x}", inst.0);
        
        // Get table
        let table_val = self.get_register(base + a)?;
        eprintln!("DEBUG SETTABLE: Retrieved table from register {} (base+a): {:?}", 
                 base + a, table_val);
        
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
        
        // Get key and value
        let key = self.read_rk(base, inst.get_b())?;
        let value = self.read_rk(base, inst.get_c())?;
        
        eprintln!("DEBUG SETTABLE: Key={:?}, Value={:?}", key, value);
        eprintln!("DEBUG SETTABLE: About to call heap.set_table_field");
        
        // Use the heap's metamethod-aware set function
        let metamethod_result = self.heap.set_table_field(table_handle, &key, &value)?;
        eprintln!("DEBUG SETTABLE: heap.set_table_field returned: {:?}", 
                 metamethod_result.is_some());
        
        if let Some(metamethod) = metamethod_result {
            eprintln!("DEBUG SETTABLE: Metamethod found, queueing call");
            // A __newindex function was found, queue the call
            self.operation_queue.borrow_mut().push_back(PendingOperation::MetamethodCall {
                method: metamethod,
                args: vec![Value::Table(table_handle.clone()), key.clone(), value.clone()],
                expected_results: 0,
                result_base: 0, // No results expected
            });
        } else {
            eprintln!("DEBUG SETTABLE: No metamethod, direct field storage completed");
        }
        
        // VERIFICATION: Check if the value was actually stored
        let stored_value = self.heap.get_table_field(table_handle, &key)?;
        eprintln!("DEBUG SETTABLE: VERIFICATION - immediately retrieved stored value: {:?}", stored_value);
        
        eprintln!("DEBUG SETTABLE: Operation complete");
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
                        self.operation_queue.borrow_mut().push_back(PendingOperation::MetamethodCall {
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
        self.op_arithmetic(inst, base, ArithOp::Add)
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
        
        // Use the unified read_rk for proper RK addressing
        let left = self.read_rk(base, inst.get_b())?;
        let right = self.read_rk(base, inst.get_c())?;
        
        // Debug logging for arithmetic operations
        if matches!(op, ArithOp::Add) {
            eprintln!("DEBUG ADD: Executing at base={}, A={}", base, a);
            eprintln!("DEBUG ADD: Left operand: {:?}", left);
            eprintln!("DEBUG ADD: Right operand: {:?}", right);
            eprintln!("DEBUG ADD: Performing addition between: {:?} and {:?}", left, right);
        }
        
        // Try to perform the operation according to Lua 5.1 semantics
        // First, check if both are numbers.
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

        // If not both numbers, try metamethods.
        let mm_key = match op {
            ArithOp::Add => &self.heap.metamethod_names.add,
            ArithOp::Sub => &self.heap.metamethod_names.sub,
            ArithOp::Mul => &self.heap.metamethod_names.mul,
            ArithOp::Div => &self.heap.metamethod_names.div,
            ArithOp::Mod => &self.heap.metamethod_names.mod_op,
            ArithOp::Pow => &self.heap.metamethod_names.pow,
        };
        
        // Try metamethods on both operands
        let mm = match self.find_metamethod(&left, &Value::String(Rc::clone(mm_key)))? {
            Some(method) => Some(method),
            None => self.find_metamethod(&right, &Value::String(Rc::clone(mm_key)))?,
        };

        if let Some(method) = mm {
            if matches!(op, ArithOp::Add) {
                eprintln!("DEBUG ADD: Found metamethod __add, queuing call");
            }
            self.operation_queue.borrow_mut().push_back(PendingOperation::MetamethodCall {
                method,
                args: vec![left, right],
                expected_results: 1,
                result_base: base + a,
            });
            return Ok(());
        }
        
        // No numeric conversion and no metamethod, throw error.
        if matches!(op, ArithOp::Add) {
            eprintln!("DEBUG ADD: No metamethod found, giving type error");
        }
        Err(LuaError::TypeError {
            expected: "number".to_string(),
            got: format!("'{}' and '{}'", left.type_name(), right.type_name()),
        })
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
                let mm = match self.find_metamethod(&operand, &Value::String(Rc::clone(&self.heap.metamethod_names.unm))) {
                    Ok(m) => m,
                    Err(e) => return Err(e),
                };
                
                if let Some(method) = mm {
                    // Queue metamethod call
                    self.operation_queue.borrow_mut().push_back(PendingOperation::MetamethodCall {
                        method,
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
                let mm = match self.find_metamethod(&operand, &Value::String(Rc::clone(&self.heap.metamethod_names.len))) {
                    Ok(m) => m,
                    Err(e) => return Err(e),
                };
                
                if let Some(method) = mm {
                    // Queue metamethod call
                    self.operation_queue.borrow_mut().push_back(PendingOperation::MetamethodCall {
                        method,
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
        
        for (_idx, value) in values.iter().enumerate() {
            match value {
                Value::String(_) | Value::Number(_) => {
                    // These can be concatenated directly
                },
                _ => {
                    // Found a value that needs metamethod checking
                    can_concat_directly = false;
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
            // Need metamethod support for multi-value cases
            // Set up temp for accumulating result
            let temp = base + a;
            self.set_register(temp, values[0].clone())?;

            for i in 1..values.len() {
                let left = self.get_register(temp)?;
                let right = values[i].clone();

                let both_primitive = matches!(&left, Value::String(_) | Value::Number(_)) &&
                                     matches!(&right, Value::String(_) | Value::Number(_));

                if both_primitive {
                    // Direct concat
                    let mut s = String::new();
                    match left {
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
                    self.set_register(temp, Value::String(h))?;
                } else {
                    // Need metamethod
                    let mm = match self.find_metamethod(&left, &Value::String(Rc::clone(&self.heap.metamethod_names.concat))) {
                        Ok(Some(m)) => Some(m),
                        Ok(None) => match self.find_metamethod(&right, &Value::String(Rc::clone(&self.heap.metamethod_names.concat))) {
                            Ok(m) => m,
                            Err(e) => return Err(e),
                        },
                        Err(e) => return Err(e),
                    };
                    if let Some(method) = mm {
                        let new_temp = base + a;
                        self.operation_queue.borrow_mut().push_back(PendingOperation::MetamethodCall {
                            method,
                            args: vec![left, right],
                            expected_results: 1,
                            result_base: new_temp,
                        });
                        self.operation_queue.borrow_mut().push_back(PendingOperation::ConcatContinuation {
                            base,
                            start: b,
                            end: c,
                            current: b + i,
                            temp: new_temp,
                        });
                        return Ok(());
                    } else {
                        return Err(LuaError::TypeError {
                            expected: "string or number".to_string(),
                            got: format!("{} and {}", left.type_name(), right.type_name()),
                        });
                    }
                }
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
        self.operation_queue.borrow_mut().push_back(PendingOperation::FunctionCall {
            func,
            args,
            expected_results,
            result_base: base + a,
            is_protected: false,
            xpcall_handler: None,
        });
        
        Ok(())
    }
    
    /// RETURN: return R(A), ..., R(A+B-2)
    fn op_return(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b();

        eprintln!("DEBUG RETURN: Executing with A={}, B={}, base={}", a, b, base);

        // Per Lua 5.1 spec, upvalues must be closed before the stack frame is unwound.
        // The `base` here is the function's base register, which is the correct boundary for locals.
        eprintln!("DEBUG RETURN: Closing all upvalues at or above stack index {}", base);
        self.heap.close_upvalues(&self.current_thread, base)?;
        eprintln!("DEBUG RETURN: Finished closing upvalues");

        // Collect return values based on the RETURN instruction's B operand.
        // This is the local responsibility of op_return.
        let mut values = Vec::new();
        if b == 0 {
            // B=0: multireturn. Return all values from R(A) to the top of the stack.
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
            // B>0: Return a fixed number of values (B-1).
            let num_returns = (b - 1) as usize;
            eprintln!("DEBUG RETURN: Fixed return (B={}). Collecting {} values from R({})", 
                     b, num_returns, base + a);
            
            for i in 0..num_returns {
                // A function can validly return a value from an uninitialized register, which should be nil.
                // get_register will error if the index is out of bounds, which we treat as nil for robustness.
                if let Ok(value) = self.get_register(base + a + i) {
                    values.push(value);
                } else {
                    values.push(Value::Nil);
                }
            }
        }

        eprintln!("DEBUG RETURN: Collected {} return values:", values.len());
        for (i, val) in values.iter().enumerate() {
            eprintln!("  Return[{}]: {:?}", i, val);
        }

        // Always queue the return operation. The centralized `process_return` will handle
        // popping the call frame and placing the correct number of values based on the
        // CALLER'S expectations, which is the correct architectural pattern.
        self.operation_queue.borrow_mut().push_back(PendingOperation::Return { values });

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
                    let absolute_idx = base + idx;

                    // SYSTEMATIC FIX: A preceding function call may have truncated the
                    // stack, deallocating the register we are about to capture.  Make
                    // sure the register is still valid before touching it.
                    let stack_size = self.heap.get_stack_size(&self.current_thread);
                    if absolute_idx >= stack_size {
                        return Err(LuaError::RuntimeError(format!(
                            "op_closure: cannot capture upvalue from invalid register {} (stack size: {})",
                            absolute_idx, stack_size
                        )));
                    }

                    eprintln!(
                        "DEBUG CLOSURE: Upvalue {} is local var at parent register {}, absolute {}",
                        i, idx, absolute_idx
                    );

                    // Optional verbose debug of the current value
                    #[cfg(debug_assertions)]
                    {
                        let stack_value = self.get_register(absolute_idx)?;
                        eprintln!(
                            "DEBUG CLOSURE: Stack value at R({}) (abs: {}): {:?}",
                            idx, absolute_idx, stack_value
                        );
                    }

                    // Create (or reuse) an upvalue for this local
                    self.heap
                        .find_or_create_upvalue(&self.current_thread, absolute_idx)?
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
        let initial_val = self.get_register(base + a)?;
        let limit_val = self.get_register(base + a + 1)?;
        let step_val = self.get_register(base + a + 2)?;

        // Lua 5.1 Specification: Coerce all loop parameters to numbers.
        // If coercion is successful, update the register with the number value.
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

        let limit_num = match limit_val {
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

        // Subtract step from initial value (Lua 5.1 specification)
        let prepared = initial_num - step_num;
        self.set_register(base + a, Value::Number(prepared))?;

        // ALWAYS jump to FORLOOP (Lua 5.1 specification)
        let pc = self.heap.get_pc(&self.current_thread)?;
        let new_pc = (pc as isize + sbx as isize) as usize;
        self.heap.set_pc(&self.current_thread, new_pc)?;

        Ok(())
    }
    
    /// FORLOOP: R(A) += R(A+2); if R(A) <?= R(A+1) then { R(A+3) = R(A); pc -= sBx }
    fn op_forloop(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let sbx = inst.get_sbx();
        
        // Get loop values. FORPREP should have already converted these to numbers.
        let counter_val = self.get_register(base + a)?;
        let limit_val = self.get_register(base + a + 1)?;
        let step_val = self.get_register(base + a + 2)?;
        
        // We can expect numbers here due to FORPREP's type coercion.
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
    
    /// TFORCALL: R(A+3), ... ,R(A+2+C) := R(A)(R(A+1), R(A+2))
    fn op_tforcall(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let c = inst.get_c() as usize;

        // The results of the iterator call need to go into R(A+3), R(A+4), etc.
        // To use the existing FunctionCall machinery, we need to stage the call
        // in a way that looks like a standard CALL opcode. The natural place to
        // stage this is where the results will go.
        let call_base = base + a + 3;

        // Get the iterator function and its arguments.
        let iter_func = self.get_register(base + a)?;
        let state = self.get_register(base + a + 1)?;
        let control = self.get_register(base + a + 2)?;
        let args = vec![state, control];

        // Ensure we have enough stack space for the staged call.
        // The call will need space for the function + arguments.
        self.ensure_stack_space(call_base + 1 + args.len())?;

        // Stage the function and arguments in consecutive registers starting at `call_base`.
        // The `call_lua_function` will treat this as a standard call frame.
        self.set_register(call_base, iter_func.clone())?;
        for (i, arg) in args.iter().enumerate() {
            self.set_register(call_base + 1 + i, arg.clone())?;
        }

        // Now, we can queue a FunctionCall that operates on this staged area.
        // The result_base is `call_base`, so results will overwrite the staged
        // function and arguments, which is the correct and desired behavior.
        self.operation_queue.borrow_mut().push_back(PendingOperation::FunctionCall {
            func: iter_func,
            args,
            expected_results: c as i32,
            result_base: call_base,
            is_protected: false,
            xpcall_handler: None,
        });

        Ok(())
    }
    
    /// TFORLOOP: if R(A+3) ~= nil then R(A+2) := R(A+3); pc += sBx
    fn op_tforloop_continuation(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let sbx = inst.get_sbx();  // Jump back if continuing
        
        // Get the first result (control variable)
        let control = self.get_register(base + a + 3)?;
        
        if !control.is_nil() {
            // Continue loop: copy control to R(A+2)
            self.set_register(base + a + 2, control)?;
            
            // Jump back
            let pc = self.heap.get_pc(&self.current_thread)?;
            let new_pc = (pc as isize + sbx as isize) as usize;
            self.heap.set_pc(&self.current_thread, new_pc)?;
        } else {
            // End loop: proceed to next instruction
        }
        
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
    
    /// SETUPVAL: UpValue[B] := R(A)
    fn op_setupval(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        eprintln!("DEBUG SETUPVAL: Executing with A(reg)={}, B(upval)={}, base={}", a, b, base);
        
        // Get value to store
        let value = self.get_register(base + a)?;
        eprintln!("DEBUG SETUPVAL: Value from R({}) (absolute {}): {:?}", a, base + a, value);
        
        // Get current frame and closure
        let frame = self.heap.get_current_frame(&self.current_thread)?;
        let closure_ref = frame.closure.borrow();
        
        eprintln!("DEBUG SETUPVAL: Closure has {} upvalues", closure_ref.upvalues.len());
        
        // Check upvalue index
        if b >= closure_ref.upvalues.len() {
            eprintln!("DEBUG SETUPVAL: Upvalue index {} out of bounds", b);
            drop(closure_ref);
            return Err(LuaError::RuntimeError(format!(
                "Upvalue index {} out of bounds", b
            )));
        }
        
        // Get upvalue
        let upvalue = Rc::clone(&closure_ref.upvalues[b]);
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
    fn op_setlist(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let mut b = inst.get_b() as usize;
        let mut c = inst.get_c() as usize;
        
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
        
        // Handle C=0 case where the next instruction holds the real C value
        if c == 0 {
            let pc = self.heap.get_pc(&self.current_thread)?;
            let frame = self.heap.get_current_frame(&self.current_thread)?;
            c = self.get_instruction(&frame.closure, pc)? as usize;
            self.heap.increment_pc(&self.current_thread)?; // Skip the extra instruction
        }
        
        // Calculate base array index
        let array_base = (c - 1) * FIELDS_PER_FLUSH;
        
        // Determine number of elements to set
        let count = if b == 0 {
            // Use all values up to stack top
            b = self.heap.get_stack_size(&self.current_thread) - (base + a + 1);
            b
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
        self.operation_queue.borrow_mut().push_back(PendingOperation::FunctionCall {
            func,
            args,
            expected_results,
            result_base,
            is_protected: false,
            xpcall_handler: None,
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
        let a = inst.get_a();
        let left = self.read_rk(base, inst.get_b())?;
        let right = self.read_rk(base, inst.get_c())?;

        // Step 1: Raw equality. Handles primitives, and same-object for tables/userdata.
        if left == right {
            let result = true;
            if result != (a != 0) {
                self.heap.increment_pc(&self.current_thread)?;
            }
            return Ok(());
        }

        // Step 2: Check for metamethod. This is only possible if types are the same
        // and they are tables or userdata. The `left == right` check above already
        // returns false if types are different because of `Value::eq` implementation.
        // We only proceed if both are tables (or in future, userdata).
        if !matches!(left, Value::Table(_)) || !matches!(right, Value::Table(_)) {
            let result = false;
            if result != (a != 0) {
                self.heap.increment_pc(&self.current_thread)?;
            }
            return Ok(());
        }

        // At this point, we have two different tables. Check for __eq metamethod.
        let mt1 = self.get_metatable(&left)?;
        let mt2 = self.get_metatable(&right)?;

        if let (Some(mt1_handle), Some(mt2_handle)) = (mt1, mt2) {
            let eq_key = Value::String(Rc::clone(&self.heap.metamethod_names.eq));
            let mm1 = self.heap.get_table_field(&mt1_handle, &eq_key)?;
            
            // Per Lua 5.1 spec, __eq is only called if it's present and the same for both operands.
            if !mm1.is_nil() {
                let mm2 = self.heap.get_table_field(&mt2_handle, &eq_key)?;
                if mm1 == mm2 { // `Value::eq` will compare functions by pointer.
                    // Metamethod found, queue the call.
                    let temp = self.heap.get_stack_size(&self.current_thread);
                    self.ensure_stack_space(temp + 1)?;

                    self.operation_queue.borrow_mut().push_back(PendingOperation::MetamethodCall {
                        method: mm1,
                        args: vec![left, right],
                        expected_results: 1,
                        result_base: temp,
                    });
                    let pc = self.heap.get_pc(&self.current_thread)?;
                    self.operation_queue.borrow_mut().push_back(PendingOperation::ComparisonContinuation {
                        comp_op: CompOp::Eq,
                        a,
                        temp,
                        pc,
                        negate: false,
                    });
                    return Ok(());
                }
            }
        }

        // Step 3: No applicable metamethod. Result is false.
        let result = false;
        if result != (a != 0) {
            self.heap.increment_pc(&self.current_thread)?;
        }
        Ok(())
    }
    
    /// LT: if ((RK(B) < RK(C)) ~= A) then pc++
    fn op_lt(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a();
        let left = self.read_rk(base, inst.get_b())?;
        let right = self.read_rk(base, inst.get_c())?;

        let mut result = false;
        match (&left, &right) {
            (Value::Number(l), Value::Number(r)) => result = l < r,
            (Value::String(l), Value::String(r)) => {
                result = l.borrow().bytes < r.borrow().bytes;
            }
            _ => {
                // Try __lt metamethod
                let mm = match self.find_metamethod(&left, &Value::String(Rc::clone(&self.heap.metamethod_names.lt))) {
                    Ok(Some(m)) => Some(m),
                    Ok(None) => match self.find_metamethod(&right, &Value::String(Rc::clone(&self.heap.metamethod_names.lt))) {
                        Ok(m) => m,
                        Err(e) => return Err(e),
                    },
                    Err(e) => return Err(e),
                };
                if let Some(method) = mm {
                    let temp = self.heap.get_stack_size(&self.current_thread);
                    self.ensure_stack_space(temp + 1)?;

                    self.operation_queue.borrow_mut().push_back(PendingOperation::MetamethodCall {
                        method,
                        args: vec![left, right],
                        expected_results: 1,
                        result_base: temp,
                    });
                    let pc = self.heap.get_pc(&self.current_thread)?;
                    self.operation_queue.borrow_mut().push_back(PendingOperation::ComparisonContinuation {
                        comp_op: CompOp::Lt,
                        a,
                        temp,
                        pc,
                        negate: false,
                    });
                    return Ok(());
                } else {
                    return Err(LuaError::TypeError {
                        expected: "comparable values".to_string(),
                        got: format!("{} and {}", left.type_name(), right.type_name()),
                    });
                }
            }
        }

        let skip = result != (a != 0);
        if skip {
            self.heap.increment_pc(&self.current_thread)?;
        }
        Ok(())
    }
    
    /// LE: if ((RK(B) <= RK(C)) ~= A) then pc++
    fn op_le(&mut self, inst: Instruction, base: usize) -> LuaResult<()> {
        let a = inst.get_a();
        let left = self.read_rk(base, inst.get_b())?;
        let right = self.read_rk(base, inst.get_c())?;

        let mut result = false;
        match (&left, &right) {
            (Value::Number(l), Value::Number(r)) => result = l <= r,
            (Value::String(l), Value::String(r)) => {
                result = l.borrow().bytes <= r.borrow().bytes;
            }
            _ => {
                // First try __le metamethod per Lua 5.1 specification
                let le_key = Value::String(Rc::clone(&self.heap.metamethod_names.le));
                let mm_le = match self.find_metamethod(&left, &le_key)? {
                    Some(m) => Some(m),
                    None => self.find_metamethod(&right, &le_key)?,
                };

                if let Some(method) = mm_le {
                    let temp = self.heap.get_stack_size(&self.current_thread);
                    self.ensure_stack_space(temp + 1)?;

                    self.operation_queue.borrow_mut().push_back(PendingOperation::MetamethodCall {
                        method,
                        args: vec![left, right],
                        expected_results: 1,
                        result_base: temp,
                    });
                    let pc = self.heap.get_pc(&self.current_thread)?;
                    self.operation_queue.borrow_mut().push_back(PendingOperation::ComparisonContinuation {
                        comp_op: CompOp::Le,
                        a,
                        temp,
                        pc,
                        negate: false,
                    });
                    return Ok(());
                }

                // CRITICAL Lua 5.1 FALLBACK: try __lt with swapped operands and negate result
                let lt_key = Value::String(Rc::clone(&self.heap.metamethod_names.lt));
                let mm_lt = match self.find_metamethod(&left, &lt_key)? {
                    Some(m) => Some(m),
                    None => self.find_metamethod(&right, &lt_key)?,
                };

                if let Some(method) = mm_lt {
                    let temp = self.heap.get_stack_size(&self.current_thread);
                    self.ensure_stack_space(temp + 1)?;

                    // Call __lt(right, left) and negate result: a <= b becomes not (b < a)
                    self.operation_queue.borrow_mut().push_back(PendingOperation::MetamethodCall {
                        method,
                        args: vec![right, left], // SWAPPED operands for LE fallback
                        expected_results: 1,
                        result_base: temp,
                    });
                    
                    let pc = self.heap.get_pc(&self.current_thread)?;
                    self.operation_queue.borrow_mut().push_back(PendingOperation::ComparisonContinuation {
                        comp_op: CompOp::Le,
                        a,
                        temp,
                        pc,
                        negate: true, // NEGATE the result for LE fallback
                    });
                    return Ok(());
                }

                // No metamethods found
                return Err(LuaError::TypeError {
                    expected: "comparable values".to_string(),
                    got: format!("{} and {}", left.type_name(), right.type_name()),
                });
            }
        }

        let skip = result != (a != 0);
        if skip {
            self.heap.increment_pc(&self.current_thread)?;
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
    
    /// Get metatable for a value
    fn get_metatable(&self, value: &Value) -> LuaResult<Option<TableHandle>> {
        match value {
            Value::Table(ref handle) => {
                let table_ref = handle.borrow();
                Ok(table_ref.metatable.clone())
            },
            // Other types with metatables would be handled here
            _ => Ok(None)
        }
    }

    /// Find a metamethod for a value according to Lua 5.1 specification
    fn find_metamethod(&self, value: &Value, method_name: &Value) -> LuaResult<Option<Value>> {
        match value {
            Value::Table(ref handle) => {
                let table_ref = handle.borrow();
                
                // Check if table has a metatable
                let metatable_opt = table_ref.metatable.clone();
                
                // Drop the table borrow before accessing metatable
                drop(table_ref);
                
                if let Some(metatable) = metatable_opt {
                    // Lua 5.1 specification: check metatable for the metamethod
                    let mt_ref = metatable.borrow();
                    
                    // Use raw field access to avoid recursive metamethod calls
                    if let Some(method) = mt_ref.get_field(method_name) {
                        match method {
                            Value::Nil => {
                                // Metamethod present but nil - no metamethod according to spec
                                drop(mt_ref);
                                return Ok(None);
                            },
                            _ => {
                                // Found valid metamethod
                                drop(mt_ref);
                                return Ok(Some(method));
                            }
                        }
                    }
                    
                    drop(mt_ref);
                }
            },
            // According to Lua 5.1 spec, other types can also have metamethods
            // but the VM currently only implements them for tables
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
                "Argument {} out of range (nargs: {})", index, self.nargs
            )));
        }
        
        let args_base = self.base;
        let register_index = args_base + index;
        
        eprintln!("DEBUG ExecutionContext::get_arg:");
        eprintln!("  Argument index: {}", index);
        eprintln!("  Base: {}", self.base);
        eprintln!("  Args base: {}", args_base);
        eprintln!("  Calculated register index: {}", register_index);
        eprintln!("  Total nargs: {}", self.nargs);
        
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
        match self.vm.heap.set_table_field(table, &key, &value)? {
            Some(metamethod) => {
                // A metamethod was found. Queue a call to it using the proper architecture.
                let op = PendingOperation::MetamethodCall {
                    method: metamethod,
                    args: vec![Value::Table(table.clone()), key, value],
                    expected_results: 0, // __newindex returns no results
                    result_base: 0,      // Not relevant for 0 results
                };

                // Use fine-grained interior mutability - borrow only the operation_queue
                // This works because self.vm is an immutable reference and operation_queue is RefCell
                self.vm.operation_queue.borrow_mut().push_back(op);
                
                Ok(())
            },
            None => {
                // No metamethod needed; the operation completed successfully
                Ok(())
            }
        }
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

        // State 1: Start of iteration (key is nil)
        if key.is_nil() {
            // Try array part first
            for i in 0..table_ref.array.len() {
                if !table_ref.array[i].is_nil() {
                    return Ok(Some((Value::Number((i + 1) as f64), table_ref.array[i].clone())));
                }
            }
            // If array is empty or all nils, start with the hash part
            if let Some((k, v)) = table_ref.map.iter().next() {
                return Ok(Some((k.to_value(), v.clone())));
            }
            return Ok(None); // Table is empty
        }

        // State 2: Continuing iteration
        let mut in_array = false;
        if let Value::Number(n) = key {
            if n.fract() == 0.0 && *n >= 1.0 {
                let index = *n as usize;
                if index <= table_ref.array.len() {
                    in_array = true;
                    // Search for the next non-nil element in the array part (next Lua index is `index + 1`, which is Rust index `index`)
                    for i in index..table_ref.array.len() {
                        if !table_ref.array[i].is_nil() {
                            return Ok(Some((Value::Number((i + 1) as f64), table_ref.array[i].clone())));
                        }
                    }
                    // Array part exhausted, fall through to start hash part iteration
                }
            }
        }

        // State 3: Hash part iteration
        // This is reached if the key was not in the array, or if array iteration just finished.
        if !in_array {
            // The key must be in the hash map. Find it, then find the next one.
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
            // This is the transition from array to hash. Just get the first hash element.
            if let Some((k, v)) = table_ref.map.iter().next() {
                return Ok(Some((k.to_value(), v.clone())));
            }
        }
        
        // No more keys
        Ok(None)
    }
    
    fn globals_handle(&self) -> LuaResult<TableHandle> {
        Ok(self.vm.heap.globals())
    }
    
    fn get_call_base(&self) -> usize {
        self.base
    }
    
    fn pcall(&mut self, func: Value, args: Vec<Value>) -> LuaResult<()> {
        let result_base = self.base.saturating_sub(1); // pcall results overwrite pcall itself
        self.vm.operation_queue.borrow_mut().push_back(PendingOperation::FunctionCall {
            func,
            args,
            expected_results: -1, // Variable results
            result_base,
            is_protected: true,
            xpcall_handler: None,
        });
        Ok(())
    }
    
    fn xpcall(&mut self, func: Value, err_handler: Value) -> LuaResult<()> {
        let result_base = self.base.saturating_sub(1);
        let args = vec![]; // xpcall takes no extra arguments for the function
        self.vm.operation_queue.borrow_mut().push_back(PendingOperation::FunctionCall {
            func,
            args,
            expected_results: -1,
            result_base,
            is_protected: true,
            xpcall_handler: Some(err_handler),
        });
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
        let vm = RcVM::new()?;
        assert!(vm.operation_queue.borrow().is_empty());
        Ok(())
    }
    
    // Add more tests as needed
}