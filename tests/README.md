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

## 🔍 Validation Tests for Reported Issues

We have comprehensive validation tests for reported compatibility issues:

```bash
cd ferrous/tests
./run_validation_tests.sh  # Run all validation tests
```

The validation suite tests:
1. **Pub/Sub Protocol Compliance** - RESP2 format validation, redis-py compatibility
2. **Lua Scripting** - Atomic lock release, SCRIPT LOAD, hanging detection
3. **Missing Commands** - ZCARD and other potentially missing commands
4. **Event Bus Patterns** - Chainlit/Codemaestro compatibility scenarios

### Individual Validation Tests

```bash
# Pub/Sub protocol validation
python3 features/pubsub/test_pubsub_protocol_validation.py

# Lua scripting comprehensive tests
python3 features/lua/test_lua_comprehensive.py

# Missing commands tests
python3 features/commands/test_missing_commands.py

# Event bus compatibility
python3 features/event_bus/test_event_bus_compatibility.py
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
  - **`pubsub/`**: Pub/Sub tests including protocol validation
  - **`lua/`**: Lua scripting tests including atomic patterns
  - **`commands/`**: Command coverage and missing command tests
  - **`event_bus/`**: Event bus pattern compatibility tests
- **`performance/`**: Benchmarking suite

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
✅ **Validation tests added** - Comprehensive tests for reported compatibility issues