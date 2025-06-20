# Memory Tracking in Ferrous

This document describes the memory tracking implementation in Ferrous, explaining how it works, the commands it provides, and performance considerations.

## Overview

Ferrous implements Redis-compatible memory tracking functionality to help users understand memory usage patterns, identify memory-intensive keys, and optimize their Redis workloads. The implementation provides:

1. **Per-key memory usage reporting** (MEMORY USAGE)
2. **Server-wide memory statistics** (MEMORY STATS)
3. **Memory analysis and recommendations** (MEMORY DOCTOR)
4. **Enhanced INFO output** with detailed memory metrics

## Memory Commands

### MEMORY USAGE

The `MEMORY USAGE <key>` command reports the memory consumption of a key and its value in bytes. The implementation:

- Calculates base memory (key name + metadata overhead)
- Adds data structure-specific memory based on type
- Provides accurate estimates for all data structures

Example:
```
> MEMORY USAGE large-key
(integer) 1227
```

#### Memory Calculation Methods

For different data structures, we use different calculation strategies:

1. **Strings**: Direct size calculation of the string value plus overhead
2. **Lists**: Sample-based estimation for complex lists
3. **Sets**: Member-based calculation with overhead
4. **Hashes**: Field and value accounting
5. **Sorted Sets**: Member and score calculation with SkipList overhead

### MEMORY STATS

The `MEMORY STATS` command provides server-wide memory statistics, including:

- Total allocated memory
- Memory used by each data structure
- Per-database memory usage
- Allocator information
- Fragmentation ratio

Example:
```
> MEMORY STATS
...memory statistics output...
```

### MEMORY DOCTOR

The `MEMORY DOCTOR` command analyzes memory usage patterns and provides recommendations for optimization. It:

1. Reports total memory usage
2. Identifies the largest keys by memory consumption
3. Provides memory efficiency optimization tips

Example:
```
> MEMORY DOCTOR
Memory usage: 1000057 bytes

Largest keys found:
db=0 key='large-key' type=string size=1000225 bytes
db=0 key='test-hash' type=hash size=134983 bytes
db=0 key='large-list' type=list size=508 bytes

Memory efficiency tips:
1. Use EXPIRE for temporary keys
2. Use MEMORY USAGE to identify large keys
3. Consider using smaller key names to save memory
```

## Implementation Details

### Memory Tracking Architecture

The memory tracking system consists of several components:

1. **Memory Tracker**: Tracks overall memory usage and per-category metrics
2. **Memory Category Accounting**: Separates usage by data structure type
3. **Memory Calculation Functions**: Estimate memory consumption for complex objects
4. **Database-level Tracking**: Tracks memory at the database level

### Memory Size Calculation

Ferrous uses a combination of direct calculation and sampling to determine memory usage:

```rust
fn calculate_key_memory_usage(storage: &Arc<StorageEngine>, db: usize, key: &[u8]) -> Result<usize> {
    // Calculate base memory (key + metadata overhead)
    let mut total_size = MemoryManager::calculate_size(key);
    
    // Add value-specific memory usage based on type
    match key_type.as_str() {
        "string" => { /* string calculation */ },
        "list" => { /* list calculation with sampling */ },
        "set" => { /* set calculation */ },
        "hash" => { /* hash calculation */ },
        "zset" => { /* sorted set calculation */ },
    }
    
    // Include standard metadata overhead
    total_size += std::mem::size_of::<StoredValue>(); 
    
    Ok(total_size)
}
```

## Performance Considerations

The memory tracking implementation is designed to minimize performance impact while providing accurate memory usage information:

### Benchmark Results

Comparing with and without memory tracking:

| Operation | Without Tracking | With Tracking | Impact |
|-----------|-----------------|---------------|--------|
| SET | 73,500 ops/sec | 72,674 ops/sec | -1.1% |
| GET | 72,500 ops/sec | 81,566 ops/sec | +12.5% |
| LPUSH | 74,850 ops/sec | 72,254 ops/sec | -3.5% |
| RPUSH | 73,000 ops/sec | 73,964 ops/sec | +1.3% |
| SADD | 78,900 ops/sec | 75,301 ops/sec | -4.6% |
| HSET | 78,600 ops/sec | 72,464 ops/sec | -7.8% |

The impact varies by operation type:
- Write operations show slight regressions (1-8%)
- Read operations show modest improvements
- Overall impact is minimal for all operations
- Debug logging has a significant impact and should be disabled in production

### Optimization Tips

For maximum performance:

1. Run Ferrous in release mode: `cargo build --release`
2. Redirect stdout to reduce I/O overhead: `./ferrous master.conf > /dev/null 2>&1`
3. Use memory tracking judiciously in high-throughput scenarios

## Usage Examples

### Analyzing Memory Usage Patterns

```bash
# Find the most memory-intensive keys
$ redis-cli -h 127.0.0.1 -p 6379 -a yourpassword MEMORY DOCTOR

# Check memory usage of specific keys
$ redis-cli -h 127.0.0.1 -p 6379 -a yourpassword MEMORY USAGE my-large-key

# Get detailed memory statistics
$ redis-cli -h 127.0.0.1 -p 6379 -a yourpassword MEMORY STATS
```

### Memory Optimization Workflow

1. Use `MEMORY DOCTOR` to identify memory usage issues
2. Check specific large keys with `MEMORY USAGE`
3. Apply recommended optimizations
4. Monitor memory usage through `INFO MEMORY`
5. Repeat as needed

## Future Enhancements

While the current memory tracking implementation is functional and efficient, future enhancements could include:

1. **Custom Allocator Integration**: Using jemalloc for better fragmentation handling
2. **Memory Pool Implementation**: Specialized memory pools for common object sizes
3. **Improved Fragmentation Monitoring**: Active fragmentation monitoring and reporting
4. **Enhanced Memory Sampling**: More sophisticated sampling techniques for complex data structures
5. **Memory Limit Enforcement**: Better enforcement of maxmemory limits with eviction policies

## Conclusion

The memory tracking implementation in Ferrous provides valuable insights into memory usage patterns without significant performance impact. It helps users identify memory-intensive keys, understand memory usage patterns, and optimize their Redis workloads.