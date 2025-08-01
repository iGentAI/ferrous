#!/bin/bash
# Ferrous Test Runner - Simple way to run all tests with correct configurations

set -e

SCRIPT_DIR="$(dirname "${BASH_SOURCE[0]}")"
cd "$SCRIPT_DIR"

show_help() {
    echo "Ferrous Test Runner"
    echo ""
    echo "Usage: $0 [OPTION]"
    echo ""
    echo "Options:"
    echo "  default     Run tests with default configuration (no auth)"
    echo "  auth        Run tests with authentication (master.conf)"  
    echo "  perf        Run performance tests (log redirection)"
    echo "  unit        Run Rust unit tests only"
    echo "  all         Run all test configurations"
    echo "  help        Show this help"
    echo ""
    echo "Examples:"
    echo "  $0 default  # Most common - basic functionality tests"
    echo "  $0 perf     # For benchmarking against Valkey"
    echo "  $0 auth     # For replication testing"
    echo ""
}

run_default_tests() {
    echo "========================================="
    echo "RUNNING DEFAULT CONFIGURATION TESTS"
    echo "========================================="
    
    # Check if server is already running
    if redis-cli -p 6379 PING > /dev/null 2>&1; then
        echo "✅ Using existing server on port 6379"
        EXTERNAL_SERVER=true
    else
        echo "Starting server: ./target/release/ferrous"
        echo ""
        
        # Start server
        ./target/release/ferrous > /tmp/ferrous-default.log 2>&1 &
        SERVER_PID=$!
        sleep 2
        
        # Verify server is running
        if ! redis-cli -p 6379 PING > /dev/null 2>&1; then
            echo "❌ Server failed to start"
            kill $SERVER_PID 2>/dev/null || true
            exit 1
        fi
        
        echo "✅ Server running on port 6379"
        EXTERNAL_SERVER=false
    fi
    
    echo ""
    
    # Run basic integration tests
    echo "Running basic tests..."
    cd tests
    ./integration/test_basic.sh
    echo ""
    
    # Run command tests including new commands
    echo "Running command tests..."
    ./integration/test_commands.sh
    echo ""
    
    # Run tests for newly implemented commands
    echo "Running tests for newly implemented commands..."
    ./integration/test_new_commands.sh
    echo ""
    
    # Run tests for blocking operations
    echo "Running blocking operations tests..."
    ./integration/test_blocking_operations.sh
    echo ""
    
    # Run protocol compliance tests
    echo "Running protocol compliance tests..."
    python3 protocol/test_comprehensive.py
    echo ""
    
    # Run comprehensive feature tests  
    echo "Running comprehensive feature validation..."
    python3 features/pubsub/test_pubsub_comprehensive.py
    echo ""
    python3 features/persistence/test_persistence_integration_clean.py
    echo ""
    python3 features/transactions/test_transactions_comprehensive.py
    echo ""
    
    cd ..
    
    # Test global Lua script cache
    echo ""
    echo "Testing global Lua script cache..."
    SCRIPT_SHA=$(redis-cli -p 6379 SCRIPT LOAD "return 'Global cache test'")
    RESULT=$(redis-cli -p 6379 EVALSHA $SCRIPT_SHA 0)
    # Remove quotes for comparison since redis-cli may or may not include them
    RESULT_CLEAN=$(echo "$RESULT" | sed 's/^"\(.*\)"$/\1/')
    if [[ "$RESULT_CLEAN" == "Global cache test" ]]; then
        echo "✅ Global Lua script cache working correctly"
    else
        echo "❌ Lua script cache failed. Expected: 'Global cache test', Got: '$RESULT_CLEAN'"
    fi
    
    # Cleanup only if we started the server
    if [[ "$EXTERNAL_SERVER" == "false" ]]; then
        kill $SERVER_PID 2>/dev/null || true
    fi
    
    echo ""
    echo "✅ Default tests completed successfully"
}

run_auth_tests() {
    echo "========================================="
    echo "RUNNING AUTHENTICATED CONFIGURATION TESTS" 
    echo "========================================="
    echo "Starting server: ./target/release/ferrous master.conf"
    echo ""
    
    # Ensure data directory exists
    mkdir -p data/master
    
    # Start server 
    ./target/release/ferrous master.conf > /tmp/ferrous-auth.log 2>&1 &
    SERVER_PID=$!
    sleep 3
    
    # Verify server is running with auth
    if ! redis-cli -p 6379 -a mysecretpassword PING > /dev/null 2>&1; then
        echo "❌ Authenticated server failed to start"
        kill $SERVER_PID 2>/dev/null || true
        exit 1
    fi
    
    echo "✅ Authenticated server running on port 6379"
    echo ""
    
    # Run replication tests
    cd tests
    echo "Running replication tests..."
    ./integration/test_replication.sh
    cd ..
    
    # Cleanup
    kill $SERVER_PID
    echo ""
    echo "✅ Authenticated tests completed successfully"
}

run_perf_tests() {
    echo "========================================="
    echo "RUNNING PERFORMANCE TESTS"
    echo "========================================="
    echo "Starting server: ./target/release/ferrous > /dev/null 2>&1 &"
    echo ""
    
    # Start server with log redirection for best performance
    ./target/release/ferrous > /dev/null 2>&1 &
    SERVER_PID=$!
    sleep 2
    
    # Verify server
    if ! redis-cli -p 6379 PING > /dev/null 2>&1; then
        echo "❌ Performance server failed to start"
        exit 1
    fi
    
    echo "✅ Performance-optimized server running"
    echo ""
    
    # Run benchmarks
    cd tests
    ./performance/test_benchmark.sh
    cd ..
    
    # Cleanup
    kill $SERVER_PID
    echo ""
    echo "✅ Performance tests completed successfully"
}

run_unit_tests() {
    echo "========================================="  
    echo "RUNNING RUST UNIT TESTS"
    echo "========================================="
    echo ""
    
    cargo test --release
    
    echo ""
    echo "✅ Unit tests completed successfully"
}

case "${1:-help}" in
    default)
        run_default_tests
        ;;
    auth)
        run_auth_tests
        ;;
    perf) 
        run_perf_tests
        ;;
    unit)
        run_unit_tests
        ;;
    all)
        run_unit_tests
        run_default_tests  
        run_auth_tests
        run_perf_tests
        ;;
    help|*)
        show_help
        ;;
esac