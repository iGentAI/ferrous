//! Lua VM implementation with generational arena architecture

use crate::lua_new::heap::{LuaHeap, ThreadObject, CallFrame, ThreadStatus};
use crate::lua_new::value::{Value, StringHandle, TableHandle, ClosureHandle, ThreadHandle, 
                             FunctionProto, UpvalueRef, Instruction, OpCode};
use crate::lua_new::error::{LuaError, Result};
use crate::lua_new::{VMConfig, LuaLimits};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Execution context for built-in functions
pub struct ExecutionContext<'a> {
    /// The VM instance
    pub vm: &'a mut LuaVM,
    
    /// Base register for this call
    pub base: usize,
    
    /// Number of arguments
    pub arg_count: usize,
}

impl<'a> ExecutionContext<'a> {
    /// Get the number of arguments
    pub fn get_arg_count(&self) -> usize {
        self.arg_count
    }
    
    /// Get an argument value
    pub fn get_arg(&self, index: usize) -> Result<Value> {
        if index >= self.arg_count {
            return Err(LuaError::Runtime(format!("argument {} out of bounds", index)));
        }
        
        let thread = self.vm.heap.get_thread(self.vm.current_thread)?;
        let stack_idx = self.base + index;
        
        thread.stack.get(stack_idx)
            .copied()
            .ok_or_else(|| LuaError::StackUnderflow)
    }
    
    /// Get a string argument
    pub fn get_string_arg(&self, index: usize) -> Result<StringHandle> {
        match self.get_arg(index)? {
            Value::String(s) => Ok(s),
            _ => Err(LuaError::TypeError(format!("argument {} must be a string", index))),
        }
    }
    
    /// Push a return value
    pub fn push_result(&mut self, value: Value) -> Result<()> {
        let thread = self.vm.heap.get_thread_mut(self.vm.current_thread)?;
        thread.stack.push(value);
        Ok(())
    }
    
    /// Access the heap
    pub fn heap(&mut self) -> &mut LuaHeap {
        &mut self.vm.heap
    }
    
    /// Get the current thread handle
    pub fn current_thread(&self) -> ThreadHandle {
        self.vm.current_thread
    }
}

/// Execution status
#[derive(Debug, Clone, Copy)]
pub enum ExecutionStatus {
    /// Continue execution
    Continue,
    
    /// Return from function
    Return(Value),
    
    /// Yield (coroutine)
    Yield(Value),
}

// Implement Clone for CallFrame
impl Clone for CallFrame {
    fn clone(&self) -> Self {
        CallFrame {
            closure: self.closure,
            pc: self.pc,
            base_register: self.base_register,
            return_count: self.return_count,
        }
    }
}

/// The Lua virtual machine
pub struct LuaVM {
    /// Memory heap
    pub heap: LuaHeap,
    
    /// Currently executing thread
    pub current_thread: ThreadHandle,
    
    /// Configuration options
    pub config: VMConfig,
    
    /// Resource tracking
    pub instruction_count: u64,
    
    /// Kill flag for script termination
    pub kill_flag: Option<Arc<AtomicBool>>,
    
    /// Global environment table
    pub globals: TableHandle,
    
    /// Registry table
    pub registry: TableHandle,
}

impl LuaVM {
    /// Create a new VM instance
    pub fn new(config: VMConfig) -> Self {
        let mut heap = LuaHeap::new();
        
        // Create main thread
        let main_thread = heap.alloc_thread();
        
        // Create global environment
        let globals = heap.alloc_table();
        
        // Create registry
        let registry = heap.alloc_table();
        
        LuaVM {
            heap,
            current_thread: main_thread,
            config,
            instruction_count: 0,
            kill_flag: None,
            globals,
            registry,
        }
    }
    
    /// Set kill flag for script termination
    pub fn set_kill_flag(&mut self, flag: Arc<AtomicBool>) {
        self.kill_flag = Some(flag);
    }
    
    /// Check if script should be killed
    pub fn check_kill(&self) -> Result<()> {
        if let Some(ref flag) = self.kill_flag {
            if flag.load(Ordering::Relaxed) {
                return Err(LuaError::ScriptKilled);
            }
        }
        Ok(())
    }
    
    /// Check resource limits
    pub fn check_limits(&mut self) -> Result<()> {
        // Check kill flag
        self.check_kill()?;
        
        // Increment and check instruction count
        self.instruction_count += 1;
        if self.instruction_count > self.config.limits.instruction_limit {
            return Err(LuaError::InstructionLimit);
        }
        
        // Check memory usage
        if self.heap.stats.allocated > self.config.limits.memory_limit {
            return Err(LuaError::MemoryLimit);
        }
        
        // Check stack depth
        let thread = self.heap.get_thread(self.current_thread)?;
        if thread.call_frames.len() > self.config.limits.call_stack_limit {
            return Err(LuaError::StackOverflow);
        }
        
        if thread.stack.len() > self.config.limits.value_stack_limit {
            return Err(LuaError::StackOverflow);
        }
        
        Ok(())
    }
    
    /// Execute a function
    pub fn execute_function(&mut self, closure: ClosureHandle, args: &[Value]) -> Result<Value> {
        // Push a new call frame
        self.push_call_frame(closure, args)?;
        
        // Execute until return
        loop {
            match self.step()? {
                ExecutionStatus::Continue => continue,
                ExecutionStatus::Return(value) => {
                    self.pop_call_frame()?;
                    return Ok(value);
                }
                ExecutionStatus::Yield(_) => {
                    return Err(LuaError::NotImplemented("coroutines"));
                }
            }
        }
    }
    
    /// Execute with custom limit checking
    pub fn execute_with_limits(
        &mut self, 
        closure: ClosureHandle, 
        args: &[Value],
        kill_flag: Arc<AtomicBool>
    ) -> Result<Value> {
        self.set_kill_flag(kill_flag);
        self.execute_function(closure, args)
    }
    
    /// Push a new call frame
    fn push_call_frame(&mut self, closure: ClosureHandle, args: &[Value]) -> Result<()> {
        // Get proto from closure
        let proto = {
            let closure_obj = self.heap.get_closure(closure)?;
            closure_obj.proto.clone()
        };
        
        // Get thread
        let thread = self.heap.get_thread_mut(self.current_thread)?;
        
        // Check call stack depth
        if thread.call_frames.len() >= self.config.limits.call_stack_limit {
            return Err(LuaError::StackOverflow);
        }
        
        // Record base register
        let base_register = thread.stack.len() as u16;
        
        // Push arguments
        for i in 0..proto.param_count as usize {
            if i < args.len() {
                thread.stack.push(args[i]);
            } else {
                thread.stack.push(Value::Nil);
            }
        }
        
        // Allocate space for locals
        let stack_size = proto.max_stack_size as usize;
        thread.stack.resize(base_register as usize + stack_size, Value::Nil);
        
        // Push call frame
        thread.call_frames.push(CallFrame {
            closure,
            pc: 0,
            base_register,
            return_count: 1,
        });
        
        Ok(())
    }
    
    /// Pop a call frame
    fn pop_call_frame(&mut self) -> Result<CallFrame> {
        let thread = self.heap.get_thread_mut(self.current_thread)?;
        
        let frame = thread.call_frames.pop()
            .ok_or(LuaError::StackUnderflow)?;
        
        // Restore stack
        thread.stack.truncate(frame.base_register as usize);
        
        Ok(frame)
    }
    
    /// Execute a single instruction
    pub fn step(&mut self) -> Result<ExecutionStatus> {
        // Check limits
        self.check_limits()?;
        
        // Get current instruction
        let (frame, instr) = {
            let thread = self.heap.get_thread(self.current_thread)?;
            let frame = thread.call_frames.last()
                .ok_or(LuaError::StackUnderflow)?;
            
            let closure = self.heap.get_closure(frame.closure)?;
            let instr = closure.proto.code.get(frame.pc)
                .ok_or(LuaError::InvalidProgramCounter)?;
            
            // Clone frame and copy instruction
            (frame.clone(), *instr)
        };
        
        // Increment PC
        {
            let thread = self.heap.get_thread_mut(self.current_thread)?;
            if let Some(frame) = thread.call_frames.last_mut() {
                frame.pc += 1;
            }
        }
        
        // Execute instruction
        self.execute_instruction(frame, instr)
    }
    
    /// Execute a single instruction
    fn execute_instruction(&mut self, frame: CallFrame, instr: Instruction) -> Result<ExecutionStatus> {
        let op = OpCode::from(instr.opcode());
        let a = instr.a() as usize;
        let b = instr.b() as usize;
        let c = instr.c() as usize;
        
        if self.config.debug {
            println!("VM: {:?} A={} B={} C={}", op, a, b, c);
        }
        
        match op {
            OpCode::Move => {
                // R(A) := R(B)
                let value = self.get_register(frame.base_register, b)?;
                self.set_register(frame.base_register, a, value)?;
            }
            
            OpCode::LoadK => {
                // R(A) := Kst(Bx)
                let bx = instr.bx() as usize;
                let constant = self.get_constant(frame.closure, bx)?;
                self.set_register(frame.base_register, a, constant)?;
            }
            
            OpCode::LoadBool => {
                // R(A) := (Bool)B; if (C) pc++
                let value = Value::Boolean(b != 0);
                self.set_register(frame.base_register, a, value)?;
                
                if c != 0 {
                    // Skip next instruction
                    let thread = self.heap.get_thread_mut(self.current_thread)?;
                    if let Some(frame) = thread.call_frames.last_mut() {
                        frame.pc += 1;
                    }
                }
            }
            
            OpCode::LoadNil => {
                // R(A), R(A+1), ..., R(A+B) := nil
                for i in 0..=b {
                    self.set_register(frame.base_register, a + i, Value::Nil)?;
                }
            }
            
            OpCode::GetUpval => {
                // R(A) := UpValue[B]
                let value = self.get_upvalue(frame.closure, b)?;
                self.set_register(frame.base_register, a, value)?;
            }
            
            OpCode::GetGlobal => {
                // R(A) := Gbl[Kst(Bx)]
                let bx = instr.bx() as usize;
                let key = self.get_constant(frame.closure, bx)?;
                let value = self.table_get(self.globals, key)?;
                self.set_register(frame.base_register, a, value)?;
            }
            
            OpCode::GetTable => {
                // R(A) := R(B)[R(C)]
                let table_val = self.get_register(frame.base_register, b)?;
                let key = if c >= 0x100 {
                    // This is an RK format (constant with high bit set)
                    // Extract the constant index (remove the high bit)
                    let const_idx = c & 0xFF;
                    
                    // Get the constant from closure constants
                    let closure_obj = self.heap.get_closure(frame.closure)?;
                    if const_idx >= closure_obj.proto.constants.len() {
                        return Err(LuaError::InvalidOperation(format!("constant index {} out of bounds", const_idx)));
                    }
                    
                    closure_obj.proto.constants[const_idx].clone()
                } else {
                    // Regular register
                    self.get_register(frame.base_register, c)?
                };
                
                if let Value::Table(table) = table_val {
                    let value = self.table_get(table, key)?;
                    self.set_register(frame.base_register, a, value)?;
                } else {
                    return Err(LuaError::TypeError("attempt to index a non-table".to_string()));
                }
            }
            
            OpCode::SetGlobal => {
                // Gbl[Kst(Bx)] := R(A)
                let bx = instr.bx() as usize;
                let key = self.get_constant(frame.closure, bx)?;
                let value = self.get_register(frame.base_register, a)?;
                self.table_set(self.globals, key, value)?;
            }
            
            OpCode::SetUpval => {
                // UpValue[B] := R(A)
                let value = self.get_register(frame.base_register, a)?;
                self.set_upvalue(frame.closure, b, value)?;
            }
            
            OpCode::SetTable => {
                // R(A)[R(B)] := R(C)
                let table_val = self.get_register(frame.base_register, a)?;
                let key = if b >= 0x100 {
                    // This is an RK format (constant with high bit set)
                    // Extract the constant index (remove the high bit)
                    let const_idx = b & 0xFF;
                    
                    // Get the constant from closure constants
                    let closure_obj = self.heap.get_closure(frame.closure)?;
                    if const_idx >= closure_obj.proto.constants.len() {
                        return Err(LuaError::InvalidOperation(format!("constant index {} out of bounds", const_idx)));
                    }
                    
                    closure_obj.proto.constants[const_idx].clone()
                } else {
                    // Regular register
                    self.get_register(frame.base_register, b)?
                };
                
                let value = self.get_register(frame.base_register, c)?;
                
                if let Value::Table(table) = table_val {
                    self.table_set(table, key, value)?;
                } else {
                    return Err(LuaError::TypeError("attempt to index a non-table".to_string()));
                }
            }
            
            OpCode::NewTable => {
                // R(A) := {} (size = B,C)
                let table = self.heap.alloc_table();
                self.set_register(frame.base_register, a, Value::Table(table))?;
            }
            
            OpCode::Self_ => {
                // R(A+1) := R(B); R(A) := R(B)[RK(C)]
                let table_val = self.get_register(frame.base_register, b)?;
                let key = self.get_rk(&frame, c)?;
                
                // Set self
                self.set_register(frame.base_register, a + 1, table_val)?;
                
                // Get method
                if let Value::Table(table) = table_val {
                    let method = self.table_get(table, key)?;
                    self.set_register(frame.base_register, a, method)?;
                } else {
                    return Err(LuaError::TypeError("attempt to index a non-table".to_string()));
                }
            }
            
            OpCode::Add => self.execute_arithmetic(&frame, a, b, c, |x, y| x + y)?,
            OpCode::Sub => self.execute_arithmetic(&frame, a, b, c, |x, y| x - y)?,
            OpCode::Mul => self.execute_arithmetic(&frame, a, b, c, |x, y| x * y)?,
            OpCode::Div => self.execute_arithmetic(&frame, a, b, c, |x, y| x / y)?,
            OpCode::Mod => self.execute_arithmetic(&frame, a, b, c, |x, y| x % y)?,
            OpCode::Pow => self.execute_arithmetic(&frame, a, b, c, |x, y| x.powf(y))?,
            
            OpCode::Unm => {
                // R(A) := -R(B)
                let value = self.get_register(frame.base_register, b)?;
                match value {
                    Value::Number(n) => {
                        self.set_register(frame.base_register, a, Value::Number(-n))?;
                    }
                    _ => return Err(LuaError::TypeError("attempt to negate a non-number".to_string())),
                }
            }
            
            OpCode::Not => {
                // R(A) := not R(B)
                let value = self.get_register(frame.base_register, b)?;
                let result = Value::Boolean(!value.to_bool());
                self.set_register(frame.base_register, a, result)?;
            }
            
            OpCode::Len => {
                // R(A) := length of R(B)
                let value = self.get_register(frame.base_register, b)?;
                let len = match value {
                    Value::String(s) => {
                        let bytes = self.heap.get_string(s)?;
                        bytes.len()
                    }
                    Value::Table(t) => {
                        let table = self.heap.get_table(t)?;
                        table.len()
                    }
                    _ => return Err(LuaError::TypeError("attempt to get length of a non-string/table".to_string())),
                };
                self.set_register(frame.base_register, a, Value::Number(len as f64))?;
            }
            
            OpCode::Concat => {
                // R(A) := R(B).. ... ..R(C)
                let mut result = String::new();
                
                for i in b..=c {
                    let value = self.get_register(frame.base_register, i)?;
                    match value {
                        Value::String(s) => {
                            let str = self.heap.get_string_utf8(s)?;
                            result.push_str(str);
                        }
                        Value::Number(n) => {
                            result.push_str(&n.to_string());
                        }
                        _ => return Err(LuaError::TypeError("attempt to concatenate a non-string/number".to_string())),
                    }
                }
                
                let str_handle = self.heap.create_string(&result);
                self.set_register(frame.base_register, a, Value::String(str_handle))?;
            }
            
            OpCode::Jmp => {
                // pc += sBx
                let sbx = instr.sbx();
                let thread = self.heap.get_thread_mut(self.current_thread)?;
                if let Some(frame) = thread.call_frames.last_mut() {
                    if sbx >= 0 {
                        frame.pc = frame.pc.saturating_add(sbx as usize);
                    } else {
                        frame.pc = frame.pc.saturating_sub((-sbx) as usize);
                    }
                }
            }
            
            OpCode::Eq => {
                // Handle equality directly without closure
                let b_val = self.get_rk(&frame, b)?;
                let c_val = self.get_rk(&frame, c)?;
                let result = b_val == c_val;
                
                if result != (a != 0) {
                    // Skip next instruction
                    let thread = self.heap.get_thread_mut(self.current_thread)?;
                    if let Some(frame) = thread.call_frames.last_mut() {
                        frame.pc += 1;
                    }
                }
            }
            
            OpCode::Lt => {
                // Handle less than directly without closure
                let b_val = self.get_rk(&frame, b)?;
                let c_val = self.get_rk(&frame, c)?;
                let result = self.value_lt(&b_val, &c_val);
                
                if result != (a != 0) {
                    // Skip next instruction
                    let thread = self.heap.get_thread_mut(self.current_thread)?;
                    if let Some(frame) = thread.call_frames.last_mut() {
                        frame.pc += 1;
                    }
                }
            }
            
            OpCode::Le => {
                // Handle less than or equal directly without closure
                let b_val = self.get_rk(&frame, b)?;
                let c_val = self.get_rk(&frame, c)?;
                let result = self.value_le(&b_val, &c_val);
                
                if result != (a != 0) {
                    // Skip next instruction
                    let thread = self.heap.get_thread_mut(self.current_thread)?;
                    if let Some(frame) = thread.call_frames.last_mut() {
                        frame.pc += 1;
                    }
                }
            }
            
            OpCode::Test => {
                // if (R(A) <=> C) then pc++
                let value = self.get_register(frame.base_register, a)?;
                if value.to_bool() != (c != 0) {
                    let thread = self.heap.get_thread_mut(self.current_thread)?;
                    if let Some(frame) = thread.call_frames.last_mut() {
                        frame.pc += 1;
                    }
                }
            }
            
            OpCode::TestSet => {
                // if (R(B) <=> C) then R(A) := R(B) else pc++
                let value = self.get_register(frame.base_register, b)?;
                if value.to_bool() == (c != 0) {
                    self.set_register(frame.base_register, a, value)?;
                } else {
                    let thread = self.heap.get_thread_mut(self.current_thread)?;
                    if let Some(frame) = thread.call_frames.last_mut() {
                        frame.pc += 1;
                    }
                }
            }
            
            OpCode::Call => {
                // R(A), ... ,R(A+C-2) := R(A)(R(A+1), ... ,R(A+B-1))
                let func = self.get_register(frame.base_register, a)?;
                
                // Collect arguments
                let arg_count = if b == 0 {
                    // Variable arguments
                    let thread = self.heap.get_thread(self.current_thread)?;
                    thread.stack.len() - frame.base_register as usize - a - 1
                } else {
                    b - 1
                };
                
                let mut args = Vec::with_capacity(arg_count);
                for i in 0..arg_count {
                    args.push(self.get_register(frame.base_register, a + 1 + i)?);
                }
                
                // Call function
                match func {
                    Value::Closure(closure) => {
                        // Save expected returns
                        let returns = if c == 0 { 255 } else { c - 1 };
                        
                        // Push new frame
                        self.push_call_frame(closure, &args)?;
                        
                        // Set return count
                        let thread = self.heap.get_thread_mut(self.current_thread)?;
                        if let Some(frame) = thread.call_frames.last_mut() {
                            frame.return_count = returns as u8;
                        }
                    }
                    Value::CFunction(cfunc) => {
                        // Call C function
                        let mut ctx = ExecutionContext {
                            vm: self,
                            base: frame.base_register as usize + a + 1,
                            arg_count: args.len(),
                        };
                        
                        let ret_count = cfunc(&mut ctx)?;
                        
                        // Move returns to correct positions
                        let thread = self.heap.get_thread_mut(self.current_thread)?;
                        let base = frame.base_register as usize;
                        
                        for i in 0..ret_count as usize {
                            if base + a + i < thread.stack.len() {
                                thread.stack[base + a + i] = thread.stack[base + a + 1 + arg_count + i];
                            }
                        }
                        
                        // Clear extra values
                        thread.stack.truncate(base + a + ret_count as usize);
                    }
                    _ => return Err(LuaError::TypeError("attempt to call a non-function".to_string())),
                }
            }
            
            OpCode::TailCall => {
                // return R(A)(R(A+1), ... ,R(A+B-1))
                // For now, implement as regular call + return
                // TODO: Optimize with proper tail call
                let frame_clone = frame.clone();
                self.execute_instruction(frame_clone.clone(), Instruction::new(
                    (OpCode::Call as u32) | (a as u32) << 6 | (b as u32) << 14 | 0 << 23
                ))?;
                
                return self.execute_instruction(frame_clone, Instruction::new(
                    (OpCode::Return as u32) | (a as u32) << 6 | 0 << 14
                ));
            }
            
            OpCode::Return => {
                // return R(A), ... ,R(A+B-2)
                let ret_count = if b == 0 {
                    // Return all values from A to top
                    let thread = self.heap.get_thread(self.current_thread)?;
                    thread.stack.len() - frame.base_register as usize - a
                } else {
                    b - 1
                };
                
                // Collect return values
                let mut returns = Vec::with_capacity(ret_count);
                for i in 0..ret_count {
                    returns.push(self.get_register(frame.base_register, a + i)?);
                }
                
                // Return first value (or nil)
                return Ok(ExecutionStatus::Return(returns.get(0).copied().unwrap_or(Value::Nil)));
            }
            
            OpCode::ForLoop => {
                // R(A)+=R(A+2); if R(A) <?= R(A+1) then { pc+=sBx; R(A+3)=R(A) }
                let idx = self.get_register(frame.base_register, a)?;
                let limit = self.get_register(frame.base_register, a + 1)?;
                let step = self.get_register(frame.base_register, a + 2)?;
                
                if let (Value::Number(idx_n), Value::Number(limit_n), Value::Number(step_n)) = (idx, limit, step) {
                    let new_idx = idx_n + step_n;
                    self.set_register(frame.base_register, a, Value::Number(new_idx))?;
                    
                    let continue_loop = if step_n > 0.0 {
                        new_idx <= limit_n
                    } else {
                        new_idx >= limit_n
                    };
                    
                    if continue_loop {
                        // Jump back
                        let sbx = instr.sbx();
                        let thread = self.heap.get_thread_mut(self.current_thread)?;
                        if let Some(frame) = thread.call_frames.last_mut() {
                            if sbx >= 0 {
                                frame.pc = frame.pc.saturating_add(sbx as usize);
                            } else {
                                frame.pc = frame.pc.saturating_sub((-sbx) as usize);
                            }
                        }
                        
                        // Set loop variable
                        self.set_register(frame.base_register, a + 3, Value::Number(new_idx))?;
                    }
                } else {
                    return Err(LuaError::TypeError("'for' variables must be numbers".to_string()));
                }
            }
            
            OpCode::ForPrep => {
                // R(A)-=R(A+2); pc+=sBx
                let idx = self.get_register(frame.base_register, a)?;
                let step = self.get_register(frame.base_register, a + 2)?;
                
                if let (Value::Number(idx_n), Value::Number(step_n)) = (idx, step) {
                    self.set_register(frame.base_register, a, Value::Number(idx_n - step_n))?;
                    
                    // Jump
                    let sbx = instr.sbx();
                    let thread = self.heap.get_thread_mut(self.current_thread)?;
                    if let Some(frame) = thread.call_frames.last_mut() {
                        if sbx >= 0 {
                            frame.pc = frame.pc.saturating_add(sbx as usize);
                        } else {
                            frame.pc = frame.pc.saturating_sub((-sbx) as usize);
                        }
                    }
                } else {
                    return Err(LuaError::TypeError("'for' initial value must be a number".to_string()));
                }
            }
            
            OpCode::TForLoop => {
                // R(A+3), ... ,R(A+2+C) := R(A)(R(A+1), R(A+2)); if R(A+3) ~= nil then { R(A+2)=R(A+3); pc++ }
                // Generic for loop - not fully implemented for Redis compatibility
                return Err(LuaError::NotImplemented("generic for loops"));
            }
            
            OpCode::SetList => {
                // R(A)[(C-1)*FPF+i] := R(A+i), 1 <= i <= B
                const FPF: usize = 50; // Fields per flush
                
                let table_val = self.get_register(frame.base_register, a)?;
                if let Value::Table(table) = table_val {
                    let base_idx = if c > 0 {
                        (c - 1) * FPF + 1
                    } else {
                        // Next instruction contains actual C value
                        let thread = self.heap.get_thread_mut(self.current_thread)?;
                        if let Some(frame) = thread.call_frames.last_mut() {
                            frame.pc += 1;
                        }
                        1 // Simplified for now
                    };
                    
                    let count = if b > 0 { b } else {
                        // B = 0 means to top of stack
                        let thread = self.heap.get_thread(self.current_thread)?;
                        thread.stack.len() - frame.base_register as usize - a - 1
                    };
                    
                    // Set values
                    for i in 0..count {
                        let value = self.get_register(frame.base_register, a + 1 + i)?;
                        let key = Value::Number((base_idx + i) as f64);
                        self.table_set(table, key, value)?;
                    }
                } else {
                    return Err(LuaError::TypeError("attempt to index a non-table".to_string()));
                }
            }
            
            OpCode::Close => {
                // Close all variables in the stack up to (>=) R(A)
                // This is for closing upvalues - simplified for now
            }
            
            OpCode::Closure => {
                // R(A) := closure(KPROTO[Bx])
                let bx = instr.bx() as usize;
                let proto = self.get_proto(frame.closure, bx)?;
                
                // Create upvalues
                let mut upvalues = Vec::new();
                let _closure_obj = self.heap.get_closure(frame.closure)?;
                
                for _ in 0..proto.upvalue_count {
                    // Next instructions define upvalues
                    // For now, create empty upvalues
                    upvalues.push(UpvalueRef::Closed { value: Value::Nil });
                }
                
                let new_closure = self.heap.alloc_closure(proto, upvalues);
                self.set_register(frame.base_register, a, Value::Closure(new_closure))?;
            }
            
            OpCode::Vararg => {
                // R(A), R(A+1), ..., R(A+B-2) = vararg
                // Not implemented for Redis compatibility
                return Err(LuaError::NotImplemented("varargs"));
            }
        }
        
        Ok(ExecutionStatus::Continue)
    }
    
    /// Get a register value
    fn get_register(&self, base: u16, index: usize) -> Result<Value> {
        // Ensure the register index is valid (only the lower 9 bits should be considered for register indices)
        if index >= 256 {
            return Err(LuaError::InvalidOperation(format!("register {} out of bounds", index)));
        }
        
        let thread = self.heap.get_thread(self.current_thread)?;
        let idx = base as usize + index;
        
        thread.stack.get(idx)
            .copied()
            .ok_or(LuaError::InvalidOperation(format!("register {} out of bounds", index)))
    }
    
    /// Set a register value
    fn set_register(&mut self, base: u16, index: usize, value: Value) -> Result<()> {
        let thread = self.heap.get_thread_mut(self.current_thread)?;
        let idx = base as usize + index;
        
        // Ensure stack is large enough
        if idx >= thread.stack.len() {
            thread.stack.resize(idx + 1, Value::Nil);
        }
        
        thread.stack[idx] = value;
        Ok(())
    }
    
    /// Get RK value (register or constant)
    fn get_rk(&self, frame: &CallFrame, index: usize) -> Result<Value> {
        if index & 0x100 != 0 {
            // Constant - but we should check if the index is valid
            let const_idx = index & 0xFF;
            
            // Get the constant directly - don't go past the end of the constants table
            let closure_obj = self.heap.get_closure(frame.closure)?;
            
            if const_idx >= closure_obj.proto.constants.len() {
                return Err(LuaError::InvalidOperation(
                    format!("register {} out of bounds", index)
                ));
            }
            
            Ok(closure_obj.proto.constants[const_idx].clone())
        } else {
            // Register
            self.get_register(frame.base_register, index)
        }
    }
    
    /// Get a constant value
    fn get_constant(&self, closure: ClosureHandle, index: usize) -> Result<Value> {
        let closure_obj = self.heap.get_closure(closure)?;
        
        closure_obj.proto.constants.get(index)
            .copied()
            .ok_or(LuaError::InvalidConstant(index))
    }
    
    /// Get a function prototype
    fn get_proto(&self, closure: ClosureHandle, index: usize) -> Result<FunctionProto> {
        let closure_obj = self.heap.get_closure(closure)?;
        
        // In real implementation, protos would be stored separately
        // For now, return a clone of the current proto
        if index == 0 {
            Ok(closure_obj.proto.clone())
        } else {
            Err(LuaError::InvalidConstant(index))
        }
    }
    
    /// Get an upvalue
    fn get_upvalue(&self, closure: ClosureHandle, index: usize) -> Result<Value> {
        let closure_obj = self.heap.get_closure(closure)?;
        
        match closure_obj.upvalues.get(index) {
            Some(UpvalueRef::Open { register_idx }) => {
                let thread = self.heap.get_thread(self.current_thread)?;
                thread.stack.get(*register_idx as usize)
                    .copied()
                    .ok_or(LuaError::InvalidUpvalue(index))
            }
            Some(UpvalueRef::Closed { value }) => Ok(*value),
            None => Err(LuaError::InvalidUpvalue(index)),
        }
    }
    
    /// Set an upvalue
    fn set_upvalue(&mut self, closure: ClosureHandle, index: usize, value: Value) -> Result<()> {
        // Split the borrow by using a separate scope
        {
            let closure_obj = self.heap.get_closure_mut(closure)?;
            
            if let Some(upval) = closure_obj.upvalues.get_mut(index) {
                match upval {
                    UpvalueRef::Open { register_idx } => {
                        let reg_idx = *register_idx;
                        
                        // Set register directly
                        let thread = self.heap.get_thread_mut(self.current_thread)?;
                        let idx = reg_idx as usize;
                        if idx < thread.stack.len() {
                            thread.stack[idx] = value;
                            return Ok(());
                        } else {
                            return Err(LuaError::InvalidUpvalue(index));
                        }
                    }
                    UpvalueRef::Closed { value: ref mut upval_value } => {
                        // Set closed upvalue
                        *upval_value = value;
                        return Ok(());
                    }
                }
            }
        }
        
        Err(LuaError::InvalidUpvalue(index))
    }
    
    /// Table get operation
    pub fn table_get(&mut self, table: TableHandle, key: Value) -> Result<Value> {
        // First check direct lookup without metamethods
        if let Some(value) = {
            let table_obj = self.heap.get_table(table)?;
            table_obj.get(&key).copied()
        } {
            return Ok(value);
        }
        
        // Now check for metatable
        let metatable = {
            let table_obj = self.heap.get_table(table)?;
            table_obj.metatable
        };
        
        if let Some(mt) = metatable {
            // Get the __index metamethod
            if let Some(index_value) = self.get_metamethod(mt, "__index")? {
                match index_value {
                    Value::Table(t) => {
                        // Recursive lookup in the metatable's __index table
                        return self.table_get(t, key);
                    }
                    Value::Closure(func) => {
                        // Call metamethod function with (table, key)
                        return self.execute_function(func, &[Value::Table(table), key]);
                    }
                    Value::CFunction(func) => {
                        // Need to handle C function specially to avoid borrow checker issues
                        // Create a separate scope to organize borrows
                        let (thread_handle, stack_len) = {
                            let thread = self.heap.get_thread_mut(self.current_thread)?;
                            // Push table and key onto stack
                            thread.stack.push(Value::Table(table));
                            thread.stack.push(key);
                            (self.current_thread, thread.stack.len())
                        };
                        
                        // Create execution context without borrowing self directly
                        let mut ctx = ExecutionContext {
                            vm: self,
                            base: stack_len - 2,
                            arg_count: 2,
                        };
                        
                        // Call the function
                        let ret_count = func(&mut ctx)?;
                        
                        // Get return value
                        if ret_count > 0 {
                            let thread = self.heap.get_thread(thread_handle)?;
                            if thread.stack.len() >= ret_count as usize {
                                return Ok(thread.stack[thread.stack.len() - ret_count as usize]);
                            }
                        }
                        
                        // No return value or error
                        return Ok(Value::Nil);
                    }
                    _ => {}
                }
            }
        }
        
        Ok(Value::Nil)
    }
    
    /// Set a value in a table
    fn table_set(&mut self, table: TableHandle, key: Value, value: Value) -> Result<()> {
        let table_obj = self.heap.get_table_mut(table)?;
        table_obj.set(key, value);
        Ok(())
    }
    
    /// Get metamethod from a table
    fn get_metamethod(&mut self, table: TableHandle, method: &str) -> Result<Option<Value>> {
        let method_key = self.heap.create_string(method);
        
        // Get the table first
        let table_obj = self.heap.get_table(table)?;
        
        // Check if table has the metamethod
        Ok(table_obj.get(&Value::String(method_key)).copied())
    }
    
    /// Execute arithmetic operation
    fn execute_arithmetic<F>(&mut self, frame: &CallFrame, a: usize, b: usize, c: usize, op: F) -> Result<()>
    where
        F: Fn(f64, f64) -> f64,
    {
        let b_val = self.get_rk(frame, b)?;
        let c_val = self.get_rk(frame, c)?;
        
        match (b_val, c_val) {
            (Value::Number(x), Value::Number(y)) => {
                let result = op(x, y);
                self.set_register(frame.base_register, a, Value::Number(result))?;
            }
            _ => {
                // Try metamethods - simplified for now
                return Err(LuaError::TypeError("attempt to perform arithmetic on non-numbers".to_string()));
            }
        }
        
        Ok(())
    }
    

    
    /// Compare values for less than 
    pub fn value_lt(&self, a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => x < y,
            (Value::String(x), Value::String(y)) => {
                if let (Ok(x_str), Ok(y_str)) = (self.heap.get_string(*x), self.heap.get_string(*y)) {
                    x_str < y_str
                } else {
                    false
                }
            }
            _ => false,
        }
    }
    
    /// Compare values for less than or equal
    pub fn value_le(&self, a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => x <= y,
            (Value::String(x), Value::String(y)) => {
                if let (Ok(x_str), Ok(y_str)) = (self.heap.get_string(*x), self.heap.get_string(*y)) {
                    x_str <= y_str
                } else {
                    false
                }
            }
            _ => false,
        }
    }
    
    /// Get global environment
    pub fn globals(&self) -> TableHandle {
        self.globals
    }
    
    /// Get registry
    pub fn registry(&self) -> TableHandle {
        self.registry
    }
}