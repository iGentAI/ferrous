# Ferrous Test Plan

## Overview

This document outlines the comprehensive testing strategy for Ferrous. Our testing philosophy emphasizes correctness, compatibility, and performance through multiple layers of testing.

## Testing Principles

1. **Test-Driven Development**: Write tests before implementation
2. **Comprehensive Coverage**: Aim for >90% code coverage
3. **Property-Based Testing**: Use fuzzing for protocol robustness
4. **Behavior Compatibility**: Match Redis behavior exactly, including edge cases
5. **Performance Regression**: Automated benchmarks to catch slowdowns

## Test Categories

### 1. Unit Tests

Located in each module next to the code:

```rust
// src/protocol/resp.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_string() {
        let input = b"+OK\r\n";
        let result = parse_resp(input);
        assert_eq!(result, RespValue::SimpleString("OK".to_string()));
    }

    #[test]
    fn test_parse_error() {
        let input = b"-Error message\r\n";
        let result = parse_resp(input);
        assert_eq!(result, RespValue::Error("Error message".to_string()));
    }

    #[test]
    fn test_parse_integer() {
        let input = b":1000\r\n";
        let result = parse_resp(input);
        assert_eq!(result, RespValue::Integer(1000));
    }
}
```

#### Unit Test Coverage Areas

**Protocol Layer**
- RESP parsing (all types, edge cases, malformed input)
- RESP serialization (round-trip testing)
- Command parsing
- Response building

**Data Structures**
```rust
// String operations
#[test]
fn test_string_set_get() {
    let mut db = Database::new();
    db.set("key", "value");
    assert_eq!(db.get("key"), Some("value"));
}

#[test]
fn test_string_expiration() {
    let mut db = Database::new();
    db.set_with_expire("key", "value", Duration::from_millis(100));
    assert!(db.exists("key"));
    thread::sleep(Duration::from_millis(150));
    assert!(!db.exists("key"));
}

// List operations
#[test]
fn test_list_push_pop() {
    let mut list = RedisList::new();
    list.lpush("item1");
    list.rpush("item2");
    assert_eq!(list.lpop(), Some("item1"));
    assert_eq!(list.rpop(), Some("item2"));
}
```

**Memory Management**
```rust
#[test]
fn test_lru_eviction() {
    let mut cache = LRUCache::new(2);
    cache.set("k1", "v1");
    cache.set("k2", "v2");
    cache.get("k1"); // Access k1
    cache.set("k3", "v3"); // Should evict k2
    assert!(cache.get("k1").is_some());
    assert!(cache.get("k2").is_none());
    assert!(cache.get("k3").is_some());
}
```

### 2. Integration Tests

Located in `tests/` directory:

```rust
// tests/redis_compatibility.rs
use ferrous::Client;

#[test]
fn test_redis_workflow() {
    let mut client = Client::connect("127.0.0.1:6379").unwrap();
    
    // String operations
    assert_eq!(client.set("key", "value"), Ok("OK"));
    assert_eq!(client.get("key"), Ok(Some("value")));
    
    // List operations
    assert_eq!(client.lpush("list", "item1"), Ok(1));
    assert_eq!(client.lpush("list", "item2"), Ok(2));
    assert_eq!(client.lrange("list", 0, -1), Ok(vec!["item2", "item1"]));
    
    // Transactions
    client.multi().unwrap();
    client.incr("counter").unwrap();
    client.incr("counter").unwrap();
    let results = client.exec().unwrap();
    assert_eq!(results, vec![1, 2]);
}
```

#### Integration Test Scenarios

**Multi-Client Concurrency**
```rust
#[test]
fn test_concurrent_clients() {
    let server_addr = "127.0.0.1:6379";
    let handles: Vec<_> = (0..10).map(|i| {
        thread::spawn(move || {
            let mut client = Client::connect(server_addr).unwrap();
            for j in 0..1000 {
                let key = format!("key{}:{}", i, j);
                client.set(&key, "value").unwrap();
                assert_eq!(client.get(&key).unwrap(), Some("value"));
            }
        })
    }).collect();
    
    for handle in handles {
        handle.join().unwrap();
    }
}
```

**Pipeline Testing**
```rust
#[test]
fn test_pipeline() {
    let mut client = Client::connect("127.0.0.1:6379").unwrap();
    let mut pipe = client.pipeline();
    
    for i in 0..100 {
        pipe.set(format!("key{}", i), format!("value{}", i));
    }
    
    let results = pipe.execute().unwrap();
    assert_eq!(results.len(), 100);
    assert!(results.iter().all(|r| r == &"OK"));
}
```

### 3. Compatibility Tests

Port Redis's TCL test suite to Rust:

```rust
// tests/compat/strings.rs
#[test]
fn test_set_get_compat() {
    // Matches Redis test: unit/type/string.tcl
    let mut client = Client::connect("127.0.0.1:6379").unwrap();
    
    // Basic SET/GET
    client.set("foo", "bar").unwrap();
    assert_eq!(client.get("foo").unwrap(), Some("bar"));
    
    // SET with NX
    assert_eq!(client.set_nx("foo", "baz").unwrap(), false);
    assert_eq!(client.get("foo").unwrap(), Some("bar"));
    
    // SET with XX
    assert_eq!(client.set_xx("newkey", "value").unwrap(), false);
    assert!(client.get("newkey").unwrap().is_none());
}
```

### 4. Property-Based Tests

Using a property-testing framework:

```rust
// tests/property_tests.rs
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_set_get_property(key in "[a-zA-Z0-9]+", value in ".*") {
        let mut client = Client::connect("127.0.0.1:6379").unwrap();
        client.set(&key, &value).unwrap();
        assert_eq!(client.get(&key).unwrap(), Some(value));
    }
    
    #[test]
    fn test_incr_decr_property(initial in i64::MIN/2..i64::MAX/2) {
        let mut client = Client::connect("127.0.0.1:6379").unwrap();
        client.set("counter", &initial.to_string()).unwrap();
        client.incr("counter").unwrap();
        let result: i64 = client.get("counter").unwrap().unwrap().parse().unwrap();
        assert_eq!(result, initial + 1);
    }
}
```

### 5. Fuzz Tests

```rust
// tests/fuzz_targets/protocol.rs
#![no_main]
use libfuzzer_sys::fuzz_target;
use ferrous::protocol::parse_resp;

fuzz_target!(|data: &[u8]| {
    // Should not panic on any input
    let _ = parse_resp(data);
});
```

### 6. Performance Tests

```rust
// benches/commands.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_get_command(c: &mut Criterion) {
    let server = TestServer::new();
    let mut client = Client::connect(server.addr()).unwrap();
    
    // Prepare data
    for i in 0..1000 {
        client.set(format!("key{}", i), "value").unwrap();
    }
    
    c.bench_function("GET command", |b| {
        let mut i = 0;
        b.iter(|| {
            let key = format!("key{}", i % 1000);
            black_box(client.get(&key).unwrap());
            i += 1;
        })
    });
}

criterion_group!(benches, bench_get_command);
criterion_main!(benches);
```

### Performance Test Expansion

Based on our initial benchmark comparison with Redis (Valkey), we've established the following additional performance validation criteria:

#### Benchmark Comparison Methodology

```bash
# Run identical benchmark tests against both Redis and Ferrous
# Compare results for key operations

# 1. Basic operations
redis-benchmark -p 6379 -t set,get -n 10000 -q

# 2. Pipeline operations 
redis-benchmark -p 6379 -P 10 -t ping -n 10000 -q

# 3. Concurrent client operations
redis-benchmark -p 6379 -c 50 -t ping -n 10000 -q

# 4. Latency profile
redis-cli -p 6379 --latency-history
```

#### Performance Gap Analysis

| Operation | Redis Baseline | Current % of Redis | Target |
|-----------|----------------|-------------------|--------|
| SET | ~73,500 ops/sec | 57.6% | ≥100% |
| GET | ~72,500 ops/sec | 61.5% | ≥100% |
| Pipeline PING | ~650,000 ops/sec | 0% (not working) | ≥100% |
| Concurrent (50) | ~73,000 ops/sec | 0% (not working) | ≥100% | 
| Latency | ~0.05 ms | 240% (worse) | ≤100% |

#### Performance Regression Testing

For each optimization, we will:

1. **Establish Baseline**: 
   - Document current performance metrics
   - Identify specific bottlenecks through profiling

2. **Implement Optimization**:
   - Make targeted changes to address bottlenecks
   - Keep changes isolated to measure impact

3. **Validate Improvement**:
   - Run identical benchmarks post-optimization
   - Document percentage improvement
   - Ensure no regressions in other areas

4. **Continuous Tracking**:
   - Add benchmark to CI pipeline
   - Alert on performance regressions
   - Track progress toward target metrics

#### Phase-based Performance Milestones

| Phase | SET/GET | Pipeline | Concurrent | Latency |
|-------|---------|----------|------------|---------|
| Current | ~60% | Not working | Not working | ~240% higher |
| Phase 3.5 | ≥70% | Basic support | Basic support | ≤200% higher |
| Phase 4 | ≥85% | ≥70% | ≥70% | ≤150% higher |
| Phase 5 | ≥95% | ≥85% | ≥85% | ≤120% higher |
| Final | ≥100% | ≥100% | ≥100% | ≤100% |

#### Performance Test Infrastructure

We will enhance the test infrastructure to include:

```rust
pub struct PerformanceTestSuite {
    // Test configuration
    operations: Vec<BenchmarkOperation>,
    client_counts: Vec<usize>,
    pipeline_sizes: Vec<usize>,
    
    // Result tracking
    baselines: HashMap<String, PerformanceMetric>,
    results: HashMap<String, PerformanceMetric>,
    
    // Comparison
    pub fn compare_with_baseline(&self) -> PerformanceReport {
        // Calculate percentage of baseline
        // Identify areas requiring optimization
        // Generate detailed report
    }
}
```

This expanded testing will ensure we maintain focus on the performance goals throughout development and can quantify our progress toward Redis parity.

### 7. Stress Tests

```rust
// tests/stress.rs
#[test]
#[ignore] // Run with --ignored flag
fn test_stress_load() {
    let mut client = Client::connect("127.0.0.1:6379").unwrap();
    let start = Instant::now();
    
    // Insert 1M keys
    for i in 0..1_000_000 {
        client.set(format!("key:{}", i), "x".repeat(100)).unwrap();
        if i % 10000 == 0 {
            println!("Inserted {} keys", i);
        }
    }
    
    let duration = start.elapsed();
    println!("Inserted 1M keys in {:?}", duration);
    
    // Verify random samples
    let mut rng = thread_rng();
    for _ in 0..1000 {
        let i = rng.gen_range(0..1_000_000);
        assert_eq!(
            client.get(format!("key:{}", i)).unwrap(),
            Some("x".repeat(100))
        );
    }
}
```

## Test Organization

```
ferrous/
├── src/
│   ├── protocol/
│   │   ├── mod.rs
│   │   └── resp.rs      # Contains unit tests
│   ├── storage/
│   │   ├── mod.rs
│   │   └── string.rs    # Contains unit tests
│   └── ...
├── tests/
│   ├── common/
│   │   ├── mod.rs       # Test utilities
│   │   └── server.rs    # Test server helpers
│   ├── integration/
│   │   ├── basic.rs
│   │   ├── concurrent.rs
│   │   └── transactions.rs
│   ├── compatibility/
│   │   ├── strings.rs
│   │   ├── lists.rs
│   │   └── ...
│   └── stress/
│       ├── load.rs
│       └── memory.rs
├── benches/
│   ├── protocol.rs
│   ├── commands.rs
│   └── throughput.rs
└── fuzz/
    └── fuzz_targets/
        ├── protocol.rs
        └── commands.rs
```

## Testing Infrastructure

### Test Helpers

```rust
// tests/common/mod.rs
pub struct TestServer {
    process: Child,
    port: u16,
}

impl TestServer {
    pub fn new() -> Self {
        let port = find_free_port();
        let process = Command::new("./target/debug/ferrous")
            .args(&["--port", &port.to_string(), "--test-mode"])
            .spawn()
            .expect("Failed to start test server");
        
        // Wait for server to start
        wait_for_port(port);
        
        TestServer { process, port }
    }
    
    pub fn addr(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.process.kill().ok();
    }
}
```

### Test Data Generation

```rust
pub fn generate_test_data(size: usize) -> Vec<(String, String)> {
    (0..size)
        .map(|i| {
            let key = format!("key:{:08}", i);
            let value = format!("value:{}", "x".repeat(i % 100));
            (key, value)
        })
        .collect()
}
```

## Continuous Testing

### CI Pipeline

```yaml
# .github/workflows/test.yml
name: Test Suite

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v2
    
    - name: Unit Tests
      run: cargo test --lib
    
    - name: Integration Tests
      run: cargo test --test '*'
    
    - name: Doc Tests
      run: cargo test --doc
    
    - name: Compatibility Tests
      run: ./scripts/run-compat-tests.sh
    
    - name: Benchmarks
      run: cargo bench --no-run
    
    - name: Coverage
      run: |
        cargo tarpaulin --out Xml
        bash <(curl -s https://codecov.io/bash)
```

### Nightly Tests

```yaml
# Run extended tests nightly
- name: Stress Tests
  run: cargo test --test stress -- --ignored
  
- name: Fuzz Tests
  run: cargo fuzz run protocol -- -max_total_time=3600
  
- name: Memory Leak Check
  run: ./scripts/valgrind-tests.sh
```

## Test Metrics and Goals

### Coverage Goals
- Unit Test Coverage: >90%
- Integration Test Coverage: >80%
- Command Coverage: 100% of implemented commands

### Performance Goals
- Benchmark Suite Runtime: <5 minutes
- Stress Test Completion: <30 minutes
- Memory Usage: Within 10% of Redis

### Reliability Goals
- Zero panics in fuzzing (1M iterations)
- Zero memory leaks (24h stress test)
- Zero data races (thread sanitizer)

## Test Review Process

1. **Pre-commit**: Run unit tests
2. **PR**: Run unit + integration tests
3. **Merge**: Full test suite
4. **Nightly**: Extended tests + fuzzing
5. **Release**: Full validation suite

This comprehensive test plan ensures Ferrous maintains the highest standards of quality, compatibility, and performance throughout development.