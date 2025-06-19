# ferrous
A Redis-compatible in-memory database server written in pure Rust with zero external dependencies

## Project Status

Ferrous is currently at Phase 3 implementation, with several Phase 4 features completed:

### Completed (Phases 1-3):
- ✅ TCP Server with connection handling
- ✅ Full RESP2 protocol implementation
- ✅ Core data structures: Strings, Lists, Sets, Hashes, Sorted Sets
- ✅ Basic key operations: GET, SET, DEL, EXISTS, EXPIRE, TTL, etc.
- ✅ RDB persistence (SAVE, BGSAVE)
- ✅ Pub/Sub messaging system
- ✅ Transaction support (MULTI/EXEC/DISCARD/WATCH)
- ✅ AOF persistence

### Phase 4 Features Completed:
- ✅ Pipelined command processing
- ✅ Concurrent client handling (50+ connections)
- ✅ Configuration commands (CONFIG GET)
- ✅ Enhanced RESP protocol parsing

### Coming Soon (Remaining Phase 4-6):
- Master-slave replication
- Production monitoring (INFO, SLOWLOG)
- Advanced features (Lua scripting, HyperLogLog)
- Cluster support

## Performance

Current benchmarks show Ferrous achieving impressive performance:

### Single Operations:
- 54,000+ SET operations/sec
- 64,000+ GET operations/sec

### Pipelined Operations:
- 250,000+ PING operations/sec
- 156,000+ SET operations/sec 
- 161,000+ GET operations/sec
- 153,000+ INCR operations/sec
- 156,000+ SADD operations/sec
- 135,000+ HSET operations/sec
- 135,000+ ZADD operations/sec

### Known Issues:
- LPUSH: 1,972 operations/sec (significantly lower performance)

### Concurrent Clients (50):
- 75,000+ requests/sec

Average latency is approximately 0.06ms (sub-millisecond), with p50 times ranging from 0.4-3ms for most operations, with the exception of LPUSH (226ms).

These performance numbers are from debug builds and are expected to improve by 30-50% in release builds.

## Building and Running

```bash
# Build the project
cargo build

# Run the server
cargo run

# Build with optimizations for better performance
cargo build --release
```

## Testing

Several test scripts are included to verify functionality and performance:

```bash
# Run basic functionality tests
./test_basic.sh

# Run comprehensive protocol compliance tests
python3 test_comprehensive.py

# Run Redis command tests
./test_commands.sh

# Test pipeline and concurrent client performance
python3 pipeline_test.py

# Run performance benchmarks
./test_benchmark.sh
```