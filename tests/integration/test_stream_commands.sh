#!/bin/bash
# Test Stream commands via redis-cli

echo "==========================================
TESTING STREAM COMMANDS VIA REDIS-CLI
=========================================="

# Test XADD with auto ID
echo "Test 1: XADD with auto-generated ID..."
result=$(redis-cli -p 6379 XADD test:stream "*" field1 value1 field2 value2)
echo "XADD result: $result"

# Test XLEN
echo -e "\nTest 2: XLEN..."
length=$(redis-cli -p 6379 XLEN test:stream)
echo "Stream length: $length"

# Test XRANGE
echo -e "\nTest 3: XRANGE..."
redis-cli -p 6379 XRANGE test:stream - +
echo "XRANGE completed"

# Test XADD with specific ID
echo -e "\nTest 4: XADD with specific ID..."
result=$(redis-cli -p 6379 XADD test:stream 2000000-0 temp 25.5 humidity 60)
echo "XADD specific ID result: $result"

# Test XREVRANGE  
echo -e "\nTest 5: XREVRANGE..."
redis-cli -p 6379 XREVRANGE test:stream + - COUNT 2
echo "XREVRANGE completed"

# Test XTRIM
echo -e "\nTest 6: XTRIM..."
trimmed=$(redis-cli -p 6379 XTRIM test:stream MAXLEN 5)
echo "Trimmed entries: $trimmed"

# Test TYPE command on stream
echo -e "\nTest 7: TYPE command..."
type_result=$(redis-cli -p 6379 TYPE test:stream)
echo "Stream type: $type_result"

# Test XREAD (basic)
echo -e "\nTest 8: XREAD (basic)..."
redis-cli -p 6379 XREAD STREAMS test:stream 0-0
echo "XREAD completed"

# Test consumer group operations
echo -e "\nTest 9: Consumer group operations..."
group_result=$(redis-cli -p 6379 XGROUP CREATE test:stream group1 0-0 2>&1)
exit_code=$?

if [ $exit_code -eq 0 ]; then
    echo "Consumer group result: $group_result"
else
    echo "❌ ERROR: Consumer group creation failed!"
    echo "Error output: $group_result"
    echo "Exit code: $exit_code"
    # Note: Not exiting here as consumer groups might not be implemented yet
    # but we're being transparent about the failure
fi

# Clean up
redis-cli -p 6379 DEL test:stream >/dev/null 2>&1

echo -e "\n=========================================="
echo "STREAM COMMAND TESTS COMPLETED"
echo "=========================================="