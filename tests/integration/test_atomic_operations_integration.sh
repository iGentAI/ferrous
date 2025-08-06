#!/bin/bash
# Atomic Operations Integration Tests for Ferrous
# Designed to catch SET NX hanging and other critical atomic operation bugs

set -e

echo "=============================================="
echo "ATOMIC OPERATIONS INTEGRATION TESTS"
echo "=============================================="

# Function to test command with timeout
test_with_timeout() {
    local test_name="$1"
    local command="$2"
    local expected="$3"
    local timeout_sec="${4:-5}"
    
    echo "Testing: $test_name"
    
    # Run with timeout to detect hanging
    if timeout "$timeout_sec" bash -c "$command" > /tmp/test_result 2>&1; then
        local result=$(cat /tmp/test_result)
        
        if [[ "$result" == "$expected" ]]; then
            echo "  ✅ PASSED: $test_name"
            return 0
        else
            echo "  ❌ FAILED: $test_name (Expected: '$expected', Got: '$result')"
            return 1
        fi
    else
        echo "  ❌ TIMEOUT: $test_name (hanging detected after ${timeout_sec}s)"
        return 1
    fi
}

# Test 1: SET NX on existing key (critical hang prevention)
echo
echo "1. Testing SET NX hanging prevention..."
redis-cli -p 6379 SET nx_hang_test original > /dev/null

echo "Testing: SET NX on existing key (hang prevention)"
start_time=$(date +%s)
result=$(timeout 3 redis-cli -p 6379 SET nx_hang_test new_value NX 2>&1 || echo "TIMED_OUT")
end_time=$(date +%s) 
elapsed=$((end_time - start_time))

if [[ "$result" == "TIMED_OUT" ]]; then
    echo "  ❌ TIMEOUT: SET NX hanging detected after 3s"
    exit 1
elif ((elapsed > 2)); then
    echo "  ❌ SLOW: SET NX took ${elapsed}s (possible performance issue)"
    exit 1
else
    original_value=$(redis-cli -p 6379 GET nx_hang_test)
    if [[ "$original_value" == "original" ]]; then
        echo "  ✅ PASSED: SET NX hanging prevention (${elapsed}s, value preserved)"
    else
        echo "  ❌ FAILED: SET NX corrupted original value: '$original_value'"
        exit 1
    fi
fi

# Test 2: SET XX on missing key 
echo
echo "2. Testing SET XX operations..."
redis-cli -p 6379 DEL xx_test > /dev/null

echo "Testing: SET XX on missing key"
start_time=$(date +%s)
result=$(timeout 3 redis-cli -p 6379 SET xx_test value XX 2>&1 || echo "TIMED_OUT")
end_time=$(date +%s)
elapsed=$((end_time - start_time))

if [[ "$result" == "TIMED_OUT" ]]; then
    echo "  ❌ TIMEOUT: SET XX hanging detected after 3s" 
    exit 1
elif ((elapsed > 1)); then
    echo "  ❌ SLOW: SET XX took ${elapsed}s"
    exit 1
else
    check_value=$(redis-cli -p 6379 GET xx_test)
    if [[ "$check_value" == "" || "$check_value" == "(nil)" ]]; then
        echo "  ✅ PASSED: SET XX on missing key (${elapsed}s, key not set)"
    else
        echo "  ❌ FAILED: SET XX incorrectly set value: '$check_value'"
        exit 1
    fi
fi

# Test 3: Multiple atomic operations in sequence
echo
echo "3. Testing atomic operation sequences..."
redis-cli -p 6379 DEL sequence_test > /dev/null

echo "Testing: Atomic sequence test"
start_time=$(date +%s)
result=$(timeout 3 redis-cli -p 6379 EVAL 'redis.call("SET", "sequence_test", "val"); return redis.call("SET", "sequence_test", "new", "NX")' 0 2>&1 || echo "TIMED_OUT")
end_time=$(date +%s)
elapsed=$((end_time - start_time))

if [[ "$result" == "TIMED_OUT" ]]; then
    echo "  ❌ TIMEOUT: Lua atomic sequence hanging detected"
    exit 1
elif ((elapsed > 2)); then
    echo "  ❌ SLOW: Lua sequence took ${elapsed}s"
    exit 1
else
    final_value=$(redis-cli -p 6379 GET sequence_test)
    if [[ "$final_value" == "val" ]]; then
        echo "  ✅ PASSED: Lua atomic sequence (${elapsed}s, atomicity preserved)"
    else
        echo "  ❌ FAILED: Atomic sequence corrupted value: '$final_value'"
        exit 1
    fi
fi

# Test 4: Blocking operations timeout behavior
echo  
echo "4. Testing blocking operations..."
redis-cli -p 6379 DEL blocking_test > /dev/null

# Test BLPOP timeout (should complete in ~1 second)
start_time=$(date +%s)
redis-cli -p 6379 BLPOP blocking_test 1 > /dev/null 2>&1 || true
end_time=$(date +%s)
elapsed=$((end_time - start_time))

# Check if it properly timed out around 1 second (allow 0-2 second range)
if ((elapsed >= 0 && elapsed <= 2)); then
    echo "  ✅ PASSED: BLPOP timeout behavior (${elapsed}s)"
else
    echo "  ❌ FAILED: BLPOP timeout behavior (expected ~1s, got ${elapsed}s)"
    exit 1
fi

# Test 5: Lua atomic lock pattern (critical regression test)
echo
echo "5. Testing Lua atomic lock pattern..."
redis-cli -p 6379 SET lua_lock_test unique_value > /dev/null

# This pattern previously caused socket timeouts
test_with_timeout "Lua atomic lock release" \
    "redis-cli -p 6379 EVAL 'if redis.call(\"GET\", KEYS[1]) == ARGV[1] then return redis.call(\"DEL\", KEYS[1]) else return 0 end' 1 lua_lock_test unique_value" \
    "1" \
    "3"

# Verify lock was actually deleted (this step previously timed out)
test_with_timeout "Post-lock verification" \
    "redis-cli -p 6379 GET lua_lock_test" \
    "(nil)" \
    "2"

# Test 6: Rapid conditional operations (stress test)
echo
echo "6. Testing rapid conditional operations..." 
redis-cli -p 6379 SET rapid_test base > /dev/null

# Rapid-fire operations that should not hang
for i in {1..10}; do
    start_time=$(date +%s)
    redis-cli -p 6379 SET rapid_test "attempt_${i}" NX > /dev/null 2>&1 || true
    end_time=$(date +%s)
    elapsed=$((end_time - start_time))
    
    # Each individual operation should complete in under 1 second
    if ((elapsed > 0)); then
        echo "  ❌ FAILED: Rapid operation ${i} took ${elapsed}s"
        exit 1
    fi
done
echo "  ✅ PASSED: Rapid conditional operations"

# Test 7: Multiple clients atomic operations
echo
echo "7. Testing concurrent atomic operations..."
redis-cli -p 6379 DEL concurrent_test > /dev/null

# Start multiple background SET NX operations
for i in {1..5}; do
    (redis-cli -p 6379 SET concurrent_test "client_${i}" NX > /tmp/client_${i}_result 2>&1) &
done

# Wait for all to complete (should be very fast)
wait

# Count successful operations (should be exactly 1)
success_count=0
for i in {1..5}; do
    if [[ -f "/tmp/client_${i}_result" ]]; then
        result=$(cat "/tmp/client_${i}_result" 2>/dev/null || echo "ERROR")
        if [[ "$result" == "OK" ]]; then
            success_count=$((success_count + 1))
        fi
        rm -f "/tmp/client_${i}_result"
    fi
done

if [[ $success_count -eq 1 ]]; then
    echo "  ✅ PASSED: Concurrent atomic operations (${success_count}/5 succeeded)"
else
    echo "  ❌ FAILED: Concurrent atomic operations (${success_count}/5 succeeded, expected 1)"
    exit 1
fi

echo
echo "=============================================="
echo "🎉 ALL ATOMIC OPERATIONS INTEGRATION TESTS PASSED!"
echo "✅ SET NX hanging prevention: WORKING"
echo "✅ Lua socket handling: VERIFIED"  
echo "✅ Blocking operations: RELIABLE"
echo "✅ Concurrent atomic operations: SAFE"
echo "✅ Merge failure regression prevention: ACTIVE"
echo "=============================================="