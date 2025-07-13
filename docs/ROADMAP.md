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
- ✅ Basic Lua VM with unified stack model
- 🔄 Complete Lua standard library implementation
- 🔄 Advanced Lua VM features
  - ✅ Table operations
  - ✅ Closures and upvalues
  - ✅ Numerical for loops
  - ⚠️ Generic for loops (in progress)
  - ⚠️ Full metamethod support (partially implemented)
  - ❌ Coroutines
  - ❌ Garbage collection
  - ❌ Error handling with traceback
    
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

The Lua VM implementation follows a specific roadmap to ensure compatibility with Redis Lua scripting.

### Phase 1: Core VM (Completed)
- ✅ Unified stack architecture
- ✅ Transaction-based memory safety
- ✅ Basic opcode execution
- ✅ Table creation and manipulation
- ✅ Numerical for loops

### Phase 2: Advanced Features (In Progress)
- ⚠️ Generic for loops
- ⚠️ Metamethod handling
- ❌ Coroutines
- ❌ Garbage collection
- ❌ Comprehensive error handling

### Phase 3: Redis Integration (Planned)
- ⚠️ EVAL/EVALSHA commands
- ⚠️ Standard library completion
- ❌ Script caching
- ❌ SCRIPT LOAD/FLUSH commands
- ❌ Script timeout management

### Phase 4: Performance Optimization (Future)
- ❌ Bytecode optimization
- ❌ Memory usage reduction
- ❌ Benchmarking against Lua 5.1