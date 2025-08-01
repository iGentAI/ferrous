#!/bin/bash
# Run performance tests with optimal server configuration

echo "======================================"
echo "FERROUS PERFORMANCE TESTS"
echo "======================================"
echo ""
echo "These tests require the server started with log redirection:"
echo "  ./target/release/ferrous > /dev/null 2>&1 &"
echo ""
echo "Note: Log redirection improves performance by 2-3x"
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

# Run performance tests
echo "Running comprehensive performance benchmarks..."
./performance/test_benchmark.sh

echo ""
echo "======================================"
echo "PERFORMANCE TESTS COMPLETE"
echo ""
echo "Expected results with log redirection:"
echo "- PING: ~81k ops/sec"
echo "- SET:  ~80k ops/sec"
echo "- GET:  ~81k ops/sec"
echo "- Pipelined: ~769k ops/sec"
echo "======================================"