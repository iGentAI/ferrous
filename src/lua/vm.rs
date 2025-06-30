//! Lua Virtual Machine Implementation
//!
//! This module implements the core Lua VM using a non-recursive state machine
//! architecture that avoids stack overflow and works with Rust's ownership model.

use std::collections::{VecDeque, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::error::{LuaError, Result};
use super::value::{
    Value, TableHandle, StringHandle, ClosureHandle, ThreadHandle,
    CallFrame, CallFrameType, CFunction, FunctionProto, MetamethodType,
};
use super::heap::LuaHeap;
use super::transaction::HeapTransaction;
use super::compiler::CompiledModule;

/// Lua instruction representation
#[derive(Clone, Copy, Debug)]
pub struct Instruction(pub u32);

impl Instruction {
    /// Get the opcode
    pub fn opcode(&self) -> u8 {
        (self.0 & 0x3F) as u8
    }
    
    /// Get operand A
    pub fn a(&self) -> u8 {
        ((self.0 >> 6) & 0xFF) as u8
    }
    
    /// Get operand B
    pub fn b(&self) -> u16 {
        ((self.0 >> 14) & 0x1FF) as u16
    }
    
    /// Get operand C
    pub fn c(&self) -> u16 {
        ((self.0 >> 23) & 0x1FF) as u16
    }
    
    /// Get Bx (B and C combined)
    pub fn bx(&self) -> u32 {
        (self.0 >> 14) & 0x3FFFF
    }
    
    /// Get sBx (signed Bx)
    pub fn sbx(&self) -> i32 {
        (self.bx() as i32) - 131071
    }
    
    /// Check if B is a constant
    pub fn b_is_constant(&self) -> bool {
        (self.b() & 0x100) != 0
    }
    
    /// Check if C is a constant
    pub fn c_is_constant(&self) -> bool {
        (self.c() & 0x100) != 0
    }
    
    /// Get B as constant index
    pub fn b_as_constant(&self) -> usize {
        (self.b() & 0xFF) as usize
    }
    
    /// Get C as constant index
    pub fn c_as_constant(&self) -> usize {
        (self.c() & 0xFF) as usize
    }
}

/// Lua opcodes
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpCode {
    Move = 0,
    LoadK = 1,
    LoadBool = 2,
    LoadNil = 3,
    GetUpVal = 4,
    GetGlobal = 5,
    GetTable = 6,
    SetGlobal = 7,
    SetUpVal = 8,
    SetTable = 9,
    NewTable = 10,
    Self_ = 11,
    Add = 12,
    Sub = 13,
    Mul = 14,
    Div = 15,
    Mod = 16,
    Pow = 17,
    Unm = 18,
    Not = 19,
    Len = 20,
    Concat = 21,
    Jmp = 22,
    Eq = 23,
    Lt = 24,
    Le = 25,
    Test = 26,
    TestSet = 27,
    Call = 28,
    TailCall = 29,
    Return = 30,
    ForLoop = 31,
    ForPrep = 32,
    TForLoop = 33,
    SetList = 34,
    Close = 35,
    Closure = 36,
    VarArg = 37,
}

impl From<u8> for OpCode {
    fn from(value: u8) -> Self {
        unsafe { std::mem::transmute(value.min(37)) }
    }
}

/// Pending operations to be processed by the state machine
#[derive(Debug, Clone)]
pub enum PendingOperation {
    /// Function call
    FunctionCall {
        closure: ClosureHandle,
        args: Vec<Value>,
        context: PostCallContext,
    },
    
    /// Metamethod call
    MetamethodCall {
        method_name: StringHandle,
        object: Value,
        args: Vec<Value>,
        context: PostCallContext,
    },
    
    /// Concatenation operation
    Concat {
        values: Vec<Value>,
        current_index: usize,
        base_register: u16,
        target_register: usize,
        accumulated: Vec<String>,
    },
}

/// Context for handling function returns
#[derive(Debug, Clone)]
pub enum PostCallContext {
    /// Normal call - store result in register
    Normal {
        base_register: u16,
        result_register: usize,
        expected_results: usize,
    },
    
    /// Iterator call for generic for loop
    Iterator {
        base_register: u16,
        register_a: usize,
        var_count: usize,
    },
    
    /// Metamethod call
    Metamethod {
        method: MetamethodType,
        base_register: u16,
        result_register: usize,
    },
    
    /// Concatenation __tostring call
    ConcatToString {
        remaining_values: Vec<Value>,
        current_index: usize,
        base_register: u16,
        target_register: usize,
        accumulated: Vec<String>,
    },
    
    /// Final result
    FinalResult,
}

/// Execution context for C functions
pub struct ExecutionContext<'a> {
    /// The VM
    pub vm: &'a mut LuaVM,
    /// Base stack position
    pub base: usize,
    /// Number of arguments
    pub arg_count: usize,
}

impl<'a> ExecutionContext<'a> {
    /// Get an argument
    pub fn get_arg(&self, index: usize) -> Result<Value> {
        if index >= self.arg_count {
            return Err(LuaError::ArgError(index + 1, "not enough arguments".to_string()));
        }
        
        let thread = self.vm.heap.get_main_thread()?;
        self.vm.heap.get_thread_stack_value(thread, self.base + index)
    }
    
    /// Push a result
    pub fn push_result(&mut self, value: Value) -> Result<()> {
        let thread = self.vm.heap.get_main_thread()?;
        let mut tx = HeapTransaction::new(&mut self.vm.heap);
        tx.push_stack(thread, value);
        tx.commit()?;
        Ok(())
    }
    
    /// Get number of arguments
    pub fn arg_count(&self) -> usize {
        self.arg_count
    }
}

/// The Lua virtual machine
pub struct LuaVM {
    /// The heap
    pub heap: LuaHeap,
    
    /// Current thread
    current_thread: ThreadHandle,
    
    /// Pending operations queue
    pending_operations: VecDeque<PendingOperation>,
    
    /// Return contexts by call depth
    return_contexts: HashMap<usize, PostCallContext>,
    
    /// Kill flag
    kill_flag: Option<Arc<AtomicBool>>,
    
    /// Instruction count
    instruction_count: u64,
    
    /// Maximum instructions
    max_instructions: Option<u64>,
}

impl LuaVM {
    /// Create a new VM
    pub fn new() -> Result<Self> {
        let heap = LuaHeap::new();
        let current_thread = heap.get_main_thread()?;
        
        Ok(LuaVM {
            heap,
            current_thread,
            pending_operations: VecDeque::new(),
            return_contexts: HashMap::new(),
            kill_flag: None,
            instruction_count: 0,
            max_instructions: Some(50_000_000),
        })
    }
    
    /// Set kill flag
    pub fn set_kill_flag(&mut self, flag: Arc<AtomicBool>) {
        self.kill_flag = Some(flag);
    }
    
    /// Check if should kill
    fn should_kill(&self) -> bool {
        if let Some(ref flag) = self.kill_flag {
            flag.load(Ordering::Relaxed)
        } else {
            false
        }
    }
    
    /// Check resource limits
    fn check_limits(&self) -> Result<()> {
        // Check instruction count
        if let Some(max) = self.max_instructions {
            if self.instruction_count > max {
                return Err(LuaError::InstructionLimit);
            }
        }
        
        // Check call depth
        let depth = self.heap.get_thread_call_depth(self.current_thread.clone())?;
        if depth > 1000 {
            return Err(LuaError::StackOverflow);
        }
        
        Ok(())
    }
    
    /// Execute a compiled module
    pub fn execute_module(&mut self, module: &CompiledModule, args: &[Value]) -> Result<Value> {
        // Load module into heap
        let closure = self.load_module(module)?;
        
        // Execute the main function
        self.execute_function(closure, args)
    }
    
    /// Load a compiled module
    pub fn load_module(&mut self, module: &CompiledModule) -> Result<ClosureHandle> {
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Create strings
        let mut string_map = HashMap::new();
        for (i, s) in module.strings.iter().enumerate() {
            let handle = tx.create_string(s)?;
            string_map.insert(i, handle);
        }
        
        // Convert constants
        let mut constants = Vec::new();
        for c in &module.constants {
            let value = self.convert_compilation_value(c, &string_map, &tx)?;
            constants.push(value);
        }
        
        // Create function prototype
        let proto = FunctionProto {
            bytecode: module.bytecode.clone(),
            constants,
            upvalues: Vec::new(), // Would need proper upvalue conversion
            param_count: 0,
            is_vararg: false,
            source: None,
            line_defined: 0,
            last_line_defined: 0,
            line_info: Vec::new(),
            locals: Vec::new(),
        };
        
        // Create closure
        let closure = tx.create_closure(proto, Vec::new())?;
        
        tx.commit()?;
        
        Ok(closure)
    }
    
    /// Convert compilation value to runtime value
    fn convert_compilation_value(
        &self,
        value: &super::compiler::CompilationValue,
        string_map: &HashMap<usize, StringHandle>,
        _tx: &HeapTransaction,
    ) -> Result<Value> {
        use super::compiler::CompilationValue;
        
        match value {
            CompilationValue::Nil => Ok(Value::Nil),
            CompilationValue::Boolean(b) => Ok(Value::Boolean(*b)),
            CompilationValue::Number(n) => Ok(Value::Number(*n)),
            CompilationValue::StringRef(idx) => {
                let handle = string_map.get(idx)
                    .ok_or(LuaError::CompileError("Invalid string reference".to_string()))?;
                Ok(Value::String(handle.clone()))
            }
        }
    }
    
    /// Execute a function (main entry point)
    pub fn execute_function(&mut self, closure: ClosureHandle, args: &[Value]) -> Result<Value> {
        // Record initial call depth
        let initial_depth = self.heap.get_thread_call_depth(self.current_thread.clone())?;
        
        // Push initial call frame
        self.push_call_frame(closure, args)?;
        
        // Initialize result
        let mut final_result = Value::Nil;
        
        // Main execution loop - NO RECURSION
        loop {
            // Check termination conditions
            if self.should_kill() {
                return Err(LuaError::ScriptKilled);
            }
            
            // Check resource limits
            self.check_limits()?;
            
            // Process any pending operations first
            if !self.pending_operations.is_empty() {
                let op = self.pending_operations.pop_front().unwrap();
                self.process_pending_operation(op)?;
                continue;
            }
            
            // Check if we're back to initial depth
            let depth = self.heap.get_thread_call_depth(self.current_thread.clone())?;
            if depth <= initial_depth {
                return Ok(final_result);
            }
            
            // Execute next instruction
            match self.step()? {
                StepResult::Continue => {
                    self.instruction_count += 1;
                },
                StepResult::Return(values) => {
                    final_result = values.first().cloned().unwrap_or(Value::Nil);
                    
                    // Pop the frame
                    self.pop_call_frame()?;
                    
                    // Check if we're back to initial depth
                    let depth = self.heap.get_thread_call_depth(self.current_thread.clone())?;
                    if depth <= initial_depth {
                        return Ok(final_result);
                    }
                    
                    // Handle return in current context
                    if let Some(context) = self.return_contexts.remove(&depth) {
                        self.handle_return(values, context)?;
                    }
                },
                StepResult::Yield(_) => {
                    return Err(LuaError::NotImplemented("coroutines".to_string()));
                }
            }
        }
    }
    
    /// Process pending operations
    fn process_pending_operation(&mut self, op: PendingOperation) -> Result<ExecutionStatus> {
        match op {
            PendingOperation::FunctionCall { closure, args, context } => {
                // Get current depth
                let depth = self.heap.get_thread_call_depth(self.current_thread.clone())?;
                
                // Store return context
                self.return_contexts.insert(depth, context);
                
                // Push new call frame
                self.push_call_frame(closure, &args)?;
                
                Ok(ExecutionStatus::Continue)
            }
            
            PendingOperation::MetamethodCall { method_name, object, args, context } => {
                // We need to clone method_name to avoid it being moved
                let method_name_cloned = method_name.clone();
                
                // Find metamethod
                let method_value = self.get_metamethod(&object, method_name)?;
                
                match method_value {
                    Value::Closure(closure) => {
                        // Queue function call
                        self.pending_operations.push_back(PendingOperation::FunctionCall {
                            closure,
                            args,
                            context,
                        });
                        
                        Ok(ExecutionStatus::Continue)
                    }
                    Value::CFunction(cfunc) => {
                        // Execute C function in a way that avoids borrow checker issues
                        let args_clone = args.clone();
                        let results = self.call_c_function(cfunc, &args_clone)?;
                        self.handle_return(results, context)?;
                        Ok(ExecutionStatus::Continue)
                    }
                    _ => {
                        // Avoid second use of moved value by using the clone
                        let method_name_str = self.heap.get_string_value(method_name_cloned)?;
                        Err(LuaError::TypeError(format!(
                            "metamethod {} is not a function", method_name_str
                        )))
                    }
                }
            }
            
            PendingOperation::Concat { values, current_index, base_register, target_register, accumulated } => {
                self.process_concat(values, current_index, base_register, target_register, accumulated)?;
                Ok(ExecutionStatus::Continue)
            }
        }
    }
    
    /// Step method that handles execution of a single instruction
    fn step(&mut self) -> Result<StepResult> {
        // Create transaction first to ensure consistent heap access
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Get current frame and instruction data
        let frame = tx.get_current_call_frame(self.current_thread.clone())?.clone();
        let instr = tx.get_instruction(frame.closure.clone(), frame.pc)?;
        
        // Commit transaction before execute_instruction
        tx.commit()?;
        
        // Now execute the instruction separately
        let result = self.execute_instruction_sep(&frame, instr)?;
        
        Ok(result)
    }

    /// Execute instruction separately to avoid double mut borrow
    fn execute_instruction_sep(&mut self, frame: &CallFrame, instr: Instruction) -> Result<StepResult> {
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        let opcode = OpCode::from(instr.opcode());
        let a = instr.a();
        let b = instr.b();
        let c = instr.c();
        
        // Increment PC except for jumps
        if !matches!(opcode, OpCode::Jmp | OpCode::ForLoop | OpCode::ForPrep) {
            tx.increment_pc(self.current_thread.clone())?;
        }
        
        // Execute the instruction
        let result = self.execute_instruction(&mut tx, frame, instr)?;
        
        // Commit transaction and get pending operations
        let pending_ops = tx.commit()?;
        
        // Queue pending operations
        for op in pending_ops {
            self.pending_operations.push_back(op);
        }
        
        Ok(result)
    }
    
    /// Execute a single instruction
    fn execute_instruction(
        &mut self,
        tx: &mut HeapTransaction,
        frame: &CallFrame,
        instr: Instruction,
    ) -> Result<StepResult> {
        let opcode = OpCode::from(instr.opcode());
        let a = instr.a() as usize;
        let b = instr.b() as usize;
        let c = instr.c() as usize;
        let base = frame.base_register as usize;
        
        match opcode {
            OpCode::Move => {
                // R(A) := R(B)
                let value = tx.read_register(self.current_thread.clone(), base + b)?;
                tx.set_register(self.current_thread.clone(), base + a, value);
                Ok(StepResult::Continue)
            }
            
            OpCode::LoadK => {
                // R(A) := Kst(Bx)
                let value = tx.get_constant(frame.closure.clone(), instr.bx() as usize)?;
                tx.set_register(self.current_thread.clone(), base + a, value);
                Ok(StepResult::Continue)
            }
            
            OpCode::LoadBool => {
                // R(A) := (Bool)B; if (C) pc++
                tx.set_register(self.current_thread.clone(), base + a, Value::Boolean(b != 0));
                if c != 0 {
                    tx.increment_pc(self.current_thread.clone())?;
                }
                Ok(StepResult::Continue)
            }
            
            OpCode::LoadNil => {
                // R(A), ..., R(B) := nil
                for i in a..=b {
                    tx.set_register(self.current_thread.clone(), base + i, Value::Nil);
                }
                Ok(StepResult::Continue)
            }
            
            OpCode::GetTable => {
                // R(A) := R(B)[RK(C)]
                let table_val = tx.read_register(self.current_thread.clone(), base + b)?;
                let key = if instr.c_is_constant() {
                    tx.get_constant(frame.closure.clone(), instr.c_as_constant())?
                } else {
                    tx.read_register(self.current_thread.clone(), base + c)?
                };
                
                match table_val {
                    Value::Table(table) => {
                        match tx.read_table_field(table.clone(), &key) {
                            Ok(value) => {
                                tx.set_register(self.current_thread.clone(), base + a, value);
                                Ok(StepResult::Continue)
                            }
                            Err(_) => {
                                // Check __index metamethod
                                self.handle_index_metamethod(tx, table, key, base + a)
                            }
                        }
                    }
                    _ => Err(LuaError::TypeError(format!(
                        "attempt to index a {} value",
                        table_val.type_name()
                    ))),
                }
            }
            
            OpCode::SetTable => {
                // R(A)[RK(B)] := RK(C)
                let table_val = tx.read_register(self.current_thread.clone(), base + a)?;
                let key = if instr.b_is_constant() {
                    tx.get_constant(frame.closure.clone(), instr.b_as_constant())?
                } else {
                    tx.read_register(self.current_thread.clone(), base + b)?
                };
                let value = if instr.c_is_constant() {
                    tx.get_constant(frame.closure.clone(), instr.c_as_constant())?
                } else {
                    tx.read_register(self.current_thread.clone(), base + c)?
                };
                
                match table_val {
                    Value::Table(table) => {
                        tx.set_table_field(table, key, value)?;
                        Ok(StepResult::Continue)
                    }
                    _ => Err(LuaError::TypeError(format!(
                        "attempt to index a {} value",
                        table_val.type_name()
                    ))),
                }
            }
            
            OpCode::NewTable => {
                // R(A) := {} (size = B,C)
                let table = tx.create_table()?;
                tx.set_register(self.current_thread.clone(), base + a, Value::Table(table));
                Ok(StepResult::Continue)
            }
            
            OpCode::Add => self.execute_arithmetic(tx, frame, a, b, c, |x, y| x + y),
            OpCode::Sub => self.execute_arithmetic(tx, frame, a, b, c, |x, y| x - y),
            OpCode::Mul => self.execute_arithmetic(tx, frame, a, b, c, |x, y| x * y),
            OpCode::Div => self.execute_arithmetic(tx, frame, a, b, c, |x, y| x / y),
            OpCode::Mod => self.execute_arithmetic(tx, frame, a, b, c, |x, y| x % y),
            OpCode::Pow => self.execute_arithmetic(tx, frame, a, b, c, |x, y| x.powf(y)),
            
            OpCode::Unm => {
                // R(A) := -R(B)
                let value = tx.read_register(self.current_thread.clone(), base + b)?;
                match value {
                    Value::Number(n) => {
                        tx.set_register(self.current_thread.clone(), base + a, Value::Number(-n));
                        Ok(StepResult::Continue)
                    }
                    _ => Err(LuaError::TypeError(format!(
                        "attempt to perform arithmetic on a {} value",
                        value.type_name()
                    ))),
                }
            }
            
            OpCode::Not => {
                // R(A) := not R(B)
                let value = tx.read_register(self.current_thread.clone(), base + b)?;
                let result = !value.is_truthy();
                tx.set_register(self.current_thread.clone(), base + a, Value::Boolean(result));
                Ok(StepResult::Continue)
            }
            
            OpCode::Len => {
                // R(A) := length of R(B)
                let value = tx.read_register(self.current_thread.clone(), base + b)?;
                match value {
                    Value::String(s) => {
                        let len = self.heap.get_string_bytes(s)?.len();
                        tx.set_register(self.current_thread.clone(), base + a, Value::Number(len as f64));
                        Ok(StepResult::Continue)
                    }
                    Value::Table(t) => {
                        let len = self.heap.get_table(t)?.len();
                        tx.set_register(self.current_thread.clone(), base + a, Value::Number(len as f64));
                        Ok(StepResult::Continue)
                    }
                    _ => Err(LuaError::TypeError(format!(
                        "attempt to get length of a {} value",
                        value.type_name()
                    ))),
                }
            }
            
            OpCode::Concat => {
                // R(A) := R(B).. ... ..R(C)
                let mut values = Vec::new();
                for i in b..=c {
                    values.push(tx.read_register(self.current_thread.clone(), base + i)?);
                }
                
                tx.queue_operation(PendingOperation::Concat {
                    values,
                    current_index: 0,
                    base_register: frame.base_register,
                    target_register: a,
                    accumulated: Vec::new(),
                });
                
                Ok(StepResult::Continue)
            }
            
            OpCode::Jmp => {
                // PC += sBx
                tx.jump(self.current_thread.clone(), instr.sbx())?;
                Ok(StepResult::Continue)
            }
            
            OpCode::Eq => self.execute_comparison(tx, frame, a, b, c, |x, y| x == y),
            OpCode::Lt => self.execute_comparison(tx, frame, a, b, c, |x, y| self.lua_lt(x, y)),
            OpCode::Le => self.execute_comparison(tx, frame, a, b, c, |x, y| self.lua_le(x, y)),
            
            OpCode::Test => {
                // if not (R(A) <=> C) then pc++
                let value = tx.read_register(self.current_thread.clone(), base + a)?;
                if value.is_truthy() != (c != 0) {
                    tx.increment_pc(self.current_thread.clone())?;
                }
                Ok(StepResult::Continue)
            }
            
            OpCode::TestSet => {
                // if (R(B) <=> C) then R(A) := R(B) else pc++
                let value = tx.read_register(self.current_thread.clone(), base + b)?;
                if value.is_truthy() == (c != 0) {
                    tx.set_register(self.current_thread.clone(), base + a, value);
                } else {
                    tx.increment_pc(self.current_thread.clone())?;
                }
                Ok(StepResult::Continue)
            }
            
            OpCode::Call => {
                // R(A), ... ,R(A+C-2) := R(A)(R(A+1), ... ,R(A+B-1))
                let func = tx.read_register(self.current_thread.clone(), base + a)?;
                
                let arg_count = if b == 0 {
                    // All values from A+1 to top
                    tx.get_stack_top(self.current_thread.clone())? - base - a - 1
                } else {
                    b - 1
                };
                
                let mut args = Vec::new();
                for i in 0..arg_count {
                    args.push(tx.read_register(self.current_thread.clone(), base + a + 1 + i)?);
                }
                
                let expected_results = if c == 0 { 1 } else { c - 1 };
                
                self.execute_call(tx, func, args, base, a, expected_results)
            }
            
            OpCode::TailCall => {
                // return R(A)(R(A+1), ... ,R(A+B-1))
                let func = tx.read_register(self.current_thread.clone(), base + a)?;
                
                let arg_count = if b == 0 {
                    tx.get_stack_top(self.current_thread.clone())? - base - a - 1
                } else {
                    b - 1
                };
                
                let mut args = Vec::new();
                for i in 0..arg_count {
                    args.push(tx.read_register(self.current_thread.clone(), base + a + 1 + i)?);
                }
                
                // Pop current frame
                tx.pop_call_frame(self.current_thread.clone())?;
                
                // Execute as normal call
                match func {
                    Value::Closure(closure) => {
                        tx.queue_operation(PendingOperation::FunctionCall {
                            closure,
                            args,
                            context: PostCallContext::FinalResult,
                        });
                        Ok(StepResult::Continue)
                    }
                    Value::CFunction(cfunc) => {
                        let results = self.call_c_function(cfunc, &args)?;
                        Ok(StepResult::Return(results))
                    }
                    _ => Err(LuaError::TypeError(format!(
                        "attempt to call a {} value",
                        func.type_name()
                    ))),
                }
            }
            
            OpCode::Return => {
                // return R(A), ... ,R(A+B-2)
                let count = if b == 0 {
                    tx.get_stack_top(self.current_thread.clone())? - base - a
                } else {
                    b - 1
                };
                
                let mut results = Vec::new();
                for i in 0..count {
                    results.push(tx.read_register(self.current_thread.clone(), base + a + i)?);
                }
                
                Ok(StepResult::Return(results))
            }
            
            OpCode::ForPrep => {
                // R(A)-=R(A+2); pc+=sBx
                let init = tx.read_register(self.current_thread.clone(), base + a)?;
                let step = tx.read_register(self.current_thread.clone(), base + a + 2)?;
                
                match (init, step) {
                    (Value::Number(init_n), Value::Number(step_n)) => {
                        tx.set_register(self.current_thread.clone(), base + a, Value::Number(init_n - step_n));
                        tx.jump(self.current_thread.clone(), instr.sbx())?;
                        Ok(StepResult::Continue)
                    }
                    _ => Err(LuaError::TypeError("'for' variables must be numbers".to_string())),
                }
            }
            
            OpCode::ForLoop => {
                // R(A)+=R(A+2); if R(A) <?= R(A+1) then { pc+=sBx; R(A+3)=R(A) }
                let index = tx.read_register(self.current_thread.clone(), base + a)?;
                let limit = tx.read_register(self.current_thread.clone(), base + a + 1)?;
                let step = tx.read_register(self.current_thread.clone(), base + a + 2)?;
                
                match (index, limit, step) {
                    (Value::Number(idx), Value::Number(lim), Value::Number(stp)) => {
                        let new_idx = idx + stp;
                        tx.set_register(self.current_thread.clone(), base + a, Value::Number(new_idx));
                        
                        let continue_loop = if stp > 0.0 {
                            new_idx <= lim
                        } else {
                            new_idx >= lim
                        };
                        
                        if continue_loop {
                            tx.set_register(self.current_thread.clone(), base + a + 3, Value::Number(new_idx));
                            tx.jump(self.current_thread.clone(), instr.sbx())?;
                        }
                        
                        Ok(StepResult::Continue)
                    }
                    _ => Err(LuaError::TypeError("'for' variables must be numbers".to_string())),
                }
            }
            
            OpCode::TForLoop => {
                // R(A+3), ... ,R(A+2+C) := R(A)(R(A+1), R(A+2));
                // if R(A+3) ~= nil then R(A+2)=R(A+3) else pc++
                let iter = tx.read_register(self.current_thread.clone(), base + a)?;
                let state = tx.read_register(self.current_thread.clone(), base + a + 1)?;
                let control = tx.read_register(self.current_thread.clone(), base + a + 2)?;
                
                match iter {
                    Value::Closure(closure) => {
                        tx.queue_operation(PendingOperation::FunctionCall {
                            closure,
                            args: vec![state, control],
                            context: PostCallContext::Iterator {
                                base_register: frame.base_register,
                                register_a: a,
                                var_count: c,
                            },
                        });
                        Ok(StepResult::Continue)
                    }
                    Value::CFunction(cfunc) => {
                        let results = self.call_c_function(cfunc, &[state, control])?;
                        self.handle_iterator_results(tx, results, frame.base_register, a, c)?;
                        Ok(StepResult::Continue)
                    }
                    _ => Err(LuaError::TypeError(format!(
                        "attempt to call a {} value",
                        iter.type_name()
                    ))),
                }
            }
            
            _ => Err(LuaError::NotImplemented(format!("opcode {:?}", opcode))),
        }
    }
    
    // Helper methods
    
    /// Execute arithmetic operation
    fn execute_arithmetic<F>(
        &self,
        tx: &mut HeapTransaction,
        frame: &CallFrame,
        a: usize,
        b: usize,
        c: usize,
        op: F,
    ) -> Result<StepResult>
    where
        F: Fn(f64, f64) -> f64,
    {
        let base = frame.base_register as usize;
        let instr = tx.get_instruction(frame.closure.clone(), frame.pc - 1)?; // We already incremented PC
        
        let b_val = if instr.b_is_constant() {
            tx.get_constant(frame.closure.clone(), instr.b_as_constant())?
        } else {
            tx.read_register(self.current_thread.clone(), base + b)?
        };
        
        let c_val = if instr.c_is_constant() {
            tx.get_constant(frame.closure.clone(), instr.c_as_constant())?
        } else {
            tx.read_register(self.current_thread.clone(), base + c)?
        };
        
        match (self.to_number(&b_val), self.to_number(&c_val)) {
            (Some(b_num), Some(c_num)) => {
                let result = op(b_num, c_num);
                tx.set_register(self.current_thread.clone(), base + a, Value::Number(result));
                Ok(StepResult::Continue)
            }
            _ => Err(LuaError::TypeError(format!(
                "attempt to perform arithmetic on a {} and a {} value",
                b_val.type_name(),
                c_val.type_name()
            ))),
        }
    }
    
    /// Execute comparison operation
    fn execute_comparison<F>(
        &self,
        tx: &mut HeapTransaction,
        frame: &CallFrame,
        a: usize,
        b: usize,
        c: usize,
        op: F,
    ) -> Result<StepResult>
    where
        F: Fn(&Value, &Value) -> bool,
    {
        let base = frame.base_register as usize;
        let instr = tx.get_instruction(frame.closure.clone(), frame.pc - 1)?;
        
        let b_val = if instr.b_is_constant() {
            tx.get_constant(frame.closure.clone(), instr.b_as_constant())?
        } else {
            tx.read_register(self.current_thread.clone(), base + b)?
        };
        
        let c_val = if instr.c_is_constant() {
            tx.get_constant(frame.closure.clone(), instr.c_as_constant())?
        } else {
            tx.read_register(self.current_thread.clone(), base + c)?
        };
        
        let result = op(&b_val, &c_val);
        if result != (a != 0) {
            tx.increment_pc(self.current_thread.clone())?;
        }
        
        Ok(StepResult::Continue)
    }
    
    /// Lua less than comparison
    fn lua_lt(&self, a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => x < y,
            (Value::String(x), Value::String(y)) => {
                let x_bytes = self.heap.get_string_bytes(x.clone()).unwrap_or(&[]);
                let y_bytes = self.heap.get_string_bytes(y.clone()).unwrap_or(&[]);
                x_bytes < y_bytes
            }
            _ => false,
        }
    }
    
    /// Lua less than or equal comparison
    fn lua_le(&self, a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => x <= y,
            (Value::String(x), Value::String(y)) => {
                let x_bytes = self.heap.get_string_bytes(x.clone()).unwrap_or(&[]);
                let y_bytes = self.heap.get_string_bytes(y.clone()).unwrap_or(&[]);
                x_bytes <= y_bytes
            }
            _ => false,
        }
    }
    
    /// Convert value to number
    fn to_number(&self, value: &Value) -> Option<f64> {
        match value {
            Value::Number(n) => Some(*n),
            Value::String(s) => {
                let bytes = self.heap.get_string_bytes(s.clone()).ok()?;
                let s = std::str::from_utf8(bytes).ok()?;
                s.trim().parse().ok()
            }
            _ => None,
        }
    }
    
    /// Execute function call
    fn execute_call(
        &self,
        tx: &mut HeapTransaction,
        func: Value,
        args: Vec<Value>,
        base: usize,
        a: usize,
        expected_results: usize,
    ) -> Result<StepResult> {
        match func {
            Value::Closure(closure) => {
                tx.queue_operation(PendingOperation::FunctionCall {
                    closure,
                    args,
                    context: PostCallContext::Normal {
                        base_register: base as u16,
                        result_register: a,
                        expected_results,
                    },
                });
                Ok(StepResult::Continue)
            }
            Value::CFunction(cfunc) => {
                let results = self.call_c_function(cfunc, &args)?;
                
                // Store results
                for i in 0..expected_results.min(results.len()) {
                    tx.set_register(self.current_thread.clone(), base + a + i, results[i].clone());
                }
                
                // Fill remaining with nil
                for i in results.len()..expected_results {
                    tx.set_register(self.current_thread.clone(), base + a + i, Value::Nil);
                }
                
                Ok(StepResult::Continue)
            }
            _ => Err(LuaError::TypeError(format!(
                "attempt to call a {} value",
                func.type_name()
            ))),
        }
    }
    
    /// Call a C function
    fn call_c_function(&mut self, cfunc: CFunction, args: &[Value]) -> Result<Vec<Value>> {
        // Push arguments to stack
        let thread = self.heap.get_main_thread()?;
        let base = {
            let stack_size = self.heap.get_thread_stack_size(thread.clone())?;
            stack_size
        };
        
        {
            let mut tx = HeapTransaction::new(&mut self.heap);
            for arg in args {
                tx.push_stack(thread.clone(), arg.clone());
            }
            tx.commit()?;
        }
        
        // Create execution context
        let mut ctx = ExecutionContext {
            vm: self,
            base,
            arg_count: args.len(),
        };
        
        // Call function
        let ret_count = cfunc(&mut ctx)?;
        
        // Get results
        let mut results = Vec::new();
        for i in 0..ret_count as usize {
            let value = self.heap.get_thread_stack_value(thread.clone(), base + i)?;
            results.push(value);
        }
        
        // Clean up stack
        {
            let thread_obj = self.heap.get_thread_mut(thread.clone())?;
            thread_obj.stack.truncate(base);
        }
        
        Ok(results)
    }
    
    /// Handle __index metamethod
    fn handle_index_metamethod(
        &self,
        tx: &mut HeapTransaction,
        table: TableHandle,
        key: Value,
        result_register: usize,
    ) -> Result<StepResult> {
        let index_str = tx.create_string("__index")?;
        
        if let Some(metatable) = tx.get_metatable(table.clone())? {
            if let Ok(metamethod) = tx.read_table_field(metatable.clone(), &Value::String(index_str.clone())) {
                match metamethod {
                    Value::Table(meta_table) => {
                        // __index is a table, look up in it
                        match tx.read_table_field(meta_table.clone(), &key) {
                            Ok(value) => {
                                tx.set_register(self.current_thread.clone(), result_register, value);
                                Ok(StepResult::Continue)
                            }
                            Err(_) => {
                                tx.set_register(self.current_thread.clone(), result_register, Value::Nil);
                                Ok(StepResult::Continue)
                            }
                        }
                    }
                    Value::Closure(_) | Value::CFunction(_) => {
                        // __index is a function, queue call
                        tx.queue_operation(PendingOperation::MetamethodCall {
                            method_name: index_str,
                            object: Value::Table(table.clone()),
                            args: vec![Value::Table(table), key],
                            context: PostCallContext::Metamethod {
                                method: MetamethodType::Index,
                                base_register: result_register as u16,
                                result_register: 0,
                            },
                        });
                        Ok(StepResult::Continue)
                    }
                    _ => {
                        tx.set_register(self.current_thread.clone(), result_register, Value::Nil);
                        Ok(StepResult::Continue)
                    }
                }
            } else {
                tx.set_register(self.current_thread.clone(), result_register, Value::Nil);
                Ok(StepResult::Continue)
            }
        } else {
            tx.set_register(self.current_thread.clone(), result_register, Value::Nil);
            Ok(StepResult::Continue)
        }
    }
    
    /// Get metamethod
    fn get_metamethod(&self, value: &Value, method: StringHandle) -> Result<Value> {
        match value {
            Value::Table(handle) => self.heap.get_metamethod(handle.clone(), method.clone()),
            Value::UserData(handle) => {
                let userdata = self.heap.get_userdata(handle.clone())?;
                if let Some(ref mt) = userdata.metatable {
                    self.heap.get_table_field(mt.clone(), &Value::String(method.clone()))
                } else {
                    Ok(Value::Nil)
                }
            }
            _ => Ok(Value::Nil),
        }
    }
    
    /// Handle function return
    fn handle_return(&mut self, values: Vec<Value>, context: PostCallContext) -> Result<()> {
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        match context {
            PostCallContext::Normal { base_register, result_register, expected_results } => {
                // Store results
                for i in 0..expected_results.min(values.len()) {
                    tx.set_register(self.current_thread.clone(), base_register as usize + result_register + i, values[i].clone());
                }
                
                // Fill remaining with nil
                for i in values.len()..expected_results {
                    tx.set_register(self.current_thread.clone(), base_register as usize + result_register + i, Value::Nil);
                }
            }
            
            PostCallContext::Iterator { base_register, register_a, var_count } => {
                self.handle_iterator_results(&mut tx, values, base_register, register_a, var_count)?;
            }
            
            PostCallContext::Metamethod { method, base_register, result_register } => {
                match method {
                    MetamethodType::Index => {
                        let value = values.first().cloned().unwrap_or(Value::Nil);
                        tx.set_register(self.current_thread.clone(), base_register as usize + result_register, value);
                    }
                    _ => {
                        // Other metamethods
                        let value = values.first().cloned().unwrap_or(Value::Nil);
                        tx.set_register(self.current_thread.clone(), base_register as usize + result_register, value);
                    }
                }
            }
            
            PostCallContext::ConcatToString { remaining_values, current_index, base_register, target_register, mut accumulated } => {
                // Get string result
                if let Some(Value::String(s)) = values.first() {
                    let str_val = self.heap.get_string_value(*s)?;
                    accumulated.push(str_val);
                }
                
                // Continue with next value
                tx.queue_operation(PendingOperation::Concat {
                    values: remaining_values,
                    current_index: current_index + 1,
                    base_register,
                    target_register,
                    accumulated,
                });
            }
            
            PostCallContext::FinalResult => {
                // Nothing to do - result will be captured by main loop
            }
        }
        
        tx.commit()?;
        Ok(())
    }
    
    /// Handle iterator results
    fn handle_iterator_results(
        &self,
        tx: &mut HeapTransaction,
        results: Vec<Value>,
        base_register: u16,
        register_a: usize,
        var_count: usize,
    ) -> Result<()> {
        if let Some(first) = results.first() {
            if !first.is_nil() {
                // Update control variable
                tx.set_register(self.current_thread.clone(), base_register as usize + register_a + 2, first.clone());
                
                // Set loop variables
                for i in 0..var_count.min(results.len()) {
                    tx.set_register(self.current_thread.clone(), base_register as usize + register_a + 3 + i, results[i].clone());
                }
            } else {
                // End of iteration - skip next instruction
                tx.increment_pc(self.current_thread.clone())?;
            }
        } else {
            // No results - end iteration
            tx.increment_pc(self.current_thread.clone())?;
        }
        
        Ok(())
    }
    
    /// Process string concatenation
    fn process_concat(
        &mut self,
        values: Vec<Value>,
        current_index: usize,
        base_register: u16,
        target_register: usize,
        mut accumulated: Vec<String>,
    ) -> Result<()> {
        if current_index >= values.len() {
            // Done - create final string
            let result = accumulated.join("");
            let mut tx = HeapTransaction::new(&mut self.heap);
            let str_handle = tx.create_string(&result)?;
            tx.set_register(self.current_thread.clone(), base_register as usize + target_register, Value::String(str_handle));
            tx.commit()?;
            return Ok(());
        }

        let value = &values[current_index];
        
        match value {
            Value::String(s) => {
                // Create a new transaction for this operation to avoid borrow conflicts
                let str_val = {
                    let tx_result = self.heap.get_string_value(s.clone());
                    match tx_result {
                        Ok(val) => val,
                        Err(e) => return Err(e),
                    }
                };
                
                accumulated.push(str_val);
                
                // Continue with next
                self.pending_operations.push_back(PendingOperation::Concat {
                    values,
                    current_index: current_index + 1,
                    base_register,
                    target_register,
                    accumulated,
                });
            }
            Value::Number(n) => {
                accumulated.push(n.to_string());
                
                // Continue with next
                self.pending_operations.push_back(PendingOperation::Concat {
                    values,
                    current_index: current_index + 1,
                    base_register,
                    target_register,
                    accumulated,
                });
            }
            _ => {
                // Check for __tostring metamethod
                let tostring = {
                    // Get in a separate transaction
                    let result = self.heap.create_string_internal("__tostring");
                    match result {
                        Ok(handle) => handle,
                        Err(e) => return Err(e),
                    }
                };
                
                // Clone tostring to avoid it being moved
                let metamethod = self.get_metamethod(value, tostring.clone())?;
                
                if !metamethod.is_nil() {
                    // Queue metamethod call
                    let remaining_values = values[current_index+1..].to_vec();
                    self.pending_operations.push_back(PendingOperation::MetamethodCall {
                        method_name: tostring,
                        object: value.clone(),
                        args: vec![value.clone()],
                        context: PostCallContext::ConcatToString {
                            remaining_values,
                            current_index,
                            base_register,
                            target_register,
                            accumulated,
                        },
                    });
                } else {
                    return Err(LuaError::TypeError(format!(
                        "attempt to concatenate a {} value",
                        value.type_name()
                    )));
                }
            }
        }
        
        Ok(())
    }
    
    /// Push a call frame
    fn push_call_frame(&mut self, closure: ClosureHandle, args: &[Value]) -> Result<()> {
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Get base register
        let base_register = tx.get_stack_top(self.current_thread.clone())?;
        
        // Push arguments
        for arg in args {
            tx.push_stack(self.current_thread.clone(), arg.clone());
        }
        
        // Create call frame
        let frame = CallFrame {
            closure,
            pc: 0,
            base_register: base_register as u16,
            return_count: 1,
            frame_type: CallFrameType::Normal,
        };
        
        tx.push_call_frame(self.current_thread.clone(), frame)?;
        
        tx.commit()?;
        Ok(())
    }
    
    /// Pop a call frame
    fn pop_call_frame(&mut self) -> Result<CallFrame> {
        let mut tx = HeapTransaction::new(&mut self.heap);
        tx.pop_call_frame(self.current_thread.clone())?;
        tx.commit()?;
        
        // Get the popped frame from heap
        self.heap.get_current_frame(self.current_thread.clone())
            .map(|f| f.clone())
            .or_else(|_| {
                // If no current frame, return a dummy one
                let dummy_closure = ClosureHandle(super::arena::Handle::new(0, 0));
                Ok(CallFrame {
                    closure: dummy_closure,
                    pc: 0,
                    base_register: 0,
                    return_count: 1,
                    frame_type: CallFrameType::Normal,
                })
            })
    }
    
    // Public API methods
    
    /// Initialize standard library
    pub fn init_stdlib(&mut self) -> Result<()> {
        super::stdlib::register_stdlib(self)
    }
    
    /// Create a string
    pub fn create_string(&mut self, s: &str) -> Result<StringHandle> {
        self.heap.create_string_internal(s)
    }
    
    /// Create a table
    pub fn create_table(&mut self) -> Result<TableHandle> {
        self.heap.create_table()
    }
    
    /// Get globals table
    pub fn globals(&self) -> TableHandle {
        self.heap.get_globals().unwrap()
    }
    
    /// Set table value
    pub fn set_table(&mut self, table: TableHandle, key: Value, value: Value) -> Result<()> {
        self.heap.set_table_field_internal(table, key, value)
    }
    
    /// Get table value
    pub fn get_table(&self, table: TableHandle, key: &Value) -> Result<Value> {
        self.heap.get_table_field(table, key)
    }
    
    /// Get string length
    pub fn get_string_length(&self, handle: StringHandle) -> Result<usize> {
        let bytes = self.heap.get_string_bytes(handle)?;
        Ok(bytes.len())
    }
    
    /// Reset the VM
    pub fn reset(&mut self) -> Result<()> {
        self.pending_operations.clear();
        self.return_contexts.clear();
        self.instruction_count = 0;
        self.heap.reset_thread_internal(self.current_thread.clone())?;
        Ok(())
    }

    /// Get table length
    pub fn get_table_length(&self, handle: TableHandle) -> Result<usize> {
        let table = self.heap.get_table(handle)?;
        Ok(table.len())
    }

    /// Get table index value
    pub fn get_table_index(&self, table: TableHandle, index: usize) -> Result<Value> {
        self.get_table(table.clone(), &Value::Number(index as f64))
    }

    /// Set table index value
    pub fn set_table_index(&mut self, table: TableHandle, index: usize, value: Value) -> Result<()> {
        self.set_table(table.clone(), Value::Number(index as f64), value)
    }
}

impl Default for LuaVM {
    fn default() -> Self {
        Self::new().unwrap()
    }
}

/// Step result
#[derive(Debug)]
enum StepResult {
    /// Continue execution
    Continue,
    /// Return from function
    Return(Vec<Value>),
    /// Yield (coroutines)
    Yield(Value),
}

/// Execution status
#[derive(Debug)]
enum ExecutionStatus {
    /// Continue execution
    Continue,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_vm_creation() {
        let vm = LuaVM::new().unwrap();
        assert!(vm.heap.get_main_thread().is_ok());
    }
}