//! RefCell-based Lua Heap Management
//! 
//! This module implements a Lua heap using RefCell for interior mutability,
//! eliminating the need for the transaction system while maintaining memory safety.

use super::arena::Arena;
use super::error::{LuaError, LuaResult};
use super::handle::{StringHandle, TableHandle, ClosureHandle, ThreadHandle, 
                    UpvalueHandle, UserDataHandle, FunctionProtoHandle};
use super::resource::ResourceLimits;
use super::value::{Value, HashableValue, OrderedFloat, LuaString, Table, Closure, 
                   Thread, Upvalue, UserData, FunctionProto, CallFrame};
use std::cell::{RefCell, Ref, RefMut};
use std::collections::HashMap;

/// A Lua heap with RefCell-based interior mutability
pub struct RefCellHeap {
    /// Arena for string storage
    strings: RefCell<Arena<LuaString>>,
    
    /// String interning cache for deduplication
    string_cache: RefCell<HashMap<Vec<u8>, StringHandle>>,
    
    /// Arena for table storage
    tables: RefCell<Arena<Table>>,
    
    /// Arena for closure storage
    closures: RefCell<Arena<Closure>>,
    
    /// Arena for thread storage
    threads: RefCell<Arena<Thread>>,
    
    /// Arena for upvalue storage
    upvalues: RefCell<Arena<Upvalue>>,
    
    /// Arena for userdata storage
    userdata: RefCell<Arena<UserData>>,
    
    /// Arena for function prototype storage
    function_protos: RefCell<Arena<FunctionProto>>,
    
    /// Global table handle
    globals: Option<TableHandle>,
    
    /// Registry table handle
    registry: Option<TableHandle>,
    
    /// Main thread handle
    main_thread: Option<ThreadHandle>,
    
    /// Resource limits for this VM instance 
    pub resource_limits: ResourceLimits,
}

impl RefCellHeap {
    /// Create a new heap
    pub fn new() -> LuaResult<Self> {
        // Step 1: Create a temporary heap structure to use for initialization
        let mut temp_heap = RefCellHeap {
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
        
        // Step 2: Pre-intern common strings
        temp_heap.pre_intern_common_strings()?;
        
        // Step 3: Create the initial objects
        let globals_handle = temp_heap.create_table()?;
        let registry_handle = temp_heap.create_table()?;
        let main_thread_handle = temp_heap.create_thread()?;
        
        // Step 4: Now create the final heap with all fields initialized
        let heap = RefCellHeap {
            strings: temp_heap.strings,
            string_cache: temp_heap.string_cache,
            tables: temp_heap.tables,
            closures: temp_heap.closures,
            threads: temp_heap.threads,
            upvalues: temp_heap.upvalues,
            userdata: temp_heap.userdata,
            function_protos: temp_heap.function_protos,
            globals: Some(globals_handle),
            registry: Some(registry_handle),
            main_thread: Some(main_thread_handle),
            resource_limits: temp_heap.resource_limits,
        };
        
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
    
    // String operations
    
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
    
    /// Validate a string handle
    pub fn validate_string_handle(&self, handle: StringHandle) -> LuaResult<()> {
        let strings = self.strings.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow strings arena".to_string()))?;
        
        if !strings.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        Ok(())
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
    
    // Table operations
    
    /// Create a table
    pub fn create_table(&self) -> LuaResult<TableHandle> {
        let mut tables = self.tables.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow tables arena".to_string()))?;
        
        let table = Table::new();
        let handle = TableHandle::from(tables.insert(table));
        Ok(handle)
    }
    
    /// Validate a table handle
    pub fn validate_table_handle(&self, handle: TableHandle) -> LuaResult<()> {
        let tables = self.tables.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow tables arena".to_string()))?;
        
        if !tables.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        Ok(())
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
        let tables = self.tables.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow tables arena".to_string()))?;
        
        let table_obj = tables.get(table.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        // Try array optimization for integer keys
        if let Value::Number(n) = key {
            if n.fract() == 0.0 && *n > 0.0 && *n <= table_obj.array.len() as f64 {
                let idx = *n as usize;
                return Ok(table_obj.array[idx - 1].clone());
            }
        }
        
        // For string keys, we need to get the content hash
        if let Value::String(string_handle) = key {
            // Need to temporarily drop tables borrow to access strings
            drop(tables);
            
            let strings = self.strings.try_borrow()
                .map_err(|_| LuaError::BorrowError("Failed to borrow strings arena".to_string()))?;
            
            let string = strings.get(string_handle.0)
                .ok_or(LuaError::InvalidHandle)?;
            let hashable = HashableValue::String(*string_handle, string.content_hash);
            
            // Re-borrow tables
            drop(strings);
            let tables = self.tables.try_borrow()
                .map_err(|_| LuaError::BorrowError("Failed to re-borrow tables arena".to_string()))?;
            
            let table_obj = tables.get(table.0)
                .ok_or(LuaError::InvalidHandle)?;
            
            return Ok(table_obj.get_by_hashable(&hashable).cloned().unwrap_or(Value::Nil));
        }
        
        // For other hashable keys
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
                    } else if idx <= table_obj.array.len() {
                        table_obj.array[idx - 1] = value.clone();
                    }
                    return Ok(());
                }
            }
        }
        
        // For string keys, we need to get the content hash
        if let Value::String(string_handle) = key {
            let strings = self.strings.try_borrow()
                .map_err(|_| LuaError::BorrowError("Failed to borrow strings arena".to_string()))?;
            
            let string = strings.get(string_handle.0)
                .ok_or(LuaError::InvalidHandle)?;
            let hashable = HashableValue::String(*string_handle, string.content_hash);
            
            // Drop strings borrow before getting mutable table borrow
            drop(strings);
            
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
    
    /// Get table metatable  
    pub fn get_table_metatable(&self, table: TableHandle) -> LuaResult<Option<TableHandle>> {
        let tables = self.tables.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow tables arena".to_string()))?;
        
        let table_obj = tables.get(table.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        Ok(table_obj.metatable)
    }
    
    /// Set table metatable
    pub fn set_table_metatable(&self, table: TableHandle, metatable: Option<TableHandle>) -> LuaResult<()> {
        let mut tables = self.tables.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow tables arena for writing".to_string()))?;
        
        let table_obj = tables.get_mut(table.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        table_obj.metatable = metatable;
        Ok(())
    }
    
    // Thread operations
    
    /// Create a thread
    pub fn create_thread(&self) -> LuaResult<ThreadHandle> {
        let mut threads = self.threads.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena".to_string()))?;
        
        let thread = Thread::new();
        let handle = ThreadHandle::from(threads.insert(thread));
        Ok(handle)
    }
    
    /// Validate a thread handle
    pub fn validate_thread_handle(&self, handle: ThreadHandle) -> LuaResult<()> {
        let threads = self.threads.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena".to_string()))?;
        
        if !threads.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        Ok(())
    }
    
    /// Get a thread by handle
    pub fn get_thread(&self, handle: ThreadHandle) -> LuaResult<Ref<'_, Thread>> {
        let threads = self.threads.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena".to_string()))?;
        
        if !threads.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        
        Ok(Ref::map(threads, |t| t.get(handle.0).unwrap()))
    }
    
    /// Get a mutable thread by handle
    pub fn get_thread_mut(&self, handle: ThreadHandle) -> LuaResult<RefMut<'_, Thread>> {
        let threads = self.threads.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena for writing".to_string()))?;
        
        if !threads.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        
        Ok(RefMut::map(threads, |t| t.get_mut(handle.0).unwrap()))
    }
    
    /// Get thread register - special handling for FOR loop registers
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
    
    /// Set thread register - special handling for FOR loop registers
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
    
    /// Push value to thread stack
    pub fn push_stack(&self, thread: ThreadHandle, value: &Value) -> LuaResult<()> {
        let mut threads = self.threads.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena for writing".to_string()))?;
        
        let thread_obj = threads.get_mut(thread.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        thread_obj.stack.push(value.clone());
        Ok(())
    }
    
    /// Pop values from thread stack
    pub fn pop_stack(&self, thread: ThreadHandle, count: usize) -> LuaResult<()> {
        let mut threads = self.threads.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena for writing".to_string()))?;
        
        let thread_obj = threads.get_mut(thread.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        for _ in 0..count {
            thread_obj.stack.pop();
        }
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
    
    /// Grow thread stack to at least the specified size
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
    
    /// Push call frame
    pub fn push_call_frame(&self, thread: ThreadHandle, frame: CallFrame) -> LuaResult<()> {
        let mut threads = self.threads.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena for writing".to_string()))?;
        
        let thread_obj = threads.get_mut(thread.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        thread_obj.call_frames.push(frame);
        Ok(())
    }
    
    /// Pop call frame
    pub fn pop_call_frame(&self, thread: ThreadHandle) -> LuaResult<CallFrame> {
        let mut threads = self.threads.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena for writing".to_string()))?;
        
        let thread_obj = threads.get_mut(thread.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        thread_obj.call_frames.pop()
            .ok_or(LuaError::RuntimeError("No call frame to pop".to_string()))
    }
    
    /// Get current frame
    pub fn get_current_frame(&self, thread: ThreadHandle) -> LuaResult<CallFrame> {
        let threads = self.threads.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena".to_string()))?;
        
        let thread_obj = threads.get(thread.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        thread_obj.call_frames.last()
            .cloned()
            .ok_or(LuaError::RuntimeError("No active call frame".to_string()))
    }
    
    /// Get current PC
    pub fn get_pc(&self, thread: ThreadHandle) -> LuaResult<usize> {
        let threads = self.threads.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena".to_string()))?;
        
        let thread_obj = threads.get(thread.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        if let Some(frame) = thread_obj.call_frames.last() {
            Ok(frame.pc)
        } else {
            Err(LuaError::RuntimeError("No active call frame".to_string()))
        }
    }
    
    /// Set PC
    pub fn set_pc(&self, thread: ThreadHandle, pc: usize) -> LuaResult<()> {
        let mut threads = self.threads.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena for writing".to_string()))?;
        
        let thread_obj = threads.get_mut(thread.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        if let Some(frame) = thread_obj.call_frames.last_mut() {
            frame.pc = pc;
            Ok(())
        } else {
            Err(LuaError::RuntimeError("No active call frame".to_string()))
        }
    }
    
    /// Increment PC  
    pub fn increment_pc(&self, thread: ThreadHandle) -> LuaResult<()> {
        let mut threads = self.threads.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena for writing".to_string()))?;
        
        let thread_obj = threads.get_mut(thread.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        if let Some(frame) = thread_obj.call_frames.last_mut() {
            frame.pc += 1;
            Ok(())
        } else {
            Err(LuaError::RuntimeError("No active call frame".to_string()))
        }
    }
    
    /// Get call depth
    pub fn get_thread_call_depth(&self, thread: ThreadHandle) -> LuaResult<usize> {
        let threads = self.threads.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena".to_string()))?;
        
        let thread_obj = threads.get(thread.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        Ok(thread_obj.call_frames.len())
    }
    
    // Closure operations
    
    /// Create a closure
    pub fn create_closure(&self, closure: Closure) -> LuaResult<ClosureHandle> {
        // Validate upvalues before creating
        for upvalue in &closure.upvalues {
            self.validate_upvalue_handle(*upvalue)?;
        }
        
        let mut closures = self.closures.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow closures arena".to_string()))?;
        
        let handle = ClosureHandle::from(closures.insert(closure));
        Ok(handle)
    }
    
    /// Validate a closure handle
    pub fn validate_closure_handle(&self, handle: ClosureHandle) -> LuaResult<()> {
        let closures = self.closures.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow closures arena".to_string()))?;
        
        if !closures.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        Ok(())
    }
    
    /// Get a closure by handle
    pub fn get_closure(&self, handle: ClosureHandle) -> LuaResult<Ref<'_, Closure>> {
        let closures = self.closures.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow closures arena".to_string()))?;
        
        if !closures.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        
        Ok(Ref::map(closures, |c| c.get(handle.0).unwrap()))
    }
    
    /// Get instruction from closure
    pub fn get_instruction(&self, closure: ClosureHandle, pc: usize) -> LuaResult<u32> {
        let closures = self.closures.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow closures arena".to_string()))?;
        
        let closure_obj = closures.get(closure.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        closure_obj.proto.bytecode.get(pc)
            .copied()
            .ok_or(LuaError::RuntimeError(format!(
                "PC {} out of bounds (bytecode size: {})",
                pc,
                closure_obj.proto.bytecode.len()
            )))
    }
    
    // Upvalue operations
    
    /// Create an upvalue
    pub fn create_upvalue(&self, upvalue: Upvalue) -> LuaResult<UpvalueHandle> {
        let mut upvalues = self.upvalues.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow upvalues arena".to_string()))?;
        
        let handle = UpvalueHandle::from(upvalues.insert(upvalue));
        Ok(handle)
    }
    
    /// Validate an upvalue handle
    pub fn validate_upvalue_handle(&self, handle: UpvalueHandle) -> LuaResult<()> {
        let upvalues = self.upvalues.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow upvalues arena".to_string()))?;
        
        if !upvalues.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        Ok(())
    }
    
    /// Get an upvalue by handle
    pub fn get_upvalue(&self, handle: UpvalueHandle) -> LuaResult<Ref<'_, Upvalue>> {
        let upvalues = self.upvalues.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow upvalues arena".to_string()))?;
        
        if !upvalues.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        
        Ok(Ref::map(upvalues, |u| u.get(handle.0).unwrap()))
    }
    
    /// Get a mutable upvalue by handle
    pub fn get_upvalue_mut(&self, handle: UpvalueHandle) -> LuaResult<RefMut<'_, Upvalue>> {
        let upvalues = self.upvalues.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow upvalues arena for writing".to_string()))?;
        
        if !upvalues.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        
        Ok(RefMut::map(upvalues, |u| u.get_mut(handle.0).unwrap()))
    }
    
    /// Get upvalue value (handles open/closed upvalues)
    pub fn get_upvalue_value(&self, upvalue: UpvalueHandle, thread: ThreadHandle) -> LuaResult<Value> {
        let upvalues = self.upvalues.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow upvalues arena".to_string()))?;
        
        let upvalue_obj = upvalues.get(upvalue.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        if let Some(stack_index) = upvalue_obj.stack_index {
            // Open upvalue - get from thread stack
            drop(upvalues); // Release borrow
            self.get_thread_register(thread, stack_index)
        } else {
            // Closed upvalue - get stored value
            Ok(upvalue_obj.value.clone().unwrap_or(Value::Nil))
        }
    }
    
    /// Set upvalue value (handles open/closed upvalues)
    pub fn set_upvalue_value(&self, upvalue: UpvalueHandle, value: &Value, thread: ThreadHandle) -> LuaResult<()> {
        // First check if it's open or closed
        let stack_index = {
            let upvalues = self.upvalues.try_borrow()
                .map_err(|_| LuaError::BorrowError("Failed to borrow upvalues arena".to_string()))?;
            
            let upvalue_obj = upvalues.get(upvalue.0)
                .ok_or(LuaError::InvalidHandle)?;
            
            upvalue_obj.stack_index
        };
        
        if let Some(stack_index) = stack_index {
            // Open upvalue - set in thread stack
            self.set_thread_register(thread, stack_index, value)
        } else {
            // Closed upvalue - set stored value
            let mut upvalues = self.upvalues.try_borrow_mut()
                .map_err(|_| LuaError::BorrowError("Failed to borrow upvalues arena for writing".to_string()))?;
            
            let upvalue_obj = upvalues.get_mut(upvalue.0)
                .ok_or(LuaError::InvalidHandle)?;
            
            upvalue_obj.value = Some(value.clone());
            Ok(())
        }
    }
    
    /// Close an upvalue
    pub fn close_upvalue(&self, upvalue: UpvalueHandle, value: &Value) -> LuaResult<()> {
        let mut upvalues = self.upvalues.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow upvalues arena for writing".to_string()))?;
        
        let upvalue_obj = upvalues.get_mut(upvalue.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        upvalue_obj.stack_index = None;
        upvalue_obj.value = Some(value.clone());
        Ok(())
    }
    
    /// Find or create an upvalue for a given stack index
    pub fn find_or_create_upvalue(&self, thread: ThreadHandle, stack_index: usize) -> LuaResult<UpvalueHandle> {
        // Phase 1: Check stack bounds and get open upvalues
        let open_upvalues = {
            let threads = self.threads.try_borrow()
                .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena".to_string()))?;
            
            let thread_obj = threads.get(thread.0)
                .ok_or(LuaError::InvalidHandle)?;
            
            if stack_index >= thread_obj.stack.len() {
                return Err(LuaError::RuntimeError(format!(
                    "Cannot create upvalue for stack index {} (stack size: {})",
                    stack_index, thread_obj.stack.len()
                )));
            }
            
            thread_obj.open_upvalues.clone()
        };
        
        // Phase 2: Search for existing upvalue
        for &upvalue_handle in &open_upvalues {
            let upvalues = self.upvalues.try_borrow()
                .map_err(|_| LuaError::BorrowError("Failed to borrow upvalues arena".to_string()))?;
            
            if let Some(upvalue) = upvalues.get(upvalue_handle.0) {
                if let Some(idx) = upvalue.stack_index {
                    if idx == stack_index {
                        return Ok(upvalue_handle);
                    }
                }
            }
        }
        
        // Phase 3: Create new upvalue
        let new_upvalue = Upvalue {
            stack_index: Some(stack_index),
            value: None,
        };
        
        let upvalue_handle = self.create_upvalue(new_upvalue)?;
        
        // Phase 4: Add to thread's open upvalues list
        let mut threads = self.threads.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena for writing".to_string()))?;
        
        let thread_obj = threads.get_mut(thread.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        // Find insertion position (sorted by stack index, highest first)
        let insert_pos = thread_obj.open_upvalues.iter()
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
        
        thread_obj.open_upvalues.insert(insert_pos, upvalue_handle);
        
        Ok(upvalue_handle)
    }
    
    /// Close all upvalues at or above the given stack index
    pub fn close_thread_upvalues(&self, thread: ThreadHandle, threshold: usize) -> LuaResult<()> {
        // Phase 1: Collect upvalues to close
        let (to_close, keep_open) = {
            let threads = self.threads.try_borrow()
                .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena".to_string()))?;
            
            let thread_obj = threads.get(thread.0)
                .ok_or(LuaError::InvalidHandle)?;
            
            let mut to_close = Vec::new();
            let mut keep_open = Vec::new();
            
            for &upvalue_handle in &thread_obj.open_upvalues {
                let upvalues = self.upvalues.try_borrow()
                    .map_err(|_| LuaError::BorrowError("Failed to borrow upvalues arena".to_string()))?;
                
                if let Some(upvalue) = upvalues.get(upvalue_handle.0) {
                    if let Some(idx) = upvalue.stack_index {
                        if idx >= threshold {
                            // Get the value from stack
                            drop(upvalues);
                            let value = thread_obj.stack.get(idx).cloned().unwrap_or(Value::Nil);
                            to_close.push((upvalue_handle, value));
                        } else {
                            keep_open.push(upvalue_handle);
                        }
                    }
                }
            }
            
            (to_close, keep_open)
        };
        
        // Phase 2: Close the upvalues
        for (upvalue_handle, value) in to_close {
            self.close_upvalue(upvalue_handle, &value)?;
        }
        
        // Phase 3: Update thread's open upvalues list
        let mut threads = self.threads.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow threads arena for writing".to_string()))?;
        
        let thread_obj = threads.get_mut(thread.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        thread_obj.open_upvalues = keep_open;
        
        Ok(())
    }
    
    // UserData operations
    
    /// Create userdata
    pub fn create_userdata(&self, userdata: UserData) -> LuaResult<UserDataHandle> {
        // Validate metatable if present
        if let Some(mt) = userdata.metatable {
            self.validate_table_handle(mt)?;
        }
        
        let mut userdata_arena = self.userdata.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow userdata arena".to_string()))?;
        
        let handle = UserDataHandle::from(userdata_arena.insert(userdata));
        Ok(handle)
    }
    
    /// Validate a userdata handle
    pub fn validate_userdata_handle(&self, handle: UserDataHandle) -> LuaResult<()> {
        let userdata = self.userdata.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow userdata arena".to_string()))?;
        
        if !userdata.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        Ok(())
    }
    
    /// Get userdata by handle
    pub fn get_userdata(&self, handle: UserDataHandle) -> LuaResult<Ref<'_, UserData>> {
        let userdata = self.userdata.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow userdata arena".to_string()))?;
        
        if !userdata.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        
        Ok(Ref::map(userdata, |u| u.get(handle.0).unwrap()))
    }
    
    /// Get userdata metatable
    pub fn get_userdata_metatable(&self, handle: UserDataHandle) -> LuaResult<Option<TableHandle>> {
        let userdata = self.userdata.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow userdata arena".to_string()))?;
        
        let userdata_obj = userdata.get(handle.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        Ok(userdata_obj.metatable)
    }
    
    // Function prototype operations
    
    /// Create a function prototype
    pub fn create_function_proto(&self, proto: FunctionProto) -> LuaResult<FunctionProtoHandle> {
        let mut function_protos = self.function_protos.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow function protos arena".to_string()))?;
        
        let handle = FunctionProtoHandle::from(function_protos.insert(proto));
        Ok(handle)
    }
    
    /// Validate a function prototype handle
    pub fn validate_function_proto_handle(&self, handle: FunctionProtoHandle) -> LuaResult<()> {
        let function_protos = self.function_protos.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow function protos arena".to_string()))?;
        
        if !function_protos.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        Ok(())
    }
    
    /// Get a function prototype by handle
    pub fn get_function_proto(&self, handle: FunctionProtoHandle) -> LuaResult<Ref<'_, FunctionProto>> {
        let function_protos = self.function_protos.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow function protos arena".to_string()))?;
        
        if !function_protos.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        
        Ok(Ref::map(function_protos, |f| f.get(handle.0).unwrap()))
    }
    
    /// Get function prototype copy
    pub fn get_function_proto_copy(&self, handle: FunctionProtoHandle) -> LuaResult<FunctionProto> {
        let function_protos = self.function_protos.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow function protos arena".to_string()))?;
        
        let proto = function_protos.get(handle.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        Ok(proto.clone())
    }
    
    /// Replace a function prototype in place
    pub fn replace_function_proto(&self, handle: FunctionProtoHandle, proto: FunctionProto) -> LuaResult<()> {
        let mut function_protos = self.function_protos.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow function protos arena for writing".to_string()))?;
        
        // Ensure the handle is valid
        if !function_protos.contains(handle.0) {
            return Err(LuaError::InvalidHandle);
        }
        
        // Replace the prototype
        if let Some(proto_slot) = function_protos.get_mut(handle.0) {
            *proto_slot = proto;
            Ok(())
        } else {
            Err(LuaError::InvalidHandle)
        }
    }
    
    // Helper methods for common patterns
    
    /// Get table field with metamethod support
    pub fn get_table_with_metamethods(&self, table: TableHandle, key: &Value) -> LuaResult<Value> {
        // Try direct table access first
        let direct_result = self.get_table_field(table, key)?;
        
        if !direct_result.is_nil() {
            return Ok(direct_result);
        }
        
        // Check for __index metamethod
        let metatable_opt = self.get_table_metatable(table)?;
        
        let Some(metatable) = metatable_opt else {
            return Ok(Value::Nil);
        };
        
        // Look up __index
        let index_key = self.create_string("__index")?;
        let metamethod = self.get_table_field(metatable, &Value::String(index_key))?;
        
        match metamethod {
            Value::Nil => Ok(Value::Nil),
            Value::Table(mm_table) => {
                // __index is a table, access it directly
                self.get_table_field(mm_table, key)
            },
            Value::Closure(_) => {
                // __index is a function - would need to be called by VM
                // For now, just return nil
                Ok(Value::Nil)
            },
            _ => Ok(Value::Nil),
        }
    }
    
    /// Get next key-value pair for table iteration
    pub fn table_next(&self, table: TableHandle, current_key: Value) -> LuaResult<Option<(Value, Value)>> {
        let tables = self.tables.try_borrow()
            .map_err(|_| LuaError::BorrowError("Failed to borrow tables arena".to_string()))?;
        
        let table_obj = tables.get(table.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        // Case 1: nil key means get the first key/value pair
        if current_key.is_nil() {
            // First try array part
            if !table_obj.array.is_empty() && !table_obj.array[0].is_nil() {
                return Ok(Some((Value::Number(1.0), table_obj.array[0].clone())));
            }
            
            // Check rest of array
            for i in 1..table_obj.array.len() {
                if !table_obj.array[i].is_nil() {
                    return Ok(Some((Value::Number((i + 1) as f64), table_obj.array[i].clone())));
                }
            }
            
            // Try hash part
            if let Some((k, v)) = table_obj.map.iter().next() {
                return Ok(Some((k.to_value(), v.clone())));
            }
            
            return Ok(None);
        }
        
        // Case 2: Current key is numeric
        if let Value::Number(n) = &current_key {
            if n.fract() == 0.0 && *n >= 1.0 {
                let idx = *n as usize;
                
                // Look for next in array
                if idx <= table_obj.array.len() {
                    for i in idx..table_obj.array.len() {
                        if !table_obj.array[i].is_nil() {
                            return Ok(Some((Value::Number((i + 1) as f64), table_obj.array[i].clone())));
                        }
                    }
                    
                    // Move to hash part
                    if let Some((k, v)) = table_obj.map.iter().next() {
                        return Ok(Some((k.to_value(), v.clone())));
                    }
                }
                
                return Ok(None);
            }
        }
        
        // Case 3: Current key is in hash part
        let current_hashable = match HashableValue::from_value_with_context(&current_key, "table_next") {
            Ok(k) => k,
            Err(_) => return Ok(None),
        };
        
        // Find current key and return next
        let mut found = false;
        for (k, v) in &table_obj.map {
            if found {
                return Ok(Some((k.to_value(), v.clone())));
            }
            if k == &current_hashable {
                found = true;
            }
        }
        
        Ok(None)
    }
    
    /// Optimized method for setting array elements
    pub fn set_table_array_element(&self, table: TableHandle, index: usize, value: &Value) -> LuaResult<()> {
        let mut tables = self.tables.try_borrow_mut()
            .map_err(|_| LuaError::BorrowError("Failed to borrow tables arena for writing".to_string()))?;
        
        let table_obj = tables.get_mut(table.0)
            .ok_or(LuaError::InvalidHandle)?;
        
        if index > 0 {
            // For array indices
            if index <= table_obj.array.len() {
                table_obj.array[index - 1] = value.clone();
            } else if index == table_obj.array.len() + 1 {
                table_obj.array.push(value.clone());
            } else {
                // Fill gaps with nil
                while table_obj.array.len() < index - 1 {
                    table_obj.array.push(Value::Nil);
                }
                table_obj.array.push(value.clone());
            }
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_heap_creation() {
        let heap = RefCellHeap::new().unwrap();
        
        // Verify initial state
        assert!(heap.globals().is_ok());
        assert!(heap.registry().is_ok());
        assert!(heap.main_thread().is_ok());
    }
    
    #[test]
    fn test_string_interning() {
        let heap = RefCellHeap::new().unwrap();
        
        // Create the same string twice
        let handle1 = heap.create_string("hello").unwrap();
        let handle2 = heap.create_string("hello").unwrap();
        
        // Should get the same handle (interning)
        assert_eq!(handle1, handle2);
        
        // Different strings should get different handles
        let handle3 = heap.create_string("world").unwrap();
        assert_ne!(handle1, handle3);
    }
    
    #[test]
    fn test_table_operations() {
        let heap = RefCellHeap::new().unwrap();
        
        // Create a table
        let table = heap.create_table().unwrap();
        
        // Set a field
        let key = Value::Number(1.0);
        let value = Value::Boolean(true);
        heap.set_table_field(table, &key, &value).unwrap();
        
        // Get the field
        let retrieved = heap.get_table_field(table, &key).unwrap();
        assert_eq!(retrieved, value);
    }
    
    #[test]
    fn test_thread_operations() {
        let heap = RefCellHeap::new().unwrap();
        
        let thread = heap.main_thread().unwrap();
        
        // Set a register
        heap.set_thread_register(thread, 0, &Value::Number(42.0)).unwrap();
        
        // Get the register
        let value = heap.get_thread_register(thread, 0).unwrap();
        assert_eq!(value, Value::Number(42.0));
        
        // Push to stack
        heap.push_stack(thread, &Value::String(heap.create_string("test").unwrap())).unwrap();
        
        // Check stack size
        let size = heap.get_stack_size(thread).unwrap();
        assert!(size > 0);
    }
    
    #[test]
    fn test_refcell_borrow_patterns() {
        let heap = RefCellHeap::new().unwrap();
        
        // Test that we can't hold multiple mutable borrows
        let table = heap.create_table().unwrap();
        
        // This should work - borrows are released properly
        heap.set_table_field(table, &Value::Number(1.0), &Value::Boolean(true)).unwrap();
        heap.set_table_field(table, &Value::Number(2.0), &Value::Boolean(false)).unwrap();
        
        // Test string operations don't conflict with table operations
        let key = heap.create_string("key").unwrap();
        heap.set_table_field(table, &Value::String(key), &Value::Number(3.0)).unwrap();
    }
    
    #[test]
    fn test_upvalue_operations() {
        let heap = RefCellHeap::new().unwrap();
        
        let thread = heap.main_thread().unwrap();
        
        // Set up stack
        heap.set_thread_register(thread, 0, &Value::Number(10.0)).unwrap();
        heap.set_thread_register(thread, 1, &Value::Number(20.0)).unwrap();
        
        // Create upvalue for stack index 0
        let upvalue = heap.find_or_create_upvalue(thread, 0).unwrap();
        
        // Get upvalue value (should be from stack)
        let value = heap.get_upvalue_value(upvalue, thread).unwrap();
        assert_eq!(value, Value::Number(10.0));
        
        // Close upvalue
        heap.close_upvalue(upvalue, &Value::Number(10.0)).unwrap();
        
        // Get value again (should be stored value now)
        let value = heap.get_upvalue_value(upvalue, thread).unwrap();
        assert_eq!(value, Value::Number(10.0));
    }
}