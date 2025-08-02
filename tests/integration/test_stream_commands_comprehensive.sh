#!/bin/bash
# Comprehensive Stream command testing via redis-cli

echo "=========================================="
echo "COMPREHENSIVE STREAM TESTING VIA REDIS-CLI"  
echo "=========================================="

# Clean up any existing test data
redis-cli -p 6379 DEL comprehensive:stream >/dev/null 2>&1

# Test 1: Basic XADD operations
echo "Test 1: XADD operations..."
auto_id=$(redis-cli -p 6379 XADD comprehensive:stream "*" temp 25.5 humidity 60)
echo "Auto ID result: $auto_id"

specific_id=$(redis-cli -p 6379 XADD comprehensive:stream "1700000-0" event start location server1)
echo "Specific ID result: $specific_id"

# Test 2: XLEN  
echo -e "\nTest 2: XLEN..."
length=$(redis-cli -p 6379 XLEN comprehensive:stream)
echo "Stream length: $length"

# Test 3: XRANGE variations
echo -e "\nTest 3: XRANGE variations..."
echo "Full range:"
redis-cli -p 6379 XRANGE comprehensive:stream - +

echo -e "\nWith COUNT:"
redis-cli -p 6379 XRANGE comprehensive:stream - + COUNT 1

# Test 4: XREVRANGE
echo -e "\nTest 4: XREVRANGE..."
redis-cli -p 6379 XREVRANGE comprehensive:stream + -

echo -e "\nWith COUNT:"
redis-cli -p 6379 XREVRANGE comprehensive:stream + - COUNT 1

# Test 5: XREAD
echo -e "\nTest 5: XREAD..."
redis-cli -p 6379 XREAD STREAMS comprehensive:stream 0-0

# Test 6: Add more data for trimming test
echo -e "\nTest 6: Adding more data..."
for i in {1..5}; do
    redis-cli -p 6379 XADD comprehensive:stream "*" batch $i >/dev/null
done

new_length=$(redis-cli -p 6379 XLEN comprehensive:stream)
echo "Length after adding batch: $new_length"

# Test 7: XTRIM
echo -e "\nTest 7: XTRIM..."
trimmed=$(redis-cli -p 6379 XTRIM comprehensive:stream MAXLEN 3)
echo "Entries trimmed: $trimmed"

final_length=$(redis-cli -p 6379 XLEN comprehensive:stream)
echo "Final length: $final_length"

# Test 8: Consumer Groups (basic)
echo -e "\nTest 8: Consumer Groups..."
group_create=$(redis-cli -p 6379 XGROUP CREATE comprehensive:stream testgroup 0-0 2>/dev/null || echo "NOGROUP")
echo "Group creation: $group_create"

group_destroy=$(redis-cli -p 6379 XGROUP DESTROY comprehensive:stream testgroup 2>/dev/null || echo "NOGROUP")
echo "Group destroy: $group_destroy"

# Test 9: TYPE verification
echo -e "\nTest 9: TYPE verification..."
stream_type=$(redis-cli -p 6379 TYPE comprehensive:stream)
echo "Stream type: $stream_type"

# Test 10: Error handling
echo -e "\nTest 10: Error handling..."
error_result=$(redis-cli -p 6379 XADD comprehensive:stream invalid-id field value 2>&1 || echo "Error caught")
echo "Invalid ID error: $error_result"

# Clean up
redis-cli -p 6379 DEL comprehensive:stream >/dev/null 2>&1

echo -e "\n=========================================="
echo "COMPREHENSIVE STREAM TESTS COMPLETED"
echo "=========================================="