#!/bin/bash
# Validation Test Runner for Reported Ferrous Issues
# Tests pub/sub protocol, Lua scripting, missing commands, and event bus patterns

set -e

SCRIPT_DIR="$(dirname "${BASH_SOURCE[0]}")"
cd "$SCRIPT_DIR"

echo "========================================="
echo "FERROUS VALIDATION TEST SUITE"
echo "Testing Reported Compatibility Issues"
echo "========================================="
echo ""

# Check if server is running
if ! redis-cli -p 6379 PING > /dev/null 2>&1; then
    echo "❌ Error: Ferrous server not running on port 6379"
    echo ""
    echo "Start the server first:"
    echo "  cd .."
    echo "  ./target/release/ferrous > /dev/null 2>&1 &"
    echo ""
    exit 1
fi

echo "✅ Server detected on port 6379"
echo ""

# Function to run a test and track results
TOTAL_TESTS=0
PASSED_TESTS=0

run_test() {
    local test_name="$1"
    local test_command="$2"
    
    echo "Running: $test_name"
    echo "-----------------------------------------"
    
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    if $test_command; then
        PASSED_TESTS=$((PASSED_TESTS + 1))
        echo ""
    else
        echo "⚠️  Test revealed issues"
        echo ""
    fi
}

# 1. Pub/Sub Protocol Validation
run_test "Pub/Sub Protocol Validation" "python3 features/pubsub/test_pubsub_protocol_validation.py"

# 2. Lua Scripting Comprehensive Tests
run_test "Lua Scripting Tests" "python3 features/lua/test_lua_comprehensive.py"

# 3. Missing Commands Tests
run_test "Missing Commands Tests" "python3 features/commands/test_missing_commands.py"

# 4. Event Bus Compatibility
run_test "Event Bus Compatibility" "python3 features/event_bus/test_event_bus_compatibility.py"

# 5. Run original pub/sub test for comparison
echo "Running original pub/sub test for comparison..."
run_test "Original Pub/Sub Test" "python3 features/pubsub/test_pubsub_comprehensive.py"

# Summary
echo ""
echo "========================================="
echo "VALIDATION TEST SUMMARY"
echo "========================================="
echo "Tests Run: $TOTAL_TESTS"
echo "Tests Passed: $PASSED_TESTS"
echo "Success Rate: $(( PASSED_TESTS * 100 / TOTAL_TESTS ))%"
echo ""

if [ $PASSED_TESTS -eq $TOTAL_TESTS ]; then
    echo "🎉 All validation tests passed!"
    echo "The reported issues may have been resolved."
else
    echo "❌ Some validation tests failed."
    echo ""
    echo "Confirmed Issues:"
    echo "1. Pub/Sub RESP2 protocol violations causing IndexError"
    echo "2. Lua script execution problems (hanging/incorrect results)"
    echo "3. Missing commands (e.g., ZCARD)"
    echo "4. Event bus patterns not fully supported"
    echo ""
    echo "These tests confirm the compatibility report findings."
fi

echo "========================================="

exit $(( TOTAL_TESTS - PASSED_TESTS ))