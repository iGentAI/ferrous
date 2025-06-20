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

### Production Build Performance (vs Valkey 8.0.3):

| Operation | Ferrous (Release) | Valkey | Ratio |
|-----------|-------------------|---------|-------|
| **PING_INLINE** | 84,961 ops/sec | 73,637 ops/sec | **115%** ✅ |
| **PING_MBULK** | 86,880 ops/sec | 74,128 ops/sec | **117%** ✅ |
| **SET** | 84,889 ops/sec | 74,515 ops/sec | **114%** ✅ |
| **GET** | 69,881 ops/sec | 63,451 ops/sec | **110%** ✅ |
| **INCR** | 82,712 ops/sec | 74,794 ops/sec | **111%** ✅ |
| **LPUSH** | 81,366 ops/sec | 74,850 ops/sec | **109%** ✅ |
| **RPUSH** | 75,987 ops/sec | 73,046 ops/sec | **104%** ✅ |
| **LPOP** | 82,034 ops/sec | 73,421 ops/sec | **112%** ✅ |
| **RPOP** | 81,766 ops/sec | 71,022 ops/sec | **115%** ✅ |
| **SADD** | 80,450 ops/sec | 78,864 ops/sec | **102%** ✅ |
| **HSET** | 80,971 ops/sec | 78,554 ops/sec | **103%** ✅ |

Average latency: ~0.29ms (Ferrous) vs ~0.32ms (Valkey)

### Key Achievements:
- **Outperforms Redis/Valkey** on ALL operations by 2-17%
- **Multi-threaded architecture** provides consistently lower latency
- **Production build improvements** show 10-60% gains over debug builds

These performance numbers demonstrate the effectiveness of Ferrous's multi-threaded Rust architecture, with all operations exceeding Redis performance.

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