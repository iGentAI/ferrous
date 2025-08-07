#!/bin/bash
# Comprehensive tests for BLPOP/BRPOP blocking operations

echo "=========================================="
echo "TESTING BLOCKING OPERATIONS (BLPOP/BRPOP)"
echo "=========================================="

# Test 1: Basic BLPOP with immediate data
echo "Test 1: BLPOP with immediate data available..."
redis-cli -p 6379 FLUSHDB > /dev/null
redis-cli -p 6379 LPUSH test_queue "item1" "item2" > /dev/null

result=$(redis-cli -p 6379 BLPOP test_queue 1)
# Check if result contains both queue name and value (newline-separated format)
if echo "$result" | grep -q "test_queue" && echo "$result" | grep -q "item"; then
    echo "✅ BLPOP with immediate data: PASSED"
else
    echo "❌ BLPOP with immediate data: FAILED - $result"
fi
echo ""

# Test 2: Basic BRPOP with immediate data
echo "Test 2: BRPOP with immediate data available..."
redis-cli -p 6379 LPUSH test_queue "item3" "item4" > /dev/null

result=$(redis-cli -p 6379 BRPOP test_queue 1)
# Check if result contains both queue name and value (newline-separated format)
if echo "$result" | grep -q "test_queue" && echo "$result" | grep -q "item"; then
    echo "✅ BRPOP with immediate data: PASSED"
else
    echo "❌ BRPOP with immediate data: FAILED - $result"
fi
echo ""

# Test 3: BLPOP timeout test
echo "Test 3: BLPOP timeout behavior..."
redis-cli -p 6379 FLUSHDB > /dev/null

echo "Starting BLPOP with 2 second timeout (should return null)..."
timeout 5 redis-cli -p 6379 BLPOP empty_queue 2
echo "BLPOP timeout completed"
echo ""

# Test 4: BLPOP wake-up test
echo "Test 4: BLPOP wake-up on data arrival..."
redis-cli -p 6379 FLUSHDB > /dev/null

# Start BLPOP in background
echo "Starting background BLPOP..."
redis-cli -p 6379 BLPOP wake_queue 10 &
BLPOP_PID=$!

# Wait a moment then push data
sleep 1
echo "Pushing data to wake up BLPOP..."
redis-cli -p 6379 LPUSH wake_queue "wakeup_value" > /dev/null

# Wait for BLPOP to complete
wait $BLPOP_PID
echo "BLPOP wake-up test completed"
echo ""

# Test 5: Multiple key BLPOP
echo "Test 5: BLPOP with multiple keys..."
redis-cli -p 6379 FLUSHDB > /dev/null
redis-cli -p 6379 LPUSH queue2 "multi_item" > /dev/null

result=$(redis-cli -p 6379 BLPOP queue1 queue2 queue3 1)
# Check if result contains the correct queue and item (newline-separated format)
if echo "$result" | grep -q "queue2" && echo "$result" | grep -q "multi_item"; then
    echo "✅ Multi-key BLPOP: PASSED"
else
    echo "❌ Multi-key BLPOP: FAILED - $result"
fi
echo ""

# Test 6: Error handling
echo "Test 6: Error handling for invalid arguments..."
result=$(redis-cli -p 6379 BLPOP)
echo "BLPOP with no arguments: $result"

result=$(redis-cli -p 6379 BLPOP key1)  
echo "BLPOP with insufficient arguments: $result"

result=$(redis-cli -p 6379 BLPOP key1 invalid_timeout)
echo "BLPOP with invalid timeout: $result"
echo ""

echo "=========================================="
echo "BLOCKING OPERATIONS TESTS COMPLETED"
echo "=========================================="