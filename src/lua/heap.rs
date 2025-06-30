//! Lua Heap Implementation
//!
//! This module implements the memory management for the Lua VM using
//! generational arenas and a transaction-based access pattern that works
//! harmoniously with Rust's ownership model.

use std::collections::HashMap;

use super::arena::Arena;
use super::error::{LuaError, Result};
use super::value::{
    Value, LuaString, Table, Closure, Thread, Upvalue, UserData,
    StringHandle, TableHandle, ClosureHandle, ThreadHandle, UpvalueHandle,
    UserDataHandle, FunctionProto, CallFrame, ThreadStatus,
};

/// The Lua heap containing all allocated objects
pub struct LuaHeap {
    /// Current generation for validation
    generation: u32,
    
    /// String arena
    strings: Arena<LuaString>,
    
    /// Table arena
    tables: Arena<Table>,
    
    /// Closure arena
    closures: Arena<Closure>,
    
    /// Thread arena
    threads: Arena<Thread>,
    
    /// Upvalue arena
    upvalues: Arena<Upvalue>,
    
    /// User data arena
    userdata: Arena<UserData>,
    
    /// String interning cache
    string_cache: HashMap<Vec<u8>, StringHandle>,
    
    /// Registry table
    registry: Option<TableHandle>,
    
    /// Globals table
    globals: Option<TableHandle>,
    
    /// Main thread
    main_thread: Option<ThreadHandle>,
    
    /// Metatables by type
    metatables: HashMap<&'static str, TableHandle>,
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
            string_cache: HashMap::new(),
            registry: None,
            globals: None,
            main_thread: None,
            metatables: HashMap::new(),
        };
        
        // Initialize core structures
        heap.initialize();
        
        heap
    }
    
    /// Initialize core structures
    fn initialize(&mut self) {
        // Create registry table
        let registry = self.create_table_internal();
        self.registry = Some(registry);
        
        // Create globals table  
        let globals = self.create_table_internal();
        self.globals = Some(globals);
        
        // Create main thread
        let main_thread = self.create_thread_internal();
        self.main_thread = Some(main_thread);
    }
    
    /// Begin a transaction for heap modifications
    pub fn begin_transaction(&mut self) -> super::transaction::HeapTransaction {
        super::transaction::HeapTransaction::new(self)
    }
    
    /// Get the current generation
    pub fn generation(&self) -> u32 {
        self.generation
    }
    
    /// Get the registry table
    pub fn get_registry(&self) -> Result<TableHandle> {
        self.registry.clone().ok_or(LuaError::InternalError("registry not initialized".to_string()))
    }
    
    /// Get the globals table
    pub fn get_globals(&self) -> Result<TableHandle> {
        self.globals.clone().ok_or(LuaError::InternalError("globals not initialized".to_string()))
    }
    
    /// Get the main thread
    pub fn get_main_thread(&self) -> Result<ThreadHandle> {
        self.main_thread.clone().ok_or(LuaError::InternalError("main thread not initialized".to_string()))
    }
    
    // String operations
    
    /// Create a string (internal - use through transactions)
    pub(crate) fn create_string_internal(&mut self, s: &str) -> Result<StringHandle> {
        let bytes = s.as_bytes().to_vec();
        
        // Check if already interned
        if let Some(handle) = self.string_cache.get(&bytes).cloned() {
            return Ok(handle);
        }
        
        // Create new string
        let lua_string = LuaString { bytes: bytes.clone() };
        let handle = self.strings.insert(lua_string);
        let typed_handle = StringHandle::new(handle);
        
        // Cache for interning
        self.string_cache.insert(bytes, typed_handle.clone());
        
        Ok(typed_handle)
    }
    
    /// Get string bytes
    pub fn get_string(&self, handle: StringHandle) -> Result<&LuaString> {
        self.strings.get(&handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    /// Get string value as UTF-8
    pub fn get_string_value(&self, handle: StringHandle) -> Result<String> {
        let lua_str = self.get_string(handle)?;
        String::from_utf8(lua_str.bytes.clone())
            .map_err(|_| LuaError::InvalidEncoding)
    }
    
    /// Get string bytes
    pub fn get_string_bytes(&self, handle: StringHandle) -> Result<&[u8]> {
        let lua_str = self.get_string(handle)?;
        Ok(&lua_str.bytes)
    }
    
    /// Check if string handle is valid
    pub fn is_valid_string(&self, handle: StringHandle) -> bool {
        self.strings.contains(&handle.0)
    }
    
    // Table operations
    
    /// Create a table (internal)
    fn create_table_internal(&mut self) -> TableHandle {
        let table = Table::new();
        let handle = self.tables.insert(table);
        TableHandle::new(handle)
    }
    
    /// Create a table (use through transactions)
    pub(crate) fn create_table(&mut self) -> Result<TableHandle> {
        Ok(self.create_table_internal())
    }
    
    /// Get a table
    pub fn get_table(&self, handle: TableHandle) -> Result<&Table> {
        self.tables.get(&handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    /// Get a mutable table
    pub fn get_table_mut(&mut self, handle: TableHandle) -> Result<&mut Table> {
        self.tables.get_mut(&handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    /// Get table field
    pub fn get_table_field(&self, handle: TableHandle, key: &Value) -> Result<Value> {
        let table = self.get_table(handle)?;
        Ok(table.get(key).cloned().unwrap_or(Value::Nil))
    }
    
    /// Set table field (internal - use through transactions)
    pub(crate) fn set_table_field_internal(&mut self, handle: TableHandle, key: Value, value: Value) -> Result<()> {
        let table = self.get_table_mut(handle)?;
        table.set(key, value);
        Ok(())
    }
    
    /// Get table metatable
    pub fn get_metatable(&self, handle: TableHandle) -> Result<Option<TableHandle>> {
        let table = self.get_table(handle)?;
        Ok(table.metatable.clone())
    }
    
    /// Set table metatable (internal - use through transactions)
    pub(crate) fn set_metatable_internal(&mut self, handle: TableHandle, metatable: Option<TableHandle>) -> Result<()> {
        let table = self.get_table_mut(handle)?;
        table.metatable = metatable;
        Ok(())
    }
    
    /// Get metamethod
    pub fn get_metamethod(&self, handle: TableHandle, method: StringHandle) -> Result<Value> {
        if let Some(metatable) = self.get_metatable(handle)? {
            self.get_table_field(metatable, &Value::String(method))
        } else {
            Ok(Value::Nil)
        }
    }
    
    /// Check if table handle is valid
    pub fn is_valid_table(&self, handle: TableHandle) -> bool {
        self.tables.contains(&handle.0)
    }
    
    // Closure operations
    
    /// Create a closure (use through transactions)
    pub(crate) fn create_closure(&mut self, proto: FunctionProto, upvalues: Vec<UpvalueHandle>) -> Result<ClosureHandle> {
        let closure = Closure { proto, upvalues };
        let handle = self.closures.insert(closure);
        Ok(ClosureHandle::new(handle))
    }
    
    /// Get a closure
    pub fn get_closure(&self, handle: ClosureHandle) -> Result<&Closure> {
        self.closures.get(&handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    /// Get a mutable closure
    pub fn get_closure_mut(&mut self, handle: ClosureHandle) -> Result<&mut Closure> {
        self.closures.get_mut(&handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    /// Check if closure handle is valid
    pub fn is_valid_closure(&self, handle: ClosureHandle) -> bool {
        self.closures.contains(&handle.0)
    }
    
    // Thread operations
    
    /// Create a thread (internal)
    fn create_thread_internal(&mut self) -> ThreadHandle {
        let thread = Thread::new();
        let handle = self.threads.insert(thread);
        ThreadHandle::new(handle)
    }
    
    /// Create a thread (use through transactions)
    pub(crate) fn create_thread(&mut self) -> Result<ThreadHandle> {
        Ok(self.create_thread_internal())
    }
    
    /// Get a thread
    pub fn get_thread(&self, handle: ThreadHandle) -> Result<&Thread> {
        self.threads.get(&handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    /// Get a mutable thread
    pub fn get_thread_mut(&mut self, handle: ThreadHandle) -> Result<&mut Thread> {
        self.threads.get_mut(&handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    /// Get thread call depth
    pub fn get_thread_call_depth(&self, handle: ThreadHandle) -> Result<usize> {
        let thread = self.get_thread(handle)?;
        Ok(thread.call_frames.len())
    }
    
    /// Get thread stack size
    pub fn get_thread_stack_size(&self, handle: ThreadHandle) -> Result<usize> {
        let thread = self.get_thread(handle)?;
        Ok(thread.stack.len())
    }
    
    /// Get thread register
    pub fn get_thread_register(&self, handle: ThreadHandle, index: usize) -> Result<Value> {
        let thread = self.get_thread(handle)?;
        if index < thread.stack.len() {
            Ok(thread.stack[index].clone())
        } else {
            Ok(Value::Nil)
        }
    }
    
    /// Set thread register (internal - use through transactions)
    pub(crate) fn set_thread_register_internal(&mut self, handle: ThreadHandle, index: usize, value: Value) -> Result<()> {
        let thread = self.get_thread_mut(handle)?;
        
        // Extend stack if needed
        if index >= thread.stack.len() {
            thread.stack.resize(index + 1, Value::Nil);
        }
        
        thread.stack[index] = value;
        Ok(())
    }
    
    /// Push call frame (internal - use through transactions)
    pub(crate) fn push_call_frame_internal(&mut self, handle: ThreadHandle, frame: CallFrame) -> Result<()> {
        let thread = self.get_thread_mut(handle)?;
        
        // Check stack depth
        if thread.call_frames.len() >= 1000 {
            return Err(LuaError::StackOverflow);
        }
        
        thread.call_frames.push(frame);
        Ok(())
    }
    
    /// Pop call frame (internal - use through transactions)
    pub(crate) fn pop_call_frame_internal(&mut self, handle: ThreadHandle) -> Result<CallFrame> {
        let thread = self.get_thread_mut(handle)?;
        thread.call_frames.pop()
            .ok_or(LuaError::StackEmpty)
    }
    
    /// Get current call frame
    pub fn get_current_frame(&self, handle: ThreadHandle) -> Result<&CallFrame> {
        let thread = self.get_thread(handle)?;
        thread.call_frames.last()
            .ok_or(LuaError::StackEmpty)
    }
    
    /// Get current call frame mutably 
    pub fn get_current_frame_mut(&mut self, handle: ThreadHandle) -> Result<&mut CallFrame> {
        let thread = self.get_thread_mut(handle)?;
        thread.call_frames.last_mut()
            .ok_or(LuaError::StackEmpty)
    }
    
    /// Increment PC (internal - use through transactions)
    pub(crate) fn increment_pc_internal(&mut self, handle: ThreadHandle) -> Result<()> {
        let frame = self.get_current_frame_mut(handle)?;
        frame.pc += 1;
        Ok(())
    }
    
    /// Check if thread handle is valid
    pub fn is_valid_thread(&self, handle: ThreadHandle) -> bool {
        self.threads.contains(&handle.0)
    }
    
    /// Reset thread (internal - use through transactions)
    pub(crate) fn reset_thread_internal(&mut self, handle: ThreadHandle) -> Result<()> {
        let thread = self.get_thread_mut(handle)?;
        thread.call_frames.clear();
        thread.stack.clear();
        thread.status = ThreadStatus::Ready;
        Ok(())
    }
    
    /// Push to thread stack (internal - use through transactions)
    pub(crate) fn push_thread_stack_internal(&mut self, handle: ThreadHandle, value: Value) -> Result<()> {
        let thread = self.get_thread_mut(handle)?;
        
        // Check stack size limit
        if thread.stack.len() >= 1000000 {
            return Err(LuaError::StackOverflow);
        }
        
        thread.stack.push(value);
        Ok(())
    }
    
    /// Get thread stack value
    pub fn get_thread_stack_value(&self, handle: ThreadHandle, index: usize) -> Result<Value> {
        let thread = self.get_thread(handle)?;
        if index < thread.stack.len() {
            Ok(thread.stack[index].clone())
        } else {
            Ok(Value::Nil)
        }
    }
    
    // Upvalue operations
    
    /// Create an upvalue (use through transactions)
    pub(crate) fn create_upvalue(&mut self, value: Upvalue) -> Result<UpvalueHandle> {
        let handle = self.upvalues.insert(value);
        Ok(UpvalueHandle::new(handle))
    }
    
    /// Get an upvalue
    pub fn get_upvalue(&self, handle: UpvalueHandle) -> Result<&Upvalue> {
        self.upvalues.get(&handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    /// Get a mutable upvalue
    pub fn get_upvalue_mut(&mut self, handle: UpvalueHandle) -> Result<&mut Upvalue> {
        self.upvalues.get_mut(&handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    /// Check if upvalue handle is valid
    pub fn is_valid_upvalue(&self, handle: UpvalueHandle) -> bool {
        self.upvalues.contains(&handle.0)
    }
    
    // UserData operations
    
    /// Create userdata (use through transactions)
    pub(crate) fn create_userdata(&mut self, data_type: String) -> Result<UserDataHandle> {
        let userdata = UserData {
            data_type,
            metatable: None,
        };
        let handle = self.userdata.insert(userdata);
        Ok(UserDataHandle::new(handle))
    }
    
    /// Get userdata
    pub fn get_userdata(&self, handle: UserDataHandle) -> Result<&UserData> {
        self.userdata.get(&handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    /// Get mutable userdata
    pub fn get_userdata_mut(&mut self, handle: UserDataHandle) -> Result<&mut UserData> {
        self.userdata.get_mut(&handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    /// Check if userdata handle is valid
    pub fn is_valid_userdata(&self, handle: UserDataHandle) -> bool {
        self.userdata.contains(&handle.0)
    }
}

impl Default for LuaHeap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heap_creation() {
        let heap = LuaHeap::new();
        
        // Core structures should be initialized
        assert!(heap.registry.is_some());
        assert!(heap.globals.is_some());
        assert!(heap.main_thread.is_some());
    }
    
    #[test]
    fn test_string_interning() {
        let mut heap = LuaHeap::new();
        
        let s1 = heap.create_string_internal("hello").unwrap();
        let s2 = heap.create_string_internal("hello").unwrap();
        
        // Should return same handle for same string
        assert_eq!(s1, s2);
    }
    
    #[test]
    fn test_table_operations() {
        let mut heap = LuaHeap::new();
        
        let table = heap.create_table().unwrap();
        assert!(heap.is_valid_table(table));
        
        // Set and get field
        let key = Value::String(heap.create_string_internal("key").unwrap());
        let value = Value::Number(42.0);
        
        heap.set_table_field_internal(table, key.clone(), value).unwrap();
        
        let retrieved = heap.get_table_field(table, &key).unwrap();
        assert_eq!(retrieved, value);
    }
}