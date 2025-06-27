//! Lua VM implementation with generational arena architecture

use crate::lua_new::heap::{LuaHeap, ThreadObject, CallFrame, ThreadStatus};
use crate::lua_new::value::{Value, StringHandle, TableHandle, ClosureHandle, ThreadHandle, 
                             FunctionProto, UpvalueRef, Instruction, OpCode};
use crate::lua_new::error::{LuaError, Result};
use crate::lua_new::{VMConfig, LuaLimits};
use crate::lua_new::compilation::{CompilationScript, CompilationProto, CompilationValue};
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
        // Add directly to the thread stack - the Call opcode handler will
        // find the values based on the stack size before and after the call
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
    
    /// Check resource limits and kill flag
    pub fn check_limits(&mut self) -> Result<()> {
        // Check kill flag first
        if let Some(flag) = &self.kill_flag {
            if flag.load(Ordering::Relaxed) {
                return Err(LuaError::ScriptKilled);
            }
        }
        
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
    
    /// Load a compile script and create a closure
    pub fn load_compilation_script(&mut self, script: &CompilationScript) -> Result<ClosureHandle> {
        println!("[VM] Loading compiled script with {} strings, {} constants in main proto",
                 script.string_pool.len(), script.main_proto.constants.len());
        
        // First, intern all strings from the string pool
        let string_handles: Vec<StringHandle> = script.string_pool.iter()
            .map(|s| self.heap.create_string(s))
            .collect();
        
        // Now load the main prototype
        let proto = self.load_compilation_proto(&script.main_proto, &string_handles)?;
        
        // Create a closure with no upvalues
        let closure_handle = self.heap.alloc_closure(proto, Vec::new());
        
        println!("[VM] Compiled script loaded successfully");
        Ok(closure_handle)
    }
    
    /// Load a compiled prototype recursively
    fn load_compilation_proto(&mut self, proto: &CompilationProto, string_handles: &[StringHandle]) -> Result<FunctionProto> {
        // Convert constants
        let constants = proto.constants.iter()
            .map(|c| self.compilation_value_to_value(c, string_handles))
            .collect::<Result<Vec<Value>>>()?;
        
        // Load nested prototypes
        let mut nested_protos = Vec::new();
        for nested_proto in &proto.nested_protos {
            nested_protos.push(self.load_compilation_proto(nested_proto, string_handles)?);
        }
        
        // Create function prototype
        let function_proto = FunctionProto {
            code: proto.code.clone(),
            constants,
            param_count: proto.param_count,
            is_vararg: proto.is_vararg,
            max_stack_size: proto.max_stack_size,
            upvalue_count: proto.upvalue_count,
            source: None, // We don't have this info yet
            line_info: proto.line_info.clone(),
            nested_protos,
        };
        
        Ok(function_proto)
    }
    
    /// Convert a compilation value to a runtime value
    fn compilation_value_to_value(&mut self, value: &CompilationValue, string_handles: &[StringHandle]) -> Result<Value> {
        match value {
            CompilationValue::Nil => Ok(Value::Nil),
            CompilationValue::Boolean(b) => Ok(Value::Boolean(*b)),
            CompilationValue::Number(n) => Ok(Value::Number(*n)),
            CompilationValue::String(idx) => {
                if *idx >= string_handles.len() {
                    return Err(LuaError::InvalidConstant(*idx));
                }
                Ok(Value::String(string_handles[*idx]))
            },
            CompilationValue::FunctionPrototype(idx) => {
                // For now, we just return a placeholder number
                // In the future, we'll need to handle this properly
                Ok(Value::Number(*idx as f64))
            },
            CompilationValue::TableConstructor => {
                // For now, we just return a placeholder
                Ok(Value::Nil)
            },
        }
    }
    
    /// Execute a script directly from source
    pub fn execute_script(&mut self, source: &str) -> Result<Value> {
        println!("[VM] Executing script from source");
        
        // Create a compiler
        let mut compiler = crate::lua_new::compiler::Compiler::new();
        compiler.set_heap(&mut self.heap as *mut _);
        
        // Compile the script
        let compilation_script = compiler.compile(source)?;
        
        // Load the script
        let closure = self.load_compilation_script(&compilation_script)?;
        
        // Execute the script
        self.execute_function(closure, &[])
    }
    
    /// Execute a function
    pub fn execute_function(&mut self, closure: ClosureHandle, args: &[Value]) -> Result<Value> {
        // Record initial call stack depth to track nested calls
        let initial_depth = {
            let thread = self.heap.get_thread(self.current_thread)?;
            thread.call_frames.len()
        };
        
        // Push a new call frame
        self.push_call_frame(closure, args)?;
        
        // Main execution loop - keep executing until we return to the initial depth
        loop {
            // Check kill flag first - this allows script termination
            if let Some(flag) = &self.kill_flag {
                if flag.load(Ordering::Relaxed) {
                    println!("[LUA_VM] Script killed by kill flag");
                    return Err(LuaError::ScriptKilled);
                }
            }
            
            // Check resource limits
            if let Err(e) = self.check_limits() {
                println!("[LUA_VM] Resource limit exceeded: {}", e);
                return Err(e);
            }
            
            // Check if we've returned to the initial level or below
            let current_depth = {
                let thread = self.heap.get_thread(self.current_thread)?;
                if thread.call_frames.len() <= initial_depth {
                    break;
                }
                thread.call_frames.len()
            };
            
            // Execute a single step
            match self.step()? {
                ExecutionStatus::Continue => {
                    // Continue execution
                    continue;
                },
                ExecutionStatus::Return(value) => {
                    // Pop the current frame
                    self.pop_call_frame()?;
                    
                    // Check if we've returned to the initial level
                    let current_depth = {
                        let thread = self.heap.get_thread(self.current_thread)?;
                        thread.call_frames.len()
                    };
                    
                    if current_depth <= initial_depth {
                        // We're back to our starting point - return the value
                        return Ok(value);
                    }
                    
                    // For nested returns, we need to find the call instruction in the caller
                    // and store the return value in the appropriate register
                    
                    // 1. First collect information needed to store the return value
                    let register_info = {
                        // Capture the register info from the current top frame (that's the caller)
                        let thread = self.heap.get_thread(self.current_thread)?;
                        if let Some(frame) = thread.call_frames.last() {
                            let pc = frame.pc.saturating_sub(1); // Previous instruction (CALL)
                            let frame_closure = frame.closure;
                            let base_register = frame.base_register;
                            
                            // Fetch the CALL instruction from the closure
                            let closure_obj = self.heap.get_closure(frame_closure)?;
                            if let Some(instr) = closure_obj.proto.code.get(pc) {
                                if instr.opcode() == 28 { // CALL
                                    // The destination register is register A
                                    Some((base_register, instr.a() as usize))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };
                    
                    // 2. Then set the register with the return value if needed
                    if let Some((base_reg, a_reg)) = register_info {
                        // This operation borrows self mutably, so we do it after the previous scope ends
                        self.set_register(base_reg, a_reg, value)?;
                    }
                    
                    // Continue with caller's execution
                    continue;
                },
                ExecutionStatus::Yield(_) => {
                    return Err(LuaError::NotImplemented("coroutines"));
                },
            }
        }
        
        // At this point, we've returned to the initial depth
        // Extract the result value from register 0 of the current frame
        let result = {
            let thread = self.heap.get_thread(self.current_thread)?;
            if thread.call_frames.is_empty() {
                // No frame? Should never happen if we started with a frame
                Value::Nil
            } else if let Some(frame) = thread.call_frames.last() {
                // Get value from register 0 or first available register
                if (frame.base_register as usize) < thread.stack.len() {
                    thread.stack[frame.base_register as usize]
                } else {
                    Value::Nil
                }
            } else {
                // No frame? That's odd, but return Nil
                Value::Nil
            }
        };
        
        Ok(result)
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
                
                // Extra validation for table type
                if !matches!(table_val, Value::Table(_)) {
                    return Err(LuaError::TypeError(format!(
                        "attempt to index a {} value (not a table)", 
                        table_val.type_name()
                    )));
                }
                
                let key = self.get_rk(&frame, c)?;
                
                // Additional validation for nil key
                if matches!(key, Value::Nil) {
                    return Err(LuaError::TypeError("table index is nil".to_string()));
                }
                
                if let Value::Table(table) = table_val {
                    // Extra validation for table handle
                    if !self.heap.is_valid_table(table) {
                        println!("[VM_ERROR] Invalid table handle: {:?}", table);
                        return Err(LuaError::InvalidHandle);
                    }
                    
                    // Extra debug information
                    println!("[VM_DEBUG] GetTable: A={}, B={}, C={}, table={:?}, key={:?}", 
                             a, b, c, table, key);
                    
                    match self.table_get(table, key) {
                        Ok(value) => {
                            self.set_register(frame.base_register, a, value)?;
                        }
                        Err(e) => {
                            println!("[VM_ERROR] Table get failed: {}", e);
                            return Err(e);
                        }
                    }
                } else {
                    // Should never happen due to the type check above
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
                
                // Extra validation for table type
                if !matches!(table_val, Value::Table(_)) {
                    return Err(LuaError::TypeError(format!(
                        "attempt to index a {} value (not a table)", 
                        table_val.type_name()
                    )));
                }
                
                let key = self.get_rk(&frame, b)?;
                
                // Additional validation for nil key
                if matches!(key, Value::Nil) {
                    return Err(LuaError::TypeError("table index is nil".to_string()));
                }
                
                let value = self.get_rk(&frame, c)?;
                
                if let Value::Table(table) = table_val {
                    // Extra validation for table handle
                    if !self.heap.is_valid_table(table) {
                        println!("[VM_ERROR] Invalid table handle: {:?}", table);
                        return Err(LuaError::InvalidHandle);
                    }
                    
                    // Extra debug information
                    println!("[VM_DEBUG] SetTable: A={}, B={}, C={}, table={:?}, key={:?}, value={:?}", 
                             a, b, c, table, key, value);
                    
                    match self.table_set(table, key, value) {
                        Ok(()) => {},
                        Err(e) => {
                            println!("[VM_ERROR] Table set failed: {}", e);
                            return Err(e);
                        }
                    }
                } else {
                    // Should never happen due to the type check above
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
                println!("[VM_DEBUG] Executing CONCAT operation: A={}, B={}, C={}", a, b, c);
                // R(A) := R(B).. ... ..R(C)
                // Important: This concatenates a RANGE of values from B to C inclusive
                
                // First collect all values and convert them to strings to avoid double borrowing
                let mut values_to_concat = Vec::new();
                
                // For each value in the range from B to C inclusive
                for i in b..=c {
                    // Use get_register directly (not get_rk) for register access to handle table fields properly
                    let reg_value = self.get_register(frame.base_register, i)?;
                    
                    println!("[VM_DEBUG] Concat value at register {}: {:?}", i, reg_value);
                    
                    // Convert each value to a string representation
                    let str_value = match reg_value {
                        Value::String(s) => {
                            let bytes = self.heap.get_string(s)?;
                            std::str::from_utf8(bytes)
                                .map_err(|_| LuaError::InvalidEncoding)?
                                .to_string()
                        }
                        Value::Number(n) => {
                            // Format number appropriately - Lua automatically converts numbers to strings
                            n.to_string()
                        },
                        Value::Table(t) => {
                            // For table values, try metamethod or error
                            if let Some(meta_fn) = self.get_table_metamethod(t, "__tostring")? {
                                // Call metamethod with the table as argument
                                let result = self.execute_function(meta_fn, &[Value::Table(t)])?;
                                match result {
                                    Value::String(s) => {
                                        let bytes = self.heap.get_string(s)?;
                                        std::str::from_utf8(bytes)
                                            .map_err(|_| LuaError::InvalidEncoding)?
                                            .to_string()
                                    }
                                    _ => return Err(LuaError::TypeError(format!(
                                        "__tostring metamethod did not return a string (got {})",
                                        result.type_name()
                                    ))),
                                }
                            } else {
                                return Err(LuaError::TypeError("attempt to concatenate a table value".to_string()));
                            }
                        },
                        Value::Nil => {
                            return Err(LuaError::TypeError("attempt to concatenate a nil value".to_string()));
                        },
                        Value::Boolean(_) => {
                            return Err(LuaError::TypeError("attempt to concatenate a boolean value".to_string()));
                        },
                        Value::Closure(_) | Value::CFunction(_) => {
                            return Err(LuaError::TypeError("attempt to concatenate a function value".to_string()));
                        },
                        Value::Thread(_) => {
                            return Err(LuaError::TypeError("attempt to concatenate a thread value".to_string()));
                        },
                    };
                    
                    values_to_concat.push(str_value);
                }
                
                // Now concatenate all the strings
                let mut result = String::new();
                for value in values_to_concat {
                    result.push_str(&value);
                }
                
                println!("[VM_DEBUG] Concatenation result: {}", result);
                
                // Create the final string and store in the result register
                let str_handle = self.heap.create_string(&result);
                self.set_register(frame.base_register, a, Value::String(str_handle))?;
            },
            
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
                        
                        // Continue execution - DO NOT call execute_function directly which would cause recursion
                        return Ok(ExecutionStatus::Continue);
                    }
                    Value::CFunction(cfunc) => {
                        // Record stack size before call to know where results will be pushed
                        let stack_size_before = {
                            let thread = self.heap.get_thread(self.current_thread)?;
                            thread.stack.len()
                        };
                        
                        // Create execution context
                        let mut ctx = ExecutionContext {
                            vm: self,
                            base: frame.base_register as usize + a + 1,
                            arg_count: args.len(),
                        };
                        
                        // Call function and get return count
                        let ret_count = cfunc(&mut ctx)?;
                        
                        // The return values have been pushed to the end of the stack
                        // We need to move them to their expected locations
                        let thread = self.heap.get_thread_mut(self.current_thread)?;
                        let base = frame.base_register as usize;
                        
                        // Copy values from where they were pushed to where they're expected
                        for i in 0..ret_count as usize {
                            // Make sure we don't go out of bounds
                            if stack_size_before + i < thread.stack.len() && base + a + i < thread.stack.len() {
                                thread.stack[base + a + i] = thread.stack[stack_size_before + i];
                            }
                        }
                        
                        // Truncate the stack to remove extra copies of the return values
                        thread.stack.truncate(base + a + ret_count as usize);
                        
                        // Continue execution
                        return Ok(ExecutionStatus::Continue);
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
                
                // For simplicity, just return the first value for now
                let value = if ret_count > 0 {
                    self.get_register(frame.base_register, a)?
                } else {
                    Value::Nil
                };
                
                // Return with the value - this will be handled by execute_function
                return Ok(ExecutionStatus::Return(value));
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
    
    /// Get register or constant (RK format) with enhanced error handling
    fn get_rk(&self, frame: &CallFrame, index: usize) -> Result<Value> {
        println!("[VM_DEBUG] get_rk called with index: {}", index);
        
        if index & 0x100 != 0 {
            // This is an RK format (constant with high bit set)
            // Extract the constant index (remove the high bit)
            let const_idx = index & 0xFF;
            
            // Get the closure to access its constants table
            let closure_obj = match self.heap.get_closure(frame.closure) {
                Ok(c) => c,
                Err(e) => {
                    println!("[VM_ERROR] Failed to get closure for RK format: {}", e);
                    return Err(e);
                }
            };
            
            // Safely check if the constant index is valid
            if const_idx >= closure_obj.proto.constants.len() {
                println!("[VM_ERROR] Constant index {} out of bounds (max {})", 
                        const_idx, 
                        if closure_obj.proto.constants.is_empty() { 0 } 
                        else { closure_obj.proto.constants.len() - 1 });
                return Err(LuaError::InvalidConstant(const_idx));
            }
            
            println!("[VM_DEBUG] RK format: using constant at index {}", const_idx);
            // Return a copy of the constant
            Ok(closure_obj.proto.constants[const_idx].clone())
        } else {
            // Regular register
            println!("[VM_DEBUG] RK format: using register at index {}", index);
            self.get_register(frame.base_register, index)
        }
    }
    
    /// Get a constant value with proper bounds checking and error handling
    fn get_constant(&self, closure: ClosureHandle, index: usize) -> Result<Value> {
        let closure_obj = self.heap.get_closure(closure)?;
        
        // Improved constant index validation with better error reporting
        if closure_obj.proto.constants.is_empty() {
            println!("[VM_ERROR] No constants available in closure (requested index: {})", index);
            return Err(LuaError::InvalidOperation(format!(
                "Attempt to access constant at index {} but constants array is empty", index
            )));
        }

        if index >= closure_obj.proto.constants.len() {
            println!("[VM_ERROR] Constant index {} out of bounds (max: {})", 
                    index, 
                    closure_obj.proto.constants.len() - 1);
                    
            return Err(LuaError::InvalidConstant(index));
        }
        
        // Make sure we return a owned copy of the constant
        let constant = closure_obj.proto.constants[index].clone();
        
        println!("[VM_DEBUG] Retrieved constant at index {}: {:?}", index, constant);
        
        // Success, return the constant
        Ok(constant)
    }
    
    /// Get a function prototype for CLOSURE instruction with support for nested prototypes
    fn get_proto(&mut self, closure: ClosureHandle, index: usize) -> Result<FunctionProto> {
        println!("[VM_DEBUG] get_proto called with index: {}", index);
        
        // Get the closure to access its prototype
        let closure_obj = self.heap.get_closure(closure)?;
        
        // Check if the requested prototype is the closure's own prototype (index 0)
        if index == 0 {
            // Return a clone of the closure's own prototype
            return Ok(closure_obj.proto.clone());
        }
        
        // Otherwise, check if the index is valid for a nested prototype
        if index > 0 && index <= closure_obj.proto.nested_protos.len() {
            // Rust indexes are 0-based, but Lua function prototypes use 1-based indexing for nested protos
            let nested_index = index - 1;
            println!("[VM_DEBUG] Accessing nested prototype at index {}", nested_index);
            return Ok(closure_obj.proto.nested_protos[nested_index].clone());
        }
        
        // If we reach here, the index is invalid
        println!("[VM_ERROR] Invalid prototype index: {} (max: {})", 
                index, closure_obj.proto.nested_protos.len());
        
        // Return a clear error message with diagnostic information
        Err(LuaError::InvalidConstant(index))
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
    
    /// Table get operation with enhanced handle validation
    pub fn table_get(&mut self, table: TableHandle, key: Value) -> Result<Value> {
        // Add explicit handle validation first
        if !self.heap.is_valid_table(table) {
            println!("[LUA_ERROR] Invalid table handle in table_get: {:?}", table);
            return Err(LuaError::InvalidHandle);
        }
        
        // Direct table lookup - most common case
        let table_lookup_result = {
            let table_obj = self.heap.get_table(table)?;
            table_obj.get(&key).copied()  // Copy value to avoid borrowing issues
        };
        
        if let Some(value) = table_lookup_result {
            return Ok(value);
        }
        
        // Check if key access would be valid
        if matches!(key, Value::Nil) {
            println!("[LUA_ERROR] Invalid nil key in table_get");
            return Err(LuaError::TypeError("table index is nil".to_string()));
        }
        
        // Metatable lookup - use the __index metamethod if available
        let metatable_opt = {
            let table_obj = match self.heap.get_table(table) {
                Ok(t) => t,
                Err(e) => {
                    println!("[LUA_ERROR] Failed to get table for metatable lookup: {}", e);
                    return Err(e);
                }
            };
            table_obj.metatable  // Get metatable handle (if any)
        };
        
        if let Some(metatable) = metatable_opt {
            // Validate metatable handle
            if !self.heap.is_valid_table(metatable) {
                println!("[LUA_ERROR] Invalid metatable handle: {:?}", metatable);
                return Err(LuaError::InvalidHandle);
            }
            
            let metamethod_key = self.heap.create_string("__index");
            
            // Look up __index in metatable
            let index_opt = {
                let metatable_obj = match self.heap.get_table(metatable) {
                    Ok(t) => t,
                    Err(e) => {
                        println!("[LUA_ERROR] Failed to get metatable: {}", e);
                        return Err(e);
                    }
                };
                metatable_obj.get(&Value::String(metamethod_key)).copied()
            };
            
            // Process based on metamethod type
            if let Some(index_value) = index_opt {
                match index_value {
                    Value::Table(index_table) => {
                        // Recursive lookup in the __index table
                        println!("[LUA_DEBUG] using table __index: {:?}", index_table);
                        self.table_get(index_table, key)
                    },
                    Value::Closure(func) => {
                        // Call the __index function with (table, key)
                        println!("[LUA_DEBUG] calling closure __index: {:?}", func);
                        let args = vec![Value::Table(table), key]; 
                        self.execute_function(func, &args)
                    },
                    Value::CFunction(func) => {
                        // Call C function for __index
                        println!("[LUA_DEBUG] calling C function __index");
                        // First prepare the stack
                        let thread = self.heap.get_thread_mut(self.current_thread)?;
                        thread.stack.push(Value::Table(table));
                        thread.stack.push(key);
                        
                        // Create execution context
                        let stack_size = thread.stack.len();
                        let mut ctx = ExecutionContext {
                            vm: self,
                            base: stack_size - 2,
                            arg_count: 2,
                        };
                        
                        // Call the function
                        let ret_count = func(&mut ctx)?;
                        
                        // Return the result (or nil if no results)
                        if ret_count > 0 {
                            let thread = self.heap.get_thread(self.current_thread)?;
                            // Return the top value from stack
                            Ok(thread.stack[thread.stack.len() - 1])
                        } else {
                            Ok(Value::Nil)
                        }
                    },
                    _ => {
                        println!("[LUA_DEBUG] __index is not a table or function: {:?}", index_value);
                        Ok(Value::Nil)  // __index is not a table or function
                    }
                }
            } else {
                println!("[LUA_DEBUG] No __index metamethod found");
                Ok(Value::Nil)  // No __index metamethod
            }
        } else {
            println!("[LUA_DEBUG] No metatable found");
            Ok(Value::Nil)  // No metatable
        }
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
    
    /// Get metamethod from a table
    fn get_table_metamethod(&mut self, table: TableHandle, method: &str) -> Result<Option<ClosureHandle>> {
        println!("[VM_DEBUG] Looking up metamethod '{}' for table {:?}", method, table);
        
        // Create method key first, before any table borrows
        let method_key = self.heap.create_string(method);
        let method_key_val = Value::String(method_key);
        
        // Get table object
        let table_obj = self.heap.get_table(table)?;
        
        // Check if table has metatable
        if let Some(metatable) = table_obj.metatable {
            let metatable_obj = self.heap.get_table(metatable)?;
            
            // Look up metamethod using the pre-created key
            if let Some(method_val) = metatable_obj.get(&method_key_val) {
                println!("[VM_DEBUG] Found metamethod '{}': {:?}", method, method_val);
                
                // Convert to closure if possible
                match *method_val {
                    Value::Closure(closure) => {
                        return Ok(Some(closure));
                    },
                    _ => {
                        println!("[VM_DEBUG] Metamethod is not a closure: {:?}", method_val);
                    }
                }
            }
        }
        
        println!("[VM_DEBUG] No metamethod '{}' found for table {:?}", method, table);
        Ok(None)
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
    
    /// Reset VM after execution to clean state for reuse
    pub fn reset(&mut self) {
        // Reset instruction count
        self.instruction_count = 0;
        
        // Reset call frames and stack in main thread
        if let Ok(thread) = self.heap.get_thread_mut(self.current_thread) {
            // Clear call frames
            thread.call_frames.clear();
            
            // Clear value stack
            thread.stack.clear();
            
            // Reset thread status
            thread.status = crate::lua_new::heap::ThreadStatus::Running;
        }
        
        // Clear kill flag
        self.kill_flag = None;
    }
    
    /// Reset the VM to a clean state
    pub fn full_reset(&mut self) {
        println!("[LUA_VM] Performing full VM reset");
        
        // Reset kill flag
        self.kill_flag = None;
        
        // Reset instruction count
        self.instruction_count = 0;
        
        // Create a completely fresh heap
        // This is the key to resolving handle validity issues
        self.heap = LuaHeap::new();
        
        // Create a new main thread
        self.current_thread = self.heap.alloc_thread();
        
        // Create fresh global environment and registry
        self.globals = self.heap.alloc_table();
        self.registry = self.heap.alloc_table();
        
        println!("[LUA_VM] Full reset complete, created fresh heap and thread");
    }
}

