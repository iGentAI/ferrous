# RefCellVM Implementation Guide

## Introduction

This document provides detailed implementation guidance for the RefCellVM, a Lua virtual machine that uses Rust's interior mutability pattern (RefCell) instead of transactions for memory safety. The implementation focuses on simplicity, correctness, and maintainability while providing the same functionality as the transaction-based VM.

## Core Components Implementation

### 1. RefCellHeap

The RefCellHeap is the central component of the RefCellVM, responsible for safely managing all Lua objects. Here's how to implement its key methods:

#### 1.1 Basic Access Methods

```rust
impl RefCellHeap {
    /// Create a new heap
    pub fn new() -> LuaResult<Self> {
        // Initialize all arenas
        let mut heap = RefCellHeap {
            strings: RefCell::new(Arena::new()),
            string_cache: RefCell::new(HashMap::new()),
            tables: RefCell::new(Arena::new()),
            closures: RefCell::new(Arena::new()),
            threads: RefCell::new(Arena::new()),
            upvalues: RefCell::new(Arena::new()),
            userdata: RefCell::new(Arena::new()),
            function_protos: RefCell::new(Arena::new()),
            globals: None,
            registry: None,
            main_thread: None,
            resource_limits: ResourceLimits::default(),
        };
        
        // Pre-intern common strings
        heap.pre_intern_common_strings()?;
        
        // Create globals table
        let globals_handle = heap.create_table()?;
        heap.globals = Some(globals_handle);
        
        // Create registry table
        let registry_handle = heap.create_table()?;
        heap.registry = Some(registry_handle);
        
        // Create main thread
        let main_thread_handle = heap.create_thread()?;
        heap.main_thread = Some(main_thread_handle);
        
        Ok(heap)
    }
    
    /// Pre-intern common strings used by the VM
    fn pre_intern_common_strings(&self) -> LuaResult<()> {
        const COMMON_STRINGS: &[&str] = &[
            // Standard library functions
            "print", "type", "tostring", "tonumber", 
            "next", "pairs", "ipairs", 
            "getmetatable", "setmetatable",
            "rawget", "rawset", "rawequal",
            
            // Metamethods
            "__index", "__newindex", "__call", "__tostring",
            "__add", "__sub", "__mul", "__div", "__mod", "__pow",
            "__concat", "__len", "__eq", "__lt", "__le",
            
            // Common keys
            "_G", "self", "value",
        ];
        
        for s in COMMON_STRINGS {
            self.create_string(s)?;
        }
        
        Ok(())
    }
    
    /// Get the main thread
    pub fn main_thread(&self) -> LuaResult<ThreadHandle> {
        self.main_thread.ok_or(LuaError::InternalError(
            "Main thread not initialized".to_string()
        ))
    }
    
    /// Get the globals table
    pub fn globals(&self) -> LuaResult<TableHandle> {
        self.globals.ok_or(LuaError::InternalError(
            "Globals table not initialized".to_string()
        ))
    }
    
    /// Get the registry table
    pub fn registry(&self) -> LuaResult<TableHandle> {
        self.registry.ok_or(LuaError::InternalError(
            "Registry table not initialized".to_string()
        ))
    }
}
```

#### 1.2 String Operations

```rust
impl RefCellHeap {
    /// Create a string with interning
    pub fn create_string(&self, s: &str) -> LuaResult<StringHandle> {
        // First check the cache with a read lock
        {
            let cache = self.string_cache.try_borrow()
                .map_err(|_| LuaError::BorrowError("Failed to borrow string cache for reading".to_string()))?;
            
            if let Some(&handle) = cache.get(s.as_bytes()) {
                // Verify handle is still valid
                let strings = self.strings.try_borrow()
                    .map_err(|_| LuaError::BorrowError("Failed to borrow strings arena".to_string()))?;
                
                if strings.contains(handle.0) {
                    return Ok(handle);
                }
                // Stale cache entry will be cleaned up below
            }
        }
        
        // Need to create new string - get write access
        let mut strings = self.strings.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow strings arena for writing".to_string()))?;
        let mut cache = self.string_cache.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow string cache for writing".to_string()))?;
        
        // Clean up stale cache entry if it exists
        cache.remove(s.as_bytes());
        
        // Create new string
        let lua_string = LuaString::new(s);
        let handle = StringHandle::from(strings.insert(lua_string));
        
        // Add to cache
        cache.insert(s.as_bytes().to_vec(), handle);
        
        Ok(handle)
    }
    
    /// Get a string by handle
    pub fn get_string(&self, handle: StringHandle) -> LuaResult<Ref<'_, LuaString>> {
        let strings = self.strings.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow strings arena".to_string()))?;
        
        // Check if handle is valid
        if !strings.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        
        Ok(Ref::map(strings, |s| s.get(handle.0).unwrap()))
    }
    
    /// Get string value as a String
    pub fn get_string_value(&self, handle: StringHandle) -> LuaResult<String> {
        let strings = self.strings.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow strings arena".to_string()))?;
        
        let lua_string = strings.get(handle.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        match lua_string.to_str() {
            Ok(s) => Ok(s.to_string()),
            Err(_) => Err(LuaError::RuntimeError("Invalid UTF-8 in string".to_string())),
        }
    }
    
    /// Get string bytes
    pub fn get_string_bytes(&self, handle: StringHandle) -> LuaResult<Vec<u8>> {
        let strings = self.strings.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow strings arena".to_string()))?;
        
        let lua_string = strings.get(handle.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        Ok(lua_string.bytes.clone())
    }
}
```

#### 1.3 Table Operations

```rust
impl RefCellHeap {
    /// Create a table
    pub fn create_table(&self) -> LuaResult<TableHandle> {
        let mut tables = self.tables.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow tables arena".to_string()))?;
        
        let table = Table::new();
        let handle = TableHandle::from(tables.insert(table));
        Ok(handle)
    }
    
    /// Get a table by handle
    pub fn get_table(&self, handle: TableHandle) -> LuaResult<Ref<'_, Table>> {
        let tables = self.tables.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow tables arena".to_string()))?;
        
        if !tables.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        
        Ok(Ref::map(tables, |t| t.get(handle.0).unwrap()))
    }
    
    /// Get a mutable table by handle
    pub fn get_table_mut(&self, handle: TableHandle) -> LuaResult<RefMut<'_, Table>> {
        let tables = self.tables.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow tables arena for writing".to_string()))?;
        
        if !tables.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        
        Ok(RefMut::map(tables, |t| t.get_mut(handle.0).unwrap()))
    }
    
    /// Get table field
    pub fn get_table_field(&self, table: TableHandle, key: &Value) -> LuaResult<Value> {
        // Try array optimization for integer keys
        if let Value::Number(n) = key {
            if n.fract() == 0.0 && *n > 0.0 {
                let tables = self.tables.try_borrow()
                    .map_err(|_| LuaError::BorrowError("Failed to borrow tables arena".to_string()))?;
                
                let table_obj = tables.get(table.0)
                    .ok_or(LuaError::InvalidHandle)?;
                
                let idx = *n as usize;
                if idx <= table_obj.array.len() {
                    return Ok(table_obj.array[idx - 1].clone());
                }
            }
        }
        
        // For string keys, we need to get the content hash
        if let Value::String(string_handle) = key {
            // Create hashable key with string content hash
            let content_hash = {
                let strings = self.strings.try_borrow()
                    .map_err(|_| LuaError::BorrowError("Failed to borrow strings arena".to_string()))?;
                
                let string = strings.get(string_handle.0)
                    .ok_or(LuaError::InvalidHandle)?;
                
                string.content_hash
            };
            
            let hashable = HashableValue::String(*string_handle, content_hash);
            
            // Get value from table
            let tables = self.tables.try_borrow()
                .map_err(|_| LuaError::BorrowError("Failed to borrow tables arena".to_string()))?;
            
            let table_obj = tables.get(table.0)
                .ok_or(LuaError::InvalidHandle)?;
            
            return Ok(table_obj.get_by_hashable(&hashable).cloned().unwrap_or(Value::Nil));
        }
        
        // For other hashable keys
        let tables = self.tables.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow tables arena".to_string()))?;
        
        let table_obj = tables.get(table.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        match HashableValue::from_value_with_context(key, "get_table_field") {
            Ok(hashable) => {
                Ok(table_obj.get_by_hashable(&hashable).cloned().unwrap_or(Value::Nil))
            },
            Err(_) => {
                // Key is not hashable (e.g., a table or function)
                Ok(Value::Nil)
            }
        }
    }
    
    /// Set table field
    pub fn set_table_field(&self, table: TableHandle, key: &Value, value: &Value) -> LuaResult<()> {
        // Try array optimization for integer keys
        if let Value::Number(n) = key {
            if n.fract() == 0.0 && *n > 0.0 {
                let idx = *n as usize;
                
                let mut tables = self.tables.try_borrow_mut()
                    .map_err(|_| LuaError::BorrowError("Failed to borrow tables arena for writing".to_string()))?;
                
                let table_obj = tables.get_mut(table.0)
                    .ok_or(LuaError::InvalidHandle)?;
                
                // Expand array if needed
                if idx <= table_obj.array.len() + 1 {
                    if idx == table_obj.array.len() + 1 {
                        table_obj.array.push(value.clone());
                    } else {
                        table_obj.array[idx - 1] = value.clone();
                    }
                    return Ok(());
                }
            }
        }
        
        // For string keys, we need to set up a hashable value with content hash
        if let Value::String(string_handle) = key {
            let content_hash = {
                let strings = self.strings.try_borrow()
                    .map_err(|_| LuaError::BorrowError("Failed to borrow strings arena".to_string()))?;
                
                let string = strings.get(string_handle.0)
                    .ok_or(LuaError::InvalidHandle)?;
                
                string.content_hash
            };
            
            let hashable = HashableValue::String(*string_handle, content_hash);
            
            let mut tables = self.tables.try_borrow_mut()
                .map_err(|_| LuaError::BorrowError("Failed to borrow tables arena for writing".to_string()))?;
            
            let table_obj = tables.get_mut(table.0)
                .ok_or(LuaError::InvalidHandle)?;
            
            table_obj.set_by_hashable(hashable, value.clone());
            return Ok(());
        }
        
        // For other hashable keys
        match HashableValue::from_value_with_context(key, "set_table_field") {
            Ok(hashable) => {
                let mut tables = self.tables.try_borrow_mut()
                    .map_err(|_| LuaError::BorrowError("Failed to borrow tables arena for writing".to_string()))?;
                
                let table_obj = tables.get_mut(table.0)
                    .ok_or(LuaError::InvalidHandle)?;
                
                table_obj.set_by_hashable(hashable, value.clone());
                Ok(())
            },
            Err(_) => {
                // Key is not hashable - silently ignore (matches Lua behavior)
                Ok(())
            }
        }
    }
}
```

#### 1.4 Thread Operations

```rust
impl RefCellHeap {    
    /// Get thread register
    pub fn get_thread_register(&self, thread: ThreadHandle, index: usize) -> LuaResult<Value> {
        let threads = self.threads.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena".to_string()))?;
        
        let thread_obj = threads.get(thread.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        thread_obj.stack.get(index)
            .cloned()
            .ok_or_else(|| LuaError::RuntimeError(format!(
                "Register {} out of bounds (stack size: {})",
                index,
                thread_obj.stack.len()
            )))
    }
    
    ///  Set thread register
    pub fn set_thread_register(&self, thread: ThreadHandle, index: usize, value: &Value) -> LuaResult<()> {
        let mut threads = self.threads.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena for writing".to_string()))?;
        
        let thread_obj = threads.get_mut(thread.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        // Grow stack if needed
        if index >= thread_obj.stack.len() {
            thread_obj.stack.resize(index + 1, Value::Nil);
        }
        
        thread_obj.stack[index] = value.clone();
        Ok(())
    }
    
    /// Get stack size
    pub fn get_stack_size(&self, thread: ThreadHandle) -> LuaResult<usize> {
        let threads = self.threads.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena".to_string()))?;
        
        let thread_obj = threads.get(thread.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        Ok(thread_obj.stack.len())
    }
    
    /// Grow stack to at least the specified size
    pub fn grow_stack(&self, thread: ThreadHandle, min_size: usize) -> LuaResult<()> {
        let mut threads = self.threads.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena for writing".to_string()))?;
        
        let thread_obj = threads.get_mut(thread.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        if thread_obj.stack.len() < min_size {
            thread_obj.stack.resize(min_size, Value::Nil);
        }
        
        Ok(())
    }
}
```

#### 1.5 Upvalue and Closures

```rust
impl RefCellHeap {
    /// Find or create an upvalue for a stack position
    pub fn find_or_create_upvalue(&self, thread: ThreadHandle, stack_index: usize) -> LuaResult<UpvalueHandle> {
        // Step 1: Check if an upvalue for this position already exists
        let open_upvalues = {
            let threads = self.threads.try_borrow()
                .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena".to_string()))?;
            
            let thread_obj = threads.get(thread.0)
                .ok_or(LuaError::InvalidHandle)?;
            
            // First verify the stack index is valid
            if stack_index >= thread_obj.stack.len() {
                return Err(LuaError::RuntimeError(format!(
                    "Cannot create upvalue for stack index {} (stack size: {})",
                    stack_index, thread_obj.stack.len()
                )));
            }
            
            thread_obj.open_upvalues.clone()
        };
        
        // Step 2: Check if an upvalue for this index already exists
        for &upvalue_handle in &open_upvalues {
            let upvalues = self.upvalues.try_borrow()
                .map_err(|_| LuaError::BorrowError("Failed to borrow upvalues arena".to_string()))?;
            
            let upvalue = upvalues.get(upvalue_handle.0)
                .ok_or(LuaError::InvalidHandle)?;
            
            if let Some(idx) = upvalue.stack_index {
                if idx == stack_index {
                    return Ok(upvalue_handle);
                }
            }
        }
        
        // Step 3: Create a new upvalue
        let upvalue = Upvalue {
            stack_index: Some(stack_index),
            value: None,
        };
        
        let upvalue_handle = {
            let mut upvalues = self.upvalues.try_borrow_mut()
                .map_err(|_| LuaError::BorrowError("Failed to borrow upvalues arena for writing".to_string()))?;
            
            UpvalueHandle::from(upvalues.insert(upvalue))
        };
        
        // Step 4: Add to thread's open upvalue list
        let mut threads = self.threads.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena for writing".to_string()))?;
        
        let thread_obj = threads.get_mut(thread.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        // Insert in correct position (sorted by stack index, highest first)
        let pos = thread_obj.open_upvalues.iter()
            .position(|&handle| {
                if let Ok(upvalues) = self.upvalues.try_borrow() {
                    if let Some(upval) = upvalues.get(handle.0) {
                        if let Some(idx) = upval.stack_index {
                            return idx < stack_index;
                        }
                    }
                }
                false
            })
            .unwrap_or(thread_obj.open_upvalues.len());
        
        thread_obj.open_upvalues.insert(pos, upvalue_handle);
        
        Ok(upvalue_handle)
    }
    
    /// Close upvalues for thread above a threshold
    pub fn close_thread_upvalues(&self, thread: ThreadHandle, threshold: usize) -> LuaResult<()> {
        // Step 1: Identify upvalues to close and keep
        let upvalues_to_close_and_values: Vec<(UpvalueHandle, Value)> = {
            let threads = self.threads.try_borrow()
                .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena".to_string()))?;
            
            let thread_obj = threads.get(thread.0)
                .ok_or(LuaError::InvalidHandle)?;
            
            let mut to_close = Vec::new();
            
            for &upvalue_handle in &thread_obj.open_upvalues {
                let upvalues = self.upvalues.try_borrow()
                    .map_err(|_| LuaError::BorrowError("Failed to borrow upvalues arena".to_string()))?;
                
                let upvalue = upvalues.get(upvalue_handle.0)
                    .ok_or(LuaError::InvalidHandle)?;
                
                if let Some(idx) = upvalue.stack_index {
                    if idx >= threshold {
                        // Get the value from stack
                        let value = if idx < thread_obj.stack.len() {
                            thread_obj.stack[idx].clone()
                        } else {
                            // If the stack position is invalid, use nil
                            Value::Nil
                        };
                        
                        to_close.push((upvalue_handle, value));
                    }
                }
            }
            
            to_close
        };
        
        // Step 2: Close the identified upvalues
        for (upvalue_handle, value) in upvalues_to_close_and_values {
            let mut upvalues = self.upvalues.try_borrow_mut()
                .map_err(|_| LuaError::BorrowError("Failed to borrow upvalues arena for writing".to_string()))?;
            
            let upvalue = upvalues.get_mut(upvalue_handle.0)
                .ok_or(LuaError::InvalidHandle)?;
            
            upvalue.stack_index = None;
            upvalue.value = Some(value);
        }
        
        // Step 3: Update the thread's open_upvalues list
        let mut threads = self.threads.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena for writing".to_string()))?;
        
        let thread_obj = threads.get_mut(thread.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        // Keep only upvalues with stack_index < threshold
        thread_obj.open_upvalues.retain(|&handle| {
            if let Ok(upvalues) = self.upvalues.try_borrow() {
                if let Some(upvalue) = upvalues.get(handle.0) {
                    if let Some(idx) = upvalue.stack_index {
                        return idx < threshold;
                    }
                }
            }
            false
        });
        
        Ok(())
    }
}
```

### 2. RefCellVM Implementation

#### 2.1 Core Structure

```rust
pub struct RefCellVM {
    heap: RefCellHeap,
    operation_queue: VecDeque<PendingOperation>,
    main_thread: ThreadHandle,
    current_thread: ThreadHandle,
    config: VMConfig,
}

impl RefCellVM {
    pub fn new() -> LuaResult<Self> {
        Self::with_config(VMConfig::default())
    }
    
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
    
    pub fn heap(&self) -> &RefCellHeap {
        &self.heap
    }
    
    pub fn heap_mut(&mut self) -> &mut RefCellHeap {
        &mut self.heap
    }
}
```

#### 2.2 Execution Methods

```rust
impl RefCellVM {
    pub fn execute(&mut self, closure: ClosureHandle) -> LuaResult<Vec<Value>> {
        // Reset state
        self.operation_queue.clear();
        
        // Place closure at stack position 0
        self.heap.set_thread_register(self.main_thread, 0, &Value::Closure(closure))?;
        
        // Queue initial call
        self.operation_queue.push_back(PendingOperation::FunctionCall {
            func_index: 0,
            nargs: 0,
            expected_results: -1,
        });
        
        // Execute until completion
        loop {
            match self.step()? {
                StepResult::Continue => continue,
                StepResult::Completed(values) => return Ok(values),
            }
        }
    }
    
    pub fn execute_module(&mut self, module: &CompileModule, args: &[Value]) -> LuaResult<Value> {
        // Reset state
        self.operation_queue.clear();
        
        // Create a function prototype from the module
        let proto = FunctionProto {
            bytecode: module.bytecode.clone(),
            constants: module.constants.clone(),
            num_params: module.num_params,
            is_vararg: module.is_vararg,
            max_stack_size: module.max_stack_size,
            upvalues: module.upvalues.clone(),
        };
        
        // Create closure
        let closure = Closure {
            proto,
            upvalues: Vec::new(), // Top-level module has no upvalues
        };
        
        let closure_handle = self.heap.create_closure(closure)?;
        
        // Place closure at position 0 and arguments after it
        self.heap.set_thread_register(self.main_thread, 0, &Value::Closure(closure_handle))?;
        
        for (i, arg) in args.iter().enumerate() {
            self.heap.set_thread_register(self.main_thread, 1 + i, arg)?;
        }
        
        // Queue function call
        self.operation_queue.push_back(PendingOperation::FunctionCall {
            func_index: 0,
            nargs: args.len(),
            expected_results: -1,
        });
        
        // Execute until completion
        loop {
            match self.step()? {
                StepResult::Continue => continue,
                StepResult::Completed(values) => {
                    return Ok(values.get(0).cloned().unwrap_or(Value::Nil));
                }
            }
        }
    }
}
```

#### 2.3 VM Stepping Logic

```rust
impl RefCellVM {
    fn step(&mut self) -> LuaResult<StepResult> {
        // Process pending operations first
        if let Some(op) = self.operation_queue.pop_front() {
            let result = self.process_pending_operation(op)?;
            
            // Check for completion
            if matches!(result, StepResult::Completed(_)) {
                return Ok(result);
            }
            
            return Ok(StepResult::Continue);
        }
        
        // Check if there are any call frames
        if self.heap.get_thread_call_depth(self.current_thread)? == 0 {
            // Execution complete
            return Ok(StepResult::Completed(vec![]));
        }
        
        // Get current frame info
        let frame = self.heap.get_current_frame(self.current_thread)?;
        let base = frame.base_register;
        let pc = frame.pc;
        
        // Increment PC for next instruction
        self.heap.increment_pc(self.current_thread)?;
        
        // Get and execute the current instruction
        let instruction = self.heap.get_instruction(frame.closure, pc)?;
        let inst = Instruction(instruction);
        
        // Execute the instruction
        match inst.get_opcode() {
            OpCode::Move => self.op_move(inst, base)?,
            OpCode::LoadK => self.op_loadk(inst, base)?,
            // ... other opcodes ...
        }
        
        Ok(StepResult::Continue)
    }
    
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
            // ... other operations ...
        }
    }
}
```

### 3. Standard Library Implementation

#### 3.1 RefCellExecutionContext

```rust
pub struct RefCellExecutionContext<'a> {
    heap: &'a RefCellHeap,
    thread: ThreadHandle,
    base: u16,
    nargs: usize,
    results_pushed: usize,
}

impl<'a> RefCellExecutionContext<'a> {
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
    
    pub fn get_arg(&self, index: usize) -> LuaResult<Value> {
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
    
    pub fn push_result(&mut self, value: Value) -> LuaResult<()> {
        // Results go where the function was (at base), not after arguments
        let register = self.base as usize + self.results_pushed;
        self.heap.set_thread_register(self.thread, register, &value)?;
        self.results_pushed += 1;
        Ok(())
    }
    
    // ... other helper methods ...
}
```

#### 3.2 Standard Library Initialization

```rust
pub fn init_refcell_stdlib(vm: &mut RefCellVM) -> LuaResult<()> {
    // Get globals table
    let globals = vm.heap().globals()?;
    
    // Set up _G._G = _G
    let g_str = vm.heap().create_string("_G")?;
    vm.heap().set_table_field(globals, &Value::String(g_str), &Value::Table(globals))?;
    
    // Register base library functions
    register_base_lib(vm, globals)?;
    
    // Register math library
    register_math_lib(vm)?;
    
    // Register string library
    register_string_lib(vm)?;
    
    // Register table library
    register_table_lib(vm)?;
    
    Ok(())
}

fn register_function(
    vm: &RefCellVM,
    table: TableHandle,
    name: &str,
    func: CFunction
) -> LuaResult<()> {
    let name_str = vm.heap().create_string(name)?;
    vm.heap().set_table_field(table, &Value::String(name_str), &Value::CFunction(func))?;
    Ok(())
}

fn register_base_lib(vm: &mut RefCellVM, globals: TableHandle) -> LuaResult<()> {
    // Register print function
    register_function(vm, globals, "print", refcell_print)?;
    
    // Register type function
    register_function(vm, globals, "type", refcell_type)?;
    
    // ... more functions ...
    
    Ok(())
}
```

### 4. Opcode Implementation

Here's how to implement key opcodes in the RefCellVM:

#### 4.1 Basic Opcodes

```rust
impl RefCellVM {
    fn op_move(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        let value = self.heap.get_thread_register(self.current_thread, base as usize + b)?;
        self.heap.set_thread_register(self.current_thread, base as usize + a, &value)?;
        
        Ok(())
    }
    
    fn op_loadk(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let bx = inst.get_bx() as usize;
        
        let constant = self.get_constant(bx)?;
        self.heap.set_thread_register(self.current_thread, base as usize + a, &constant)?;
        
        Ok(())
    }
    
    fn op_loadnil(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let b = inst.get_b() as usize;
        
        for i in a..=b {
            self.heap.set_thread_register(self.current_thread, base as usize + i, &Value::Nil)?;
        }
        
        Ok(())
    }
}
```

#### 4.2 The Critical For Loop Implementation

```rust
impl RefCellVM {
    fn op_forprep(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let sbx = inst.get_sbx();
        
        let loop_base = base as usize + a;
        
        // Ensure stack space for all loop registers
        self.heap.grow_stack(self.current_thread, loop_base + 4)?;  // Need 4 registers
        
        // Get all loop control values
        let initial = self.heap.get_thread_register(self.current_thread, loop_base)?;
        let limit = self.heap.get_thread_register(self.current_thread, loop_base + 1)?;
        let mut step = self.heap.get_thread_register(self.current_thread, loop_base + 2)?;
        
        // CRITICAL FIX: Initialize step with default value if nil
        if step.is_nil() {
            step = Value::Number(1.0);
            self.heap.set_thread_register(self.current_thread, loop_base + 2, &step)?;
        }
        
        // Convert to numbers with error handling
        let initial_num = match initial {
            Value::Number(n) => n,
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: format!("for initial value: {}", initial.type_name()),
            }),
        };
        
        let limit_num = match limit {
            Value::Number(n) => n,
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: format!("for limit value: {}", limit.type_name()),
            }),
        };
        
        let step_num = match step {
            Value::Number(n) => n,
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: format!("for step value: {}", step.type_name()),
            }),
        };
        
        // Ensure step is not zero
        if step_num == 0.0 {
            return Err(LuaError::RuntimeError("for loop step must be different from 0".to_string()));
        }
        
        // Per Lua 5.1 spec: Subtract step from initial counter 
        let prepared_initial = initial_num - step_num;
        self.heap.set_thread_register(self.current_thread, loop_base, &Value::Number(prepared_initial))?;
        
        // Check if loop should run at all
        let should_run = if step_num > 0.0 {
            prepared_initial <= limit_num // For positive step
        } else {
            prepared_initial >= limit_num // For negative step
        };
        
        if !should_run {
            // Skip the loop entirely
            let pc = self.heap.get_pc(self.current_thread)?;
            let new_pc = (pc as i32 + sbx) as usize;
            self.heap.set_pc(self.current_thread, new_pc)?;
        }
        
        Ok(())
    }
    
    fn op_forloop(&mut self, inst: Instruction, base: u16) -> LuaResult<()> {
        let a = inst.get_a() as usize;
        let sbx = inst.get_sbx();
        
        let loop_base = base as usize + a;
        
        // Get the loop values
        let loop_var = self.heap.get_thread_register(self.current_thread, loop_base)?;
        let limit = self.heap.get_thread_register(self.current_thread, loop_base + 1)?;
        let step = self.heap.get_thread_register(self.current_thread, loop_base + 2)?;
        
        // Convert to numbers (with proper error handling)
        let loop_num = match loop_var {
            Value::Number(n) => n,
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: loop_var.type_name().to_string(),
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
        
        // Increment the loop counter
        let new_loop_num = loop_num + step_num;
        self.heap.set_thread_register(self.current_thread, loop_base, &Value::Number(new_loop_num))?;
        
        // Check if loop should continue
        let should_continue = if step_num > 0.0 {
            new_loop_num <= limit_num
        } else {
            new_loop_num >= limit_num
        };
        
        if should_continue {
            // Update the user visible variable at R(A+3)
            self.heap.set_thread_register(self.current_thread, loop_base + 3, &Value::Number(new_loop_num))?;
            
            // Jump back to loop start
            let pc = self.heap.get_pc(self.current_thread)?;
            let new_pc = (pc as i32 + sbx) as usize;
            self.heap.set_pc(self.current_thread, new_pc)?;
        }
        
        Ok(())
    }
}
```

### 5. C Function Implementation

#### 5.1 Process C Function Call

```rust
impl RefCellVM {
    fn process_c_function_call(
        &mut self,
        function: CFunction,
        base: u16,
        nargs: usize,
        expected_results: i32,
    ) -> LuaResult<StepResult> {
        // Create execution context
        let mut ctx = RefCellExecutionContext::new(&self.heap, self.current_thread, base, nargs);
        
        // Call the C function
        let actual_results = function(&mut ctx)?;
        
        // Validate result count
        if actual_results < 0 {
            return Err(LuaError::RuntimeError(
                "C function returned negative result count".to_string()
            ));
        }
        
        // Adjust results to expected count if needed
        if expected_results >= 0 {
            let expected = expected_results as usize;
            if actual_results as usize < expected {
                // Fill missing results with nil
                for i in actual_results as usize..expected {
                    ctx.set_return(i, Value::Nil)?;
                }
            }
        }
        
        Ok(StepResult::Continue)
    }
}
```

### 6. Standard Library Function Examples

Here are examples of standard library functions implemented for the RefCellVM:

#### 6.1 Print Function

```rust
pub fn refcell_print(ctx: &mut RefCellExecutionContext) -> LuaResult<i32> {
    let arg_count = ctx.arg_count();
    let mut output = Vec::new();
    
    for i in 0..arg_count {
        let value = ctx.get_arg(i)?;
        
        // Convert value to string
        let string_repr = match value {
            Value::Nil => "nil".to_string(),
            Value::Boolean(b) => b.to_string(),
            Value::Number(n) => {
                // Format number according to Lua conventions
                if n.fract() == 0.0 && n.abs() < 1e14 {
                    format!("{:.0}", n)
                } else {
                    n.to_string()
                }
            },
            Value::String(_) => {
                // Get string value
                ctx.get_arg_str(i)?
            },
            Value::Table(_) => {
                // For tables, we would ideally call tostring metamethod
                // For now, return a simple representation
                format!("table: {:?}", value)
            },
            Value::Closure(_) => {
                format!("function: {:?}", value)
            },
            Value::Thread(_) => {
                format!("thread: {:?}", value)
            },
            Value::CFunction(_) => {
                format!("function: {:?}", value)
            },
            Value::UserData(_) => {
                format!("userdata: {:?}", value)
            },
            Value::FunctionProto(_) => {
                format!("proto: {:?}", value)
            },
        };
        
        output.push(string_repr);
    }
    
    // Print the output (in a real implementation, this would go to the configured output)
    println!("{}", output.join("\t"));
    
    // print returns no values
    Ok(0)
}
```

#### 6.2 Type Function

```rust
pub fn refcell_type(ctx: &mut RefCellExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let value = ctx.get_arg(0)?;
    let type_name = value.type_name();
    
    // Create string for the type name
    let handle = ctx.create_string(type_name)?;
    ctx.push_result(Value::String(handle))?;
    
    // Return 1 result
    Ok(1)
}
```

## Common Patterns and Best Practices

### Handle Two-Phase Borrows

When you need to access multiple components that might cause borrow conflicts:

```rust
// Bad: Causes borrow checker error
let table = self.heap.get_table(handle)?;
let value = self.heap.get_table_field(table.some_field_handle, key)?;

// Good: Two-phase borrowing
let field_handle = {
    let table = self.heap.get_table(handle)?;
    table.some_field_handle.clone()
};
let value = self.heap.get_table_field(field_handle, key)?;
```

### Consistent Error Handling

```rust
// Consistent pattern for RefCell borrow errors
fn some_method(&self) -> LuaResult<T> {
    let arena = self.arena.try_borrow()
        .map_err(|e| LuaError::BorrowError(format!("Failed to borrow arena: {}", e)))?;
        
    // Rest of implementation...
}
```

### Direct Register Access

```rust
// Always pass references to set_thread_register
self.heap.set_thread_register(self.current_thread, index, &value)?;

// No need for pending writes or commits - changes take effect immediately
```

## Testing Strategies

### Unit Testing RefCellHeap

```rust
#[test]
fn test_refcell_heap_string_interning() {
    let heap = RefCellHeap::new().unwrap();
    
    // Create the same string twice
    let handle1 = heap.create_string("test").unwrap();
    let handle2 = heap.create_string("test").unwrap();
    
    // Should be interned (same handle)
    assert_eq!(handle1, handle2);
    
    // Verify content
    let value = heap.get_string_value(handle1).unwrap();
    assert_eq!(value, "test");
}
```

### Integration Testing

```rust
#[test]
fn test_refcell_vm_for_loop() {
    let mut vm = RefCellVM::new().unwrap();
    vm.init_stdlib().unwrap();
    
    // Simple for loop test
    let code = "
        local sum = 0
        for i = 1, 5 do
            sum = sum + i
        end
        return sum
    ";
    
    let module = compile(code).unwrap();
    let result = vm.execute_module(&module, &[]).unwrap();
    
    assert_eq!(result, Value::Number(15.0));
}
```

## Performance Considerations

- **String Interning**: Ensure strings are properly interned for fast lookups
- **Table Access Patterns**: Optimize array vs hash part access
- **Call Frame Management**: Efficiently handle call frames for recursive functions
- **Memory Reuse**: Reuse table slots and other resources where possible

## Error Handling

- **Provide Clear Messages**: Include context in error messages
- **Type Errors**: Give detailed information about expected vs actual types
- **Handle Borrow Errors**: Gracefully handle RefCell borrow failures
- **Upvalue Closure**: Ensure proper upvalue closure on errors

## Security Considerations

- **Resource Limits**: Implement memory and operation limits
- **Sandbox Environment**: Ensure proper isolation for embedded environments
- **Handle Validation**: Always validate handles before use

## Conclusion

This implementation guide provides the foundation for completely replacing the transaction-based VM with the RefCellVM. By following these patterns and implementations, you'll create a simpler, more maintainable, and more correct Lua implementation in Rust.

The RefCellVM approach directly addresses key issues in the transaction-based system, particularly the register corruption bug in for loops, while maintaining Rust's strong safety guarantees through the controlled interior mutability pattern.