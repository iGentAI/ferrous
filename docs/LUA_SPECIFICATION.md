# Ferrous Lua 5.1 Interpreter Specification

## Executive Summary

This specification defines the implementation of a minimal, pure Rust Lua 5.1 interpreter for Ferrous, providing Redis-compatible scripting functionality through EVAL/EVALSHA commands. The interpreter prioritizes security, determinism, and seamless integration with Ferrous's multi-threaded architecture while maintaining the zero-dependency philosophy.

## 1. Architecture Overview

### 1.1 Design Principles

```rust
// Core design constraints
pub struct LuaDesignPrinciples {
    zero_dependencies: bool,        // No external crates beyond std
    redis_compatible: bool,         // Matches Redis Lua behavior exactly
    memory_safe: bool,              // Rust safety guarantees
    deterministic: bool,            // Same input = same output
    sandboxed: bool,               // No filesystem/network access
    performance_focused: bool,      // Minimal overhead
}
```

### 1.2 High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Ferrous Server                           │
├─────────────────────────────────────────────────────────────┤
│                  Command Layer                               │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐    │
│  │   EVAL      │  │  EVALSHA    │  │  SCRIPT        │    │
│  │   Handler   │  │  Handler    │  │  Commands      │    │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────────┘    │
│         └────────────────┴────────────────┴─────────┐      │
├─────────────────────────────────────────────────────▼─────┤
│                    Lua Engine Layer                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐    │
│  │  Script     │  │    Lua VM   │  │  Redis API     │    │
│  │  Cache      │  │  Instances  │  │  Bridge        │    │
│  └─────────────┘  └─────────────┘  └─────────────────┘    │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐    │
│  │  Bytecode   │  │  Memory     │  │  Security      │    │
│  │  Compiler   │  │  Allocator  │  │  Sandbox       │    │
│  └─────────────┘  └─────────────┘  └─────────────────┘    │
├─────────────────────────────────────────────────────────────┤
│                  Storage Engine                             │
└─────────────────────────────────────────────────────────────┘
```

## 2. Lua VM Core Design

### 2.1 VM Structure

```rust
pub struct LuaVm {
    /// Stack for Lua values
    stack: LuaStack,
    
    /// Global environment
    globals: LuaTable,
    
    /// Currently executing function
    call_stack: Vec<CallFrame>,
    
    /// Memory tracking
    memory_used: usize,
    memory_limit: usize,
    
    /// Instruction counter for timeout
    instruction_count: u64,
    instruction_limit: u64,
    
    /// String interning pool
    strings: StringPool,
    
    /// Upvalue management
    open_upvalues: Vec<UpvalueRef>,
}

pub struct CallFrame {
    /// Function being called
    closure: LuaClosure,
    
    /// Program counter
    pc: usize,
    
    /// Base pointer in stack
    base: usize,
    
    /// Number of expected results
    nresults: i32,
}
```

### 2.2 Value Representation

```rust
#[derive(Clone)]
pub enum LuaValue {
    Nil,
    Boolean(bool),
    Number(f64),
    String(LuaString),
    Table(Rc<RefCell<LuaTable>>),
    Function(LuaFunction),
    Thread(LuaThread),
    UserData(LuaUserData),
}

/// Interned string for efficiency
pub struct LuaString {
    bytes: Arc<Vec<u8>>,
    hash: u64,
}

pub enum LuaFunction {
    /// Lua function (compiled bytecode)
    Lua(Rc<LuaClosure>),
    
    /// Rust function callable from Lua
    Rust(LuaRustFn),
}

pub type LuaRustFn = fn(&mut LuaVm) -> Result<i32>;
```

### 2.3 Bytecode Representation

```rust
/// Lua 5.1 compatible instruction format
#[repr(u32)]
pub struct Instruction(u32);

impl Instruction {
    pub fn opcode(&self) -> OpCode {
        OpCode::from_u8((self.0 & 0x3F) as u8)
    }
    
    pub fn a(&self) -> u8 {
        ((self.0 >> 6) & 0xFF) as u8
    }
    
    pub fn b(&self) -> u16 {
        ((self.0 >> 14) & 0x1FF) as u16
    }
    
    pub fn c(&self) -> u16 {
        ((self.0 >> 23) & 0x1FF) as u16
    }
    
    pub fn bx(&self) -> u32 {
        (self.0 >> 14) & 0x3FFFF
    }
    
    pub fn sbx(&self) -> i32 {
        (self.bx() as i32) - 131071
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(u8)]
pub enum OpCode {
    Move,
    LoadK,
    LoadBool,
    LoadNil,
    GetUpval,
    GetGlobal,
    GetTable,
    SetGlobal,
    SetUpval,
    SetTable,
    NewTable,
    Self_,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Unm,
    Not,
    Len,
    Concat,
    Jmp,
    Eq,
    Lt,
    Le,
    Test,
    TestSet,
    Call,
    TailCall,
    Return,
    ForLoop,
    ForPrep,
    TForLoop,
    SetList,
    Close,
    Closure,
    Vararg,
}
```

## 3. Redis API Integration

### 3.1 Redis Command Bridge

```rust
pub struct RedisApi {
    /// Connection to storage engine
    engine: Arc<StorageEngine>,
    
    /// Current database
    db: DatabaseIndex,
    
    /// Command execution mode
    mode: ExecMode,
    
    /// Call statistics
    stats: CallStats,
}

#[derive(Clone, Copy)]
pub enum ExecMode {
    /// redis.call() - errors abort script
    Call,
    
    /// redis.pcall() - errors return as values
    PCall,
}

impl RedisApi {
    /// Register Redis API in Lua global environment
    pub fn register(&self, vm: &mut LuaVm) -> Result<()> {
        let redis_table = vm.create_table();
        
        // redis.call()
        redis_table.set("call", vm.create_function(redis_call)?)?;
        
        // redis.pcall()
        redis_table.set("pcall", vm.create_function(redis_pcall)?)?;
        
        // redis.log()
        redis_table.set("log", vm.create_function(redis_log)?)?;
        
        // redis.sha1hex()
        redis_table.set("sha1hex", vm.create_function(redis_sha1hex)?)?;
        
        // redis.error_reply()
        redis_table.set("error_reply", vm.create_function(redis_error_reply)?)?;
        
        // redis.status_reply()
        redis_table.set("status_reply", vm.create_function(redis_status_reply)?)?;
        
        // Constants
        redis_table.set("LOG_DEBUG", LuaValue::Number(0.0))?;
        redis_table.set("LOG_VERBOSE", LuaValue::Number(1.0))?;
        redis_table.set("LOG_NOTICE", LuaValue::Number(2.0))?;
        redis_table.set("LOG_WARNING", LuaValue::Number(3.0))?;
        
        vm.set_global("redis", redis_table)?;
        Ok(())
    }
}

/// Implementation of redis.call()
fn redis_call(vm: &mut LuaVm) -> Result<i32> {
    let nargs = vm.get_top();
    if nargs < 1 {
        return Err(LuaError::Runtime("wrong number of arguments".into()));
    }
    
    // Extract command name and arguments
    let cmd_name = vm.to_string(1)?;
    let mut args = Vec::with_capacity(nargs - 1);
    
    for i in 2..=nargs {
        args.push(lua_to_redis_value(vm.get(i)?)?);
    }
    
    // Execute Redis command
    let result = execute_redis_command(&cmd_name, args)?;
    
    // Convert result back to Lua
    vm.push(redis_to_lua_value(result)?);
    Ok(1)
}
```

### 3.2 Type Conversions

```rust
/// Convert Lua value to Redis protocol value
fn lua_to_redis_value(val: LuaValue) -> Result<RespFrame> {
    match val {
        LuaValue::Nil => Ok(RespFrame::Null),
        LuaValue::Boolean(false) => Ok(RespFrame::Null),
        LuaValue::Boolean(true) => Ok(RespFrame::Integer(1)),
        LuaValue::Number(n) => {
            if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
                Ok(RespFrame::Integer(n as i64))
            } else {
                Ok(RespFrame::BulkString(Some(Arc::new(n.to_string().into_bytes()))))
            }
        }
        LuaValue::String(s) => Ok(RespFrame::BulkString(Some(s.bytes))),
        LuaValue::Table(t) => {
            // Check if array-like
            let table = t.borrow();
            if is_array_like(&table) {
                let mut arr = Vec::new();
                for i in 1..=table.len() {
                    if let Some(v) = table.get(&LuaValue::Number(i as f64)) {
                        arr.push(lua_to_redis_value(v.clone())?);
                    }
                }
                Ok(RespFrame::Array(Some(arr)))
            } else {
                Err(LuaError::Runtime("Cannot convert Lua table to Redis value".into()))
            }
        }
        _ => Err(LuaError::Runtime("Cannot convert Lua type to Redis value".into())),
    }
}

/// Convert Redis value to Lua value
fn redis_to_lua_value(frame: RespFrame) -> Result<LuaValue> {
    match frame {
        RespFrame::SimpleString(s) => Ok(LuaValue::String(LuaString::from_bytes(s))),
        RespFrame::Error(e) => Ok(create_error_table(e)),
        RespFrame::Integer(i) => Ok(LuaValue::Number(i as f64)),
        RespFrame::BulkString(Some(s)) => Ok(LuaValue::String(LuaString::from_bytes(s))),
        RespFrame::BulkString(None) => Ok(LuaValue::Boolean(false)),
        RespFrame::Array(Some(arr)) => {
            let table = LuaTable::new();
            for (i, frame) in arr.into_iter().enumerate() {
                table.set(i + 1, redis_to_lua_value(frame)?)?;
            }
            Ok(LuaValue::Table(Rc::new(RefCell::new(table))))
        }
        RespFrame::Array(None) => Ok(LuaValue::Boolean(false)),
        RespFrame::Null => Ok(LuaValue::Boolean(false)),
        _ => Err(LuaError::Runtime("Unsupported Redis type".into())),
    }
}
```

## 4. Security Sandbox

### 4.1 Sandbox Configuration

```rust
pub struct LuaSandbox {
    /// Allowed global functions
    allowed_globals: HashSet<&'static str>,
    
    /// Removed dangerous functions
    blacklist: HashSet<&'static str>,
    
    /// Maximum memory per script
    memory_limit: usize,
    
    /// Maximum instructions per script
    instruction_limit: u64,
    
    /// Deterministic mode
    deterministic: bool,
}

impl LuaSandbox {
    pub fn redis_compatible() -> Self {
        let mut allowed = HashSet::new();
        
        // Safe Lua functions
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
        
        // Math library (deterministic functions only)
        allowed.insert("math.abs");
        allowed.insert("math.ceil");
        allowed.insert("math.floor");
        allowed.insert("math.max");
        allowed.insert("math.min");
        allowed.insert("math.sqrt");
        
        // String library
        allowed.insert("string.byte");
        allowed.insert("string.char");
        allowed.insert("string.find");
        allowed.insert("string.format");
        allowed.insert("string.gsub");
        allowed.insert("string.len");
        allowed.insert("string.lower");
        allowed.insert("string.match");
        allowed.insert("string.rep");
        allowed.insert("string.reverse");
        allowed.insert("string.sub");
        allowed.insert("string.upper");
        
        // Table library
        allowed.insert("table.concat");
        allowed.insert("table.insert");
        allowed.insert("table.remove");
        allowed.insert("table.sort");
        
        let mut blacklist = HashSet::new();
        
        // Dangerous functions
        blacklist.insert("collectgarbage");
        blacklist.insert("dofile");
        blacklist.insert("getfenv");
        blacklist.insert("getmetatable");
        blacklist.insert("load");
        blacklist.insert("loadfile");
        blacklist.insert("loadstring");
        blacklist.insert("module");
        blacklist.insert("print");
        blacklist.insert("rawget");
        blacklist.insert("rawset");
        blacklist.insert("require");
        blacklist.insert("setfenv");
        blacklist.insert("setmetatable");
        
        // Remove all of io, os, debug, package libraries
        blacklist.insert("io");
        blacklist.insert("os");
        blacklist.insert("debug");
        blacklist.insert("package");
        
        // Non-deterministic math functions
        blacklist.insert("math.random");
        blacklist.insert("math.randomseed");
        
        Self {
            allowed_globals: allowed,
            blacklist,
            memory_limit: 64 * 1024 * 1024, // 64MB default
            instruction_limit: 100_000_000,   // 100M instructions
            deterministic: true,
        }
    }
    
    pub fn apply(&self, vm: &mut LuaVm) -> Result<()> {
        // Remove blacklisted globals
        for name in &self.blacklist {
            vm.set_global(name, LuaValue::Nil)?;
        }
        
        // Set resource limits
        vm.set_memory_limit(self.memory_limit);
        vm.set_instruction_limit(self.instruction_limit);
        
        Ok(())
    }
}
```

### 4.2 Resource Limits

```rust
impl LuaVm {
    /// Check instruction count on every Nth instruction
    #[inline(always)]
    pub fn check_limits(&mut self) -> Result<()> {
        self.instruction_count += 1;
        
        // Check every 1000 instructions for efficiency
        if self.instruction_count % 1000 == 0 {
            if self.instruction_count > self.instruction_limit {
                return Err(LuaError::InstructionLimit);
            }
            
            if self.memory_used > self.memory_limit {
                return Err(LuaError::MemoryLimit);
            }
        }
        
        Ok(())
    }
    
    /// Custom allocator that tracks memory
    pub fn alloc(&mut self, size: usize) -> Result<*mut u8> {
        if self.memory_used + size > self.memory_limit {
            return Err(LuaError::MemoryLimit);
        }
        
        self.memory_used += size;
        
        // Use Rust's allocator
        let layout = Layout::from_size_align(size, 8)
            .map_err(|_| LuaError::MemoryLimit)?;
        
        let ptr = unsafe { alloc(layout) };
        if ptr.is_null() {
            self.memory_used -= size;
            return Err(LuaError::MemoryLimit);
        }
        
        Ok(ptr)
    }
}
```

## 5. Script Execution Pipeline

### 5.1 EVAL Command Implementation

```rust
pub struct ScriptExecutor {
    /// Compiled script cache
    cache: Arc<RwLock<LruCache<String, CompiledScript>>>,
    
    /// VM pool for reuse
    vm_pool: Arc<Mutex<Vec<LuaVm>>>,
    
    /// Redis API bridge
    redis_api: Arc<RedisApi>,
    
    /// Security sandbox
    sandbox: LuaSandbox,
}

pub struct CompiledScript {
    /// SHA1 hash of script
    sha: String,
    
    /// Compiled bytecode
    bytecode: Vec<u8>,
    
    /// Source for debugging
    source: String,
}

impl ScriptExecutor {
    pub fn eval(
        &self,
        script: &str,
        keys: Vec<Vec<u8>>,
        args: Vec<Vec<u8>>,
        db: DatabaseIndex,
    ) -> Result<RespFrame> {
        // Compute SHA1 for caching
        let sha = compute_sha1(script);
        
        // Check cache
        let compiled = {
            let cache = self.cache.read().unwrap();
            cache.get(&sha).cloned()
        };
        
        let compiled = match compiled {
            Some(c) => c,
            None => {
                // Compile script
                let compiled = self.compile_script(script)?;
                
                // Cache it
                let mut cache = self.cache.write().unwrap();
                cache.put(sha.clone(), compiled.clone());
                compiled
            }
        };
        
        // Execute script
        self.execute_compiled(compiled, keys, args, db)
    }
    
    fn execute_compiled(
        &self,
        script: CompiledScript,
        keys: Vec<Vec<u8>>,
        args: Vec<Vec<u8>>,
        db: DatabaseIndex,
    ) -> Result<RespFrame> {
        // Get or create VM
        let mut vm = self.get_vm()?;
        
        // Set up execution environment
        self.setup_environment(&mut vm, keys, args, db)?;
        
        // Load and execute script
        vm.load_bytecode(&script.bytecode)?;
        let result = vm.execute()?;
        
        // Convert result
        let resp = lua_to_redis_value(result)?;
        
        // Return VM to pool
        self.return_vm(vm);
        
        Ok(resp)
    }
    
    fn setup_environment(
        &self,
        vm: &mut LuaVm,
        keys: Vec<Vec<u8>>,
        args: Vec<Vec<u8>>,
        db: DatabaseIndex,
    ) -> Result<()> {
        // Create KEYS array
        let keys_table = vm.create_table();
        for (i, key) in keys.into_iter().enumerate() {
            keys_table.set(i + 1, LuaValue::String(LuaString::from_bytes(key)))?;
        }
        vm.set_global("KEYS", keys_table)?;
        
        // Create ARGV array
        let argv_table = vm.create_table();
        for (i, arg) in args.into_iter().enumerate() {
            argv_table.set(i + 1, LuaValue::String(LuaString::from_bytes(arg)))?;
        }
        vm.set_global("ARGV", argv_table)?;
        
        // Register Redis API
        self.redis_api.register(vm)?;
        
        // Apply sandbox
        self.sandbox.apply(vm)?;
        
        Ok(())
    }
}
```

### 5.2 Built-in Libraries

```rust
/// cjson library for JSON encoding/decoding
pub mod cjson {
    pub fn register(vm: &mut LuaVm) -> Result<()> {
        let cjson = vm.create_table();
        
        cjson.set("encode", vm.create_function(json_encode)?)?;
        cjson.set("decode", vm.create_function(json_decode)?)?;
        
        vm.set_global("cjson", cjson)?;
        Ok(())
    }
    
    fn json_encode(vm: &mut LuaVm) -> Result<i32> {
        let val = vm.get(1)?;
        let json = lua_to_json(val)?;
        vm.push(LuaValue::String(LuaString::from_string(json)));
        Ok(1)
    }
}

/// cmsgpack library for MessagePack encoding/decoding
pub mod cmsgpack {
    pub fn register(vm: &mut LuaVm) -> Result<()> {
        let cmsgpack = vm.create_table();
        
        cmsgpack.set("pack", vm.create_function(msgpack_pack)?)?;
        cmsgpack.set("unpack", vm.create_function(msgpack_unpack)?)?;
        
        vm.set_global("cmsgpack", cmsgpack)?;
        Ok(())
    }
}

/// bit operations library
pub mod bit {
    pub fn register(vm: &mut LuaVm) -> Result<()> {
        let bit = vm.create_table();
        
        bit.set("band", vm.create_function(bit_and)?)?;
        bit.set("bor", vm.create_function(bit_or)?)?;
        bit.set("bxor", vm.create_function(bit_xor)?)?;
        bit.set("bnot", vm.create_function(bit_not)?)?;
        bit.set("lshift", vm.create_function(bit_lshift)?)?;
        bit.set("rshift", vm.create_function(bit_rshift)?)?;
        
        vm.set_global("bit", bit)?;
        Ok(())
    }
}
```

## 6. Integration with Ferrous

### 6.1 Command Handlers

```rust
/// EVAL command handler
pub fn handle_eval(
    ctx: &mut CommandContext,
    args: Vec<RespFrame>,
) -> Result<RespFrame> {
    if args.len() < 3 {
        return Err(CommandError::WrongNumberOfArguments);
    }
    
    // Parse arguments
    let script = extract_string(&args[1])?;
    let numkeys = extract_integer(&args[2])?;
    
    if numkeys < 0 || numkeys as usize > args.len() - 3 {
        return Err(CommandError::InvalidArgument);
    }
    
    let keys = args[3..3 + numkeys as usize]
        .iter()
        .map(extract_bytes)
        .collect::<Result<Vec<_>>>()?;
    
    let argv = args[3 + numkeys as usize..]
        .iter()
        .map(extract_bytes)
        .collect::<Result<Vec<_>>>()?;
    
    // Execute script
    ctx.script_executor.eval(&script, keys, argv, ctx.db)
}

/// EVALSHA command handler
pub fn handle_evalsha(
    ctx: &mut CommandContext,
    args: Vec<RespFrame>,
) -> Result<RespFrame> {
    if args.len() < 3 {
        return Err(CommandError::WrongNumberOfArguments);
    }
    
    let sha = extract_string(&args[1])?;
    
    // Lookup script in cache
    let script = ctx.script_executor.get_cached(&sha)
        .ok_or(CommandError::NoScript)?;
    
    // Rest is same as EVAL
    let numkeys = extract_integer(&args[2])?;
    // ... extract keys and args ...
    
    ctx.script_executor.execute_cached(&sha, keys, argv, ctx.db)
}

/// SCRIPT subcommands
pub fn handle_script(
    ctx: &mut CommandContext,
    args: Vec<RespFrame>,
) -> Result<RespFrame> {
    if args.len() < 2 {
        return Err(CommandError::WrongNumberOfArguments);
    }
    
    let subcommand = extract_string(&args[1])?.to_uppercase();
    
    match subcommand.as_str() {
        "LOAD" => {
            if args.len() != 3 {
                return Err(CommandError::WrongNumberOfArguments);
            }
            let script = extract_string(&args[2])?;
            let sha = ctx.script_executor.load_script(&script)?;
            Ok(RespFrame::BulkString(Some(Arc::new(sha.into_bytes()))))
        }
        "EXISTS" => {
            let mut results = Vec::new();
            for arg in &args[2..] {
                let sha = extract_string(arg)?;
                let exists = ctx.script_executor.script_exists(&sha);
                results.push(RespFrame::Integer(if exists { 1 } else { 0 }));
            }
            Ok(RespFrame::Array(Some(results)))
        }
        "FLUSH" => {
            ctx.script_executor.flush_scripts();
            Ok(RespFrame::ok())
        }
        "KILL" => {
            // Kill currently running script if any
            if ctx.script_executor.kill_running_script() {
                Ok(RespFrame::ok())
            } else {
                Err(CommandError::NotBusy)
            }
        }
        _ => Err(CommandError::UnknownSubcommand),
    }
}
```

### 6.2 Thread Safety

```rust
/// Thread-safe script executor
impl ScriptExecutor {
    /// VM pool management for thread efficiency
    fn get_vm(&self) -> Result<LuaVm> {
        // Try to get from pool first
        if let Ok(mut pool) = self.vm_pool.try_lock() {
            if let Some(vm) = pool.pop() {
                return Ok(vm);
            }
        }
        
        // Create new VM if pool is empty
        Ok(LuaVm::new())
    }
    
    fn return_vm(&self, mut vm: LuaVm) {
        // Reset VM state
        vm.reset();
        
        // Return to pool
        if let Ok(mut pool) = self.vm_pool.try_lock() {
            if pool.len() < MAX_VM_POOL_SIZE {
                pool.push(vm);
            }
        }
    }
}

/// Integration with storage engine
impl RedisApi {
    fn execute_command(
        &self,
        cmd: &str,
        args: Vec<RespFrame>,
    ) -> Result<RespFrame> {
        // Commands execute in script's database context
        let engine = Arc::clone(&self.engine);
        
        // Route through command handler with script context
        match cmd.to_uppercase().as_str() {
            "GET" => {
                let key = extract_bytes(&args[0])?;
                match engine.get(self.db, &key)? {
                    GetResult::Found(value) => {
                        Ok(value_to_resp(value))
                    }
                    _ => Ok(RespFrame::Null),
                }
            }
            "SET" => {
                let key = extract_bytes(&args[0])?;
                let value = extract_bytes(&args[1])?;
                engine.set_string(self.db, key, value)?;
                Ok(RespFrame::ok())
            }
            // ... other commands ...
            _ => Err(CommandError::UnknownCommand),
        }
    }
}
```

## 7. Implementation Phases

### Phase 1: Core VM
**Goals**: Basic Lua 5.1 interpreter functionality

**Deliverables**:
- Lexer and parser for Lua 5.1 syntax
- Bytecode compiler
- Basic VM with stack operations
- Core data types (nil, boolean, number, string, table)
- Basic operators and control flow

**Milestones**:
1. Lexer/parser complete, can parse simple scripts
2. Bytecode compiler, can compile expressions
3. VM execution of basic scripts

**Validation**: Run Lua 5.1 test suite subset

### Phase 2: Standard Libraries
**Goals**: Implement Redis-allowed standard library functions

**Deliverables**:
- String library functions
- Table library functions
- Math library (deterministic subset)
- Basic library functions

**Validation**: Library-specific test suites

### Phase 3: Redis Integration
**Goals**: Full Redis API and sandboxing

**Deliverables**:
- redis.call() and redis.pcall()
- Type conversion layer
- Security sandbox implementation
- Resource limits (memory, instructions)

**Validation**: Redis Lua script compatibility tests

### Phase 4: Advanced Features
**Goals**: Production-ready features

**Deliverables**:
- Script caching (EVALSHA support)
- cjson library
- cmsgpack library
- bit operations library
- Performance optimizations

**Validation**: Performance benchmarks vs Redis

### Phase 5: Production Hardening
**Goals**: Production readiness

**Deliverables**:
- Comprehensive error handling
- Memory leak detection and fixes
- Fuzzing and security testing
- Documentation

**Validation**: Extended stress testing

## 8. Testing Strategy

### 8.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_arithmetic() {
        let mut vm = LuaVm::new();
        let result = vm.execute_string("return 2 + 3").unwrap();
        assert_eq!(result, LuaValue::Number(5.0));
    }
    
    #[test]
    fn test_table_operations() {
        let mut vm = LuaVm::new();
        let result = vm.execute_string(r#"
            local t = {a = 1, b = 2}
            return t.a + t.b
        "#).unwrap();
        assert_eq!(result, LuaValue::Number(3.0));
    }
    
    #[test]
    fn test_redis_call() {
        let executor = create_test_executor();
        let result = executor.eval(
            "return redis.call('GET', KEYS[1])",
            vec![b"mykey".to_vec()],
            vec![],
            0,
        ).unwrap();
        // Assert result
    }
}
```

### 8.2 Integration Tests

```rust
#[test]
fn test_redis_script_compatibility() {
    let scripts = vec![
        // Test basic operations
        ("return redis.call('set', KEYS[1], ARGV[1])", 1),
        ("return redis.call('get', KEYS[1])", 1),
        
        // Test complex scripts
        (include_str!("test_scripts/incr_with_limit.lua"), 1),
        (include_str!("test_scripts/conditional_set.lua"), 2),
    ];
    
    for (script, numkeys) in scripts {
        // Test against both Redis and Ferrous
        let redis_result = test_with_redis(script, numkeys);
        let ferrous_result = test_with_ferrous(script, numkeys);
        
        assert_eq!(redis_result, ferrous_result);
    }
}
```

### 8.3 Performance Benchmarks

```rust
fn bench_script_execution() {
    let scripts = vec![
        ("simple", "return ARGV[1]", 100_000),
        ("redis_call", "return redis.call('GET', KEYS[1])", 50_000),
        ("complex", include_str!("bench_complex.lua"), 10_000),
    ];
    
    for (name, script, iterations) in scripts {
        let start = Instant::now();
        
        for _ in 0..iterations {
            executor.eval(script, vec![], vec![], 0).unwrap();
        }
        
        let elapsed = start.elapsed();
        println!("{}: {} ops/sec", name, iterations as f64 / elapsed.as_secs_f64());
    }
}
```

## 9. Security Considerations

### 9.1 Attack Vectors and Mitigations

| Attack Vector | Mitigation |
|---------------|------------|
| Infinite loops | Instruction count limits |
| Memory exhaustion | Memory allocation tracking |
| Stack overflow | Stack depth limits |
| File system access | No io library |
| Network access | No socket operations |
| Code injection | No dynamic code loading |
| Timing attacks | Deterministic execution |

### 9.2 Sandbox Validation

```rust
#[test]
fn test_sandbox_blocks_dangerous_operations() {
    let dangerous_scripts = vec![
        "require('os').execute('rm -rf /')",
        "io.open('/etc/passwd', 'r')",
        "load('malicious code')()",
        "debug.getinfo(1)",
        "while true do end",
    ];
    
    for script in dangerous_scripts {
        let result = executor.eval(script, vec![], vec![], 0);
        assert!(result.is_err() || matches!(result, Ok(RespFrame::Error(_))));
    }
}
```

## 10. Performance Targets

| Metric | Target | Measurement |
|--------|--------|-------------|
| Script compilation | < 1ms for typical scripts | Time from source to bytecode |
| Simple script execution | > 100k ops/sec | `return ARGV[1]` benchmark |
| Redis call overhead | < 10% vs native | Compare scripted vs direct GET/SET |
| Memory per VM | < 1MB baseline | Empty VM memory usage |
| Script cache size | 10k scripts | LRU cache capacity |

## 11. Example Implementation Snippets

### 11.1 Minimal Lexer

```rust
pub struct Lexer<'a> {
    input: &'a str,
    position: usize,
    current_char: Option<char>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Literals
    Number(f64),
    String(String),
    Identifier(String),
    
    // Keywords
    Local,
    Function,
    Return,
    If,
    Then,
    Else,
    End,
    
    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    
    // Delimiters
    LeftParen,
    RightParen,
    Comma,
    
    // End of input
    Eof,
}

impl<'a> Lexer<'a> {
    pub fn next_token(&mut self) -> Result<Token> {
        self.skip_whitespace();
        
        match self.current_char {
            None => Ok(Token::Eof),
            Some(ch) => match ch {
                '+' => { self.advance(); Ok(Token::Plus) }
                '-' => { self.advance(); Ok(Token::Minus) }
                '*' => { self.advance(); Ok(Token::Star) }
                '/' => { self.advance(); Ok(Token::Slash) }
                '(' => { self.advance(); Ok(Token::LeftParen) }
                ')' => { self.advance(); Ok(Token::RightParen) }
                ',' => { self.advance(); Ok(Token::Comma) }
                '"' | '\'' => self.read_string(ch),
                '0'..='9' => self.read_number(),
                'a'..='z' | 'A'..='Z' | '_' => self.read_identifier(),
                _ => Err(LuaError::Syntax(format!("unexpected character: {}", ch))),
            }
        }
    }
}
```

### 11.2 Basic VM Loop

```rust
impl LuaVm {
    pub fn execute(&mut self) -> Result<LuaValue> {
        while let Some(frame) = self.call_stack.last_mut() {
            // Check limits
            self.check_limits()?;
            
            // Fetch instruction
            let inst = frame.closure.proto.code[frame.pc];
            frame.pc += 1;
            
            // Decode and execute
            match inst.opcode() {
                OpCode::LoadK => {
                    let a = inst.a();
                    let bx = inst.bx();
                    let k = &frame.closure.proto.constants[bx as usize];
                    self.set_stack(frame.base + a as usize, k.clone());
                }
                
                OpCode::Add => {
                    let a = inst.a();
                    let b = inst.b();
                    let c = inst.c();
                    
                    let v1 = self.get_rk(frame.base, b)?;
                    let v2 = self.get_rk(frame.base, c)?;
                    
                    match (v1, v2) {
                        (LuaValue::Number(n1), LuaValue::Number(n2)) => {
                            self.set_stack(frame.base + a as usize, LuaValue::Number(n1 + n2));
                        }
                        _ => return Err(LuaError::TypeError),
                    }
                }
                
                OpCode::Return => {
                    let a = inst.a();
                    let b = inst.b();
                    
                    // Collect return values
                    let mut results = Vec::new();
                    if b > 0 {
                        for i in 0..b-1 {
                            results.push(self.get_stack(frame.base + a as usize + i as usize));
                        }
                    }
                    
                    // Pop call frame
                    self.call_stack.pop();
                    
                    if self.call_stack.is_empty() {
                        // Top-level return
                        return Ok(results.first().cloned().unwrap_or(LuaValue::Nil));
                    } else {
                        // Return to caller
                        // ... handle return values ...
                    }
                }
                
                // ... other opcodes ...
                _ => return Err(LuaError::Runtime("unimplemented opcode".into())),
            }
        }
        
        Ok(LuaValue::Nil)
    }
}
```

## 12. Conclusion

This specification provides a comprehensive blueprint for implementing a Lua 5.1 interpreter tailored for Ferrous. The design prioritizes:

1. **Zero dependencies**: Pure Rust implementation using only std
2. **Redis compatibility**: Exact behavioral match with Redis's Lua environment
3. **Security**: Strong sandboxing with resource limits
4. **Performance**: Efficient execution with minimal overhead
5. **Integration**: Seamless fit with Ferrous's architecture

The phased implementation approach allows for incremental development with clear milestones and validation criteria at each stage. The final implementation will provide Ferrous with a powerful, safe, and performant scripting capability that maintains full compatibility with Redis while adhering to Rust's safety guarantees.