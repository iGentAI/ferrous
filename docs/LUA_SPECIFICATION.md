# Ferrous Lua 5.1 Interpreter Specification: Generational Arena Architecture

## Executive Summary

This specification defines the implementation of a high-performance, memory-safe Lua 5.1 interpreter for Ferrous, providing Redis-compatible scripting functionality through EVAL/EVALSHA commands. The interpreter uses the Generational Arena + Handle pattern to achieve superior memory efficiency, eliminate reference cycles, and leverage Rust's type system while maintaining the zero-dependency philosophy and full Redis compatibility.

**Update Note (2025-06-24)**: This updated specification describes the generational arena architecture chosen to replace the original implementation, which had several fundamental design issues including circular reference handling, limited register capacity, and inefficient memory management.

## 1. Core Architecture

### 1.1 Design Principles

```rust
// Core architectural principles
pub struct LuaDesignPrinciples {
    zero_dependencies: bool,        // No external crates beyond std
    redis_compatible: bool,         // Matches Redis Lua behavior exactly
    memory_safe: bool,              // Rust safety guarantees
    deterministic: bool,            // Same input = same output
    sandboxed: bool,                // No filesystem/network access
    performance_focused: bool,      // Minimal overhead
    handle_based: bool,             // Use handles, not references
    type_safe: bool,                // Leverage Rust's type system
}
```

### 1.2 High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Ferrous Server                           │
├─────────────────────────────────────────────────────────────┤
│                    Command Layer                            │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐     │
│  │   EVAL      │  │  EVALSHA    │  │  SCRIPT        │     │
│  │   Handler   │  │  Handler    │  │  Commands      │     │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────────┘     │
│         └────────────────┴────────────────┴─────────┐      │
├─────────────────────────────────────────────────────▼─────┤
│                    Lua Engine Layer                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐     │
│  │  Script     │  │   LuaVM     │  │  Redis API     │     │
│  │  Cache      │  │  Instances  │  │  Bridge        │     │
│  └─────────────┘  └─────────────┘  └─────────────────┘     │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐     │
│  │  LuaHeap    │  │ Generational│  │  Security      │     │
│  │  Arenas     │  │     GC      │  │  Sandbox       │     │
│  └─────────────┘  └─────────────┘  └─────────────────┘     │
├─────────────────────────────────────────────────────────────┤
│                  Storage Engine                             │
└─────────────────────────────────────────────────────────────┘
```

## 2. Value Representation

### 2.1 Core Value Type

```rust
/// 16-byte Lua value (fits in two machine words)
#[derive(Clone, Copy, Debug)]
pub enum Value {
    /// Nil value
    Nil,
    
    /// Boolean value
    Boolean(bool),
    
    /// Number value (Lua uses f64 for all numbers)
    Number(f64),
    
    /// String handle (points to interned string in heap)
    String(StringHandle),
    
    /// Table handle (points to table in heap)
    Table(TableHandle),
    
    /// Closure handle (points to closure in heap)
    Closure(ClosureHandle),
    
    /// Thread handle (points to thread in heap)
    Thread(ThreadHandle),
    
    /// Built-in function pointer
    CFunction(CFunction),
}
```

### 2.2 Handle Types

```rust
/// Type-safe string handle
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StringHandle(Handle);

/// Type-safe table handle
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TableHandle(Handle);

/// Type-safe closure handle
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClosureHandle(Handle);

/// Type-safe thread handle
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ThreadHandle(Handle);

/// Raw handle (arena index + generation)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Handle {
    /// Index into the appropriate arena
    index: u32,
    
    /// Generation count for detecting stale references
    generation: u32,
}
```

### 2.3 Memory Storage

```rust
/// Core heap implementation
pub struct LuaHeap {
    /// Arena for string objects
    strings: SlotMap<StringHandle, StringObject>,
    
    /// Arena for table objects
    tables: SlotMap<TableHandle, TableObject>,
    
    /// Arena for closure objects
    closures: SlotMap<ClosureHandle, ClosureObject>,
    
    /// Arena for thread objects
    threads: SlotMap<ThreadHandle, ThreadObject>,
    
    /// String interner (for string deduplication)
    string_interner: FxHashMap<u64, StringHandle>,
    
    /// Garbage collection state
    gc_state: GcState,
    
    /// Memory usage statistics
    stats: MemoryStats,
}

pub struct StringObject {
    /// Actual string bytes
    bytes: Box<[u8]>,
    
    /// Pre-computed hash for efficiency
    hash: u64,
    
    /// GC mark
    mark: GcMark,
}

pub struct TableObject {
    /// Array part (contiguous integer keys)
    array: SmallVec<[Value; 8]>,
    
    /// Hash part (non-integer or sparse keys) 
    map: FxHashMap<Value, Value>,
    
    /// Metatable (handle to another table)
    metatable: Option<TableHandle>,
    
    /// GC mark
    mark: GcMark,
}

pub struct ClosureObject {
    /// Function prototype (bytecode and constants)
    proto: FunctionProto,
    
    /// Upvalues (captured variables)
    upvalues: Box<[UpvalueRef]>,
    
    /// GC mark
    mark: GcMark,
}

pub struct ThreadObject {
    /// Value stack
    stack: Vec<Value>,
    
    /// Call frames 
    call_frames: Vec<CallFrame>,
    
    /// Current thread status
    status: ThreadStatus,
    
    /// GC mark
    mark: GcMark,
}
```

### 2.4 Memory Management

```rust
impl LuaHeap {
    /// Allocate a new string
    pub fn alloc_string(&mut self, bytes: &[u8]) -> StringHandle {
        // Compute hash for interning
        let hash = compute_hash(bytes);
        
        // Check intern table first (string deduplication)
        if let Some(handle) = self.string_interner.get(&hash) {
            let existing = &self.strings[*handle];
            if existing.bytes.as_ref() == bytes {
                return *handle;
            }
        }
        
        // Allocate new string
        let handle = self.strings.insert(StringObject {
            bytes: bytes.into(),
            hash,
            mark: GcMark::White,
        });
        
        // Add to intern table
        self.string_interner.insert(hash, handle);
        
        // Account for memory usage
        self.stats.allocated += bytes.len() + std::mem::size_of::<StringObject>();
        
        // Trigger GC if needed
        self.check_gc_threshold();
        
        handle
    }
    
    /// Allocate a new table
    pub fn alloc_table(&mut self) -> TableHandle {
        let handle = self.tables.insert(TableObject {
            array: SmallVec::new(),
            map: FxHashMap::default(),
            metatable: None,
            mark: GcMark::White,
        });
        
        // Account for memory
        self.stats.allocated += std::mem::size_of::<TableObject>();
        
        // Trigger GC if needed
        self.check_gc_threshold();
        
        handle
    }
    
    /// Check if GC should run
    fn check_gc_threshold(&mut self) {
        if self.stats.allocated >= self.gc_state.threshold {
            self.gc_state.phase = GcPhase::MarkRoots;
        }
    }
}
```

## 3. VM Execution

### 3.1 VM Structure

```rust
pub struct LuaVM {
    /// Memory heap
    heap: LuaHeap,
    
    /// Currently executing thread
    current_thread: ThreadHandle,
    
    /// Configuration options
    config: VMConfig,
    
    /// Resource limits
    limits: ResourceLimits,
    
    /// Instruction counter
    instruction_count: u64,
}

pub struct ResourceLimits {
    /// Maximum memory usage in bytes
    memory_limit: usize,
    
    /// Maximum instructions to execute
    instruction_limit: u64,
    
    /// Maximum call stack depth
    call_stack_limit: usize,
    
    /// Maximum value stack size
    value_stack_limit: usize,
}

impl LuaVM {
    /// Execute bytecode
    pub fn execute(&mut self) -> Result<Value, RuntimeError> {
        // Initialize thread state
        let thread = self.heap.get_thread_mut(self.current_thread)
            .ok_or(RuntimeError::InvalidHandle)?;
        
        // Execution loop
        loop {
            // Check resource limits
            self.check_limits()?;
            
            // Get current call frame
            let frame = thread.current_frame()?;
            
            // Fetch instruction
            let instr = frame.proto.code.get(frame.pc)
                .ok_or(RuntimeError::InvalidProgramCounter)?;
            
            // Increment PC
            frame.pc += 1;
            
            // Execute instruction
            match self.execute_instruction(*instr)? {
                ExecutionStatus::Continue => continue,
                ExecutionStatus::Return(value) => return Ok(value),
                ExecutionStatus::Yield(value) => {
                    // Coroutines not implemented in Redis Lua
                    return Err(RuntimeError::NotImplemented("coroutines"));
                },
            }
        }
    }
    
    /// Execute a single bytecode instruction
    fn execute_instruction(&mut self, instr: u32) -> Result<ExecutionStatus, RuntimeError> {
        // Decode instruction
        let op = (instr & 0x3F) as u8;
        let a = ((instr >> 6) & 0xFF) as u8;
        let b = ((instr >> 14) & 0x1FF) as u16;
        let c = ((instr >> 23) & 0x1FF) as u16;
        
        match op {
            // Load a constant
            OP_LOADK => {
                let bx = ((instr >> 14) & 0x3FFFF) as u32;
                let k = self.get_constant(bx)?;
                self.set_register(a, k)?;
                Ok(ExecutionStatus::Continue)
            }
            
            // Addition
            OP_ADD => {
                let b_val = self.get_rk(b)?;
                let c_val = self.get_rk(c)?;
                
                match (b_val, c_val) {
                    (Value::Number(b_num), Value::Number(c_num)) => {
                        self.set_register(a, Value::Number(b_num + c_num))?;
                    }
                    _ => {
                        // Try metamethods
                        let result = self.heap.apply_binary_metamethod(
                            Metamethod::Add, b_val, c_val
                        )?;
                        self.set_register(a, result)?;
                    }
                }
                
                Ok(ExecutionStatus::Continue)
            }
            
            // Return
            OP_RETURN => {
                let values = self.collect_returns(a, b)?;
                Ok(ExecutionStatus::Return(values[0]))
            }
            
            // ... other opcodes ...
            
            _ => Err(RuntimeError::InvalidOpcode(op))
        }
    }
}
```

### 3.2 Call Frame Management

```rust
pub struct CallFrame {
    /// Function being executed
    function: ClosureHandle,
    
    /// Current program counter
    pc: usize,
    
    /// Base register for this frame
    base_register: u16,
    
    /// Expected return values
    return_count: u8,
}

impl ThreadObject {
    /// Push a new call frame
    pub fn push_frame(&mut self, function: ClosureHandle, args: &[Value]) -> Result<(), RuntimeError> {
        let proto = self.heap.get_closure(function)?.proto;
        
        // Check stack depth
        if self.call_frames.len() >= self.limits.call_stack_limit {
            return Err(RuntimeError::StackOverflow);
        }
        
        // Prepare registers for arguments
        let base_register = self.registers.len() as u16;
        
        // Push arguments to registers
        let num_params = proto.param_count as usize;
        for i in 0..num_params {
            if i < args.len() {
                self.registers.push(args[i]);
            } else {
                self.registers.push(Value::Nil);
            }
        }
        
        // Push call frame
        self.call_frames.push(CallFrame {
            function,
            pc: 0,
            base_register,
            return_count: 1, // Default to one return value
        });
        
        Ok(())
    }
    
    /// Pop a call frame and clean up
    pub fn pop_frame(&mut self) -> Result<CallFrame, RuntimeError> {
        let frame = self.call_frames.pop()
            .ok_or(RuntimeError::InvalidOperation("No call frame to pop"))?;
        
        // Truncate registers back to base
        self.registers.truncate(frame.base_register as usize);
        
        Ok(frame)
    }
}
```

## 4. Upvalue and Closure Management

```rust
/// Upvalue reference
pub enum UpvalueRef {
    /// Reference to a register in a parent stack frame
    Open { 
        /// Index in the thread's register array
        register_idx: u16,
    },
    
    /// Closed upvalue with captured value
    Closed {
        /// The captured value
        value: Value,
    },
}

pub struct ClosureObject {
    /// Function prototype
    proto: FunctionProto,
    
    /// Upvalue references
    upvalues: Box<[UpvalueRef]>,
    
    /// GC mark
    mark: GcMark,
}

impl LuaHeap {
    /// Create a closure from a function prototype
    pub fn create_closure(
        &mut self, 
        proto: FunctionProto,
        upvalues: Vec<UpvalueRef>
    ) -> Result<ClosureHandle, RuntimeError> {
        let handle = self.closures.insert(ClosureObject {
            proto,
            upvalues: upvalues.into_boxed_slice(),
            mark: GcMark::White,
        });
        
        // Update memory stats
        self.stats.allocated += std::mem::size_of::<ClosureObject>() + 
            (upvalues.len() * std::mem::size_of::<UpvalueRef>());
        
        // Trigger GC if needed
        self.check_gc_threshold();
        
        Ok(handle)
    }
    
    /// Close an upvalue
    pub fn close_upvalue(
        &mut self, 
        closure: ClosureHandle,
        upvalue_idx: usize,
        value: Value
    ) -> Result<(), RuntimeError> {
        let closure_obj = self.closures.get_mut(closure)
            .ok_or(RuntimeError::InvalidHandle)?;
            
        if upvalue_idx < closure_obj.upvalues.len() {
            closure_obj.upvalues[upvalue_idx] = UpvalueRef::Closed { value };
            Ok(())
        } else {
            Err(RuntimeError::InvalidOperation("Upvalue index out of bounds"))
        }
    }
}
```

## 5. Table Operations

```rust
impl LuaHeap {
    /// Get a value from a table
    pub fn table_get(
        &self, 
        table: TableHandle, 
        key: Value
    ) -> Result<Value, RuntimeError> {
        let table_obj = self.tables.get(table)
            .ok_or(RuntimeError::InvalidHandle)?;
        
        // Fast path for array indices
        if let Value::Number(idx) = key {
            if idx.fract() == 0.0 && idx >= 1.0 && idx <= table_obj.array.len() as f64 {
                let array_idx = idx as usize - 1; // Convert to 0-based
                return Ok(table_obj.array[array_idx]);
            }
        }
        
        // Look in hash part
        if let Some(value) = table_obj.map.get(&key) {
            return Ok(*value);
        }
        
        // Try metatable __index
        if let Some(metatable) = table_obj.metatable {
            if let Some(index_func) = self.get_metamethod(metatable, Metamethod::Index)? {
                return self.call_metamethod(
                    index_func,
                    &[Value::Table(table), key]
                );
            }
        }
        
        // Not found
        Ok(Value::Nil)
    }
    
    /// Set a value in a table
    pub fn table_set(
        &mut self,
        table: TableHandle,
        key: Value,
        value: Value
    ) -> Result<(), RuntimeError> {
        let table_obj = self.tables.get_mut(table)
            .ok_or(RuntimeError::InvalidHandle)?;
        
        // Check for nil value (deletion)
        let is_nil = matches!(value, Value::Nil);
        
        // Handle array part (integer keys from 1 to array.len())
        if let Value::Number(idx) = key {
            if idx.fract() == 0.0 && idx >= 1.0 {
                let array_idx = idx as usize - 1; // Convert to 0-based
                
                if array_idx < table_obj.array.len() {
                    // Direct array access
                    table_obj.array[array_idx] = value;
                    return Ok(());
                } else if array_idx == table_obj.array.len() && !is_nil {
                    // Extend array for sequential insertion
                    table_obj.array.push(value);
                    return Ok(());
                }
            }
        }
        
        // Handle hash part
        if is_nil {
            table_obj.map.remove(&key);
        } else {
            table_obj.map.insert(key, value);
        }
        
        Ok(())
    }
    
    /// Get the length of a table (# operator)
    pub fn table_len(&self, table: TableHandle) -> Result<usize, RuntimeError> {
        let table_obj = self.tables.get(table)
            .ok_or(RuntimeError::InvalidHandle)?;
            
        // Try metamethod first
        if let Some(metatable) = table_obj.metatable {
            if let Some(len_func) = self.get_metamethod(metatable, Metamethod::Len)? {
                if let Value::Number(n) = self.call_metamethod(
                    len_func,
                    &[Value::Table(table)]
                )? {
                    return Ok(n as usize);
                }
            }
        }
        
        // Array length in Lua is the index of the last non-nil element
        let mut len = table_obj.array.len();
        while len > 0 && matches!(table_obj.array[len - 1], Value::Nil) {
            len -= 1;
        }
        
        Ok(len)
    }
}
```

## 6. Redis API Integration

```rust
pub struct RedisApiContext {
    /// Storage engine
    storage: Arc<StorageEngine>,
    
    /// Current database
    db: DatabaseIndex,
    
    /// Execution mode (call vs pcall)
    mode: ExecMode,
    
    /// Memory arena for results
    heap: &'static mut LuaHeap,
}

/// Execute Redis commands from Lua
impl RedisApiContext {
    /// Register API in the VM
    pub fn register(vm: &mut LuaVM) -> Result<(), RuntimeError> {
        // Create redis table
        let redis = vm.heap.alloc_table()?;
        
        // Register functions
        vm.register_function(redis, "call", redis_call)?;
        vm.register_function(redis, "pcall", redis_pcall)?;
        vm.register_function(redis, "log", redis_log)?;
        vm.register_function(redis, "sha1hex", redis_sha1hex)?;
        vm.register_function(redis, "error_reply", redis_error_reply)?;
        vm.register_function(redis, "status_reply", redis_status_reply)?;
        
        // Register constants
        vm.register_number(redis, "LOG_DEBUG", 0.0)?;
        vm.register_number(redis, "LOG_VERBOSE", 1.0)?;
        vm.register_number(redis, "LOG_NOTICE", 2.0)?;
        vm.register_number(redis, "LOG_WARNING", 3.0)?;
        
        // Set in globals
        let globals = vm.globals();
        vm.heap.table_set(globals, vm.heap.create_string("redis"), Value::Table(redis))?;
        
        Ok(())
    }
    
    /// Implementation of redis.call()
    pub fn call(
        &self,
        args: &[Value]
    ) -> Result<Value, RuntimeError> {
        if args.is_empty() {
            return Err(RuntimeError::LuaError("redis.call requires at least a command name".into()));
        }
        
        // Extract command name
        let cmd_name = self.heap.value_to_string(args[0])?;
        
        // Convert arguments
        let mut redis_args = Vec::with_capacity(args.len());
        for arg in &args[1..] {
            redis_args.push(self.lua_to_redis(*arg)?);
        }
        
        // Execute command
        match self.storage.execute_command(self.db, &cmd_name, redis_args) {
            Ok(result) => Ok(self.redis_to_lua(result)?),
            Err(e) => {
                // In call mode, propagate errors
                Err(RuntimeError::LuaError(format!("Redis error: {}", e)))
            }
        }
    }
    
    /// Implementation of redis.pcall()
    pub fn pcall(
        &self,
        args: &[Value]
    ) -> Result<Value, RuntimeError> {
        // Same as call, but catch errors
        match self.call(args) {
            Ok(result) => Ok(result),
            Err(RuntimeError::LuaError(msg)) => {
                // Create error table
                let error_table = self.heap.alloc_table()?;
                self.heap.table_set(
                    error_table,
                    self.heap.create_string("err"),
                    self.heap.create_string(&msg)
                )?;
                
                Ok(Value::Table(error_table))
            },
            Err(e) => Err(e), // Other errors still propagate
        }
    }
}
```

## 7. Security and Sandboxing

```rust
pub struct LuaSandbox {
    /// Functions allowed in the global environment
    allowed_functions: FxHashSet<&'static str>,
    
    /// Maximum memory usage per script
    memory_limit: usize,
    
    /// Maximum instruction count per script
    instruction_limit: u64,
    
    /// Maximum stack depth
    stack_limit: usize,
    
    /// Maximum table size
    table_limit: usize,
    
    /// Deterministic mode (no random, etc.)
    deterministic: bool,
}

impl LuaSandbox {
    /// Create a Redis-compatible sandbox configuration
    pub fn redis_compatible() -> Self {
        let mut allowed = FxHashSet::default();
        
        // Base library (safe subset)
        allowed.insert("assert");
        allowed.insert("error");
        allowed.insert("ipairs");
        allowed.insert("next");
        allowed.insert("pairs");
        allowed.insert("pcall");
        allowed.insert("select");
        allowed.insert("tonumber");
        allowed.insert("tostring");
        allowed.insert("type");
        allowed.insert("unpack");
        
        // String library (all safe)
        allowed.insert("string.byte");
        allowed.insert("string.char");
        allowed.insert("string.find");
        allowed.insert("string.format");
        allowed.insert("string.gmatch");
        allowed.insert("string.gsub");
        allowed.insert("string.len");
        allowed.insert("string.lower");
        allowed.insert("string.match");
        allowed.insert("string.rep");
        allowed.insert("string.reverse");
        allowed.insert("string.sub");
        allowed.insert("string.upper");
        
        // Table library (all safe)
        allowed.insert("table.concat");
        allowed.insert("table.insert");
        allowed.insert("table.remove");
        allowed.insert("table.sort");
        
        // Math library (deterministic subset)
        allowed.insert("math.abs");
        allowed.insert("math.ceil");
        allowed.insert("math.floor");
        allowed.insert("math.max");
        allowed.insert("math.min");
        allowed.insert("math.pow");
        allowed.insert("math.sqrt");
        
        Self {
            allowed_functions: allowed,
            memory_limit: 64 * 1024 * 1024,  // 64MB
            instruction_limit: 100_000_000,   // 100M instructions
            stack_limit: 1000,               // 1000 calls max
            table_limit: 1_000_000,          // 1M entries max
            deterministic: true,
        }
    }
    
    /// Apply sandbox to a VM
    pub fn apply(&self, vm: &mut LuaVM) -> Result<(), RuntimeError> {
        // Set resource limits
        vm.set_memory_limit(self.memory_limit);
        vm.set_instruction_limit(self.instruction_limit);
        vm.set_stack_limit(self.stack_limit);
        vm.set_table_limit(self.table_limit);
        
        // Get globals table
        let globals = vm.globals();
        
        // Remove unsafe libraries
        vm.heap.table_remove(globals, vm.heap.create_string("io"))?;
        vm.heap.table_remove(globals, vm.heap.create_string("os"))?;
        vm.heap.table_remove(globals, vm.heap.create_string("debug"))?;
        vm.heap.table_remove(globals, vm.heap.create_string("package"))?;
        
        // Remove unsafe math functions
        if let Some(Value::Table(math)) = vm.heap.table_get(
            globals,
            vm.heap.create_string("math")
        )? {
            vm.heap.table_remove(math, vm.heap.create_string("random"))?;
            vm.heap.table_remove(math, vm.heap.create_string("randomseed"))?;
        }
        
        // Apply custom environment
        if self.deterministic {
            // Ensure deterministic execution
            vm.set_deterministic_mode(true);
        }
        
        Ok(())
    }
}
```

## 8. Bytecode Execution Optimization

```rust
impl LuaVM {
    /// Fast paths for common operations
    #[inline]
    fn execute_add(&mut self, a: u8, b: u16, c: u16) -> Result<(), RuntimeError> {
        // Get operands
        let b_val = self.get_rk(b)?;
        let c_val = self.get_rk(c)?;
        
        // Fast path for number + number
        match (b_val, c_val) {
            (Value::Number(b_num), Value::Number(c_num)) => {
                self.set_register(a, Value::Number(b_num + c_num))?;
                return Ok(());
            }
            _ => {}
        }
        
        // Try metamethods
        let result = self.heap.apply_binary_metamethod(
            Metamethod::Add,
            b_val,
            c_val
        )?;
        
        self.set_register(a, result)
    }
    
    /// Fast path for table access
    #[inline]
    fn execute_gettable(&mut self, a: u8, b: u16, c: u16) -> Result<(), RuntimeError> {
        let table_val = self.get_register(b)?;
        let key_val = self.get_rk(c)?;
        
        // Fast path for array access
        if let (Value::Table(table), Value::Number(idx)) = (table_val, key_val) {
            if idx.fract() == 0.0 && idx >= 1.0 {
                let array_idx = idx as usize - 1;
                let table_obj = self.heap.get_table(table)?;
                
                if array_idx < table_obj.array.len() {
                    self.set_register(a, table_obj.array[array_idx])?;
                    return Ok(());
                }
            }
        }
        
        // Normal path
        let result = self.heap.table_get(
            table_val.as_table()?,
            key_val
        )?;
        
        self.set_register(a, result)
    }
}
```

## 9. String Interning System

```rust
impl LuaHeap {
    /// Create an interned string
    pub fn create_string(&mut self, s: &str) -> StringHandle {
        self.create_string_bytes(s.as_bytes())
    }
    
    /// Create an interned string from bytes
    pub fn create_string_bytes(&mut self, bytes: &[u8]) -> StringHandle {
        // Compute hash
        let hash = compute_hash(bytes);
        
        // Check intern map first
        if let Some(&handle) = self.string_interner.get(&hash) {
            let string_obj = &self.strings[handle];
            if string_obj.bytes.as_ref() == bytes {
                return handle;
            }
        }
        
        // Not found, create new
        let handle = self.strings.insert(StringObject {
            bytes: Box::from(bytes),
            hash,
            mark: GcMark::White,
        });
        
        // Add to intern map
        self.string_interner.insert(hash, handle);
        
        // Update memory stats
        self.stats.allocated += bytes.len() + std::mem::size_of::<StringObject>();
        
        handle
    }
    
    /// Get string bytes
    pub fn get_string_bytes(&self, handle: StringHandle) -> Result<&[u8], RuntimeError> {
        let string_obj = self.strings.get(handle)
            .ok_or(RuntimeError::InvalidHandle)?;
            
        Ok(&string_obj.bytes)
    }
    
    /// Get string as Rust str (if valid UTF-8)
    pub fn get_string(&self, handle: StringHandle) -> Result<&str, RuntimeError> {
        let bytes = self.get_string_bytes(handle)?;
        std::str::from_utf8(bytes)
            .map_err(|_| RuntimeError::InvalidEncoding)
    }
}
```

## 10. Garbage Collection

```rust
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GcMark {
    /// Not reachable (or not yet reached)
    White,
    
    /// Reachable but not fully processed
    Gray,
    
    /// Reachable and fully processed
    Black,
}

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

impl LuaHeap {
    /// Mark a value as reachable
    fn mark_value(&mut self, value: Value) {
        match value {
            Value::String(handle) => self.mark_string(handle),
            Value::Table(handle) => self.mark_table(handle),
            Value::Closure(handle) => self.mark_closure(handle),
            Value::Thread(handle) => self.mark_thread(handle),
            _ => {} // Primitive values don't need marking
        }
    }
    
    /// Mark a table as reachable
    fn mark_table(&mut self, handle: TableHandle) {
        let table = match self.tables.get_mut(handle) {
            Some(table) => table,
            None => return, // Invalid handle
        };
        
        // Already marked?
        if table.mark == GcMark::Black {
            return;
        }
        
        // Mark as gray
        if table.mark == GcMark::White {
            table.mark = GcMark::Gray;
            self.gc_state.gray_stack.push(GcObject::Table(handle));
        }
    }
    
    /// Propagate marks
    fn propagate_marks(&mut self, work_limit: usize) -> bool {
        let mut work_done = 0;
        
        while work_done < work_limit {
            if let Some(obj) = self.gc_state.gray_stack.pop() {
                self.scan_object(obj);
                work_done += 1;
            } else {
                return true; // Done with mark phase
            }
        }
        
        false // More work to do
    }
    
    /// Scan a gray object
    fn scan_object(&mut self, obj: GcObject) {
        match obj {
            GcObject::Table(handle) => {
                let table = match self.tables.get_mut(handle) {
                    Some(table) => table,
                    None => return, // Invalid handle
                };
                
                // Already black?
                if table.mark == GcMark::Black {
                    return;
                }
                
                // Mark as black
                table.mark = GcMark::Black;
                
                // Mark keys and values
                for value in &table.array {
                    self.mark_value(*value);
                }
                
                for (key, value) in &table.map {
                    self.mark_value(*key);
                    self.mark_value(*value);
                }
                
                // Mark metatable
                if let Some(metatable) = table.metatable {
                    self.mark_table(metatable);
                }
            }
            
            // Similar for other object types...
        }
    }
    
    /// Sweep phase
    fn sweep(&mut self, work_limit: usize) -> bool {
        // Sweep strings
        let mut work_done = self.sweep_strings(work_limit);
        if work_done < work_limit {
            // Sweep tables
            work_done += self.sweep_tables(work_limit - work_done);
        }
        if work_done < work_limit {
            // Sweep closures
            work_done += self.sweep_closures(work_limit - work_done);
        }
        if work_done < work_limit {
            // Sweep threads
            work_done += self.sweep_threads(work_limit - work_done);
        }
        
        // Return true if we've completed all sweeping
        work_done < work_limit
    }
}
```

## 11. Error Handling

```rust
/// Comprehensive error types
pub enum RuntimeError {
    /// Error raised by Lua
    LuaError(String),
    
    /// Type error
    TypeError(String),
    
    /// Syntax error
    SyntaxError {
        message: String,
        line: usize, 
        column: usize,
    },
    
    /// Memory limit exceeded
    MemoryLimit,
    
    /// Instruction limit exceeded
    InstructionLimit,
    
    /// Stack overflow
    StackOverflow,
    
    /// Invalid handle reference
    InvalidHandle,
    
    /// Invalid operation
    InvalidOperation(String),
    
    /// Invalid program counter
    InvalidProgramCounter,
    
    /// Invalid opcode
    InvalidOpcode(u8),
    
    /// Invalid encoding (e.g., non-UTF8 string)
    InvalidEncoding,
    
    /// Feature not implemented
    NotImplemented(&'static str),
}

/// Error context for better debugging
pub struct ErrorContext {
    /// Error type and message
    error: RuntimeError,
    
    /// Call stack trace
    stack_trace: Vec<StackFrame>,
    
    /// Source location
    location: Option<SourceLocation>,
}

pub struct StackFrame {
    /// Function name (if available)
    function: Option<String>,
    
    /// Program counter
    pc: usize,
    
    /// Function type
    function_type: FunctionType,
    
    /// Source location
    location: Option<SourceLocation>,
}

pub struct SourceLocation {
    /// Source file name
    file: Option<String>,
    
    /// Line number
    line: usize,
    
    /// Column
    column: usize,
}
```

## 12. Script Executor Integration

```rust
pub struct ScriptExecutor {
    /// Script cache
    cache: LruCache<String, CompiledScript>,
    
    /// VM pool
    vm_pool: Arc<Mutex<Vec<LuaVM>>>,
    
    /// Storage engine reference
    storage: Arc<StorageEngine>,
    
    /// Currently running script, if any
    current_script: Arc<Mutex<Option<RunningScript>>>,
    
    /// Execution stats
    stats: ExecutionStats,
}

pub struct CompiledScript {
    /// Original source code
    source: String,
    
    /// SHA1 hash
    sha1: String,
    
    /// Compiled bytecode
    bytecode: Vec<u8>,
    
    /// Function prototype
    proto: FunctionProto,
}

impl ScriptExecutor {
    /// Execute a Lua script (EVAL)
    pub fn eval(
        &self,
        source: &str,
        keys: Vec<Vec<u8>>,
        args: Vec<Vec<u8>>,
        db: DatabaseIndex
    ) -> std::result::Result<RespFrame, FerrousError> {
        // Compute SHA1
        let sha1 = compute_sha1(source);
        
        // Try cache first
        let script = match self.get_cached(&sha1) {
            Some(script) => script,
            None => {
                // Compile new script
                match self.compile_script(source, sha1) {
                    Ok(script) => {
                        // Add to cache
                        self.add_to_cache(script.clone());
                        script
                    }
                    Err(e) => return Err(FerrousError::Script(e)),
                }
            }
        };
        
        // Execute script
        self.execute_script(script, keys, args, db)
    }
    
    /// Execute a script by SHA1 (EVALSHA)
    pub fn evalsha(
        &self,
        sha1: &str,
        keys: Vec<Vec<u8>>,
        args: Vec<Vec<u8>>,
        db: DatabaseIndex
    ) -> std::result::Result<RespFrame, FerrousError> {
        // Look up in cache
        let script = match self.get_cached(sha1) {
            Some(script) => script,
            None => return Err(FerrousError::Script(ScriptError::NotFound)),
        };
        
        // Execute script
        self.execute_script(script, keys, args, db)
    }
    
    /// Execute a compiled script
    fn execute_script(
        &self,
        script: CompiledScript,
        keys: Vec<Vec<u8>>,
        args: Vec<Vec<u8>>,
        db: DatabaseIndex
    ) -> std::result::Result<RespFrame, FerrousError> {
        // Get a VM from the pool
        let mut vm = self.get_vm()?;
        
        // Set up killable execution
        let kill_flag = Arc::new(AtomicBool::new(false));
        vm.set_kill_flag(Arc::clone(&kill_flag));
        
        // Set up KEYS and ARGV tables
        self.setup_environment(&mut vm, keys, args, db)?;
        
        // Track execution
        let running = RunningScript {
            sha1: script.sha1.clone(),
            start_time: Instant::now(),
            kill_flag: Arc::clone(&kill_flag),
        };
        
        *self.current_script.lock().unwrap() = Some(running);
        
        // Execute script
        let result = match vm.execute_script(&script.bytecode) {
            Ok(value) => {
                // Convert to Redis response
                self.value_to_resp(value, &vm.heap)
            }
            Err(e) => {
                // Map error to Redis format
                if kill_flag.load(Ordering::Relaxed) {
                    Ok(RespFrame::Error(Arc::new("ERR Script execution aborted".into())))
                } else {
                    Ok(RespFrame::Error(Arc::new(format!("ERR {}", e).into_bytes())))
                }
            }
        };
        
        // Clear current script
        *self.current_script.lock().unwrap() = None;
        
        // Return VM to pool
        self.return_vm(vm);
        
        result
    }
    
    /// Kill the currently running script
    pub fn kill_running_script(&self) -> bool {
        let current = self.current_script.lock().unwrap();
        if let Some(script) = &*current {
            // Set kill flag
            script.kill_flag.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }
}
```

## 13. Performance Targets

| Metric | Target | Current | Improvement Factor |
|--------|--------|---------|-------------------|
| Value Size | 16 bytes | 72 bytes | 4.5x smaller |
| Table Access | 10ns per op | ~50ns per op | 5x faster |
| Table Creation | 50ns | ~200ns | 4x faster |
| Memory Usage | 30MB for 1M values | ~120MB for 1M values | 4x less memory |
| GC Pause | <5ms for 50MB heap | ~100ms for 50MB heap | 20x shorter pauses |
| Scripts/second | 200,000 ops/sec | ~50,000 ops/sec | 4x throughput |

## 14. Implementation Timeline and Priority

The implementation will proceed in the following priority order:

1. Core value representation and arena system
2. Basic VM execution loop
3. Table implementation with metamethods  
4. String interning system
5. Garbage collection
6. Closure and upvalue handling
7. Redis API integration
8. Error handling and reporting
9. Optimization and performance tuning
10. Testing and hardening

## 15. Conclusion

This specification provides a comprehensive blueprint for implementing a high-performance Lua interpreter for Ferrous using the Generational Arena + Handle pattern. The design addresses all identified issues in the current implementation while maintaining full Redis compatibility.

The handle-based approach eliminates circular reference issues, drastically reduces memory usage, and provides a more robust foundation for implementing advanced Lua features. By leveraging Rust's type system and zero-cost abstractions, this implementation will establish a new standard for what's possible in pure Rust VM implementations.
