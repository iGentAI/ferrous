#!/bin/bash
# Comprehensive Performance Benchmarks for ALL Ferrous Operations
# Tests every Redis-compatible feature implemented in Ferrous for complete baseline validation

echo "=========================================="
echo "FERROUS COMPREHENSIVE ALL-FEATURES PERFORMANCE BENCHMARKS"
echo "Tests: ALL redis-benchmark operations + Stream operations + Advanced features"
echo "=========================================="

# Verify server is running with optimal configuration
echo "Verifying Ferrous server status..."
if ! redis-cli -p 6379 PING > /dev/null 2>&1; then
    echo "❌ Error: Ferrous server not running on port 6379"
    echo ""
    echo "For accurate performance results, start server with log redirection:"
    echo "  ./target/release/ferrous > /dev/null 2>&1 &"
    echo ""
    exit 1
fi

echo "✅ Server detected on port 6379"
echo ""

# Function to run a timed test for custom commands
run_timed_test() {
    local name="$1"
    local command="$2"
    local iterations="$3"
    
    echo "Testing $name..."
    start_time=$(date +%s.%N)
    for i in $(seq 1 $iterations); do
        eval "$command" >/dev/null 2>&1
    done
    end_time=$(date +%s.%N)
    
    # Use Python instead of bc for calculations
    duration=$(python3 -c "print(f'{$end_time - $start_time:.3f}')")
    ops_per_sec=$(python3 -c "print(f'{$iterations / ($end_time - $start_time):.2f}')")
    echo "$name: ${ops_per_sec} ops/sec ($iterations operations in ${duration}s)"
}

echo "=================================================="
echo "SECTION 1: CORE REDIS-BENCHMARK OPERATIONS (50k operations each)"
echo "=================================================="

echo "1.1 Connection and Protocol Tests:"
redis-benchmark -p 6379 -t ping -n 50000 -q

echo ""
echo "1.2 String Operations:"
redis-benchmark -p 6379 -t set,get,incr -n 50000 -q
redis-benchmark -p 6379 -t mset -n 25000 -q  # Lower count for multi-key ops

echo ""
echo "1.3 List Operations:"
redis-benchmark -p 6379 -t lpush,rpush,lpop,rpop,lrange -n 50000 -q

echo ""
echo "1.4 Set Operations:"
redis-benchmark -p 6379 -t sadd,spop -n 50000 -q

echo ""
echo "1.5 Hash Operations:"
redis-benchmark -p 6379 -t hset -n 50000 -q

echo ""
echo "1.6 Sorted Set Operations:"
redis-benchmark -p 6379 -t zadd -n 50000 -q

echo ""
echo "1.7 Advanced Operations:"
# Create some test data for SORT
for i in {1..100}; do
    redis-cli -p 6379 LPUSH sort_test_list $((RANDOM % 1000)) >/dev/null
done
redis-benchmark -p 6379 -t sort -n 1000 -q  # SORT is expensive
redis-benchmark -p 6379 -t randomkey -n 10000 -q

echo ""
echo "=================================================="
echo "SECTION 2: FERROUS STREAM OPERATIONS" 
echo "=================================================="

# Clean up any existing stream data
redis-cli -p 6379 DEL perfstream >/dev/null 2>&1

echo "2.1 Stream Basic Operations:"
run_timed_test "XADD (with auto ID)" "redis-cli -p 6379 XADD perfstream '*' field value" 5000
run_timed_test "XLEN" "redis-cli -p 6379 XLEN perfstream" 10000

# Add some stream entries for range operations
for i in {1..100}; do
    redis-cli -p 6379 XADD perfstream '*' seq $i data "payload_$i" >/dev/null
done

run_timed_test "XRANGE (with COUNT)" "redis-cli -p 6379 XRANGE perfstream - + COUNT 10" 5000
run_timed_test "XREVRANGE (with COUNT)" "redis-cli -p 6379 XREVRANGE perfstream + - COUNT 10" 5000
run_timed_test "XTRIM" "redis-cli -p 6379 XTRIM perfstream MAXLEN 500" 1000

echo ""
echo "2.2 Stream Consumer Group Operations:"
run_timed_test "XGROUP CREATE" "redis-cli -p 6379 XGROUP CREATE perfstream group_${i} 0-0" 100
run_timed_test "XPENDING" "redis-cli -p 6379 XPENDING perfstream group_1" 1000

echo ""
echo "=================================================="
echo "SECTION 3: FERROUS ADVANCED FEATURES"
echo "=================================================="

echo "3.1 Database Management:"
run_timed_test "SELECT" "redis-cli -p 6379 SELECT 1" 10000
run_timed_test "DBSIZE" "redis-cli -p 6379 DBSIZE" 10000
redis-cli -p 6379 SELECT 0 >/dev/null  # Return to DB 0

echo ""
echo "3.2 Key Management:"
run_timed_test "EXISTS" "redis-cli -p 6379 EXISTS perfstream" 10000
run_timed_test "TYPE" "redis-cli -p 6379 TYPE perfstream" 10000
run_timed_test "TTL" "redis-cli -p 6379 TTL perfstream" 10000
run_timed_test "EXPIRE" "redis-cli -p 6379 SET temp_key temp_value && redis-cli -p 6379 EXPIRE temp_key 300" 5000

echo ""
echo "3.3 String Advanced Operations:"
run_timed_test "SETNX" "redis-cli -p 6379 SETNX nx_test_${i} value" 5000
run_timed_test "SETEX" "redis-cli -p 6379 SETEX ex_test_${i} 300 value" 5000
run_timed_test "APPEND" "redis-cli -p 6379 SET append_test initial && redis-cli -p 6379 APPEND append_test _appended" 5000
run_timed_test "STRLEN" "redis-cli -p 6379 STRLEN append_test" 10000

echo ""
echo "3.4 Sorted Set Advanced Operations:"
# Create sorted set test data
for i in {1..100}; do
    redis-cli -p 6379 ZADD perf_zset $((RANDOM % 1000)) member_$i >/dev/null
done

run_timed_test "ZSCORE" "redis-cli -p 6379 ZSCORE perf_zset member_1" 10000
run_timed_test "ZRANGE" "redis-cli -p 6379 ZRANGE perf_zset 0 10" 5000
run_timed_test "ZREVRANGE" "redis-cli -p 6379 ZREVRANGE perf_zset 0 10" 5000
run_timed_test "ZRANGEBYSCORE" "redis-cli -p 6379 ZRANGEBYSCORE perf_zset 1 500" 5000
run_timed_test "ZCOUNT" "redis-cli -p 6379 ZCOUNT perf_zset 1 500" 10000
run_timed_test "ZRANK" "redis-cli -p 6379 ZRANK perf_zset member_1" 10000

echo ""
echo "3.5 Hash Advanced Operations:"
# Create hash test data
for i in {1..50}; do
    redis-cli -p 6379 HSET perf_hash field_$i value_$i >/dev/null
done

run_timed_test "HGET" "redis-cli -p 6379 HGET perf_hash field_1" 10000
run_timed_test "HMGET" "redis-cli -p 6379 HMGET perf_hash field_1 field_2 field_3" 5000
run_timed_test "HGETALL" "redis-cli -p 6379 HGETALL perf_hash" 5000
run_timed_test "HKEYS" "redis-cli -p 6379 HKEYS perf_hash" 10000
run_timed_test "HVALS" "redis-cli -p 6379 HVALS perf_hash" 10000
run_timed_test "HEXISTS" "redis-cli -p 6379 HEXISTS perf_hash field_1" 10000
run_timed_test "HLEN" "redis-cli -p 6379 HLEN perf_hash" 10000

echo ""
echo "3.6 Set Advanced Operations:"
# Create set test data
for i in {1..100}; do
    redis-cli -p 6379 SADD perf_set member_$i >/dev/null
done

run_timed_test "SMEMBERS" "redis-cli -p 6379 SMEMBERS perf_set" 5000
run_timed_test "SISMEMBER" "redis-cli -p 6379 SISMEMBER perf_set member_1" 10000
run_timed_test "SCARD" "redis-cli -p 6379 SCARD perf_set" 10000
run_timed_test "SRANDMEMBER" "redis-cli -p 6379 SRANDMEMBER perf_set 5" 10000

echo ""
echo "3.7 List Advanced Operations:"
# Create list test data
for i in {1..100}; do
    redis-cli -p 6379 LPUSH perf_list item_$i >/dev/null
done

run_timed_test "LINDEX" "redis-cli -p 6379 LINDEX perf_list 5" 10000
run_timed_test "LLEN" "redis-cli -p 6379 LLEN perf_list" 10000
run_timed_test "LRANGE" "redis-cli -p 6379 LRANGE perf_list 0 10" 5000

echo ""
echo "=================================================="
echo "SECTION 4: FERROUS BLOCKING OPERATIONS"
echo "=================================================="

# Test blocking operations with immediate data
redis-cli -p 6379 DEL block_test_queue >/dev/null
redis-cli -p 6379 LPUSH block_test_queue item1 item2 item3 >/dev/null

run_timed_test "BLPOP (immediate)" "redis-cli -p 6379 LPUSH block_test_queue item && redis-cli -p 6379 BLPOP block_test_queue 1" 1000
run_timed_test "BRPOP (immediate)" "redis-cli -p 6379 RPUSH block_test_queue item && redis-cli -p 6379 BRPOP block_test_queue 1" 1000

echo ""
echo "=================================================="
echo "SECTION 5: FERROUS TRANSACTION OPERATIONS"
echo "=================================================="

run_timed_test "MULTI/EXEC" "redis-cli -p 6379 MULTI && redis-cli -p 6379 SET tx_key value && redis-cli -p 6379 EXEC" 2500

echo ""
echo "3.8 WATCH/Transaction Integration:"
# Test WATCH mechanism performance (should have minimal overhead now)
run_timed_test "WATCH (baseline establishment)" "redis-cli -p 6379 WATCH watch_perf_key" 5000

echo ""
echo "=================================================="
echo "SECTION 6: FERROUS PERSISTENCE OPERATIONS"
echo "=================================================="

run_timed_test "BGSAVE" "redis-cli -p 6379 BGSAVE" 5

echo ""
echo "=================================================="
echo "SECTION 7: FERROUS ADMINISTRATIVE OPERATIONS"
echo "=================================================="

run_timed_test "INFO" "redis-cli -p 6379 INFO" 1000
run_timed_test "CONFIG GET" "redis-cli -p 6379 CONFIG GET save" 5000

echo ""
echo "=================================================="
echo "SECTION 8: SCAN OPERATIONS (SAFE ITERATION)"
echo "=================================================="

run_timed_test "SCAN" "redis-cli -p 6379 SCAN 0 MATCH perf* COUNT 10" 5000
run_timed_test "HSCAN" "redis-cli -p 6379 HSCAN perf_hash 0 MATCH field* COUNT 10" 5000
run_timed_test "SSCAN" "redis-cli -p 6379 SSCAN perf_set 0 COUNT 10" 5000
run_timed_test "ZSCAN" "redis-cli -p 6379 ZSCAN perf_zset 0 COUNT 10" 5000

echo ""
echo "=================================================="
echo "SECTION 9: PIPELINE PERFORMANCE (HIGH-THROUGHPUT)"
echo "=================================================="

echo "9.1 Massive Pipeline Tests:"
redis-benchmark -p 6379 -t ping -P 16 -n 50000 -q
redis-benchmark -p 6379 -t set,get -P 10 -n 50000 -q

echo ""
echo "=================================================="
echo "SECTION 10: CONCURRENT CLIENT SCALING"
echo "=================================================="

echo "10.1 Multi-Client Performance:"
redis-benchmark -p 6379 -t ping,set,get -c 50 -n 50000 -q
redis-benchmark -p 6379 -t incr,lpush,sadd -c 100 -n 50000 -q

echo ""
echo "=================================================="
echo "SECTION 11: MIXED REALISTIC WORKLOADS"
echo "=================================================="

echo "11.1 Realistic Application Patterns:"
echo "Simulating typical web application workload (caching + sessions + queues + events)..."

start_time=$(date +%s.%N)
for i in {1..1000}; do
    # Web session simulation
    redis-cli -p 6379 SET "session:user_$i" "session_data_$i" >/dev/null
    redis-cli -p 6379 EXPIRE "session:user_$i" 3600 >/dev/null
    
    # User profile (hash)
    redis-cli -p 6379 HSET "profile:user_$i" name "User$i" email "user$i@test.com" >/dev/null
    
    # Activity queue (list)
    redis-cli -p 6379 LPUSH "queue:activities" "user_$i:action" >/dev/null
    
    # Leaderboard (sorted set)
    redis-cli -p 6379 ZADD "leaderboard" $((RANDOM % 10000)) "User$i" >/dev/null
    
    # Event stream
    redis-cli -p 6379 XADD "events:user_actions" "*" user "User$i" action "login" timestamp $i >/dev/null
done
end_time=$(date +%s.%N)

# Use Python instead of bc for calculations
duration=$(python3 -c "print(f'{$end_time - $start_time:.3f}')")
total_ops=5000  # 1000 iterations * 5 operations each
mixed_ops_per_sec=$(python3 -c "print(f'{$total_ops / ($end_time - $start_time):.2f}')")

echo "Mixed realistic workload: ${mixed_ops_per_sec} ops/sec (${total_ops} operations)"

echo ""
echo "=================================================="
echo "SECTION 12: NEWLY IMPLEMENTED FEATURES PERFORMANCE"
echo "=================================================="

echo "12.1 Recently Added Commands:"
run_timed_test "SETNX" "redis-cli -p 6379 SETNX new_key_${i} value" 5000
run_timed_test "SETEX" "redis-cli -p 6379 SETEX temp_key_${i} 300 value" 5000
run_timed_test "PSETEX" "redis-cli -p 6379 PSETEX ms_key_${i} 30000 value" 5000
run_timed_test "DECRBY" "redis-cli -p 6379 SET decr_test 100 && redis-cli -p 6379 DECRBY decr_test 5" 5000
run_timed_test "RENAMENX" "redis-cli -p 6379 SET rename_src value && redis-cli -p 6379 RENAMENX rename_src rename_dst_${i}" 2500

echo ""
echo "12.2 Database Management Commands:"
run_timed_test "SELECT" "redis-cli -p 6379 SELECT 2" 10000
run_timed_test "FLUSHDB" "redis-cli -p 6379 FLUSHDB" 100  # Expensive operation
redis-cli -p 6379 SELECT 0 >/dev/null  # Return to DB 0

echo ""
echo "=================================================="
echo "SECTION 13: LATENCY AND PERCENTILE ANALYSIS"
echo "=================================================="

echo "13.1 Detailed Latency Analysis (smaller sample for precision):"
redis-benchmark -p 6379 -t ping,set,get,incr -n 10000 --precision 3

echo ""
echo "=================================================="
echo "PERFORMANCE BASELINE SUMMARY"
echo "=================================================="

echo ""
echo "COMPREHENSIVE TEST COVERAGE INCLUDES:"
echo "✅ All Standard Redis-benchmark Operations (ping, set, get, incr, lists, sets, hashes, sorted sets)"
echo "✅ Complete Stream Operations (XADD, XLEN, XRANGE, XREVRANGE, XTRIM, Consumer Groups)"
echo "✅ Advanced Data Structure Operations (ZSCORE, ZRANGE, HGETALL, SMEMBERS, etc.)"
echo "✅ Newly Implemented Commands (SETNX, SETEX, PSETEX, DECRBY, RENAMENX)"
echo "✅ Database Management (SELECT, DBSIZE, FLUSHDB)"
echo "✅ Blocking Operations (BLPOP, BRPOP)"
echo "✅ Transaction System (MULTI/EXEC, WATCH)"
echo "✅ Persistence Operations (BGSAVE)" 
echo "✅ Scan Operations (SCAN, HSCAN, SSCAN, ZSCAN)"
echo "✅ Pipeline Performance (high-throughput testing)"
echo "✅ Concurrent Client Scaling (50-100 clients)"
echo "✅ Mixed Realistic Workloads (web application simulation)"
echo "✅ Latency Percentile Analysis"
echo ""
echo "Expected Performance Targets (with conditional WATCH optimization):"
echo "- Core Operations: >80k ops/sec (PING, SET, GET, INCR)"
echo "- List/Set/Hash: >70k ops/sec" 
echo "- Sorted Set: >60k ops/sec"
echo "- Stream Operations: >10k ops/sec (XADD), >30k ops/sec (reads)"
echo "- Advanced Features: >5k ops/sec"
echo "- Mixed Workloads: >3k ops/sec"
echo "=================================================="

# Cleanup test data
echo "Cleaning up performance test data..."
redis-cli -p 6379 FLUSHALL >/dev/null 2>&1

echo "✅ Comprehensive performance testing completed!"
echo "✅ All data cleaned up"