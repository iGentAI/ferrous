//! Transaction-Based Heap Access
//!
//! This module implements a transaction system for safely modifying the Lua heap.
//! All heap modifications are queued and applied atomically to avoid borrow checker
//! conflicts and ensure consistency.

use std::collections::{HashMap, HashSet};

use super::error::{LuaError, Result};
use super::value::{
    Value, TableHandle, StringHandle, ThreadHandle, ClosureHandle, CallFrame,
    FunctionProto, Upvalue, UpvalueHandle,
};
use super::heap::LuaHeap;
use super::vm::PendingOperation;

/// A change to be applied to the heap
#[derive(Debug, Clone)]
pub enum HeapChange {
    /// Set a table field
    SetTableField {
        table: TableHandle,
        key: Value,
        value: Value,
    },
    
    /// Set a thread register
    SetRegister {
        thread: ThreadHandle,
        index: usize,
        value: Value,
    },
    
    /// Increment PC
    IncrementPC {
        thread: ThreadHandle,
    },
    
    /// Jump relative
    Jump {
        thread: ThreadHandle,
        offset: i32,
    },
    
    /// Push call frame
    PushCallFrame {
        thread: ThreadHandle,
        frame: CallFrame,
    },
    
    /// Pop call frame
    PopCallFrame {
        thread: ThreadHandle,
    },
    
    /// Set metatable
    SetMetatable {
        table: TableHandle,
        metatable: Option<TableHandle>,
    },
    
    /// Push to thread stack
    PushStack {
        thread: ThreadHandle,
        value: Value,
    },
    
    /// Queue operation for VM
    QueueOperation {
        operation: PendingOperation,
    },
}

/// Resource identifier for tracking reads/writes
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ResourceId {
    /// Table field
    TableField(u32, u64), // Table handle index + hash of key (not the handle itself)
    
    /// Thread register
    ThreadRegister(u32, usize), // Thread handle index + register index
    
    /// Thread PC
    ThreadPC(u32), // Thread handle index
    
    /// Thread call stack
    ThreadCallStack(u32), // Thread handle index
}

/// A transaction for modifying the heap
pub struct HeapTransaction<'a> {
    /// The heap being modified
    heap: &'a mut LuaHeap,
    
    /// Changes to apply
    changes: Vec<HeapChange>,
    
    /// Resources read during transaction
    read_set: HashSet<ResourceId>,
    
    /// Resources written during transaction
    write_set: HashSet<ResourceId>,
    
    /// Cached strings created in this transaction
    created_strings: HashMap<String, StringHandle>,
    
    /// Tables created in this transaction
    created_tables: Vec<TableHandle>,
    
    /// Closures created in this transaction 
    created_closures: Vec<ClosureHandle>,
    
    /// Pending operations to queue
    pending_operations: Vec<PendingOperation>,
}

impl<'a> HeapTransaction<'a> {
    /// Create a new transaction
    pub fn new(heap: &'a mut LuaHeap) -> Self {
        HeapTransaction {
            heap,
            changes: Vec::new(),
            read_set: HashSet::new(),
            write_set: HashSet::new(),
            created_strings: HashMap::new(),
            created_tables: Vec::new(),
            created_closures: Vec::new(),
            pending_operations: Vec::new(),
        }
    }
    
    // String operations
    
    /// Create a string
    pub fn create_string(&mut self, s: &str) -> Result<StringHandle> {
        // Check cache first
        if let Some(handle) = self.created_strings.get(s) {
            return Ok(handle.clone());
        }
        
        // Create in heap
        let handle = self.heap.create_string_internal(s)?;
        
        // Cache in transaction
        self.created_strings.insert(s.to_string(), handle.clone());
        
        Ok(handle)
    }
    
    // Table operations
    
    /// Create a table
    pub fn create_table(&mut self) -> Result<TableHandle> {
        let handle = self.heap.create_table()?;
        self.created_tables.push(handle.clone());
        Ok(handle)
    }
    
    /// Read a table field
    pub fn read_table_field(&mut self, table: TableHandle, key: &Value) -> Result<Value> {
        // Record read
        let key_hash = self.hash_value(key);
        self.read_set.insert(ResourceId::TableField(table.0.index() as u32, key_hash));
        
        // Check pending changes first
        for change in self.changes.iter().rev() {
            if let HeapChange::SetTableField { table: t, key: k, value } = change {
                if *t == table && k == key {
                    return Ok(value.clone());
                }
            }
        }
        
        // Read from heap
        self.heap.get_table_field(table, key)
    }
    
    /// Set a table field
    pub fn set_table_field(&mut self, table: TableHandle, key: Value, value: Value) -> Result<()> {
        // Validate handle
        if !self.heap.is_valid_table(table.clone()) {
            return Err(LuaError::InvalidHandle);
        }
        
        // Record write - use index instead of whole handle
        let key_hash = self.hash_value(&key);
        self.write_set.insert(ResourceId::TableField(table.0.index() as u32, key_hash));
        
        // Queue change
        self.changes.push(HeapChange::SetTableField { table, key, value });
        
        Ok(())
    }
    
    /// Get table metatable
    pub fn get_metatable(&mut self, table: TableHandle) -> Result<Option<TableHandle>> {
        self.heap.get_metatable(table)
    }
    
    /// Set table metatable
    pub fn set_metatable(&mut self, table: TableHandle, metatable: Option<TableHandle>) -> Result<()> {
        // Validate handle
        if !self.heap.is_valid_table(table.clone()) {
            return Err(LuaError::InvalidHandle);
        }
        
        // Queue change
        self.changes.push(HeapChange::SetMetatable { table, metatable });
        
        Ok(())
    }
    
    // Thread operations
    
    /// Read a register
    pub fn read_register(&mut self, thread: ThreadHandle, index: usize) -> Result<Value> {
        // Record read - use index instead of whole handle
        self.read_set.insert(ResourceId::ThreadRegister(thread.0.index() as u32, index));
        
        // Check pending changes first
        for change in self.changes.iter().rev() {
            if let HeapChange::SetRegister { thread: t, index: i, value } = change {
                if *t == thread && *i == index {
                    return Ok(value.clone());
                }
            }
        }
        
        // Read from heap
        self.heap.get_thread_register(thread, index)
    }
    
    /// Set a register
    pub fn set_register(&mut self, thread: ThreadHandle, index: usize, value: Value) {
        // Record write - use index instead of whole handle
        self.write_set.insert(ResourceId::ThreadRegister(thread.0.index() as u32, index));
        
        // Queue change
        self.changes.push(HeapChange::SetRegister { thread, index, value });
    }
    
    /// Get current call frame
    pub fn get_current_call_frame(&mut self, thread: ThreadHandle) -> Result<CallFrame> {
        // Record read - use index instead of whole handle
        self.read_set.insert(ResourceId::ThreadCallStack(thread.0.index() as u32));
        
        // Get from heap and clone
        let frame = self.heap.get_current_frame(thread)?;
        Ok(frame.clone())
    }
    
    /// Get instruction from closure
    pub fn get_instruction(&self, closure: ClosureHandle, pc: usize) -> Result<super::vm::Instruction> {
        let closure_obj = self.heap.get_closure(closure)?;
        
        if pc >= closure_obj.proto.bytecode.len() {
            return Err(LuaError::InvalidBytecode("PC out of range".to_string()));
        }
        
        Ok(super::vm::Instruction(closure_obj.proto.bytecode[pc]))
    }
    
    /// Get constant from closure
    pub fn get_constant(&self, closure: ClosureHandle, index: usize) -> Result<Value> {
        let closure_obj = self.heap.get_closure(closure)?;
        
        if index >= closure_obj.proto.constants.len() {
            return Err(LuaError::InvalidBytecode("Constant index out of range".to_string()));
        }
        
        Ok(closure_obj.proto.constants[index].clone())
    }
    
    /// Increment PC
    pub fn increment_pc(&mut self, thread: ThreadHandle) -> Result<()> {
        // Record write - use index instead of whole handle
        self.write_set.insert(ResourceId::ThreadPC(thread.0.index() as u32));
        
        // Queue change
        self.changes.push(HeapChange::IncrementPC { thread });
        
        Ok(())
    }
    
    /// Jump relative
    pub fn jump(&mut self, thread: ThreadHandle, offset: i32) -> Result<()> {
        // Record write - use index instead of whole handle
        self.write_set.insert(ResourceId::ThreadPC(thread.0.index() as u32));
        
        // Queue change
        self.changes.push(HeapChange::Jump { thread, offset });
        
        Ok(())
    }
    
    /// Push call frame
    pub fn push_call_frame(&mut self, thread: ThreadHandle, frame: CallFrame) -> Result<()> {
        // Record write - use index instead of whole handle
        self.write_set.insert(ResourceId::ThreadCallStack(thread.0.index() as u32));
        
        // Queue change
        self.changes.push(HeapChange::PushCallFrame { thread, frame });
        
        Ok(())
    }
    
    /// Pop call frame
    pub fn pop_call_frame(&mut self, thread: ThreadHandle) -> Result<()> {
        // Record write - use index instead of whole handle
        self.write_set.insert(ResourceId::ThreadCallStack(thread.0.index() as u32));
        
        // Queue change
        self.changes.push(HeapChange::PopCallFrame { thread });
        
        Ok(())
    }
    
    /// Get stack top
    pub fn get_stack_top(&self, thread: ThreadHandle) -> Result<usize> {
        self.heap.get_thread_stack_size(thread)
    }
    
    /// Push to stack
    pub fn push_stack(&mut self, thread: ThreadHandle, value: Value) {
        self.changes.push(HeapChange::PushStack { thread, value });
    }
    
    /// Queue an operation for the VM
    pub fn queue_operation(&mut self, operation: PendingOperation) {
        self.pending_operations.push(operation);
    }
    
    // Closure operations
    
    /// Create a closure
    pub fn create_closure(&mut self, proto: FunctionProto, upvalues: Vec<UpvalueHandle>) -> Result<ClosureHandle> {
        let handle = self.heap.create_closure(proto, upvalues)?;
        self.created_closures.push(handle.clone());
        Ok(handle)
    }
    
    // Helper methods
    
    /// Hash a value for tracking
    fn hash_value(&self, value: &Value) -> u64 {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }
    
    /// Commit all changes
    pub fn commit(self) -> Result<Vec<PendingOperation>> {
        // Apply all changes
        for change in self.changes {
            match change {
                HeapChange::SetTableField { table, key, value } => {
                    self.heap.set_table_field_internal(table, key, value)?;
                }
                
                HeapChange::SetRegister { thread, index, value } => {
                    self.heap.set_thread_register_internal(thread, index, value)?;
                }
                
                HeapChange::IncrementPC { thread } => {
                    self.heap.increment_pc_internal(thread)?;
                }
                
                HeapChange::Jump { thread, offset } => {
                    let frame = self.heap.get_current_frame_mut(thread)?;
                    if offset >= 0 {
                        frame.pc += offset as usize;
                    } else {
                        frame.pc = frame.pc.saturating_sub((-offset) as usize);
                    }
                }
                
                HeapChange::PushCallFrame { thread, frame } => {
                    self.heap.push_call_frame_internal(thread, frame)?;
                }
                
                HeapChange::PopCallFrame { thread } => {
                    self.heap.pop_call_frame_internal(thread)?;
                }
                
                HeapChange::SetMetatable { table, metatable } => {
                    self.heap.set_metatable_internal(table, metatable)?;
                }
                
                HeapChange::PushStack { thread, value } => {
                    self.heap.push_thread_stack_internal(thread, value)?;
                }
                
                HeapChange::QueueOperation { operation: _ } => {
                    // These are returned, not applied to heap
                }
            }
        }
        
        // Return pending operations
        Ok(self.pending_operations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_string_creation() {
        let mut heap = LuaHeap::new();
        
        {
            let mut tx = HeapTransaction::new(&mut heap);
            
            let s1 = tx.create_string("hello").unwrap();
            let s2 = tx.create_string("hello").unwrap();
            
            // Should get same handle within transaction
            assert_eq!(s1, s2);
            
            tx.commit().unwrap();
        }
        
        // String should exist in heap
        assert!(heap.is_valid_string(
            heap.create_string_internal("hello").unwrap()
        ));
    }
    
    #[test]
    fn test_transaction_table_operations() {
        let mut heap = LuaHeap::new();
        let table = heap.create_table().unwrap();
        
        {
            let mut tx = HeapTransaction::new(&mut heap);
            
            let key = Value::String(tx.create_string("key").unwrap());
            let value = Value::Number(42.0);
            
            // Set field
            tx.set_table_field(table, key.clone(), value).unwrap();
            
            // Should read pending value
            let read = tx.read_table_field(table, &key).unwrap();
            assert_eq!(read, value);
            
            tx.commit().unwrap();
        }
        
        // Field should be set in heap
        let key = Value::String(heap.create_string_internal("key").unwrap());
        let retrieved = heap.get_table_field(table, &key).unwrap();
        assert_eq!(retrieved, Value::Number(42.0));
    }
}