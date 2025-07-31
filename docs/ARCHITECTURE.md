# Ferrous Technical Architecture

## Overview

This document provides a detailed technical architecture of Ferrous, describing the internal design, component interactions, and implementation details of our Redis-compatible server.

## System Architecture

### Component Hierarchy

```
┌─────────────────────────────────────────────────────────────────┐
│                         Ferrous Server                          │
├─────────────────────────────────────────────────────────────────┤
│                      Network Layer (async)                      │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐   │
│  │  Acceptor   │  │ Connection  │  │  Protocol Parser    │   │
│  │   Thread    │  │   Pool      │  │  (RESP2/3)         │   │
│  └─────────────┘  └─────────────┘  └─────────────────────┘   │
├─────────────────────────────────────────────────────────────────┤
│                    Command Processing Layer                     │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐   │
│  │   Command   │  │  Command    │  │   Authorization     │   │
│  │   Router    │  │  Handlers   │  │   & Validation      │   │
│  └─────────────┘  └─────────────┘  └─────────────────────┘   │
├─────────────────────────────────────────────────────────────────┤
│                      Storage Layer                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐   │
│  │  Database   │  │   Memory    │  │    Persistence      │   │
│  │   Engine    │  │  Manager    │  │   (RDB/AOF)         │   │
│  └─────────────┘  └─────────────┘  └─────────────────────┘   │
├─────────────────────────────────────────────────────────────────┤
│                    System Services                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐   │
│  │ Replication │  │  Pub/Sub    │  │   Background        │   │
│  │   Manager   │  │  Engine     │  │    Tasks            │   │
│  └─────────────┘  └─────────────┘  └─────────────────────┘   │
├─────────────────────────────────────────────────────────────────┤
│                    Monitoring Layer (New)                       │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐   │
│  │  SLOWLOG    │  │  MONITOR    │  │   Memory Tracking   │   │
│  │   System    │  │  Broadcast  │  │   & CLIENT Cmds     │   │
│  └─────────────┘  └─────────────┘  └─────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## Monitoring Layer (New)

### SLOWLOG System

The SLOWLOG system tracks command execution times and provides insights into slow operations:

```rust
pub struct Slowlog {
    /// Entries stored in order (newest first)
    entries: Arc<Mutex<VecDeque<SlowlogEntry>>>,
    
    /// Maximum number of entries to keep
    max_len: AtomicU64,
    
    /// Threshold in microseconds
    threshold_micros: AtomicI64,
    
    /// ID generator for entries
    next_id: AtomicU64,
}
```

Key components:
- Circular buffer of slow command entries
- Configurable threshold (microseconds)
- Configurable maximum length
- Thread-safe implementation with atomic counters

### MONITOR Implementation

The MONITOR subsystem supports real-time command broadcasting to monitoring clients:

```rust
pub struct MonitorSubscribers {
    /// Set of connection IDs that are monitoring
    subscribers: Arc<Mutex<HashSet<u64>>>,
}
```

Key components:
- Connection ID tracking for subscribers
- Thread-safe broadcasting to all monitors
- Properly formatted output with timestamps and connection details
- Security filtering (AUTH commands not broadcast)

### Client Management

The client management system provides comprehensive client connection control:

```rust
pub trait ConnectionProvider {
    /// Execute a function on a specific connection
    fn with_connection<F, R>(&self, id: u64, f: F) -> Option<R>
    where
        F: FnOnce(&mut Connection) -> R;
    
    /// Get all connection IDs
    fn all_connection_ids(&self) -> Vec<u64>;
    
    /// Close a connection by ID
    fn close_connection(&self, id: u64) -> bool;
}
```

Key components:
- Client connection listing
- Connection termination
- Client pause functionality
- Connection naming
- Sharded connection management for concurrency

### Memory Tracking

The memory tracking system provides detailed insights into memory usage:

```rust
pub struct MemoryStats {
    /// Total memory usage
    pub total_used: AtomicUsize,
    
    /// Peak memory usage
    pub peak_used: AtomicUsize,
    
    /// Memory used by keys (key names)
    pub keys_size: AtomicUsize,
    
    /// Memory categorized by data structure type
    pub strings_size: AtomicUsize,
    pub lists_size: AtomicUsize,
    pub sets_size: AtomicUsize,
    pub hashes_size: AtomicUsize,
    pub zsets_size: AtomicUsize,
    
    /// Per-database memory usage
    pub db_memory: Arc<RwLock<HashMap<usize, usize>>>,
}
```

Key components:
- Per-key memory usage calculation
- Memory usage categorization by data structure
- System-wide memory statistics
- Memory usage analysis and recommendations
- Enhanced INFO command with memory details

## Core Components

### 1. Network Layer

#### Connection Management

```rust
pub struct NetworkLayer {
    acceptor: TcpListener,
    connections: Arc<Mutex<HashMap<u64, Connection>>>,
    thread_pool: ThreadPool,
    config: NetworkConfig,
}

pub struct Connection {
    id: u64,
    stream: TcpStream,
    addr: SocketAddr,
    state: ConnectionState,
    buffer: ByteBuffer,
    last_activity: Instant,
}

pub enum ConnectionState {
    Connected,
    Authenticated,
    Blocked(BlockedOn),
    Closing,
}
```

#### Async I/O Design
- **Edge-triggered epoll** on Linux
- **kqueue** on macOS/BSD  
- **IOCP** on Windows (future)

#### Buffer Management

```rust
pub struct ByteBuffer {
    read_buf: Vec<u8>,
    write_buf: Vec<u8>,
    read_pos: usize,
    write_pos: usize,
}

impl ByteBuffer {
    pub fn read_resp(&mut self) -> Result<Option<RespFrame>> {
        // Zero-copy RESP parsing
    }
    
    pub fn write_resp(&mut self, frame: &RespFrame) -> Result<()> {
        // Efficient serialization
    }
}
```

### 2. Protocol Layer

#### RESP Parser Architecture

```rust
pub enum RespFrame {
    SimpleString(Bytes),
    Error(Bytes),
    Integer(i64),
    BulkString(Option<Bytes>),
    Array(Option<Vec<RespFrame>>),
    // RESP3 additions
    Null,
    Boolean(bool),
    Double(f64),
    Map(Vec<(RespFrame, RespFrame)>),
    Set(Vec<RespFrame>),
}

pub struct RespParser {
    state: ParserState,
    stack: Vec<PartialFrame>,
}

enum ParserState {
    FrameType,
    SimpleString { buf: Vec<u8> },
    Integer { buf: Vec<u8>, negative: bool },
    BulkString { len: Option<usize>, buf: Vec<u8> },
    Array { len: usize, frames: Vec<RespFrame> },
}
```

#### Zero-Copy Optimization

```rust
// Use Bytes for zero-copy string handling
type Bytes = Arc<Vec<u8>>;

impl RespFrame {
    pub fn parse_zero_copy(buf: &mut ByteBuffer) -> Result<Option<Self>> {
        // Parse without copying data when possible
    }
}
```

### 3. Command Processing

#### Command Router Design

```rust
pub struct CommandRouter {
    commands: HashMap<&'static str, CommandHandler>,
}

type CommandHandler = Box<dyn Fn(&mut Context, Vec<RespFrame>) -> Result<RespFrame>>;

pub struct Context<'a> {
    connection: &'a mut Connection,
    database: &'a mut Database,
    config: &'a Config,
}

impl CommandRouter {
    pub fn route(&self, ctx: &mut Context, cmd: Command) -> Result<RespFrame> {
        let handler = self.commands.get(cmd.name.to_uppercase().as_str())
            .ok_or(Error::UnknownCommand)?;
        
        handler(ctx, cmd.args)
    }
}
```

#### Command Categories

```rust
mod commands {
    pub mod connection;  // PING, AUTH, SELECT, QUIT
    pub mod string;      // GET, SET, INCR, etc.
    pub mod list;        // LPUSH, LPOP, LRANGE, etc.
    pub mod set;         // SADD, SREM, SMEMBERS, etc.
    pub mod hash;        // HGET, HSET, HDEL, etc.
    pub mod sorted_set;  // ZADD, ZRANGE, etc.
    pub mod generic;     // DEL, EXISTS, EXPIRE, etc.
    pub mod pubsub;      // PUBLISH, SUBSCRIBE, etc.
    pub mod transaction; // MULTI, EXEC, WATCH, etc.
}
```

### 4. Storage Engine

#### Multi-Threaded Architecture

```rust
pub struct StorageEngine {
    databases: Vec<Database>,
    allocator: MemoryAllocator,
    stats: Arc<Statistics>,
}

pub struct Database {
    shards: Vec<Shard>,
    expires: Arc<RwLock<ExpiryIndex>>,
}

pub struct Shard {
    data: RwLock<HashMap<Key, Value>>,
    lock_stripe: LockStripe,
}
```

#### Sharding Strategy

```rust
const NUM_SHARDS: usize = 16; // Tunable

impl Database {
    pub fn get(&self, key: &[u8]) -> Option<Value> {
        let shard_idx = hash(key) % NUM_SHARDS;
        let shard = &self.shards[shard_idx];
        
        let guard = shard.data.read();
        guard.get(key).cloned()
    }
}
```

#### Value Storage

```rust
pub struct Value {
    data: ValueData,
    metadata: Metadata,
}

pub enum ValueData {
    String(Bytes),
    List(Arc<RwLock<VecDeque<Bytes>>>),
    Set(Arc<RwLock<HashSet<Bytes>>>),
    Hash(Arc<RwLock<HashMap<Bytes, Bytes>>>),
    SortedSet(Arc<RwLock<SkipList<Bytes, f64>>>),
    Stream(Arc<RwLock<Stream>>),
}

pub struct Metadata {
    created_at: Instant,
    accessed_at: AtomicU64,
    expires_at: Option<Instant>,
    encoding: Encoding,
}
```

### 5. Memory Management

#### Custom Allocator

```rust
pub struct MemoryManager {
    used_memory: AtomicUsize,
    max_memory: usize,
    eviction_policy: EvictionPolicy,
}

pub enum EvictionPolicy {
    NoEviction,
    AllKeysLRU,
    VolatileLRU,
    AllKeysRandom,
    VolatileRandom,
    VolatileTTL,
}

impl MemoryManager {
    pub fn allocate(&self, size: usize) -> Result<*mut u8> {
        // Check memory limits
        // Update statistics
        // Trigger eviction if needed
    }
}
```

#### LRU Implementation

```rust
pub struct LRUCache {
    map: HashMap<Key, Node>,
    head: *mut Node,
    tail: *mut Node,
}

struct Node {
    key: Key,
    value: Value,
    prev: *mut Node,
    next: *mut Node,
    frequency: u32,
}
```

### 6. Persistence

#### RDB Engine

```rust
pub struct RdbEngine {
    version: u32,
    checksum: bool,
}

impl RdbEngine {
    pub fn save(&self, db: &Database, path: &Path) -> Result<()> {
        let mut encoder = RdbEncoder::new(path)?;
        
        encoder.write_header()?;
        encoder.write_metadata()?;
        
        for (key, value) in db.iter() {
            encoder.write_key_value(key, value)?;
        }
        
        encoder.write_checksum()?;
        encoder.finish()
    }
}
```

#### AOF Engine

```rust
pub struct AofEngine {
    file: Arc<Mutex<File>>,
    buffer: Arc<Mutex<Vec<u8>>>,
    fsync_policy: FsyncPolicy,
}

pub enum FsyncPolicy {
    Always,    // fsync after every command
    EverySecond, // fsync every second
    No,        // let OS handle fsync
}

impl AofEngine {
    pub fn append_command(&self, cmd: &Command) -> Result<()> {
        let mut buf = self.buffer.lock();
        cmd.write_resp(&mut buf)?;
        
        if self.should_fsync() {
            self.flush_and_sync()?;
        }
        Ok(())
    }
}
```

## System Services

### 4. Replication Manager
```rust
pub struct ReplicationManager {
    /// Current role (master or replica)
    role: Arc<RwLock<ReplicationRole>>,
    
    /// Replication configuration
    config: ReplicationConfig,
    
    /// Connected replicas (for master)
    replicas: Arc<Mutex<HashMap<u64, Arc<ReplicaInfo>>>>,
    
    /// Replication backlog (for master)
    backlog: Arc<ReplicationBacklog>,
    
    /// Current replication state
    state: Arc<Mutex<ReplicationState>>,
    
    /// Flag to pause replication
    paused: AtomicBool,
    
    /// Master connection ID (for replica)
    master_conn_id: Arc<Mutex<Option<u64>>>,
    
    /// Handle to stop background replication
    replication_stop_flag: Arc<Mutex<Option<Arc<AtomicBool>>>>,
}

pub enum ReplicationRole {
    Master {
        repl_id: String,
        repl_offset: Arc<AtomicU64>,
        // ... additional fields
    },
    Replica {
        master_addr: SocketAddr,
        master_link_status: MasterLinkStatus,
        master_repl_id: String,
        repl_offset: u64,
    },
}

pub enum MasterLinkStatus {
    Connecting,
    Synchronizing,
    Up,
    Down,
}
```

## Critical Architectural Constraint: Lua VM Integration

### **The Dual-Mutability Constraint**

**Status**: ❌ **Blocking Issue** - Fundamental architectural limitation preventing completion

The current Lua VM implementation has encountered a **fundamental architectural constraint** that prevents completion of Lua 5.1 specification compliance. This constraint is not a bug but a **design incompatibility** with Rust's ownership model.

#### **The Problem: ExecutionContext Anti-Pattern**

The current design uses an ExecutionContext trait that requires:

```rust
trait ExecutionContext {
    fn pcall(&mut self, func: Value, args: Vec<Value>) -> LuaResult<()>
    fn table_next(&self, table: &TableHandle, key: &Value) -> LuaResult<Option<(Value, Value)>>
    // ... other methods requiring VM state modification
}

// Implementation attempts to mutably borrow VM during C function execution
struct VmExecutionContext<'a> {
    vm: &'a mut RcVM,  // ❌ Causes E0596: cannot borrow as mutable
    // ...
}
```

**This creates a dual-mutability deadlock:**
1. **VM execution** holds mutable borrows during opcode processing
2. **Standard library functions** need mutable VM access for `pcall`, metamethods, etc.
3. **Rust's borrow checker prevents** simultaneous mutable access

#### **Compilation Error Evidence**

```
error[E0596]: cannot borrow `*self.vm` as mutable, as it is behind a `&` reference
    --> src/lua/rc_vm.rs:2404:27
     |
2404 |         let call_result = self.vm.execute_function_call(
     |                           ^^^^^^^ cannot borrow as mutable
```

This error represents **Rust's ownership system rejecting our architectural design**, not a fixable implementation bug.

#### **Impact on System Integration**

The constraint manifests as:
- ✅ **Individual components work perfectly** (17/27 tests passing)
- ❌ **Integration scenarios fail** where VM-stdlib interaction is required
- ❌ **Cannot complete basic standard library functions** (`pcall`, `xpcall`, etc.)
- ❌ **Complex metamethod scenarios blocked**

### **Architectural Solutions from Successful Rust Lua Interpreters**

Research into production Rust Lua interpreters reveals proven patterns that solve this constraint:

#### **Solution 1: Piccolo's Sequence Pattern (Recommended)**

**Architecture**: VM-mediated operations eliminate dual-mutability

```rust
// Standard library functions return operation descriptions
enum VMRequest {
    CallFunction(Value, Vec<Value>),
    GetTableField(TableHandle, Value),
    SetTableField(TableHandle, Value, Value),
}

trait ExecutionContext {
    fn request_operation(&mut self, req: VMRequest) -> RequestHandle;
    fn get_result(&self, handle: RequestHandle) -> LuaResult<Value>;
}

// VM processes requests with exclusive mutable control
impl RcVM {
    fn process_request(&mut self, request: VMRequest) -> LuaResult<Value> {
        // VM has exclusive mutable access, no borrowing conflicts
    }
}
```

**Benefits:**
- ✅ **Eliminates dual-mutability**: No simultaneous mutable borrows
- ✅ **VM maintains control**: Exclusive mutable access throughout
- ✅ **Proven production use**: Piccolo uses this pattern successfully
- ✅ **Rust-friendly**: Works with ownership model, not against it

#### **Solution 2: mlua's Proxy Pattern**

**Architecture**: Controlled access through proxy objects

```rust
pub struct Lua {
    raw: XRc<ReentrantMutex<RawLua>>, // Thread-safe, runtime borrow checking
}

// Proxy objects provide controlled access
impl Lua {
    pub fn create_proxy(&self) -> LuaProxy {
        // Provides controlled access without direct borrowing conflicts
    }
}
```

**Benefits:**
- ✅ **Runtime borrow checking**: ReentrantMutex enables runtime checking
- ✅ **Production proven**: mlua is widely used in Rust ecosystem
- ✅ **Thread-safe design**: Suitable for concurrent environments

#### **Solution 3: Callback-Based Pattern**

**Architecture**: VM provides callbacks to standard library functions

```rust
struct VMCallbacks<'a> {
    pcall: Box<dyn Fn(Value, Vec<Value>) -> LuaResult<Vec<Value>> + 'a>,
    table_next: Box<dyn Fn(&TableHandle, &Value) -> LuaResult<Option<(Value, Value)>> + 'a>,
}

trait ExecutionContext {
    fn with_vm_callbacks<R>(&mut self, f: impl FnOnce(VMCallbacks) -> R) -> R;
}
```

**Benefits:**
- ✅ **Clear ownership boundaries**: Callbacks owned by VM
- ✅ **Flexible integration**: Easy to add new VM operations
- ✅ **Type safety**: Compile-time verification of callback signatures

### **Migration Strategy Recommendations**

#### **Phase 1: Proof of Concept (1-2 weeks)**
1. **Implement basic VMRequest/Response mechanism** for one operation (`pcall`)
2. **Verify compilation succeeds** without E0596 errors
3. **Test basic integration** with existing VM execution

#### **Phase 2: Core Migration (2-3 weeks)**  
1. **Replace ExecutionContext trait** with chosen pattern
2. **Migrate essential standard library functions** to new architecture
3. **Maintain existing component functionality**

#### **Phase 3: Full Integration (1-2 weeks)**
1. **Complete standard library implementation** using new pattern
2. **Achieve full Lua 5.1 specification compliance**
3. **Performance optimization and testing**

### **Why the Current Architecture Cannot Be Fixed**

**The dual-mutability constraint is not a bug—it's a fundamental design incompatibility:**

1. **Rust's ownership model** prevents the pattern by design for memory safety
2. **Workarounds fail**: Attempts to use `Cell`, `RefCell`, or other interior mutability patterns still hit borrowing conflicts during VM execution
3. **Successful implementations prove** that proper architectures work within Rust's constraints
4. **The pattern is an anti-pattern**: Other systems languages (C/C++) allow this dangerous pattern, but Rust correctly prevents it

### **Development Guidance**

#### **For Contributors**

⚠️ **Critical Understanding**: This is **not a traditional debugging problem**. The remaining issues are symptoms of architectural incompatibility, not implementation bugs.

**High-impact contributions:**
- ✅ Implementing proven architectural patterns (Sequence, Proxy, Callback)
- ✅ Architectural expertise and pattern migration
- ✅ Integration testing with new patterns

**Low-impact contributions:**
- ❌ Bug fixes for failing tests (symptoms of architectural issue)
- ❌ Standard library incremental improvements (blocked by architecture)
- ❌ Performance optimizations (requires stable architecture first)

#### **For Users**

**Current capabilities:** The interpreter is **highly functional** for:
- Basic Lua scripts and function execution
- Simple Redis Lua integration scenarios
- Table operations and control flow
- Standard mathematical and string operations

**Limitations:** Complex scenarios requiring VM-stdlib integration will fail until architectural migration is complete.

This constraint represents a **learning opportunity** about language interpreter design in Rust and demonstrates the importance of **architecture-first design** when working within Rust's ownership constraints.

### 5. Lua VM Architecture (**UPDATED - Direct Execution Model**)

The Lua VM has been completely refactored to use a unified Frame-based execution model:

```rust
pub struct RcVM {
    /// The Lua heap with fine-grained Rc<RefCell> objects
    pub heap: RcHeap,
    
    /// Current thread (NO operation queue!)
    current_thread: ThreadHandle,
    
    /// VM configuration
    config: VMConfig,
}

// Direct execution loop - no queue processing!
fn run_to_completion(&mut self) -> LuaResult<Value> {
    loop {
        match self.step() {
            Ok(StepResult::Continue) => continue,
            Ok(StepResult::Completed(values)) => return Ok(values.first().unwrap_or(Value::Nil)),
            Err(e) => return self.handle_error(e),
        }
    }
}
```

Key architectural improvements:
- **Eliminated Queue Infrastructure**: Removed all PendingOperation and temporal state separation
- **Direct Metamethod Execution**: Immediate metamethod calls without queue delays
- **Unified Frame Model**: Frame enum supports both calls and continuations
- **Improved Reliability**: Test pass rate improved from 55.6% to 59.3%
- **Simplified Architecture**: ~500 lines of queue complexity eliminated

### 6. Transaction Support

```rust
pub struct Transaction {
    commands: Vec<Command>,
    watched_keys: HashSet<Key>,
    state: TxState,
}

pub enum TxState {
    Open,
    Queuing,
    Aborted(String),
}

impl Connection {
    pub fn exec_transaction(&mut self) -> Result<Vec<RespFrame>> {
        // Check watched keys
        // Execute commands atomically
        // Return results
    }
}
```

### Replication Architecture

```
┌─────────────────────────┐      ┌─────────────────────────┐
│     Ferrous Master      │      │     Ferrous Replica     │
│                         │      │                         │
│  ┌─────────────────┐    │      │  ┌─────────────────┐    │
│  │ ReplicationMgr  │    │      │  │ ReplicationMgr  │    │
│  │ (Master Role)   │◄───┼──────┼──┤ (Replica Role)  │    │
│  └─────────────────┘    │      │  └─────────┬───────┘    │
│          ▲              │      │            │            │
│          │              │      │            ▼            │
│  ┌───────┴─────────┐    │      │  ┌─────────────────┐    │
│  │ Command Router  │    │      │  │ReplicationClient│    │
│  └───────┬─────────┘    │      │  └─────────┬───────┘    │
│          │              │      │            │            │
│          ▼              │      │            ▼            │
│  ┌─────────────────┐    │      │  ┌─────────────────┐    │
│  │Storage Engine   │    │      │  │Storage Engine   │    │
│  └─────────────────┘    │      │  └─────────────────┘    │
└─────────────────────────┘      └─────────────────────────┘
        REPLICATION PROTOCOL
        1. AUTH (Authentication)
        2. PING/PONG (Connection check)
        3. REPLCONF (Options negotiation)
        4. PSYNC (Synchronization)
        5. RDB Transfer (Initial data sync)
        6. Command Propagation (Ongoing sync)
```

#### Replication Process Flow

1. **Initialization**:
   - Replica connects to master using `ReplicationClient`
   - Authentication with master using configured password
   - PING/PONG exchange to verify connection

2. **Handshake and Capabilities**:
   - REPLCONF exchange to negotiate capabilities and parameters
   - Replica provides listening port and supported features
   - Master acknowledges capabilities

3. **Synchronization**:
   - PSYNC command sent from replica to master
   - Master determines synchronization mode:
     - FULLRESYNC for new replicas
     - CONTINUE for incremental updates (future enhancement)
   - Master sends replication ID and offset 

4. **RDB Transfer**:
   - Master generates RDB snapshot of current dataset
   - RDB data sent to replica as bulk string
   - Replica reads RDB data and populates its storage engine

5. **Command Propagation**:
   - Master propagates all write commands to connected replicas
   - Commands are sent in RESP format
   - Replica applies commands locally to maintain consistency

6. **Continuous Replication**:
   - Replica sends periodic ACKs to master
   - Replica updates status based on connection health
   - Replica maintains current offset for tracking

### 7. Legacy Replication Reference

## Threading Model

### Thread Types

1. **Main Thread**: Server initialization and coordination
2. **Acceptor Thread**: Accepts new connections
3. **I/O Worker Pool**: Handles network I/O (N threads)
4. **Command Worker Pool**: Processes commands (M threads)
5. **Background Thread Pool**: Persistence, expiry, etc.

### Thread Communication

```rust
pub struct ThreadComm {
    // Command queue for I/O -> Worker communication
    command_queue: Arc<Mutex<VecDeque<WorkItem>>>,
    
    // Response queue for Worker -> I/O communication  
    response_queue: Arc<Mutex<VecDeque<Response>>>,
    
    // Condition variables for wake-ups
    work_available: Arc<Condvar>,
    response_available: Arc<Condvar>,
}

pub struct WorkItem {
    connection_id: u64,
    command: Command,
    timestamp: Instant,
}
```

## Performance Optimizations

### 1. Lock-Free Data Structures

```rust
// Lock-free increment for INCR command
pub struct AtomicString {
    value: AtomicPtr<StringData>,
}

impl AtomicString {
    pub fn increment(&self) -> Result<i64> {
        loop {
            let current = self.value.load(Ordering::Acquire);
            // Parse integer, increment, create new
            // CAS loop until success
        }
    }
}
```

### 2. Memory Pool

```rust
pub struct MemoryPool {
    small_objects: Vec<Vec<u8>>, // 64B chunks
    medium_objects: Vec<Vec<u8>>, // 512B chunks  
    large_objects: Vec<Vec<u8>>, // 4KB chunks
}
```

### 3. Zero-Copy Networking

```rust
// Use splice/sendfile for large values
pub fn send_bulk_string(&mut self, data: &[u8]) -> Result<()> {
    if data.len() > ZERO_COPY_THRESHOLD {
        self.splice_from_memory(data)?;
    } else {
        self.write_to_buffer(data)?;
    }
    Ok(())
}
```

## Configuration System

```rust
pub struct Config {
    // Network
    bind: Vec<SocketAddr>,
    port: u16,
    tcp_backlog: i32,
    tcp_keepalive: Duration,
    
    // Limits
    max_clients: usize,
    max_memory: Option<usize>,
    
    // Persistence
    save_rules: Vec<SaveRule>,
    aof_enabled: bool,
    aof_fsync: FsyncPolicy,
    
    // Performance
    io_threads: usize,
    worker_threads: usize,
}
```

## Error Handling

```rust
#[derive(Debug)]
pub enum Error {
    // Network errors
    ConnectionClosed,
    ConnectionTimeout,
    
    // Protocol errors
    ProtocolError(String),
    InvalidCommand(String),
    
    // Storage errors  
    OutOfMemory,
    KeyNotFound,
    WrongType,
    
    // System errors
    IoError(io::Error),
    InternalError(String),
}

pub type Result<T> = std::result::Result<T, Error>;
```

## Monitoring and Metrics

```rust
pub struct Metrics {
    // Counters
    total_connections: AtomicU64,
    total_commands: AtomicU64,
    
    // Gauges
    used_memory: AtomicU64,
    connected_clients: AtomicU64,
    
    // Histograms
    command_latency: Histogram,
    
    // Per-command stats
    command_stats: RwLock<HashMap<String, CommandStats>>,
}
```

## Performance Optimization

Based on benchmark comparisons with Redis (Valkey), here's our current performance status and optimization priorities:

### Current Benchmark Results

| Operation | Redis Performance | Ferrous Performance | Ratio | 
|-----------|-------------------|---------------------|-------|
| SET | ~73,500 ops/sec | ~49,750 ops/sec | 68% |
| GET | ~72,500 ops/sec | ~55,250 ops/sec | 76% |
| Pipeline PING | ~650,000 ops/sec | Working with direct execution | Improving |
| Concurrent (50 clients) | ~73,000 ops/sec | Supported with Frame architecture | Improving |
| Latency | ~0.05ms | ~0.16ms | 3x higher |

### Optimization Priority Areas

**1. Direct Execution Benefits**
The elimination of queue infrastructure has already provided:
- Reduced latency from temporal state separation elimination
- Improved throughput from immediate operation processing  
- Better reliability from direct metamethod execution
- Simplified debugging and profiling

**2. Further Optimization Opportunities**
```rust
// Direct execution optimizations
pub fn process_commands(&self, instructions: &[Instruction]) -> LuaResult<Vec<Value>> {
    // Process instructions immediately without queue overhead
    // Leverage direct metamethod execution for better performance
    // Use unified Frame architecture for efficient call handling
}
```

Recent improvements from architecture refactor:
- **Fixed temporal state separation issues** - eliminated register overflow errors  
- **Improved metamethod performance** - direct execution vs queue processing
- **Enhanced call handling** - unified Frame architecture reduces overhead
- **Better error handling** - immediate error processing and propagation

Our performance targets focus on leveraging the direct execution model benefits, with full parity expected as the architecture matures. The elimination of queue overhead provides a strong foundation for continued performance improvements.

This architecture provides a solid foundation for building a high-performance, Redis-compatible server in Rust while leveraging the language's safety guarantees and the newly optimized direct execution model.