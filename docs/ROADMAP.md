# Ferrous Development Roadmap

## Phase 1: Core Functionality (Completed)
- TCP server with connection handling
- Full RESP2 protocol implementation
- Basic Redis commands (PING, GET, SET, DEL, etc.)
- String data type implementation

## Phase 2: Data Structures (Completed)
- List data type and commands
- Set data type and commands
- Hash data type and commands
- Sorted Set data type and commands
- Basic key operations (EXISTS, EXPIRE, TTL, etc.)

## Phase 3: Persistence & Messaging (Completed)
- RDB persistence (SAVE, BGSAVE)
- AOF persistence
- Pub/Sub messaging
- Transaction support (MULTI/EXEC/DISCARD/WATCH)

## Phase 4: Advanced Features (In Progress)
- ✅ Pipelined command processing
- ✅ Concurrent client handling
- ✅ Configuration commands
- ✅ Enhanced RESP parsing
- ✅ Master-slave replication
- ✅ SCAN command family for safe iteration
- 🔄 RefCellVM Lua implementation
  - ✅ Basic language features (variables, arithmetic, strings)
  - ✅ Numerical for loops
  - ⚠️ Basic table operations (partially implemented)
  - ❌ Functions and closures (not implemented)
  - ❌ Generic for loops (not implemented)
  - ❌ Metamethods (not implemented)
- 🔄 Lua Standard Library
  - ⚠️ Basic functions (print, type, tostring implemented)
  - ❌ Table library (not implemented)
  - ❌ String library (not implemented)
  - ❌ Math library (not implemented)
    
## Phase 5: Performance & Monitoring (Planned)
- ⚠️ Production monitoring (INFO)
- ✅ Performance benchmarking
- ⚠️ SLOWLOG implementation (in progress)
- ❌ Memory usage optimization
- ❌ CLIENT command family
- ❌ Latency monitoring tools

## Phase 6: Clustering & Enterprise Features (Future)
- ❌ Redis cluster protocol support
- ❌ Slot-based sharding
- ❌ Cluster state management
- ❌ Redis Streams implementation
- ❌ ACL system
- ❌ TLS support

## Legend
- ✅ Completed
- 🔄 In active development
- ⚠️ Partially implemented
- ❌ Not yet implemented

## Lua VM Development Roadmap

The Lua VM implementation roadmap reflects the current RefCellVM architecture.

### Phase 1: Core VM Features (In Progress)
- ✅ RefCellVM architecture with interior mutability
- ✅ Arena-based memory management with handle validation
- ✅ Basic opcode execution
- ✅ Numerical for loops
- ✅ String interning with content-based comparison
- ✅ Basic table operations
- ✅ Basic standard library functions (print, type, tostring)

### Phase 2: Function Implementation (Planned)
- ❌ Function definitions and calls
- ❌ Closures
- ❌ Upvalue handling
- ❌ Variable arguments
- ❌ Tail call optimization 

### Phase 3: Advanced Language Features (Future)
- ❌ Generic for loops (pairs/ipairs)
- ❌ Metamethod handling
- ❌ Complete standard library
  - ❌ Table library
  - ❌ String library
  - ❌ Math library
- ❌ Coroutines
- ❌ Error handling with traceback

### Phase 4: Redis Integration & Optimization (Future)
- ❌ EVAL/EVALSHA commands
- ❌ Script caching
- ❌ SCRIPT LOAD/FLUSH commands
- ❌ Script timeout management
- ❌ redis.call and redis.pcall functions
- ❌ Memory usage optimization
- ❌ Benchmarking against Lua 5.1