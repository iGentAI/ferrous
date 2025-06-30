//! Transaction-Based Heap Access
//!
//! This module implements a transaction system for safely modifying the Lua heap
//! in the face of Rust's ownership rules. Instead of fighting the borrow checker,
//! this system works with it by collecting changes and applying them all at once.

use std::collections::{HashMap, HashSet};

use super::error::{LuaError, Result};
use super::value::{
    Value, TableHandle, StringHandle, ThreadHandle, ClosureHandle, CallFrame, 
    FunctionProto, CallFrameType, LuaThread
};
use super::heap::LuaHeap;

/// A resource identifier for tracking reads and writes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceId {
    /// A table field
    TableField(TableHandle, u32), // Using u32 as a hash of the key
    
    /// A thread register
    ThreadRegister(ThreadHandle, usize),
    
    /// A thread property
    ThreadProperty(ThreadHandle, ThreadProperty),
    
    /// An upvalue
    Upvalue(ClosureHandle, usize),
}

/// A thread property
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThreadProperty {
    /// Program Counter
    PC,
    
    /// Call Stack
    CallStack,
    
    /// Value Stack
    ValueStack,
    
    /// Status
    Status,
}

/// Information about a call frame
#[derive(Debug, Clone)]
pub struct FrameInfo {
    /// The closure being executed
    pub closure: ClosureHandle,
    
    /// The base register
    pub base: u16,
    
    /// The program counter
    pub pc: usize,
}

/// A transaction for safely modifying the Lua heap
pub struct HeapTransaction<'a> {
    /// The heap being modified
    heap: &'a mut LuaHeap,
    
    /// Changes to be applied
    changes: Vec<HeapChange>,
    
    /// Resources that have been read
    read_set: HashSet<ResourceId>,
    
    /// Resources that have been written
    write_set: HashSet<ResourceId>,
    
    /// Created string handles - to return before commit
    created_strings: HashMap<String, StringHandle>,
    
    /// Created table handles - to return before commit
    created_tables: Vec<TableHandle>,
    
    /// Current thread
    current_thread: ThreadHandle,
}

/// A change to be applied to the heap
#[derive(Debug, Clone)]
pub enum HeapChange {
    /// Set a table field
    SetTableField {
        /// The table
        table: TableHandle,
        
        /// The key
        key: Value,
        
        /// The value
        value: Value,
    },
    
    /// Set a register
    SetRegister {
        /// The thread
        thread: ThreadHandle,
        
        /// The register index
        index: usize,
        
        /// The value
        value: Value,
    },
    
    /// Set program counter
    SetPC {
        /// The thread
        thread: ThreadHandle,
        
        /// The frame index
        frame_index: usize,
        
        /// The new PC
        pc: usize,
    },
    
    /// Increment program counter
    IncrementPC {
        /// The thread
        thread: ThreadHandle,
    },
    
    /// Push a call frame
    PushCallFrame {
        /// The thread
        thread: ThreadHandle,
        
        /// The frame to push
        frame: CallFrame,
    },
    
    /// Pop a call frame
    PopCallFrame {
        /// The thread
        thread: ThreadHandle,
    },
    
    /// Create a string
    CreateString {
        /// The string content
        content: String,
    },
    
    /// Create a table
    CreateTable,
    
    /// Queue an operation
    QueueOperation {
        /// The operation to queue
        operation: super::vm::PendingOperation,
    },
    
    /// Jump relative
    JumpRelative {
        /// The thread
        thread: ThreadHandle,
        
        /// The relative offset
        offset: i32,
    },
}

impl<'a> HeapTransaction<'a> {
    /// Create a new transaction
    pub fn new(heap: &'a mut LuaHeap) -> Self {
        let current_thread = match heap.get_main_thread() {
            Ok(thread) => thread,
            Err(_) => panic!("No main thread available"),
        };
        
        HeapTransaction {
            heap,
            changes: Vec::new(),
            read_set: HashSet::new(),
            write_set: HashSet::new(),
            created_strings: HashMap::new(),
            created_tables: Vec::new(),
            current_thread,
        }
    }
    
    /// Get the current thread
    pub fn current_thread(&self) -> ThreadHandle {
        self.current_thread
    }
    
    /// Get the current call frame
    pub fn get_current_call_frame(&mut self, thread: ThreadHandle) -> Result<CallFrame> {
        // Track read
        self.read_set.insert(ResourceId::ThreadProperty(thread, ThreadProperty::CallStack));
        
        // Check pending changes
        // This is a simplification - full implementation would track individual frames
        
        // Read from heap
        self.heap.get_thread_current_frame(thread).map(|frame| frame.clone())
    }
    
    /// Helper method to get current instruction
    pub fn get_instruction(&self, closure: ClosureHandle, pc: usize) -> Result<super::vm::Instruction> {
        // Validate closure
        if !self.heap.is_valid_closure(closure) {
            return Err(LuaError::InvalidHandle);
        }
        
        // Get the closure
        let closure_obj = self.heap.get_closure(closure)?;
        
        // Get instruction
        if pc >= closure_obj.proto.bytecode.len() {
            return Err(LuaError::InvalidBytecode("Program counter out of range".to_string()));
        }
        
        Ok(super::vm::Instruction(closure_obj.proto.bytecode[pc]))
    }
    
    /// Increment program counter
    pub fn increment_pc(&mut self, thread: ThreadHandle) -> Result<()> {
        // Track write
        self.write_set.insert(ResourceId::ThreadProperty(thread, ThreadProperty::PC));
        
        // Queue change
        self.changes.push(HeapChange::IncrementPC { thread });
        
        Ok(())
    }
    
    /// Read a register
    pub fn read_register(&self, thread: ThreadHandle, index: usize) -> Result<Value> {
        // Record the read in a way that doesn't mutate self
        // This is a simplification - in a more complete implementation, we would track reads
        
        // Check pending changes first (from most recent to oldest)
        for change in self.changes.iter().rev() {
            if let HeapChange::SetRegister { thread: t, index: i, value } = change {
                if *t == thread && *i == index {
                    return Ok(value.clone());
                }
            }
        }
        
        // Fall back to reading from the heap
        self.heap.get_thread_register(thread, index)
    }
    
    /// Set a register
    pub fn set_register(&mut self, thread: ThreadHandle, index: usize, value: Value) {
        // Track write
        self.write_set.insert(ResourceId::ThreadRegister(thread, index));
        
        // Queue change
        self.changes.push(HeapChange::SetRegister { thread, index, value });
    }
    
    /// Read a constant-or-register
    pub fn read_rk(&mut self, thread: ThreadHandle, rk: u16) -> Result<Value> {
        if rk & 0x100 != 0 {
            // This is a constant
            let frame = self.get_current_call_frame(thread)?;
            let constant_index = (rk & 0xFF) as usize;
            self.get_constant(frame.closure, constant_index)
        } else {
            // This is a register
            self.read_register(thread, rk as usize)
        }
    }
    
    /// Get a constant from a closure
    pub fn get_constant(&self, closure: ClosureHandle, index: usize) -> Result<Value> {
        // Validate closure
        if !self.heap.is_valid_closure(closure) {
            return Err(LuaError::InvalidHandle);
        }
        
        // Get the closure
        let closure_obj = self.heap.get_closure(closure)?;
        
        // Get the constant
        if index >= closure_obj.proto.constants.len() {
            return Err(LuaError::InvalidBytecode(format!("Constant index out of range: {}", index)));
        }
        
        Ok(closure_obj.proto.constants[index].clone())
    }
    
    /// Jump relative to current position
    pub fn jump(&mut self, thread: ThreadHandle, offset: i32) -> Result<()> {
        // Track write
        self.write_set.insert(ResourceId::ThreadProperty(thread, ThreadProperty::PC));
        
        // Queue change
        self.changes.push(HeapChange::JumpRelative { thread, offset });
        
        Ok(())
    }
    
    /// Get current base register
    pub fn get_current_base_register(&mut self) -> Result<u16> {
        let frame = self.get_current_call_frame(self.current_thread)?;
        Ok(frame.base_register) 
    }
    
    /// Get stack top
    pub fn get_stack_top(&self, thread: ThreadHandle) -> Result<usize> {
        // In a more complete implementation, we would track reads and changes to stack size
        
        // Fall back to reading from the heap
        self.heap.get_thread_stack_size(thread)
    }
    
    /// Get a metatable
    pub fn get_metatable(&mut self, table: TableHandle) -> Result<Option<TableHandle>> {
        // Validate table
        if !self.heap.is_valid_table(table) {
            return Err(LuaError::InvalidHandle);
        }
        
        // Get metatable directly from heap without tracking
        // This avoids issues with borrowing
        self.heap.get_metatable(table)
    }
    
    /// Get current frame info
    pub fn get_current_frame_info(&mut self) -> Result<FrameInfo> {
        let frame = self.get_current_call_frame(self.current_thread)?;
        Ok(FrameInfo {
            closure: frame.closure,
            base: frame.base_register,
            pc: frame.pc,
        })
    }
    
    /// Get the current frame base
    pub fn current_frame_base(&mut self) -> Result<u16> {
        let frame = self.get_current_call_frame(self.current_thread)?;
        Ok(frame.base_register)
    }
    
    /// Get current instruction
    pub fn get_current_instruction(&mut self) -> Result<super::vm::Instruction> {
        // Get the frame
        let frame = self.get_current_call_frame(self.current_thread)?;
        
        // Get the instruction
        self.get_instruction(frame.closure, frame.pc)
    }
    
    /// Read a table field
    pub fn read_table_field(&self, table: TableHandle, key: &Value) -> Result<Value> {
        // Validate table
        if !self.heap.is_valid_table(table) {
            return Err(LuaError::InvalidHandle);
        }
        
        // In a more complete implementation, we would track reads
        
        // Check pending changes first (from most recent to oldest)
        for change in self.changes.iter().rev() {
            if let HeapChange::SetTableField { table: t, key: k, value } = change {
                if *t == table && k == key {
                    return Ok(value.clone());
                }
            }
        }
        
        // Fall back to reading from the heap
        self.heap.get_table_field(table, key)
    }
    
    /// Set a table field
    pub fn set_table_field(&mut self, table: TableHandle, key: Value, value: Value) -> Result<()> {
        // Validate table
        if !self.heap.is_valid_table(table) {
            return Err(LuaError::InvalidHandle);
        }
        
        // Track write using a hash of the key
        let key_hash = self.hash_value(&key);
        self.write_set.insert(ResourceId::TableField(table, key_hash));
        
        // Queue change
        self.changes.push(HeapChange::SetTableField { table, key, value });
        
        Ok(())
    }
    
    /// Create a string through the transaction
    pub fn create_string(&mut self, content: &str) -> Result<StringHandle> {
        // Check if we've already created this string in this transaction
        if let Some(handle) = self.created_strings.get(content) {
            return Ok(*handle);
        }
        
        // Create string using the heap
        let handle = self.heap.create_string(content)?;
        
        // Cache for this transaction
        self.created_strings.insert(content.to_string(), handle);
        
        Ok(handle)
    }
    
    /// Create a table
    pub fn create_table(&mut self) -> Result<TableHandle> {
        // Create table immediately - safe because it doesn't depend on other changes
        let handle = self.heap.create_table()?;
        
        // Cache for this transaction
        self.created_tables.push(handle);
        
        // Queue change for tracking
        self.changes.push(HeapChange::CreateTable);
        
        Ok(handle)
    }
    
    /// Push a call frame
    pub fn push_call_frame(&mut self, thread: ThreadHandle, frame: CallFrame) -> Result<()> {
        // Track write
        self.write_set.insert(ResourceId::ThreadProperty(thread, ThreadProperty::CallStack));
        
        // Queue change
        self.changes.push(HeapChange::PushCallFrame { thread, frame });
        
        Ok(())
    }
    
    /// Pop a call frame
    pub fn pop_call_frame(&mut self, thread: ThreadHandle) -> Result<()> {
        // Track write
        self.write_set.insert(ResourceId::ThreadProperty(thread, ThreadProperty::CallStack));
        
        // Queue change
        self.changes.push(HeapChange::PopCallFrame { thread });
        
        Ok(())
    }
    
    /// Queue an operation
    pub fn queue_operation(&mut self, operation: super::vm::PendingOperation) {
        // Queue change
        self.changes.push(HeapChange::QueueOperation { operation });
    }
    
    /// Compute a hash of a value for tracking reads/writes
    fn hash_value(&self, value: &Value) -> u32 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish() as u32
    }
    
    /// Commit all queued changes
    pub fn commit(self) -> Result<()> {
        for change in self.changes {
            match change {
                HeapChange::SetTableField { table, key, value } => {
                    self.heap.set_table_field(table, key, value)?;
                }
                HeapChange::SetRegister { thread, index, value } => {
                    self.heap.set_thread_register(thread, index, value)?;
                }
                HeapChange::SetPC { thread, frame_index, pc } => {
                    self.heap.set_thread_pc(thread, frame_index, pc)?;
                }
                HeapChange::IncrementPC { thread } => {
                    let frame = self.heap.get_thread_current_frame_mut(thread)?;
                    frame.pc += 1;
                }
                HeapChange::PushCallFrame { thread, frame } => {
                    self.heap.push_thread_call_frame(thread, frame)?;
                }
                HeapChange::PopCallFrame { thread } => {
                    self.heap.pop_thread_call_frame(thread)?;
                }
                HeapChange::CreateString { .. } => {
                    // String was already created immediately
                }
                HeapChange::CreateTable => {
                    // Table was already created immediately
                }
                HeapChange::QueueOperation { operation } => {
                    // Add to VM's pending operations
                    self.heap.queue_operation(operation)?;
                }
                HeapChange::JumpRelative { thread, offset } => {
                    // Get current frame
                    let frame = self.heap.get_thread_current_frame_mut(thread)?;
                    
                    // Update PC
                    if offset >= 0 {
                        frame.pc += offset as usize;
                    } else {
                        frame.pc = frame.pc.saturating_sub((-offset) as usize);
                    }
                }
            }
        }
        
        Ok(())
    }
}