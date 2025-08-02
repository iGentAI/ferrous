#!/bin/bash
# Run tests that work with default server configuration (no authentication)

echo "======================================"
echo "FERROUS DEFAULT CONFIGURATION TESTS"
echo "======================================"
echo ""
echo "These tests require the server started with:"
echo "  ./target/release/ferrous"
echo ""

# Check if server is running
if ! redis-cli -p 6379 PING > /dev/null 2>&1; then
    echo "❌ Error: Ferrous server not running on port 6379"
    echo ""
    echo "Start the server first:"
    echo "  cd .."
    echo "  ./target/release/ferrous"
    echo ""
    exit 1
fi

echo "✅ Server detected on port 6379"
echo ""

# Run all default configuration tests
echo "Running basic functionality tests..."
./integration/test_basic.sh

echo -e "\nRunning commands tests..." 
./integration/test_commands.sh

echo -e "\nRunning protocol compliance tests..."
python3 protocol/test_comprehensive.py

echo -e "\nRunning pipeline tests..."
python3 protocol/pipeline_test.py

echo -e "\nRunning client command tests..."
python3 features/client/test_client_commands.py

echo -e "\nRunning memory tests..."
python3 features/memory/test_memory.py

echo -e "\nTesting Lua script cache (global cache feature)..."
SCRIPT_SHA=$(redis-cli -p 6379 SCRIPT LOAD "return 'Test global cache'")
RESULT=$(redis-cli -p 6379 EVALSHA $SCRIPT_SHA 0)
if [[ "$RESULT" == "\"Test global cache\"" ]]; then
    echo "✅ Global Lua script cache working correctly"
else 
    echo "❌ Global Lua script cache test failed: $RESULT"
fi

# Run comprehensive feature tests  
echo "Running comprehensive feature validation..."
python3 features/pubsub/test_pubsub_comprehensive.py
echo ""
python3 features/persistence/test_persistence_integration_clean.py
echo ""
python3 features/transactions/test_transactions_comprehensive.py
echo ""

# Run comprehensive Stream testing including edge cases
echo "Running comprehensive Stream validation..."
python3 features/streams/test_streams_complete.py
echo ""
python3 features/streams/test_streams_edge_cases.py
echo ""

echo ""
echo "======================================"
echo "DEFAULT CONFIGURATION TESTS COMPLETE"
echo "======================================"