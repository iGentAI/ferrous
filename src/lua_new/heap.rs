//! Lua heap management with generational arena architecture

use crate::lua_new::arena::{Arena, Handle};
use crate::lua_new::value::{Value, StringHandle, TableHandle, ClosureHandle, ThreadHandle, FunctionProto, UpvalueRef};
use crate::lua_new::error::{LuaError, Result};
use std::collections::HashMap;
use std::hash::{Hash, Hasher, DefaultHasher};

/// Garbage collection mark
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcMark {
    /// Not reachable (or not yet reached)
    White,
    
    /// Reachable but not fully processed
    Gray,
    
    /// Reachable and fully processed
    Black,
}

/// String object in heap
#[derive(Debug)]
pub struct StringObject {
    /// Actual string bytes
    pub bytes: Box<[u8]>,
    
    /// Pre-computed hash for efficiency
    pub hash: u64,
    
    /// GC mark
    pub mark: GcMark,
}

/// Table object in heap
#[derive(Debug)]
pub struct TableObject {
    /// Array part (contiguous integer keys)
    pub array: Vec<Value>,
    
    /// Hash part (non-integer or sparse keys)
    pub map: HashMap<Value, Value>,
    
    /// Metatable (handle to another table)
    pub metatable: Option<TableHandle>,
    
    /// GC mark
    pub mark: GcMark,
}

impl TableObject {
    /// Create a new empty table
    pub fn new() -> Self {
        TableObject {
            array: Vec::new(),
            map: HashMap::new(),
            metatable: None,
            mark: GcMark::White,
        }
    }
    
    /// Get a value by key
    pub fn get(&self, key: &Value) -> Option<&Value> {
        match key {
            Value::Number(n) if n.fract() == 0.0 && *n >= 1.0 => {
                let index = (*n as usize).saturating_sub(1);
                self.array.get(index).filter(|v| !v.is_nil())
            }
            _ => self.map.get(key),
        }
    }
    
    /// Set a value by key
    pub fn set(&mut self, key: Value, value: Value) {
        match key {
            Value::Number(n) if n.fract() == 0.0 && n >= 1.0 => {
                let index = (n as usize).saturating_sub(1);
                
                // Extend array if needed
                if index >= self.array.len() && !value.is_nil() {
                    self.array.resize(index + 1, Value::Nil);
                }
                
                if index < self.array.len() {
                    self.array[index] = value;
                } else {
                    // Too sparse, use hash part
                    self.map.insert(key, value);
                }
            }
            _ => {
                if value.is_nil() {
                    self.map.remove(&key);
                } else {
                    self.map.insert(key, value);
                }
            }
        }
    }
    
    /// Get the length of the table (# operator)
    pub fn len(&self) -> usize {
        // Find the last non-nil element in the array
        for (i, v) in self.array.iter().enumerate().rev() {
            if !v.is_nil() {
                return i + 1;
            }
        }
        0
    }
    
    /// Check if table is empty
    pub fn is_empty(&self) -> bool {
        self.array.iter().all(|v| v.is_nil()) && self.map.is_empty()
    }
}

/// Closure object in heap
#[derive(Debug)]
pub struct ClosureObject {
    /// Function prototype (bytecode and constants)
    pub proto: FunctionProto,
    
    /// Upvalues (captured variables)
    pub upvalues: Box<[UpvalueRef]>,
    
    /// GC mark
    pub mark: GcMark,
}

/// Thread object in heap
#[derive(Debug)]
pub struct ThreadObject {
    /// Value stack
    pub stack: Vec<Value>,
    
    /// Call frames
    pub call_frames: Vec<CallFrame>,
    
    /// Current thread status
    pub status: ThreadStatus,
    
    /// GC mark
    pub mark: GcMark,
}

/// Call frame information
#[derive(Debug)]
pub struct CallFrame {
    /// Function being executed
    pub closure: ClosureHandle,
    
    /// Current program counter
    pub pc: usize,
    
    /// Base register for this frame
    pub base_register: u16,
    
    /// Expected return values
    pub return_count: u8,
}

/// Thread execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadStatus {
    /// Running normally
    Running,
    
    /// Yielded (coroutine)
    Yielded,
    
    /// Finished execution
    Dead,
    
    /// Error occurred
    Error,
}

/// Garbage collection phase
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcPhase {
    /// No collection in progress
    Idle,
    
    /// Mark root set
    MarkRoots,
    
    /// Propagate marks through object graph
    Propagate,
    
    /// Sweep unmarked objects
    Sweep,
}

/// Garbage collection state
#[derive(Debug)]
pub struct GcState {
    /// Current phase
    pub phase: GcPhase,
    
    /// Gray stack for marking
    pub gray_stack: Vec<GcObject>,
    
    /// Memory threshold for triggering GC
    pub threshold: usize,
    
    /// Current memory debt
    pub debt: isize,
}

/// GC object reference
#[derive(Debug, Clone)]
pub enum GcObject {
    String(StringHandle),
    Table(TableHandle),
    Closure(ClosureHandle),
    Thread(ThreadHandle),
}

/// Memory statistics
#[derive(Debug, Default)]
pub struct MemoryStats {
    /// Total allocated memory
    pub allocated: usize,
    
    /// Number of strings
    pub strings: usize,
    
    /// Number of tables
    pub tables: usize,
    
    /// Number of closures
    pub closures: usize,
    
    /// Number of threads
    pub threads: usize,
}

/// Core heap implementation
pub struct LuaHeap {
    /// Arena for string objects
    pub strings: Arena<StringObject>,
    
    /// Arena for table objects
    pub tables: Arena<TableObject>,
    
    /// Arena for closure objects
    pub closures: Arena<ClosureObject>,
    
    /// Arena for thread objects
    pub threads: Arena<ThreadObject>,
    
    /// String interner (for string deduplication)
    pub string_interner: HashMap<u64, StringHandle>,
    
    /// Garbage collection state
    pub gc_state: GcState,
    
    /// Memory usage statistics
    pub stats: MemoryStats,
}

impl LuaHeap {
    /// Create a new heap
    pub fn new() -> Self {
        LuaHeap {
            strings: Arena::new(),
            tables: Arena::new(),
            closures: Arena::new(),
            threads: Arena::new(),
            string_interner: HashMap::new(),
            gc_state: GcState {
                phase: GcPhase::Idle,
                gray_stack: Vec::new(),
                threshold: 1024 * 1024, // 1MB initial threshold
                debt: 0,
            },
            stats: MemoryStats::default(),
        }
    }
    
    /// Allocate a new string
    pub fn alloc_string(&mut self, bytes: &[u8]) -> StringHandle {
        // Compute hash for interning
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        let hash = hasher.finish();
        
        // Check intern table first (string deduplication)
        if let Some(&handle) = self.string_interner.get(&hash) {
            if let Some(existing) = self.strings.get(handle.0) {
                if existing.bytes.as_ref() == bytes {
                    return handle;
                }
            }
        }
        
        // Allocate new string
        let string_obj = StringObject {
            bytes: bytes.into(),
            hash,
            mark: GcMark::White,
        };
        
        let handle_raw = self.strings.insert(string_obj);
        let handle = StringHandle(handle_raw);
        
        // Add to intern table
        self.string_interner.insert(hash, handle);
        
        // Account for memory usage
        self.stats.allocated += bytes.len() + std::mem::size_of::<StringObject>();
        self.stats.strings += 1;
        
        // Trigger GC if needed
        self.check_gc_threshold();
        
        handle
    }
    
    /// Create a string from UTF-8
    pub fn create_string(&mut self, s: &str) -> StringHandle {
        self.alloc_string(s.as_bytes())
    }
    
    /// Get string bytes
    pub fn get_string(&self, handle: StringHandle) -> Result<&[u8]> {
        self.strings.get(handle.0)
            .map(|s| s.bytes.as_ref())
            .ok_or(LuaError::InvalidHandle)
    }
    
    /// Get string as UTF-8
    pub fn get_string_utf8(&self, handle: StringHandle) -> Result<&str> {
        let bytes = self.get_string(handle)?;
        std::str::from_utf8(bytes).map_err(|_| LuaError::InvalidEncoding)
    }
    
    /// Allocate a new table
    pub fn alloc_table(&mut self) -> TableHandle {
        let table = TableObject::new();
        let handle_raw = self.tables.insert(table);
        let handle = TableHandle(handle_raw);
        
        // Account for memory
        self.stats.allocated += std::mem::size_of::<TableObject>();
        self.stats.tables += 1;
        
        // Trigger GC if needed
        self.check_gc_threshold();
        
        handle
    }
    
    /// Get a table reference
    pub fn get_table(&self, handle: TableHandle) -> Result<&TableObject> {
        self.tables.get(handle.0).ok_or(LuaError::InvalidHandle)
    }
    
    /// Get a mutable table reference
    pub fn get_table_mut(&mut self, handle: TableHandle) -> Result<&mut TableObject> {
        self.tables.get_mut(handle.0).ok_or(LuaError::InvalidHandle)
    }
    
    /// Allocate a new closure
    pub fn alloc_closure(&mut self, proto: FunctionProto, upvalues: Vec<UpvalueRef>) -> ClosureHandle {
        let closure = ClosureObject {
            proto,
            upvalues: upvalues.into_boxed_slice(),
            mark: GcMark::White,
        };
        
        let handle_raw = self.closures.insert(closure);
        let handle = ClosureHandle(handle_raw);
        
        // Account for memory
        self.stats.allocated += std::mem::size_of::<ClosureObject>();
        self.stats.closures += 1;
        
        // Trigger GC if needed  
        self.check_gc_threshold();
        
        handle
    }
    
    /// Get a closure reference
    pub fn get_closure(&self, handle: ClosureHandle) -> Result<&ClosureObject> {
        self.closures.get(handle.0).ok_or(LuaError::InvalidHandle)
    }
    
    /// Get a mutable closure reference
    pub fn get_closure_mut(&mut self, handle: ClosureHandle) -> Result<&mut ClosureObject> {
        self.closures.get_mut(handle.0).ok_or(LuaError::InvalidHandle)
    }
    
    /// Allocate a new thread
    pub fn alloc_thread(&mut self) -> ThreadHandle {
        let thread = ThreadObject {
            stack: Vec::new(),
            call_frames: Vec::new(),
            status: ThreadStatus::Running,
            mark: GcMark::White,
        };
        
        let handle_raw = self.threads.insert(thread);
        let handle = ThreadHandle(handle_raw);
        
        // Account for memory
        self.stats.allocated += std::mem::size_of::<ThreadObject>();
        self.stats.threads += 1;
        
        // Trigger GC if needed
        self.check_gc_threshold();
        
        handle
    }
    
    /// Get a thread reference
    pub fn get_thread(&self, handle: ThreadHandle) -> Result<&ThreadObject> {
        self.threads.get(handle.0).ok_or(LuaError::InvalidHandle)
    }
    
    /// Get a mutable thread reference
    pub fn get_thread_mut(&mut self, handle: ThreadHandle) -> Result<&mut ThreadObject> {
        self.threads.get_mut(handle.0).ok_or(LuaError::InvalidHandle)
    }
    
    /// Check if GC should run
    fn check_gc_threshold(&mut self) {
        if self.stats.allocated >= self.gc_state.threshold {
            self.gc_state.phase = GcPhase::MarkRoots;
        }
    }
    
    /// Run a garbage collection cycle
    pub fn collect_garbage(&mut self, work_limit: usize, roots: &[Value]) -> bool {
        match self.gc_state.phase {
            GcPhase::Idle => {
                // Check if we should start a new cycle
                if self.stats.allocated >= self.gc_state.threshold {
                    self.gc_state.phase = GcPhase::MarkRoots;
                    self.gc_state.gray_stack.clear();
                    false
                } else {
                    true // Nothing to do
                }
            }
            GcPhase::MarkRoots => {
                // Mark root values
                for root in roots {
                    self.mark_value(root);
                }
                self.gc_state.phase = GcPhase::Propagate;
                false
            }
            GcPhase::Propagate => {
                // Propagate marks
                let done = self.propagate_marks(work_limit);
                if done {
                    self.gc_state.phase = GcPhase::Sweep;
                }
                false
            }
            GcPhase::Sweep => {
                // Sweep unmarked objects
                let done = self.sweep(work_limit);
                if done {
                    // Update threshold for next collection
                    self.gc_state.threshold = (self.stats.allocated as f64 * 1.5) as usize;
                    self.gc_state.phase = GcPhase::Idle;
                    true
                } else {
                    false
                }
            }
        }
    }
    
    /// Mark a value as reachable
    fn mark_value(&mut self, value: &Value) {
        match value {
            Value::String(handle) => self.mark_string(*handle),
            Value::Table(handle) => self.mark_table(*handle),
            Value::Closure(handle) => self.mark_closure(*handle),
            Value::Thread(handle) => self.mark_thread(*handle),
            _ => {} // Primitive values don't need marking
        }
    }
    
    /// Mark a string as reachable
    fn mark_string(&mut self, handle: StringHandle) {
        if let Some(string) = self.strings.get_mut(handle.0) {
            if string.mark == GcMark::White {
                string.mark = GcMark::Black; // Strings have no references
            }
        }
    }
    
    /// Mark a table as reachable
    fn mark_table(&mut self, handle: TableHandle) {
        if let Some(table) = self.tables.get_mut(handle.0) {
            if table.mark == GcMark::White {
                table.mark = GcMark::Gray;
                self.gc_state.gray_stack.push(GcObject::Table(handle));
            }
        }
    }
    
    /// Mark a closure as reachable
    fn mark_closure(&mut self, handle: ClosureHandle) {
        if let Some(closure) = self.closures.get_mut(handle.0) {
            if closure.mark == GcMark::White {
                closure.mark = GcMark::Gray;
                self.gc_state.gray_stack.push(GcObject::Closure(handle));
            }
        }
    }
    
    /// Mark a thread as reachable
    fn mark_thread(&mut self, handle: ThreadHandle) {
        if let Some(thread) = self.threads.get_mut(handle.0) {
            if thread.mark == GcMark::White {
                thread.mark = GcMark::Gray;
                self.gc_state.gray_stack.push(GcObject::Thread(handle));
            }
        }
    }
    
    /// Propagate marks through the object graph
    fn propagate_marks(&mut self, work_limit: usize) -> bool {
        for _ in 0..work_limit {
            if let Some(obj) = self.gc_state.gray_stack.pop() {
                self.scan_object(obj);
            } else {
                return true; // Done
            }
        }
        false // More work to do
    }
    
    /// Scan a gray object and mark its references
    fn scan_object(&mut self, obj: GcObject) {
        match obj {
            GcObject::Table(handle) => {
                // First, mark the table as black
                if let Some(table) = self.tables.get_mut(handle.0) {
                    table.mark = GcMark::Black;
                    
                    // Collect values to mark (to avoid borrow issues)
                    let mut values_to_mark = Vec::new();
                    
                    // Mark array values
                    for value in &table.array {
                        values_to_mark.push(*value);
                    }
                    
                    // Mark hash values
                    for (k, v) in &table.map {
                        values_to_mark.push(*k);
                        values_to_mark.push(*v);
                    }
                    
                    // Mark metatable
                    if let Some(mt) = table.metatable {
                        values_to_mark.push(Value::Table(mt));
                    }
                    
                    // Drop the borrow on table
                    drop(table);
                    
                    // Now mark all collected values
                    for value in values_to_mark {
                        self.mark_value(&value);
                    }
                }
            }
            GcObject::Closure(handle) => {
                // First, mark the closure as black
                let mut constants = Vec::new();
                let mut upvalues = Vec::new();
                
                if let Some(closure) = self.closures.get_mut(handle.0) {
                    // Mark as black
                    closure.mark = GcMark::Black;
                    
                    // Collect constants
                    constants = closure.proto.constants.clone();
                    
                    // Collect upvalues
                    for upvalue in closure.upvalues.iter() {
                        if let UpvalueRef::Closed { value } = upvalue {
                            upvalues.push(*value);
                        }
                    }
                }
                
                // Now mark collected values
                for constant in constants {
                    self.mark_value(&constant);
                }
                
                for value in upvalues {
                    self.mark_value(&value);
                }
            }
            GcObject::Thread(handle) => {
                // First, collect values to mark
                let mut stack_values = Vec::new();
                let mut closure_handles = Vec::new();
                
                if let Some(thread) = self.threads.get_mut(handle.0) {
                    // Mark as black
                    thread.mark = GcMark::Black;
                    
                    // Collect stack values
                    stack_values = thread.stack.clone();
                    
                    // Collect closure handles
                    for frame in &thread.call_frames {
                        closure_handles.push(frame.closure);
                    }
                }
                
                // Mark collected values
                for value in stack_values {
                    self.mark_value(&value);
                }
                
                for closure in closure_handles {
                    self.mark_closure(closure);
                }
            }
            GcObject::String(_) => {
                // Strings have no references, already marked black
            }
        }
    }
    
    /// Sweep phase - remove unmarked objects
    fn sweep(&mut self, work_limit: usize) -> bool {
        let mut work = 0;
        
        // Sweep strings
        if work < work_limit {
            work += self.sweep_strings(work_limit - work);
        }
        
        // Sweep tables
        if work < work_limit {
            work += self.sweep_tables(work_limit - work);
        }
        
        // Sweep closures
        if work < work_limit {
            work += self.sweep_closures(work_limit - work);
        }
        
        // Sweep threads
        if work < work_limit {
            work += self.sweep_threads(work_limit - work);
        }
        
        // Return true if we've swept everything
        work < work_limit
    }
    
    /// Sweep strings
    fn sweep_strings(&mut self, _limit: usize) -> usize {
        let mut to_remove = Vec::new();
        let mut white_count = 0;
        let mut black_count = 0;
        
        // First pass: find objects to remove and count objects to update
        {
            for (handle, string) in self.strings.iter() {
                if string.mark == GcMark::White {
                    to_remove.push(handle);
                    white_count += 1;
                    
                    // Update stats
                    self.stats.allocated = self.stats.allocated.saturating_sub(
                        string.bytes.len() + std::mem::size_of::<StringObject>()
                    );
                    self.stats.strings = self.stats.strings.saturating_sub(1);
                } else {
                    black_count += 1;
                }
            }
        }
        
        // Remove from interner
        for handle in &to_remove {
            self.string_interner.retain(|_, &mut v| v.0 != *handle);
        }
        
        // Create a copy of remaining handles
        let remaining_handles: Vec<_> = self.strings.iter()
            .map(|(h, _)| h)
            .filter(|h| !to_remove.contains(h))
            .collect();
        
        // Second pass: reset marks on remaining objects
        for &handle in &remaining_handles {
            if let Some(string) = self.strings.get_mut(handle) {
                string.mark = GcMark::White;
            }
        }
        
        // Remove white objects
        for handle in to_remove {
            let _ = self.strings.remove(handle);
        }
        
        white_count + black_count
    }
    
    /// Sweep tables  
    fn sweep_tables(&mut self, _limit: usize) -> usize {
        let mut to_remove = Vec::new();
        let mut white_count = 0;
        let mut black_count = 0;
        
        // First pass: find objects to remove and count objects to update
        {
            for (handle, table) in self.tables.iter() {
                if table.mark == GcMark::White {
                    to_remove.push(handle);
                    white_count += 1;
                    
                    // Update stats
                    self.stats.allocated = self.stats.allocated.saturating_sub(
                        std::mem::size_of::<TableObject>()
                    );
                    self.stats.tables = self.stats.tables.saturating_sub(1);
                } else {
                    black_count += 1;
                }
            }
        }
        
        // Create a copy of remaining handles
        let remaining_handles: Vec<_> = self.tables.iter()
            .map(|(h, _)| h)
            .filter(|h| !to_remove.contains(h))
            .collect();
        
        // Second pass: reset marks on remaining objects
        for &handle in &remaining_handles {
            if let Some(table) = self.tables.get_mut(handle) {
                table.mark = GcMark::White;
            }
        }
        
        // Remove white objects
        for handle in to_remove {
            let _ = self.tables.remove(handle);
        }
        
        white_count + black_count
    }
    
    /// Sweep closures
    fn sweep_closures(&mut self, _limit: usize) -> usize {
        let mut to_remove = Vec::new();
        let mut white_count = 0;
        let mut black_count = 0;
        
        // First pass: find objects to remove and count objects to update
        {
            for (handle, closure) in self.closures.iter() {
                if closure.mark == GcMark::White {
                    to_remove.push(handle);
                    white_count += 1;
                    
                    // Update stats
                    self.stats.allocated = self.stats.allocated.saturating_sub(
                        std::mem::size_of::<ClosureObject>()
                    );
                    self.stats.closures = self.stats.closures.saturating_sub(1);
                } else {
                    black_count += 1;
                }
            }
        }
        
        // Create a copy of remaining handles
        let remaining_handles: Vec<_> = self.closures.iter()
            .map(|(h, _)| h)
            .filter(|h| !to_remove.contains(h))
            .collect();
        
        // Second pass: reset marks on remaining objects
        for &handle in &remaining_handles {
            if let Some(closure) = self.closures.get_mut(handle) {
                closure.mark = GcMark::White;
            }
        }
        
        // Remove white objects
        for handle in to_remove {
            let _ = self.closures.remove(handle);
        }
        
        white_count + black_count
    }
    
    /// Sweep threads
    fn sweep_threads(&mut self, _limit: usize) -> usize {
        let mut to_remove = Vec::new();
        let mut white_count = 0;
        let mut black_count = 0;
        
        // First pass: find objects to remove and count objects to update
        {
            for (handle, thread) in self.threads.iter() {
                if thread.mark == GcMark::White {
                    to_remove.push(handle);
                    white_count += 1;
                    
                    // Update stats
                    self.stats.allocated = self.stats.allocated.saturating_sub(
                        std::mem::size_of::<ThreadObject>()
                    );
                    self.stats.threads = self.stats.threads.saturating_sub(1);
                } else {
                    black_count += 1;
                }
            }
        }
        
        // Create a copy of remaining handles
        let remaining_handles: Vec<_> = self.threads.iter()
            .map(|(h, _)| h)
            .filter(|h| !to_remove.contains(h))
            .collect();
        
        // Second pass: reset marks on remaining objects
        for &handle in &remaining_handles {
            if let Some(thread) = self.threads.get_mut(handle) {
                thread.mark = GcMark::White;
            }
        }
        
        // Remove white objects
        for handle in to_remove {
            let _ = self.threads.remove(handle);
        }
        
        white_count + black_count
    }
}

impl Default for LuaHeap {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to compute string hash
pub fn compute_hash(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}