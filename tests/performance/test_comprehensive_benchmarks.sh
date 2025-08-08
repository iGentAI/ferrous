#!/bin/bash
# Comprehensive Performance Benchmarks for ALL Ferrous Operations

echo "=========================================="
echo "FERROUS COMPREHENSIVE PERFORMANCE BENCHMARKS"
echo "=========================================="

# Function to run a timed test
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
    ops_per_sec=$(python3 -c "print(f'{$iterations / ($end_time - $start_time):.1f}')")
    echo "$name: ${ops_per_sec} ops/sec ($iterations operations in ${duration}s)"
}

# Verify server is running with optimal configuration
echo "1. CORE OPERATIONS (redis-benchmark)"
echo "======================================"

echo "Basic PING/SET/GET Operations:"
redis-benchmark -p 6379 -t ping,set,get,incr -n 50000 -q

echo ""
echo "List Operations:"  
redis-benchmark -p 6379 -t lpush,lpop,rpush,rpop -n 50000 -q

echo ""
echo "Set Operations:"
redis-benchmark -p 6379 -t sadd,spop -n 50000 -q

echo ""
echo "Hash Operations:"
redis-benchmark -p 6379 -t hset -n 50000 -q

echo ""
echo "Sorted Set Operations:"
redis-benchmark -p 6379 -t zadd -n 50000 -q

echo ""
echo "Pipeline Performance:"
redis-benchmark -p 6379 -t ping -n 50000 -P 16 -q

echo ""
echo "Concurrent Client Performance:"
redis-benchmark -p 6379 -t set,get -n 50000 -c 50 -q

echo ""
echo "2. ADVANCED DATA STRUCTURES"
echo "======================================"

# Test sorted set range operations
echo "SORTED SET RANGE OPERATIONS:"
redis-cli -p 6379 DEL perfsortedset >/dev/null
for i in {1..100}; do
    redis-cli -p 6379 ZADD perfsortedset $i member$i >/dev/null
done

run_timed_test "ZRANGE" "redis-cli -p 6379 ZRANGE perfsortedset 0 10" 1000
run_timed_test "ZREVRANGE" "redis-cli -p 6379 ZREVRANGE perfsortedset 0 10" 1000
run_timed_test "ZRANGEBYSCORE" "redis-cli -p 6379 ZRANGEBYSCORE perfsortedset 1 50" 1000

# Test hash operations  
echo ""
echo "HASH OPERATIONS:"
redis-cli -p 6379 DEL perfhash >/dev/null
run_timed_test "HMSET" "redis-cli -p 6379 HMSET perfhash field1 value1 field2 value2" 1000
run_timed_test "HGETALL" "redis-cli -p 6379 HGETALL perfhash" 1000
run_timed_test "HMGET" "redis-cli -p 6379 HMGET perfhash field1 field2" 1000

echo ""
echo "3. STREAM OPERATIONS"
echo "======================================"

# Test stream operations performance
redis-cli -p 6379 DEL perfstream >/dev/null

run_timed_test "XADD" "redis-cli -p 6379 XADD perfstream '*' field value" 1000
run_timed_test "XLEN" "redis-cli -p 6379 XLEN perfstream" 1000  
run_timed_test "XRANGE (with COUNT)" "redis-cli -p 6379 XRANGE perfstream - + COUNT 10" 500
run_timed_test "XREVRANGE (with COUNT)" "redis-cli -p 6379 XREVRANGE perfstream + - COUNT 10" 500
run_timed_test "XTRIM" "redis-cli -p 6379 XTRIM perfstream MAXLEN 500" 100

# Consumer group operations
run_timed_test "XGROUP CREATE" "redis-cli -p 6379 XGROUP CREATE perfstream group1 0-0" 100

echo ""
echo "4. BLOCKING OPERATIONS"  
echo "======================================"

# Test blocking operations (with timeouts)
redis-cli -p 6379 DEL blockqueue >/dev/null
redis-cli -p 6379 LPUSH blockqueue item1 item2 item3 >/dev/null

run_timed_test "BLPOP (immediate)" "redis-cli -p 6379 LPUSH blockqueue item && redis-cli -p 6379 BLPOP blockqueue 1" 100
run_timed_test "BRPOP (immediate)" "redis-cli -p 6379 RPUSH blockqueue item && redis-cli -p 6379 BRPOP blockqueue 1" 100

echo ""
echo "5. PUB/SUB OPERATIONS"
echo "======================================"

# Test pub/sub performance 
run_timed_test "PUBLISH" "redis-cli -p 6379 PUBLISH testchannel 'test message'" 1000

echo ""
echo "6. PERSISTENCE OPERATIONS"
echo "======================================"

# Test persistence performance
run_timed_test "BGSAVE" "redis-cli -p 6379 BGSAVE" 5
sleep 2 # Allow background saves to complete

echo ""
echo "7. TRANSACTION OPERATIONS"
echo "======================================"

# Test transaction performance
run_timed_test "MULTI/EXEC" "redis-cli -p 6379 MULTI && redis-cli -p 6379 SET txkey value && redis-cli -p 6379 EXEC" 500

echo ""
echo "8. LUA SCRIPTING OPERATIONS"
echo "======================================"

# Test Lua scripting performance
SCRIPT_LOAD_OUTPUT=$(redis-cli -p 6379 SCRIPT LOAD "return redis.call('GET', KEYS[1])" 2>&1)
if [ $? -ne 0 ]; then
    echo "❌ CRITICAL ERROR: Lua SCRIPT LOAD failed!"
    echo "Error output: $SCRIPT_LOAD_OUTPUT"
    exit 1
fi

# Remove quotes from redis-cli output (it returns JSON formatted string)
SCRIPT_SHA=$(echo "$SCRIPT_LOAD_OUTPUT" | tail -n1 | tr -d '"')
if [ -z "$SCRIPT_SHA" ] || [[ "$SCRIPT_SHA" == *"ERR"* ]]; then
    echo "❌ CRITICAL ERROR: Invalid SCRIPT SHA returned: '$SCRIPT_SHA'"
    exit 1
fi

echo "✅ Lua SCRIPT LOAD successful: SHA=$SCRIPT_SHA"

# Set up test key for EVALSHA test
redis-cli -p 6379 SET testkey "testvalue" >/dev/null
if [ $? -ne 0 ]; then
    echo "❌ ERROR: Failed to set up test key for Lua tests"
    exit 1
fi

# Run Lua performance tests
run_timed_test "EVAL" "redis-cli -p 6379 EVAL 'return redis.call(\"PING\")' 0" 500
run_timed_test "EVALSHA" "redis-cli -p 6379 EVALSHA $SCRIPT_SHA 1 testkey" 500

# Validate that EVALSHA actually worked
EVALSHA_RESULT=$(redis-cli -p 6379 EVALSHA $SCRIPT_SHA 1 testkey 2>&1)
if [ "$EVALSHA_RESULT" != "testvalue" ]; then
    echo "❌ ERROR: EVALSHA did not return expected value. Got: '$EVALSHA_RESULT'"
    exit 1
fi

echo "✅ Lua scripting tests completed successfully"

echo ""
echo "9. KEY MANAGEMENT OPERATIONS"
echo "======================================"

# Test key management performance  
redis-cli -p 6379 SET perfkey perfvalue >/dev/null
run_timed_test "EXISTS" "redis-cli -p 6379 EXISTS perfkey" 1000
run_timed_test "TTL" "redis-cli -p 6379 TTL perfkey" 1000
run_timed_test "TYPE" "redis-cli -p 6379 TYPE perfkey" 1000
run_timed_test "DEL" "redis-cli -p 6379 SET tempkey value && redis-cli -p 6379 DEL tempkey" 1000

echo ""
echo "10. SCAN OPERATIONS"
echo "======================================"

# Test SCAN family performance
for i in {1..100}; do
    redis-cli -p 6379 SET scankey$i value$i >/dev/null
    redis-cli -p 6379 HSET scanhash field$i value$i >/dev/null
    redis-cli -p 6379 SADD scanset member$i >/dev/null
    redis-cli -p 6379 ZADD scanzsert $i member$i >/dev/null
done

run_timed_test "SCAN" "redis-cli -p 6379 SCAN 0 MATCH scankey* COUNT 10" 500
run_timed_test "HSCAN" "redis-cli -p 6379 HSCAN scanhash 0 MATCH field* COUNT 10" 500
run_timed_test "SSCAN" "redis-cli -p 6379 SSCAN scanset 0 COUNT 10" 500
run_timed_test "ZSCAN" "redis-cli -p 6379 ZSCAN scanzsert 0 COUNT 10" 500

echo ""
echo "11. ADMINISTRATIVE OPERATIONS"
echo "======================================"

# Test admin operations performance
run_timed_test "INFO" "redis-cli -p 6379 INFO" 500
run_timed_test "CONFIG GET" "redis-cli -p 6379 CONFIG GET save" 500

echo ""
echo "12. MIXED WORKLOAD TESTING"  
echo "======================================"

# Test realistic mixed workload
echo "Running mixed workload test (1000 operations)..."
start_time=$(date +%s.%N)

for i in {1..200}; do
    # Simulate realistic application patterns
    redis-cli -p 6379 SET "user:$i:session" "session$i" >/dev/null
    redis-cli -p 6379 HSET "user:$i:profile" name "User$i" email "user$i@test.com" >/dev/null  
    redis-cli -p 6379 LPUSH "user:$i:logs" "action$i" >/dev/null
    redis-cli -p 6379 ZADD "leaderboard" $i "User$i" >/dev/null
    redis-cli -p 6379 XADD "events:$((i % 10))" "*" user "User$i" action "login" >/dev/null
done

end_time=$(date +%s.%N)
duration=$(python3 -c "print(f'{$end_time - $start_time:.3f}')")
total_ops=1000  # 200 * 5 operations
mixed_ops_per_sec=$(python3 -c "print(f'{$total_ops / ($end_time - $start_time):.1f}')")

echo "Mixed workload: ${mixed_ops_per_sec} ops/sec (${total_ops} operations in ${duration}s)"

echo ""
echo "=========================================="
echo "COMPREHENSIVE BENCHMARK RESULTS SUMMARY"
echo "=========================================="
echo ""
echo "This test suite covers:"
echo "✅ Core Operations (PING, SET, GET, INCR)"
echo "✅ All Data Structures (Lists, Sets, Hashes, Sorted Sets, Streams)" 
echo "✅ Advanced Features (Pub/Sub, Transactions, Lua Scripting)"
echo "✅ Persistence Operations (BGSAVE)"
echo "✅ Blocking Operations (BLPOP/BRPOP)"
echo "✅ Scan Operations (SCAN, HSCAN, SSCAN, ZSCAN)"
echo "✅ Administrative Commands (INFO, CONFIG)"
echo "✅ Mixed Workload Simulation"
echo ""
echo "PERFORMANCE TARGETS COMPARISON:"
echo "Expected with log redirection (production mode):"
echo "- Core Ops: >75k ops/sec (PING, SET, GET)" 
echo "- List Ops: >70k ops/sec (LPUSH, LPOP)"
echo "- Complex Ops: >30k ops/sec (HSET, ZADD, XADD)"
echo "- Range/Scan Ops: >10k ops/sec"
echo "- Mixed Workload: >5k ops/sec"
echo ""
echo "Note: Performance is 2-3x higher with log redirection"
echo "      Run: ./target/release/ferrous > /dev/null 2>&1 &"
echo "=========================================="

# Cleanup test data
redis-cli -p 6379 FLUSHALL >/dev/null 2>&1

echo "✅ Performance testing completed with cleanup"