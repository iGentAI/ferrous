# Lua VM State Machine Architecture for Ferrous

## 1. Core Architecture Principles

The Ferrous Lua VM implementation follows these fundamental principles:

1. **Non-Recursive State Machine**: The VM uses a single execution loop with explicit state transitions, never making recursive calls for any operation.

2. **Ownership-Friendly Design**: All operations work with (not against) Rust's ownership model through transaction patterns and handle-based memory management.

3. **Clean Component Separation**: The compiler, VM, and heap have clearly defined interfaces with no raw pointer usage.

4. **Safe Handle Management**: All dynamic objects use generational arena-based handles with proper validation.

## 2. Component Architecture

### 2.1 Memory Management

```
┌─────────────────┐       ┌─────────────────┐       ┌─────────────────┐
│  Generational   │       │   TypedHandle   │       │   HandleMap     │
│     Arena       │◄─────►│   <T: Resource> │◄─────►│   Operations    │
└─────────────────┘       └─────────────────┘       └─────────────────┘
```

#### 2.1.1 Generational Arena

```rust
pub struct Arena<T> {
    items: Vec<Entry<T>>,
    free: Vec<usize>,
    generation: u32,
}

enum Entry<T> {
    Occupied { generation: u32, value: T },
    Free { next_free: Option<usize> },
}
```

#### 2.1.2 Typed Handles

```rust
pub struct Handle<T> {
    index: u32,
    generation: u32,
    _phantom: PhantomData<T>,
}

// Type-specific handles for compile-time safety
pub struct StringHandle(Handle<String>);
pub struct TableHandle(Handle<Table>);
pub struct ClosureHandle(Handle<Closure>);
pub struct ThreadHandle(Handle<Thread>);
```

#### 2.1.3 Handle Validation

```rust
impl<'a> ValidScope<'a> {
    // Validate handle within a specific scope
    pub fn validate<T: Resource>(&self, handle: Handle<T>) -> Result<ValidHandle<T>> {
        if handle.generation != self.generation {
            return Err(LuaError::StaleHandle);
        }
        if !self.heap.contains(handle.index as usize) {
            return Err(LuaError::InvalidHandle);
        }
        Ok(ValidHandle { handle, _scope: PhantomData })
    }
}
```

### 2.2 Lua Heap

```rust
pub struct LuaHeap {
    generation: u32,
    strings: Arena<LuaString>,
    tables: Arena<Table>,
    closures: Arena<Closure>,
    threads: Arena<Thread>,
    metatables: HashMap<TypeId, TableHandle>,
}

impl LuaHeap {
    // Transaction-based access
    pub fn begin_transaction(&mut self) -> HeapTransaction {
        HeapTransaction {
            heap: self,
            changes: Vec::new(),
        }
    }
}
```

### 2.3 Transaction-Based Heap Access

```rust
pub struct HeapTransaction<'a> {
    heap: &'a mut LuaHeap,
    changes: Vec<HeapChange>,
}

pub enum HeapChange {
    SetTableField { table: TableHandle, key: Value, value: Value },
    SetRegister { thread: ThreadHandle, frame: usize, register: usize, value: Value },
    // Other change types...
}

impl<'a> HeapTransaction<'a> {
    // Queue changes without immediate application
    pub fn set_table_field(&mut self, table: TableHandle, key: Value, value: Value) {
        self.changes.push(HeapChange::SetTableField { table, key, value });
    }
    
    // Apply all changes in one go
    pub fn commit(self) -> Result<()> {
        for change in self.changes {
            match change {
                HeapChange::SetTableField { table, key, value } => {
                    self.heap.get_table_mut(table)?.set(key, value);
                }
                // Handle other change types...
            }
        }
        Ok(())
    }
}
```

### 2.4 Lua Values

```rust
pub enum Value {
    Nil,
    Boolean(bool),
    Number(f64),
    String(StringHandle),
    Table(TableHandle),
    Closure(ClosureHandle),
    Thread(ThreadHandle),
    CFunction(fn(&mut ExecutionContext) -> Result<i32>),
}

pub struct Table {
    array: Vec<Value>,
    map: HashMap<Value, Value>,
    metatable: Option<TableHandle>,
}

pub struct Closure {
    proto: FunctionProto,
    upvalues: Vec<UpvalueHandle>,
}
```

### 2.5 VM State Machine

```rust
pub struct LuaVM {
    heap: LuaHeap,
    current_thread: ThreadHandle,
    pending_operations: VecDeque<PendingOperation>,
    execution_state: ExecutionState,
    resource_limits: ResourceLimits,
}

pub enum ExecutionState {
    Ready,
    Running,
    Yielded,
    Completed(Value),
    Error(LuaError),
}

pub enum PendingOperation {
    FunctionCall {
        closure: ClosureHandle,
        args: Vec<Value>,
        return_context: ReturnContext,
    },
    MetamethodCall {
        method: StringHandle,
        table: TableHandle,
        args: Vec<Value>,
        return_context: ReturnContext,
    },
    IteratorCall {
        closure: ClosureHandle,
        state: Value,
        control_var: Value,
        base_register: u16,
        var_count: u8,
    },
    Concatenation {
        values: Vec<Value>,
        current_index: usize,
        dest_register: u16,
        accumulated: Vec<String>,
    },
    // Other operations...
}

pub enum ReturnContext {
    Register { base: u16, offset: usize },
    TableField { table: TableHandle, key: Value },
    FinalResult,
    ForLoop { base: u16, a: usize, c: usize },
    Metamethod { type_: MetamethodType },
}
```

### 2.6 Non-Recursive Execution Loop

```rust
impl LuaVM {
    pub fn execute_function(&mut self, closure: ClosureHandle, args: &[Value]) -> Result<Value> {
        // Record initial depth
        let initial_depth = self.get_call_depth()?;
        
        // Queue initial function call
        self.pending_operations.push_back(PendingOperation::FunctionCall {
            closure,
            args: args.to_vec(),
            return_context: ReturnContext::FinalResult,
        });
        
        // Initialize result
        let mut final_result = Value::Nil;
        
        // Main state machine loop - NO RECURSION
        while self.execution_state != ExecutionState::Error {
            // Check termination conditions
            if self.get_call_depth()? <= initial_depth && self.pending_operations.is_empty() {
                return Ok(final_result);
            }
            
            // Process any pending operations first
            if !self.pending_operations.is_empty() {
                match self.process_pending_operation(self.pending_operations.pop_front().unwrap()) {
                    Ok(_) => continue,
                    Err(e) => return Err(e),
                }
            }
            
            // Execute next instruction
            match self.step()? {
                StepResult::Continue => continue,
                StepResult::Return(value) => {
                    self.handle_return(value)?;
                    if self.get_call_depth()? <= initial_depth {
                        final_result = value;
                    }
                },
                StepResult::Yield(_) => return Err(LuaError::NotImplemented("coroutines")),
            }
        }
        
        // Handle error state
        if let ExecutionState::Error(error) = &self.execution_state {
            Err(error.clone())
        } else {
            // Shouldn't happen
            Err(LuaError::InternalError("Unexpected state after execution".into()))
        }
    }
}
```

### 2.7 Opcode Handlers

#### 2.7.1 CALL Instruction

```rust
fn handle_call_instruction(&mut self, tx: &mut HeapTransaction, instr: Instruction) -> Result<StepResult> {
    // Get function and arguments
    let a = instr.a() as usize;
    let b = instr.b() as usize;
    let c = instr.c() as usize;
    
    let func = tx.read_register(a)?;
    
    // Collect arguments
    let args = tx.collect_arguments(a + 1, if b == 0 { 255 } else { b - 1 })?;
    
    match func {
        Value::Closure(closure) => {
            // Queue function call - NEVER call execute_function directly
            tx.queue_operation(PendingOperation::FunctionCall {
                closure,
                args,
                return_context: ReturnContext::Register {
                    base: tx.current_frame_base(),
                    offset: a,
                },
            });
            
            // Return Continue to let the main loop process the pending operation
            Ok(StepResult::Continue)
        },
        Value::CFunction(cfunc) => {
            // For C functions, we can call directly (no recursion risk)
            // ... implementation ...
            Ok(StepResult::Continue)
        },
        _ => Err(LuaError::TypeError("attempt to call a non-function")),
    }
}
```

#### 2.7.2 CONCAT Instruction

```rust
fn handle_concat_instruction(&mut self, tx: &mut HeapTransaction, instr: Instruction) -> Result<StepResult> {
    // Get parameters
    let a = instr.a() as usize;
    let b = instr.b() as usize;
    let c = instr.c() as usize;
    
    // Collect all values to concatenate
    let mut values = Vec::with_capacity(c - b + 1);
    for i in b..=c {
        values.push(tx.read_register(i)?);
    }
    
    // Queue concatenation operation
    tx.queue_operation(PendingOperation::Concatenation {
        values,
        current_index: 0,
        dest_register: tx.current_frame_base() + a as u16,
        accumulated: Vec::new(),
    });
    
    Ok(StepResult::Continue)
}
```

#### 2.7.3 TFORLOOP Instruction (Generic For Loop)

```rust
fn handle_tforloop_instruction(&mut self, tx: &mut HeapTransaction, instr: Instruction) -> Result<StepResult> {
    let a = instr.a() as usize;
    let c = instr.c() as usize;
    
    // Get iterator, state, and control variable
    let iterator = tx.read_register(a)?;
    let state = tx.read_register(a + 1)?;
    let control = tx.read_register(a + 2)?;
    
    match iterator {
        Value::Closure(closure) => {
            // Queue iterator call
            tx.queue_operation(PendingOperation::IteratorCall {
                closure,
                state,
                control_var: control,
                base_register: tx.current_frame_base(),
                var_count: c,
            });
            
            Ok(StepResult::Continue)
        },
        _ => Err(LuaError::TypeError("invalid iterator (not a function)")),
    }
}
```

### 2.8 Compiler Design

```rust
// No raw pointers to heap!
pub struct Compiler {
    string_interner: StringInterner,
    register_allocator: RegisterAllocator,
    scope_stack: Vec<Scope>,
}

// Self-contained compilation output
pub struct CompiledModule {
    bytecode: Vec<Instruction>,
    constants: Vec<CompilationValue>,
    strings: Vec<String>,
    upvalues: Vec<UpvalueInfo>,
    debug_info: DebugInfo,
}

impl Compiler {
    pub fn compile(&mut self, source: &str) -> Result<CompiledModule> {
        // Complete compilation without VM heap interaction
        let ast = self.parse(source)?;
        self.generate_code(&ast)?;
        
        Ok(CompiledModule {
            bytecode: self.bytecode.clone(),
            constants: self.constants.clone(),
            strings: self.string_interner.export_strings(),
            upvalues: self.upvalues.clone(),
            debug_info: self.debug_info.clone(),
        })
    }
}
```

### 2.9 Redis-Lua Integration

```rust
// Clean GIL implementation
pub struct LuaGIL {
    vm_pool: Arc<Mutex<Vec<LuaVM>>>,
    script_cache: Arc<RwLock<HashMap<String, CompiledModule>>>,
}

// Context for Redis command execution
pub struct RedisContext {
    storage: Arc<StorageEngine>,
    db: usize,
    keys: Vec<Vec<u8>>,
    args: Vec<Vec<u8>>,
}

// Transaction-safe Redis API
impl LuaGIL {
    pub fn eval_script(&self, script: &str, context: RedisContext) -> Result<RespFrame> {
        // Get a VM from the pool
        let mut vm = self.get_vm_from_pool()?;
        
        // Set up Redis context
        self.setup_redis_context(&mut vm, &context)?;
        
        // Compile or get cached script
        let module = self.get_compiled_script(script)?;
        
        // Execute with proper error handling
        let result = match vm.execute_module(&module, &[]) {
            Ok(value) => value_to_resp(&mut vm, value),
            Err(e) => handle_lua_error(e),
        };
        
        // Return VM to pool
        self.return_vm_to_pool(vm);
        
        result
    }
}
```

## 3. Key Implementation Details

### 3.1 Handle Validation

Every handle operation must be validated before use:

```rust
fn get_table_field(&mut self, table: TableHandle, key: Value) -> Result<Value> {
    // Always validate the handle first
    if !self.heap.is_valid_handle(table) {
        return Err(LuaError::InvalidHandle);
    }
    
    // Phase 1: Read with immutable borrow
    let direct_result = {
        let table_obj = self.heap.get_table(table)?;
        table_obj.get(&key).copied()
    };
    
    if let Some(value) = direct_result {
        return Ok(value);
    }
    
    // Phase 2: Try metamethods if direct lookup failed
    // ...
}
```

### 3.2 Two-Phase Heap Access

All operations must follow a two-phase pattern to avoid borrow checker issues:

```rust
// WRONG - Fighting the borrow checker
let table_obj = self.heap.get_table(table)?;
let metatable = table_obj.metatable;
if let Some(meta) = metatable {
    // Error: table_obj still borrowed here
    let meta_obj = self.heap.get_table(meta)?;
    // ...
}

// CORRECT - Two-phase access
let metatable = {
    let table_obj = self.heap.get_table(table)?;
    table_obj.metatable
};

if let Some(meta) = metatable {
    // table_obj borrow dropped, safe to borrow again
    let meta_obj = self.heap.get_table(meta)?;
    // ...
}
```

### 3.3 Transaction Pattern for VM Operations

All VM operations should use a transaction pattern:

```rust
fn execute_instruction(&mut self, instr: Instruction) -> Result<StepResult> {
    // Create transaction for this instruction
    let mut tx = HeapTransaction::new(&mut self.heap);
    
    // Modify state through transaction
    match instr.opcode() {
        OpCode::SetTable => {
            let a = instr.a();
            let b = instr.b();
            let c = instr.c();
            
            let table = tx.read_register(b)?;
            let key = tx.read_register(c)?;
            let value = tx.read_register(a)?;
            
            tx.set_table_field(table, key, value)?;
        },
        // Handle other opcodes...
    }
    
    // Commit all changes atomically
    tx.commit()?;
    
    Ok(StepResult::Continue)
}
```

### 3.4 Pending Operations Queue

The pending operations queue is the key to avoiding recursion:

```rust
fn process_pending_operations(&mut self) -> Result<()> {
    while let Some(op) = self.pending_operations.pop_front() {
        match op {
            PendingOperation::FunctionCall { closure, args, return_context } => {
                // Validate handle
                if !self.heap.is_valid_handle(closure) {
                    return Err(LuaError::InvalidHandle);
                }
                
                // Push call frame
                let frame = CallFrame {
                    closure,
                    pc: 0,
                    base_register: self.next_available_register(),
                };
                
                self.push_call_frame(frame)?;
                
                // Setup arguments
                self.setup_arguments(&args)?;
                
                // Store return context
                self.return_contexts.insert(
                    self.get_call_depth()? - 1, 
                    return_context
                );
            },
            // Handle other operation types...
        }
    }
    
    Ok(())
}
```

### 3.5 Compiler Architecture

The compiler uses a clean, self-contained design with no raw pointers:

```rust
pub struct Compiler {
    // No raw heap pointer!
    string_interner: StringInterner,
    register_allocator: RegisterAllocator,
    scope_stack: Vec<Scope>,
    constants: Vec<ConstantValue>,
    bytecode: Vec<Instruction>,
}

impl Compiler {
    pub fn compile(&mut self, source: &str) -> Result<CompiledModule> {
        // Parse to AST
        let ast = self.parse(source)?;
        
        // Generate bytecode from AST
        self.generate_bytecode(ast)?;
        
        // Return a self-contained result
        Ok(CompiledModule {
            bytecode: self.bytecode.clone(),
            constants: self.constants.clone(),
            strings: self.string_interner.export_strings(),
            // ...other fields...
        })
    }
}

// The CompiledModule can later be loaded into a heap
impl CompiledModule {
    pub fn load_into_heap(&self, heap: &mut LuaHeap) -> Result<ClosureHandle> {
        // No raw pointers or unsafe code needed
        // Just create handles for all strings and objects
        
        // Create string map
        let mut string_map = HashMap::new();
        for (i, s) in self.strings.iter().enumerate() {
            string_map.insert(i, heap.create_string(s)?);
        }
        
        // Create function prototype
        // ...
        
        // Create closure
        heap.create_closure(proto, vec![])
    }
}
```

## 4. Implementation Strategy

### 4.1 Core Components

1. **Arena.rs**: Generational arena for memory management
2. **Heap.rs**: Lua object storage with transaction support
3. **Value.rs**: Value types and handle definitions
4. **VM.rs**: State machine execution engine
5. **Parser.rs**: Lua syntax parser
6. **Compiler.rs**: Bytecode compiler
7. **Transaction.rs**: Transaction-based heap access
8. **Error.rs**: Error handling
9. **Interop.rs**: Redis integration layer

### 4.2 Development Phases

1. **Phase 1 - Core VM Engine**: Implement arena, heap, and VM with basic opcodes
2. **Phase 2 - Compiler**: Implement parser and compiler without heap references
3. **Phase 3 - State Machine**: Implement core pending operations and function calls
4. **Phase 4 - Redis Integration**: Implement Redis API and integration layer

### 4.3 Validation Strategy

Comprehensive test suites at each layer:

1. Unit tests for arenas and handle management
2. Opcode-level tests for VM instructions
3. Function call and control flow tests
4. Full Lua compliance test suite
5. Redis integration tests

## 5. Error Handling

```rust
pub enum LuaError {
    // Compilation errors
    SyntaxError(String, usize, usize), // message, line, column
    CompileError(String),
    
    // Runtime errors
    RuntimeError(String),
    TypeError(String),
    ArgError(usize, String), // argument number, message
    
    // System errors
    StackOverflow,
    MemoryError,
    InstructionLimitExceeded,
    KilledByTimeout,
    
    // Handle errors
    InvalidHandle,
    StaleHandle,
    
    // Other errors
    InternalError(String),
    NotImplemented(String),
}
```

## 6. Performance Considerations

1. **Instruction Profiling**: Measure time spent in each instruction type
2. **Memory Analysis**: Track object creation/destruction rates
3. **Optimization Targets**: Function calls, table access, string concatenation
4. **Resource Limits**: Instructions, memory, call depth, string size

## 7. Compatibility Guarantees

The implementation must pass all tests for Redis Lua compatibility:
- EVAL/EVALSHA commands
- Proper error propagation
- All Redis commands accessible from Lua
- Standard Redis API (redis.call, redis.pcall)
- KEYS/ARGV tables
- Proper sandboxing and resource limits

## 8. Implementation Lessons Learned

Our implementation attempts have revealed several key insights that are critical to successfully implementing this architecture:

### 8.1 Transaction Consistency is Non-Negotiable

**CRITICAL:** All VM operations must use the transaction pattern consistently. Any direct heap access that bypasses transactions will cause borrow checker conflicts. This includes:

```rust
// INCORRECT - Direct heap access
let s = self.heap.create_string("index")?;

// CORRECT - Access through transaction
let s = tx.create_string("index")?;
```

### 8.2 Transaction Commit Must Not Consume Self

The transaction.commit() method must not consume self to allow for incremental operations:

```rust
// INCORRECT
pub fn commit(self) -> Result<()> { /* ... */ }

// CORRECT
pub fn commit(&mut self) -> Result<()> { /* ... */ }
```

### 8.3 Functions Must Never Call Themselves Recursively

All operations that might recursively call into the VM must be queued, not executed directly:

```rust
// INCORRECT - Direct recursive execution
let result = self.execute_function(closure, &args)?;

// CORRECT - Queue for non-recursive execution
tx.queue_operation(PendingOperation::FunctionCall {
    closure,
    args: args.clone(),
    context: ReturnContext::Register { base, offset },
});
```

### 8.4 Two-Phase Borrow Pattern is Essential

All complex operations need a two-phase borrow pattern:

```rust
// Phase 1: Gather needed handles
let metatable_handle = {
    let table_obj = self.heap.get_table(table)?;
    table_obj.metatable // Copy the handle, borrow ends here
};

// Phase 2: Use extracted handles
if let Some(metatable) = metatable_handle {
    // Safe to borrow heap again
    let meta_obj = self.heap.get_table(metatable)?;
    // ...
}
```

### 8.5. Special Handling for C Functions

C functions must be handled specially to avoid borrow checker issues:

```rust
// Extract what we need first
let func = cfunc.clone();
let args_copy = args.clone();

// Execute with clean borrow boundaries
let result = self.execute_c_function(func, &args_copy)?;
```

### 8.6 Never Mix Direct and Transaction Access

Once you start using a transaction, all subsequent operations must go through that transaction until it's committed:

```rust
// INCORRECT - Mixed access patterns
let mut tx = HeapTransaction::new(&mut self.heap);
let value1 = tx.read_register(thread, reg1)?;
let value2 = self.heap.get_thread_register(thread, reg2)?; // WRONG - direct access

// CORRECT - Consistent transaction use
let mut tx = HeapTransaction::new(&mut self.heap);
let value1 = tx.read_register(thread, reg1)?;
let value2 = tx.read_register(thread, reg2)?;
tx.commit()?;
```

## 9. Conclusion

This architecture provides a robust foundation for a Redis-compatible Lua VM in Rust that:
- Never causes stack overflow due to recursive calls
- Works with Rust's ownership model, not against it
- Manages memory safely through a handle system
- Provides complete Redis Lua compatibility
- Maintains high performance through careful memory management

By following these principles and implementation patterns, we can build a Lua VM that is both safe and efficient, while maintaining full compatibility with Redis's Lua scripting capabilities.