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
└─────────────────────────────────────────────────────────────────┘
```

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

\\ ... existing code ...

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

### 8. Transaction Support

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
| Pipeline PING | ~650,000 ops/sec | Not working | N/A |
| Concurrent (50 clients) | ~73,000 ops/sec | Not working | N/A |
| Latency | ~0.05ms | ~0.16ms | 3x higher |

### Optimization Priority Areas

1. **Pipeline Processing**
```rust
// Current implementation issues:
// 1. Connection closures under high load
// 2. Pipeline command batching not fully implemented

// Priority improvements:
pub fn process_pipeline(&mut self, frames: Vec<RespFrame>) -> Vec<RespFrame> {
    // Process all commands in a batch
    // Maintain connection state throughout
    // Return all responses efficiently
}
```

2. **Connection Management**
```rust
// Connection pooling for better scalability
pub struct ConnectionPool {
    active: Arc<Mutex<HashMap<u64, Connection>>>,
    max_per_thread: usize,
    thread_local: ThreadLocal<Vec<Connection>>,
}

// Event-driven I/O for better concurrency
pub fn handle_connections(&self) -> Result<()> {
    // Use epoll/kqueue for more efficient I/O multiplexing
    // Better support for high connection counts
}
```

3. **Command Processing Optimization**
```rust
// Zero-copy processing where possible
// Memory pooling for allocations
pub struct CommandProcessor {
    memory_pool: MemoryPool,
    thread_allocator: ThreadLocalAllocator,
}

// Command batching
pub fn process_commands(&self, commands: &[Command], responses: &mut Vec<Response>) {
    // Group similar commands
    // Optimize read vs. write operations
    // Minimize lock contention
}
```

4. **Lock Contention Reduction**
```rust
// More granular locking strategy
pub struct StorageShard {
    // More shards for less contention
    lock_striping: Vec<RwLock<HashMap<Range<Key>, Value>>>,
    // Reader-biased locks for read-heavy workloads
}
```

5. **Memory Efficiency**
```rust
// Object pooling
pub struct ObjectPool<T> {
    free_list: Vec<T>,
    // Reuse objects to reduce allocation overhead
}

// Custom allocator optimized for Redis workloads
pub struct FerrousAllocator {
    small_objects: SlabAllocator,  // For strings ≤64 bytes
    medium_objects: BuddyAllocator, // For medium objects
    large_objects: MmapAllocator,   // For very large values
}
```

These optimizations are currently in progress, with a focus on resolving the pipelining and concurrent client handling as the top priorities. Performance on basic operations (SET/GET) is already approaching target levels, currently at ~70% of Redis performance.

Recent improvements:
- Fixed borrowing conflicts in all data structure operations
- Optimized value access patterns to reduce unnecessary clones
- Improved error handling and command execution flow

Our performance targets for Phase 4 completion are to reach at least 90% of Redis performance on all metrics, with full parity expected by the end of Phase 5.

This architecture provides a solid foundation for building a high-performance, Redis-compatible server in Rust while leveraging the language's safety guarantees and concurrency primitives.