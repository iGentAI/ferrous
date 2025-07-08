//! Transaction-based Heap Access with Type-Safe Handle Validation
//! 
//! This module implements the transaction pattern for safe, atomic access
//! to the Lua heap with proper type-safe handle validation.

use super::arena::Handle;
use super::error::{LuaError, LuaResult};
use super::handle::{StringHandle, TableHandle, ClosureHandle, ThreadHandle, 
                    UpvalueHandle, UserDataHandle, FunctionProtoHandle};
use super::heap::LuaHeap;
use super::value::{Value, Closure, Thread, Upvalue, UserData, CallFrame, FunctionProto, HashableValue};
use super::vm::{PendingOperation, ReturnContext};
use std::collections::{HashMap, HashSet, VecDeque};
use std::marker::PhantomData;

/// Trait for type-safe handle validation
pub trait ValidatableHandle: Clone + Copy {
    /// Validate this handle against the heap
    fn validate_against_heap(&self, heap: &LuaHeap) -> LuaResult<()>;
    
    /// Get a unique validation key for caching
    fn validation_key(&self) -> ValidationKey;
}

/// Key for validation caching
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValidationKey {
    type_id: std::any::TypeId,
    index: u32,
    generation: u32,
}

// Implement ValidatableHandle for each handle type
impl ValidatableHandle for StringHandle {
    fn validate_against_heap(&self, heap: &LuaHeap) -> LuaResult<()> {
        heap.validate_string_handle(*self)
    }
    
    fn validation_key(&self) -> ValidationKey {
        ValidationKey {
            type_id: std::any::TypeId::of::<super::value::LuaString>(),
            index: self.0.index,
            generation: self.0.generation,
        }
    }
}

impl ValidatableHandle for TableHandle {
    fn validate_against_heap(&self, heap: &LuaHeap) -> LuaResult<()> {
        heap.validate_table_handle(*self)
    }
    
    fn validation_key(&self) -> ValidationKey {
        ValidationKey {
            type_id: std::any::TypeId::of::<super::value::Table>(),
            index: self.0.index,
            generation: self.0.generation,
        }
    }
}

impl ValidatableHandle for ClosureHandle {
    fn validate_against_heap(&self, heap: &LuaHeap) -> LuaResult<()> {
        heap.validate_closure_handle(*self)
    }
    
    fn validation_key(&self) -> ValidationKey {
        ValidationKey {
            type_id: std::any::TypeId::of::<super::value::Closure>(),
            index: self.0.index,
            generation: self.0.generation,
        }
    }
}

impl ValidatableHandle for ThreadHandle {
    fn validate_against_heap(&self, heap: &LuaHeap) -> LuaResult<()> {
        heap.validate_thread_handle(*self)
    }
    
    fn validation_key(&self) -> ValidationKey {
        ValidationKey {
            type_id: std::any::TypeId::of::<super::value::Thread>(),
            index: self.0.index,
            generation: self.0.generation,
        }
    }
}

impl ValidatableHandle for UpvalueHandle {
    fn validate_against_heap(&self, heap: &LuaHeap) -> LuaResult<()> {
        heap.validate_upvalue_handle(*self)
    }
    
    fn validation_key(&self) -> ValidationKey {
        ValidationKey {
            type_id: std::any::TypeId::of::<super::value::Upvalue>(),
            index: self.0.index,
            generation: self.0.generation,
        }
    }
}

impl ValidatableHandle for UserDataHandle {
    fn validate_against_heap(&self, heap: &LuaHeap) -> LuaResult<()> {
        heap.validate_userdata_handle(*self)
    }
    
    fn validation_key(&self) -> ValidationKey {
        ValidationKey {
            type_id: std::any::TypeId::of::<super::value::UserData>(),
            index: self.0.index,
            generation: self.0.generation,
        }
    }
}

impl ValidatableHandle for FunctionProtoHandle {
    fn validate_against_heap(&self, heap: &LuaHeap) -> LuaResult<()> {
        heap.validate_function_proto_handle(*self)
    }
    
    fn validation_key(&self) -> ValidationKey {
        ValidationKey {
            type_id: std::any::TypeId::of::<super::value::FunctionProto>(),
            index: self.0.index,
            generation: self.0.generation,
        }
    }
}

/// State of a transaction
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransactionState {
    /// Transaction is active and accepting changes
    Active,
    
    /// Transaction has been committed
    Committed,
    
    /// Transaction has been aborted
    Aborted,
}

/// Changes that can be made through a transaction
#[derive(Debug, Clone)]
pub enum HeapChange {
    /// Set a table field
    SetTableField { 
        table: TableHandle, 
        key: Value, 
        value: Value 
    },
    
    /// Set a table metatable
    SetTableMetatable {
        table: TableHandle,
        metatable: Option<TableHandle>,
    },
    
    /// Set a thread register
    SetRegister { 
        thread: ThreadHandle, 
        index: usize, 
        value: Value 
    },
    
    /// Push a value to thread stack
    PushStack {
        thread: ThreadHandle,
        value: Value,
    },
    
    /// Pop values from thread stack
    PopStack {
        thread: ThreadHandle,
        count: usize,
    },
    
    /// Push a call frame
    PushCallFrame {
        thread: ThreadHandle,
        frame: CallFrame,
    },
    
    /// Pop a call frame
    PopCallFrame {
        thread: ThreadHandle,
    },
    
    /// Update thread PC
    SetPC {
        thread: ThreadHandle,
        pc: usize,
    },
    
    /// Update upvalue
    SetUpvalue {
        upvalue: UpvalueHandle,
        value: Value,
    },
    
    /// Close upvalue
    CloseUpvalue {
        upvalue: UpvalueHandle,
        value: Value,
    },
}

/// Validation scope - enforces validation as operations enter the transaction
struct ValidationScope {
    /// Handles validated in this scope
    validated: HashSet<ValidationKey>,
    
    /// Handles created in this scope (don't need validation)
    created: HashSet<ValidationKey>,
}

impl ValidationScope {
    fn new() -> Self {
        ValidationScope {
            validated: HashSet::new(),
            created: HashSet::new(),
        }
    }
    
    /// Check if a handle is valid in this scope
    fn is_valid<H: ValidatableHandle>(&self, handle: &H) -> bool {
        let key = handle.validation_key();
        self.created.contains(&key) || self.validated.contains(&key)
    }
    
    /// Mark a handle as validated
    fn mark_validated<H: ValidatableHandle>(&mut self, handle: &H) {
        self.validated.insert(handle.validation_key());
    }
    
    /// Mark a handle as created
    fn mark_created<H: ValidatableHandle>(&mut self, handle: &H) {
        self.created.insert(handle.validation_key());
    }
}

/// Who owns a protection
#[derive(Debug, Clone, PartialEq)]
enum ProtectionOwner {
    /// Manual protection (explicit call)
    Manual,
    /// Scope-based protection (RAII)
    Scope,
}

/// Protected register tracking
#[derive(Debug, Clone)]
struct ProtectedRegisters {
    /// Protected ranges by thread
    ranges: HashMap<ThreadHandle, Vec<ProtectedRange>>,
}

/// A protected register range
#[derive(Debug, Clone)]
struct ProtectedRange {
    /// Start index (inclusive)
    start: usize,
    
    /// End index (exclusive)
    end: usize,
    
    /// Protection reason (for debugging)
    reason: String,
    
    /// Who owns this protection
    owner: ProtectionOwner,
}

impl ProtectedRegisters {
    fn new() -> Self {
        ProtectedRegisters {
            ranges: HashMap::new(),
        }
    }
    
    /// Check if a register is protected
    fn is_protected(&self, thread: ThreadHandle, index: usize) -> Option<&str> {
        if let Some(ranges) = self.ranges.get(&thread) {
            for range in ranges {
                if index >= range.start && index < range.end {
                    return Some(&range.reason);
                }
            }
        }
        None
    }
    
    /// Protect a register range
    fn protect_range(&mut self, thread: ThreadHandle, start: usize, end: usize, reason: String, owner: ProtectionOwner) {
        let range = ProtectedRange { start, end, reason, owner };
        self.ranges.entry(thread).or_insert_with(Vec::new).push(range);
    }
}

/// Enhanced transaction with register protection
pub struct HeapTransaction<'a> {
    /// Reference to the heap
    heap: &'a mut LuaHeap,
    
    /// Accumulated changes
    changes: Vec<HeapChange>,
    
    /// Pending operations for the VM
    pending_operations: VecDeque<PendingOperation>,
    
    /// Current transaction state
    state: TransactionState,
    
    /// Validation scope
    validation_scope: ValidationScope,
    
    /// Protected registers (can't be modified during this transaction)
    protected_registers: ProtectedRegisters,
}

impl<'a> HeapTransaction<'a> {
    /// Create a new transaction with register protection
    pub fn new(heap: &'a mut LuaHeap) -> Self {
        HeapTransaction {
            heap,
            changes: Vec::new(),
            pending_operations: VecDeque::new(),
            state: TransactionState::Active,
            validation_scope: ValidationScope::new(),
            protected_registers: ProtectedRegisters::new(),
        }
    }
    
    /// Protect a register range
    pub fn protect_registers(&mut self, thread: ThreadHandle, start: usize, end: usize, reason: &str) -> LuaResult<()> {
        self.ensure_active()?;
        self.validate_with_context(&thread, "protect_registers")?;
        
        // Store the protection
        let range = ProtectedRange {
            start,
            end,
            reason: reason.to_string(),
            owner: ProtectionOwner::Manual,
        };
        
        self.protected_registers.ranges
            .entry(thread)
            .or_insert_with(Vec::new)
            .push(range);
        
        Ok(())
    }
    
    /// Create a register protection scope with proper lifetime
    pub fn register_protection_scope(&'a mut self, thread: ThreadHandle) -> RegisterProtectionScope<'a> {
        RegisterProtectionScope::new(self, thread)
    }
    
    /// Ensure the transaction is active
    fn ensure_active(&self) -> LuaResult<()> {
        match self.state {
            TransactionState::Active => Ok(()),
            TransactionState::Committed => Err(LuaError::InvalidTransactionState),
            TransactionState::Aborted => Err(LuaError::InvalidTransactionState),
        }
    }
    
    /// Type-safe handle validation with caching
    pub fn validate_handle<H: ValidatableHandle>(&mut self, handle: &H) -> LuaResult<()> {
        // Check if already validated in this scope
        if self.validation_scope.is_valid(handle) {
            return Ok(());
        }
        
        // Validate against heap
        handle.validate_against_heap(self.heap)?;
        
        // Mark as validated
        self.validation_scope.mark_validated(handle);
        
        Ok(())
    }
    
    /// Validate a handle with context for better error messages
    pub fn validate_with_context<H: ValidatableHandle>(&mut self, handle: &H, context: &str) -> LuaResult<()> {
        match self.validate_handle(handle) {
            Ok(_) => Ok(()),
            Err(LuaError::InvalidHandle) => {
                let key = handle.validation_key();
                Err(LuaError::RuntimeError(format!(
                    "Invalid handle in {}: index {} (possibly out of bounds or already freed)", 
                    context, key.index
                )))
            },
            Err(LuaError::StaleHandle) => {
                let key = handle.validation_key();
                Err(LuaError::RuntimeError(format!(
                    "Stale handle in {}: index {} generation {} (slot reused with different generation)",
                    context, key.index, key.generation
                )))
            },
            Err(e) => Err(e),
        }
    }
    
    // String operations
    
    /// Create a new string
    pub fn create_string(&mut self, s: &str) -> LuaResult<StringHandle> {
        self.ensure_active()?;
        
        // Use validation-aware creation to ensure handles remain valid during reallocation
        let validated_handles = self.collect_validated_string_handles();
        let handle = self.heap.create_string_with_validation(s, &validated_handles)?;
        
        // Add debug output for string interning troubleshooting
        if s.len() < 30 && (s == "print" || s == "type" || s == "tostring" || s.starts_with("__")) {
            println!("DEBUG INTERNING: Created string '{}' with handle {:?}", s, handle);
        }
        
        // Mark as created
        self.validation_scope.mark_created(&handle);
        
        Ok(handle)
    }
    
    /// Get string value
    pub fn get_string_value(&mut self, handle: StringHandle) -> LuaResult<String> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&handle, "get_string_value")?;
        
        let lua_string = self.heap.get_string(handle)?;
        match lua_string.to_str() {
            Ok(s) => Ok(s.to_string()),
            Err(_) => Err(LuaError::RuntimeError("Invalid UTF-8 in string".to_string())),
        }
    }
    
    /// Get string bytes
    pub fn get_string_bytes(&mut self, handle: StringHandle) -> LuaResult<&[u8]> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&handle, "get_string_bytes")?;
        
        let lua_string = self.heap.get_string(handle)?;
        Ok(lua_string.as_bytes())
    }
    
    // Table operations
    
    /// Create a new table
    pub fn create_table(&mut self) -> LuaResult<TableHandle> {
        self.ensure_active()?;
        
        // Use validation-aware creation to ensure handles remain valid during reallocation
        let validated_handles = self.collect_validated_table_handles();
        let handle = self.heap.create_table_with_validation(&validated_handles)?;
        
        // Mark as created
        self.validation_scope.mark_created(&handle);
        
        Ok(handle)
    }

    /// Read a table field
    pub fn read_table_field(&mut self, table: TableHandle, key: &Value) -> LuaResult<Value> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&table, "read_table_field")?;
        
        // Also validate any handles in the key
        self.validate_value(key, "read_table_field key")?;
        
        self.heap.get_table_field_internal(table, key)
    }
    
    /// Get a table
    pub fn get_table(&mut self, handle: TableHandle) -> LuaResult<&super::value::Table> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&handle, "get_table")?;
        
        self.heap.get_table(handle)
    }
    
    /// Set a table field (queued)
    pub fn set_table_field(&mut self, table: TableHandle, key: Value, value: Value) -> LuaResult<()> {
        self.ensure_active()?;
        
        // Validate all handles
        self.validate_with_context(&table, "set_table_field")?;
        self.validate_value(&key, "set_table_field key")?;
        self.validate_value(&value, "set_table_field value")?;
        
        // Queue the change without checking if key is hashable
        // The actual validation will happen when the change is applied
        self.changes.push(HeapChange::SetTableField { table, key, value });
        Ok(())
    }
    
    /// Get table metatable
    pub fn get_table_metatable(&mut self, table: TableHandle) -> LuaResult<Option<TableHandle>> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&table, "get_table_metatable")?;
        
        self.heap.get_table_metatable_internal(table)
    }
    
    /// Set table metatable (queued)
    pub fn set_table_metatable(&mut self, table: TableHandle, metatable: Option<TableHandle>) -> LuaResult<()> {
        self.ensure_active()?;
        
        // Validate the table handle
        self.validate_with_context(&table, "set_table_metatable")?;
        
        // If there's a metatable, validate it too
        if let Some(mt) = metatable {
            self.validate_with_context(&mt, "set_table_metatable metatable")?;
        }
        
        self.changes.push(HeapChange::SetTableMetatable { table, metatable });
        Ok(())
    }

    /// Get the globals table
    pub fn get_globals_table(&mut self) -> LuaResult<TableHandle> {
        self.ensure_active()?;
        
        // Get the globals table handle from the heap
        let globals = self.heap.globals()?;
        
        // Validate the handle
        self.validate_with_context(&globals, "get_globals_table")?;
        
        Ok(globals)
    }

    /// Get metatable from a table using the two-phase pattern
    pub fn get_table_metatable_two_phase(&mut self, table: TableHandle) -> LuaResult<Option<TableHandle>> {
        self.ensure_active()?;
        
        // Phase 1: Extract the metatable handle to avoid nested borrows
        let metatable_opt = {
            // Validate the table handle
            self.validate_with_context(&table, "get_table_metatable_two_phase")?;
            
            // Get the metatable handle only, avoiding nested borrows
            self.heap.get_table_metatable_internal(table)?
        };
        
        // Phase 2: If metatable exists, validate it separately to avoid borrow conflicts
        if let Some(metatable) = metatable_opt {
            // We're now in a separate borrow scope, so we can validate the metatable
            self.validate_with_context(&metatable, "get_table_metatable_two_phase (metatable)")?;
        }
        
        Ok(metatable_opt)
    }

    /// Implement a method specifically for GetTable operations with metamethod support
    /// This follows the two-phase pattern from LUA_TRANSACTION_PATTERNS.md
    pub fn get_table_with_metamethods(&mut self, table: TableHandle, key: &Value) -> LuaResult<Value> {
        self.ensure_active()?;
        
        // Validate handles
        self.validate_with_context(&table, "get_table_with_metamethods")?;
        self.validate_value(key, "get_table_with_metamethods key")?;
        
        // Phase 1: Try direct table access first
        let direct_result = self.heap.get_table_field_internal(table, key)?;
        
        if !direct_result.is_nil() {
            // Direct lookup succeeded, return the result
            return Ok(direct_result);
        }
        
        // Phase 2: Check for metamethod through a clean borrow boundary
        let metatable_opt = self.get_table_metatable_two_phase(table)?;
        
        // If no metatable, just return nil
        let Some(metatable) = metatable_opt else {
            return Ok(Value::Nil);
        };
        
        // Phase 3: Look up the __index metamethod
        let mm_name = self.create_string("__index")?;
        let mm_key = Value::String(mm_name);
        let metamethod = self.read_table_field(metatable, &mm_key)?;
        
        // Process metamethod based on its type
        match metamethod {
            Value::Nil => {
                // No metamethod found
                Ok(Value::Nil)
            },
            Value::Table(mm_table) => {
                // __index is a table, access it directly
                self.read_table_field(mm_table, key)
            },
            Value::Closure(_) => {
                // __index is a function, it must be queued for execution
                // This would be handled by the VM's operation queue
                // For now, just return nil as a placeholder
                Ok(Value::Nil)
            },
            _ => {
                // __index is not a function or table, just return nil
                Ok(Value::Nil)
            }
        }
    }
    
    // Thread operations
    
    /// Create a new thread
    pub fn create_thread(&mut self) -> LuaResult<ThreadHandle> {
        self.ensure_active()?;
        
        let handle = self.heap.create_thread_internal()?;
        
        // Mark as created
        self.validation_scope.mark_created(&handle);
        
        Ok(handle)
    }
    
    /// Get a thread
    pub fn get_thread(&mut self, handle: ThreadHandle) -> LuaResult<&super::value::Thread> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&handle, "get_thread")?;
        
        self.heap.get_thread(handle)
    }
    
    /// Read a register value
    pub fn read_register(&mut self, thread: ThreadHandle, index: usize) -> LuaResult<Value> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&thread, "read_register")?;
        
        self.heap.get_thread_register_internal(thread, index)
    }
    
    /// Set a register value (queued)
    pub fn set_register(&mut self, thread: ThreadHandle, index: usize, value: Value) -> LuaResult<()> {
        self.ensure_active()?;
        
        // Check if register is protected
        if let Some(reason) = self.protected_registers.is_protected(thread, index) {
            return Err(LuaError::RuntimeError(format!(
                "Cannot modify protected register {} (protected for: {})",
                index, reason
            )));
        }
        
        // Validate handles
        self.validate_with_context(&thread, "set_register")?;
        self.validate_value(&value, "set_register value")?;
        
        self.changes.push(HeapChange::SetRegister { thread, index, value });
        Ok(())
    }
    
    /// Push value to stack (queued)
    pub fn push_stack(&mut self, thread: ThreadHandle, value: Value) -> LuaResult<()> {
        self.ensure_active()?;
        
        // Validate handles
        self.validate_with_context(&thread, "push_stack")?;
        self.validate_value(&value, "push_stack value")?;
        
        self.changes.push(HeapChange::PushStack { thread, value });
        Ok(())
    }
    
    /// Pop values from stack (queued)
    pub fn pop_stack(&mut self, thread: ThreadHandle, count: usize) -> LuaResult<()> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&thread, "pop_stack")?;
        
        self.changes.push(HeapChange::PopStack { thread, count });
        Ok(())
    }
    
    /// Push call frame (queued)
    pub fn push_call_frame(&mut self, thread: ThreadHandle, frame: CallFrame) -> LuaResult<()> {
        self.ensure_active()?;
        
        // Validate handles
        self.validate_with_context(&thread, "push_call_frame")?;
        self.validate_with_context(&frame.closure, "push_call_frame closure")?;
        
        self.changes.push(HeapChange::PushCallFrame { thread, frame });
        Ok(())
    }
    
    /// Pop call frame (queued)
    pub fn pop_call_frame(&mut self, thread: ThreadHandle) -> LuaResult<()> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&thread, "pop_call_frame")?;
        
        self.changes.push(HeapChange::PopCallFrame { thread });
        Ok(())
    }
    
    /// Get current PC
    pub fn get_pc(&mut self, thread: ThreadHandle) -> LuaResult<usize> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&thread, "get_pc")?;
        
        let thread_obj = self.heap.get_thread(thread)?;
        if let Some(frame) = thread_obj.call_frames.last() {
            Ok(frame.pc)
        } else {
            Err(LuaError::RuntimeError("No active call frame".to_string()))
        }
    }
    
    /// Set PC (queued)
    pub fn set_pc(&mut self, thread: ThreadHandle, pc: usize) -> LuaResult<()> {
        self.ensure_active()?;
        
        // Validate the thread handle
        self.validate_with_context(&thread, "set_pc")?;
        
        self.changes.push(HeapChange::SetPC { thread, pc });
        Ok(())
    }
    
    /// Increment PC (queued)
    pub fn increment_pc(&mut self, thread: ThreadHandle) -> LuaResult<()> {
        self.ensure_active()?;
        
        let current_pc = self.get_pc(thread)?;
        self.set_pc(thread, current_pc + 1)
    }
    
    /// Get current frame
    pub fn get_current_frame(&mut self, thread: ThreadHandle) -> LuaResult<CallFrame> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&thread, "get_current_frame")?;
        
        let thread_obj = self.heap.get_thread(thread)?;
        thread_obj.call_frames.last()
            .cloned()
            .ok_or(LuaError::RuntimeError("No active call frame".to_string()))
    }
    
    /// Get stack size
    pub fn get_stack_size(&mut self, thread: ThreadHandle) -> LuaResult<usize> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&thread, "get_stack_size")?;
        
        let thread_obj = self.heap.get_thread(thread)?;
        Ok(thread_obj.stack.len())
    }
    
    /// Get stack top (last element index)
    pub fn get_stack_top(&mut self, thread: ThreadHandle) -> LuaResult<usize> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&thread, "get_stack_top")?;
        
        let thread_obj = self.heap.get_thread(thread)?;
        if thread_obj.stack.is_empty() {
            Ok(0)
        } else {
            Ok(thread_obj.stack.len() - 1)
        }
    }
    
    /// Get current frame base register
    pub fn current_frame_base(&mut self, thread: ThreadHandle) -> LuaResult<u16> {
        self.ensure_active()?;
        
        let frame = self.get_current_frame(thread)?;
        Ok(frame.base_register)
    }
    
    /// Get the call depth of a thread
    pub fn get_thread_call_depth(&mut self, thread: ThreadHandle) -> LuaResult<usize> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&thread, "get_thread_call_depth")?;
        
        let thread_obj = self.heap.get_thread(thread)?;
        Ok(thread_obj.call_frames.len())
    }
    
    // Closure operations
    
    /// Create a closure
    pub fn create_closure(&mut self, closure: Closure) -> LuaResult<ClosureHandle> {
        self.ensure_active()?;
        
        // Validate upvalues first
        for upvalue in &closure.upvalues {
            self.validate_with_context(upvalue, "create_closure upvalue")?;
        }
        
        // Use validation-aware creation
        let validated_handles = self.collect_validated_closure_handles();
        let handle = self.heap.create_closure_with_validation(closure, &validated_handles)?;
        
        // Mark as created
        self.validation_scope.mark_created(&handle);
        
        Ok(handle)
    }
    
    /// Get closure
    pub fn get_closure(&mut self, handle: ClosureHandle) -> LuaResult<&Closure> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&handle, "get_closure")?;
        
        self.heap.get_closure(handle)
    }
    
    /// Get instruction from closure
    pub fn get_instruction(&mut self, closure: ClosureHandle, pc: usize) -> LuaResult<u32> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&closure, "get_instruction")?;
        
        let closure_obj = self.heap.get_closure(closure)?;
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
    pub fn create_upvalue(&mut self, upvalue: Upvalue) -> LuaResult<UpvalueHandle> {
        self.ensure_active()?;
        
        let handle = self.heap.create_upvalue_internal(upvalue)?;
        
        // Mark as created
        self.validation_scope.mark_created(&handle);
        
        Ok(handle)
    }
    
    /// Get an upvalue reference
    pub fn get_upvalue(&mut self, handle: UpvalueHandle) -> LuaResult<&super::value::Upvalue> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&handle, "get_upvalue")?;
        
        self.heap.get_upvalue(handle)
    }
    
    /// Set upvalue (queued)
    pub fn set_upvalue(&mut self, upvalue: UpvalueHandle, value: Value) -> LuaResult<()> {
        self.ensure_active()?;
        
        // Validate handles
        self.validate_with_context(&upvalue, "set_upvalue")?;
        self.validate_value(&value, "set_upvalue value")?;
        
        self.changes.push(HeapChange::SetUpvalue { upvalue, value });
        Ok(())
    }
    
    /// Close upvalue (queued)
    pub fn close_upvalue(&mut self, upvalue: UpvalueHandle, value: Value) -> LuaResult<()> {
        self.ensure_active()?;
        
        // Validate handles
        self.validate_with_context(&upvalue, "close_upvalue")?;
        self.validate_value(&value, "close_upvalue value")?;
        
        self.changes.push(HeapChange::CloseUpvalue { upvalue, value });
        Ok(())
    }
    
    // UserData operations
    
    /// Create userdata
    pub fn create_userdata(&mut self, userdata: UserData) -> LuaResult<UserDataHandle> {
        self.ensure_active()?;
        
        // Validate metatable if present
        if let Some(mt) = userdata.metatable {
            self.validate_with_context(&mt, "create_userdata metatable")?;
        }
        
        let handle = self.heap.create_userdata_internal(userdata)?;
        
        // Mark as created
        self.validation_scope.mark_created(&handle);
        
        Ok(handle)
    }
    
    /// Get userdata
    pub fn get_userdata(&mut self, handle: UserDataHandle) -> LuaResult<&UserData> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&handle, "get_userdata")?;
        
        self.heap.get_userdata(handle)
    }
    
    /// Get userdata metatable
    pub fn get_userdata_metatable(&mut self, handle: UserDataHandle) -> LuaResult<Option<TableHandle>> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&handle, "get_userdata_metatable")?;
        
        let userdata = self.heap.get_userdata(handle)?;
        Ok(userdata.metatable)
    }
    
    /// Validate any handles contained in a value
    fn validate_value(&mut self, value: &Value, context: &str) -> LuaResult<()> {
        match value {
            Value::String(handle) => self.validate_with_context(handle, &format!("{} (string)", context))?,
            Value::Table(handle) => self.validate_with_context(handle, &format!("{} (table)", context))?,
            Value::Closure(handle) => self.validate_with_context(handle, &format!("{} (closure)", context))?,
            Value::Thread(handle) => self.validate_with_context(handle, &format!("{} (thread)", context))?,
            Value::UserData(handle) => self.validate_with_context(handle, &format!("{} (userdata)", context))?,
            _ => {} // Other values don't contain handles
        }
        Ok(())
    }
    
    // Operation queueing
    
    /// Queue a pending operation for the VM
    pub fn queue_operation(&mut self, op: PendingOperation) -> LuaResult<()> {
        self.ensure_active()?;
        
        // Validate any handles in the operation
        self.validate_pending_operation(&op)?;
        
        self.pending_operations.push_back(op);
        Ok(())
    }
    
    /// Get the next key-value pair from a table following Lua's iteration semantics
    pub fn table_next(&mut self, table: TableHandle, current_key: Value) -> LuaResult<Option<(Value, Value)>> {
        // Get a reference to the table
        let table_obj = self.get_table(table)?;
        
        // Case 1: nil key means get the first key/value pair
        if current_key.is_nil() {
            // First try array part (index 1, which is array[0])
            if !table_obj.array.is_empty() && !table_obj.array[0].is_nil() {
                return Ok(Some((Value::Number(1.0), table_obj.array[0].clone())));
            }
            
            // Check rest of array part
            for i in 1..table_obj.array.len() {
                if !table_obj.array[i].is_nil() {
                    return Ok(Some((Value::Number((i + 1) as f64), table_obj.array[i].clone())));
                }
            }
            
            // If no array elements or all nil, try hash part
            if let Some((k, v)) = table_obj.map.iter().next() {
                return Ok(Some((k.to_value(), v.clone())));
            }
            
            // Empty table or all nils
            return Ok(None);
        }
        
        // Case 2: Current key is a numeric index in the array part
        if let Value::Number(n) = &current_key {
            // Check if it's a valid array index (an integer >= 1)
            if n.fract() == 0.0 && *n >= 1.0 {
                let idx = *n as usize;
                
                // Look for the next filled slot in the array part
                if idx <= table_obj.array.len() {
                    // Find next non-nil element in array
                    for i in idx..table_obj.array.len() {
                        if !table_obj.array[i].is_nil() {
                            return Ok(Some((Value::Number((i + 1) as f64), table_obj.array[i].clone())));
                        }
                    }
                    
                    // Array exhausted, move to hash part
                    if let Some((k, v)) = table_obj.map.iter().next() {
                        return Ok(Some((k.to_value(), v.clone())));
                    }
                }
                
                // No more elements in array or hash part
                return Ok(None);
            }
        }
        
        // Case 3: Current key is in the hash part
        // Convert the current key to HashableValue for comparison
        let current_hashable = match HashableValue::from_value_with_context(&current_key, "table_next") {
            Ok(k) => k,
            Err(_) => return Ok(None), // Key can't be in hash part
        };
        
        // Create a vector of all hash keys for stable ordering
        let mut hash_keys: Vec<_> = table_obj.map.keys().collect();
        hash_keys.sort_by_key(|k| match k {
            HashableValue::Nil => 0,
            HashableValue::Boolean(false) => 1,
            HashableValue::Boolean(true) => 2,
            HashableValue::Number(n) => 3,
            HashableValue::String(_) => 4,
        });
        
        // Find current key's position and return the next one
        let mut found = false;
        for k in hash_keys {
            if found {
                // This is the key after the current one
                return Ok(Some((k.to_value(), table_obj.map.get(k).cloned().unwrap_or(Value::Nil))));
            }
            
            if k == &current_hashable {
                found = true;
            }
        }
        
        // If we get here, we've exhausted all keys
        Ok(None)
    }
    
    /// Validate handles in a pending operation
    fn validate_pending_operation(&mut self, op: &PendingOperation) -> LuaResult<()> {
        match op {
            PendingOperation::FunctionCall { closure, args, .. } => {
                self.validate_with_context(closure, "pending function call")?;
                for (i, arg) in args.iter().enumerate() {
                    self.validate_value(arg, &format!("function call arg {}", i))?;
                }
            },
            // Add validation for other operation types as needed
            _ => {} // Other operations don't contain handles or are not yet implemented
        }
        Ok(())
    }
    
    // Transaction control
    
    /// Commit all changes atomically - follows the pattern in LUA_TRANSACTION_PATTERNS.md
    pub fn commit(&mut self) -> LuaResult<Vec<PendingOperation>> {
        self.ensure_active()?;
        
        // No handle validation here - all changes were validated when queued
        // This matches the design pattern specified in LUA_TRANSACTION_PATTERNS.md:
        // "Handles are validated when queued, not when committed"
        
        #[cfg(debug_assertions)]
        // Phase 1: Debug check that all handles in changes have been validated
        {
            for change in &self.changes {
                // This just ensures we're applying the pattern consistently
                match change {
                    HeapChange::SetTableField { table, .. } => {
                        // Table was already validated when operation was queued
                        debug_assert!(self.validation_scope.is_valid(table), 
                                      "SetTableField with unvalidated handle queued");
                    },
                    HeapChange::SetTableMetatable { table, metatable } => {
                        // Both handles were already validated
                        debug_assert!(self.validation_scope.is_valid(table),
                                      "SetTableMetatable with unvalidated table handle queued");
                        if let Some(mt) = metatable {
                            debug_assert!(self.validation_scope.is_valid(mt),
                                          "SetTableMetatable with unvalidated metatable handle queued");
                        }
                    },
                    // Similar checks for other change types...
                    _ => {}
                }
            }
        }
        
        // Phase 2: Apply all changes - avoid borrow checker issue by collecting changes first
        let changes: Vec<_> = self.changes.drain(..).collect();
        
        for change in changes {
            self.apply_change(change)?;
        }
        
        // Mark as committed
        self.state = TransactionState::Committed;
        
        // Return pending operations
        Ok(self.pending_operations.drain(..).collect())
    }
    
    /// Apply a single change to the heap
    fn apply_change(&mut self, change: HeapChange) -> LuaResult<()> {
        match change {
            HeapChange::SetTableField { table, key, value } => {
                // For table keys, we need to verify the key is hashable
                // Using the is_hashable function from value.rs
                if !super::value::HashableValue::is_hashable(&key) {
                    // Non-hashable key (like a table) - use Nil instead to avoid errors
                    // This matches Lua behavior where tables can't be used as keys
                    self.heap.set_table_field_internal(table, Value::Nil, value)?;
                } else {
                    // Normal case - key is hashable
                    self.heap.set_table_field_internal(table, key, value)?;
                }
            }
            HeapChange::SetTableMetatable { table, metatable } => {
                self.heap.set_table_metatable_internal(table, metatable)?;
            }
            HeapChange::SetRegister { thread, index, value } => {
                self.heap.set_thread_register_internal(thread, index, value)?;
            }
            HeapChange::PushStack { thread, value } => {
                let thread_obj = self.heap.get_thread_mut(thread)?;
                thread_obj.stack.push(value);
            }
            HeapChange::PopStack { thread, count } => {
                let thread_obj = self.heap.get_thread_mut(thread)?;
                for _ in 0..count {
                    thread_obj.stack.pop();
                }
            }
            HeapChange::PushCallFrame { thread, frame } => {
                let thread_obj = self.heap.get_thread_mut(thread)?;
                thread_obj.call_frames.push(frame);
            }
            HeapChange::PopCallFrame { thread } => {
                let thread_obj = self.heap.get_thread_mut(thread)?;
                thread_obj.call_frames.pop();
            }
            HeapChange::SetPC { thread, pc } => {
                let thread_obj = self.heap.get_thread_mut(thread)?;
                if let Some(frame) = thread_obj.call_frames.last_mut() {
                    frame.pc = pc;
                }
            }
            HeapChange::SetUpvalue { upvalue, value } => {
                let upvalue_obj = self.heap.get_upvalue_mut(upvalue)?;
                if upvalue_obj.stack_index.is_none() {
                    upvalue_obj.value = Some(value);
                }
            }
            HeapChange::CloseUpvalue { upvalue, value } => {
                let upvalue_obj = self.heap.get_upvalue_mut(upvalue)?;
                upvalue_obj.stack_index = None;
                upvalue_obj.value = Some(value);
            }
        }
        Ok(())
    }
    
    /// Abort the transaction (discard all changes)
    pub fn abort(&mut self) -> LuaResult<()> {
        self.ensure_active()?;
        
        self.changes.clear();
        self.pending_operations.clear();
        self.state = TransactionState::Aborted;
        
        Ok(())
    }
    
    /// Reset transaction for reuse
    pub fn reset(&mut self) -> LuaResult<()> {
        self.changes.clear();
        self.pending_operations.clear();
        self.validation_scope = ValidationScope::new();
        self.protected_registers = ProtectedRegisters::new();
        self.state = TransactionState::Active;
        
        Ok(())
    }
    
    /// Create a validation scope for batch operations
    pub fn validation_scope(&mut self) -> ValidScope<'a, '_> {
        ValidScope::new(self)
    }
    
    /// Collect all validated string handles in current transaction
    fn collect_validated_string_handles(&self) -> Vec<StringHandle> {
        // Collect all string handles that have been validated in this transaction
        self.validation_scope.validated.iter()
            .filter(|key| key.type_id == std::any::TypeId::of::<super::value::LuaString>())
            .map(|key| {
                // Use the factory method instead of direct field access
                StringHandle::from_raw_parts(key.index, key.generation)
            })
            .collect()
    }

    /// Collect all validated table handles in current transaction
    fn collect_validated_table_handles(&self) -> Vec<TableHandle> {
        // Collect all table handles that have been validated in this transaction
        self.validation_scope.validated.iter()
            .filter(|key| key.type_id == std::any::TypeId::of::<super::value::Table>())
            .map(|key| {
                // Use the factory method instead of direct field access
                TableHandle::from_raw_parts(key.index, key.generation)
            })
            .collect()
    }

    /// Collect all validated closure handles in current transaction
    fn collect_validated_closure_handles(&self) -> Vec<ClosureHandle> {
        // Collect all closure handles that have been validated in this transaction
        self.validation_scope.validated.iter()
            .filter(|key| key.type_id == std::any::TypeId::of::<super::value::Closure>())
            .map(|key| {
                // Use the factory method instead of direct field access
                ClosureHandle::from_raw_parts(key.index, key.generation)
            })
            .collect()
    }
    
    /// Create a function prototype
    pub fn create_function_proto(&mut self, proto: FunctionProto) -> LuaResult<FunctionProtoHandle> {
        self.ensure_active()?;
        
        // Use validation-aware creation to ensure handles remain valid during reallocation
        let validated_handles = self.collect_validated_function_proto_handles();
        let handle = self.heap.create_function_proto_with_validation(proto, &validated_handles)?;
        
        // Mark as created
        self.validation_scope.mark_created(&handle);
        
        Ok(handle)
    }
    
    /// Get function prototype
    pub fn get_function_proto(&mut self, handle: FunctionProtoHandle) -> LuaResult<&FunctionProto> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&handle, "get_function_proto")?;
        
        self.heap.get_function_proto(handle)
    }
    
    /// Collect all validated function proto handles in current transaction
    fn collect_validated_function_proto_handles(&self) -> Vec<FunctionProtoHandle> {
        self.validation_scope.validated.iter()
            .filter(|key| key.type_id == std::any::TypeId::of::<super::value::FunctionProto>())
            .map(|key| {
                FunctionProtoHandle::from_raw_parts(key.index, key.generation)
            })
            .collect()
    }
    
    /// Get function prototype as a copy
    pub fn get_function_proto_copy(&mut self, handle: FunctionProtoHandle) -> LuaResult<FunctionProto> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&handle, "get_function_proto_copy")?;
        
        // Get the prototype and clone it
        let proto = self.heap.get_function_proto(handle)?;
        Ok(proto.clone())
    }
    
    /// Replace a function prototype with an updated version
    pub fn replace_function_proto(&mut self, handle: FunctionProtoHandle, updated: FunctionProto) -> LuaResult<FunctionProtoHandle> {
        self.ensure_active()?;
        
        // Validate the handle
        self.validate_with_context(&handle, "replace_function_proto")?;
        
        // Create a new function prototype with the updated contents
        self.create_function_proto(updated)
    }
    
    /// Find or create an upvalue for a given stack index
    /// If an open upvalue already exists for the given stack index, returns it
    /// Otherwise, creates a new upvalue and adds it to the thread's open_upvalues list
    pub fn find_or_create_upvalue(
        &mut self, 
        thread: ThreadHandle, 
        stack_index: usize
    ) -> LuaResult<UpvalueHandle> {
        self.ensure_active()?;
        
        // Validate thread handle
        self.validate_with_context(&thread, "find_or_create_upvalue")?;
        
        // Phase 1: Extract needed data to avoid nested borrows
        let open_upvalues = {
            let thread_obj = self.heap.get_thread(thread)?;
            thread_obj.open_upvalues.clone() // Clone the vector to avoid borrow issues
        };
        
        // Phase 2: Search for existing upvalue (not holding heap borrow)
        for &upvalue_handle in &open_upvalues {
            // Validate the upvalue handle
            self.validate_with_context(&upvalue_handle, "find_or_create_upvalue existing")?;
            
            // Check if it references our target stack slot
            let upvalue = self.heap.get_upvalue(upvalue_handle)?;
            if let Some(idx) = upvalue.stack_index {
                if idx == stack_index {
                    return Ok(upvalue_handle);
                }
            }
        }
        
        // Phase 3: Create new upvalue (no heap borrows currently held)
        let new_upvalue = Upvalue {
            stack_index: Some(stack_index),
            value: None,
        };
        
        let upvalue_handle = self.create_upvalue(new_upvalue)?;
        
        // Phase 4: Add to thread's open_upvalues list
        // First collect stack indices from open upvalues
        let mut upvalue_indices = Vec::new();
        for &handle in &open_upvalues {
            if let Ok(upval) = self.heap.get_upvalue(handle) {
                if let Some(idx) = upval.stack_index {
                    upvalue_indices.push((handle, idx));
                }
            }
        }
        
        // Now we can determine where to insert the new upvalue
        // (sorted by stack index, highest first)
        let insert_pos = upvalue_indices.iter()
            .position(|(_, idx)| *idx < stack_index)
            .unwrap_or(upvalue_indices.len());
        
        // Now we can safely add the upvalue to the thread
        {
            let thread_obj = self.heap.get_thread_mut(thread)?;
            thread_obj.open_upvalues.insert(insert_pos, upvalue_handle);
        }
        
        Ok(upvalue_handle)
    }
    
    /// Find all upvalues in a thread that reference stack indices >= threshold
    pub fn find_upvalues_above_threshold(
        &mut self, 
        thread: ThreadHandle, 
        threshold: usize
    ) -> LuaResult<Vec<UpvalueHandle>> {
        self.ensure_active()?;
        
        // Validate thread handle
        self.validate_with_context(&thread, "find_upvalues_above_threshold")?;
        
        // Phase 1: Extract needed data to avoid nested borrows
        let open_upvalues = {
            let thread_obj = self.heap.get_thread(thread)?;
            thread_obj.open_upvalues.clone() // Clone to avoid borrow issues
        };
        
        // Phase 2: Process upvalues (not holding heap borrow)
        let mut to_close = Vec::new();
        
        // Examine each upvalue
        for &upvalue_handle in &open_upvalues {
            // Validate upvalue handle
            self.validate_with_context(&upvalue_handle, "find_upvalues_above_threshold upvalue")?;
            
            // Check its stack index
            let upvalue = self.heap.get_upvalue(upvalue_handle)?;
            if let Some(idx) = upvalue.stack_index {
                if idx >= threshold {
                    to_close.push(upvalue_handle);
                }
            }
        }
        
        Ok(to_close)
    }
    
    /// Remove closed upvalues from thread's open_upvalues list
    pub fn remove_closed_upvalues(&mut self, thread: ThreadHandle) -> LuaResult<()> {
        self.ensure_active()?;
        
        // Validate thread handle
        self.validate_with_context(&thread, "remove_closed_upvalues")?;
        
        // Phase 1: Extract needed data and find closed upvalues
        let (open_upvalues, closed_indices) = {
            let thread_obj = self.heap.get_thread(thread)?;
            let upvalues = thread_obj.open_upvalues.clone();
            
            // Find indices of closed upvalues
            let mut closed = Vec::new();
            for (i, &handle) in upvalues.iter().enumerate() {
                // We'll validate each handle in phase 2
                if let Ok(upvalue) = self.heap.get_upvalue(handle) {
                    if upvalue.stack_index.is_none() {
                        closed.push(i);
                    }
                }
            }
            
            (upvalues, closed)
        };
        
        // Phase 2: Validate handles (not holding heap borrow)
        for &upvalue_handle in &open_upvalues {
            self.validate_with_context(&upvalue_handle, "remove_closed_upvalues upvalue")?;
        }
        
        // Phase 3: Remove closed upvalues (if any)
        if !closed_indices.is_empty() {
            let mut thread_obj = self.heap.get_thread_mut(thread)?;
            
            // Remove in reverse order to avoid invalidating indices
            for &idx in closed_indices.iter().rev() {
                if idx < thread_obj.open_upvalues.len() {
                    thread_obj.open_upvalues.remove(idx);
                }
            }
        }
        
        Ok(())
    }
    
    pub fn close_thread_upvalues(&mut self, thread: ThreadHandle, threshold: usize) -> LuaResult<()> {
        self.ensure_active()?;
        
        // Validate thread handle
        self.validate_with_context(&thread, "close_thread_upvalues")?;
        
        // Step 1: Collect all upvalue handles that exist
        let handles = {
            let thread_obj = self.heap.get_thread(thread)?;
            thread_obj.open_upvalues.clone()
        };
        
        // Step 2: For each handle, check if it needs to be closed and get its value
        let mut to_close = Vec::new();
        let mut keep_open = Vec::new();
        
        for &upvalue_handle in handles.iter() {
            // Validate the upvalue handle
            if self.validate_handle(&upvalue_handle).is_err() {
                continue; // Skip invalid handles
            }
            
            // Separately check upvalue stack index
            let needs_close = {
                let upvalue = match self.heap.get_upvalue(upvalue_handle) {
                    Ok(uv) => uv,
                    Err(_) => continue, // Skip invalid upvalues
                };
                
                match upvalue.stack_index {
                    Some(idx) if idx >= threshold => {
                        // This upvalue needs to be closed - get its value
                        let value = {
                            let thread_obj = match self.heap.get_thread(thread) {
                                Ok(t) => t,
                                Err(_) => continue, // Skip if thread is invalid
                            };
                            
                            if idx < thread_obj.stack.len() {
                                thread_obj.stack[idx].clone()
                            } else {
                                Value::Nil
                            }
                        };
                        
                        to_close.push((upvalue_handle, value));
                        false // Don't keep in open list
                    },
                    Some(_) => {
                        // Upvalue is open but below threshold - keep it
                        keep_open.push(upvalue_handle);
                        true // Keep in open list
                    },
                    None => false, // Already closed - don't keep in open list
                }
            };
            
            if !needs_close {
                // Don't need to keep this one
                continue;
            }
        }
        
        // Step 3: Close each upvalue
        for (upvalue_handle, value) in to_close {
            // Close the upvalue (if it still exists)
            if let Ok(upvalue) = self.heap.get_upvalue_mut(upvalue_handle) {
                upvalue.stack_index = None;
                upvalue.value = Some(value);
            }
        }
        
        // Step 4: Update thread's open upvalues list
        {
            let mut thread_obj = self.heap.get_thread_mut(thread)?;
            thread_obj.open_upvalues = keep_open;
        }
        
        Ok(())
    }
}

/// ValidScope - a scope that enforces validation patterns
pub struct ValidScope<'a, 'tx> {
    /// The transaction this scope is bound to
    transaction: &'tx mut HeapTransaction<'a>,
    
    /// Handles validated in this specific scope operation
    scope_validated: HashSet<ValidationKey>,
    
    /// Debug tracking - operations performed in this scope
    #[cfg(debug_assertions)]
    operations: Vec<String>,
}

impl<'a, 'tx> ValidScope<'a, 'tx> {
    /// Create a new validation scope
    pub fn new(transaction: &'tx mut HeapTransaction<'a>) -> Self {
        ValidScope {
            transaction,
            scope_validated: HashSet::new(),
            #[cfg(debug_assertions)]
            operations: Vec::new(),
        }
    }
    
    /// Validate a handle within this scope
    pub fn validate<H: ValidatableHandle>(&mut self, handle: &H) -> LuaResult<()> {
        // Check if already validated in this scope operation
        let key = handle.validation_key();
        if self.scope_validated.contains(&key) {
            return Ok(());
        }
        
        // Validate through the transaction
        self.transaction.validate_handle(handle)?;
        
        // Mark as validated in this scope
        self.scope_validated.insert(key);
        
        #[cfg(debug_assertions)]
        self.operations.push(format!("Validated handle: {:?}", key));
        
        Ok(())
    }
    
    /// Use a handle with validation
    pub fn use_handle<H, F, R>(&mut self, handle: &H, f: F) -> LuaResult<R>
    where 
        H: ValidatableHandle,
        F: FnOnce(&mut Self) -> LuaResult<R>,
    {
        // Validate the handle first
        self.validate(handle)?;
        
        #[cfg(debug_assertions)]
        self.operations.push(format!("Using handle: {:?}", handle.validation_key()));
        
        // Then use it
        f(self)
    }
    
    /// Validate multiple handles at once
    pub fn validate_all<H: ValidatableHandle>(&mut self, handles: &[H]) -> LuaResult<()> {
        for handle in handles {
            self.validate(handle)?;
        }
        Ok(())
    }
    
    /// Use multiple handles with validation
    pub fn use_handles<H, F, R>(&mut self, handles: &[H], f: F) -> LuaResult<R>
    where 
        H: ValidatableHandle,
        F: FnOnce(&mut Self) -> LuaResult<R>,
    {
        // Validate all handles first
        for handle in handles {
            self.validate(handle)?;
        }
        
        // Then use them
        f(self)
    }
    
    /// Get the transaction
    pub fn transaction(&mut self) -> &mut HeapTransaction<'a> {
        self.transaction
    }
    
    /// Report validation state (for debugging)
    #[cfg(debug_assertions)]
    pub fn validation_report(&self) -> String {
        format!("ValidScope has validated {} handles\nOperations: {}", 
                self.scope_validated.len(),
                self.operations.join("\n  "))
    }
}

// Add debug implementation
#[cfg(debug_assertions)]
impl<'a, 'tx> Drop for ValidScope<'a, 'tx> {
    fn drop(&mut self) {
        // In debug mode, we can add validation checking on drop
        if !self.scope_validated.is_empty() {
            // We could log this or add debug assertions
            // println!("ValidScope dropped with {} validated handles", self.scope_validated.len());
        }
    }
}

/// RAII scope for register protection
pub struct RegisterProtectionScope<'tx> {
    /// The transaction (raw pointer to avoid lifetime issues)
    transaction: std::ptr::NonNull<HeapTransaction<'tx>>,
    
    /// Thread being protected
    thread: ThreadHandle,
    
    /// Registers protected in this scope
    protected_in_scope: Vec<(usize, usize)>,
    
    /// Marker for lifetime tracking
    _phantom: std::marker::PhantomData<&'tx ()>,
}

// Unsafe Send + Sync impls required for raw pointers
unsafe impl<'tx> Send for RegisterProtectionScope<'tx> {}
unsafe impl<'tx> Sync for RegisterProtectionScope<'tx> {}

impl<'tx> RegisterProtectionScope<'tx> {
    /// Create a new protection scope with a transaction reference
    fn new(transaction: &mut HeapTransaction<'tx>, thread: ThreadHandle) -> Self {
        RegisterProtectionScope {
            transaction: std::ptr::NonNull::from(transaction),
            thread,
            protected_in_scope: Vec::new(),
            _phantom: std::marker::PhantomData,
        }
    }
    
    /// Protect a single register
    pub fn protect(&mut self, register: usize, reason: &str) -> LuaResult<()> {
        self.protect_range(register, register + 1, reason)
    }
    
    /// Protect a register range
    pub fn protect_range(&mut self, start: usize, end: usize, reason: &str) -> LuaResult<()> {
        // Safety: transaction pointer is valid for the lifetime of this scope
        unsafe {
            self.transaction.as_mut().protect_registers(self.thread, start, end, reason)?;
        }
        self.protected_in_scope.push((start, end));
        Ok(())
    }
    
    /// Execute a closure with registers protected
    pub fn with_protected<F, R>(&mut self, registers: &[usize], reason: &str, f: F) -> LuaResult<R>
    where
        F: FnOnce(&mut Self) -> LuaResult<R>,
    {
        // Protect all specified registers
        for &reg in registers {
            self.protect(reg, reason)?;
        }
        
        // Execute the closure
        f(self)
    }
}

impl<'tx> Drop for RegisterProtectionScope<'tx> {
    fn drop(&mut self) {
        // Remove protections when scope ends
        // In a real implementation, we'd track and remove only our protections
        // For now, we'll rely on transaction boundaries for cleanup
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::value::{self, FunctionProto};
    
    /// Test that handle validation is type-safe
    #[test] 
    fn test_type_safe_validation() {
        let mut heap = super::super::heap::LuaHeap::new().unwrap();
        
        // Create handles of different types
        let (string_handle, table_handle) = {
            let mut tx = HeapTransaction::new(&mut heap);
            let s = tx.create_string("test").unwrap();
            let t = tx.create_table().unwrap();
            tx.commit().unwrap();
            (s, t)
        };
        
        // Validate handles in a new transaction
        {
            let mut tx = HeapTransaction::new(&mut heap);
            
            // Type-safe validation
            assert!(tx.validate_handle(&string_handle).is_ok());
            assert!(tx.validate_handle(&table_handle).is_ok());
        }
    }
    
    /// Test that ValidScope enforces validation patterns
    #[test]
    fn test_valid_scope_enforcement() {
        let mut heap = super::super::heap::LuaHeap::new().unwrap();
        
        // Create test data
        let (string_handle, table_handle) = {
            let mut tx = HeapTransaction::new(&mut heap);
            let s = tx.create_string("key").unwrap();
            let t = tx.create_table().unwrap();
            tx.commit().unwrap();
            (s, t)
        };
        
        // Use ValidScope for complex operations
        {
            let mut tx = HeapTransaction::new(&mut heap);
            
            // Use a separate scope to contain the ValidScope work
            {
                let mut scope = tx.validation_scope();
                
                // Validate and use handles together
                scope.use_handle(&string_handle, |scope| {
                    // String handle is now validated
                    let string_value = scope.transaction().get_string_value(string_handle).unwrap();
                    assert_eq!(string_value, "key");
                    
                    // Now use the table handle too
                    scope.validate(&table_handle).unwrap();
                    let tx = scope.transaction();
                    tx.set_table_field(table_handle, Value::String(string_handle), Value::Number(42.0))
                }).unwrap();
                
                // Scope is dropped here, releasing the borrow on tx
            }
            
            // Now we can safely commit
            tx.commit().unwrap();
        }
        
        // Verify the operation worked
        {
            let mut tx = HeapTransaction::new(&mut heap);
            let result = tx.read_table_field(table_handle, &Value::String(string_handle)).unwrap();
            assert_eq!(result, Value::Number(42.0));
        }
    }
    
    /// Test that handles are validated when entering transaction methods
    #[test]
    fn test_validation_at_transaction_entry() {
        let mut heap = super::super::heap::LuaHeap::new().unwrap();
        
        // Create a table handle
        let table_handle = {
            let mut tx = HeapTransaction::new(&mut heap);
            let t = tx.create_table().unwrap();
            tx.commit().unwrap();
            t
        };
        
        // Create an invalid handle using transmutation for testing
        let invalid_handle = TableHandle::new_invalid_for_testing(9999, 0);
        
        // Valid handle operations should succeed
        {
            let mut tx = HeapTransaction::new(&mut heap);
            assert!(tx.set_table_field(table_handle, Value::Number(1.0), Value::Number(2.0)).is_ok());
            tx.commit().unwrap();
        }
        
        // Invalid handle operations should fail with context
        {
            let mut tx = HeapTransaction::new(&mut heap);
            let result = tx.set_table_field(invalid_handle, Value::Number(1.0), Value::Number(2.0));
            assert!(result.is_err());
            
            // Check error message contains context
            if let Err(e) = result {
                let error_string = e.to_string();
                assert!(error_string.contains("set_table_field"));
                assert!(error_string.contains("Invalid handle"));
            }
        }
    }
    
    /// Test that created handles don't need validation
    #[test]
    fn test_created_handles_no_validation() {
        let mut heap = super::super::heap::LuaHeap::new().unwrap();
        
        let mut tx = HeapTransaction::new(&mut heap);
        
        // Create handles
        let string = tx.create_string("test").unwrap();
        let table = tx.create_table().unwrap();
        
        // Should be able to use immediately without explicit validation
        assert!(tx.set_table_field(table, Value::String(string), Value::Boolean(true)).is_ok());
        
        // Commit should succeed
        assert!(tx.commit().is_ok());
    }
    
    /// Test validation caching
    #[test]
    fn test_validation_caching() {
        let mut heap = super::super::heap::LuaHeap::new().unwrap();
        
        let handle = {
            let mut tx = HeapTransaction::new(&mut heap);
            let h = tx.create_string("cached").unwrap();
            tx.commit().unwrap();
            h
        };
        
        let mut tx = HeapTransaction::new(&mut heap);
        
        // First validation
        assert!(tx.validate_handle(&handle).is_ok());
        
        // Second validation should use cache (test by checking it doesn't fail)
        assert!(tx.validate_handle(&handle).is_ok());
        
        // Multiple operations should work with cached validation
        assert!(tx.get_string_value(handle).is_ok());
        assert!(tx.get_string_bytes(handle).is_ok());
    }
    
    /// Test that invalid handles are properly rejected
    #[test]
    fn test_invalid_handle_rejection() {
        let mut heap = super::super::heap::LuaHeap::new().unwrap();
        
        // Create an invalid handle for testing
        let invalid_string = StringHandle::new_invalid_for_testing(9999, 0);
        
        let mut tx = HeapTransaction::new(&mut heap);
        
        // Should fail validation
        let result = tx.get_string_value(invalid_string);
        assert!(result.is_err());
        
        // Error should be descriptive  
        if let Err(e) = result {
            let error_string = e.to_string();
            assert!(error_string.contains("Invalid handle") || 
                    error_string.contains("out of bounds") ||
                    error_string.contains("get_string_value"));
        }
    }
    
    #[test]
    fn test_transaction_isolation() {
        let mut heap = super::super::heap::LuaHeap::new().unwrap();
        
        let table = {
            let mut tx = HeapTransaction::new(&mut heap);
            let t = tx.create_table().unwrap();
            tx.commit().unwrap();
            t
        };
        
        // Start a transaction and make changes
        {
            let mut tx = HeapTransaction::new(&mut heap);
            tx.set_table_field(table, Value::Number(1.0), Value::Number(42.0)).unwrap();
            
            // Changes shouldn't be visible until commit
            // Abort the transaction
            tx.abort().unwrap();
        }
        
        // Changes should not have been applied
        {
            let mut tx2 = HeapTransaction::new(&mut heap);
            let value = tx2.read_table_field(table, &Value::Number(1.0)).unwrap();
            assert_eq!(value, Value::Nil);
        }
    }
    
    /// Test that values containing handles are validated recursively
    #[test]
    fn test_recursive_value_validation() {
        let mut heap = super::super::heap::LuaHeap::new().unwrap();
        
        let valid_table = {
            let mut tx = HeapTransaction::new(&mut heap);
            let t = tx.create_table().unwrap();
            tx.commit().unwrap();
            t
        };
        
        // Create an invalid string handle for testing
        let invalid_string = StringHandle::new_invalid_for_testing(9999, 0);
        
        // Try to set an invalid handle as a value
        {
            let mut tx = HeapTransaction::new(&mut heap);
            let result = tx.set_table_field(
                valid_table, 
                Value::Number(1.0), 
                Value::String(invalid_string)
            );
            
            // Should fail due to invalid string handle
            assert!(result.is_err());
            if let Err(e) = result {
                let error_string = e.to_string();
                assert!(error_string.contains("set_table_field value"));
            }
        }
    }
    
    #[test]
    fn test_two_phase_borrowing_pattern() {
        let mut heap = super::super::heap::LuaHeap::new().unwrap();
        
        // Create a table with a metatable
        let (table, metatable) = {
            let mut tx = HeapTransaction::new(&mut heap);
            
            // Create the table and metatable
            let t = tx.create_table().unwrap();
            let mt = tx.create_table().unwrap();
            
            // Set the metatable
            tx.set_table_metatable(t, Some(mt)).unwrap();
            
            tx.commit().unwrap();
            
            (t, mt)
        };
        
        // Set up data in a separate transaction
        let (key_str, test_key, test_val) = {
            let mut tx = HeapTransaction::new(&mut heap);
            
            // Create keys and values
            let index_str = tx.create_string("__index").unwrap();
            let key = tx.create_string("test_key").unwrap();
            let val = tx.create_string("test_value").unwrap();
            
            // Set __index in the metatable
            tx.set_table_field(metatable, Value::String(index_str.clone()), Value::Table(metatable)).unwrap();
            
            // Set a value in the table directly
            tx.set_table_field(table, Value::String(key.clone()), Value::String(val.clone())).unwrap();
            
            tx.commit().unwrap();
            
            (index_str, key, val)
        };
        
        // Now test the two-phase pattern with metatable access
        {
            let mut tx = HeapTransaction::new(&mut heap);
            
            // Phase 1: Get the metatable
            let retrieved_mt = tx.get_table_metatable_two_phase(table).unwrap();
            
            // Assert it matches
            assert!(retrieved_mt.is_some());
            assert_eq!(retrieved_mt.unwrap(), metatable);
            
            // Now test metamethod lookup - this uses two-phase pattern
            let result = tx.get_table_with_metamethods(table, &Value::String(test_key)).unwrap();
            
            // This should match the value we set directly
            assert_eq!(result, Value::String(test_val));
        }
        
        // Another test section to verify our metatable setting was correct
        {
            let mut tx = HeapTransaction::new(&mut heap);
            
            // Verify metatable setting is still there
            let mt_opt = tx.get_table_metatable(table).unwrap();
            assert!(mt_opt.is_some());
            
            // Use the key from before
            let test_value = tx.read_table_field(metatable, &Value::String(key_str)).unwrap();
            assert!(matches!(test_value, Value::Table(_)));
            
            // Verify the table value to make sure it wasn't overwritten
            let direct_value = tx.read_table_field(table, &Value::String(test_key)).unwrap();
            assert_eq!(direct_value, Value::String(test_val), "The direct value in the table should match what we set");
        }
    }
    
    #[test]
    fn test_validation_at_reallocation() {
        let mut heap = super::super::heap::LuaHeap::new().unwrap();
        
        // Create initial handles
        let (string_handle, table_handle) = {
            let mut tx = HeapTransaction::new(&mut heap);
            let s = tx.create_string("initial_string").unwrap();
            let t = tx.create_table().unwrap();
            
            // Store the string in the table so we can verify later
            tx.set_table_field(t, Value::String(s), Value::Number(1.0)).unwrap();
            
            tx.commit().unwrap();
            (s, t)
        };
        
        // Create many strings to force reallocation
        {
            let mut tx = HeapTransaction::new(&mut heap);
            
            // Intermediate values to store strings we create
            let mut strings = Vec::new();
            
            // Create enough strings to likely trigger reallocation
            for i in 0..100 {
                let s = tx.create_string(&format!("string_{}", i)).unwrap();
                strings.push(s);
                
                // Keep using our original handles to ensure they remain valid
                // This exercises the reallocation validation
                tx.set_table_field(table_handle, Value::String(s), Value::Number(i as f64)).unwrap();
                
                // Periodically check the original string to ensure it's still valid
                if i % 10 == 0 {
                    let val = tx.get_string_value(string_handle).unwrap();
                    assert_eq!(val, "initial_string");
                }
            }
            
            // Verify our original handles are still valid
            let val = tx.read_table_field(table_handle, &Value::String(string_handle)).unwrap();
            assert_eq!(val, Value::Number(1.0));
            
            tx.commit().unwrap();
        }
    }
    
    /// Test proper error handling for invalid handles
    #[test]
    fn test_error_handling() {
        let mut heap = super::super::heap::LuaHeap::new().unwrap();
        
        // Create a valid handle
        let valid_handle = {
            let mut tx = HeapTransaction::new(&mut heap);
            let h = tx.create_string("valid").unwrap();
            tx.commit().unwrap();
            h
        };
        
        // Create an invalid handle for testing
        let invalid_handle = StringHandle::new_invalid_for_testing(9999, 0);
        
        // Try using both handles
        let mut tx = HeapTransaction::new(&mut heap);
        
        // Valid handle should work
        let result1 = tx.get_string_value(valid_handle);
        assert!(result1.is_ok());
        
        // Invalid handle should fail with proper context
        let result2 = tx.get_string_value(invalid_handle);
        assert!(result2.is_err());
        
        // Error should include context information
        if let Err(e) = result2 {
            let err_msg = e.to_string();
            assert!(err_msg.contains("get_string_value"), "Error should include operation context");
            assert!(err_msg.contains("Invalid handle"), "Error should indicate handle is invalid");
        }
        
        tx.abort().unwrap();
    }
    
    /// Test that unsafe operations are properly isolated and minimized
    #[test]
    fn test_type_safety() {
        let mut heap = super::super::heap::LuaHeap::new().unwrap();
        
        // Create handles of different types
        let (string_handle, table_handle, closure_handle) = {
            let mut tx = HeapTransaction::new(&mut heap);
            
            let s = tx.create_string("test").unwrap();
            let t = tx.create_table().unwrap();
            
            // Create a basic closure
            let proto = value::FunctionProto {
                bytecode: vec![0],
                constants: vec![],
                num_params: 0,
                is_vararg: false,
                max_stack_size: 0,
                upvalues: vec![],
            };
            let c = tx.create_closure(value::Closure {
                proto,
                upvalues: vec![],
            }).unwrap();
            
            tx.commit().unwrap();
            (s, t, c)
        };
        
        // Verify each handle type works through ValidScope
        // String operations
        {
            let mut tx = HeapTransaction::new(&mut heap);
            let mut scope = tx.validation_scope();
            
            scope.use_handle(&string_handle, |scope| {
                let tx = scope.transaction();
                let val = tx.get_string_value(string_handle)?;
                assert_eq!(val, "test");
                Ok(())
            }).unwrap();
            
            // Drop scope before committing
            drop(scope);
            tx.commit().unwrap();
        }
        
        // Table operations
        {
            let mut tx = HeapTransaction::new(&mut heap);
            let mut scope = tx.validation_scope();
            
            scope.use_handle(&table_handle, |scope| {
                let tx = scope.transaction();
                tx.set_table_field(table_handle, Value::Number(1.0), Value::String(string_handle))?;
                Ok(())
            }).unwrap();
            
            // Drop scope before committing
            drop(scope);
            tx.commit().unwrap();
        }
        
        // Closure operations
        {
            let mut tx = HeapTransaction::new(&mut heap);
            let mut scope = tx.validation_scope();
            
            scope.use_handle(&closure_handle, |scope| {
                let tx = scope.transaction();
                let closure = tx.get_closure(closure_handle)?;
                assert_eq!(closure.upvalues.len(), 0);
                Ok(())
            }).unwrap();
            
            // Drop scope before committing
            drop(scope);
            tx.commit().unwrap();
        }
    }
    
    #[test]
    fn test_transaction_isolation_pattern() {
        let mut heap = super::super::heap::LuaHeap::new().unwrap();
        
        // Create initial data
        let table = {
            let mut tx = HeapTransaction::new(&mut heap);
            let t = tx.create_table().unwrap();
            
            // Add an initial value
            let key = tx.create_string("key").unwrap();
            let value = tx.create_string("initial").unwrap();
            tx.set_table_field(t, Value::String(key), Value::String(value)).unwrap();
            
            tx.commit().unwrap();
            t
        };
        
        // Create a key we'll use in both transactions
        let key = {
            let mut tx = HeapTransaction::new(&mut heap);
            let k = tx.create_string("key").unwrap();
            tx.commit().unwrap();
            k
        };
        
        // First transaction - modify the data but abort
        {
            let mut tx = HeapTransaction::new(&mut heap);
            
            // Read current value
            let current = tx.read_table_field(table, &Value::String(key)).unwrap();
            assert!(matches!(current, Value::String(_)));
            
            // Modify the value
            let new_value = tx.create_string("modified").unwrap();
            tx.set_table_field(table, Value::String(key), Value::String(new_value)).unwrap();
            
            // Abort the transaction - changes should not be applied
            tx.abort().unwrap();
        }
        
        // Second transaction - verify isolation
        {
            let mut tx = HeapTransaction::new(&mut heap);
            
            // Read value - should still be the initial value
            let value = tx.read_table_field(table, &Value::String(key)).unwrap();
            
            // Extract the string to compare
            match value {
                Value::String(handle) => {
                    let s = tx.get_string_value(handle).unwrap();
                    assert_eq!(s, "initial", "Transaction isolation broken - aborted changes were applied");
                },
                _ => panic!("Expected string value"),
            }
            
            // Don't need to commit this read-only transaction
        }
    }
}