//! Lua Heap Implementation
//! 
//! The heap manages all Lua objects and provides transaction-based access
//! to ensure memory safety and consistency.

use super::arena::{Arena, Handle};
use super::error::{LuaError, LuaResult};
use super::handle::{StringHandle, TableHandle, ClosureHandle, ThreadHandle, 
                    UpvalueHandle, UserDataHandle, FunctionProtoHandle};
use super::value::{LuaString, Table, Closure, Thread, Upvalue, UserData, FunctionProto, Value};
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
}

impl LuaHeap {
    /// Create a new empty heap
    pub fn new() -> LuaResult<Self> {
        let mut heap = LuaHeap {
            strings: Arena::with_capacity(256),
            string_cache: HashMap::with_capacity(256),
            tables: Arena::with_capacity(64),
            closures: Arena::with_capacity(32),
            threads: Arena::with_capacity(4),
            upvalues: Arena::with_capacity(32),
            userdata: Arena::with_capacity(16),
            function_protos: Arena::with_capacity(32),
            globals: None,
            registry: None,
            main_thread: None,
            generation: 0,
        };
        
        // Initialize core structures
        heap.initialize()?;
        
        Ok(heap)
    }
    
    /// Initialize core heap structures
    fn initialize(&mut self) -> LuaResult<()> {
        // Create globals table
        let globals_handle = self.create_table_internal()?;
        self.globals = Some(globals_handle);
        
        // Create registry table
        let registry_handle = self.create_table_internal()?;
        self.registry = Some(registry_handle);
        
        // Create main thread
        let main_thread_handle = self.create_thread_internal()?;
        
        // Initialize main thread with reasonable stack space
        self.initialize_thread_stack(main_thread_handle, 256)?;
        
        self.main_thread = Some(main_thread_handle);
        
        // Increment generation after initialization
        self.generation = self.generation.wrapping_add(1);
        
        Ok(())
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
    
    /// Get the main thread
    pub fn main_thread(&self) -> LuaResult<ThreadHandle> {
        self.main_thread.ok_or(LuaError::InternalError(
            "Main thread not initialized".to_string()
        ))
    }
    
    /// Check if adding a new item might require reallocation
    pub fn might_reallocate<T>(&self, arena: &Arena<T>) -> bool {
        // Use the public method from Arena to check if reallocation might occur
        arena.might_reallocate_on_insert()
    }

    /// Validate all handles before a reallocation operation
    pub fn validate_before_reallocation<T: 'static, F>(&self, handles: &[Handle<T>], mut validator: F) -> LuaResult<()>
    where
        F: FnMut(&Self, &Handle<T>) -> LuaResult<()>
    {
        // Check each handle
        for handle in handles {
            validator(self, handle)?;
        }
        
        Ok(())
    }

    // Internal creation methods (used during initialization and transactions)
    
    pub(crate) fn create_string_internal(&mut self, s: &str) -> LuaResult<StringHandle> {
        let bytes = s.as_bytes().to_vec();
        
        // Check string cache first
        if let Some(&handle) = self.string_cache.get(&bytes) {
            // Validate that the cached handle is still valid
            if self.strings.contains(handle.0) {
                return Ok(handle);
            } else {
                // Remove stale cache entry
                self.string_cache.remove(&bytes);
            }
        }
        
        // Create new string
        let lua_string = LuaString::from_bytes(bytes.clone());
        let handle = StringHandle::from(self.strings.insert(lua_string));
        
        // Add to cache
        self.string_cache.insert(bytes, handle);
        
        Ok(handle)
    }
    
    pub(crate) fn create_table_internal(&mut self) -> LuaResult<TableHandle> {
        let table = Table::new();
        let handle = TableHandle::from(self.tables.insert(table));
        Ok(handle)
    }
    
    pub(crate) fn create_closure_internal(&mut self, closure: Closure) -> LuaResult<ClosureHandle> {
        let handle = ClosureHandle::from(self.closures.insert(closure));
        Ok(handle)
    }
    
    pub(crate) fn create_thread_internal(&mut self) -> LuaResult<ThreadHandle> {
        let thread = Thread::new();
        let handle = ThreadHandle::from(self.threads.insert(thread));
        Ok(handle)
    }
    
    pub(crate) fn create_upvalue_internal(&mut self, upvalue: Upvalue) -> LuaResult<UpvalueHandle> {
        let handle = UpvalueHandle::from(self.upvalues.insert(upvalue));
        Ok(handle)
    }
    
    pub(crate) fn create_userdata_internal(&mut self, userdata: UserData) -> LuaResult<UserDataHandle> {
        let handle = UserDataHandle::from(self.userdata.insert(userdata));
        Ok(handle)
    }

    /// Create a string with validation before potential reallocation
    pub fn create_string_with_validation(&mut self, s: &str, validated_handles: &[StringHandle]) -> LuaResult<StringHandle> {
        // Check if we might need to reallocate
        if self.strings.might_reallocate_on_insert() {
            // Validate all handles before reallocation
            for handle in validated_handles {
                self.validate_string_handle(*handle)?;
            }
        }
        
        // Now it's safe to create the string
        self.create_string_internal(s)
    }

    /// Create a table with validation before potential reallocation
    pub fn create_table_with_validation(&mut self, validated_handles: &[TableHandle]) -> LuaResult<TableHandle> {
        // Check if we might need to reallocate
        if self.tables.might_reallocate_on_insert() {
            // Validate all handles before reallocation
            for handle in validated_handles {
                self.validate_table_handle(*handle)?;
            }
        }
        
        // Now it's safe to create the table
        self.create_table_internal()
    }

    /// Create a closure with validation before potential reallocation
    pub fn create_closure_with_validation(&mut self, closure: Closure, validated_handles: &[ClosureHandle]) -> LuaResult<ClosureHandle> {
        // Check if we might need to reallocate
        if self.closures.might_reallocate_on_insert() {
            // Validate all handles before reallocation
            for handle in validated_handles {
                self.validate_closure_handle(*handle)?;
            }
        }
        
        // Now it's safe to create the closure
        self.create_closure_internal(closure)
    }
    
    // Internal getter methods
    
    pub(crate) fn get_string(&self, handle: StringHandle) -> LuaResult<&LuaString> {
        self.strings.get(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    /// Get the string value from a handle
    pub fn get_string_value(&self, handle: StringHandle) -> LuaResult<String> {
        self.strings.get(handle.0)
            .map(|s| String::from_utf8_lossy(&s.bytes).to_string())
            .ok_or(LuaError::InvalidHandle)
    }
    
    pub(crate) fn get_table(&self, handle: TableHandle) -> LuaResult<&Table> {
        self.tables.get(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    pub(crate) fn get_table_mut(&mut self, handle: TableHandle) -> LuaResult<&mut Table> {
        self.tables.get_mut(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    pub(crate) fn get_closure(&self, handle: ClosureHandle) -> LuaResult<&Closure> {
        self.closures.get(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    pub(crate) fn get_thread(&self, handle: ThreadHandle) -> LuaResult<&Thread> {
        self.threads.get(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    pub(crate) fn get_thread_mut(&mut self, handle: ThreadHandle) -> LuaResult<&mut Thread> {
        self.threads.get_mut(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    pub(crate) fn get_upvalue(&self, handle: UpvalueHandle) -> LuaResult<&Upvalue> {
        self.upvalues.get(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    pub(crate) fn get_upvalue_mut(&mut self, handle: UpvalueHandle) -> LuaResult<&mut Upvalue> {
        self.upvalues.get_mut(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    pub(crate) fn get_userdata(&self, handle: UserDataHandle) -> LuaResult<&UserData> {
        self.userdata.get(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    pub(crate) fn get_userdata_mut(&mut self, handle: UserDataHandle) -> LuaResult<&mut UserData> {
        self.userdata.get_mut(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    // Internal field access methods
    
    pub(crate) fn get_table_field_internal(&self, table: TableHandle, key: &Value) -> LuaResult<Value> {
        let table_obj = self.get_table(table)?;
        
        if let Some(value) = table_obj.get_field(key) {
            Ok(value.clone())
        } else {
            Ok(Value::Nil)
        }
    }
    
    pub(crate) fn set_table_field_internal(&mut self, table: TableHandle, key: Value, value: Value) -> LuaResult<()> {
        let table_obj = self.get_table_mut(table)?;
        table_obj.set_field(key, value)?;
        Ok(())
    }
    
    pub(crate) fn get_table_metatable_internal(&self, table: TableHandle) -> LuaResult<Option<TableHandle>> {
        let table_obj = self.get_table(table)?;
        Ok(table_obj.metatable())
    }
    
    pub(crate) fn set_table_metatable_internal(&mut self, table: TableHandle, metatable: Option<TableHandle>) -> LuaResult<()> {
        let table_obj = self.get_table_mut(table)?;
        table_obj.set_metatable(metatable);
        Ok(())
    }
    
    // Thread register access
    
    pub(crate) fn get_thread_register_internal(&self, thread: ThreadHandle, index: usize) -> LuaResult<Value> {
        let thread_obj = self.get_thread(thread)?;
        
        println!("DEBUG: get_register - index: {}, stack size: {}", 
                 index, thread_obj.stack.len());
        
        // Check bounds
        if index >= thread_obj.stack.len() {
            // Out of bounds access - return an error with detailed information
            return Err(LuaError::RuntimeError(format!(
                "Stack index {} out of bounds (stack size: {})",
                index,
                thread_obj.stack.len()
            )));
        }
        
        // Clone value to avoid borrowing issues
        Ok(thread_obj.stack[index].clone())
    }
    
    pub(crate) fn set_thread_register_internal(&mut self, thread: ThreadHandle, index: usize, value: Value) -> LuaResult<()> {
        let thread_obj = self.get_thread_mut(thread)?;
        
        // Print debug info for stack operations
        println!("DEBUG: set_register - index: {}, stack size: {}, value: {:?}", 
                 index, thread_obj.stack.len(), value);
        
        // If index is out of bounds, grow the stack
        if index >= thread_obj.stack.len() {
            println!("DEBUG: Growing stack from {} to {}", thread_obj.stack.len(), index + 1);
            
            // Need to ensure stack.len() becomes at least (index + 1)
            let additional_needed = index + 1 - thread_obj.stack.len();
            thread_obj.stack.reserve(additional_needed);
            
            // Add Nil values up to but not including index
            for i in 0..additional_needed-1 {
                println!("DEBUG: Pushing Nil at position {}", thread_obj.stack.len());
                thread_obj.stack.push(Value::Nil);
            }
            
            // Add the target value at index
            println!("DEBUG: Pushing value at position {}", thread_obj.stack.len());
            thread_obj.stack.push(value);
            println!("DEBUG: After push, stack size: {}", thread_obj.stack.len());
            
            return Ok(());
        }
        
        // Otherwise, we can just set the value directly
        thread_obj.stack[index] = value;
        Ok(())
    }
    
    /// Initialize a thread's stack with a specific size
    /// This ensures the stack has enough space for a function's registers
    pub(crate) fn initialize_thread_stack(&mut self, thread: ThreadHandle, size: usize) -> LuaResult<()> {
        let thread_obj = self.get_thread_mut(thread)?;
        
        let current_size = thread_obj.stack.len();
        
        if current_size < size {
            thread_obj.stack.reserve(size - current_size);
            for _ in current_size..size {
                thread_obj.stack.push(Value::Nil);
            }
        }
        
        Ok(())
    }
    
    // Validation methods
    
    pub fn validate_string_handle(&self, handle: StringHandle) -> LuaResult<()> {
        self.strings.validate_handle(&handle.0)
    }
    
    pub fn validate_table_handle(&self, handle: TableHandle) -> LuaResult<()> {
        self.tables.validate_handle(&handle.0)
    }
    
    pub fn validate_closure_handle(&self, handle: ClosureHandle) -> LuaResult<()> {
        self.closures.validate_handle(&handle.0)
    }
    
    pub fn validate_thread_handle(&self, handle: ThreadHandle) -> LuaResult<()> {
        self.threads.validate_handle(&handle.0)
    }
    
    pub fn validate_upvalue_handle(&self, handle: UpvalueHandle) -> LuaResult<()> {
        self.upvalues.validate_handle(&handle.0)
    }
    
    pub fn validate_userdata_handle(&self, handle: UserDataHandle) -> LuaResult<()> {
        self.userdata.validate_handle(&handle.0)
    }

    /// Check if a string index is valid
    pub fn is_valid_string_index(&self, index: u32) -> bool {
        index < self.strings.len() as u32
    }

    /// Check if a string generation is valid
    pub fn is_valid_string_generation(&self, index: u32, generation: u32) -> bool {
        if !self.is_valid_string_index(index) {
            return false;
        }
        
        // Get generation from arena
        if let Some(stored_gen) = self.strings.get_generation(index) {
            stored_gen == generation
        } else {
            false
        }
    }

    /// Check if a table index is valid
    pub fn is_valid_table_index(&self, index: u32) -> bool {
        index < self.tables.len() as u32
    }

    /// Check if a table generation is valid
    pub fn is_valid_table_generation(&self, index: u32, generation: u32) -> bool {
        if !self.is_valid_table_index(index) {
            return false;
        }
        
        if let Some(stored_gen) = self.tables.get_generation(index) {
            stored_gen == generation
        } else {
            false
        }
    }

    /// Check if a closure index is valid
    pub fn is_valid_closure_index(&self, index: u32) -> bool {
        index < self.closures.len() as u32
    }

    /// Check if a closure generation is valid
    pub fn is_valid_closure_generation(&self, index: u32, generation: u32) -> bool {
        if !self.is_valid_closure_index(index) {
            return false;
        }
        
        if let Some(stored_gen) = self.closures.get_generation(index) {
            stored_gen == generation
        } else {
            false
        }
    }

    /// Check if a thread index is valid
    pub fn is_valid_thread_index(&self, index: u32) -> bool {
        index < self.threads.len() as u32
    }

    /// Check if a thread generation is valid
    pub fn is_valid_thread_generation(&self, index: u32, generation: u32) -> bool {
        if !self.is_valid_thread_index(index) {
            return false;
        }
        
        if let Some(stored_gen) = self.threads.get_generation(index) {
            stored_gen == generation
        } else {
            false
        }
    }

    /// Check if an upvalue index is valid
    pub fn is_valid_upvalue_index(&self, index: u32) -> bool {
        index < self.upvalues.len() as u32
    }

    /// Check if an upvalue generation is valid
    pub fn is_valid_upvalue_generation(&self, index: u32, generation: u32) -> bool {
        if !self.is_valid_upvalue_index(index) {
            return false;
        }
        
        if let Some(stored_gen) = self.upvalues.get_generation(index) {
            stored_gen == generation
        } else {
            false
        }
    }

    /// Check if a userdata index is valid
    pub fn is_valid_userdata_index(&self, index: u32) -> bool {
        index < self.userdata.len() as u32
    }

    /// Check if a userdata generation is valid
    pub fn is_valid_userdata_generation(&self, index: u32, generation: u32) -> bool {
        if !self.is_valid_userdata_index(index) {
            return false;
        }
        
        if let Some(stored_gen) = self.userdata.get_generation(index) {
            stored_gen == generation
        } else {
            false
        }
    }
    
    pub(crate) fn create_function_proto_internal(&mut self, proto: FunctionProto) -> LuaResult<FunctionProtoHandle> {
        let handle = FunctionProtoHandle::from(self.function_protos.insert(proto));
        Ok(handle)
    }
    
    /// Get a function prototype reference
    pub(crate) fn get_function_proto(&self, handle: FunctionProtoHandle) -> LuaResult<&FunctionProto> {
        self.function_protos.get(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    /// Get a mutable function prototype reference
    pub(crate) fn get_function_proto_mut(&mut self, handle: FunctionProtoHandle) -> LuaResult<&mut FunctionProto> {
        self.function_protos.get_mut(handle.0)
            .ok_or(LuaError::InvalidHandle)
    }
    
    /// Create a function prototype with validation before potential reallocation
    pub fn create_function_proto_with_validation(&mut self, proto: FunctionProto, validated_handles: &[FunctionProtoHandle]) -> LuaResult<FunctionProtoHandle> {
        // Check if we might need to reallocate
        if self.function_protos.might_reallocate_on_insert() {
            // Validate all handles before reallocation
            for handle in validated_handles {
                self.validate_function_proto_handle(*handle)?;
            }
        }
        
        // Now it's safe to create the prototype
        self.create_function_proto_internal(proto)
    }
    
    /// Validate a function prototype handle
    pub fn validate_function_proto_handle(&self, handle: FunctionProtoHandle) -> LuaResult<()> {
        self.function_protos.validate_handle(&handle.0)
    }
    
    /// Check if a function prototype index is valid
    pub fn is_valid_function_proto_index(&self, index: u32) -> bool {
        index < self.function_protos.len() as u32
    }
    
    /// Check if a function prototype generation is valid
    pub fn is_valid_function_proto_generation(&self, index: u32, generation: u32) -> bool {
        if !self.is_valid_function_proto_index(index) {
            return false;
        }
        
        if let Some(stored_gen) = self.function_protos.get_generation(index) {
            stored_gen == generation
        } else {
            false
        }
    }
}

impl Default for LuaHeap {
    fn default() -> Self {
        Self::new().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::marker::PhantomData;
    use crate::lua::arena::Handle;
    
    // Helper to create a test handle for testing
    fn create_test_handle<T>(index: u32, generation: u32) -> Handle<T> {
        unsafe {
            std::mem::transmute::<(u32, u32), Handle<T>>((index, generation))
        }
    }
    
    #[test]
    fn test_heap_initialization() {
        let heap = LuaHeap::new().unwrap();
        
        assert!(heap.globals().is_ok());
        assert!(heap.registry().is_ok());
        assert!(heap.main_thread().is_ok());
    }
    
    #[test]
    fn test_string_interning() {
        let mut heap = LuaHeap::new().unwrap();
        
        // Create same string twice
        let handle1 = heap.create_string_internal("hello").unwrap();
        let handle2 = heap.create_string_internal("hello").unwrap();
        
        // Should get same handle (interned)
        assert_eq!(handle1, handle2);
        
        // Different string should get different handle
        let handle3 = heap.create_string_internal("world").unwrap();
        assert_ne!(handle1, handle3);
    }
    
    #[test]
    fn test_table_operations() {
        let mut heap = LuaHeap::new().unwrap();
        
        let table = heap.create_table_internal().unwrap();
        let key = Value::Number(1.0);
        let value = Value::Boolean(true);
        
        // Set field
        heap.set_table_field_internal(table, key.clone(), value.clone()).unwrap();
        
        // Get field
        let retrieved = heap.get_table_field_internal(table, &key).unwrap();
        assert_eq!(retrieved, value);
    }
    
    #[test]
    fn test_handle_validation() {
        let mut heap = LuaHeap::new().unwrap();
        
        let handle = heap.create_string_internal("test").unwrap();
        
        // Should be valid
        assert!(heap.validate_string_handle(handle).is_ok());
        
        // Create invalid handle
        let invalid_handle = StringHandle(create_test_handle::<LuaString>(999, 999));
        
        // Should be invalid
        assert!(heap.validate_string_handle(invalid_handle).is_err());
    }
}