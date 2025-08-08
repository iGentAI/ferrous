# Ferrous Comprehensive Testing Guide

This guide documents the extensive test suite for Ferrous, covering ~200 individual test functions across comprehensive Redis functionality validation.

## Test Suite Overview

Ferrous uses a **unified test framework** that validates all aspects of Redis compatibility with comprehensive edge case coverage and concurrent operation validation.

### Test Infrastructure
- **48 Python test files** - Feature validation and integration testing
- **17 Shell scripts** - Integration testing and performance benchmarks  
- **74 Rust unit tests** - Core functionality and storage engine validation
- **Multiple configuration scenarios** - Tests requiring specific server setups

## Test Execution

### Quick Testing
```bash
# Basic functionality validation (~15 minutes)
./run_tests.sh default

# Core unit tests only (~2 minutes)  
./run_tests.sh unit

# Performance benchmarking (~10 minutes)
./run_tests.sh perf
```

### Complete Validation
```bash
# Full comprehensive testing (45+ minutes)
./run_tests.sh all

# This runs all categories:
# - Unit tests (Rust core functionality)
# - Integration tests (Redis command validation)
# - Feature tests (Advanced functionality)
# - Performance tests (Benchmarking)
# - Monitoring tests (Configuration-dependent)
# - Load tests (Concurrent stress validation)
# - Authentication tests (Replication scenarios)
```

### Specialized Testing Categories

#### **Configuration-Dependent Tests**
```bash
# Monitoring features (requires ferrous-monitoring.conf)
./run_tests.sh monitoring
# Tests: slowlog, monitor functionality, stats collection

# High-load optimization (log redirection for max performance)
./run_tests.sh load  
# Tests: stress testing, concurrent operations, resource limits

# Authentication & replication (master.conf)
./run_tests.sh auth
# Tests: replication, authentication scenarios
```

#### **Standalone Test Execution**
```bash
# Protocol compliance validation
python3 tests/features/protocol/test_wrongtype_compliance.py

# Pub/sub concurrency testing (critical for message systems)
python3 tests/features/pubsub/test_pubsub_concurrency_comprehensive.py

# Memory command validation
python3 tests/features/memory/test_memory_efficient.py

# Lua scripting comprehensive validation
python3 tests/features/lua/test_lua_comprehensive.py
```

## Test Categories Explained

### **Core Functionality Tests**
- **Protocol Compliance**: RESP2 specification validation, error handling
- **Basic Operations**: GET, SET, DEL, EXISTS with edge cases
- **Data Structures**: Lists, Sets, Hashes, Sorted Sets with comprehensive validation
- **String Operations**: All Redis string commands with Unicode and binary support

### **Advanced Feature Tests**  
- **Lua Scripting**: EVAL, EVALSHA, SCRIPT commands with complex script validation
- **Streams**: Time-series data operations with stress testing
- **Pub/Sub Messaging**: Channel and pattern subscriptions with concurrency validation
- **Transactions**: MULTI/EXEC/WATCH with isolation and optimistic concurrency
- **Blocking Operations**: BLPOP/BRPOP queue patterns with timeout precision

### **Concurrency & Performance Tests**
- **Multi-client simulation**: Up to 100 concurrent connections
- **Concurrent data operations**: 1000+ simultaneous operations
- **Pipeline efficiency**: High-throughput operation batching
- **Resource cleanup**: Connection lifecycle and memory management
- **Load testing**: Performance validation under stress

### **Edge Case & Reliability Tests**
- **Large data handling**: 1MB+ values, 10K+ collection sizes
- **Unicode support**: Multi-language character handling  
- **Binary data**: All byte values 0-255 validation
- **Protocol edge cases**: Malformed input handling
- **Error scenarios**: Network failures, timeouts, invalid operations

## Critical Test Validations

### **Redis Compatibility Verification**
- **Protocol compliance**: 15/15 RESP2 tests passing
- **Command coverage**: 114 Redis commands with proper error handling
- **Client library support**: redis-py, redis-cli, raw socket compatibility
- **Performance benchmarking**: Superior to Redis/Valkey baseline in most operations

### **Concurrency Robustness**
- **Pub/sub operations**: 9/9 concurrent tests passing
- **Blocking operations**: 7/7 tests with proper FIFO semantics
- **Connection stress**: 100 concurrent connections with 100% success
- **Data integrity**: 3/3 tests with cross-command safety validation

### **Production Readiness Indicators**
- **Memory efficiency**: Sub-second operations with large datasets
- **Error handling**: Proper Redis error responses without connection closures  
- **Resource management**: Clean connection lifecycle without leaks
- **Protocol security**: No internal implementation details exposed

## Known Test Configuration Requirements

### **Slowlog Testing**
Slowlog tests require server configuration enabling monitoring features:
```bash
# Use monitoring configuration for slowlog tests
./target/release/ferrous ferrous-monitoring.conf
```

### **Performance Testing**  
For optimal performance benchmarking, use log redirection:
```bash
./target/release/ferrous > /dev/null 2>&1 &
```

### **Authentication Testing**
Replication and authenticated scenarios use master.conf:
```bash
./target/release/ferrous master.conf
```

## Test Development Guidelines

### **Adding New Tests**
- Place feature tests in `tests/features/{category}/`
- Use proper Redis client patterns (redis-py recommended)
- Include comprehensive error handling and edge cases
- Add proper timeouts for operations that may hang
- Follow Redis protocol semantics (fire-and-forget for pub/sub, etc.)

### **Test Quality Standards**
- **Concurrent safety**: Test multi-threaded scenarios for pub/sub and blocking operations
- **Protocol compliance**: Use proper RESP format for raw socket tests
- **Resource cleanup**: Ensure tests clean up connections and data
- **Performance consideration**: Use reasonable data sizes (hundreds, not tens of thousands)
- **Error validation**: Test both success and failure scenarios

## Troubleshooting Tests

### **Common Issues**
- **Hanging tests**: Usually due to inefficient connection patterns or large datasets
- **Timeout failures**: May indicate server not ready or connection issues
- **Protocol errors**: Check RESP encoding for raw socket tests (byte counts must be exact)
- **Concurrency failures**: Indicate real bugs in multi-threaded scenarios

### **Debugging Commands**
```bash
# Check server responsiveness
redis-cli -p 6379 PING

# Monitor server logs during testing
./target/release/ferrous  # (without background redirection)

# Test specific functionality in isolation
redis-cli -p 6379 COMMAND COUNT  # Should return 114
redis-cli -p 6379 SCRIPT LOAD 'return redis.call("PING")'  # Should return SHA1
```

The comprehensive test framework ensures Ferrous maintains Redis compatibility and production reliability across all usage patterns, concurrent access scenarios, and edge cases.