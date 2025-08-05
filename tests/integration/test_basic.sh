#!/bin/bash
# Basic test script for Ferrous server

echo "Testing Ferrous with redis-cli..."

# Define a function to check test results
check_result() {
    local test_name="$1"
    local expected="$2"
    local actual="$3"
    
    if [ "$actual" = "$expected" ]; then
        echo "✅ $test_name: PASSED"
    else
        echo "❌ $test_name: FAILED"
        echo "   Expected: $expected"
        echo "   Actual: $actual"
        exit 1
    fi
}

# Test PING command
echo "Testing PING..."
result=$(redis-cli -p 6379 PING)
check_result "PING" "PONG" "$result"

# Test ECHO command
echo "Testing ECHO..."
result=$(redis-cli -p 6379 ECHO "Hello Ferrous")
check_result "ECHO" "Hello Ferrous" "$result"

# Test SET and GET (basic)
echo "Testing SET/GET..."
result=$(redis-cli -p 6379 SET test "value")
check_result "SET" "OK" "$result"

result=$(redis-cli -p 6379 GET test)
check_result "GET" "value" "$result"

# Test QUIT
echo "Testing QUIT..."
result=$(echo "QUIT" | redis-cli -p 6379 | head -n1)
check_result "QUIT" "OK" "$result"

echo ""
echo "All basic tests completed successfully!"