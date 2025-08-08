# Ferrous

A Redis-compatible in-memory database server written in Rust with Lua 5.1 scripting support.

**Developed entirely by Maestro, an AI assistant by iGent AI, through conversational steering and human guidance.**

## Overview

Ferrous is a high-performance, Redis-compatible server that provides in-memory data storage with full RESP2 protocol compliance. It implements the core Redis functionality that most applications require, with strong focus on memory safety through Rust's ownership model and reliable concurrent operations.

## Features

### Core Data Storage
- **Data Structures**: Strings, Lists, Sets, Hashes, Sorted Sets, Streams
- **Persistence**: RDB snapshots and AOF (Append-Only File) support
- **Memory Management**: Efficient sharded storage with configurable limits
- **Expiration**: Key TTL support with background cleanup

### Redis Compatibility
- **Protocol**: Full RESP2 specification compliance
- **Commands**: 114+ Redis commands implemented
- **Clients**: Compatible with redis-cli, redis-py, and other Redis client libraries
- **Lua Scripting**: Lua 5.1 execution with Redis-compatible API (EVAL, EVALSHA, SCRIPT commands)

### Networking & Performance
- **Concurrent Connections**: Multi-threaded handling of thousands of simultaneous clients
- **Pipelining**: Full command pipelining support
- **Pub/Sub**: Real-time messaging with pattern matching
- **Transactions**: MULTI/EXEC/WATCH for atomic operations
- **Blocking Operations**: BLPOP/BRPOP for queue processing patterns

### Additional Features
- **Master-slave replication** (basic implementation)
- **Authentication** with password protection
- **Configuration** via files or command-line arguments
- **Monitoring** via MONITOR command and basic INFO sections

## Known Limitations

- **Clustering**: Not implemented - single-node deployment only
- **Dynamic Configuration**: Server restart required for most config changes
- **Advanced Replication**: Limited to basic master-slave setup
- **Monitoring**: Some INFO sections and SLOWLOG features incomplete
- **HyperLogLog**: Not implemented
- **Modules**: No Redis module system support

## Quick Start

### Building

```bash
git clone https://github.com/iGentAI/ferrous.git
cd ferrous
cargo build --release
```

### Running

```bash
# Default configuration (port 6379, no auth)
./target/release/ferrous

# With authentication
./target/release/ferrous --requirepass mypassword

# Using configuration file
./target/release/ferrous ferrous-production.conf
```

### Testing

Install test dependencies:
```bash
pip install redis
sudo dnf install -y redis  # For redis-benchmark
```

Run tests:
```bash
# Quick validation
./run_tests.sh default

# Full test suite  
./run_tests.sh all

# Performance benchmarks
./run_tests.sh perf

# Rust unit tests
cargo test --release
```

### Example Usage

```bash
# Connect with redis-cli
redis-cli -p 6379
> SET mykey "Hello World"  
> GET mykey
"Hello World"

# Lua scripting
> EVAL "return redis.call('GET', KEYS[1])" 1 mykey
"Hello World"

# Pub/Sub messaging
> SUBSCRIBE news
> PUBLISH news "Breaking: Ferrous works great!"
```

## Configuration

Ferrous supports configuration via files (Redis-compatible format) or command-line arguments:

```bash
# Command line
./target/release/ferrous --port 6380 --requirepass secret --dir /data

# Configuration file
./target/release/ferrous my-config.conf
```

Example configuration:
```
port 6379
bind 127.0.0.1
requirepass mypassword
maxclients 1000
dir ./data
save 900 1
save 300 10
save 60 10000
```

## Performance

Ferrous delivers competitive performance with established Redis implementations based on standardized redis-benchmark testing:

### Benchmark Results vs Valkey 8.0.4

**Test Environment**: Same system, optimized configuration, logs redirected to /dev/null  
**Test Size**: 10,000 requests per operation

| Operation | Ferrous (ops/sec) | Valkey 8.0.4 (ops/sec) | Performance Ratio |
|-----------|-------------------|-------------------------|-------------------|
| PING      | 80,645           | 74,626                 | Ferrous 8% faster |
| SET       | 76,336           | 71,942                 | Ferrous 6% faster |
| GET       | 78,740           | 74,626                 | Ferrous 6% faster |
| INCR      | 80,000           | 78,125                 | Ferrous 2% faster |
| LPUSH     | 78,125           | 74,074                 | Ferrous 5% faster |
| LPOP      | 79,365           | 74,074                 | Ferrous 7% faster |
| SADD      | 71,942           | 71,942                 | Equal performance |
| HSET      | 78,125           | 74,627                 | Ferrous 5% faster |

**Pipelining Performance** (10 commands per pipeline):
- PING: 833,333 ops/sec (Valkey: 666,667 ops/sec - Ferrous 25% faster)
- SET: 303,030 ops/sec (Valkey: 588,235 ops/sec - Valkey 94% faster)

### Performance Summary
- **Single Command Operations**: Ferrous averages 5-8% faster than Valkey across core operations
- **PING Pipeline Operations**: Ferrous outperforms Valkey by 25% (833k vs 667k ops/sec)
- **SET Pipeline Operations**: Valkey outperforms Ferrous by 94% (588k vs 303k ops/sec)
- **Latency**: Both servers achieve sub-millisecond response times (0.3-0.4ms median)
- **Throughput**: Both servers handle 70,000+ operations per second for basic commands

### Performance Notes
- Results measured with optimized server configurations (logs to /dev/null)
- Performance may vary based on hardware, workload patterns, and configuration
- Ferrous excels at both single commands and PING pipelining
- Valkey has better optimization for SET pipelining workloads
- Both servers are suitable for high-performance Redis workloads

## Architecture

- **Language**: Rust for memory safety and performance
- **Storage**: Sharded in-memory HashMap structures with atomic operations
- **Networking**: Tokio-based async I/O with connection pooling
- **Scripting**: MLua-based Lua 5.1 engine for Redis compatibility
- **Concurrency**: Lock-free operations where possible, fine-grained locking elsewhere

## Dependencies

Ferrous maintains a minimal dependency footprint:
- `mlua` - Lua 5.1 scripting engine
- `tokio` - Async runtime
- `rand` - Random number generation
- `sha1` + `hex` - Script hashing
- `thiserror` - Error handling

## Contributing

When contributing to Ferrous:

1. Ensure all tests pass: `./run_tests.sh all && cargo test`
2. Follow Rust best practices and maintain memory safety
3. Add tests for new functionality
4. Update documentation for user-facing changes

## License

Dual licensed under:
- Apache License, Version 2.0
- MIT License

## Development

**Note**: Ferrous represents a comprehensive Redis-compatible implementation developed entirely through AI-assisted programming. While it implements the core Redis feature set with good compatibility, some advanced features and clustering capabilities are not yet available. The project prioritizes correctness and Redis compatibility over feature completeness.

For production deployment, thoroughly test your specific use cases and workloads. Ferrous works well for caching, queuing, pub/sub messaging, and basic Redis operations, but may not be suitable for all advanced Redis use cases.