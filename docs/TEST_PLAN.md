# Ferrous Test Plan

## Overview

This document outlines the testing strategy for Ferrous, our Redis-compatible server implemented in Rust. The test plan is structured around the technical groupings in the project roadmap, with comprehensive test coverage for all implemented features.

## Testing Philosophy

Our testing approach follows these principles:

1. **Comprehensive Coverage**: Every feature must have corresponding tests
2. **Protocol Compliance**: Redis compatibility is tested at the protocol level
3. **Performance Validation**: Benchmarks against Redis/Valkey are required
4. **Client Compatibility**: Tests with major Redis client libraries
5. **Fuzzing and Edge Cases**: Security and reliability focus

## Test Categories

### 1. Unit Tests

All core components have dedicated unit tests:
- Protocol parsing/serialization
- Command handling logic
- Data structure operations
- Internal utilities

### 2. Integration Tests

Test interactions between subsystems:
- Command → Storage pipeline
- Protocol → Command integration
- Persistence mechanisms
- Pub/Sub functionality
- Transaction execution

### 3. Functional Tests

Test complete features from a user perspective:
- Command behavior matches Redis specification
- Error handling conforms to Redis behavior
- Multi-command sequences work correctly
- Admin operations function properly

### 4. Performance Tests

Benchmark performance against Redis/Valkey:
- Individual command throughput
- Latency distributions
- Pipeline performance
- Multi-client scenarios
- Memory efficiency

### 5. Compatibility Tests

Test with real-world Redis clients:
- redis-cli compatibility
- Language-specific client libraries
- Common Redis tools

## Test Matrix by Technical Group

### Technical Group 1: Foundation

| Component | Test Type | Test Tool | Status |
|-----------|-----------|-----------|--------|
| Network Layer | Integration | TCP client tests | ✅ COMPLETE |
| RESP Protocol | Unit + Fuzz | Protocol test suite | ✅ COMPLETE |
| Command Parser | Unit | Command parse tests | ✅ COMPLETE |
| Basic Commands | Functional | test_basic.sh | ✅ COMPLETE |

### Technical Group 2: Core Data Structures

| Component | Test Type | Test Tool | Status |
|-----------|-----------|-----------|--------|
| String Operations | Functional | test_strings.sh | ✅ COMPLETE |
| List Operations | Functional | test_lists.sh | ✅ COMPLETE |
| Set Operations | Functional | test_sets.sh | ✅ COMPLETE |
| Hash Operations | Functional | test_hashes.sh | ✅ COMPLETE |
| Key Management | Functional | test_keys.sh | ✅ COMPLETE |

### Technical Group 3: Advanced Features

| Component | Test Type | Test Tool | Status |
|-----------|-----------|-----------|--------|
| Sorted Sets | Functional | test_sorted_sets.sh | ✅ COMPLETE |
| RDB Persistence | Functional | test_persistence.py | ✅ COMPLETE |
| AOF Persistence | Functional | test_aof.py | ✅ COMPLETE |
| Pub/Sub System | Functional | test_pubsub.py | ✅ COMPLETE |
| Transactions | Functional | test_transactions.py | ✅ COMPLETE |

### Technical Group 4: Production Readiness

| Component | Test Type | Test Tool | Status |
|-----------|-----------|-----------|--------|
| Performance Optimization | Benchmark | redis-benchmark | ✅ COMPLETE |
| Pipelining | Benchmark | pipeline_test.py | ✅ COMPLETE |
| Concurrent Clients | Stress | concurrent_test.py | ✅ COMPLETE |
| Replication | Integration | repl_test.py | ⚠️ PLANNED |
| Monitoring | Functional | monitor_test.py | 🟡 PARTIAL |
| Security | Security | security_test.py | 🟡 PARTIAL |
| SCAN Commands | Functional | scan_test.py | ⚠️ PLANNED |

### Technical Group 5: Feature Completeness

| Component | Test Type | Test Tool | Status |
|-----------|-----------|-----------|--------|
| Lua Scripting | Functional | lua_test.py | ⚠️ PLANNED |
| Streams | Functional | streams_test.py | ⚠️ PLANNED |
| Extended Commands | Functional | extended_test.py | ⚠️ PLANNED |

### Technical Group 6: Scale-Out Architecture

| Component | Test Type | Test Tool | Status |
|-----------|-----------|-----------|--------|
| Cluster Protocol | Functional | cluster_test.py | ⚠️ PLANNED |
| Resharding | Integration | reshard_test.py | ⚠️ PLANNED |
| Failover | Chaos | failover_test.py | ⚠️ PLANNED |

## Performance Testing Methodology

Performance testing now focuses on validating that Ferrous maintains its performance advantage over Redis/Valkey:

### Current Performance Benchmarks

Recent benchmarks show Ferrous achieving impressive performance compared to Valkey 8.0.3:

| Operation | Ferrous (Release) | Valkey | Ratio |
|-----------|-------------------|---------|-------|
| PING_INLINE | 84,961 ops/sec | 73,637 ops/sec | **115%** |
| PING_MBULK | 86,880 ops/sec | 74,128 ops/sec | **117%** |
| SET | 84,889 ops/sec | 74,515 ops/sec | **114%** |
| GET | 69,881 ops/sec | 63,451 ops/sec | **110%** |
| INCR | 82,712 ops/sec | 74,794 ops/sec | **111%** |
| LPUSH | 81,366 ops/sec | 74,850 ops/sec | **109%** |
| RPUSH | 75,987 ops/sec | 73,046 ops/sec | **104%** |
| LPOP | 82,034 ops/sec | 73,421 ops/sec | **112%** |
| RPOP | 81,766 ops/sec | 71,022 ops/sec | **115%** |
| SADD | 80,450 ops/sec | 78,864 ops/sec | **102%** |
| HSET | 80,971 ops/sec | 78,554 ops/sec | **103%** |

### Benchmark Suite

Our standard benchmark suite includes:

1. **Basic Command Tests**
   ```bash
   redis-benchmark -h 127.0.0.1 -p 6379 -t ping,set,get,incr,lpush,rpush,lpop,rpop,sadd,hset -n 100000 -q
   ```

2. **Pipeline Performance**
   ```bash
   redis-benchmark -h 127.0.0.1 -p 6379 -P 16 -n 100000 -q
   ```
   
3. **Concurrent Client Load**
   ```bash
   redis-benchmark -h 127.0.0.1 -p 6379 -c 50 -n 100000 -q
   ```
   
4. **Latency Distribution**
   ```bash
   redis-benchmark --latency-dist -i 1 -n 100000
   ```

## Regression Testing

Regression tests run on each commit to ensure no performance degradation:

1. **Performance Regression**: Ensure no command drops below 95% of its baseline performance
2. **Memory Usage**: Validate memory usage remains within 110% of baseline
3. **Latency Consistency**: p99 latency must not increase by more than 10%

## Client Library Compatibility

Tests with major Redis client libraries ensure broad compatibility:

| Client Library | Language | Test Status |
|----------------|----------|-------------|
| redis-py | Python | ✅ Passing |
| node-redis | JavaScript | ✅ Passing |
| jedis | Java | ✅ Passing |
| go-redis | Go | ✅ Passing |
| StackExchange.Redis | C# | ✅ Passing |

## Current Testing Priorities

Based on the current implementation state and roadmap priorities, these are the highest testing priorities:

1. **Replication Testing Framework**: Develop comprehensive tests for the upcoming replication implementation
2. **SCAN Command Testing**: Create test suite for the SCAN command family
3. **Long-running Stability Tests**: Extended duration testing (24h+) with varied workloads
4. **Large Dataset Behavior**: Testing with datasets exceeding available memory

## Continuous Integration

All tests run in CI on every PR and merge to main:
- Unit and integration tests
- Functional test suite
- Performance benchmarks (with alerting on regression)
- Client compatibility tests
- Code coverage reporting

This test plan will be updated as new features are implemented and priorities evolve.