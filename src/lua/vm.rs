//! Lua Virtual Machine Implementation
//! 
//! This module implements the core VM execution engine using a non-recursive
//! state machine approach with transaction-based heap access.

use super::error::{LuaError, LuaResult};
use super::handle::{StringHandle, TableHandle, ClosureHandle, ThreadHandle};
use super::heap::LuaHeap;
use super::transaction::HeapTransaction;
use super::value::{Value, CallFrame, Closure, CFunction};
use crate::storage::StorageEngine;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Bytecode instruction format (simplified for now)
#[derive(Debug, Clone, Copy)]
pub struct Instruction(u32);

impl Instruction {
    pub fn opcode(&self) -> OpCode {
        OpCode::from_u8((self.0 & 0x3F) as u8)
    }
    
    pub fn a(&self) -> u8 {
        ((self.0 >> 6) & 0xFF) as u8
    }
    
    pub fn b(&self) -> u8 {
        ((self.0 >> 23) & 0x1FF) as u8
    }
    
    pub fn c(&self) -> u8 {
        ((self.0 >> 14) & 0x1FF) as u8
    }
    
    pub fn bx(&self) -> u32 {
        (self.0 >> 14) & 0x3FFFF
    }
    
    pub fn sbx(&self) -> i32 {
        (self.bx() as i32) - 131071
    }
}

/// Lua opcodes (subset for initial implementation)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpCode {
    Move,       // R(A) := R(B)
    LoadK,      // R(A) := Kst(Bx)
    LoadBool,   // R(A) := (Bool)B; if (C) pc++
    LoadNil,    // R(A), R(A+1), ..., R(A+B) := nil
    GetGlobal,  // R(A) := Gbl[Kst(Bx)]
    SetGlobal,  // Gbl[Kst(Bx)] := R(A)
    GetTable,   // R(A) := R(B)[RK(C)]
    SetTable,   // R(A)[RK(B)] := RK(C)
    Add,        // R(A) := RK(B) + RK(C)
    Sub,        // R(A) := RK(B) - RK(C)
    Mul,        // R(A) := RK(B) * RK(C)
    Div,        // R(A) := RK(B) / RK(C)
    Call,       // R(A), ..., R(A+C-2) := R(A)(R(A+1), ..., R(A+B-1))
    Return,     // return R(A), ..., R(A+B-2)
    Unknown,
}

impl OpCode {
    fn from_u8(value: u8) -> Self {
        match value {
            0 => OpCode::Move,
            1 => OpCode::LoadK,
            2 => OpCode::LoadBool,
            3 => OpCode::LoadNil,
            5 => OpCode::GetGlobal,
            7 => OpCode::SetGlobal,
            9 => OpCode::GetTable,
            11 => OpCode::SetTable,
            12 => OpCode::Add,
            13 => OpCode::Sub,
            14 => OpCode::Mul,
            15 => OpCode::Div,
            28 => OpCode::Call,
            30 => OpCode::Return,
            _ => OpCode::Unknown,
        }
    }
}

/// Pending operation to be processed by the VM
#[derive(Debug, Clone)]
pub enum PendingOperation {
    /// Function call
    FunctionCall {
        closure: ClosureHandle,
        args: Vec<Value>,
        context: ReturnContext,
    },
    
    /// C function call result
    CFunctionReturn {
        values: Vec<Value>,
        context: ReturnContext,
    },
    
    /// Metamethod call
    MetamethodCall {
        method_name: StringHandle,
        object: Value,
        args: Vec<Value>,
        context: ReturnContext,
    },
    
    /// Table index operation with metamethod
    TableIndex {
        table: TableHandle,
        key: Value,
        context: ReturnContext,
    },
    
    /// Table newindex operation with metamethod
    TableNewIndex {
        table: TableHandle,
        key: Value,
        value: Value,
    },
    
    /// Arithmetic operation with metamethod
    ArithmeticOp {
        op: ArithmeticOperation,
        left: Value,
        right: Value,
        context: ReturnContext,
    },
}

/// Arithmetic operations that may trigger metamethods
#[derive(Debug, Clone, Copy)]
pub enum ArithmeticOperation {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Unm,
}

/// Context for where to return values
#[derive(Debug, Clone)]
pub enum ReturnContext {
    /// Store in register at base + offset
    Register { base: u16, offset: usize },
    
    /// Store as table field
    TableField { table: TableHandle, key: Value },
    
    /// Push to stack
    Stack,
    
    /// Final function result
    FinalResult,
}

/// VM execution state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExecutionState {
    /// Ready to execute
    Ready,
    
    /// Currently executing
    Running,
    
    /// Yielded (coroutine)
    Yielded,
    
    /// Completed successfully
    Completed,
    
    /// Error occurred
    Error,
}

/// Result of executing a single step
#[derive(Debug, Clone)]
pub enum StepResult {
    /// Continue execution
    Continue,
    
    /// Function returned value(s)
    Return(Vec<Value>),
    
    /// Yield (coroutine)
    Yield(Value),
    
    /// Error occurred
    Error(LuaError),
}

/// Execution context for C functions (isolated from VM internals)
pub struct ExecutionContext<'vm> {
    // Stack and argument information
    stack_base: usize,
    arg_count: usize,
    thread: ThreadHandle,
    
    // Private handle to VM for controlled access
    vm_access: &'vm mut LuaVM,
}

impl<'vm> ExecutionContext<'vm> {
    // Create a new execution context from VM
    fn new(vm: &'vm mut LuaVM, stack_base: usize, arg_count: usize, thread: ThreadHandle) -> Self {
        Self {
            stack_base,
            arg_count,
            thread,
            vm_access: vm,
        }
    }
    
    // Get argument count
    pub fn arg_count(&self) -> usize {
        self.arg_count
    }
    
    // Get an argument by index
    pub fn get_arg(&mut self, index: usize) -> LuaResult<Value> {
        if index >= self.arg_count {
            return Err(LuaError::ArgumentError {
                expected: self.arg_count,
                got: index,
            });
        }
        
        // Create a fresh transaction for each operation
        let mut tx = HeapTransaction::new(&mut self.vm_access.heap);
        let value = tx.read_register(self.thread, self.stack_base + index)?;
        tx.commit()?;
        
        Ok(value)
    }
    
    // Get an argument as a string
    pub fn get_arg_str(&mut self, index: usize) -> LuaResult<String> {
        let value = self.get_arg(index)?;
        
        match value {
            Value::String(handle) => {
                // Create a fresh transaction for string access
                let mut tx = HeapTransaction::new(&mut self.vm_access.heap);
                let result = tx.get_string_value(handle)?;
                tx.commit()?;
                Ok(result)
            },
            _ => Err(LuaError::TypeError {
                expected: "string".to_string(),
                got: value.type_name().to_string(),
            }),
        }
    }
    
    // Push a result value
    pub fn push_result(&mut self, value: Value) -> LuaResult<()> {
        // Create a fresh transaction
        let mut tx = HeapTransaction::new(&mut self.vm_access.heap);
        tx.set_register(self.thread, self.stack_base, value)?;
        tx.commit()?;
        
        Ok(())
    }
}

/// Main Lua Virtual Machine
pub struct LuaVM {
    /// Lua heap
    heap: LuaHeap,
    
    /// Current thread handle
    current_thread: ThreadHandle,
    
    /// Queue of pending operations
    pending_operations: VecDeque<PendingOperation>,
    
    /// Current execution state
    execution_state: ExecutionState,
    
    /// Return contexts for active calls
    return_contexts: HashMap<usize, ReturnContext>,
    
    /// Instruction count (for limits)
    instruction_count: u64,
    
    /// Maximum instructions before timeout
    max_instructions: u64,
    
    /// Start time for timeout checking
    start_time: Option<Instant>,
    
    /// Execution timeout
    timeout: Option<Duration>,
    
    /// Script context (Redis integration)
    script_context: Option<crate::lua::ScriptContext>,
}

impl LuaVM {
    /// Get mutable access to the heap for transaction creation
    pub fn heap_mut(&mut self) -> &mut LuaHeap {
        &mut self.heap
    }
    
    /// Example method showing proper handle validation with ValidScope
    /// This demonstrates the recommended pattern for operations on multiple handles
    pub fn example_table_operation(&mut self, table: TableHandle, key: Value) -> LuaResult<Value> {
        // Create a transaction
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Create local variables to store results outside the scope
        let mut result = Value::Nil;
        
        // Perform operations within a scope
        {
            // Create a validation scope
            let mut scope = tx.validation_scope();
            
            // Validate and use the table handle
            let temp_result = scope.use_handle(&table, |scope| {
                // The handle is now validated, we can use it safely
                let tx = scope.transaction();
                
                // Read the table field
                tx.read_table_field(table, &key)
            })?;
            
            result = temp_result;
            
            // If we need to use multiple handles, we can validate them all in sequence
            let metatable_opt = {
                // Get the table's metatable
                let tx = scope.transaction();
                tx.get_table_metatable(table)?
            };
            
            // If metatable exists, we need to validate and use it
            if let Some(metatable) = metatable_opt {
                scope.use_handle(&metatable, |scope| {
                    // Use the metatable...
                    let tx = scope.transaction();
                    
                    // For example, read a metamethod
                    let mm_key = Value::String(tx.create_string("__index")?);
                    let _mm_value = tx.read_table_field(metatable, &mm_key)?;
                    
                    Ok(())
                })?;
            }
        }
        
        // Now we can commit the transaction
        tx.commit()?;
        
        Ok(result)
    }
    
    /// Create a new VM instance
    pub fn new() -> LuaResult<Self> {
        let heap = LuaHeap::new()?;
        let main_thread = heap.main_thread()?;
        
        Ok(LuaVM {
            heap,
            current_thread: main_thread,
            pending_operations: VecDeque::new(),
            execution_state: ExecutionState::Ready,
            return_contexts: HashMap::new(),
            instruction_count: 0,
            max_instructions: 1_000_000,
            start_time: None,
            timeout: None,
            script_context: None,
        })
    }
    
    /// Set the script execution context
    pub fn set_context(&mut self, context: crate::lua::ScriptContext) -> LuaResult<()> {
        self.timeout = Some(context.timeout);
        self.script_context = Some(context);
        
        // TODO: Setup KEYS and ARGV tables
        
        Ok(())
    }
    
    /// Evaluate a script and return the result
    pub fn eval_script(&mut self, _script: &str) -> LuaResult<Value> {
        // TODO: Compile script
        // For now, return a placeholder
        
        self.start_time = Some(Instant::now());
        
        // TODO: Execute compiled script
        
        // Return a placeholder string for now
        let mut tx = HeapTransaction::new(&mut self.heap);
        let handle = tx.create_string("placeholder result")?;
        tx.commit()?;
        
        Ok(Value::String(handle))
    }
    
    /// Execute a function with arguments
    pub fn execute_function(&mut self, closure: ClosureHandle, args: &[Value]) -> LuaResult<Value> {
        // Record initial call depth
        let initial_depth = self.get_call_depth()?;
        
        // Queue initial function call
        self.pending_operations.push_back(PendingOperation::FunctionCall {
            closure,
            args: args.to_vec(),
            context: ReturnContext::FinalResult,
        });
        
        // Set execution state
        self.execution_state = ExecutionState::Running;
        self.start_time = Some(Instant::now());
        
        // Initialize result
        let mut final_result = Value::Nil;
        
        // Main execution loop - NO RECURSION
        loop {
            // Check termination conditions
            if self.should_kill() {
                return Err(LuaError::ScriptKilled);
            }
            
            if self.check_timeout() {
                return Err(LuaError::Timeout);
            }
            
            if self.instruction_count > self.max_instructions {
                return Err(LuaError::InstructionLimitExceeded);
            }
            
            // Process pending operations first
            if !self.pending_operations.is_empty() {
                let op = self.pending_operations.pop_front().unwrap();
                match self.process_pending_operation(op)? {
                    StepResult::Continue => continue,
                    StepResult::Return(values) => {
                        if !values.is_empty() {
                            final_result = values[0].clone();
                        }
                        
                        // Check if we're back to initial depth
                        if self.get_call_depth()? <= initial_depth {
                            break;
                        }
                    }
                    StepResult::Yield(_) => {
                        return Err(LuaError::NotImplemented("coroutines".to_string()));
                    }
                    StepResult::Error(e) => {
                        return Err(e);
                    }
                }
            } else {
                // Execute next instruction
                match self.step()? {
                    StepResult::Continue => continue,
                    StepResult::Return(values) => {
                        if !values.is_empty() {
                            final_result = values[0].clone();
                        }
                        
                        // Check if we're back to initial depth
                        if self.get_call_depth()? <= initial_depth {
                            break;
                        }
                    }
                    StepResult::Yield(_) => {
                        return Err(LuaError::NotImplemented("coroutines".to_string()));
                    }
                    StepResult::Error(e) => {
                        return Err(e);
                    }
                }
            }
        }
        
        self.execution_state = ExecutionState::Completed;
        Ok(final_result)
    }
    
    fn step(&mut self) -> LuaResult<StepResult> {
        // Increment instruction count
        self.instruction_count += 1;
        
        // Create transaction
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Get current frame
        let frame = tx.get_current_frame(self.current_thread)?;
        
        // Get instruction
        let instr = tx.get_instruction(frame.closure, frame.pc)?;
        let instruction = Instruction(instr);
        
        // Execute instruction inline
        let opcode = instruction.opcode();
        let a = instruction.a() as usize;
        let b = instruction.b() as usize;
        let c = instruction.c() as usize;
        
        // Default: increment PC unless instruction handles it
        let mut should_increment_pc = true;
        
        let result = match opcode {
            OpCode::Move => {
                // R(A) := R(B)
                let base = frame.base_register as usize;
                let value = tx.read_register(self.current_thread, base + b)?;
                tx.set_register(self.current_thread, base + a, value)?;
                StepResult::Continue
            }
            
            OpCode::LoadK => {
                // R(A) := Kst(Bx)
                let base = frame.base_register as usize;
                
                // Inline get_constant to avoid self-borrowing
                let closure_obj = tx.get_closure(frame.closure)?;
                let bx = instruction.bx() as usize;
                let constant = closure_obj.proto.constants.get(bx)
                    .cloned()
                    .ok_or_else(|| LuaError::RuntimeError(format!(
                        "Constant index {} out of bounds",
                        bx
                    )))?;
                
                tx.set_register(self.current_thread, base + a, constant)?;
                StepResult::Continue
            }
            
            OpCode::LoadBool => {
                // R(A) := (Bool)B; if (C) pc++
                let base = frame.base_register as usize;
                let value = Value::Boolean(b != 0);
                tx.set_register(self.current_thread, base + a, value)?;
                
                if c != 0 {
                    // Skip next instruction
                    tx.increment_pc(self.current_thread)?;
                }
                
                StepResult::Continue
            }
            
            OpCode::LoadNil => {
                // R(A), R(A+1), ..., R(A+B) := nil
                let base = frame.base_register as usize;
                for i in 0..=b {
                    tx.set_register(self.current_thread, base + a + i, Value::Nil)?;
                }
                StepResult::Continue
            }
            
            OpCode::Return => {
                // return R(A), ..., R(A+B-2)
                let base = frame.base_register as usize;
                let mut values = Vec::new();
                
                if b == 0 {
                    // Return all values from R(A) to top
                    let top = tx.get_stack_top(self.current_thread)?;
                    for i in a..=(top - base) {
                        values.push(tx.read_register(self.current_thread, base + i)?);
                    }
                } else {
                    // Return B-1 values
                    for i in 0..(b - 1) {
                        values.push(tx.read_register(self.current_thread, base + a + i)?);
                    }
                }
                
                // Pop call frame
                tx.pop_call_frame(self.current_thread)?;
                
                // Don't increment PC - we're returning
                should_increment_pc = false;
                
                StepResult::Return(values)
            }
            
            OpCode::Call => {
                // R(A), ..., R(A+C-2) := R(A)(R(A+1), ..., R(A+B-1))
                let base = frame.base_register as usize;
                let func = tx.read_register(self.current_thread, base + a)?;
                
                // Gather arguments
                let arg_count = if b == 0 {
                    // Use all values from R(A+1) to top
                    let top = tx.get_stack_top(self.current_thread)?;
                    top - base - a
                } else {
                    b - 1
                };
                
                let mut args = Vec::with_capacity(arg_count);
                for i in 0..arg_count {
                    args.push(tx.read_register(self.current_thread, base + a + 1 + i)?);
                }
                
                // Process based on function type
                match func {
                    Value::Closure(closure) => {
                        // Queue function call for later execution
                        tx.queue_operation(PendingOperation::FunctionCall {
                            closure,
                            args,
                            context: ReturnContext::Register {
                                base: frame.base_register,
                                offset: a,
                            },
                        })?;
                        
                        // Continue normal execution
                        StepResult::Continue
                    },
                    Value::CFunction(cfunc) => {
                        // For C functions, we need special handling
                        // First increment the PC
                        tx.increment_pc(self.current_thread)?;
                        
                        // Commit the transaction
                        tx.commit()?;
                        
                        // Extract arguments we need to avoid self-borrowing conflicts
                        let func_copy = cfunc; // CFunction implements Copy
                        let args_copy = args; // Clone the args
                        let base_register = frame.base_register;
                        let register_a = a;
                        let thread_handle = self.current_thread;
                        
                        // Call the handler with clean borrows
                        return self.handle_c_function_call(
                            func_copy,
                            args_copy,
                            base_register,
                            register_a,
                            thread_handle
                        );
                    },
                    _ => {
                        // Not a function - return error after committing
                        tx.commit()?;
                        return Err(LuaError::TypeError { 
                            expected: "function".to_string(), 
                            got: func.type_name().to_string(),
                        });
                    },
                }
            }
            
            _ => {
                tx.commit()?;
                return Err(LuaError::NotImplemented(format!("Opcode {:?}", opcode)));
            }
        };
        
        // Increment PC if needed
        if should_increment_pc {
            tx.increment_pc(self.current_thread)?;
        }
        
        // Commit transaction and get pending operations
        let pending_ops = tx.commit()?;
        
        // Queue any new pending operations
        for op in pending_ops {
            self.pending_operations.push_back(op);
        }
        
        Ok(result)
    }
}

impl LuaVM {
    // Handle C function call without borrowing self more than once
    fn handle_c_function_call(
        &mut self,
        func: CFunction,
        args: Vec<Value>,
        base_register: u16,
        register_a: usize,
        thread_handle: ThreadHandle,
    ) -> LuaResult<StepResult> {
        // Setup C execution context
        let stack_base = base_register as usize + register_a;
        let mut ctx = ExecutionContext::new(self, stack_base, args.len(), thread_handle);
        
        // Call C function with isolated context
        let result_count = match func(&mut ctx) {
            Ok(count) => count as usize,
            Err(e) => return Ok(StepResult::Error(e)),
        };
        
        // Collect results after function returns
        let mut results = Vec::with_capacity(result_count);
        
        // Create a new transaction to read the results
        let mut tx = HeapTransaction::new(&mut self.heap);
        for i in 0..result_count {
            if let Ok(value) = tx.read_register(thread_handle, stack_base + i) {
                results.push(value);
            } else {
                break;
            }
        }
        
        // Queue results as a CFunctionReturn operation
        tx.queue_operation(PendingOperation::CFunctionReturn {
            values: results,
            context: ReturnContext::Register {
                base: base_register,
                offset: register_a,
            },
        })?;
        tx.commit()?;
        
        Ok(StepResult::Continue)
    }

    // Process C function return values
    fn process_c_function_return(
        &mut self, 
        values: Vec<Value>, 
        context: ReturnContext
    ) -> LuaResult<StepResult> {
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        match context {
            ReturnContext::Register { base, offset } => {
                // Store return values in the appropriate registers
                if !values.is_empty() {
                    // Store the first return value in the specified register
                    tx.set_register(self.current_thread, base as usize + offset, values[0].clone())?;
                    
                    // Store any additional return values in consecutive registers
                    for (i, value) in values.iter().skip(1).enumerate() {
                        tx.set_register(self.current_thread, 
                                       base as usize + offset + 1 + i, 
                                       value.clone())?;
                    }
                } else {
                    tx.set_register(self.current_thread, base as usize + offset, Value::Nil)?;
                }
            },
            ReturnContext::FinalResult => {
                // Final result will be handled by execute_function
            },
            ReturnContext::TableField { table, key } => {
                // Store in table
                if !values.is_empty() {
                    tx.set_table_field(table, key, values[0].clone())?;
                } else {
                    tx.set_table_field(table, key, Value::Nil)?;
                }
            },
            ReturnContext::Stack => {
                // Push to stack
                for value in values {
                    tx.push_stack(self.current_thread, value)?;
                }
            },
        }
        
        tx.commit()?;
        Ok(StepResult::Continue)
    }
    
    // Update process_pending_operation to handle C function returns
    fn process_pending_operation(&mut self, op: PendingOperation) -> LuaResult<StepResult> {
        match op {
            PendingOperation::FunctionCall { closure, args, context } => {
                self.process_function_call(closure, args, context)
            },
            PendingOperation::CFunctionReturn { values, context } => {
                self.process_c_function_return(values, context)
            },
            _ => Err(LuaError::NotImplemented("Pending operation type".to_string())),
        }
    }
    
    /// Process a function call
    fn process_function_call(
        &mut self,
        closure: ClosureHandle,
        args: Vec<Value>,
        context: ReturnContext,
    ) -> LuaResult<StepResult> {
        // First transaction for function call setup
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Get closure info
        let closure_obj = tx.get_closure(closure)?;
        let num_params = closure_obj.proto.num_params as usize;
        let is_vararg = closure_obj.proto.is_vararg;
        let max_stack = closure_obj.proto.max_stack_size as usize;
        
        // Determine stack base for new frame
        let current_top = tx.get_stack_size(self.current_thread)?;
        let new_base = current_top;
        
        // Push parameters
        for i in 0..num_params {
            let value = if i < args.len() {
                args[i].clone()
            } else {
                Value::Nil
            };
            tx.push_stack(self.current_thread, value)?;
        }
        
        // Handle varargs if needed
        if is_vararg && args.len() > num_params {
            // TODO: Handle varargs
        }
        
        // Reserve stack space
        for _ in num_params..max_stack {
            tx.push_stack(self.current_thread, Value::Nil)?;
        }
        
        // Create new call frame
        let new_frame = CallFrame {
            closure,
            pc: 0,
            base_register: new_base as u16,
            expected_results: match &context {
                ReturnContext::Register { .. } => Some(1),
                _ => None,
            },
        };
        
        // Push call frame
        tx.push_call_frame(self.current_thread, new_frame)?;
        
        // Commit first transaction
        tx.commit()?;
        
        // Store return context in a separate scope to avoid borrow issues
        let call_depth = {
            // Second transaction just to get call depth
            let mut tx2 = HeapTransaction::new(&mut self.heap);
            
            // Get current call depth (after we pushed the frame)
            let frame_count = tx2.get_thread_call_depth(self.current_thread)?;
            tx2.commit()?;
            frame_count
        };
        
        // Store return context
        self.return_contexts.insert(call_depth, context);
        
        Ok(StepResult::Continue)
    }
    
    /// Get a constant from a closure
    fn get_constant(&self, tx: &mut HeapTransaction, closure: ClosureHandle, index: usize) -> LuaResult<Value> {
        let closure_obj = tx.get_closure(closure)?;
        
        closure_obj.proto.constants.get(index)
            .cloned()
            .ok_or(LuaError::RuntimeError(format!(
                "Constant index {} out of bounds",
                index
            )))
    }
    
    /// Get call depth by counting frames
    fn get_call_depth(&mut self) -> LuaResult<usize> {
        let mut tx = HeapTransaction::new(&mut self.heap);
        let depth = tx.get_thread_call_depth(self.current_thread)?;
        tx.commit()?;
        Ok(depth)
    }
    
    /// Check if we should kill the script
    fn should_kill(&self) -> bool {
        // TODO: Check kill flag
        false
    }
    
    /// Check if we've exceeded the timeout
    fn check_timeout(&self) -> bool {
        if let (Some(start), Some(timeout)) = (self.start_time, self.timeout) {
            start.elapsed() > timeout
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_vm_creation() {
        let vm = LuaVM::new().unwrap();
        assert_eq!(vm.execution_state, ExecutionState::Ready);
        assert_eq!(vm.instruction_count, 0);
    }
    
    #[test]
    fn test_instruction_parsing() {
        // Test MOVE instruction: opcode=0, A=1, B=2
        let instr = Instruction(0x00004040);
        assert_eq!(instr.opcode(), OpCode::Move);
        assert_eq!(instr.a(), 1);
        assert_eq!(instr.b(), 0);
        
        // Test LOADK instruction: opcode=1, A=0, Bx=1
        let instr = Instruction(0x00004001);
        assert_eq!(instr.opcode(), OpCode::LoadK);
        assert_eq!(instr.a(), 0);
        assert_eq!(instr.bx(), 1);
    }
}