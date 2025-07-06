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

### Table Operations & Metamethods

The basic table operations and metamethod support have been implemented:

- **GetTable/SetTable** with proper __index/__newindex metamethod handling
- **Two-phase pattern** for metamethod resolution to avoid borrow checker issues
- **Non-recursive metamethod execution** through the pending operations queue

Core implementation:

```rust
// Metamethod resolution using two-phase pattern
pub fn resolve_metamethod(
    tx: &mut HeapTransaction, 
    value: &Value,
    mm_type: MetamethodType
) -> LuaResult<Option<Value>> {
    // Phase 1: Extract metatable information
    let metatable_opt = match value {
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

### Arithmetic & Comparison Operations

Arithmetic and comparison operations with metamethod support have been implemented:

- **Add, Sub, Mul, Div** with proper __add, __sub, __mul, __div metamethods
- **Eq, Lt, Le** with proper __eq, __lt, __le metamethods
- **String-to-number coercion** for arithmetic and comparison operations
- **Special handling for Le** with fallback to inverted Lt

### Control Flow & Loops

Control flow operations and loop constructs have been implemented:

- **Jmp, Test, TestSet** for basic control flow
- **ForPrep/ForLoop** for numeric for loops (`for i=1,10,1 do ... end`)
- **TForLoop** for generic for loops (`for k,v in pairs(t) do ... end`)
- **C function and closure iterators** support in TForLoop

### Closure System

The closure system has been fully implemented with proper upvalue handling:

- **Function Prototype** value type for first-class function prototypes
- **Closure opcode** with proper prototype extraction and upvalue capture
- **Thread-wide upvalue list** for tracking open upvalues
- **Upvalue sharing** between closures capturing the same variables
- **Close opcode** with proper upvalue closing when variables go out of scope

### Bytecode Generation and Execution

Core bytecode generation and execution has been implemented:

- **Parser** for Lua 5.1 syntax with function body and return statement support
- **AST** representation of Lua code
- **Bytecode generator** with correct opcode encoding
- **Module loading** with proper function prototype handling
- **Register allocation** with proper scope tracking
- **Stack initialization** with proper size reservations

### Parser and Compiler

The parser and compiler components have been implemented:

- **Lexical analyzer** for tokenizing source code
- **Recursive descent parser** for generating AST
- **AST data structures** representing Lua syntax
- **Register allocator** for optimizing variable storage
- **Constant pool management** for strings and numbers
- **Function prototype handling** with proper nesting support

## Recently Fixed Components (July 2025)

### 1. Bytecode Encoding

Fixed the critical issue with bytecode instruction encoding:
- **Root cause**: Opcode enum values were being directly cast to u32 instead of mapping to the correct opcode numbers
- **Impact**: Generated incorrect opcodes (ADD being encoded as SUB, RETURN being encoded as FORPREP)
- **Fix**: Modified encoding functions to use proper `opcode_to_u8` mapping function

### 2. Stack Management

Improved stack initialization and register access:
- **Root cause**: Stack was not properly initialized before function execution
- **Impact**: "Stack index out of bounds" errors during execution
- **Fix**: Properly reserve stack space based on function's max_stack_size before execution
- **Fix**: Enhanced register access safety with automatic stack growth

### 3. Function Prototype Handling

Fixed function prototype handling for nested functions:
- **Root cause**: Prototype references weren't properly transferred from compiler to module loader
- **Impact**: "Invalid function prototype index" errors when executing functions
- **Fix**: Implemented two-pass loading approach that handles forward references
- **Fix**: Proper propagation of function prototypes from code generator to module

### 4. Register Allocation

Improved register allocation for variables:
- **Root cause**: Register lifetime tracking was incomplete
- **Impact**: Inefficient register usage and potential conflicts
- **Fix**: Enhanced register allocation with proper scoping and lifetime tracking

## Implementation Phases for Remaining Work

### Phase 1: Standard Library (2-3 weeks)

#### 1.1 Basic Library Functions
- Implement `print`, `type`, `tonumber`, `tostring`, etc.
- Implement error handling functions (`error`, `assert`, `pcall`, `xpcall`)

#### 1.2 String Library
- Implement string manipulation (`string.sub`, `string.find`, `string.rep`, etc.)
- Implement pattern matching (`string.match`, `string.gsub`, etc.)

#### 1.3 Table Library
- Implement table functions (`table.insert`, `table.remove`, `table.concat`, etc.)
- Implement table.sort with custom comparators

#### 1.4 Math Library
- Implement basic math functions (`math.abs`, `math.sin`, `math.cos`, etc.)
- Implement random number generation (`math.random`, `math.randomseed`)

### Phase 2: Redis Integration (2-3 weeks)

#### 2.1 Redis Command Interface
- Implement `redis.call` and `redis.pcall` for Redis command execution
- Create sandboxing for Redis commands

#### 2.2 EVAL/EVALSHA Commands
- Implement Redis EVAL/EVALSHA command handler
- Add script caching with SHA1 hashing

#### 2.3 Key & Argument Handling
- Implement KEYS and ARGV table creation
- Proper error propagation from Redis commands

### Phase 3: Advanced Features (2-3 weeks)

#### 3.1 Metatable System Completion
- Complete implementation of all metamethods
- Add comprehensive tests for metatable behavior

#### 3.2 Error Handling
- Implement proper traceback and error messages
- Add error propagation from nested function calls

#### 3.3 Resource Limits & Security
- Implement instruction and memory limits
- Add timeout mechanism for script execution

## Comprehensive Testing Plan

1. **Unit Testing**: Test individual components in isolation
2. **Integration Testing**: Test components working together
3. **Conformance Testing**: Test compliance with Lua 5.1 specification
4. **Performance Testing**: Benchmark against Redis Lua



## Progress Tracking

This section tracks implementation progress:

| Date | Components Completed | Notes |
|------|----------------------|-------|
| 2025-06-30 | Arena, Handle, Value, Heap, basic Transaction, basic VM Core | Initial implementation with core architecture in place |
| 2025-07-02 | Handle Validation, C Function Execution | Fixed unsafe code, implemented proper type-safe validation, added C function execution pattern per architecture specs |
| 2025-07-03 | Parser, Function Bodies, Return Statements | Fixed parser to properly handle function bodies with return statements |
| 2025-07-03 | Bytecode Generation, Stack Management | Fixed bytecode generation to use the correct opcode numbers, implemented stack initialization and growth |
| 2025-07-03 | Function Prototype Loading, Register Allocation | Implemented two-pass function prototype loading, fixed register allocation and tracking |

## Current Implementation Status Summary

### ✅ Successfully Implemented and Working
- **Core VM Architecture**: Non-recursive state machine, transaction-based memory, handle validation
- **Memory Management**: Generational arena, string interning, handle management, stack management
- **Types and Values**: Basic types (nil, boolean, number, string), tables, functions, closures
- **Parser and Compiler**: Lexical analysis, syntax parsing, AST generation, bytecode generation
- **Bytecode Operations**: Arithmetic, comparisons, table access, variable access, function calls, control flow
- **Language Features**: Expressions, statements, local variables, functions, parameters, returns, tables, closures

### ⚠️ Partially Implemented/Tested
- **Tables**: Complex operations, length operations, iteration methods
- **Functions**: Multiple return values, variable arguments, method syntax
- **Metatables**: Basic support exists but needs comprehensive testing
- **Control Structures**: Advanced loop constructs, nested control flow

### ❌ Not Yet Implemented
- **Standard Library**: Basic functions, string library, table library, math library
- **Advanced Features**: Error handling, module system, coroutines
- **Redis Integration**: Command interface, EVAL/EVALSHA, script caching

## References

* [LUA_ARCHITECTURE.md](LUA_ARCHITECTURE.md): Core architectural design
* [LUA_TRANSACTION_PATTERNS.md](LUA_TRANSACTION_PATTERNS.md): Transaction pattern guidance
* [HANDLE_VALIDATION_GUIDE.md](HANDLE_VALIDATION_GUIDE.md): Handle validation instructions
* [LUA_IMPLEMENTATION_PLAN.md](LUA_IMPLEMENTATION_PLAN.md): Overall implementation plan