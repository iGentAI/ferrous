# Ferrous Test Suite

## 🚀 Quick Start

Use the unified test runner from the project root:

```bash
cd ferrous/
./run_tests.sh default  # Most tests (no auth)
./run_tests.sh auth     # Replication tests  
./run_tests.sh perf     # Performance benchmarks
./run_tests.sh unit     # Rust unit tests
./run_tests.sh all      # Everything
```

## Test Configurations

### 1. Default Configuration (Most Tests)
- **Server**: `./target/release/ferrous`
- **Authentication**: None
- **Tests**: Basic functionality, protocol, features, Lua scripting
- **Run**: `./run_tests.sh default`

### 2. Authenticated Configuration  
- **Server**: `./target/release/ferrous master.conf` 
- **Authentication**: Password `mysecretpassword`
- **Tests**: Replication, authentication features
- **Run**: `./run_tests.sh auth`

### 3. Performance Configuration
- **Server**: `./target/release/ferrous > /dev/null 2>&1 &`
- **Authentication**: None
- **Tests**: Benchmarks vs Redis/Valkey 8.0.4  
- **Run**: `./run_tests.sh perf`

## Directory Structure

- **`integration/`**: End-to-end tests (basic commands, replication)
- **`protocol/`**: RESP protocol compliance tests
- **`features/`**: Specific Redis features (client, memory, monitoring)
- **`performance/`**: Benchmarking suite

## Manual Test Running

All tests updated to work with default configuration except:
- `integration/test_replication.sh` - needs `master.conf`
- `features/auth/*` - authentication-specific tests

## Prerequisites

```bash
pip install redis
sudo dnf install -y redis  # For redis-benchmark
```

## Recent Updates

✅ **Authentication alignment** - Most tests work without auth  
✅ **Global Lua script cache** - SCRIPT LOAD/EVALSHA fixed  
✅ **Performance validation** - Exceeds Redis/Valkey 8.0.4 in 8/9 operations  
✅ **All tests passing** - 57 unit tests + comprehensive integration tests