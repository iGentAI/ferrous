//! Lua Heap Management
//! 
//! This module implements the heap for storing Lua objects with handle-based
//! memory management and generation tracking for safety.

use super::arena::Arena;
use super::error::{LuaError, LuaResult};
use super::handle::{StringHandle, TableHandle, ClosureHandle, ThreadHandle, 
                    UpvalueHandle, UserDataHandle, FunctionProtoHandle};
use super::resource::ResourceLimits;
use super::value::{Value, HashableValue, OrderedFloat, LuaString, Table, Closure, 
                   Thread, Upvalue, UserData, FunctionProto};
use super::vm::PendingOperation;
use std::collections::HashMap;

/// The main Lua heap containing all allocated objects
pub struct LuaHeap {
    /// Arena for string storage
    strings: Arena<LuaString>,
    
    /// String interning cache for deduplication
    string_cache: HashMap<Vec<u8>, StringHandle>,
    
    /// Arena for table storage
    tables: Arena<Table>,
    
    /// Arena for closure storage
    closures: Arena<Closure>,
    
    /// Arena for thread storage
    threads: Arena<Thread>,
    
    /// Arena for upvalue storage
    upvalues: Arena<Upvalue>,
    
    /// Arena for userdata storage
    userdata: Arena<UserData>,
    
    /// Arena for function prototype storage
    function_protos: Arena<FunctionProto>,
    
    /// Global table handle
    globals: Option<TableHandle>,
    
    /// Registry table handle
    registry: Option<TableHandle>,
    
    /// Main thread handle
    main_thread: Option<ThreadHandle>,
    
    /// Current generation for validation
    generation: u32,
    
    /// Resource limits for this VM instance 
    pub resource_limits: ResourceLimits,
}

impl LuaHeap {
    /// Create a new heap
    pub fn new() -> LuaResult<Self> {
        let mut heap = LuaHeap {
            strings: Arena::new(),
            string_cache: HashMap::new(),
            tables: Arena::new(),
            closures: Arena::new(),
            threads: Arena::new(),
            upvalues: Arena::new(),
            userdata: Arena::new(),
            function_protos: Arena::new(),
            globals: None,
            registry: None,
            main_thread: None,
            generation: 0,
            resource_limits: ResourceLimits::default(),
        };

        // Pre-intern common strings
        heap.pre_intern_common_strings()?;
        
        // Create globals table
        let globals_handle = heap.create_table_internal()?;
        heap.globals = Some(globals_handle);
        
        // Create registry table
        let registry_handle = heap.create_table_internal()?;
        heap.registry = Some(registry_handle);
        
        // Create main thread
        let main_thread_handle = heap.create_thread_internal()?;
        heap.main_thread = Some(main_thread_handle);
        
        Ok(heap)
    }
    
    /// Pre-intern common strings used by the VM
    fn pre_intern_common_strings(&mut self) -> LuaResult<()> {
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
            self.create_string_internal(s)?;
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
    
    // String operations
    
    /// Create a string with validation
    pub fn create_string_with_validation(&mut self, s: &str, validated_handles: &[StringHandle]) -> LuaResult<StringHandle> {
        // Re-validate handles after potential reallocation
        for handle in validated_handles {
            self.validate_string_handle(*handle)?;
        }
        
        self.create_string_internal(s)
    }
    
    /// Internal string creation with interning
    fn create_string_internal(&mut self, s: &str) -> LuaResult<StringHandle> {
        // Check cache first
        if let Some(&handle) = self.string_cache.get(s.as_bytes()) {
            // Verify handle is still valid
            if self.strings.contains(handle.0) {
                return Ok(handle);
            }
            // Stale cache entry, remove it
            self.string_cache.remove(s.as_bytes());
        }
        
        // Create new string
        let lua_string = LuaString::new(s);
        let handle = StringHandle::from(self.strings.insert(lua_string));
        
        // Add to cache
        self.string_cache.insert(s.as_bytes().to_vec(), handle);
        
        Ok(handle)
    }
    
    /// Validate a string handle
    pub fn validate_string_handle(&self, handle: StringHandle) -> LuaResult<()> {
        if !self.strings.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        Ok(())
    }
    
    /// Get a string by handle
    pub fn get_string(&self, handle: StringHandle) -> LuaResult<&LuaString> {
        self.strings.get(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    // Table operations
    
    /// Create a table with validation
    pub fn create_table_with_validation(&mut self, validated_handles: &[TableHandle]) -> LuaResult<TableHandle> {
        // Re-validate handles after potential reallocation
        for handle in validated_handles {
            self.validate_table_handle(*handle)?;
        }
        
        self.create_table_internal()
    }
    
    /// Internal table creation
    fn create_table_internal(&mut self) -> LuaResult<TableHandle> {
        let table = Table::new();
        let handle = TableHandle::from(self.tables.insert(table));
        Ok(handle)
    }
    
    /// Validate a table handle
    pub fn validate_table_handle(&self, handle: TableHandle) -> LuaResult<()> {
        if !self.tables.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        Ok(())
    }
    
    /// Get a table by handle
    pub fn get_table(&self, handle: TableHandle) -> LuaResult<&Table> {
        self.tables.get(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    /// Get a mutable table by handle
    pub fn get_table_mut(&mut self, handle: TableHandle) -> LuaResult<&mut Table> {
        self.tables.get_mut(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    /// Get table field
    pub fn get_table_field_internal(&self, table: TableHandle, key: &Value) -> LuaResult<Value> {
        let table = self.get_table(table)?;
        
        // Try array optimization for integer keys
        if let Value::Number(n) = key {
            if n.fract() == 0.0 && *n > 0.0 && *n <= table.array.len() as f64 {
                let idx = *n as usize;
                return Ok(table.array[idx - 1].clone());
            }
        }
        
        // For string keys, we need to get the content hash
        if let Value::String(string_handle) = key {
            let string = self.get_string(*string_handle)?;
            let hashable = HashableValue::String(*string_handle, string.content_hash);
            return Ok(table.get_by_hashable(&hashable).cloned().unwrap_or(Value::Nil));
        }
        
        // For other hashable keys
        match HashableValue::from_value_with_context(key, "get_table_field_internal") {
            Ok(hashable) => {
                Ok(table.get_by_hashable(&hashable).cloned().unwrap_or(Value::Nil))
            },
            Err(_) => {
                // Key is not hashable (e.g., a table or function)
                Ok(Value::Nil)
            }
        }
    }
    
    /// Set table field
    pub fn set_table_field_internal(&mut self, table: TableHandle, key: &Value, value: &Value) -> LuaResult<()> {
        // Try array optimization for integer keys
        if let Value::Number(n) = key {
            if n.fract() == 0.0 && *n > 0.0 {
                let idx = *n as usize;
                let table = self.get_table_mut(table)?;
                
                // Expand array if needed
                if idx <= table.array.len() + 1 {
                    if idx == table.array.len() + 1 {
                        table.array.push(value.clone());
                    } else if idx <= table.array.len() {
                        table.array[idx - 1] = value.clone();
                    }
                    return Ok(());
                }
            }
        }
        
        // For string keys, we need to get the content hash
        if let Value::String(string_handle) = key {
            let string = self.get_string(*string_handle)?;
            let hashable = HashableValue::String(*string_handle, string.content_hash);
            let table = self.get_table_mut(table)?;
            table.set_by_hashable(hashable, value.clone());
            return Ok(());
        }
        
        // For other hashable keys
        match HashableValue::from_value_with_context(key, "set_table_field_internal") {
            Ok(hashable) => {
                let table = self.get_table_mut(table)?;
                table.set_by_hashable(hashable, value.clone());
                Ok(())
            },
            Err(_) => {
                // Key is not hashable - set as nil (matches Lua behavior)
                Ok(())
            }
        }
    }
    
    /// Get table metatable  
    pub fn get_table_metatable_internal(&self, table: TableHandle) -> LuaResult<Option<TableHandle>> {
        let table = self.get_table(table)?;
        Ok(table.metatable)
    }
    
    /// Set table metatable
    pub fn set_table_metatable_internal(&mut self, table: TableHandle, metatable: &Option<TableHandle>) -> LuaResult<()> {
        let table = self.get_table_mut(table)?;
        table.metatable = *metatable;
        Ok(())
    }
    
    // Thread operations
    
    /// Create a thread
    pub fn create_thread_internal(&mut self) -> LuaResult<ThreadHandle> {
        let thread = Thread::new();
        let handle = ThreadHandle::from(self.threads.insert(thread));
        Ok(handle)
    }
    
    /// Validate a thread handle
    pub fn validate_thread_handle(&self, handle: ThreadHandle) -> LuaResult<()> {
        if !self.threads.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        Ok(())
    }
    
    /// Get a thread by handle
    pub fn get_thread(&self, handle: ThreadHandle) -> LuaResult<&Thread> {
        self.threads.get(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    /// Get a mutable thread by handle
    pub fn get_thread_mut(&mut self, handle: ThreadHandle) -> LuaResult<&mut Thread> {
        self.threads.get_mut(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    /// Get thread register
    pub fn get_thread_register_internal(&self, thread: ThreadHandle, index: usize) -> LuaResult<Value> {
        let thread = self.get_thread(thread)?;
        thread.stack.get(index)
            .cloned()
            .ok_or_else(|| LuaError::RuntimeError(format!(
                "Register {} out of bounds (stack size: {})",
                index,
                thread.stack.len()
            )))
    }
    
    /// Set thread register
    pub fn set_thread_register_internal(&mut self, thread: ThreadHandle, index: usize, value: &Value) -> LuaResult<()> {
        let thread = self.get_thread_mut(thread)?;
        
        // Grow stack if needed
        if index >= thread.stack.len() {
            thread.stack.resize(index + 1, Value::Nil);
        }
        
        thread.stack[index] = value.clone();
        Ok(())
    }
    
    // Closure operations
    
    /// Create a closure with validation
    pub fn create_closure_with_validation(&mut self, closure: Closure, validated_handles: &[ClosureHandle]) -> LuaResult<ClosureHandle> {
        // Re-validate handles after potential reallocation
        for handle in validated_handles {
            self.validate_closure_handle(*handle)?;
        }
        
        let handle = ClosureHandle::from(self.closures.insert(closure));
        Ok(handle)
    }
    
    /// Validate a closure handle
    pub fn validate_closure_handle(&self, handle: ClosureHandle) -> LuaResult<()> {
        if !self.closures.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        Ok(())
    }
    
    /// Get a closure by handle
    pub fn get_closure(&self, handle: ClosureHandle) -> LuaResult<&Closure> {
        self.closures.get(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    // Upvalue operations
    
    /// Create an upvalue
    pub fn create_upvalue_internal(&mut self, upvalue: Upvalue) -> LuaResult<UpvalueHandle> {
        let handle = UpvalueHandle::from(self.upvalues.insert(upvalue));
        Ok(handle)
    }
    
    /// Validate an upvalue handle
    pub fn validate_upvalue_handle(&self, handle: UpvalueHandle) -> LuaResult<()> {
        if !self.upvalues.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        Ok(())
    }
    
    /// Get an upvalue by handle
    pub fn get_upvalue(&self, handle: UpvalueHandle) -> LuaResult<&Upvalue> {
        self.upvalues.get(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    /// Get a mutable upvalue by handle
    pub fn get_upvalue_mut(&mut self, handle: UpvalueHandle) -> LuaResult<&mut Upvalue> {
        self.upvalues.get_mut(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    // UserData operations
    
    /// Create userdata
    pub fn create_userdata_internal(&mut self, userdata: UserData) -> LuaResult<UserDataHandle> {
        let handle = UserDataHandle::from(self.userdata.insert(userdata));
        Ok(handle)
    }
    
    /// Validate a userdata handle
    pub fn validate_userdata_handle(&self, handle: UserDataHandle) -> LuaResult<()> {
        if !self.userdata.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        Ok(())
    }
    
    /// Get userdata by handle
    pub fn get_userdata(&self, handle: UserDataHandle) -> LuaResult<&UserData> {
        self.userdata.get(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    // Function prototype operations
    
    /// Create a function prototype with validation
    pub fn create_function_proto_with_validation(&mut self, proto: FunctionProto, validated_handles: &[FunctionProtoHandle]) -> LuaResult<FunctionProtoHandle> {
        // Re-validate handles after potential reallocation
        for handle in validated_handles {
            self.validate_function_proto_handle(*handle)?;
        }
        
        let handle = FunctionProtoHandle::from(self.function_protos.insert(proto));
        Ok(handle)
    }
    
    /// Validate a function prototype handle
    pub fn validate_function_proto_handle(&self, handle: FunctionProtoHandle) -> LuaResult<()> {
        if !self.function_protos.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        Ok(())
    }
    
    /// Get a function prototype by handle
    pub fn get_function_proto(&self, handle: FunctionProtoHandle) -> LuaResult<&FunctionProto> {
        self.function_protos.get(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_heap_creation() {
        let heap = LuaHeap::new().unwrap();
        
        // Verify initial state
        assert!(heap.globals.is_some());
        assert!(heap.registry.is_some());
        assert!(heap.main_thread.is_some());
    }
    
    #[test]
    fn test_string_interning() {
        let mut heap = LuaHeap::new().unwrap();
        
        // Create the same string twice
        let handle1 = heap.create_string_internal("hello").unwrap();
        let handle2 = heap.create_string_internal("hello").unwrap();
        
        // Should get the same handle (interning)
        assert_eq!(handle1, handle2);
        
        // Different strings should get different handles
        let handle3 = heap.create_string_internal("world").unwrap();
        assert_ne!(handle1, handle3);
    }
    
    #[test]
    fn test_table_operations() {
        let mut heap = LuaHeap::new().unwrap();
        
        // Create a table
        let table = heap.create_table_internal().unwrap();
        
        // Set a field
        let key = Value::Number(1.0);
        let value = Value::Boolean(true);
        heap.set_table_field_internal(table, &key, &value).unwrap();
        
        // Get the field
        let retrieved = heap.get_table_field_internal(table, &key).unwrap();
        assert_eq!(retrieved, value);
    }
}