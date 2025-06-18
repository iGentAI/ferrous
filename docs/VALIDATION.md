# Ferrous Validation Criteria

## Overview

This document defines the validation criteria and methodology for ensuring Ferrous is a true drop-in replacement for Redis. We validate across three dimensions: protocol compatibility, functional correctness, and performance.

## 1. Protocol Compatibility Validation

### RESP Protocol Conformance

#### Test Suite
```rust
// Protocol test cases
- Valid RESP2 parsing
- Valid RESP3 parsing
- Inline command parsing
- Error response format
- Bulk string handling
- Array handling
- Integer responses
- Null responses
```

#### Validation Method
1. **Unit tests**: Parsing edge cases and malformed data
2. **Fuzzing**: Random protocol data to ensure robustness
3. **Compatibility tests**: Real client libraries against Ferrous

### Client Library Compatibility Matrix

| Client | Language | Priority | Test Coverage |
|--------|----------|----------|---------------|
| redis-cli | C | P0 | 100% interactive commands |
| redis-py | Python | P0 | Full test suite |
| jedis | Java | P0 | Integration tests |
| node-redis | JavaScript | P0 | All async operations |
| go-redis | Go | P1 | Concurrent operations |
| StackExchange.Redis | C# | P1 | Pipeline/transaction tests |

#### Validation Process
```bash
# For each client library:
1. Run client test suite against Ferrous
2. Compare results with Redis
3. Benchmark performance differences
4. Document any deviations
```

## 2. Functional Validation

### Command Compatibility

#### Coverage Requirements
- Phase 1: 50 core commands (90% of typical usage)
- Phase 2: 150 commands (99% coverage)
- Phase 3: Full command set

#### Command Validation Framework
```rust
trait CommandValidator {
    fn validate_syntax(&self) -> Result<(), ValidationError>;
    fn validate_behavior(&self) -> Result<(), ValidationError>;
    fn validate_errors(&self) -> Result<(), ValidationError>;
    fn validate_edge_cases(&self) -> Result<(), ValidationError>;
}
```

### Redis Test Suite Integration

#### Running Redis Tests
```bash
# Adapt Redis TCL tests to run against Ferrous
./runtest --host 127.0.0.1 --port 6379 --single unit/type/string
./runtest --host 127.0.0.1 --port 6379 --single unit/type/list
# ... for each test module
```

#### Test Categories
1. **Unit Tests**: Individual command behavior
2. **Integration Tests**: Multi-command scenarios  
3. **Regression Tests**: Known Redis bugs/edge cases
4. **Stress Tests**: High load scenarios

### Data Structure Validation

For each data structure:

#### Strings
- Encoding validation (int, embstr, raw)
- Memory limits
- Binary safety
- Unicode handling

#### Lists
- Operations at both ends
- Large list handling (>1M elements)
- Blocking operations
- Memory efficiency

#### Sets
- Intersection/Union/Diff operations
- Large sets (>1M members)
- Integer set optimization

#### Sorted Sets
- Score precision
- Lexicographical ordering
- Range queries
- Large sorted sets

#### Hashes
- Field limits
- Ziplist → hashtable conversion
- Large value handling

## 3. Performance Validation

### Benchmarking Methodology

#### Standard redis-benchmark Tests
```bash
# Default test suite
redis-benchmark -h 127.0.0.1 -p 6379 -t set,get,incr,lpush,rpush,lpop,rpop,sadd,hset,spop,zadd,sort -q

# Pipeline test
redis-benchmark -h 127.0.0.1 -p 6379 -P 16 -q

# Large payload test
redis-benchmark -h 127.0.0.1 -p 6379 -d 1024 -q

# Concurrent clients
redis-benchmark -h 127.0.0.1 -p 6379 -c 50 -q
```

#### Performance Targets

Based on direct benchmark comparison with Redis (Valkey), we've refined our performance targets:

| Benchmark | Redis Baseline | Ferrous Target | Current Status |
|-----------|---------------|----------------|---------------|
| GET | ~72,500 ops/s | ≥72,500 ops/s | 44,600 ops/s (61.5%) |
| SET | ~73,500 ops/s | ≥73,500 ops/s | 42,300 ops/s (57.6%) |
| INCR | ~95,000 ops/s | ≥95,000 ops/s | Not benchmarked |
| LPUSH | ~90,000 ops/s | ≥90,000 ops/s | Not implemented |
| RPUSH | ~90,000 ops/s | ≥90,000 ops/s | Not implemented |
| LPOP | ~85,000 ops/s | ≥85,000 ops/s | Not implemented |
| SADD | ~85,000 ops/s | ≥85,000 ops/s | Not implemented |
| HSET | ~80,000 ops/s | ≥80,000 ops/s | Not implemented |
| ZADD | ~70,000 ops/s | ≥70,000 ops/s | Not benchmarked |
| Pipeline PING (10) | ~650,000 ops/s | ≥650,000 ops/s | Not fully supported |
| 50 Concurrent Clients | ~73,000 ops/s | ≥73,000 ops/s | Not fully supported |
| Latency (avg) | ~0.05ms | ≤0.05ms | ~0.12ms (240% higher) |

#### Multi-threaded Performance Expectations

Given Ferrous's multi-threaded architecture, we expect to exceed Redis's performance with multiple cores:

```bash
# Updated benchmark scaling expectations
for c in 1 10 50 100 200 500 1000; do
    echo "Clients: $c"
    redis-benchmark -c $c -n 1000000 -t get,set -q
done
```

| Clients | Redis Scaling | Expected Ferrous Scaling |
|---------|---------------|--------------------------|
| 1 | Baseline | Baseline |
| 10 | ~100% | ~110% |
| 50 | ~100% | ~150% |
| 100 | ~95% | ~180% |
| 200 | ~90% | ~250% |
| 500 | ~80% | ~300% |
| 1000 | ~70% | ~350% |

Current scaling targets are not yet met, as concurrent client handling needs improvement.

#### Performance Validation Methodology Updates

1. **Direct Comparison Benchmarking**
   - Run identical workloads on both Redis and Ferrous
   - Capture detailed metrics beyond ops/sec (latency distributions, memory usage)
   - Identify specific bottlenecks in Ferrous implementation

2. **Performance Regression Testing**
   - Automate benchmark testing in CI pipeline
   - Track performance relative to baseline Redis
   - Alert on performance degradation across commits

3. **Scaling and Concurrency Testing**
   - Verify multi-core utilization under load
   - Test with progressive concurrency levels (1-1000 clients)
   - Measure throughput vs. latency tradeoffs

4. **Profiling and Optimization**
   - Use flamegraphs to identify hot spots
   - Benchmark individual components (protocol parser, command handler, storage engine)
   - Focus optimization efforts on highest-impact areas

### Memory Usage Validation

#### Memory Efficiency Tests
```bash
# Compare memory usage for same dataset
1. Load 1M keys with 100-byte values
2. Measure RSS memory
3. Compare with Redis baseline
4. Target: ±20% of Redis memory usage
```

#### Memory Leak Detection
- Valgrind equivalent for Rust (using built-in tools)
- Long-running tests (24+ hours)
- Memory growth monitoring

### Latency Validation

#### Latency Requirements
```
P50: < 1ms
P95: < 2ms  
P99: < 5ms
P99.9: < 10ms
```

#### Latency Testing
```bash
# Redis latency monitoring equivalent
redis-cli --latency-history
redis-cli --latency-dist
```

## 4. Compatibility Test Suite

### Automated Test Pipeline

```yaml
test_pipeline:
  - protocol_tests:
      - resp2_compliance
      - resp3_compliance
      - error_formats
  
  - command_tests:
      - string_commands
      - list_commands
      - set_commands
      - hash_commands
      - sorted_set_commands
  
  - client_tests:
      - redis_py_full_suite
      - jedis_integration
      - node_redis_async
  
  - benchmark_tests:
      - single_threaded_perf
      - multi_threaded_scaling
      - memory_efficiency
      - latency_percentiles
  
  - stress_tests:
      - concurrent_clients_1000
      - large_dataset_10GB
      - sustained_load_24h
```

### Regression Test Suite

Track specific Redis behaviors that must be preserved:

```rust
#[test]
fn test_expire_precision() {
    // Redis expires keys with 1ms precision
}

#[test]
fn test_negative_expire() {
    // Redis deletes key immediately on negative expire
}

#[test]
fn test_zunionstore_weights() {
    // Specific weight calculation behavior
}
```

## 5. Validation Reporting

### Compatibility Report Format

```markdown
# Ferrous v0.1.0 Redis Compatibility Report

## Protocol Compatibility: 100%
- RESP2: ✅ Full support
- RESP3: ✅ Full support
- Inline: ✅ Full support

## Command Compatibility: 95/200 (47.5%)
- Strings: 20/22 (90.9%)
- Lists: 15/17 (88.2%)
- Sets: 14/15 (93.3%)
- ...

## Performance vs Redis 7.2:
- Single-threaded: 98% parity
- Multi-threaded: 340% improvement (4 cores)
- Memory usage: 105% of Redis

## Client Compatibility:
- redis-cli: ✅ 100%
- redis-py: ✅ 100%
- jedis: ✅ 100%
- ...

## Known Differences:
1. Multi-threaded by default
2. Different memory allocator
3. ...
```

### Continuous Validation

1. **Nightly Builds**: Run full test suite
2. **Per-Commit**: Run core tests
3. **Weekly**: Full benchmark comparison
4. **Monthly**: Client library compatibility

## 6. Validation Tools

### Custom Tools Development

1. **ferrous-check**: Validate Redis data files
2. **ferrous-compare**: Compare behavior with Redis
3. **ferrous-bench**: Extended benchmarking tool
4. **ferrous-fuzz**: Protocol fuzzing tool

### Integration with Existing Tools

```bash
# Redis tools that must work with Ferrous:
- redis-benchmark ✅
- redis-cli ✅
- redis-check-rdb (Phase 2)
- redis-check-aof (Phase 2)
- redis-sentinel (Phase 3)
```

## Success Criteria

Ferrous is considered validated when:

1. **Protocol**: 100% RESP2/RESP3 compatibility
2. **Commands**: 95%+ of Redis commands implemented correctly
3. **Performance**: Within 10% of Redis for single-threaded workloads
4. **Clients**: Top 5 client libraries pass their test suites
5. **Tools**: redis-benchmark and redis-cli work flawlessly
6. **Stability**: 24-hour stress test with no crashes or leaks

## Validation Timeline

- Week 1-2: Protocol validation framework
- Week 3-4: Command validation suite  
- Week 5-6: Performance benchmarking
- Week 7-8: Client library testing
- Week 9-10: Stress testing and hardening
- Week 11-12: Final validation report

This validation process ensures Ferrous is a true drop-in replacement for Redis while leveraging Rust's advantages for better performance and safety.