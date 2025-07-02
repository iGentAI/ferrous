# Lua VM Implementation Roadmap

This document outlines the implementation plan for the remaining components of the Ferrous Lua VM, reflecting the current progress and prioritizing future work.

## ✅ Completed Components

### Handle Validation System

The handle validation system has been successfully implemented with the following features:

- **Type-safe validation** via the `ValidatableHandle` trait
- **Validation caching** for performance optimization
- **Transaction boundary validation** ensuring all handles are validated at entry points
- **Pre-reallocation validation** preventing handle invalidation during memory operations
- **Context-aware error messages** for easier debugging
- **ValidScope pattern** for complex operations requiring multiple handles
- **Safe handle factory methods** replacing unsafe transmutes

Core implementation:

```rust
// Type-safe handle validation trait
pub trait ValidatableHandle: Clone + Copy {
    fn validate_against_heap(&self, heap: &LuaHeap) -> LuaResult<()>;
    fn validation_key(&self) -> ValidationKey;
}

// Implementation for transaction validation
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

// Safe handle creation without unsafe code
impl<T> Handle<T> {
    pub(crate) fn from_raw_parts(index: u32, generation: u32) -> Self {
        Handle {
            index,
            generation,
            _phantom: PhantomData,
        }
    }
}
```

### C Function Execution Pattern

The C function execution pattern has been implemented following the architectural specifications:

- **Isolated execution context** that separates C functions from direct VM access
- **Transaction-based memory safety** with proper validation
- **Clean borrow handling** that works with Rust's ownership model
- **Proper return value processing** with flexible return context handling
- **Integration with the VM execution loop** through pending operations

Core implementation:

```rust
// Execution context for C functions
pub struct ExecutionContext<'vm> {
    // Stack and argument information
    stack_base: usize,
    arg_count: usize,
    thread: ThreadHandle,
    
    // Private handle to VM for controlled access
    vm_access: &'vm mut LuaVM,
}

// Transaction-safe operations
pub fn get_arg(&mut self, index: usize) -> LuaResult<Value> {
    // Create fresh transaction for each operation
    let mut tx = HeapTransaction::new(&mut self.vm_access.heap);
    let value = tx.read_register(self.thread, self.stack_base + index)?;
    tx.commit()?;
    
    Ok(value)
}

// C function handling in VM
fn handle_c_function_call(
    &mut self,
    func: CFunction,
    args: Vec<Value>,
    base_register: u16,
    register_a: usize,
    thread_handle: ThreadHandle,
) -> LuaResult<StepResult> {
    // Create execution context with clean borrows
    let mut ctx = ExecutionContext::new(self, base_register as usize + register_a, args.len(), thread_handle);
    
    // Execute C function in isolation
    let result_count = func(&mut ctx)?;
    
    // Collect results and queue for processing
    // ...
}
```

## Remaining Implementation Phases

### Phase 1: Metamethod System (Days 1-3)

The metamethod system is the next priority as it's required for proper Lua semantics:

#### 1.1 Create Metamethod Types

```rust
// In metamethod.rs
pub enum MetamethodType {
    Index,        // __index
    NewIndex,     // __newindex
    Call,         // __call
    Add,          // __add
    Sub,          // __sub
    // ... other metamethods ...
}
```

#### 1.2 Implement Metamethod Resolution

Using the two-phase pattern for proper borrow management:

```rust
// In metamethod.rs
pub fn resolve_metamethod(
    tx: &mut HeapTransaction, 
    value: Value,
    mm_type: MetamethodType
) -> LuaResult<Option<Value>> {
    // Phase 1: Extract metatable information
    let metatable_opt = match &value {
        Value::Table(handle) => tx.get_table_metatable(*handle)?,
        Value::UserData(handle) => tx.get_userdata_metatable(*handle)?,
        _ => None,
    };
    
    // Early return if no metatable
    let Some(metatable) = metatable_opt else {
        return Ok(None);
    };
    
    // Phase 2: Look up metamethod with a fresh borrow
    let mm_name = tx.create_string(mm_type.name())?;
    let mm_key = Value::String(mm_name);
    let metamethod = tx.read_table_field(metatable, &mm_key)?;
    
    if metamethod.is_nil() {
        Ok(None)
    } else {
        Ok(Some(metamethod))
    }
}
```

#### 1.3 Non-Recursive Metamethod Processing

```rust
// Add to PendingOperation enum
pub enum PendingOperation {
    // ... other operations ...
    MetamethodCall {
        mm_type: MetamethodType,
        target: Value,
        args: Vec<Value>,
        continuation: MetamethodContext,
    },
    // ...
}

// In vm.rs
fn process_metamethod_call(
    &mut self,
    mm_type: MetamethodType,
    target: Value,
    args: Vec<Value>,
    continuation: MetamethodContext,
) -> LuaResult<StepResult> {
    // Implementation that follows the non-recursive pattern
    // ...
}
```

### Phase 2: VM Operations Implementation (Days 4-7)

#### 2.1 Table Operations

Implement GetTable and SetTable with metamethod support:

```rust
fn execute_get_table(&mut self, tx: &mut HeapTransaction, frame: &CallFrame, a: usize, b: usize, c: usize) -> LuaResult<StepResult> {
    // Implementation with __index metamethod support
    // ...
}

fn execute_set_table(&mut self, tx: &mut HeapTransaction, frame: &CallFrame, a: usize, b: usize, c: usize) -> LuaResult<StepResult> {
    // Implementation with __newindex metamethod support
    // ...
}
```

#### 2.2 Arithmetic Operations

Implement Add, Sub, Mul, Div, etc. with metamethod support:

```rust
fn execute_arithmetic(&mut self, tx: &mut HeapTransaction, frame: &CallFrame, a: usize, b: usize, c: usize, op: ArithmeticOperation) -> LuaResult<StepResult> {
    // Implementation with appropriate metamethod support
    // ...
}
```

#### 2.3 Comparison Operations

Implement Eq, Lt, Le with metamethod support:

```rust
fn execute_comparison(&mut self, tx: &mut HeapTransaction, frame: &CallFrame, a: usize, b: usize, c: usize, op: ComparisonOperation) -> LuaResult<StepResult> {
    // Implementation with appropriate metamethod support
    // ...
}
```

### Phase 3: Redis API Integration (Days 8-10)

#### 3.1 Create Redis API Module

```rust
// redis_api.rs
pub struct RedisApiContext {
    pub storage: Arc<StorageEngine>,
    pub db: usize,
    pub keys: Vec<Vec<u8>>,
    pub argv: Vec<Vec<u8>>,
}
```

#### 3.2 Implement Redis Call and PCAll

```rust
// redis.call implementation
pub fn redis_call(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    // Implementation using C function pattern
    // ...
}

// redis.pcall implementation
pub fn redis_pcall(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    // Implementation with error catching
    // ...
}
```

#### 3.3 Register Redis API Functions

```rust
fn setup_redis_api(&mut self, tx: &mut HeapTransaction) -> LuaResult<TableHandle> {
    // Create redis table
    let table = tx.create_table()?;
    
    // Register functions
    let call_name = tx.create_string("call")?;
    tx.set_table_field(table, Value::String(call_name), Value::CFunction(redis_call))?;
    
    let pcall_name = tx.create_string("pcall")?;
    tx.set_table_field(table, Value::String(pcall_name), Value::CFunction(redis_pcall))?;
    
    // ... other Redis functions ...
    
    Ok(table)
}
```

### Phase 4: Compiler Implementation (Days 11-15)

#### 4.1 Create Parser

Implement a Lua 5.1 parser using recursive descent:

```rust
pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

impl Parser {
    pub fn parse(&mut self) -> Result<Chunk, ParseError> {
        self.chunk()
    }
    
    fn chunk(&mut self) -> Result<Chunk, ParseError> {
        // ...
    }
    
    // ... other parse methods ...
}
```

#### 4.2 Create AST Representation

```rust
pub enum Expr {
    Nil,
    Boolean(bool),
    Number(f64),
    String(String),
    Table(TableConstructor),
    Function(FunctionDef),
    Var(Variable),
    BinaryOp(Box<Expr>, BinaryOp, Box<Expr>),
    UnaryOp(UnaryOp, Box<Expr>),
    // ...
}
```

#### 4.3 Implement Bytecode Generation

```rust
pub struct Compiler {
    current_function: FunctionBuilder,
    scope_stack: Vec<Scope>,
    // ...
}

impl Compiler {
    pub fn compile(&mut self, ast: &Chunk) -> Result<FunctionProto, CompileError> {
        // ...
    }
    
    fn compile_expr(&mut self, expr: &Expr) -> Result<(), CompileError> {
        // ...
    }
    
    // ... other compile methods ...
}
```

### Phase 5: Testing and Integration (Days 16-20)

#### 5.1 Create Component Tests

Implement detailed tests for each component:

```rust
#[test]
fn test_metamethod_resolution() {
    // Test metamethod lookup
    // ...
}

#[test]
fn test_arithmetic_metamethods() {
    // Test arithmetic operations with metamethods
    // ...
}

// ... other tests ...
```

#### 5.2 Create Integration Tests

Test the complete VM with complex scripts:

```rust
#[test]
fn test_complex_script() {
    // Test a script with tables, functions, and metamethods
    // ...
}

#[test]
fn test_redis_integration() {
    // Test Redis API integration
    // ...
}

// ... other tests ...
```

## Progress Tracking

This section tracks implementation progress:

| Date | Components Completed | Notes |
|------|----------------------|-------|
| 2025-06-30 | Arena, Handle, Value, Heap, basic Transaction, basic VM Core | Initial implementation with core architecture in place |
| 2025-07-02 | Handle Validation, C Function Execution | Fixed unsafe code, implemented proper type-safe validation, added C function execution pattern per architecture specs |
| | | |

## References

* [LUA_ARCHITECTURE.md](LUA_ARCHITECTURE.md): Core architectural design
* [LUA_TRANSACTION_PATTERNS.md](LUA_TRANSACTION_PATTERNS.md): Transaction pattern guidance
* [HANDLE_VALIDATION_GUIDE.md](HANDLE_VALIDATION_GUIDE.md): Handle validation instructions
* [LUA_IMPLEMENTATION_PLAN.md](LUA_IMPLEMENTATION_PLAN.md): Overall implementation plan