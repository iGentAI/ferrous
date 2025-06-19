# ferrous
A Redis-compatible in-memory database server written in pure Rust with zero external dependencies

## Project Status

Ferrous is currently at Phase 3 implementation, with the following features fully functional:

### Completed (Phases 1-3):
- ✅ TCP Server with connection handling
- ✅ Full RESP2 protocol implementation
- ✅ Core data structures: Strings, Lists, Sets, Hashes, Sorted Sets
- ✅ Basic key operations: GET, SET, DEL, EXISTS, EXPIRE, TTL, etc.
- ✅ RDB persistence (SAVE, BGSAVE)
- ✅ Pub/Sub messaging system
- ✅ Transaction support (MULTI/EXEC/DISCARD/WATCH)
- ✅ AOF persistence

### In Progress / Coming Soon (Phases 4-6):
- Master-slave replication
- Production monitoring (INFO, SLOWLOG)
- Advanced features (Lua scripting, HyperLogLog)
- Cluster support

## Performance

Current benchmarks show Ferrous achieving approximately:
- 50,000 SET operations/sec
- 55,000 GET operations/sec 
- 0.16ms average latency

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

# Run performance benchmarks
./test_benchmark.sh
```