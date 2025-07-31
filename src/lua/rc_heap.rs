//! Rc<RefCell> Based Lua Heap
//!
//! This module implements a Lua heap using Rc<RefCell> for fine-grained interior
//! mutability, allowing multiple objects to be borrowed simultaneously.

use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;
use std::any::TypeId;

use super::error::{LuaError, LuaResult};
use super::rc_value::{
    Value, LuaString, Table, Closure, Thread, UpvalueState, UserData, FunctionProto,
    StringHandle, TableHandle, ClosureHandle, ThreadHandle, UpvalueHandle, UserDataHandle, 
    FunctionProtoHandle, HashableValue, UpvalueInfo, CallFrame, ThreadStatus, Frame
};

/// Pre-interned common strings for metamethods
pub struct MetamethodNames {
    pub index: StringHandle,
    pub newindex: StringHandle,
    pub call: StringHandle,
    pub add: StringHandle,
    pub sub: StringHandle,
    pub mul: StringHandle,
    pub div: StringHandle,
    pub mod_op: StringHandle,
    pub pow: StringHandle,
    pub unm: StringHandle,
    pub concat: StringHandle,
    pub len: StringHandle,
    pub eq: StringHandle,
    pub lt: StringHandle,
    pub le: StringHandle,
}

/// Rc<RefCell> based Lua Heap implementation
pub struct RcHeap {
    /// String interning cache
    string_cache: RefCell<HashMap<Vec<u8>, StringHandle>>,
    
    /// Registry for strong references
    registry: RefCell<Vec<Value>>,
    
    /// Main thread
    main_thread: ThreadHandle,
    
    /// Global environment table
    globals: TableHandle,
    
    /// Registry table
    registry_table: TableHandle,
    
    /// Pre-interned metamethod names
    pub metamethod_names: MetamethodNames,
}

impl RcHeap {
    /// Create a new Lua heap
    pub fn new() -> LuaResult<Self> {
        // Create string cache
        let string_cache = RefCell::new(HashMap::new());
        
        // Create temporary registry for strong references
        let registry = RefCell::new(Vec::new());
        
        // Create initial heap without the required tables and threads
        let mut temp_heap = RcHeap {
            string_cache,
            registry,
            // These will be initialized properly below
            main_thread: Rc::new(RefCell::new(Thread::new())),
            globals: Rc::new(RefCell::new(Table::new())),
            registry_table: Rc::new(RefCell::new(Table::new())),
            // This will be initialized after strings are interned
            metamethod_names: MetamethodNames {
                index: Rc::new(RefCell::new(LuaString::new("__index"))),
                newindex: Rc::new(RefCell::new(LuaString::new("__newindex"))),
                call: Rc::new(RefCell::new(LuaString::new("__call"))),
                add: Rc::new(RefCell::new(LuaString::new("__add"))),
                sub: Rc::new(RefCell::new(LuaString::new("__sub"))),
                mul: Rc::new(RefCell::new(LuaString::new("__mul"))),
                div: Rc::new(RefCell::new(LuaString::new("__div"))),
                mod_op: Rc::new(RefCell::new(LuaString::new("__mod"))),
                pow: Rc::new(RefCell::new(LuaString::new("__pow"))),
                unm: Rc::new(RefCell::new(LuaString::new("__unm"))),
                concat: Rc::new(RefCell::new(LuaString::new("__concat"))),
                len: Rc::new(RefCell::new(LuaString::new("__len"))),
                eq: Rc::new(RefCell::new(LuaString::new("__eq"))),
                lt: Rc::new(RefCell::new(LuaString::new("__lt"))),
                le: Rc::new(RefCell::new(LuaString::new("__le"))),
            },
        };
        
        // Pre-intern common strings
        temp_heap.pre_intern_common_strings()?;
        
        // Create main thread, globals table, and registry table
        let main_thread = Rc::new(RefCell::new(Thread::new()));
        let globals = Rc::new(RefCell::new(Table::new()));
        let registry_table = Rc::new(RefCell::new(Table::new()));
        
        // Replace the temporary values with the real ones
        temp_heap.main_thread = main_thread;
        temp_heap.globals = globals;
        temp_heap.registry_table = registry_table;
        
        // Replace the metamethod names with properly interned strings
        temp_heap.metamethod_names = MetamethodNames {
            index: temp_heap.create_string("__index")?,
            newindex: temp_heap.create_string("__newindex")?,
            call: temp_heap.create_string("__call")?,
            add: temp_heap.create_string("__add")?,
            sub: temp_heap.create_string("__sub")?,
            mul: temp_heap.create_string("__mul")?,
            div: temp_heap.create_string("__div")?,
            mod_op: temp_heap.create_string("__mod")?,
            pow: temp_heap.create_string("__pow")?,
            unm: temp_heap.create_string("__unm")?,
            concat: temp_heap.create_string("__concat")?,
            len: temp_heap.create_string("__len")?,
            eq: temp_heap.create_string("__eq")?,
            lt: temp_heap.create_string("__lt")?,
            le: temp_heap.create_string("__le")?,
        };
        
        Ok(temp_heap)
    }
    
    /// Pre-intern common strings
    fn pre_intern_common_strings(&self) -> LuaResult<()> {
        const COMMON_STRINGS: &[&str] = &[
            // Standard library functions
            "print", "type", "tostring", "tonumber", 
            "next", "pairs", "ipairs", 
            "getmetatable", "setmetatable",
            "rawget", "rawset", "rawequal",
            "assert", "error", "select",
            
            // Common keys
            "_G", "self", "value", "_LOADED",
        ];
        
        for s in COMMON_STRINGS {
            self.create_string(s)?;
        }
        
        Ok(())
    }
    
    /// Get the main thread
    pub fn main_thread(&self) -> ThreadHandle {
        Rc::clone(&self.main_thread)
    }
    
    /// Get the globals table
    pub fn globals(&self) -> TableHandle {
        Rc::clone(&self.globals)
    }
    
    /// Get the registry table
    pub fn registry_table(&self) -> TableHandle {
        Rc::clone(&self.registry_table)
    }
    
    //
    // String operations
    //
    
    /// Create a string with interning
    pub fn create_string(&self, s: &str) -> LuaResult<StringHandle> {
        let bytes = s.as_bytes();
        
        // Check cache first (read-only access)
        {
            let cache = self.string_cache.borrow();
            if let Some(handle) = cache.get(bytes) {
                // CRITICAL FIX: Verify cached handle is still valid AND content matches
                if let Ok(string_ref) = handle.try_borrow() {
                    // Verify content matches exactly
                    if string_ref.bytes == bytes {
                        return Ok(Rc::clone(handle));
                    }
                }
            }
        }
        
        // Create new string
        let lua_string = LuaString::new(s);
        let handle = Rc::new(RefCell::new(lua_string));
        
        // Add to cache (write access)
        {
            let mut cache = self.string_cache.borrow_mut();
            cache.insert(bytes.to_vec(), Rc::clone(&handle));
        }
        
        Ok(handle)
    }
    
    /// Create a string from bytes with interning
    pub fn create_string_from_bytes(&self, bytes: &[u8]) -> LuaResult<StringHandle> {
        // Check cache first (read-only access)
        {
            let cache = self.string_cache.borrow();
            if let Some(handle) = cache.get(bytes) {
                // CRITICAL FIX: Verify cached handle is still valid AND content matches
                if let Ok(string_ref) = handle.try_borrow() {
                    // Verify content matches exactly
                    if string_ref.bytes == bytes {
                        return Ok(Rc::clone(handle));
                    }
                }
            }
        }
        
        // Create new string
        let lua_string = LuaString::from_bytes(bytes);
        let handle = Rc::new(RefCell::new(lua_string));
        
        // Add to cache (write access)
        {
            let mut cache = self.string_cache.borrow_mut();
            cache.insert(bytes.to_vec(), Rc::clone(&handle));
        }
        
        Ok(handle)
    }
    
    //
    // Table operations
    //
    
    /// Create a table
    pub fn create_table(&self) -> TableHandle {
        Rc::new(RefCell::new(Table::new()))
    }
    
    /// Create a table with capacity
    pub fn create_table_with_capacity(&self, array_cap: usize, map_cap: usize) -> TableHandle {
        Rc::new(RefCell::new(Table::with_capacity(array_cap, map_cap)))
    }
    
    /// Get table field with metamethod support
    pub fn get_table_field(&self, table: &TableHandle, key: &Value) -> LuaResult<Value> {
        let mut current_table = Rc::clone(table);
        for _ in 0..100 { // Loop to handle table-based __index, with a cycle guard
            let table_ref = current_table.borrow();
    
            // Step 1: Try direct access in the current table.
            if let Some(value) = table_ref.get_field(key) {
                // ALWAYS return if the key is present, even if value is nil
                return Ok(value);
            }
    
            // Key was NOT present in table. Proceed to check the metatable.
            let metatable = match &table_ref.metatable {
                Some(mt) => mt.clone(),
                None => return Ok(Value::Nil),
            };
    
            // Drop the borrow on the current table before we might borrow the metatable.
            drop(table_ref);
    
            // Step 3: Check for the __index metamethod within the metatable.
            let mt_ref = metatable.borrow();
            let index_key = Value::String(Rc::clone(&self.metamethod_names.index));
            let index_mm = mt_ref.get_field(&index_key);
            
            // Drop the borrow on the metatable.
            drop(mt_ref);

            match index_mm {
                // __index is not present or is nil. The search ends.
                None | Some(Value::Nil) => return Ok(Value::Nil),

                Some(Value::Table(index_table)) => {
                    // __index is a table. Continue the search in this new table.
                    current_table = index_table;
                    // The `for` loop will continue to the next iteration.
                },
                Some(func @ Value::Closure(_)) | Some(func @ Value::CFunction(_)) => {
                    // __index is a function. The VM must call it.
                    // Return a special value to signal this to the VM execution loop.
                    return Ok(Value::PendingMetamethod(Box::new(func)));
                },
                // __index is some other value (number, string, etc.). Return it directly.
                Some(other) => return Ok(other),
            }
        }
    
        // If we hit the loop limit, it's a potential infinite __index loop.
        Err(LuaError::RuntimeError("__index chain too deep".to_string()))
    }
    
    /// Get table field without metamethod support (raw operation)
    pub fn raw_get_table_field(&self, table: &TableHandle, key: &Value) -> Value {
        let table_ref = table.borrow();
        table_ref.get_field(key).unwrap_or(Value::Nil)
    }

    /// Set table field with metamethod support
    /// Returns a metamethod function if one needs to be called by the VM.
    pub fn set_table_field(&self, table: &TableHandle, key: &Value, value: &Value) -> LuaResult<Option<Value>> {
        let mut current_table = table.clone();
        for _iteration in 0..100 { // Loop to handle table-based __newindex
            let table_ref = current_table.borrow();

            // Lua 5.1: __newindex is only invoked if the key does NOT exist in the table.
            if table_ref.get_field(key).is_some() {
                drop(table_ref);
                self.set_table_field_raw_with_alias_check(&current_table, key, value)?;
                return Ok(None);
            }

            let metatable = match table_ref.metatable() {
                Some(mt) => mt.clone(),
                None => {
                    drop(table_ref);
                    self.set_table_field_raw_with_alias_check(&current_table, key, value)?;
                    return Ok(None);
                }
            };
            
            drop(table_ref);

            let mt_ref = metatable.borrow();
            let newindex_key = Value::String(self.metamethod_names.newindex.clone());
            let newindex_mm = mt_ref.get_field(&newindex_key).unwrap_or(Value::Nil);
            drop(mt_ref);

            match newindex_mm {
                Value::Table(newindex_table) => {
                    current_table = newindex_table;
                    continue;
                },
                func @ Value::Closure(_) | func @ Value::CFunction(_) => {
                    return Ok(Some(func));
                },
                _ => {
                    self.set_table_field_raw_with_alias_check(&current_table, key, value)?;
                    return Ok(None);
                }
            }
        }
        
        Err(LuaError::RuntimeError("__newindex chain too deep".to_string()))
    }

    /// Helper function for raw set operations that includes the two-phase commit logic.
    fn set_table_field_raw_with_alias_check(&self, table: &TableHandle, key: &Value, value: &Value) -> LuaResult<()> {
        // Check for the aliasing case: table[key] = table
        if let Value::Table(value_table) = value {
            if Rc::ptr_eq(table, value_table) {
                // Self-assignment detected. Use the two-phase commit pattern.
                // Phase 1: Insert a temporary placeholder.
                {
                    let mut table_mut = table.borrow_mut();
                    // Use Boolean(false) as placeholder instead of Nil to avoid removal logic
                    table_mut.set_field(key.clone(), Value::Boolean(false))?;
                }

                // Phase 2: Replace the placeholder with the actual self-referential value.
                {
                    let mut table_mut = table.borrow_mut();
                    if let Some(placeholder_ref) = table_mut.get_field_mut(key) {
                        *placeholder_ref = value.clone();
                    } else {
                        return Err(LuaError::RuntimeError("Internal VM error: two-phase set failed".to_string()));
                    }
                }

                return Ok(());
            }
        }

        // Default case: No self-assignment detected, perform a standard raw set.
        self.set_table_field_raw(table, key, value)
    }
    
    /// Set table field without metamethod support (raw operation)
    pub fn set_table_field_raw(&self, table: &TableHandle, key: &Value, value: &Value) -> LuaResult<()> {
        let mut table_ref = table.borrow_mut();
        table_ref.set_field(key.clone(), value.clone())?;
        Ok(())
    }
    
    //
    // Upvalue operations
    //
    
    /// Find or create an upvalue for a stack location - KISS LUA 5.1 SPECIFICATION COMPLIANT
    pub fn find_or_create_upvalue(&self, thread: &ThreadHandle, stack_index: usize) -> LuaResult<UpvalueHandle> {
        // Validate stack index is within bounds
        {
            let thread_ref = thread.borrow();
            if stack_index >= thread_ref.stack.len() {
                return Err(LuaError::RuntimeError(format!(
                    "Cannot create upvalue for stack index {} (stack size: {})",
                    stack_index, thread_ref.stack.len()
                )));
            }
            
            // KISS LUA 5.1 SPECIFICATION: "Multiple closures sharing same local variable share same upvalue object"
            // Simple criterion: same stack_index = same local variable = share upvalue
            for upvalue in &thread_ref.open_upvalues {
                if let Ok(uv_ref) = upvalue.try_borrow() {
                    if let UpvalueState::Open { stack_index: idx, .. } = &*uv_ref {
                        if *idx == stack_index {
                            return Ok(Rc::clone(upvalue));
                        }
                    }
                }
            }
        }
        
        // Create a new upvalue - no complex frame logic needed
        let upvalue = Rc::new(RefCell::new(UpvalueState::Open {
            thread: Rc::clone(thread),
            stack_index,
        }));
        
        // Add to thread's open upvalues list
        let mut thread_ref = thread.borrow_mut();
        
        // Find insertion position - sorted by stack index (highest first) for efficient closing
        let mut insert_pos = 0;
        while insert_pos < thread_ref.open_upvalues.len() {
            let uv_ref = thread_ref.open_upvalues[insert_pos].borrow();
            if let UpvalueState::Open { stack_index: idx, .. } = &*uv_ref {
                if *idx < stack_index {
                    break;
                }
            }
            insert_pos += 1;
        }
        
        thread_ref.open_upvalues.insert(insert_pos, Rc::clone(&upvalue));
        
        Ok(upvalue)
    }
    
    /// Close all upvalues at or above a stack index - KISS LUA 5.1 SPECIFICATION
    pub fn close_upvalues(&self, thread: &ThreadHandle, stack_index: usize) -> LuaResult<()> {
        // KISS APPROACH: Simple two-pass method instead of complex three-pass
        
        // Pass 1: Collect and close upvalues that need closing
        let upvalues_to_close = {
            let thread_ref = thread.borrow();
            let mut to_close = Vec::new();
            
            for upvalue in &thread_ref.open_upvalues {
                if let Ok(borrowed) = upvalue.try_borrow() {
                    if let UpvalueState::Open { stack_index: idx, .. } = &*borrowed {
                        if *idx >= stack_index {
                            // Get the current value and close immediately
                            let captured_value = if *idx < thread_ref.stack.len() {
                                thread_ref.stack[*idx].clone()
                            } else {
                                Value::Nil
                            };
                            to_close.push((Rc::clone(upvalue), captured_value));
                        }
                    }
                }
            }
            to_close
        };
        
        // Close the upvalues (outside thread borrow)
        for (upvalue, value) in &upvalues_to_close {
            if let Ok(mut uv_ref) = upvalue.try_borrow_mut() {
                *uv_ref = UpvalueState::Closed { value: value.clone() };
            }
        }
        
        // Pass 2: Clean up the open list (simple retain filter)
        {
            let mut thread_ref = thread.borrow_mut();
            thread_ref.open_upvalues.retain(|upvalue| {
                if let Ok(borrowed) = upvalue.try_borrow() {
                    matches!(&*borrowed, UpvalueState::Open { .. })
                } else {
                    true // Keep if we can't borrow
                }
            });
        }
        
        Ok(())
    }
    
    /// Get upvalue value
    pub fn get_upvalue_value(&self, upvalue: &UpvalueHandle) -> Value {
        let uv_ref = upvalue.borrow();
        
        match &*uv_ref {
            UpvalueState::Open { thread, stack_index } => {
                let thread_ref = thread.borrow();
                
                // Check if stack index is valid
                if *stack_index >= thread_ref.stack.len() {
                    return Value::Nil;
                }
                
                thread_ref.stack[*stack_index].clone()
            },
            UpvalueState::Closed { value } => {
                value.clone()
            }
        }
    }
    
    /// Set upvalue value
    pub fn set_upvalue_value(&self, upvalue: &UpvalueHandle, value: Value) -> LuaResult<()> {
        // Check for the aliasing case: upvalue.value = closure_that_captured(upvalue)
        let is_aliased = if let Value::Closure(closure_handle) = &value {
            // We only need to check if we are setting a closed upvalue.
            if let UpvalueState::Closed { .. } = *upvalue.borrow() {
                let closure_ref = closure_handle.borrow();
                closure_ref.upvalues.iter().any(|uv_in_closure| Rc::ptr_eq(upvalue, uv_in_closure))
            } else {
                false
            }
        } else {
            false
        };

        if is_aliased {
            // Aliasing detected: Use the two-phase commit pattern.
            // Phase 1: Set a temporary placeholder.
            {
                let mut uv_mut = upvalue.borrow_mut();
                if let UpvalueState::Closed { value: ref mut v } = &mut *uv_mut {
                    // Using Boolean(false) as a placeholder.
                    *v = Value::Boolean(false);
                }
            }

            // Phase 2: Replace the placeholder with the actual self-referential value.
            {
                let mut uv_mut = upvalue.borrow_mut();
                
                if let UpvalueState::Closed { value: ref mut v } = &mut *uv_mut {
                    *v = value;
                }
            }
            return Ok(());
        }
    
        // Default case: No self-assignment detected, perform a standard set.
        let mut uv_ref = upvalue.borrow_mut();
        
        match &mut *uv_ref {
            UpvalueState::Open { thread, stack_index } => {
                const LUAI_MAXSTACK: usize = 1000000; // Lua 5.1 standard
                
                if *stack_index >= LUAI_MAXSTACK {
                    return Err(LuaError::StackOverflow);
                }
            
                let mut thread_ref = thread.borrow_mut();
                
                // Add bounds check and automatic expansion up to Lua's limit
                if *stack_index >= thread_ref.stack.len() {
                    // Grow stack if needed, but limit maximum growth
                    let new_size = (*stack_index + 1).min(LUAI_MAXSTACK);
                    thread_ref.stack.resize(new_size, Value::Nil);
                }
                
                thread_ref.stack[*stack_index] = value;
                Ok(())
            },
            UpvalueState::Closed { value: ref mut v } => {
                *v = value;
                Ok(())
            },
        }
    }
    
    //
    // Closure operations
    //
    
    /// Create a closure with proper environment inheritance
    pub fn create_closure(&self, proto: FunctionProtoHandle, upvalues: Vec<UpvalueHandle>, env: TableHandle) -> ClosureHandle {
        let closure = Closure {
            proto,
            upvalues,
            env,
        };
        
        Rc::new(RefCell::new(closure))
    }
    
    //
    // Thread operations
    //
    
    /// Push a call frame to a thread
    pub fn push_call_frame(&self, thread: &ThreadHandle, frame: CallFrame) -> LuaResult<()> {
        let mut thread_ref = thread.borrow_mut();
        thread_ref.call_frames.push(Frame::Call(frame));
        Ok(())
    }
    
    /// Pop a call frame from a thread
    pub fn pop_call_frame(&self, thread: &ThreadHandle) -> LuaResult<CallFrame> {
        let mut thread_ref = thread.borrow_mut();
        match thread_ref.call_frames.pop() {
            Some(Frame::Call(cf)) => Ok(cf),
            Some(_) => Err(LuaError::RuntimeError("Attempted to pop non-call frame".into())),
            None => Err(LuaError::RuntimeError("No call frames to pop".into())),
        }
    }
    
    /// Get the current call frame from a thread
    pub fn get_current_frame(&self, thread: &ThreadHandle) -> LuaResult<CallFrame> {
        let thread_ref = thread.borrow();
        thread_ref
            .call_frames
            .last()
            .and_then(|f| match f {
                Frame::Call(cf) => Some(cf.clone()),
                _ => None,
            })
            .ok_or(LuaError::RuntimeError("No active call frames".into()))
    }
    
    /// Get the program counter of the current frame
    pub fn get_pc(&self, thread: &ThreadHandle) -> LuaResult<usize> {
        let thread_ref = thread.borrow();
        thread_ref
            .call_frames
            .last()
            .and_then(|f| match f {
                Frame::Call(cf) => Some(cf.pc),
                _ => None,
            })
            .ok_or(LuaError::RuntimeError("No active call frames".to_string()))
    }
    
    /// Set the program counter of the current frame
    pub fn set_pc(&self, thread: &ThreadHandle, pc: usize) -> LuaResult<()> {
        let mut thread_ref = thread.borrow_mut();
        if let Some(Frame::Call(frame)) = thread_ref.call_frames.last_mut() {
            frame.pc = pc;
            Ok(())
        } else {
            Err(LuaError::RuntimeError("No active call frames".into()))
        }
    }
    
    /// Increment the program counter of the current frame
    pub fn increment_pc(&self, thread: &ThreadHandle) -> LuaResult<()> {
        let mut thread_ref = thread.borrow_mut();
        if let Some(Frame::Call(frame)) = thread_ref.call_frames.last_mut() {
            frame.pc += 1;
            Ok(())
        } else {
            Err(LuaError::RuntimeError("No active call frames".into()))
        }
    }
    
    //
    // Function prototype operations
    //
    
    /// Create a function prototype
    pub fn create_function_proto(&self, proto: FunctionProto) -> FunctionProtoHandle {
        Rc::new(proto)
    }
    
    //
    // Register operations
    //
    
    /// Get the value at a stack index
    pub fn get_register(&self, thread: &ThreadHandle, index: usize) -> LuaResult<Value> {
        let thread_ref = thread.borrow();
        if index >= thread_ref.stack.len() {
            return Err(LuaError::RuntimeError(
                format!("Register {} out of bounds (stack size: {})", index, thread_ref.stack.len())
            ));
        }
        
        Ok(thread_ref.stack[index].clone())
    }
    
    /// Set the value at a stack index
    pub fn set_register(&self, thread: &ThreadHandle, index: usize, value: Value) -> LuaResult<()> {
        let mut thread_ref = thread.borrow_mut();
        
        // Grow stack if needed
        if index >= thread_ref.stack.len() {
            thread_ref.stack.resize(index + 1, Value::Nil);
        }
        
        thread_ref.stack[index] = value;
        Ok(())
    }
    
    /// Get the stack size
    pub fn get_stack_size(&self, thread: &ThreadHandle) -> usize {
        let thread_ref = thread.borrow();
        thread_ref.stack.len()
    }
    
    /// Get the call frame depth
    pub fn get_call_depth(&self, thread: &ThreadHandle) -> usize {
        let thread_ref = thread.borrow();
        thread_ref
            .call_frames
            .iter()
            .filter(|f| matches!(f, Frame::Call(_)))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_string() -> LuaResult<()> {
        let heap = RcHeap::new()?;
        
        // Create a string
        let handle1 = heap.create_string("test")?;
        
        // Create the same string again - should be interned
        let handle2 = heap.create_string("test")?;
        
        // Both handles should point to the same string
        assert!(Rc::ptr_eq(&handle1, &handle2));
        
        Ok(())
    }
}