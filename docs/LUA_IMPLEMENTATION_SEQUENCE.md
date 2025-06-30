# Lua VM Implementation Sequence

This document provides a detailed, step-by-step implementation sequence for building the Ferrous Lua VM following the design principles in `LUA_ARCHITECTURE.md`. It shows the exact order and structure for implementing each component to ensure a clean, consistent architecture.

## Phase 1: Memory Management Infrastructure

### 1. Basic Arena Implementation

Start with a simple arena without generational aspects:

```rust
// In src/lua/arena.rs

pub struct Arena<T> {
    entries: Vec<Entry<T>>,
    free: Vec<usize>,
}

enum Entry<T> {
    Occupied(T),
    Free,
}

pub struct Handle<T> {
    index: u32,
    _phantom: PhantomData<T>,
}

impl<T> Arena<T> {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            free: Vec::new(),
        }
    }
    
    pub fn insert(&mut self, value: T) -> Handle<T> {
        let index = if let Some(free_index) = self.free.pop() {
            self.entries[free_index] = Entry::Occupied(value);
            free_index
        } else {
            let index = self.entries.len();
            self.entries.push(Entry::Occupied(value));
            index
        };
        
        Handle {
            index: index as u32,
            _phantom: PhantomData,
        }
    }
    
    pub fn get(&self, handle: &Handle<T>) -> Option<&T> {
        let index = handle.index as usize;
        if index < self.entries.len() {
            match &self.entries[index] {
                Entry::Occupied(value) => Some(value),
                Entry::Free => None,
            }
        } else {
            None
        }
    }
    
    // ...etc...
}
```

### 2. Add Generational Aspect

Extend the arena to handle generational validation:

```rust
pub struct Arena<T> {
    entries: Vec<Entry<T>>,
    free: Vec<usize>,
    generation: u32,
}

enum Entry<T> {
    Occupied { value: T, generation: u32 },
    Free { next_free: Option<usize> },
}

pub struct Handle<T> {
    index: u32,
    generation: u32,
    _phantom: PhantomData<T>,
}

impl<T> Arena<T> {
    pub fn insert(&mut self, value: T) -> Handle<T> {
        self.generation = self.generation.wrapping_add(1);
        
        let index = if let Some(free_index) = self.free.pop() {
            self.entries[free_index] = Entry::Occupied { 
                value, 
                generation: self.generation 
            };
            free_index
        } else {
            let index = self.entries.len();
            self.entries.push(Entry::Occupied { 
                value, 
                generation: self.generation 
            });
            index
        };
        
        Handle {
            index: index as u32,
            generation: self.generation,
            _phantom: PhantomData,
        }
    }
    
    pub fn get(&self, handle: &Handle<T>) -> Option<&T> {
        let index = handle.index as usize;
        if index < self.entries.len() {
            match &self.entries[index] {
                Entry::Occupied { value, generation } if *generation == handle.generation => {
                    Some(value)
                },
                _ => None,
            }
        } else {
            None
        }
    }
    
    // ...etc...
}
```

### 3. Create Typed Handles

Add typed handles for stronger type safety:

```rust
pub struct TypedHandle<T>(pub Handle<T>);

impl<T> TypedHandle<T> {
    // Implement Clone, Copy, Debug, etc.
}

// Create specific handle types
pub type StringHandle = TypedHandle<LuaString>;
pub type TableHandle = TypedHandle<Table>;
pub type ClosureHandle = TypedHandle<Closure>;
pub type ThreadHandle = TypedHandle<Thread>;
```

## Phase 2: Value System

### 1. Basic Value Types

Implement the Lua value types:

```rust
// In src/lua/value.rs

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

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Boolean(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Table(_) => "table",
            Value::Closure(_) => "function",
            Value::Thread(_) => "thread",
            Value::CFunction(_) => "function",
        }
    }
    
    // Implement is_* methods, comparison, etc.
}
```

### 2. Complex Objects

Implement the complex object types:

```rust
pub struct LuaString {
    pub bytes: Vec<u8>,
}

pub struct Table {
    pub array: Vec<Value>,
    pub hash_map: Vec<(Value, Value)>, // Simple implementation for now
    pub metatable: Option<TableHandle>,
}

pub struct Closure {
    pub proto: FunctionProto,
    pub upvalues: Vec<UpvalueHandle>,
}

pub struct Thread {
    pub call_frames: Vec<CallFrame>,
    pub stack: Vec<Value>,
    pub status: ThreadStatus,
}

pub struct FunctionProto {
    pub bytecode: Vec<u32>,
    pub constants: Vec<Value>,
    // ... other fields ...
}

pub struct CallFrame {
    pub closure: ClosureHandle,
    pub pc: usize,
    pub base_register: u16,
    // ... other fields ...
}
```

## Phase 3: Heap Implementation

### 1. Basic Heap Structure

Implement the core heap structure:

```rust
// In src/lua/heap.rs

pub struct LuaHeap {
    strings: Arena<LuaString>,
    tables: Arena<Table>,
    closures: Arena<Closure>,
    threads: Arena<Thread>,
    globals: Option<TableHandle>,
    registry: Option<TableHandle>,
    main_thread: Option<ThreadHandle>,
}

impl LuaHeap {
    pub fn new() -> Self {
        let mut heap = LuaHeap {
            strings: Arena::new(),
            tables: Arena::new(),
            closures: Arena::new(),
            threads: Arena::new(),
            globals: None,
            registry: None,
            main_thread: None,
        };
        
        // Initialize globals, registry, main thread
        let globals = heap.create_table_internal().unwrap();
        let registry = heap.create_table_internal().unwrap();
        let main_thread = heap.create_thread_internal().unwrap();
        
        heap.globals = Some(globals);
        heap.registry = Some(registry);
        heap.main_thread = Some(main_thread);
        
        heap
    }
    
    // Implement create_*, get_*, etc. methods for each object type
}
```

### 2. Transaction System

Implement the transaction system:

```rust
// In src/lua/transaction.rs

pub struct HeapTransaction<'a> {
    heap: &'a mut LuaHeap,
    changes: Vec<HeapChange>,
    read_set: HashSet<ResourceId>,
    write_set: HashSet<ResourceId>,
    created_strings: HashMap<String, StringHandle>,
    created_tables: Vec<TableHandle>,
}

pub enum HeapChange {
    SetTableField { table: TableHandle, key: Value, value: Value },
    SetRegister { thread: ThreadHandle, index: usize, value: Value },
    // ... other change types ...
}

pub enum ResourceId {
    TableField(TableHandle, u32), // Hash of the key
    ThreadRegister(ThreadHandle, usize),
    // ... other resource types ...
}

impl<'a> HeapTransaction<'a> {
    pub fn new(heap: &'a mut LuaHeap) -> Self {
        Self {
            heap,
            changes: Vec::new(),
            read_set: HashSet::new(),
            write_set: HashSet::new(),
            created_strings: HashMap::new(),
            created_tables: Vec::new(),
        }
    }
    
    // Important: commit() takes &mut self, not self
    pub fn commit(&mut self) -> Result<()> {
        for change in self.changes.drain(..) {
            match change {
                HeapChange::SetTableField { table, key, value } => {
                    self.heap.set_table_field_internal(table, key, value)?;
                },
                HeapChange::SetRegister { thread, index, value } => {
                    self.heap.set_thread_register_internal(thread, index, value)?;
                },
                // ... other change types ...
            }
        }
        Ok(())
    }
    
    // Implement set_*, get_* methods for each operation
}
```

## Phase 4: VM Core

### 1. Basic VM Structure

Implement the core VM structure:

```rust
// In src/lua/vm.rs

pub struct LuaVM {
    heap: LuaHeap,
    current_thread: ThreadHandle,
    pending_operations: VecDeque<PendingOperation>,
    call_contexts: HashMap<usize, PostCallContext>,
}

pub enum PendingOperation {
    FunctionCall {
        closure: ClosureHandle,
        args: Vec<Value>,
        context: PostCallContext,
    },
    MetamethodCall {
        method_name: StringHandle,
        table: TableHandle,
        key: Value,
        context: PostCallContext,
    },
    // ... other operation types ...
}

pub enum PostCallContext {
    Normal { return_register: Option<(u16, usize)> },
    Iterator { base_register: u16, register_a: usize, var_count: usize },
    Metamethod { method: String, return_type: MetamethodReturnType },
    // ... other contexts ...
}

pub enum ExecutionStatus {
    Continue,
    Return(Value),
    Call(ClosureHandle, Vec<Value>),
    Yield(Value),
}
```

### 2. Non-Recursive Execution Loop

Implement the main execution loop:

```rust
impl LuaVM {
    pub fn execute_function(&mut self, closure: ClosureHandle, args: &[Value]) -> Result<Value> {
        // Record initial call depth
        let initial_depth = self.get_call_depth()?;
        
        // Push initial call frame
        self.push_call_frame(closure, args)?;
        
        // Initialize result
        let mut final_result = Value::Nil;
        
        // Main execution loop - NO RECURSION
        let mut done = false;
        while !done {
            // Check termination conditions
            if self.should_kill() {
                return Err(LuaError::ScriptKilled);
            }
            
            // Process any pending operations first
            if !self.pending_operations.is_empty() {
                let op = self.pending_operations.pop_front().unwrap();
                match self.process_pending_operation(op)? {
                    ExecutionStatus::Continue => {},
                    ExecutionStatus::Return(value) => {
                        final_result = value.clone();
                        
                        // Check if we're back to initial depth
                        if self.get_call_depth()? <= initial_depth {
                            done = true;
                        } else {
                            // Handle return in current context
                            // ...
                        }
                    },
                    // ... other statuses ...
                }
                
                if done {
                    break;
                } else {
                    continue;
                }
            }
            
            // Execute next instruction
            match self.step()? {
                ExecutionStatus::Continue => {},
                ExecutionStatus::Return(value) => {
                    final_result = value.clone();
                    
                    // Pop the frame
                    self.pop_call_frame()?;
                    
                    // Check if we're back to initial depth
                    if self.get_call_depth()? <= initial_depth {
                        done = true;
                    } else {
                        // Handle return in current context
                        // ...
                    }
                },
                // ... other statuses ...
            }
        }
        
        Ok(final_result)
    }
    
    fn step(&mut self) -> Result<ExecutionStatus> {
        // Create transaction
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Get current frame and instruction
        let frame = tx.get_current_frame(self.current_thread)?;
        let instr = tx.get_instruction(frame.closure, frame.pc)?;
        
        // Execute instruction
        let result = self.execute_instruction(&mut tx, frame, instr)?;
        
        // Commit transaction
        tx.commit()?;
        
        Ok(result)
    }
    
    fn execute_instruction(&mut self, tx: &mut HeapTransaction, frame: &CallFrame, instr: Instruction) -> Result<ExecutionStatus> {
        let opcode = instr.opcode();
        let a = instr.a();
        let b = instr.b();
        let c = instr.c();
        
        match opcode {
            OpCode::Move => self.execute_move(tx, frame, a, b),
            OpCode::LoadK => self.execute_loadk(tx, frame, a, instr.bx()),
            // ... other opcodes ...
            OpCode::Call => self.execute_call(tx, frame, a, b, c),
            // ... etc ...
            _ => Err(LuaError::NotImplemented(format!("Opcode {:?} not implemented", opcode))),
        }
    }
}
```

### 3. Instruction Handling

Implement individual instruction handlers:

```rust
impl LuaVM {
    fn execute_move(&mut self, tx: &mut HeapTransaction, frame: &CallFrame, a: usize, b: usize) -> Result<ExecutionStatus> {
        let base = frame.base_register as usize;
        let value = tx.read_register(self.current_thread, base + b)?;
        
        tx.set_register(self.current_thread, base + a, value);
        tx.increment_pc(self.current_thread)?;
        
        Ok(ExecutionStatus::Continue)
    }
    
    fn execute_call(&mut self, tx: &mut HeapTransaction, frame: &CallFrame, a: usize, b: usize, c: usize) -> Result<ExecutionStatus> {
        let base = frame.base_register as usize;
        let func = tx.read_register(self.current_thread, base + a)?;
        
        // Gather arguments
        let arg_count = if b == 0 {
            tx.get_stack_top(self.current_thread)? - base - a - 1
        } else {
            b - 1
        };
        
        let mut args = Vec::with_capacity(arg_count);  
        for i in 0..arg_count {
            args.push(tx.read_register(self.current_thread, base + a + 1 + i)?);  
        }
        
        // Increment PC before proceeding
        tx.increment_pc(self.current_thread)?;
        
        // Process based on function type
        match func {
            Value::Closure(closure) => {
                // Queue function call
                tx.queue_operation(PendingOperation::FunctionCall {
                    closure,
                    args,
                    context: PostCallContext::Normal {
                        return_register: Some((frame.base_register, a)),
                    },
                });
                
                Ok(ExecutionStatus::Continue)
            },
            Value::CFunction(cfunc) => {
                // Handle C function call
                // ... implementation ...
                
                Ok(ExecutionStatus::Continue)
            },
            _ => {
                Err(LuaError::TypeError(format!("attempt to call a {} value", func.type_name())))
            }
        }
    }
    
    // ... implement handlers for all opcodes ...
}
```

## Phase 5: Compiler Integration

### 1. Compiler Interface

Define a clean compiler interface:

```rust
// In src/lua/compiler.rs

pub struct Compiler {
    string_interner: StringInterner,
    register_allocator: RegisterAllocator,
    scope_stack: Vec<Scope>,
    // ... other state ...
}

pub struct CompiledModule {
    bytecode: Vec<u32>,
    constants: Vec<CompilationValue>,
    strings: Vec<String>,
    upvalues: Vec<UpvalueInfo>,
    debug_info: DebugInfo,
}

impl Compiler {
    pub fn new() -> Self {
        // ... implementation ...
    }
    
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
    
    // ... other methods ...
}
```

### 2. VM-Compiler Bridging

Implement loading compiled modules into the VM:

```rust
// In src/lua/vm.rs

impl LuaVM {
    pub fn load_module(&mut self, module: &CompiledModule) -> Result<ClosureHandle> {
        let mut string_map = HashMap::new();
        
        // Create strings in the heap
        for (i, s) in module.strings.iter().enumerate() {
            let handle = self.heap.create_string(s)?;
            string_map.insert(i, handle);
        }
        
        // Create function prototype
        let proto = FunctionProto {
            bytecode: module.bytecode.clone(),
            constants: self.convert_constants(&module.constants, &string_map)?,
            upvalues: self.convert_upvalues(&module.upvalues, &string_map)?,
            // ... other fields ...
        };
        
        // Create closure with empty upvalue list
        self.heap.create_closure(proto, Vec::new())
    }
    
    // ... helper methods ...
}
```

## Phase 6: Redis Integration

### 1. Redis API Context

Implement the Redis API context:

```rust
// In src/lua/redis_api.rs

pub struct RedisApiContext {
    pub storage: Arc<StorageEngine>,
    pub db: usize,
    pub keys: Vec<Vec<u8>>,
    pub argv: Vec<Vec<u8>>,
}

impl RedisApiContext {
    // ... implementation ...
}
```

### 2. Redis API Registration

Implement registering the Redis API with a VM:

```rust
// In src/lua/redis_api.rs

pub fn register_redis_api(vm: &mut LuaVM, context: RedisApiContext) -> Result<()> {
    // Setup KEYS and ARGV tables
    let keys_table = vm.create_table()?;
    let argv_table = vm.create_table()?;
    
    // Fill KEYS table
    for (i, key) in context.keys.iter().enumerate() {
        let key_str = vm.create_string(&String::from_utf8_lossy(key))?;
        vm.set_table_index(keys_table, i + 1, Value::String(key_str))?;
    }
    
    // Fill ARGV table
    for (i, arg) in context.argv.iter().enumerate() {
        let arg_str = vm.create_string(&String::from_utf8_lossy(arg))?;
        vm.set_table_index(argv_table, i + 1, Value::String(arg_str))?;
    }
    
    // Create global KEYS and ARGV
    let globals = vm.globals();
    
    let keys_name = vm.create_string("KEYS")?;
    vm.set_table(globals, Value::String(keys_name), Value::Table(keys_table))?;
    
    let argv_name = vm.create_string("ARGV")?;
    vm.set_table(globals, Value::String(argv_name), Value::Table(argv_table))?;
    
    // Register redis.* functions
    let redis_table = vm.create_table()?;
    
    // Register redis.call, redis.pcall, etc.
    // ... implementation ...
    
    // Add redis table to globals
    let redis_name = vm.create_string("redis")?;
    vm.set_table(globals, Value::String(redis_name), Value::Table(redis_table))?;
    
    Ok(())
}
```

### 3. Redis Command Execution

Implement Redis command execution from Lua:

```rust
// In src/lua/redis_api.rs

fn redis_call(ctx: &mut ExecutionContext) -> Result<i32> {
    // Get Redis context
    let redis_ctx = get_redis_context(ctx)?;
    
    // Get command name and args from Lua stack
    let cmd_name = ctx.get_arg_str(0)?;
    let mut args = Vec::new();
    
    for i in 1..ctx.arg_count {
        let arg = ctx.get_arg(i)?;
        args.push(lua_to_redis_arg(arg)?);
    }
    
    // Execute Redis command
    let result = redis_ctx.execute_command(&cmd_name, &args)?;
    
    // Convert result to Lua value
    let lua_result = redis_to_lua_value(ctx, &result)?;
    
    // Push result to Lua stack
    ctx.push_result(lua_result)?;
    
    // Return number of results (1)
    Ok(1)
}
```

## Implementation Order Summary

To successfully implement the Ferrous Lua VM, follow this sequence:

1. Arena and handle system
2. Value type system
3. Heap storage
4. Transaction system
5. VM core structure
6. State machine and operation queue
7. Basic opcode handlers
8. Function call mechanism
9. Parser and AST
10. Compiler
11. Standard library
12. Redis API integration

By implementing in this order, each component builds cleanly on the previous ones, maintaining architectural integrity throughout the process.