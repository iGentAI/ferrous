# Ferrous Design Document

## Overview

Ferrous is a Redis-compatible in-memory database server written in pure Rust with zero external dependencies. This document outlines the architecture, design decisions, and implementation strategy for creating a performant, safe, and fully compatible Redis alternative.

## Design Goals

1. **100% Redis Protocol Compatibility**: Implement RESP2/RESP3 to ensure drop-in compatibility with existing Redis clients
2. **Memory Safety**: Leverage Rust's ownership model to eliminate entire classes of vulnerabilities
3. **True Concurrency**: Unlike Redis's single-threaded architecture, safely utilize multiple CPU cores
4. **Zero Dependencies**: Use only Rust's standard library for maximum portability and security
5. **Performance Parity**: Achieve performance comparable to or better than Redis
6. **Permissive Licensing**: MIT/Apache-2.0 dual license for maximum adoption

## Architecture Overview

### High-Level Architecture

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  Redis Clients  │────▶│ Network Layer   │────▶│ Command Layer   │
└─────────────────┘     └─────────────────┘     └─────────────────┘
                                                          │
                        ┌─────────────────────────────────┘
                        │
                        ▼
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│ Storage Engine  │◀────│ Transaction Mgr │◀────│ Replication Mgr │
└─────────────────┘     └─────────────────┘     └─────────────────┘
```

### Core Components

#### 1. Network Layer
- **TCP Server**: Async I/O using `std::net` with thread pool
- **Connection Manager**: Handles client connections, timeouts, and limits
- **Protocol Parser**: RESP2/RESP3 parser with zero-copy optimizations

#### 2. Command Processing
- **Command Router**: Maps commands to handlers
- **Command Handlers**: Individual handlers for each Redis command
- **Pipeline Support**: Batched command processing

#### 3. Storage Engine
- **Data Structures**:
  - Strings: Simple key-value with TTL support
  - Lists: Doubly-linked lists with O(1) push/pop
  - Sets: Hash sets with efficient membership testing
  - Sorted Sets: Skip lists for range queries
  - Hashes: Nested hash maps
  - Streams: Append-only log with consumer groups
  
- **Memory Management**:
  - Custom allocator wrapper for tracking memory usage
  - Eviction policies (LRU, LFU, Random, TTL)
  - Key expiration with lazy + active deletion

#### 4. Persistence
- **RDB (Snapshots)**: Point-in-time snapshots
- **AOF (Append Only File)**: Command logging with rewrite support
- **Hybrid**: RDB + AOF for faster recovery

#### 5. Replication
- **Master-Slave**: Async replication with backlog
- **Partial Sync**: PSYNC2 protocol support
- **Failover**: Manual and automatic failover

#### 6. Cluster Support (Phase 2)
- **Hash Slots**: 16384 slots with consistent hashing
- **Gossip Protocol**: Node discovery and health checks
- **Resharding**: Online slot migration

## Concurrency Model

Unlike Redis's single-threaded model, Ferrous uses a multi-threaded architecture:

### Thread Architecture

1. **Acceptor Thread**: Accepts new connections
2. **I/O Thread Pool**: Handles network I/O (configurable size)
3. **Worker Thread Pool**: Processes commands (# CPU cores)
4. **Background Threads**:
   - Persistence thread (RDB/AOF)
   - Expiration thread
   - Replication thread

### Synchronization Strategy

```rust
// Each database shard has its own RwLock
struct DatabaseShard {
    data: RwLock<HashMap<String, Value>>,
}

// Sharding by key hash for concurrent access
struct Storage {
    shards: Vec<DatabaseShard>,
}
```

Key principles:
- Shard data structures to reduce lock contention
- Use read-write locks for read-heavy workloads
- Lock-free data structures where possible
- Command-level atomicity preserved

## Memory Layout

### Value Representation

```rust
enum Value {
    String(Vec<u8>),
    List(VecDeque<Vec<u8>>),
    Set(HashSet<Vec<u8>>),
    SortedSet(SkipList<Vec<u8>, f64>),
    Hash(HashMap<Vec<u8>, Vec<u8>>),
    Stream(StreamData),
}

struct RedisObject {
    value: Value,
    expires_at: Option<Instant>,
    lru_clock: AtomicU32,
    ref_count: AtomicU32,
}
```

### Memory Optimization Strategies

1. **Small String Optimization**: Inline storage for small strings
2. **Integer Caching**: Pre-allocated common integer values
3. **Ziplist Encoding**: Compact representation for small lists/hashes
4. **Intset Encoding**: Optimized storage for integer sets

## Command Implementation Priority

### Phase 1: Core Commands (MVP)
- Connection: PING, ECHO, AUTH, QUIT, SELECT
- Strings: GET, SET, MGET, MSET, INCR, DECR, APPEND
- Generic: DEL, EXISTS, EXPIRE, TTL, TYPE, KEYS
- Server: INFO, CONFIG GET/SET, FLUSHDB, FLUSHALL

### Phase 2: Data Structures
- Lists: LPUSH, RPUSH, LPOP, RPOP, LRANGE, LLEN
- Sets: SADD, SREM, SMEMBERS, SISMEMBER, SCARD
- Hashes: HSET, HGET, HDEL, HGETALL, HLEN
- Sorted Sets: ZADD, ZREM, ZRANGE, ZRANK, ZSCORE

### Phase 3: Advanced Features
- Transactions: MULTI, EXEC, DISCARD, WATCH
- Pub/Sub: PUBLISH, SUBSCRIBE, UNSUBSCRIBE
- Persistence: SAVE, BGSAVE, LASTSAVE
- Replication: SLAVEOF, SYNC, PSYNC

### Phase 4: Extended Features
- Streams: XADD, XREAD, XGROUP
- Modules API (limited)
- Cluster support
- Lua scripting

## Error Handling Strategy

```rust
pub enum FerrousError {
    Protocol(String),
    Command(CommandError),
    Storage(StorageError),
    Io(io::Error),
    // ... comprehensive error types
}

// All operations return Result<T, FerrousError>
```

## Performance Targets

Based on our benchmark comparison between Ferrous v0.1.0 (Phase 1-3) and Redis (Valkey), we've established the following performance targets to achieve full parity:

| Command | Valkey Baseline | Ferrous Current | Ferrous Target | Notes |
|---------|-----------------|-----------------|----------------|-------|
| SET | ~73,500 ops/sec | ~42,300 ops/sec | ≥73,500 ops/sec | Single-threaded client |
| GET | ~72,500 ops/sec | ~44,600 ops/sec | ≥72,500 ops/sec | Single-threaded client |
| Pipeline PING | ~650,000 ops/sec | Not fully supported | ≥650,000 ops/sec | Priority improvement area |
| Concurrent (50 clients) | ~73,000 ops/sec | Not fully supported | ≥73,000 ops/sec | Critical for production use |
| Latency (avg) | 0.04-0.05ms | 0.11-0.14ms | ≤0.05ms | Lower is better |

Performance gap analysis:
- Basic operations (SET/GET): Currently at ~60% of Redis performance
- Pipeline operations: Currently not fully supported, critical for high throughput
- Concurrent clients: Currently limited support for high client counts
- Latency: Currently 2-3x higher than Redis

Current phase performance is promising for a debug build with minimal optimization. The next phase should focus on:
1. Fixing pipeline support for high-throughput operations (~10x improvement)
2. Improving concurrent client handling 
3. Optimizing command processing to reduce latency
4. Implementing remaining data structures with performance parity goals

Note: All measurements were taken with debug builds on the same hardware. Release builds are expected to show 30-50% better performance.

## Compatibility Matrix

### Protocol Compatibility
- RESP2: Full support (Phase 1)
- RESP3: Full support (Phase 2)
- Inline commands: Supported

### Client Compatibility Targets
- redis-cli: 100%
- redis-py: 100%
- jedis: 100%
- node-redis: 100%
- go-redis: 100%

### Tool Compatibility
- redis-benchmark: Full support
- redis-check-rdb: Compatible RDB format
- redis-check-aof: Compatible AOF format

## Security Considerations

1. **No Buffer Overflows**: Guaranteed by Rust
2. **Command Injection Prevention**: Safe parsing
3. **AUTH Support**: Password authentication
4. **ACL Support**: User-based access control (Phase 3)
5. **TLS Support**: Using rustls (Phase 3)

## Development Timeline

### Milestone 1: Basic Server (Week 1-2)
- TCP server
- RESP protocol parser
- Basic commands (PING, SET, GET)
- Single-threaded prototype

### Milestone 2: Core Data Structures (Week 3-4)
- All string commands
- Basic list/set/hash commands
- Memory management
- Multi-threading

### Milestone 3: Redis Compatibility (Week 5-6)
- Remaining commands
- Persistence (RDB/AOF)
- Full redis-benchmark compatibility

### Milestone 4: Production Features (Week 7-8)
- Replication
- Transactions
- Performance optimization
- Comprehensive testing

## Testing Strategy

See TEST_PLAN.md for detailed testing approach.

## Benchmarking Methodology

See VALIDATION.md for performance validation approach.