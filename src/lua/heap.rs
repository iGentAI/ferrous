//! Lua Heap Implementation
//!
//! This module implements the memory management for the Lua VM,
//! using arenas and handles to avoid raw pointers and memory issues.

use std::collections::HashMap;
use std::any::Any;
use std::sync::Arc;

use super::arena::{Arena, TypedHandle, Handle, ValidScope};
use super::error::{LuaError, Result};
use super::value::{
    Value, LuaString, Table, Closure, LuaThread, Upvalue, ThreadStatus,
    StringHandle, TableHandle, ClosureHandle, ThreadHandle, UpvalueHandle,
    UserData, UserDataHandle, CallFrame, FunctionProto, CallFrameType,
};

/// Lua heap implementation
pub struct LuaHeap {
    /// Current generation
    generation: u32,
    
    /// String arena
    strings: Arena<LuaString>,
    
    /// Table arena
    tables: Arena<Table>,
    
    /// Closure arena
    closures: Arena<Closure>,
    
    /// Thread arena
    threads: Arena<LuaThread>,
    
    /// Upvalue arena
    upvalues: Arena<Upvalue>,
    
    /// User data arena
    userdata: Arena<UserData>,
    
    /// Registry table
    registry: Option<TableHandle>,
    
    /// Globals table
    globals: Option<TableHandle>,
    
    /// String interning cache
    string_cache: HashMap<Vec<u8>, StringHandle>,
    
    /// Main thread
    main_thread: Option<ThreadHandle>,
    
    /// Pending operations
    pending_operations: Vec<super::vm::PendingOperation>,
}

impl LuaHeap {
    /// Create a new heap
    pub fn new() -> Self {
        let mut heap = LuaHeap {
            generation: 0,
            strings: Arena::new(),
            tables: Arena::new(),
            closures: Arena::new(),
            threads: Arena::new(),
            upvalues: Arena::new(),
            userdata: Arena::new(),
            registry: None,
            globals: None,
            string_cache: HashMap::new(),
            main_thread: None,
            pending_operations: Vec::new(),
        };
        
        // Create registry and globals table
        let registry = heap.create_table_internal().unwrap();
        let globals = heap.create_table_internal().unwrap();
        
        heap.registry = Some(registry);
        heap.globals = Some(globals);
        
        // Create main thread
        let main_thread = heap.create_thread_internal().unwrap();
        heap.main_thread = Some(main_thread);
        
        heap
    }
    
    /// Create a validation scope
    pub fn validation_scope(&self) -> ValidScope {
        ValidScope::new(self.generation)
    }
    
    /// Get the registry table
    pub fn get_registry(&self) -> Result<TableHandle> {
        self.registry.ok_or_else(|| LuaError::InternalError("registry not initialized".to_string()))
    }
    
    /// Get the globals table
    pub fn get_globals(&self) -> Result<TableHandle> {
        self.globals.ok_or_else(|| LuaError::InternalError("globals not initialized".to_string()))
    }
    
    /// Get the main thread
    pub fn get_main_thread(&self) -> Result<ThreadHandle> {
        self.main_thread.ok_or_else(|| LuaError::InternalError("main thread not initialized".to_string()))
    }
    
    /// Create a string
    pub fn create_string(&mut self, s: &str) -> Result<StringHandle> {
        let bytes = s.as_bytes().to_vec();
        
        // Check if already interned
        if let Some(handle) = self.string_cache.get(&bytes) {
            return Ok(*handle);
        }
        
        // Create new string
        let lua_string = LuaString {
            bytes: bytes.clone(),
        };
        
        let handle = StringHandle(self.strings.insert(lua_string));
        
        // Cache it
        self.string_cache.insert(bytes, handle);
        
        Ok(handle)
    }
    
    /// Get string value
    pub fn get_string_value(&self, handle: StringHandle) -> Result<String> {
        let string = self.strings.get(handle.0).ok_or(LuaError::InvalidHandle)?;
        String::from_utf8(string.bytes.clone()).map_err(|_| LuaError::InvalidEncoding)
    }
    
    /// Get string bytes
    pub fn get_string_bytes(&self, handle: StringHandle) -> Result<&[u8]> {
        let string = self.strings.get(handle.0).ok_or(LuaError::InvalidHandle)?;
        Ok(&string.bytes)
    }
    
    /// Append to a string
    pub fn append_string(&mut self, handle: StringHandle, s: &str) -> Result<()> {
        let string = self.strings.get_mut(handle.0).ok_or(LuaError::InvalidHandle)?;
        string.bytes.extend_from_slice(s.as_bytes());
        Ok(())
    }
    
    /// Create a table (internal version)
    fn create_table_internal(&mut self) -> Result<TableHandle> {
        let table = Table {
            array: Vec::new(),
            hash_map: Vec::new(),
            metatable: None,
        };
        
        Ok(TableHandle(self.tables.insert(table)))
    }
    
    /// Create a table
    pub fn create_table(&mut self) -> Result<TableHandle> {
        self.create_table_internal()
    }
    
    /// Get a table
    pub fn get_table(&self, handle: TableHandle) -> Result<&Table> {
        self.tables.get(handle.0).ok_or(LuaError::InvalidHandle)
    }
    
    /// Get a mutable table
    pub fn get_table_mut(&mut self, handle: TableHandle) -> Result<&mut Table> {
        self.tables.get_mut(handle.0).ok_or(LuaError::InvalidHandle)
    }
    
    /// Get a table field
    pub fn get_table_field(&self, handle: TableHandle, key: &Value) -> Result<Value> {
        let table = self.get_table(handle)?;
        
        // Check array part for integer keys
        if let Value::Number(n) = key {
            if n.fract() == 0.0 && *n > 0.0 && *n <= table.array.len() as f64 {
                let idx = *n as usize - 1; // Lua is 1-indexed
                return Ok(table.array[idx].clone());
            }
        }
        
        // Check hash part
        for (k, v) in &table.hash_map {
            if k == key {
                return Ok(v.clone());
            }
        }
        
        // Not found
        Err(LuaError::TableKeyNotFound)
    }
    
    /// Set a table field
    pub fn set_table_field(&mut self, handle: TableHandle, key: Value, value: Value) -> Result<()> {
        let table = self.get_table_mut(handle)?;
        
        // Check if key is a valid array index
        if let Value::Number(n) = &key {
            if n.fract() == 0.0 && *n > 0.0 {
                let idx = *n as usize - 1; // Lua is 1-indexed
                
                // Extend array if needed
                if idx >= table.array.len() {
                    table.array.resize(idx + 1, Value::Nil);
                }
                
                table.array[idx] = value;
                return Ok(());
            }
        }
        
        // Check hash part
        for (k, v) in table.hash_map.iter_mut() {
            if *k == key {
                *v = value;
                return Ok(());
            }
        }
        
        // Not found, add new entry
        table.hash_map.push((key, value));
        Ok(())
    }
    
    /// Get a table's metatable
    pub fn get_metatable(&self, handle: TableHandle) -> Result<Option<TableHandle>> {
        let table = self.get_table(handle)?;
        Ok(table.metatable)
    }
    
    /// Set a table's metatable
    pub fn set_metatable(&mut self, handle: TableHandle, metatable: Option<TableHandle>) -> Result<()> {
        let table = self.get_table_mut(handle)?;
        table.metatable = metatable;
        Ok(())
    }
    
    /// Get a metamethod from a table
    pub fn get_metamethod(&self, handle: TableHandle, method: StringHandle) -> Result<Value> {
        let metatable = self.get_metatable(handle)?;
        
        if let Some(mt) = metatable {
            match self.get_table_field(mt, &Value::String(method)) {
                Ok(value) => Ok(value),
                Err(LuaError::TableKeyNotFound) => Ok(Value::Nil),
                Err(e) => Err(e),
            }
        } else {
            // No metatable
            Ok(Value::Nil)
        }
    }
    
    /// Create a thread (internal version)
    fn create_thread_internal(&mut self) -> Result<ThreadHandle> {
        let thread = LuaThread {
            call_frames: Vec::new(),
            stack: Vec::new(),
            status: ThreadStatus::Ready,
        };
        
        Ok(ThreadHandle(self.threads.insert(thread)))
    }
    
    /// Create a thread
    pub fn create_thread(&mut self) -> Result<ThreadHandle> {
        self.create_thread_internal()
    }
    
    /// Get a thread
    pub fn get_thread(&self, handle: ThreadHandle) -> Result<&LuaThread> {
        self.threads.get(handle.0).ok_or(LuaError::InvalidHandle)
    }
    
    /// Get a mutable thread
    pub fn get_thread_mut(&mut self, handle: ThreadHandle) -> Result<&mut LuaThread> {
        self.threads.get_mut(handle.0).ok_or(LuaError::InvalidHandle)
    }
    
    /// Get thread call depth
    pub fn get_thread_call_depth(&self, handle: ThreadHandle) -> Result<usize> {
        let thread = self.get_thread(handle)?;
        Ok(thread.call_frames.len())
    }
    
    /// Get current thread frame
    pub fn get_thread_current_frame(&self, handle: ThreadHandle) -> Result<&CallFrame> {
        let thread = self.get_thread(handle)?;
        thread.call_frames.last().ok_or(LuaError::StackEmpty)
    }
    
    /// Get current thread frame mutably
    pub fn get_thread_current_frame_mut(&mut self, handle: ThreadHandle) -> Result<&mut CallFrame> {
        let thread = self.get_thread_mut(handle)?;
        thread.call_frames.last_mut().ok_or(LuaError::StackEmpty)
    }
    
    /// Create a call frame
    pub fn create_call_frame(&self, closure: ClosureHandle) -> Result<CallFrame> {
        // Just create the frame object - doesn't modify heap
        Ok(CallFrame {
            closure,
            pc: 0,
            base_register: 0, // Will be set by caller
            return_count: 1,  // Default to 1 return value
            frame_type: CallFrameType::Normal,
        })
    }
    
    /// Push a call frame
    pub fn push_thread_call_frame(&mut self, handle: ThreadHandle, frame: CallFrame) -> Result<()> {
        let thread = self.get_thread_mut(handle)?;
        
        // Check for stack overflow
        if thread.call_frames.len() >= 1000 {
            return Err(LuaError::StackOverflow);
        }
        
        thread.call_frames.push(frame);
        Ok(())
    }
    
    /// Pop a call frame
    pub fn pop_thread_call_frame(&mut self, handle: ThreadHandle) -> Result<CallFrame> {
        let thread = self.get_thread_mut(handle)?;
        thread.call_frames.pop().ok_or(LuaError::StackEmpty)
    }
    
    /// Get thread register
    pub fn get_thread_register(&self, handle: ThreadHandle, index: usize) -> Result<Value> {
        let thread = self.get_thread(handle)?;
        
        // Check if the register exists
        if index >= thread.stack.len() {
            return Ok(Value::Nil);
        }
        
        Ok(thread.stack[index].clone())
    }
    
    /// Set thread register
    pub fn set_thread_register(&mut self, handle: ThreadHandle, index: usize, value: Value) -> Result<()> {
        let thread = self.get_thread_mut(handle)?;
        
        // Extend stack if needed
        if index >= thread.stack.len() {
            thread.stack.resize(index + 1, Value::Nil);
        }
        
        thread.stack[index] = value;
        Ok(())
    }
    
    /// Get thread stack size
    pub fn get_thread_stack_size(&self, handle: ThreadHandle) -> Result<usize> {
        let thread = self.get_thread(handle)?;
        Ok(thread.stack.len())
    }
    
    /// Set thread stack size
    pub fn set_thread_stack_size(&mut self, handle: ThreadHandle, size: usize) -> Result<()> {
        let thread = self.get_thread_mut(handle)?;
        thread.stack.truncate(size);
        Ok(())
    }
    
    /// Get thread stack value
    pub fn get_thread_stack_value(&self, handle: ThreadHandle, index: usize) -> Result<Value> {
        let thread = self.get_thread(handle)?;
        
        if index >= thread.stack.len() {
            return Ok(Value::Nil);
        }
        
        Ok(thread.stack[index].clone())
    }
    
    /// Push value to thread stack
    pub fn push_thread_stack(&mut self, handle: ThreadHandle, value: Value) -> Result<()> {
        let thread = self.get_thread_mut(handle)?;
        
        // Check for stack overflow
        if thread.stack.len() >= 1000 {
            return Err(LuaError::StackOverflow);
        }
        
        thread.stack.push(value);
        Ok(())
    }
    
    /// Pop value from thread stack
    pub fn pop_thread_stack(&mut self, handle: ThreadHandle) -> Result<Value> {
        let thread = self.get_thread_mut(handle)?;
        thread.stack.pop().ok_or(LuaError::StackEmpty)
    }
    
    /// Get thread PC
    pub fn get_thread_pc(&self, handle: ThreadHandle, frame_index: usize) -> Result<usize> {
        let thread = self.get_thread(handle)?;
        
        if frame_index >= thread.call_frames.len() {
            return Err(LuaError::InvalidOperation("Frame index out of range".to_string()));
        }
        
        Ok(thread.call_frames[frame_index].pc)
    }
    
    /// Set thread PC
    pub fn set_thread_pc(&mut self, handle: ThreadHandle, frame_index: usize, pc: usize) -> Result<()> {
        let thread = self.get_thread_mut(handle)?;
        
        if frame_index >= thread.call_frames.len() {
            return Err(LuaError::InvalidOperation("Frame index out of range".to_string()));
        }
        
        thread.call_frames[frame_index].pc = pc;
        Ok(())
    }
    
    /// Create a closure
    pub fn create_closure(&mut self, proto: FunctionProto, upvalues: Vec<UpvalueHandle>) -> Result<ClosureHandle> {
        let closure = Closure {
            proto,
            upvalues,
        };
        
        Ok(ClosureHandle(self.closures.insert(closure)))
    }
    
    /// Get a closure
    pub fn get_closure(&self, handle: ClosureHandle) -> Result<&Closure> {
        self.closures.get(handle.0).ok_or(LuaError::InvalidHandle)
    }
    
    /// Get a mutable closure
    pub fn get_closure_mut(&mut self, handle: ClosureHandle) -> Result<&mut Closure> {
        self.closures.get_mut(handle.0).ok_or(LuaError::InvalidHandle)
    }
    
    /// Create an upvalue
    pub fn create_upvalue(&mut self, value: Value) -> Result<UpvalueHandle> {
        let upvalue = Upvalue::Closed(value);
        Ok(UpvalueHandle(self.upvalues.insert(upvalue)))
    }
    
    /// Get an upvalue
    pub fn get_upvalue(&self, handle: UpvalueHandle) -> Result<&Upvalue> {
        self.upvalues.get(handle.0).ok_or(LuaError::InvalidHandle)
    }
    
    /// Get a mutable upvalue
    pub fn get_upvalue_mut(&mut self, handle: UpvalueHandle) -> Result<&mut Upvalue> {
        self.upvalues.get_mut(handle.0).ok_or(LuaError::InvalidHandle)
    }
    
    /// Get upvalue value
    pub fn get_upvalue_value(&self, closure: ClosureHandle, index: usize) -> Result<Value> {
        let closure_obj = self.get_closure(closure)?;
        
        if index >= closure_obj.upvalues.len() {
            return Err(LuaError::InvalidUpvalue);
        }
        
        let upvalue_handle = closure_obj.upvalues[index];
        
        match self.get_upvalue(upvalue_handle)? {
            Upvalue::Closed(value) => Ok(value.clone()),
            Upvalue::Open { thread, stack_index } => {
                self.get_thread_register(*thread, *stack_index)
            }
        }
    }
    
    /// Set upvalue value
    pub fn set_upvalue(&mut self, closure: ClosureHandle, index: usize, value: Value) -> Result<()> {
        let closure_obj = self.get_closure(closure)?;
        
        if index >= closure_obj.upvalues.len() {
            return Err(LuaError::InvalidUpvalue);
        }
        
        let upvalue_handle = closure_obj.upvalues[index];
        
        match self.get_upvalue_mut(upvalue_handle)? {
            Upvalue::Closed(v) => {
                *v = value;
                Ok(())
            }
            Upvalue::Open { thread, stack_index } => {
                // Copy the thread and index so we don't borrow the upvalue anymore
                let t = *thread;
                let idx = *stack_index;
                self.set_thread_register(t, idx, value)
            }
        }
    }
    
    /// Create userdata
    pub fn create_userdata(&mut self, type_name: String) -> Result<UserDataHandle> {
        let userdata = UserData {
            data_type: type_name,
            metatable: None,
        };
        
        Ok(UserDataHandle(self.userdata.insert(userdata)))
    }
    
    /// Get userdata
    pub fn get_userdata(&self, handle: UserDataHandle) -> Result<&UserData> {
        self.userdata.get(handle.0).ok_or(LuaError::InvalidHandle)
    }
    
    /// Get mutable userdata
    pub fn get_userdata_mut(&mut self, handle: UserDataHandle) -> Result<&mut UserData> {
        self.userdata.get_mut(handle.0).ok_or(LuaError::InvalidHandle)
    }
    
    /// Reset a thread
    pub fn reset_thread(&mut self, handle: ThreadHandle) -> Result<()> {
        let thread = self.get_thread_mut(handle)?;
        thread.call_frames.clear();
        thread.stack.clear();
        thread.status = ThreadStatus::Ready;
        Ok(())
    }
    
    /// Check if a string handle is valid
    pub fn is_valid_string(&self, handle: StringHandle) -> bool {
        self.strings.contains(handle.0)
    }
    
    /// Check if a table handle is valid
    pub fn is_valid_table(&self, handle: TableHandle) -> bool {
        self.tables.contains(handle.0)
    }
    
    /// Check if a closure handle is valid
    pub fn is_valid_closure(&self, handle: ClosureHandle) -> bool {
        self.closures.contains(handle.0)
    }
    
    /// Check if a thread handle is valid
    pub fn is_valid_thread(&self, handle: ThreadHandle) -> bool {
        self.threads.contains(handle.0)
    }
    
    /// Check if an upvalue handle is valid
    pub fn is_valid_upvalue(&self, handle: UpvalueHandle) -> bool {
        self.upvalues.contains(handle.0)
    }
    
    /// Check if a userdata handle is valid
    pub fn is_valid_userdata(&self, handle: UserDataHandle) -> bool {
        self.userdata.contains(handle.0)
    }
    
    /// Queue an operation for processing
    pub fn queue_operation(&mut self, operation: super::vm::PendingOperation) -> Result<()> {
        self.pending_operations.push(operation);
        Ok(())
    }
    
    /// Get and clear pending operations
    pub fn take_pending_operations(&mut self) -> Vec<super::vm::PendingOperation> {
        std::mem::take(&mut self.pending_operations)
    }
}