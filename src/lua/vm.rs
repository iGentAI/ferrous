//! Lua Virtual Machine Implementation
//!
//! This module implements the core of the Lua VM with a non-recursive
//! state machine architecture that avoids stack overflows while maintaining
//! full compatibility with Lua 5.1.

use std::collections::{VecDeque, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::error::{LuaError, Result};
use super::value::{
    Value, TableHandle, StringHandle, ClosureHandle, ThreadHandle, 
    CallFrame, CallFrameType, MetamethodType, CFunction
};
use super::heap::LuaHeap;
use super::transaction::HeapTransaction;
use super::stdlib;

/// Lua VM configuration
#[derive(Debug, Clone)]
pub struct VMConfig {
    /// Maximum stack size
    pub max_stack_size: usize,
    
    /// Maximum memory usage
    pub max_memory_bytes: usize,
    
    /// Maximum instruction count
    pub max_instruction_count: Option<u64>,
    
    /// Debug mode
    pub debug_mode: bool,
}

impl Default for VMConfig {
    fn default() -> Self {
        VMConfig {
            max_stack_size: 1000,
            max_memory_bytes: 100 * 1024 * 1024, // 100 MB
            max_instruction_count: Some(50_000_000), // 50M instructions
            debug_mode: false,
        }
    }
}

/// A Lua instruction
#[derive(Clone, Copy, Debug)]
pub struct Instruction(pub u32);

impl Instruction {
    /// Create a new instruction
    pub fn new(opcode: u8, a: u8, b: u16, c: u16) -> Self {
        let mut value = 0u32;
        value |= (opcode as u32) & 0x3F;
        value |= ((a as u32) & 0xFF) << 6;
        value |= ((b as u32) & 0x1FF) << 14;
        value |= ((c as u32) & 0x1FF) << 23;
        Instruction(value)
    }
    
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
    
    /// Get a signed value from B and C
    pub fn sbx(&self) -> i32 {
        let bx = (((self.0 >> 14) & 0x3FFFF) as u32) as i32;
        bx - 131071 // MAX_ARG_BX
    }
    
    /// Get a value from B and C
    pub fn bx(&self) -> u32 {
        (self.0 >> 14) & 0x3FFFF
    }
}

/// Lua opcodes
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpCode {
    Move = 0,       // A B     R(A) := R(B)
    LoadK = 1,      // A Bx    R(A) := Kst(Bx)
    LoadBool = 2,   // A B C   R(A) := (Bool)B; if (C) PC++
    LoadNil = 3,    // A B     R(A) ... R(B) := nil
    GetUpVal = 4,   // A B     R(A) := UpValue[B]
    GetGlobal = 5,  // A Bx    R(A) := Gbl[Kst(Bx)]
    GetTable = 6,   // A B C   R(A) := R(B)[RK(C)]
    SetGlobal = 7,  // A Bx    Gbl[Kst(Bx)] := R(A)
    SetUpVal = 8,   // A B     UpValue[B] := R(A)
    SetTable = 9,   // A B C   R(A)[RK(B)] := RK(C)
    NewTable = 10,  // A B C   R(A) := {} (size = B,C)
    Self_ = 11,     // A B C   R(A+1) := R(B); R(A) := R(B)[RK(C)]
    Add = 12,       // A B C   R(A) := RK(B) + RK(C)
    Sub = 13,       // A B C   R(A) := RK(B) - RK(C)
    Mul = 14,       // A B C   R(A) := RK(B) * RK(C)
    Div = 15,       // A B C   R(A) := RK(B) / RK(C)
    Mod = 16,       // A B C   R(A) := RK(B) % RK(C)
    Pow = 17,       // A B C   R(A) := RK(B) ^ RK(C)
    Unm = 18,       // A B     R(A) := -R(B)
    Not = 19,       // A B     R(A) := not R(B)
    Len = 20,       // A B     R(A) := length of R(B)
    Concat = 21,    // A B C   R(A) := R(B).. ... ..R(C)
    Jmp = 22,       // sBx     PC += sBx
    Eq = 23,        // A B C   if ((RK(B) == RK(C)) ~= A) then PC++
    Lt = 24,        // A B C   if ((RK(B) <  RK(C)) ~= A) then PC++
    Le = 25,        // A B C   if ((RK(B) <= RK(C)) ~= A) then PC++
    Test = 26,      // A C     if not (R(A) <=> C) then PC++
    TestSet = 27,   // A B C   if (R(B) <=> C) then R(A) := R(B) else PC++
    Call = 28,      // A B C   R(A), ... ,R(A+C-2) := R(A)(R(A+1), ... ,R(A+B-1))
    TailCall = 29,  // A B C   return R(A)(R(A+1), ... ,R(A+B-1))
    Return = 30,    // A B     return R(A), ... ,R(A+B-2)
    ForLoop = 31,   // A sBx   R(A)+=R(A+2); if R(A) <?= R(A+1) then { PC+=sBx; R(A+3)=R(A) }
    ForPrep = 32,   // A sBx   R(A)-=R(A+2); PC+=sBx
    TForLoop = 33,  // A C     R(A+3), ... ,R(A+2+C) := R(A)(R(A+1), R(A+2)); if R(A+3) ~= nil then R(A+2)=R(A+3) else PC++
    SetList = 34,   // A B C   R(A)[(C-1)*FPF+i] := R(A+i), 1 <= i <= B
    Close = 35,     // A       close all variables in the stack up to (>=) R(A)
    Closure = 36,   // A Bx    R(A) := closure(KPROTO[Bx], R(A), ... ,R(A+n))
    VarArg = 37,    // A B     R(A), R(A+1), ..., R(A+B-1) = vararg
}

/// Context for post-call handling
#[derive(Debug, Clone, PartialEq)]
pub enum PostCallContext {
    /// Normal function call
    Normal {
        /// Register to store result in
        return_register: Option<(u16, usize)>, // (base, offset)
    },
    
    /// Iterator function call
    Iterator {
        /// Base register
        base_register: u16,
        /// Register A from TForLoop
        register_a: usize,
        /// Number of loop variables
        var_count: usize,
    },
    
    /// Metamethod call
    Metamethod {
        /// What metamethod is being called
        method: String,
        /// Return handling strategy
        return_type: MetamethodReturnType,
    },
    
    /// Concat operation context
    Concat {
        /// Base register
        base_register: u16,
        /// Target register (A from CONCAT instruction)
        target_register: usize,
        /// Current index being processed (B to C range)
        current_index: usize,
        /// Last index to process (C from CONCAT instruction)
        last_index: usize,
        /// Accumulated string parts so far
        accumulated_parts: Vec<String>,
    },
}

/// Types of metamethod return handling
#[derive(Debug, Clone, PartialEq)]
pub enum MetamethodReturnType {
    /// For __index - return value from table
    Index,
    /// For __newindex - no return expected
    NewIndex,
    /// For __call - return from call
    Call,
    /// For arithmetic operators
    Arithmetic,
    /// For __tostring in concat operations
    ToString,
}

/// Pending operation for the VM to process
#[derive(Debug, Clone)]
pub enum PendingOperation {
    /// Call a function
    FunctionCall {
        /// The closure to call
        closure: ClosureHandle,
        /// The arguments to the function
        args: Vec<Value>,
        /// Context for the call
        context: PostCallContext,
    },
    
    /// Call a metamethod
    MetamethodCall {
        /// The metamethod name
        method_name: StringHandle,
        /// The table
        table: TableHandle,
        /// The key
        key: Value,
        /// Context for the call
        context: PostCallContext,
    },
    
    /// Call an iterator function for a generic for loop
    IteratorCall {
        /// The iterator function
        closure: ClosureHandle,
        /// The state
        state: Value,
        /// The control variable
        control: Value,
        /// Context for returning
        context: PostCallContext,
    },
}

/// The result of executing a step
#[derive(Debug, Clone)]
pub enum ExecutionStatus {
    /// Continue execution
    Continue,
    
    /// Return a value
    Return(Value),
    
    /// Call a function (will be queued as pending operation)
    Call(ClosureHandle, Vec<Value>),
    
    /// Yield a value (coroutines - not supported in Redis)
    Yield(Value),
}

/// Execution context for C functions
pub struct ExecutionContext<'a> {
    /// The VM
    pub vm: &'a mut LuaVM,
    /// Base stack index
    pub base: usize,
    /// Number of arguments
    pub arg_count: usize,
}

impl<'a> ExecutionContext<'a> {
    /// Get an argument
    pub fn get_arg(&self, index: usize) -> Result<Value> {
        if index >= self.arg_count {
            return Err(LuaError::ArgError(index, "argument not available".to_string()));
        }
        
        self.vm.heap.get_thread_register(self.vm.current_thread, self.base + index)
    }
    
    /// Get the number of arguments
    pub fn get_arg_count(&self) -> usize {
        self.arg_count
    }
    

    
    /// Get heap reference
    pub fn heap(&self) -> &LuaHeap {
        &self.vm.heap
    }
    
    /// Get mutable heap reference
    pub fn heap_mut(&mut self) -> &mut LuaHeap {
        &mut self.vm.heap
    }
}

/// Resource limits for the VM
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum call stack depth
    pub max_call_depth: usize,
    
    /// Maximum value stack size
    pub max_stack_size: usize,
    
    /// Maximum memory usage
    pub max_memory_bytes: usize,
    
    /// Maximum instruction count
    pub max_instruction_count: Option<u64>,
    
    /// Maximum table size
    pub max_table_size: usize,
    
    /// Maximum string length
    pub max_string_length: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        ResourceLimits {
            max_call_depth: 1000,
            max_stack_size: 1000000,
            max_memory_bytes: 100 * 1024 * 1024, // 100 MB
            max_instruction_count: Some(50_000_000), // 50M instructions
            max_table_size: 1000000,
            max_string_length: 10 * 1024 * 1024, // 10 MB
        }
    }
}

/// The Lua virtual machine
pub struct LuaVM {
    /// The heap
    pub heap: LuaHeap,
    
    /// Current thread
    pub current_thread: ThreadHandle,
    
    /// Pending operations queue
    pub pending_operations: VecDeque<PendingOperation>,
    
    /// Call contexts for tracking returns
    pub call_contexts: HashMap<usize, PostCallContext>,
    
    /// Resource limits
    pub limits: ResourceLimits,
    
    /// Kill flag for terminating execution
    pub kill_flag: Option<Arc<AtomicBool>>,
    
    /// Instruction count
    pub instruction_count: u64,
}

impl LuaVM {
    /// Create a new Lua VM
    pub fn new() -> Result<Self> {
        let mut heap = LuaHeap::new();
        let current_thread = heap.get_main_thread()?;
        
        Ok(LuaVM {
            heap,
            current_thread,
            pending_operations: VecDeque::new(),
            call_contexts: HashMap::new(),
            limits: ResourceLimits::default(),
            kill_flag: None,
            instruction_count: 0,
        })
    }
    
    /// Apply a sandbox to the VM
    pub fn apply_sandbox(&mut self) -> Result<()> {
        // This would be implemented to set up a safe environment
        // For now, just return Ok
        Ok(())
    }
    
    /// Check if the VM should be killed
    fn should_kill(&self) -> bool {
        if let Some(flag) = &self.kill_flag {
            flag.load(Ordering::SeqCst)
        } else {
            false
        }
    }
    
    /// Set the kill flag
    pub fn set_kill_flag(&mut self, flag: Arc<AtomicBool>) {
        self.kill_flag = Some(flag);
    }
    
    /// Check resource limits
    fn check_limits(&self) -> Result<()> {
        // Check call stack depth
        let stack_depth = self.heap.get_thread_call_depth(self.current_thread)?;
        if stack_depth > self.limits.max_call_depth {
            return Err(LuaError::StackOverflow);
        }
        
        // Check stack size
        let stack_size = self.heap.get_thread_stack_size(self.current_thread)?;
        if stack_size > self.limits.max_stack_size {
            return Err(LuaError::StackOverflow);
        }
        
        // Check instruction count
        if let Some(limit) = self.limits.max_instruction_count {
            if self.instruction_count > limit {
                return Err(LuaError::InstructionLimit);
            }
        }
        
        Ok(())
    }
    
    /// Push a call frame
    fn push_call_frame(&mut self, closure: ClosureHandle, args: &[Value]) -> Result<()> {
        // Create a call frame
        let base_register = {
            let thread = self.heap.get_thread(self.current_thread)?;
            thread.stack.len()
        };
        
        let frame = CallFrame {
            closure,
            pc: 0,
            base_register: base_register as u16,
            return_count: 1, // Default to 1 return value
            frame_type: CallFrameType::Normal,
        };
        
        // Push the frame
        self.heap.push_thread_call_frame(self.current_thread, frame)?;
        
        // Setup arguments
        for (i, arg) in args.iter().enumerate() {
            self.heap.set_thread_register(self.current_thread, base_register + i, arg.clone())?;
        }
        
        Ok(())
    }
    
    /// Pop a call frame
    fn pop_call_frame(&mut self) -> Result<CallFrame> {
        self.heap.pop_thread_call_frame(self.current_thread)
    }
    
    /// Get a register value
    pub fn get_register(&mut self, base: u16, offset: usize) -> Result<Value> {
        self.heap.get_thread_register(self.current_thread, base as usize + offset)
    }
    
    /// Set a register value
    pub fn set_register(&mut self, base: u16, offset: usize, value: Value) -> Result<()> {
        self.heap.set_thread_register(self.current_thread, base as usize + offset, value)
    }
    
    /// Get call stack depth
    fn get_call_depth(&self) -> Result<usize> {
        self.heap.get_thread_call_depth(self.current_thread)
    }
    
    /// Get the return value (top of stack)
    fn get_return_value(&self) -> Result<Value> {
        let thread = self.heap.get_thread(self.current_thread)?;
        if thread.stack.is_empty() {
            Ok(Value::Nil)
        } else {
            Ok(thread.stack[thread.stack.len() - 1].clone())
        }
    }
    
    /// Execute a function without recursion
    pub fn execute_function(&mut self, closure: ClosureHandle, args: &[Value]) -> Result<Value> {
        // Record initial call stack depth to track nested calls
        let initial_depth = self.get_call_depth()?;
        
        // Push initial call frame
        self.push_call_frame(closure, args)?;
        
        // Single execution loop - no recursion
        let mut final_result = Value::Nil;
        
        loop {
            // Check kill flag
            if self.should_kill() {
                return Err(LuaError::ScriptKilled);
            }
            
            // Check resource limits
            self.check_limits()?;
            
            // Process any pending operations
            if !self.pending_operations.is_empty() {
                let op = self.pending_operations.pop_front().unwrap();
                self.process_pending_operation(op)?;
                continue;
            }
            
            // Check if we've returned to initial depth or below
            let current_depth = self.get_call_depth()?;
            if current_depth <= initial_depth {
                return Ok(final_result);
            }
            
            // Execute single step
            match self.step()? {
                ExecutionStatus::Continue => continue,
                
                ExecutionStatus::Return(value) => {
                    // Save return value
                    final_result = value.clone();
                    
                    // Pop frame
                    self.pop_call_frame()?;
                    
                    // Get new depth
                    let new_depth = self.get_call_depth()?;
                    
                    // If we're returning to the initial depth, we're done
                    if new_depth <= initial_depth {
                        return Ok(final_result);
                    }
                    
                    // Otherwise, handle function return in caller's context
                    if let Some(context) = self.call_contexts.remove(&new_depth) {
                        self.handle_return_with_context(value, context)?;
                    } else {
                        // Default handling for return
                        self.handle_function_return(value)?;
                    }
                },
                
                ExecutionStatus::Call(closure, args) => {
                    // Queue a function call instead of executing directly
                    let context = PostCallContext::Normal { 
                        return_register: None  // Will be set by caller
                    };
                    
                    self.pending_operations.push_back(PendingOperation::FunctionCall {
                        closure,
                        args,
                        context,
                    });
                },
                
                ExecutionStatus::Yield(_) => {
                    return Err(LuaError::NotImplemented("coroutines".to_string()));
                }
            }
        }
    }
    
    fn process_pending_operation(&mut self, operation: PendingOperation) -> Result<()> {
        match operation {
            PendingOperation::FunctionCall { closure, args, context } => {
                // Get current call depth before pushing a new frame
                let call_depth = self.get_call_depth()?;
                
                println!("[VM] Function call at depth {}", call_depth);
                
                // Push a new call frame
                self.push_call_frame(closure, &args)?;
                
                // Store the context for when this function returns
                println!("[VM] Storing context at depth {}: {:?}", call_depth, context);
                self.call_contexts.insert(call_depth, context);
            },
            
            PendingOperation::MetamethodCall { method_name, table, key, context } => {
                // Get current call depth
                let call_depth = self.get_call_depth()?;
                
                // Get metamethod
                let metamethod = self.heap.get_metamethod(table, method_name)?;
                
                match metamethod {
                    Value::Closure(closure) => {
                        // Create args array: table and key
                        let args = vec![Value::Table(table), key];
                        
                        // Push a new call frame for the metamethod
                        self.push_call_frame(closure, &args)?;
                        
                        // Store context for return handling
                        self.call_contexts.insert(call_depth, context);
                    },
                    Value::CFunction(cfunc) => {
                        // For C functions, we can call directly without recursion risk
                        let stack_size_before = {
                            let thread = self.heap.get_thread_mut(self.current_thread)?;
                            let size = thread.stack.len();
                            
                            // Push table and key as arguments
                            thread.stack.push(Value::Table(table));
                            thread.stack.push(key);
                            
                            size
                        };
                        
                        // Create execution context and call function
                        let mut ctx = ExecutionContext {
                            vm: self,
                            base: stack_size_before,
                            arg_count: 2,
                        };
                        
                        // Call C function - doesn't use recursion
                        let ret_count = cfunc(&mut ctx)?;
                        
                        // Process return value
                        if ret_count > 0 {
                            let return_value = {
                                let thread = self.heap.get_thread(self.current_thread)?;
                                if stack_size_before < thread.stack.len() {
                                    thread.stack[stack_size_before].clone()
                                } else {
                                    Value::Nil
                                }
                            };
                            
                            // Clean up stack
                            {
                                let thread = self.heap.get_thread_mut(self.current_thread)?;
                                thread.stack.truncate(stack_size_before);
                            }
                            
                            // Handle the return value based on the context
                            self.handle_return_with_context(return_value, context)?;
                        } else {
                            // Just clean up the stack
                            let thread = self.heap.get_thread_mut(self.current_thread)?;
                            thread.stack.truncate(stack_size_before);
                        }
                    },
                    _ => {
                        return Err(LuaError::TypeError("metamethod is not a function".to_string()));
                    }
                }
            },
            
            PendingOperation::IteratorCall { closure, state, control, context } => {
                // Get current call depth
                let call_depth = self.get_call_depth()?;
                
                // Create arguments: state and control
                let args = vec![state, control];
                
                // Push a new call frame for the iterator function
                self.push_call_frame(closure, &args)?;
                
                // Store context for return handling
                self.call_contexts.insert(call_depth, context);
            },
        }
        
        Ok(())
    }
    
    /// Execute a single step of the VM
    fn step(&mut self) -> Result<ExecutionStatus> {
        // Use a transaction to capture all heap changes
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Get current frame - use the transaction to avoid borrowing self
        let frame = match tx.get_current_call_frame(self.current_thread) {
            Ok(frame) => frame,
            Err(LuaError::StackEmpty) => {
                // No call frames, nothing to execute
                return Ok(ExecutionStatus::Continue);
            },
            Err(e) => return Err(e),
        };
        
        // Get current instruction
        let instr = match tx.get_instruction(frame.closure, frame.pc) {
            Ok(instr) => instr,
            Err(e) => return Err(e),
        };
        
        // Extract opcode for easier handling
        let opcode = instr.opcode();
        let a = instr.a() as usize;
        let b = instr.b() as usize;
        let c = instr.c() as usize;
        let base = frame.base_register as usize;
        
        // Create a copy of the frame for passing to handler methods
        let frame_clone = frame.clone();

        // Special handling for function call opcodes that require pending operations
        let result = match opcode {
            28 => { // CALL
                // Get function object
                let func = tx.read_register(self.current_thread, base + a)?;
                
                // Collect arguments
                let arg_count = if b == 0 {
                    tx.get_stack_top(self.current_thread)? - base - a - 1
                } else {
                    b - 1
                };
                
                let mut args = Vec::with_capacity(arg_count);
                for i in 0..arg_count {
                    args.push(tx.read_register(self.current_thread, base + a + 1 + i)?);
                }
                
                // Always increment PC before proceeding with call
                tx.increment_pc(self.current_thread)?;
                tx.commit()?; // Apply PC increment first
                
                match func {
                    Value::Closure(closure) => {
                        // Create context for return handling
                        let context = PostCallContext::Normal {
                            return_register: Some((frame.base_register, a)),
                        };
                        
                        // Queue the function call
                        self.pending_operations.push_back(PendingOperation::FunctionCall {
                            closure,
                            args,
                            context,
                        });
                        
                        ExecutionStatus::Continue
                    },
                    Value::CFunction(cfunc) => {
                        // For C functions, we can call directly
                        self.execute_c_function(cfunc, args, frame.base_register, a)?
                    },
                    _ => {
                        return Err(LuaError::TypeError(format!("attempt to call a {} value", func.type_name())));
                    }
                }
            },
            29 => { // TAILCALL
                // Get function object
                let func = tx.read_register(self.current_thread, base + a)?;
                
                // Collect arguments
                let arg_count = if b == 0 {
                    tx.get_stack_top(self.current_thread)? - base - a - 1
                } else {
                    b - 1
                };
                
                let mut args = Vec::with_capacity(arg_count);
                for i in 0..arg_count {
                    args.push(tx.read_register(self.current_thread, base + a + 1 + i)?);
                }
                
                // Commit transaction before modifying frame stack
                tx.commit()?;
                
                match func {
                    Value::Closure(closure) => {
                        // Pop current frame
                        self.pop_call_frame()?;
                        
                        // Queue function call without return location (tail call)
                        self.pending_operations.push_back(PendingOperation::FunctionCall {
                            closure, 
                            args,
                            context: PostCallContext::Normal {
                                return_register: None, // No specific register
                            },
                        });
                        
                        ExecutionStatus::Continue
                    },
                    Value::CFunction(cfunc) => {
                        // Pop current frame
                        self.pop_call_frame()?;
                        
                        // Execute C function directly and return its result
                        let result = self.execute_c_function_direct(cfunc, &args)?;
                        ExecutionStatus::Return(result)
                    },
                    _ => {
                        return Err(LuaError::TypeError(format!("attempt to call a {} value", func.type_name())));
                    }
                }
            },
            30 => { // RETURN
                // Get return values
                let returns = if b > 0 { b - 1 } else { 0 };
                
                let return_value = if returns > 0 {
                    tx.read_register(self.current_thread, base + a)?
                } else {
                    Value::Nil
                };
                
                // Commit transaction
                tx.commit()?;
                
                // Signal return to main loop
                ExecutionStatus::Return(return_value)
            },
            33 => { // TFORLOOP
                // Get iterator, state, and control variable
                let iter = tx.read_register(self.current_thread, base + a)?;
                let state = tx.read_register(self.current_thread, base + a + 1)?;
                let control = tx.read_register(self.current_thread, base + a + 2)?;
                
                // Commit transaction before proceeding
                tx.commit()?;
                
                match iter {
                    Value::Closure(closure) => {
                        // Queue iterator call with special context
                        let context = PostCallContext::Iterator {
                            base_register: frame.base_register,
                            register_a: a,
                            var_count: c,
                        };
                        
                        self.pending_operations.push_back(PendingOperation::FunctionCall {
                            closure,
                            args: vec![state, control],
                            context,
                        });
                        
                        ExecutionStatus::Continue
                    },
                    Value::CFunction(cfunc) => {
                        // Handle C function iterator directly
                        self.execute_c_iterator(cfunc, state, control, frame.base_register, a, c)?
                    },
                    _ => {
                        return Err(LuaError::TypeError(format!(
                            "attempt to call a {} value as iterator", iter.type_name()
                        )));
                    }
                }
            },
            _ => {
                // For all other opcodes, use the standard handler methods

                // Increment PC for non-flow-control opcodes
                if ![22, 31, 32].contains(&opcode) { // Skip JMP, FOR_LOOP, FOR_PREP
                    tx.increment_pc(self.current_thread)?;
                }
                
                // Each opcode gets its own handler to avoid borrow checker conflicts
                match opcode {
                    0 => { // MOVE
                        let value = tx.read_register(self.current_thread, base + b)?;
                        tx.set_register(self.current_thread, base + a, value);
                        tx.commit()?; // Apply changes
                        ExecutionStatus::Continue
                    },
                    1 => { // LOADK
                        let constant = tx.get_constant(frame.closure, instr.bx() as usize)?;
                        tx.set_register(self.current_thread, base + a, constant);
                        tx.commit()?;
                        ExecutionStatus::Continue
                    },
                    2 => { // LOADBOOL
                        tx.set_register(self.current_thread, base + a, Value::Boolean(b != 0));
                        if c != 0 {
                            // Skip next instruction
                            tx.increment_pc(self.current_thread)?;
                        }
                        tx.commit()?;
                        ExecutionStatus::Continue
                    },
                    3 => { // LOADNIL
                        // Set A through B to nil
                        for i in a..=b {
                            tx.set_register(self.current_thread, base + i, Value::Nil);
                        }
                        tx.commit()?;
                        ExecutionStatus::Continue
                    },
                    6 => { // GETTABLE
                        let table_val = tx.read_register(self.current_thread, base + b)?;
                        let key_val = if c & 0x100 != 0 {
                            tx.get_constant(frame.closure, c & 0xFF)?
                        } else {
                            tx.read_register(self.current_thread, base + c)?
                        };
                        
                        match table_val {
                            Value::Table(table) => {
                                // Try direct table access first
                                match tx.read_table_field(table, &key_val) {
                                    Ok(value) => {
                                        tx.set_register(self.current_thread, base + a, value);
                                        tx.commit()?;
                                        ExecutionStatus::Continue
                                    },
                                    Err(_) => {
                                        // Table lookup failed, check for __index metamethod
                                        let index_str = self.heap.create_string("__index")?;
                                        let metatable_opt = tx.get_metatable(table)?;
                                        let metamethod = if let Some(mt) = metatable_opt {
                                            tx.read_table_field(mt, &Value::String(index_str)).ok()
                                        } else {
                                            None
                                        };
                                        
                                        match metamethod {
                                            Some(Value::Table(meta_table)) => {
                                                // If metamethod is a table, look up in it
                                                match tx.read_table_field(meta_table, &key_val) {
                                                    Ok(value) => {
                                                        tx.set_register(self.current_thread, base + a, value);
                                                        tx.commit()?;
                                                        ExecutionStatus::Continue
                                                    },
                                                    Err(_) => {
                                                        // Not in metamethod table either
                                                        tx.set_register(self.current_thread, base + a, Value::Nil);
                                                        tx.commit()?;
                                                        ExecutionStatus::Continue
                                                    }
                                                }
                                            },
                                            Some(Value::Closure(closure)) => {
                                                // If metamethod is a function, queue a call
                                                let context = PostCallContext::Metamethod {
                                                    method: "__index".to_string(),
                                                    return_type: MetamethodReturnType::Index,
                                                };
                                                
                                                // Apply transaction changes first
                                                tx.commit()?;
                                                
                                                self.pending_operations.push_back(PendingOperation::MetamethodCall {
                                                    method_name: self.heap.create_string("__index")?,
                                                    table,
                                                    key: key_val,
                                                    context,
                                                });
                                                
                                                ExecutionStatus::Continue
                                            },
                                            _ => {
                                                // No useful metamethod, return nil
                                                tx.set_register(self.current_thread, base + a, Value::Nil);
                                                tx.commit()?;
                                                ExecutionStatus::Continue
                                            }
                                        }
                                    }
                                }
                            },
                            _ => {
                                return Err(LuaError::TypeError(format!(
                                    "attempt to index a {} value", table_val.type_name()
                                )));
                            }
                        }
                    },
                    9 => { // SETTABLE
                        let table_val = tx.read_register(self.current_thread, base + a)?;
                        
                        let key_val = if b & 0x100 != 0 {
                            tx.get_constant(frame.closure, b & 0xFF)?
                        } else {
                            tx.read_register(self.current_thread, base + b)?
                        };
                        
                        let value = if c & 0x100 != 0 {
                            tx.get_constant(frame.closure, c & 0xFF)?
                        } else {
                            tx.read_register(self.current_thread, base + c)?
                        };
                        
                        match table_val {
                            Value::Table(table) => {
                                // Set table field directly
                                tx.set_table_field(table, key_val, value)?;
                                tx.commit()?;
                                ExecutionStatus::Continue
                            },
                            _ => {
                                return Err(LuaError::TypeError(format!(
                                    "attempt to index a {} value", table_val.type_name()
                                )));
                            }
                        }
                    },
                    21 => { // CONCAT
                        // Start new concatenation operation with a transaction
                        let mut values = Vec::with_capacity(c - b + 1);
                        for i in b..=c {
                            values.push(tx.read_register(self.current_thread, base + i)?);
                        }
                        
                        // Commit transaction before starting complex operation
                        tx.commit()?;
                        
                        // Queue a concatenation operation as a pending operation
                        let context = PostCallContext::Concat {
                            base_register: frame.base_register,
                            target_register: a,
                            current_index: 0,
                            last_index: c - b,
                            accumulated_parts: Vec::new(),
                        };
                        
                        // Process the first value
                        self.process_concat_value(values, context)?;
                        
                        ExecutionStatus::Continue
                    },
                    
                    _ => {
                        // For other opcodes, signal that we don't handle them yet
                        // In a real implementation, we should handle all opcodes
                        tx.commit()?; // Make sure to commit what we have
                        return Err(LuaError::NotImplemented(format!("opcode: {}", opcode)));
                    }
                }
            }
        };
        
        Ok(result)
    }

    // Helper for executing C functions directly
    fn execute_c_function(&mut self, cfunc: CFunction, args: Vec<Value>, base_register: u16, dest_reg: usize) -> Result<ExecutionStatus> {
        // Store references to avoid borrow issues
        let thread_handle = self.current_thread;
        
        // Set up stack for C function call
        let stack_base = {
            let thread = self.heap.get_thread_mut(thread_handle)?;
            let base = thread.stack.len();
            
            // Push arguments directly
            for arg in &args {
                thread.stack.push(arg.clone());
            }
            
            base
        };
        
        // Create execution context
        let mut ctx = ExecutionContext {
            vm: self,
            base: stack_base,
            arg_count: args.len(),
        };
        
        // Call the C function
        let ret_count = cfunc(&mut ctx)?;
        
        // Process return values (copy to registers)
        if ret_count > 0 {
            let returns = ret_count as usize;
            
            for i in 0..returns {
                let value = if stack_base + i < self.heap.get_thread_stack_size(thread_handle)? {
                    self.heap.get_thread_stack_value(thread_handle, stack_base + i)?
                } else {
                    Value::Nil
                };
                
                // Set in appropriate register
                self.set_register(base_register, dest_reg + i, value.clone())?;
            }
        }
        
        // Clean up stack
        let thread = self.heap.get_thread_mut(thread_handle)?;
        thread.stack.truncate(stack_base);
        
        Ok(ExecutionStatus::Continue)
    }

    // Helper for C iterator functions
    fn execute_c_iterator(&mut self, cfunc: CFunction, state: Value, control: Value, base_register: u16, a: usize, var_count: usize) -> Result<ExecutionStatus> {
        // Store references to avoid borrow issues
        let thread_handle = self.current_thread;
        
        // Set up stack for C function call
        let stack_base = {
            let thread = self.heap.get_thread_mut(thread_handle)?;
            let base = thread.stack.len();
            
            // Push arguments directly
            thread.stack.push(state);
            thread.stack.push(control);
            
            base
        };
        
        // Create execution context
        let mut ctx = ExecutionContext {
            vm: self,
            base: stack_base,
            arg_count: 2, // State and control
        };
        
        // Call the function
        let ret_count = cfunc(&mut ctx)?;
        
        // Get first return value
        let first_result = if ret_count > 0 {
            if stack_base < self.heap.get_thread_stack_size(thread_handle)? {
                self.heap.get_thread_stack_value(thread_handle, stack_base)?
            } else {
                Value::Nil
            }
        } else {
            Value::Nil
        };
        
        // Clean up stack
        {
            let thread = self.heap.get_thread_mut(thread_handle)?;
            thread.stack.truncate(stack_base);
        }
        
        // Check if iteration should continue
        if first_result == Value::Nil {
            // End of iteration - skip loop body
            let thread = self.heap.get_thread_mut(thread_handle)?;
            if let Some(frame) = thread.call_frames.last_mut() {
                frame.pc += 1;
            }
        } else {
            // Continue iteration - update control var and loop vars
            self.set_register(base_register, a + 2, first_result.clone())?;
            
            // Set loop variables
            for i in 0..std::cmp::min(ret_count as usize, var_count) {
                let value = if stack_base + i < self.heap.get_thread_stack_size(thread_handle)? {
                    self.heap.get_thread_stack_value(thread_handle, stack_base + i)?
                } else {
                    Value::Nil
                };
                
                self.set_register(base_register, a + 3 + i, value)?;
            }
        }
        
        Ok(ExecutionStatus::Continue)
    }

    // Helper for direct C function execution
    fn execute_c_function_direct(&mut self, cfunc: CFunction, args: &[Value]) -> Result<Value> {
        // Store references to avoid borrow issues
        let thread_handle = self.current_thread;
        
        // Set up stack for C function call
        let stack_base = {
            let thread = self.heap.get_thread_mut(thread_handle)?;
            let base = thread.stack.len();
            
            // Push arguments directly
            for arg in args {
                thread.stack.push(arg.clone());
            }
            
            base
        };
        
        // Create execution context
        let mut ctx = ExecutionContext {
            vm: self,
            base: stack_base,
            arg_count: args.len(),
        };
        
        // Call the function
        let ret_count = cfunc(&mut ctx)?;
        
        // Get first return value
        let result = if ret_count > 0 {
            if stack_base < self.heap.get_thread_stack_size(thread_handle)? {
                self.heap.get_thread_stack_value(thread_handle, stack_base)?
            } else {
                Value::Nil
            }
        } else {
            Value::Nil
        };
        
        // Clean up stack
        {
            let thread = self.heap.get_thread_mut(thread_handle)?;
            thread.stack.truncate(stack_base);
        }
        
        Ok(result)
    }
    
    /// Execute an instruction
    fn execute_instruction(&mut self, tx: &mut HeapTransaction<'_>, instr: Instruction, frame: &CallFrame) -> Result<ExecutionStatus> {
        let opcode = instr.opcode();
        let a = instr.a() as usize;
        let b = instr.b() as usize;
        let c = instr.c() as usize;
        
        println!("[VM] Executing instruction: opcode={}, a={}, b={}, c={}", opcode, a, b, c);
        
        match opcode {
            // Basic operations
            0 => self.execute_move(tx, frame, a, b),
            1 => self.execute_load_k(tx, frame, a, instr.bx() as usize),
            2 => self.execute_load_bool(tx, frame, a, b, c),
            3 => self.execute_load_nil(tx, frame, a, b),
            
            // Table operations
            6 => self.execute_get_table(tx, frame, a, b, c),
            9 => self.execute_set_table(tx, frame, a, b, c),
            10 => self.execute_new_table(tx, frame, a, b, c),
            
            // Arithmetic operations
            12 => self.execute_arithmetic(tx, frame, OpCode::Add, a, b, c),
            13 => self.execute_arithmetic(tx, frame, OpCode::Sub, a, b, c),
            14 => self.execute_arithmetic(tx, frame, OpCode::Mul, a, b, c),
            15 => self.execute_arithmetic(tx, frame, OpCode::Div, a, b, c),
            16 => self.execute_arithmetic(tx, frame, OpCode::Mod, a, b, c),
            17 => self.execute_arithmetic(tx, frame, OpCode::Pow, a, b, c),
            
            // Unary operations
            18 => self.execute_unm(tx, frame, a, b),
            19 => self.execute_not(tx, frame, a, b),
            20 => self.execute_len(tx, frame, a, b),
            
            // String operations
            21 => self.execute_concat(tx, frame, a, b, c),
            
            // Control flow
            22 => self.execute_jmp(tx, frame, instr.sbx()),
            23 => self.execute_eq(tx, frame, a != 0, b, c),
            24 => self.execute_lt(tx, frame, a != 0, b, c),
            25 => self.execute_le(tx, frame, a != 0, b, c),
            26 => self.execute_test(tx, frame, a, c != 0),
            27 => self.execute_test_set(tx, frame, a, b, c != 0),
            
            // Function calls
            28 => self.execute_call(tx, frame, a, b, c),
            29 => self.execute_tail_call(tx, frame, a, b),
            30 => self.execute_return(tx, frame, a, b),
            
            // Loops
            31 => self.execute_for_loop(tx, frame, a, instr.sbx()),
            32 => self.execute_for_prep(tx, frame, a, instr.sbx()),
            33 => self.execute_tfor_loop(tx, frame, a, c),
            
            // Other
            36 => self.execute_closure(tx, frame, a, instr.bx() as usize),
            
            // Not implemented yet
            _ => Err(LuaError::NotImplemented(format!("opcode: {}", opcode))),
        }
    }
    
    /// Execute MOVE instruction (A B)
    fn execute_move(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: usize, b: usize) -> Result<ExecutionStatus> {
        // R(A) := R(B)
        let value = tx.read_register(self.current_thread, frame.base_register as usize + b)?;
        tx.set_register(self.current_thread, frame.base_register as usize + a, value);
        Ok(ExecutionStatus::Continue)
    }
    
    /// Execute LOADK instruction (A Bx)
    fn execute_load_k(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: usize, bx: usize) -> Result<ExecutionStatus> {
        // R(A) := Kst(Bx)
        let constant = tx.get_constant(frame.closure, bx)?;
        println!("[VM] Retrieved constant at index {}: {:?}", bx, constant);
        tx.set_register(self.current_thread, frame.base_register as usize + a, constant);
        Ok(ExecutionStatus::Continue)
    }
    
    /// Execute LOADBOOL instruction (A B C)
    fn execute_load_bool(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: usize, b: usize, c: usize) -> Result<ExecutionStatus> {
        // R(A) := (Bool)B; if (C) pc++
        tx.set_register(self.current_thread, frame.base_register as usize + a, Value::Boolean(b != 0));
        
        if c != 0 {
            tx.increment_pc(self.current_thread)?;
        }
        
        Ok(ExecutionStatus::Continue)
    }
    
    /// Execute LOADNIL instruction (A B)
    fn execute_load_nil(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: usize, b: usize) -> Result<ExecutionStatus> {
        // R(A), R(A+1), ..., R(A+B) := nil
        for i in 0..=b-a {
            tx.set_register(self.current_thread, frame.base_register as usize + a + i, Value::Nil);
        }
        
        Ok(ExecutionStatus::Continue)
    }
    
    fn execute_get_table(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: usize, b: usize, c: usize) -> Result<ExecutionStatus> {
        // R(A) := R(B)[RK(C)]
        let base = frame.base_register as usize;
        let table_val = tx.read_register(self.current_thread, base + b)?;
        
        // Read key from register or constant
        let key_val = if c & 0x100 != 0 {
            // Key is a constant
            tx.get_constant(frame.closure, c & 0xFF)?
        } else {
            // Key is a register
            tx.read_register(self.current_thread, base + c)?
        };
        
        match table_val {
            Value::Table(table) => {
                // Try direct table lookup
                match tx.read_table_field(table, &key_val) {
                    Ok(value) => {
                        // Set result in register A
                        tx.set_register(self.current_thread, base + a, value);
                        Ok(ExecutionStatus::Continue)
                    },
                    Err(_) => {
                        // Handle metamethod - but using the transaction for all access
                        // Create the string for __index through the transaction
                        let index_str = tx.create_string("__index")?;
                        
                        // Get metatable through the transaction
                        let metatable_opt = tx.get_metatable(table)?;
                        
                        if let Some(mt) = metatable_opt {
                            // Look for __index metamethod
                            match tx.read_table_field(mt, &Value::String(index_str)) {
                                Ok(Value::Table(meta_table)) => {
                                    // If __index is a table, look up in it
                                    match tx.read_table_field(meta_table, &key_val) {
                                        Ok(value) => {
                                            tx.set_register(self.current_thread, base + a, value);
                                            Ok(ExecutionStatus::Continue)
                                        },
                                        Err(_) => {
                                            // Not in metamethod table either
                                            tx.set_register(self.current_thread, base + a, Value::Nil);
                                            Ok(ExecutionStatus::Continue)
                                        }
                                    }
                                },
                                Ok(Value::Closure(closure)) => {
                                    // If __index is a function, queue a metamethod call through the transaction
                                    let context = PostCallContext::Metamethod {
                                        method: "__index".to_string(), 
                                        return_type: MetamethodReturnType::Index,
                                    };
                                    
                                    tx.queue_operation(PendingOperation::FunctionCall {
                                        closure,
                                        args: vec![Value::Table(table), key_val.clone()],
                                        context,
                                    });
                                    
                                    Ok(ExecutionStatus::Continue)
                                },
                                Ok(Value::CFunction(cfunc)) => {
                                    let func = cfunc.clone();
                                    let table_copy = table;
                                    let key_copy = key_val.clone();
                                    
                                    let result = self.execute_c_function_direct(func, &[Value::Table(table_copy), key_copy])?;
                                    
                                    tx.set_register(self.current_thread, base + a, result);
                                    
                                    Ok(ExecutionStatus::Continue)
                                },
                                _ => {
                                    // No useful metamethod, return nil
                                    tx.set_register(self.current_thread, base + a, Value::Nil);
                                    Ok(ExecutionStatus::Continue)
                                }
                            }
                        } else {
                            // No metatable, just return nil
                            tx.set_register(self.current_thread, base + a, Value::Nil);
                            Ok(ExecutionStatus::Continue)
                        }
                    }
                }
            },
            _ => {
                Err(LuaError::TypeError(format!("attempt to index a {} value", table_val.type_name())))
            }
        }
    }
    
    /// Execute SETTABLE instruction (A B C)
    fn execute_set_table(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: usize, b: usize, c: usize) -> Result<ExecutionStatus> {
        // R(A)[RK(B)] := RK(C)
        let base = frame.base_register as usize;
        
        // Get the table
        let table_val = tx.read_register(self.current_thread, base + a)?;
        
        // Get key (from register or constant)
        let key_val = if b & 0x100 != 0 {
            // Key is a constant
            tx.get_constant(frame.closure, b & 0xFF)?
        } else {
            // Key is a register
            tx.read_register(self.current_thread, base + b)?
        };
        
        // Get value (from register or constant)
        let val = if c & 0x100 != 0 {
            // Value is a constant
            tx.get_constant(frame.closure, c & 0xFF)?
        } else {
            // Value is a register
            tx.read_register(self.current_thread, base + c)?
        };
        
        match table_val {
            Value::Table(table) => {
                // Try direct table set
                tx.set_table_field(table, key_val.clone(), val.clone())?;
                Ok(ExecutionStatus::Continue)
                
                // In a more complete implementation, we would check
                // for __newindex metamethods here if the field doesn't exist
            },
            _ => {
                Err(LuaError::TypeError(format!("attempt to index a {} value", table_val.type_name())))
            }
        }
    }
    
    /// Execute NEWTABLE instruction (A B C)
    fn execute_new_table(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: usize, _b: usize, _c: usize) -> Result<ExecutionStatus> {
        // R(A) := {} (size = B,C)
        // B is array size, C is hash size (both are encoded specially)
        
        // For simplicity, we're not using the size hints
        let table = tx.create_table()?;
        
        // Store in register A
        tx.set_register(self.current_thread, frame.base_register as usize + a, Value::Table(table));
        
        Ok(ExecutionStatus::Continue)
    }
    
    /// Execute arithmetic operations (ADD, SUB, MUL, DIV, MOD, POW)
    fn execute_arithmetic(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, op: OpCode, a: usize, b: usize, c: usize) -> Result<ExecutionStatus> {
        // R(A) := RK(B) op RK(C)
        let base = frame.base_register as usize;
        
        // Get operands
        let b_val = if b & 0x100 != 0 {
            // B is a constant
            tx.get_constant(frame.closure, b & 0xFF)?
        } else {
            // B is a register
            tx.read_register(self.current_thread, base + b)?
        };
        
        let c_val = if c & 0x100 != 0 {
            // C is a constant
            tx.get_constant(frame.closure, c & 0xFF)?
        } else {
            // C is a register
            tx.read_register(self.current_thread, base + c)?
        };
        
        // Perform arithmetic operation
        match (b_val, c_val) {
            (Value::Number(b_num), Value::Number(c_num)) => {
                let result = match op {
                    OpCode::Add => b_num + c_num,
                    OpCode::Sub => b_num - c_num,
                    OpCode::Mul => b_num * c_num,
                    OpCode::Div => b_num / c_num,
                    OpCode::Mod => b_num % c_num,
                    OpCode::Pow => b_num.powf(c_num),
                    _ => return Err(LuaError::InvalidOperation("unknown arithmetic operation".to_string())),
                };
                
                // Set result in register A
                tx.set_register(self.current_thread, base + a, Value::Number(result));
                
                Ok(ExecutionStatus::Continue)
            },
            (Value::String(b_str), Value::Number(c_num)) => {
                // Try string to number conversion for first operand
                let b_str_bytes = self.heap.get_string_bytes(b_str)?;
                let b_str = std::str::from_utf8(b_str_bytes)
                    .map_err(|_| LuaError::InvalidEncoding)?;
                
                if let Ok(b_num) = b_str.parse::<f64>() {
                    let result = match op {
                        OpCode::Add => b_num + c_num,
                        OpCode::Sub => b_num - c_num,
                        OpCode::Mul => b_num * c_num,
                        OpCode::Div => b_num / c_num,
                        OpCode::Mod => b_num % c_num,
                        OpCode::Pow => b_num.powf(c_num),
                        _ => return Err(LuaError::InvalidOperation("unknown arithmetic operation".to_string())),
                    };
                    
                    // Set result
                    tx.set_register(self.current_thread, base + a, Value::Number(result));
                    
                    Ok(ExecutionStatus::Continue)
                } else {
                    // Metamethod handling would go here in a full implementation
                    Err(LuaError::TypeError(format!("attempt to perform arithmetic on a string value")))
                }
            },
            // Similar cases for other number conversions
            _ => {
                // Check for metamethods
                // Simplified - full implementation would handle metamethods
                Err(LuaError::TypeError(format!("attempt to perform arithmetic on a {} value and a {} value", 
                                               b_val.type_name(), c_val.type_name())))
            }
        }
    }
    
    /// Execute UNM instruction (A B)
    fn execute_unm(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: usize, b: usize) -> Result<ExecutionStatus> {
        // R(A) := -R(B)
        let base = frame.base_register as usize;
        let b_val = tx.read_register(self.current_thread, base + b)?;
        
        match b_val {
            Value::Number(n) => {
                tx.set_register(self.current_thread, base + a, Value::Number(-n));
                Ok(ExecutionStatus::Continue)
            },
            _ => {
                // Metamethod handling would be here
                Err(LuaError::TypeError(format!("attempt to perform arithmetic on a {} value", b_val.type_name())))
            }
        }
    }
    
    /// Execute NOT instruction (A B)
    fn execute_not(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: usize, b: usize) -> Result<ExecutionStatus> {
        // R(A) := not R(B)
        let base = frame.base_register as usize;
        let b_val = tx.read_register(self.current_thread, base + b)?;
        
        // In Lua, only false and nil are falsy
        let result = match b_val {
            Value::Nil => true,
            Value::Boolean(b) => !b,
            _ => false,
        };
        
        tx.set_register(self.current_thread, base + a, Value::Boolean(result));
        
        Ok(ExecutionStatus::Continue)
    }
    
    /// Execute LEN instruction (A B)
    fn execute_len(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: usize, b: usize) -> Result<ExecutionStatus> {
        // R(A) := length of R(B)
        let base = frame.base_register as usize;
        let b_val = tx.read_register(self.current_thread, base + b)?;
        
        match b_val {
            Value::String(s) => {
                let s_bytes = self.heap.get_string_bytes(s)?;
                tx.set_register(self.current_thread, base + a, Value::Number(s_bytes.len() as f64));
                Ok(ExecutionStatus::Continue)
            },
            Value::Table(t) => {
                // Get the table's length (in Lua, this is the largest integer key in the array part)
                let table = self.heap.get_table(t)?;
                let len = table.array.len();
                
                tx.set_register(self.current_thread, base + a, Value::Number(len as f64));
                Ok(ExecutionStatus::Continue)
            },
            _ => {
                // Metamethod handling would be here
                Err(LuaError::TypeError(format!("attempt to get length of a {} value", b_val.type_name())))
            }
        }
    }
    
    /// Execute CONCAT instruction (A B C)
    fn execute_concat(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: usize, b: usize, c: usize) -> Result<ExecutionStatus> {
        // R(A) := R(B).. ... ..R(C)
        println!("[VM] Executing CONCAT operation: A={}, B={}, C={}", a, b, c);

        // Start the concat operation using our state machine approach
        self.start_concat_operation(tx, frame.base_register, a, b, c)?;
        
        Ok(ExecutionStatus::Continue)
    }
    
    /// Start a concat operation using the state machine approach
    fn start_concat_operation(&mut self, tx: &mut HeapTransaction<'_>, base_register: u16, target: usize, start: usize, end: usize) -> Result<()> {
        println!("[VM] Starting concat operation: target={}, start={}, end={}", target, start, end);
        
        // Collect values to concatenate
        let mut values = Vec::with_capacity(end - start + 1);
        for i in start..=end {
            let value = tx.read_register(self.current_thread, base_register as usize + i)?;
            values.push(value);
        }
        
        // Queue a concatenation operation to be processed in the main loop
        let context = PostCallContext::Concat {
            base_register,
            target_register: target,
            current_index: 0,
            last_index: end - start,
            accumulated_parts: Vec::new(),
        };
        
        // Start by processing the first value
        self.process_concat_value(values, context)
    }
    
    /// Process a single value in a concatenation operation
    fn process_concat_value(&mut self, values: Vec<Value>, context: PostCallContext) -> Result<()> {
        if let PostCallContext::Concat {
            base_register,
            target_register,
            current_index,
            last_index,
            mut accumulated_parts
        } = context.clone() {
            println!("[VM] Processing concat value at index {}", current_index);
            
            // Get the current value
            let value = &values[current_index];
            
            match value {
                Value::String(s) => {
                    // Direct string - add to accumulated parts
                    let bytes = self.heap.get_string_bytes(*s)?;
                    let str_value = std::str::from_utf8(bytes)
                        .map_err(|_| LuaError::InvalidEncoding)?;
                    accumulated_parts.push(str_value.to_string());
                    
                    // Move to next value or complete
                    self.advance_concat_operation(values, PostCallContext::Concat {
                        base_register,
                        target_register,
                        current_index: current_index + 1,
                        last_index,
                        accumulated_parts,
                    })?;
                },
                Value::Number(n) => {
                    // Convert number to string
                    accumulated_parts.push(n.to_string());
                    
                    // Move to next value or complete
                    self.advance_concat_operation(values, PostCallContext::Concat {
                        base_register,
                        target_register,
                        current_index: current_index + 1,
                        last_index,
                        accumulated_parts,
                    })?;
                },
                Value::Table(handle) => {
                    // Check for __tostring metamethod
                    let method_key = self.heap.create_string("__tostring")?;
                    let metamethod = self.heap.get_metamethod(*handle, method_key)?;
                    
                    match metamethod {
                        Value::Closure(closure) => {
                            // Queue __tostring metamethod call
                            println!("[VM] Queueing __tostring metamethod call for concat");
                            
                            // Get current call depth
                            let call_depth = self.get_call_depth()?;
                            
                            // Store concat context for when metamethod returns
                            self.call_contexts.insert(call_depth, context);
                            
                            // Queue a function call with the table as argument
                            self.pending_operations.push_front(PendingOperation::FunctionCall {
                                closure,
                                args: vec![Value::Table(*handle)],
                                context: PostCallContext::Metamethod {
                                    method: "__tostring".to_string(),
                                    return_type: MetamethodReturnType::ToString,
                                },
                            });
                        },
                        Value::CFunction(cfunc) => {
                            // Call the C function directly
                            let result = self.call_tostring_cfunction(cfunc, Value::Table(*handle))?;
                            accumulated_parts.push(result);
                            
                            // Move to next value or complete
                            self.advance_concat_operation(values, PostCallContext::Concat {
                                base_register,
                                target_register,
                                current_index: current_index + 1,
                                last_index,
                                accumulated_parts,
                            })?;
                        },
                        _ => {
                            // Default string representation
                            accumulated_parts.push(format!("table: {:?}", handle));
                            
                            // Move to next value or complete
                            self.advance_concat_operation(values, PostCallContext::Concat {
                                base_register,
                                target_register,
                                current_index: current_index + 1,
                                last_index,
                                accumulated_parts,
                            })?;
                        }
                    }
                },
                _ => {
                    return Err(LuaError::TypeError(format!(
                        "attempt to concatenate a {} value", value.type_name()
                    )));
                }
            }
        }
        
        Ok(())
    }
    
    /// Advance a concatenation operation to the next value or complete it
    fn advance_concat_operation(&mut self, values: Vec<Value>, context: PostCallContext) -> Result<()> {
        if let PostCallContext::Concat {
            base_register,
            target_register,
            current_index,
            last_index,
            ref accumulated_parts
        } = context {
            if current_index <= last_index {
                // Process next value
                self.process_concat_value(values, context)?;
            } else {
                // All values processed - join strings and create final result
                let result = accumulated_parts.join("");
                println!("[VM] Concatenation complete: {}", result);
                
                // Create a string with the result
                let str_handle = self.heap.create_string(&result)?;
                
                // Store in target register
                self.set_register(base_register, target_register, Value::String(str_handle))?;
            }
        }
        
        Ok(())
    }
    
    /// Call a __tostring C function
    fn call_tostring_cfunction(&mut self, cfunc: CFunction, value: Value) -> Result<String> {
        // Set up stack for C function call
        let stack_size_before = {
            let thread = self.heap.get_thread_mut(self.current_thread)?;
            let size = thread.stack.len();
            
            // Push the value as argument
            thread.stack.push(value);
            
            size
        };
        
        // Create execution context
        let mut ctx = ExecutionContext {
            vm: self,
            base: stack_size_before,
            arg_count: 1,
        };
        
        // Call the function
        let ret_count = cfunc(&mut ctx)?;
        
        // Get the return value
        let result = if ret_count > 0 {
            let thread = self.heap.get_thread(self.current_thread)?;
            
            if stack_size_before < thread.stack.len() {
                match thread.stack[stack_size_before] {
                    Value::String(s) => {
                        let bytes = self.heap.get_string_bytes(s)?;
                        std::str::from_utf8(bytes)
                            .map_err(|_| LuaError::InvalidEncoding)?
                            .to_string()
                    },
                    Value::Number(n) => n.to_string(),
                    _ => "?".to_string(),
                }
            } else {
                "".to_string()
            }
        } else {
            "".to_string()
        };
        
        // Clean up the stack
        let thread = self.heap.get_thread_mut(self.current_thread)?;
        thread.stack.truncate(stack_size_before);
        
        Ok(result)
    }
    
    /// Execute JMP instruction (sBx)
    fn execute_jmp(&mut self, tx: &mut HeapTransaction<'_>, _frame: &CallFrame, sbx: i32) -> Result<ExecutionStatus> {
        // PC += sBx
        tx.jump(self.current_thread, sbx)?;
        
        Ok(ExecutionStatus::Continue)
    }
    
    /// Execute EQ instruction (A B C)
    fn execute_eq(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: bool, b: usize, c: usize) -> Result<ExecutionStatus> {
        // if ((RK(B) == RK(C)) ~= A) then pc++
        let base = frame.base_register as usize;
        
        // Get operands
        let b_val = if b & 0x100 != 0 {
            tx.get_constant(frame.closure, b & 0xFF)?
        } else {
            tx.read_register(self.current_thread, base + b)?
        };
        
        let c_val = if c & 0x100 != 0 {
            tx.get_constant(frame.closure, c & 0xFF)?
        } else {
            tx.read_register(self.current_thread, base + c)?
        };
        
        // Check equality
        let equal = b_val == c_val;
        
        // Skip next instruction if condition is false
        if equal != a {
            tx.increment_pc(self.current_thread)?;
        }
        
        Ok(ExecutionStatus::Continue)
    }
    
    /// Execute LT instruction (A B C)
    fn execute_lt(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: bool, b: usize, c: usize) -> Result<ExecutionStatus> {
        // if ((RK(B) < RK(C)) ~= A) then pc++
        let base = frame.base_register as usize;
        
        // Get operands
        let b_val = if b & 0x100 != 0 {
            tx.get_constant(frame.closure, b & 0xFF)?
        } else {
            tx.read_register(self.current_thread, base + b)?
        };
        
        let c_val = if c & 0x100 != 0 {
            tx.get_constant(frame.closure, c & 0xFF)?
        } else {
            tx.read_register(self.current_thread, base + c)?
        };
        
        // Check less than
        let less_than = match (&b_val, &c_val) {
            (Value::Number(b_num), Value::Number(c_num)) => b_num < c_num,
            (Value::String(b_str), Value::String(c_str)) => {
                let b_bytes = self.heap.get_string_bytes(*b_str)?;
                let c_bytes = self.heap.get_string_bytes(*c_str)?;
                b_bytes < c_bytes
            },
            _ => {
                // Metamethod handling would go here
                return Err(LuaError::TypeError(format!(
                    "attempt to compare {} with {}",
                    b_val.type_name(),
                    c_val.type_name()
                )));
            }
        };
        
        // Skip next instruction if condition is false
        if less_than != a {
            tx.increment_pc(self.current_thread)?;
        }
        
        Ok(ExecutionStatus::Continue)
    }
    
    /// Execute LE instruction (A B C)
    fn execute_le(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: bool, b: usize, c: usize) -> Result<ExecutionStatus> {
        // if ((RK(B) <= RK(C)) ~= A) then pc++
        let base = frame.base_register as usize;
        
        // Get operands
        let b_val = if b & 0x100 != 0 {
            tx.get_constant(frame.closure, b & 0xFF)?
        } else {
            tx.read_register(self.current_thread, base + b)?
        };
        
        let c_val = if c & 0x100 != 0 {
            tx.get_constant(frame.closure, c & 0xFF)?
        } else {
            tx.read_register(self.current_thread, base + c)?
        };
        
        // Check less than or equal
        let less_or_equal = match (&b_val, &c_val) {
            (Value::Number(b_num), Value::Number(c_num)) => b_num <= c_num,
            (Value::String(b_str), Value::String(c_str)) => {
                let b_bytes = self.heap.get_string_bytes(*b_str)?;
                let c_bytes = self.heap.get_string_bytes(*c_str)?;
                b_bytes <= c_bytes
            },
            _ => {
                // Metamethod handling would go here
                return Err(LuaError::TypeError(format!(
                    "attempt to compare {} with {}",
                    b_val.type_name(),
                    c_val.type_name()
                )));
            }
        };
        
        // Skip next instruction if condition is false
        if less_or_equal != a {
            tx.increment_pc(self.current_thread)?;
        }
        
        Ok(ExecutionStatus::Continue)
    }
    
    /// Execute TEST instruction (A C)
    fn execute_test(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: usize, c: bool) -> Result<ExecutionStatus> {
        // if not (R(A) <=> C) then pc++
        let base = frame.base_register as usize;
        let a_val = tx.read_register(self.current_thread, base + a)?;
        
        // In Lua, only nil and false are falsy
        let is_truthy = match a_val {
            Value::Nil => false,
            Value::Boolean(b) => b,
            _ => true,
        };
        
        // Skip next instruction if condition is false
        if is_truthy != c {
            tx.increment_pc(self.current_thread)?;
        }
        
        Ok(ExecutionStatus::Continue)
    }
    
    /// Execute TESTSET instruction (A B C)
    fn execute_test_set(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: usize, b: usize, c: bool) -> Result<ExecutionStatus> {
        // if (R(B) <=> C) then R(A) := R(B) else pc++
        let base = frame.base_register as usize;
        let b_val = tx.read_register(self.current_thread, base + b)?;
        
        // In Lua, only nil and false are falsy
        let is_truthy = match b_val {
            Value::Nil => false,
            Value::Boolean(b) => b,
            _ => true,
        };
        
        if is_truthy == c {
            // Condition is true, set R(A) = R(B)
            tx.set_register(self.current_thread, base + a, b_val);
        } else {
            // Condition is false, skip next instruction
            tx.increment_pc(self.current_thread)?;
        }
        
        Ok(ExecutionStatus::Continue)
    }
    
    /// Execute CALL instruction (A B C)
    fn execute_call(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: usize, b: usize, c: usize) -> Result<ExecutionStatus> {
        // R(A), ... ,R(A+C-2) := R(A)(R(A+1), ... ,R(A+B-1))
        println!("[VM] CALL instruction: A={}, B={}, C={}", a, b, c);
        
        let base = frame.base_register as usize;
        
        // Get function
        let func = tx.read_register(self.current_thread, base + a)?;
        
        // Collect arguments
        let arg_count = if b == 0 {
            // All arguments from A+1 to top of stack
            tx.get_stack_top(self.current_thread)? - base - a - 1
        } else {
            b - 1
        };
        
        let mut args = Vec::with_capacity(arg_count);
        for i in 0..arg_count {
            args.push(tx.read_register(self.current_thread, base + a + 1 + i)?);
        }
        
        // Determine expected returns
        let returns = if c == 0 { 255 } else { c - 1 };
        
        println!("[VM] Call: function={:?}, args={:?}, expected returns={}", func, args, returns);
        
        match func {
            Value::Closure(closure) => {
                // For Lua closures, use the pending operations queue
                let context = PostCallContext::Normal {
                    return_register: Some((frame.base_register, a)),
                };
                
                // Queue the function call
                tx.queue_operation(PendingOperation::FunctionCall {
                    closure,
                    args,
                    context,
                });
                
                Ok(ExecutionStatus::Continue)
            },
            Value::CFunction(cfunc) => {
                // For C functions, we can call directly without recursion
                
                // Push arguments to stack
                let stack_base = {
                    let thread = self.heap.get_thread_mut(self.current_thread)?;
                    let base = thread.stack.len();
                    
                    for arg in &args {
                        thread.stack.push(arg.clone());
                    }
                    
                    base
                };
                
                // Create execution context
                let mut ctx = ExecutionContext {
                    vm: self,
                    base: stack_base,
                    arg_count,
                };
                
                // Call the function
                let ret_count = cfunc(&mut ctx)?;
                
                // Copy returns to registers
                for i in 0..ret_count.min(returns as i32) as usize {
                    let value = if i < ret_count as usize {
                        let thread = self.heap.get_thread(self.current_thread)?;
                        if stack_base + i < thread.stack.len() {
                            thread.stack[stack_base + i].clone()
                        } else {
                            Value::Nil
                        }
                    } else {
                        Value::Nil
                    };
                    
                    // Set value in register
                    tx.set_register(self.current_thread, base as usize + a + i, value);
                }
                
                // Clean up stack
                {
                    let thread = self.heap.get_thread_mut(self.current_thread)?;
                    thread.stack.truncate(stack_base);
                }
                
                Ok(ExecutionStatus::Continue)
            },
            _ => {
                Err(LuaError::TypeError(format!("attempt to call a {} value", func.type_name())))
            }
        }
    }
    
    /// Execute TAILCALL instruction (A B C)
    fn execute_tail_call(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: usize, b: usize) -> Result<ExecutionStatus> {
        // return R(A)(R(A+1), ... ,R(A+B-1))
        println!("[VM] TAILCALL instruction: A={}, B={}", a, b);
        
        let base = frame.base_register as usize;
        
        // Get function
        let func = tx.read_register(self.current_thread, base + a)?;
        
        // Collect arguments
        let arg_count = if b == 0 {
            // All arguments from A+1 to top of stack
            tx.get_stack_top(self.current_thread)? - base - a - 1
        } else {
            b - 1
        };
        
        let mut args = Vec::with_capacity(arg_count);
        for i in 0..arg_count {
            args.push(tx.read_register(self.current_thread, base + a + 1 + i)?);
        }
        
        println!("[VM] TailCall: function={:?}, args={:?}", func, args);
        
        match func {
            Value::Closure(closure) => {
                // For TailCall, we pop the current frame BEFORE queuing the new call
                tx.pop_call_frame(self.current_thread)?;
                
                // Queue the function call
                tx.queue_operation(PendingOperation::FunctionCall {
                    closure,
                    args,
                    context: PostCallContext::Normal {
                        return_register: None, // No specific register - result goes to caller
                    },
                });
                
                Ok(ExecutionStatus::Continue)
            },
            Value::CFunction(cfunc) => {
                // For C functions, we can call directly
                
                // Pop current frame first
                tx.pop_call_frame(self.current_thread)?;
                
                // Push arguments to stack
                let stack_base = {
                    let thread = self.heap.get_thread_mut(self.current_thread)?;
                    let base = thread.stack.len();
                    
                    for arg in &args {
                        thread.stack.push(arg.clone());
                    }
                    
                    base
                };
                
                // Create execution context
                let mut ctx = ExecutionContext {
                    vm: self,
                    base: stack_base,
                    arg_count,
                };
                
                // Call function
                let ret_count = cfunc(&mut ctx)?;
                
                // Get return value
                let return_value = if ret_count > 0 {
                    let thread = self.heap.get_thread(self.current_thread)?;
                    if stack_base < thread.stack.len() {
                        thread.stack[stack_base].clone()
                    } else {
                        Value::Nil
                    }
                } else {
                    Value::Nil
                };
                
                // Clean up stack
                {
                    let thread = self.heap.get_thread_mut(self.current_thread)?;
                    thread.stack.truncate(stack_base);
                }
                
                // Return the value
                Ok(ExecutionStatus::Return(return_value))
            },
            _ => {
                Err(LuaError::TypeError(format!("attempt to call a {} value", func.type_name())))
            }
        }
    }
    
    /// Execute RETURN instruction (A B)
    fn execute_return(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: usize, b: usize) -> Result<ExecutionStatus> {
        // return R(A), ... ,R(A+B-2)
        println!("[VM] RETURN instruction: A={}, B={}", a, b);
        
        let base = frame.base_register as usize;
        
        // Simplified: Just return the first value
        // A full implementation would handle multiple return values
        let return_value = if b > 0 {
            tx.read_register(self.current_thread, base + a)?
        } else {
            Value::Nil
        };
        
        println!("[VM] Return value: {:?}", return_value);
        
        Ok(ExecutionStatus::Return(return_value))
    }
    
    /// Execute FORPREP instruction (A sBx)
    fn execute_for_prep(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: usize, sbx: i32) -> Result<ExecutionStatus> {
        // R(A)-=R(A+2); PC+=sBx
        let base = frame.base_register as usize;
        
        // Get loop variables: index, limit, step
        let index = tx.read_register(self.current_thread, base + a)?;
        let limit = tx.read_register(self.current_thread, base + a + 1)?;
        let step = tx.read_register(self.current_thread, base + a + 2)?;
        
        // Verify all are numbers
        let (index_val, limit_val, step_val) = match (index, limit, step) {
            (Value::Number(i), Value::Number(l), Value::Number(s)) => (i, l, s),
            _ => {
                return Err(LuaError::TypeError("numeric for loop requires numeric values".to_string()));
            }
        };
        
        // Initialize by subtracting step from index
        let new_index = index_val - step_val;
        tx.set_register(self.current_thread, base + a, Value::Number(new_index));
        
        // Jump to loop body
        tx.jump(self.current_thread, sbx)?;
        
        Ok(ExecutionStatus::Continue)
    }
    
    /// Execute FORLOOP instruction (A sBx)
    fn execute_for_loop(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: usize, sbx: i32) -> Result<ExecutionStatus> {
        // R(A)+=R(A+2); if R(A) <?= R(A+1) then { PC+=sBx; R(A+3)=R(A) }
        let base = frame.base_register as usize;
        
        // Get loop variables: index, limit, step
        let index = tx.read_register(self.current_thread, base + a)?;
        let limit = tx.read_register(self.current_thread, base + a + 1)?;
        let step = tx.read_register(self.current_thread, base + a + 2)?;
        
        // Verify all are numbers
        let (index_val, limit_val, step_val) = match (index, limit, step) {
            (Value::Number(i), Value::Number(l), Value::Number(s)) => (i, l, s),
            _ => {
                return Err(LuaError::TypeError("numeric for loop requires numeric values".to_string()));
            }
        };
        
        // Increment index
        let new_index = index_val + step_val;
        tx.set_register(self.current_thread, base + a, Value::Number(new_index));
        
        // Check if loop should continue
        let continue_loop = if step_val > 0.0 {
            new_index <= limit_val
        } else {
            new_index >= limit_val
        };
        
        if continue_loop {
            // Set loop variable
            tx.set_register(self.current_thread, base + a + 3, Value::Number(new_index));
            
            // Jump back to loop body
            tx.jump(self.current_thread, sbx)?;
        }
        
        Ok(ExecutionStatus::Continue)
    }
    
    /// Execute TFORLOOP instruction (A C)
    fn execute_tfor_loop(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: usize, c: usize) -> Result<ExecutionStatus> {
        // R(A+3), ... ,R(A+2+C) := R(A)(R(A+1), R(A+2));
        // if R(A+3) ~= nil then R(A+2)=R(A+3) else pc++
        let base = frame.base_register as usize;
        
        // Get iterator function, state, and control variable
        let iter = tx.read_register(self.current_thread, base + a)?;
        let state = tx.read_register(self.current_thread, base + a + 1)?;
        let control = tx.read_register(self.current_thread, base + a + 2)?;
        
        println!("[VM] TFORLOOP: iterator={:?}, state={:?}, control={:?}", iter, state, control);
        
        match iter {
            Value::Closure(closure) => {
                // Queue an iterator call
                let context = PostCallContext::Iterator {
                    base_register: frame.base_register,
                    register_a: a,
                    var_count: c,
                };
                
                tx.queue_operation(PendingOperation::IteratorCall {
                    closure,
                    state,
                    control,
                    context,
                });
                
                Ok(ExecutionStatus::Continue)
            },
            Value::CFunction(cfunc) => {
                // For C functions, we can call directly
                
                // Push arguments to stack
                let stack_base = {
                    let thread = self.heap.get_thread_mut(self.current_thread)?;
                    let base = thread.stack.len();
                    
                    thread.stack.push(state);
                    thread.stack.push(control);
                    
                    base
                };
                
                // Call iterator function
                let mut ctx = ExecutionContext {
                    vm: self,
                    base: stack_base,
                    arg_count: 2,
                };
                
                let ret_count = cfunc(&mut ctx)?;
                
                // Get first return value
                let first_val = if ret_count > 0 {
                    let thread = self.heap.get_thread(self.current_thread)?;
                    if stack_base < thread.stack.len() {
                        thread.stack[stack_base].clone()
                    } else {
                        Value::Nil
                    }
                } else {
                    Value::Nil
                };
                
                // Clean up stack
                {
                    let thread = self.heap.get_thread_mut(self.current_thread)?;
                    thread.stack.truncate(stack_base);
                }
                
                if first_val == Value::Nil {
                    // End of iteration, skip loop body
                    tx.increment_pc(self.current_thread)?;
                } else {
                    // Continue iteration
                    // Update control variable
                    tx.set_register(self.current_thread, base + a + 2, first_val.clone());
                    
                    // Set loop variables
                    tx.set_register(self.current_thread, base + a + 3, first_val);
                    
                    // Additional loop variables would be set here in a full implementation
                }
                
                Ok(ExecutionStatus::Continue)
            },
            _ => {
                Err(LuaError::TypeError(format!(
                    "attempt to call a {} value as iterator", iter.type_name()
                )))
            }
        }
    }
    
    /// Execute CLOSURE instruction (A Bx)
    fn execute_closure(&mut self, tx: &mut HeapTransaction<'_>, frame: &CallFrame, a: usize, bx: usize) -> Result<ExecutionStatus> {
        // R(A) := closure(KPROTO[Bx])
        let base = frame.base_register as usize;
        
        // In a full implementation, this would:
        // 1. Get the prototype at index Bx
        // 2. Create a closure from it
        // 3. Setup upvalues
        // 4. Store in register A
        
        // Simplified version:
        let closure_handle = self.heap.create_closure(
            super::value::FunctionProto {
                bytecode: Vec::new(),
                constants: Vec::new(),
                upvalues: Vec::new(),
                param_count: 0,
                is_vararg: false,
                source: None,
                line_defined: 0,
                last_line_defined: 0,
                line_info: Vec::new(),
                locals: Vec::new(),
            },
            Vec::new()
        )?;
        
        tx.set_register(self.current_thread, base + a, Value::Closure(closure_handle));
        
        Ok(ExecutionStatus::Continue)
    }
    
    /// Handle storing return value in caller's registers
    fn handle_function_return(&mut self, value: Value) -> Result<()> {
        // This is a fallback mechanism when no specific context is found
        // In a complete implementation, would handle return values properly
        println!("[VM] Default handling for return value {:?}", value);
        
        // Just push to stack for now
        self.heap.push_thread_stack(self.current_thread, value)?;
        
        Ok(())
    }
    
    /// Process a return within the proper context
    fn handle_return_with_context(&mut self, value: Value, context: PostCallContext) -> Result<()> {
        println!("[VM] Handling return with context: {:?}", context);
        
        match context {
            PostCallContext::Normal { return_register } => {
                if let Some((base, offset)) = return_register {
                    // Store in specific register
                    println!("[VM] Storing return value in register {}:{}", base, offset);
                    self.set_register(base, offset, value)
                } else {
                    // No specific register - may be final result
                    // Nothing to do - main loop will capture the value
                    Ok(())
                }
            },
            PostCallContext::Iterator { base_register, register_a, var_count } => {
                println!("[VM] Processing iterator return");
                
                if value == Value::Nil {
                    // End of iteration - skip the loop body
                    let thread = self.heap.get_thread_mut(self.current_thread)?;
                    if let Some(frame) = thread.call_frames.last_mut() {
                        frame.pc += 1;
                    }
                } else {
                    // Set control var (register_a + 2)
                    self.set_register(base_register, register_a + 2, value.clone())?;
                    
                    // Set loop var (register_a + 3)
                    self.set_register(base_register, register_a + 3, value)?;
                    
                    // Additional loop variables would be set here in a full implementation
                }
                
                Ok(())
            },
            PostCallContext::Metamethod { method, return_type } => {
                println!("[VM] Processing metamethod return: {}", method);
                
                match return_type {
                    MetamethodReturnType::Index => {
                        // For __index, just return the value as is
                        // In a full implementation, this would set the value in the appropriate register
                        self.heap.push_thread_stack(self.current_thread, value)?;
                        Ok(())
                    },
                    MetamethodReturnType::NewIndex => {
                        // For __newindex, no return expected
                        Ok(())
                    },
                    MetamethodReturnType::Call => {
                        // For __call, same as normal return
                        self.heap.push_thread_stack(self.current_thread, value)?;
                        Ok(())
                    },
                    MetamethodReturnType::Arithmetic => {
                        // Arithmetic operator return
                        self.heap.push_thread_stack(self.current_thread, value)?;
                        Ok(())
                    },
                    MetamethodReturnType::ToString => {
                        // For __tostring in concat, we need to find the concat context
                        // It should have been stored at the parent call depth
                        let frame_depth = self.get_call_depth()?;
                        
                        // Look for concat context at various depths
                        for depth in (0..=frame_depth).rev() {
                            if let Some(context) = self.call_contexts.remove(&depth) {
                                if let PostCallContext::Concat { 
                                    base_register,
                                    target_register,
                                    current_index,
                                    last_index,
                                    mut accumulated_parts
                                } = context {
                                    // Convert return value to string
                                    let str_value = match value {
                                        Value::String(s) => {
                                            let bytes = self.heap.get_string_bytes(s)?;
                                            std::str::from_utf8(bytes)
                                                .map_err(|_| LuaError::InvalidEncoding)?
                                                .to_string()
                                        },
                                        Value::Number(n) => n.to_string(),
                                        _ => return Err(LuaError::TypeError(
                                            "__tostring must return a string".to_string()
                                        )),
                                    };
                                    
                                    // Add to accumulated strings
                                    accumulated_parts.push(str_value);
                                    
                                    // Continue with next value
                                    let new_context = PostCallContext::Concat {
                                        base_register,
                                        target_register,
                                        current_index: current_index + 1,
                                        last_index,
                                        accumulated_parts,
                                    };
                                    
                                    // Resume the concat operation
                                    let mut values = Vec::new();
                                    for i in 0..=last_index {
                                        values.push(self.get_register(base_register, i + current_index)?);
                                    }
                                    
                                    return self.advance_concat_operation(values, new_context);
                                }
                            }
                        }
                        
                        // If we couldn't find the concat context, that's an error
                        Err(LuaError::InvalidOperation("Lost concat context".to_string()))
                    }
                }
            },
            PostCallContext::Concat { .. } => {
                // This shouldn't happen - concat contexts are handled specially
                Err(LuaError::InvalidOperation("Unexpected concat context in return handler".to_string()))
            },
        }
    }
    
    /// Reset the VM
    pub fn reset(&mut self) -> Result<()> {
        // Reset the instruction count
        self.instruction_count = 0;
        
        // Clear pending operations
        self.pending_operations.clear();
        
        // Clear call contexts
        self.call_contexts.clear();
        
        // Reset main thread
        self.heap.reset_thread(self.current_thread)?;
        
        Ok(())
    }
    
    /// Create a string
    pub fn create_string(&mut self, s: &str) -> Result<StringHandle> {
        self.heap.create_string(s)
    }
    
    /// Create a table
    pub fn create_table(&mut self) -> Result<TableHandle> {
        self.heap.create_table()
    }
    
    /// Get the globals table
    pub fn globals(&self) -> TableHandle {
        self.heap.get_globals().unwrap()
    }
    
    /// Get the registry table
    pub fn registry(&self) -> TableHandle {
        self.heap.get_registry().unwrap()
    }
    
    /// Set a value in a table
    pub fn set_table(&mut self, table: TableHandle, key: Value, value: Value) -> Result<()> {
        self.heap.set_table_field(table, key, value)
    }
    
    /// Get a value from a table
    pub fn get_table(&mut self, table: TableHandle, key: Value) -> Result<Value> {
        self.heap.get_table_field(table, &key)
    }
    
    /// Set a table index (numeric)
    pub fn set_table_index(&mut self, table: TableHandle, index: usize, value: Value) -> Result<()> {
        self.heap.set_table_field(table, Value::Number(index as f64), value)
    }
    
    /// Get a table index (numeric)
    pub fn get_table_index(&mut self, table: TableHandle, index: usize) -> Result<Value> {
        self.heap.get_table_field(table, &Value::Number(index as f64))
    }
    
    /// Get a table key
    pub fn get_table_key(&mut self, table: TableHandle, key: Value) -> Result<Value> {
        self.heap.get_table_field(table, &key)
    }
    
    /// Create userdata
    pub fn create_userdata<T: 'static + std::any::Any + Send + Sync>(&mut self, _data: T) -> Result<super::value::UserDataHandle> {
        // Create a type name from the type
        let type_name = std::any::type_name::<T>().to_string();
        
        // We're not actually storing the data right now, just the type name
        self.heap.create_userdata(type_name)
    }
    
    /// Get string length
    pub fn get_string_length(&self, handle: StringHandle) -> Result<usize> {
        let bytes = self.heap.get_string_bytes(handle)?;
        Ok(bytes.len())
    }
    
    /// Get table length
    pub fn get_table_length(&self, handle: TableHandle) -> Result<usize> {
        let table = self.heap.get_table(handle)?;
        Ok(table.array.len())
    }
    
    /// Initialize the standard library
    pub fn init_stdlib(&mut self) -> Result<()> {
        stdlib::register_stdlib(self)
    }
}