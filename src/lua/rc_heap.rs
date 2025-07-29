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
                return Ok(Rc::clone(handle));
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
                return Ok(Rc::clone(handle));
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
                // This is the correct Lua 5.1 specification behavior
                return Ok(value);
            }
    
            // Key was NOT present in table. Proceed to check the metatable.
            let metatable = match &table_ref.metatable {
                Some(mt) => mt.clone(),
                // No metatable, so the search ends here.
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

    /// Set table field with metamethod support (WITH COMPREHENSIVE DEBUGGING)
    /// Returns a metamethod function if one needs to be called by the VM.
    pub fn set_table_field(&self, table: &TableHandle, key: &Value, value: &Value) -> LuaResult<Option<Value>> {
        eprintln!("DEBUG HEAP set_table_field: Called with key={:?}, value={:?}", key, value);
        
        let mut current_table = table.clone();
        for iteration in 0..100 { // Loop to handle table-based __newindex
            eprintln!("DEBUG HEAP set_table_field: Iteration {} of metamethod chain on table {:p}", iteration, Rc::as_ptr(&current_table));
            
            let table_ref = current_table.borrow();

            // Lua 5.1: __newindex is only invoked if the key does NOT exist in the table. An existing nil is NOT a trigger.
            if table_ref.get_field(key).is_some() {
                eprintln!("DEBUG HEAP set_table_field: Key exists, doing raw set on current table");
                drop(table_ref); // Release borrow before mutable borrow
                self.set_table_field_raw_with_alias_check(&current_table, key, value)?;
                return Ok(None); // No metamethod call needed
            }

            eprintln!("DEBUG HEAP set_table_field: Key not found, checking for metatable");

            let metatable = match table_ref.metatable() {
                Some(mt) => mt.clone(),
                None => {
                    eprintln!("DEBUG HEAP set_table_field: No metatable, doing raw set on current table");
                    drop(table_ref);
                    self.set_table_field_raw_with_alias_check(&current_table, key, value)?;
                    return Ok(None);
                }
            };
            
            drop(table_ref); // Release borrow

            let mt_ref = metatable.borrow();
            let newindex_key = Value::String(self.metamethod_names.newindex.clone());
            let newindex_mm = mt_ref.get_field(&newindex_key).unwrap_or(Value::Nil);
            eprintln!("DEBUG HEAP set_table_field: __newindex metamethod: {:?}", newindex_mm.type_name());
            drop(mt_ref);

            match newindex_mm {
                Value::Table(newindex_table) => {
                    eprintln!("DEBUG HEAP set_table_field: __newindex is a table, restarting logic on it");
                    current_table = newindex_table;
                    continue; // Continue loop with the new table
                },
                func @ Value::Closure(_) | func @ Value::CFunction(_) => {
                    eprintln!("DEBUG HEAP set_table_field: __newindex is a function, returning for VM to call");
                    return Ok(Some(func));
                },
                _ => {
                    eprintln!("DEBUG HEAP set_table_field: __newindex is nil or non-function, doing raw set on current table");
                    self.set_table_field_raw_with_alias_check(&current_table, key, value)?;
                    return Ok(None);
                }
            }
        }
        
        eprintln!("DEBUG HEAP set_table_field: ERROR - __newindex chain too deep");
        Err(LuaError::RuntimeError("__newindex chain too deep".to_string()))
    }

    /// Helper function for raw set operations that includes the two-phase commit logic.
    /// This is the core architectural solution for circular reference borrow conflicts.
    fn set_table_field_raw_with_alias_check(&self, table: &TableHandle, key: &Value, value: &Value) -> LuaResult<()> {
        eprintln!("DEBUG HEAP: Entering set_table_field_raw_with_alias_check");
        eprintln!("DEBUG HEAP: Checking for self-assignment aliasing case");
        
        // Check for the aliasing case: table[key] = table
        if let Value::Table(value_table) = value {
            if Rc::ptr_eq(table, value_table) {
                eprintln!("DEBUG HEAP: SELF-ASSIGNMENT DETECTED! Using two-phase commit pattern");
                
                // Self-assignment detected. Use the two-phase commit pattern.
                // Phase 1: Insert a temporary placeholder. This avoids the borrow conflict.
                eprintln!("DEBUG HEAP: Phase 1 - Inserting placeholder");
                {
                    let mut table_mut = table.borrow_mut();
                    // Use Boolean(false) as placeholder instead of Nil to avoid removal logic
                    table_mut.set_field(key.clone(), Value::Boolean(false))?;
                    eprintln!("DEBUG HEAP: Phase 1 complete - placeholder inserted");
                }

                // Phase 2: Replace the placeholder with the actual self-referential value.
                eprintln!("DEBUG HEAP: Phase 2 - Replacing placeholder with actual value");
                {
                    let mut table_mut = table.borrow_mut();
                    if let Some(placeholder_ref) = table_mut.get_field_mut(key) {
                        *placeholder_ref = value.clone();
                        eprintln!("DEBUG HEAP: Phase 2 complete - self-reference established");
                    } else {
                        eprintln!("DEBUG HEAP: ERROR - Phase 2 failed: placeholder not found");
                        return Err(LuaError::RuntimeError("Internal VM error: two-phase set failed".to_string()));
                    }
                }

                eprintln!("DEBUG HEAP: Two-phase commit successful for self-assignment");
                return Ok(());
            }
        }

        eprintln!("DEBUG HEAP: No self-assignment detected, performing standard raw set");
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
    
    /// Find or create an upvalue for a stack location
    pub fn find_or_create_upvalue(&self, thread: &ThreadHandle, stack_index: usize) -> LuaResult<UpvalueHandle> {
        eprintln!("DEBUG find_or_create_upvalue: Looking for upvalue at stack index {}", stack_index);
        
        // Validate stack index is within bounds
        {
            let thread_ref = thread.borrow();
            if stack_index >= thread_ref.stack.len() {
                eprintln!("DEBUG find_or_create_upvalue: ERROR - Stack index {} out of bounds (stack size: {})",
                         stack_index, thread_ref.stack.len());
                return Err(LuaError::RuntimeError(format!(
                    "Cannot create upvalue for stack index {} (stack size: {})",
                    stack_index, thread_ref.stack.len()
                )));
            }
            
            // Display the stack value at this index
            let stack_value = &thread_ref.stack[stack_index];
            eprintln!("DEBUG find_or_create_upvalue: Stack value at index {}: {:?}", 
                     stack_index, stack_value);
            
            // Check for existing upvalue
            eprintln!("DEBUG find_or_create_upvalue: Checking {} existing open upvalues", 
                     thread_ref.open_upvalues.len());
            
            // Get current call frame context for scoping
            let current_frame_base = if !thread_ref.call_frames.is_empty() {
                if let super::rc_value::Frame::Call(ref frame) = thread_ref.call_frames.last().unwrap() {
                    frame.base_register as usize
                } else {
                    0
                }
            } else {
                0
            };
            
            // Only look for upvalues that are within current lexical scope
            // (i.e., not from previous completed function calls)
            for (i, upvalue) in thread_ref.open_upvalues.iter().enumerate() {
                if let Ok(uv_ref) = upvalue.try_borrow() {
                    if let UpvalueState::Open { stack_index: idx, .. } = &*uv_ref {
                        eprintln!(
                            "DEBUG find_or_create_upvalue: Open upvalue[{}] points to stack index {} (current frame base: {})",
                            i, idx, current_frame_base
                        );
                        
                        // Only reuse upvalues that are within current call frame
                        // This ensures closure independence across different function calls
                        if *idx == stack_index && *idx >= current_frame_base {
                            eprintln!(
                                "DEBUG find_or_create_upvalue: Found existing upvalue for stack index {} within same frame",
                                stack_index
                            );
                            return Ok(Rc::clone(upvalue));
                        } else if *idx == stack_index {
                            eprintln!(
                                "DEBUG find_or_create_upvalue: Upvalue at stack index {} is from previous frame - creating independent upvalue",
                                stack_index
                            );
                        }
                    }
                } else {
                    // The up-value is currently mutably borrowed – skip it for
                    // now but continue the scan to honour Lua's sharing rule.
                    eprintln!(
                        "DEBUG find_or_create_upvalue: Skipping upvalue[{}] – already mutably borrowed",
                        i
                    );
                }
            }
            
            eprintln!("DEBUG find_or_create_upvalue: No existing upvalue found in current scope, will create new one");
        }
        
        // Create a new upvalue
        eprintln!("DEBUG find_or_create_upvalue: Creating new upvalue for stack index {}",
                 stack_index);
        
        let upvalue = Rc::new(RefCell::new(UpvalueState::Open {
            thread: Rc::clone(thread),
            stack_index,
        }));
        
        // Add to thread's open upvalues list
        let mut thread_ref = thread.borrow_mut();
        
        // Find insertion position - sorted by stack index (highest first)
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
        eprintln!("DEBUG find_or_create_upvalue: Added new upvalue at position {} in open_upvalues list (total: {})", 
                 insert_pos, thread_ref.open_upvalues.len());
        
        // Dump the current open upvalues list
        eprintln!("DEBUG find_or_create_upvalue: Current open upvalues list:");
        for (i, uv) in thread_ref.open_upvalues.iter().enumerate() {
            if let Ok(uv_ref) = uv.try_borrow() {
                if let UpvalueState::Open { stack_index: idx, .. } = &*uv_ref {
                    eprintln!("  [{}]: Stack index {}", i, idx);
                }
            }
        }
        
        Ok(upvalue)
    }
    
    /// Close all upvalues at or above a stack index
    pub fn close_upvalues(&self, thread: &ThreadHandle, stack_index: usize) -> LuaResult<()> {
        eprintln!("DEBUG close_upvalues: Called with stack_index={}", stack_index);
        
        // First pass - collect upvalues to close with their values
        let to_close: Vec<(UpvalueHandle, Value)> = {
            let thread_ref = thread.borrow();
            let mut upvalues_to_close = Vec::new();
            
            eprintln!("DEBUG close_upvalues: Thread has {} open upvalues", thread_ref.open_upvalues.len());
            
            for (i, upvalue) in thread_ref.open_upvalues.iter().enumerate() {
                if let Ok(borrowed) = upvalue.try_borrow() {
                    if let UpvalueState::Open { stack_index: idx, .. } = &*borrowed {
                        eprintln!("DEBUG close_upvalues: Checking upvalue[{}] pointing to stack index {}", i, idx);
                        
                        if *idx >= stack_index {
                            eprintln!("DEBUG close_upvalues: Upvalue[{}] needs closing (stack_index {} >= {})", 
                                     i, idx, stack_index);
                            
                            // Get the current value from the stack safely
                            let captured_value = if *idx < thread_ref.stack.len() {
                                let value = thread_ref.stack[*idx].clone();
                                eprintln!("DEBUG close_upvalues: Capturing value at stack[{}]: {:?}", idx, value);
                                value
                            } else {
                                eprintln!("DEBUG close_upvalues: WARNING - Stack index {} out of bounds (stack size: {}), using Nil", 
                                         idx, thread_ref.stack.len());
                                Value::Nil
                            };
                            
                            upvalues_to_close.push((Rc::clone(upvalue), captured_value));
                        } else {
                            eprintln!("DEBUG close_upvalues: Upvalue[{}] remains open (stack_index {} < {})", 
                                     i, idx, stack_index);
                        }
                    } else {
                        eprintln!("DEBUG close_upvalues: Upvalue[{}] is already closed", i);
                    }
                } else {
                    eprintln!("DEBUG close_upvalues: WARNING - Cannot borrow upvalue[{}]", i);
                }
            }
            
            eprintln!("DEBUG close_upvalues: Found {} upvalues to close", upvalues_to_close.len());
            upvalues_to_close
        };
        
        // Second pass - actually close the upvalues
        let mut close_count = 0;
        for (upvalue, value) in &to_close {
            eprintln!("DEBUG close_upvalues: Closing upvalue with value {:?}", value);
            
            if let Ok(mut uv_ref) = upvalue.try_borrow_mut() {
                if let UpvalueState::Open { stack_index: idx, .. } = &*uv_ref {
                    eprintln!("DEBUG close_upvalues: Closing upvalue at stack index {} with value {:?}", idx, value);
                }
                
                *uv_ref = UpvalueState::Closed { value: value.clone() };
                close_count += 1;
                
                // Verify it was closed
                drop(uv_ref);
                if let Ok(check_ref) = upvalue.try_borrow() {
                    match &*check_ref {
                        UpvalueState::Closed { value: v } => {
                            eprintln!("DEBUG close_upvalues: Verified upvalue is now closed with value: {:?}", v);
                        }
                        UpvalueState::Open { .. } => {
                            eprintln!("DEBUG close_upvalues: ERROR - Upvalue is still open after closing!");
                        }
                    }
                }
            } else {
                eprintln!("DEBUG close_upvalues: ERROR - Cannot borrow_mut upvalue for closing");
            }
        }
        
        eprintln!("DEBUG close_upvalues: Successfully closed {} upvalues", close_count);
        
        // Third pass - remove closed upvalues from the open list
        let removed_count = {
            let mut thread_ref = thread.borrow_mut();
            let initial_count = thread_ref.open_upvalues.len();
            
            thread_ref.open_upvalues.retain(|upvalue| {
                if let Ok(borrowed) = upvalue.try_borrow() {
                    let is_open = matches!(&*borrowed, UpvalueState::Open { .. });
                    if !is_open {
                        eprintln!("DEBUG close_upvalues: Removing closed upvalue from open list");
                    }
                    is_open
                } else {
                    eprintln!("DEBUG close_upvalues: WARNING - Cannot borrow upvalue during retain, keeping it");
                    true // If we can't borrow, keep it to be safe
                }
            });
            
            let final_count = thread_ref.open_upvalues.len();
            initial_count - final_count
        };
        
        eprintln!("DEBUG close_upvalues: Removed {} upvalues from open list", removed_count);
        eprintln!("DEBUG close_upvalues: Operation complete");
        
        Ok(())
    }
    
    /// Get upvalue value with detailed debugging
    pub fn get_upvalue_value(&self, upvalue: &UpvalueHandle) -> Value {
        eprintln!("DEBUG get_upvalue_value: Accessing upvalue");
        let uv_ref = upvalue.borrow();
        
        match &*uv_ref {
            UpvalueState::Open { thread, stack_index } => {
                eprintln!("DEBUG get_upvalue_value: Upvalue is OPEN, points to stack index {}", stack_index);
                
                let thread_ref = thread.borrow();
                
                // Check if stack index is valid
                if *stack_index >= thread_ref.stack.len() {
                    eprintln!("DEBUG get_upvalue_value: ERROR - Stack index {} out of bounds (stack size: {})",
                             stack_index, thread_ref.stack.len());
                    eprintln!("DEBUG get_upvalue_value: This indicates the upvalue should have been closed!");
                    
                    // Dump call stack info for debugging
                    eprintln!("DEBUG get_upvalue_value: Thread has {} call frames", thread_ref.call_frames.len());
                    for (i, frame) in thread_ref.call_frames.iter().enumerate() {
                        match frame {
                            Frame::Call(cf) => {
                                eprintln!("  Frame[{}]: PC={}, base_register={}", i, cf.pc, cf.base_register);
                            }
                            Frame::Continuation(_) => {
                                eprintln!("  Frame[{}]: Continuation", i);
                            }
                        }
                    }
                    
                    return Value::Nil;
                }
                
                let value = thread_ref.stack[*stack_index].clone();
                eprintln!("DEBUG get_upvalue_value: Got value from stack index {}: {:?}", 
                         stack_index, value);
                
                // Debug dump of stack around this position
                eprintln!("DEBUG get_upvalue_value: Stack around index {} (stack size {})", 
                         stack_index, thread_ref.stack.len());
                
                let start = stack_index.saturating_sub(2);
                let end = (*stack_index + 3).min(thread_ref.stack.len());
                
                for i in start..end {
                    let mark = if i == *stack_index { " <- UPVALUE HERE" } else { "" };
                    eprintln!("  Stack[{}] = {:?}{}", i, thread_ref.stack[i], mark);
                }
                
                // Check if upvalue is in the open_upvalues list
                let upvalue_in_list = thread_ref.open_upvalues.iter().any(|uv| {
                    if let Ok(uv_borrowed) = uv.try_borrow() {
                        if let UpvalueState::Open { stack_index: idx, .. } = &*uv_borrowed {
                            *idx == *stack_index
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                });
                
                if !upvalue_in_list {
                    eprintln!("DEBUG get_upvalue_value: WARNING - This upvalue is not in thread's open_upvalues list!");
                }
                
                return value;
            },
            UpvalueState::Closed { value } => {
                eprintln!("DEBUG get_upvalue_value: Upvalue is CLOSED with value: {:?}", value);
                return value.clone();
            }
        }
    }
    
    /// Set upvalue value
    pub fn set_upvalue_value(&self, upvalue: &UpvalueHandle, value: Value) -> LuaResult<()> {
        // Check for the aliasing case: upvalue.value = closure_that_captured(upvalue)
        let is_aliased = if let Value::Closure(closure_handle) = &value {
            // We only need to check if we are setting a closed upvalue.
            // If it's open, we're writing to the stack, which is safe.
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
                // Note: if the upvalue was open, is_aliased would be false.
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
    
    /// Create a closure
    pub fn create_closure(&self, proto: FunctionProtoHandle, upvalues: Vec<UpvalueHandle>) -> ClosureHandle {
        let closure = Closure {
            proto,
            upvalues,
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
    
    #[test]
    fn test_upvalue_sharing() -> LuaResult<()> {
        let heap = RcHeap::new()?;
        let thread = heap.main_thread();
        
        // Set up stack
        heap.set_register(&thread, 0, Value::Number(42.0))?;
        
        // Create two upvalues for the same stack location
        let upvalue1 = heap.find_or_create_upvalue(&thread, 0)?;
        let upvalue2 = heap.find_or_create_upvalue(&thread, 0)?;
        
        // They should be the same upvalue
        assert!(Rc::ptr_eq(&upvalue1, &upvalue2));
        
        // Modify value through first upvalue
        heap.set_upvalue_value(&upvalue1, Value::Number(99.0))?;
        
        // Get value through second upvalue
        let value = heap.get_upvalue_value(&upvalue2);
        
        // Value should have changed
        assert_eq!(value, Value::Number(99.0));
        
        Ok(())
    }
    
    #[test]
    fn test_close_upvalues() -> LuaResult<()> {
        let heap = RcHeap::new()?;
        let thread = heap.main_thread();
        
        // Set up stack with multiple values
        heap.set_register(&thread, 0, Value::Number(10.0))?;
        heap.set_register(&thread, 1, Value::Number(20.0))?;
        heap.set_register(&thread, 2, Value::Number(30.0))?;
        
        // Create upvalues
        let upvalue0 = heap.find_or_create_upvalue(&thread, 0)?;
        let upvalue1 = heap.find_or_create_upvalue(&thread, 1)?;
        let upvalue2 = heap.find_or_create_upvalue(&thread, 2)?;
        
        // Close upvalues at or above index 1
        heap.close_upvalues(&thread, 1)?;
        
        // Upvalue0 should still be open
        assert!(matches!(*upvalue0.borrow(), UpvalueState::Open { .. }));
        
        // Upvalue1 and Upvalue2 should be closed
        match *upvalue1.borrow() {
            UpvalueState::Closed { ref value } => {
                assert_eq!(*value, Value::Number(20.0));
            },
            _ => panic!("Upvalue1 should be closed"),
        }
        
        match *upvalue2.borrow() {
            UpvalueState::Closed { ref value } => {
                assert_eq!(*value, Value::Number(30.0));
            },
            _ => panic!("Upvalue2 should be closed"),
        }
        
        // Check thread's open_upvalues list
        let thread_ref = thread.borrow();
        assert_eq!(thread_ref.open_upvalues.len(), 1);
        assert!(Rc::ptr_eq(&thread_ref.open_upvalues[0], &upvalue0));
        
        Ok(())
    }
    
    #[test]
    fn test_table_metamethods() -> LuaResult<()> {
        let heap = RcHeap::new()?;
        
        // Create tables
        let table = heap.create_table();
        let metatable = heap.create_table();
        
        // Set up __index metamethod
        let index_key = Value::String(Rc::clone(&heap.metamethod_names.index));
        let value_key = heap.create_string("test_key")?;
        let value = Value::String(heap.create_string("test_value")?);
        
        // Set metatable
        {
            let mut table_ref = table.borrow_mut();
            table_ref.metatable = Some(Rc::clone(&metatable));
        }
        
        // Set __index value
        {
            let mut mt_ref = metatable.borrow_mut();
            mt_ref.set_field(index_key.clone(), value.clone())?;
        }
        
        // Try to get value
        let result = heap.get_table_field(&table, &Value::String(value_key));
        
        // Should get a PendingMetamethod for the VM to handle
        match result {
            Ok(Value::PendingMetamethod(_)) => {
                // Correct behavior - VM would handle this
            },
            _ => {
                panic!("Expected PendingMetamethod, got {:?}", result);
            }
        }
        
        Ok(())
    }
}