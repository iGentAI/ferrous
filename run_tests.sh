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
    echo "  atomic      Run atomic operations and regression tests only"
    echo "  all         Run all test configurations"
    echo "  help        Show this help"
    echo ""
    echo "Examples:"
    echo "  $0 default  # Most common - basic functionality tests"
    echo "  $0 perf     # For benchmarking against Redis/Valkey"
    echo "  $0 auth     # For replication testing"
    echo "  $0 atomic   # For atomic operations and regression prevention"
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
    
    # Run comprehensive pub/sub protocol validation tests
    echo "Running pub/sub protocol validation..."
    python3 features/pubsub/test_pubsub_protocol_validation.py
    echo ""
    
    # Run ZCARD command tests
    echo "Running ZCARD command validation..."
    python3 features/sorted_sets/test_zcard.py
    echo ""
    
    # Run comprehensive expiry tests
    echo "Running comprehensive expiry operations tests..."
    python3 features/expiry/test_expiry_comprehensive.py
    echo ""
    
    python3 features/persistence/test_persistence_integration_clean.py
    echo ""
    
    # Run CORRECTED transaction tests with fixed WATCH mechanism
    echo "Running corrected transaction system tests..."
    python3 features/transactions/test_transactions_comprehensive.py
    echo ""
    
    # Run corrected WATCH mechanism tests using proper redis-py patterns
    echo "Running corrected WATCH mechanism tests..."
    python3 features/transactions/test_watch_corrected_usage.py
    echo ""
    
    # Run comprehensive Stream testing
    echo "Running comprehensive Stream validation..."
    python3 features/streams/test_streams_complete.py
    echo ""
    python3 features/streams/test_streams_edge_cases.py
    echo ""
    
    # Run comprehensive RDB data type validation
    echo "Running comprehensive RDB data type validation..."
    ./integration/validate_rdb_all_types.sh
    echo ""
    
    # Run atomic operations comprehensive tests
    echo "Running atomic operations comprehensive tests..."
    python3 features/atomic_operations/test_atomic_operations_comprehensive.py
    echo ""
    
    # Run regression prevention tests
    echo "Running regression prevention tests..."
    python3 features/regression/test_merge_failure_prevention.py
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
    
    # Kill any existing Ferrous servers to ensure clean state
    echo "Cleaning up any existing Ferrous servers..."
    pkill -f "ferrous" || true
    sleep 2
    
    # Ensure data directories exist
    mkdir -p data/master data/replica
    echo ""
    
    # Start master server (port 6379)
    echo "Starting master server: ./target/release/ferrous master.conf"
    ./target/release/ferrous master.conf > /tmp/ferrous-master.log 2>&1 &
    MASTER_PID=$!
    
    # Wait for master to start
    sleep 4
    
    # Verify master is running with auth
    if ! redis-cli -h 127.0.0.1 -p 6379 -a mysecretpassword PING > /dev/null 2>&1; then
        echo "❌ Master server failed to start or authenticate"
        echo "Master server log:"
        cat /tmp/ferrous-master.log | tail -10
        kill $MASTER_PID 2>/dev/null || true
        exit 1
    fi
    
    echo "✅ Master server running on port 6379 with authentication"
    
    # Start replica server (port 6380) 
    echo "Starting replica server: ./target/release/ferrous replica.conf"
    ./target/release/ferrous replica.conf > /tmp/ferrous-replica.log 2>&1 &
    REPLICA_PID=$!
    
    # Wait for replica to start and connect to master
    sleep 5
    
    # Verify replica is running (may or may not require auth depending on config)
    if redis-cli -h 127.0.0.1 -p 6380 PING > /dev/null 2>&1; then
        echo "✅ Replica server running on port 6380"
    elif redis-cli -h 127.0.0.1 -p 6380 -a mysecretpassword PING > /dev/null 2>&1; then
        echo "✅ Replica server running on port 6380 with authentication"
    else
        echo "⚠️ Replica server may not be accessible (replication features may be limited)"
        echo "Replica server log:"
        cat /tmp/ferrous-replica.log | tail -10
    fi
    
    echo ""
    
    # Run replication tests
    cd tests
    echo "Running replication tests with both master and replica servers..."
    ./integration/test_replication.sh
    cd ..
    
    # Proper cleanup of both servers
    echo ""
    echo "Cleaning up servers..."
    kill $MASTER_PID 2>/dev/null || true
    kill $REPLICA_PID 2>/dev/null || true
    
    # Give servers time to clean shutdown
    sleep 3
    
    # Force kill any remaining processes
    pkill -f "ferrous master.conf" || true
    pkill -f "ferrous replica.conf" || true
    
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
    
    # Run comprehensive benchmarks
    cd tests
    echo "Running core operations benchmarks..."
    ./performance/test_benchmark.sh
    echo ""
    echo "Running Stream operations benchmarks..."
    ./performance/test_stream_benchmarks.sh
    echo ""
    echo "Running comprehensive feature benchmarks..."
    ./performance/test_comprehensive_benchmarks.sh
    echo ""
    echo "Running Stream edge case validation..." 
    python3 features/streams/test_streams_edge_cases.py
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

run_atomic_tests() {
    echo "========================================="
    echo "RUNNING ATOMIC OPERATIONS TESTS"
    echo "========================================="
    
    # Check if server is already running
    if redis-cli -p 6379 PING > /dev/null 2>&1; then
        echo "✅ Using existing server on port 6379"
        EXTERNAL_SERVER=true
    else
        echo "Starting server: ./target/release/ferrous"
        
        # Start server
        ./target/release/ferrous > /tmp/ferrous-atomic.log 2>&1 &
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
    
    cd tests
    
    # Run atomic operations comprehensive tests
    echo "Running atomic operations comprehensive tests..."
    python3 features/atomic_operations/test_atomic_operations_comprehensive.py
    echo ""
    
    # Run regression prevention tests
    echo "Running regression prevention tests..."
    python3 features/regression/test_merge_failure_prevention.py
    echo ""
    
    # Run atomic integration tests
    echo "Running atomic operations integration tests..."
    ./integration/test_atomic_operations_integration.sh
    echo ""
    
    cd ..
    
    # Cleanup only if we started the server
    if [[ "$EXTERNAL_SERVER" == "false" ]]; then
        kill $SERVER_PID 2>/dev/null || true
    fi
    
    echo ""
    echo "✅ Atomic operations tests completed"
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
    atomic)
        run_atomic_tests
        ;;
    all)
        run_unit_tests
        run_default_tests  
        run_atomic_tests
        run_auth_tests
        run_perf_tests
        ;;
    help|*)
        show_help
        ;;
esac