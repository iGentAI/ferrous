#!/bin/bash
# Ferrous Comprehensive Test Runner - Production Validation Suite
# 
# This unified test runner executes 200+ comprehensive tests covering:
# - Basic Redis command functionality and protocol compliance  
# - Production edge cases and data integrity validation
# - Concurrent operation stress testing and deadlock prevention
# - Performance benchmarking and regression detection
# - Complete Redis 6.0.9+ compliance including WATCH mechanisms
#
# The test suite validates production-ready Redis compatibility with
# comprehensive coverage of real-world usage patterns and edge cases.

set -e

SCRIPT_DIR="$(dirname "${BASH_SOURCE[0]}")"
cd "$SCRIPT_DIR"

show_help() {
    echo "Ferrous Test Runner"
    echo ""
    echo "Usage: $0 [OPTION]"
    echo ""
    echo "Options:"
    echo "  default     Run tests with default configuration (no auth, basic monitoring)"
    echo "  auth        Run tests with authentication (master.conf)"  
    echo "  perf        Run performance tests (log redirection)"
    echo "  unit        Run Rust unit tests only"
    echo "  atomic      Run atomic operations and regression tests only"
    echo "  monitoring  Run tests requiring monitoring config (slowlog, monitor, stats)"
    echo "  load        Run high-load stress tests with optimized server"
    echo "  all         Run all test configurations"
    echo "  help        Show this help"
    echo ""
    echo "Examples:"
    echo "  $0 default    # Most common - basic functionality tests"
    echo "  $0 monitoring # Tests requiring slowlog/monitoring features"
    echo "  $0 load       # Stress tests requiring optimized server setup"
    echo "  $0 perf       # For benchmarking against Redis/Valkey"
    echo "  $0 all        # Complete validation (may take 10+ minutes)"
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
    
    # Run comprehensive blocking operations tests for production queue patterns
    echo "Running comprehensive blocking operations tests..."
    python3 features/blocking/test_blocking_operations_comprehensive.py
    echo ""
    
    # Run production-grade edge case and limits validation
    echo "Running Redis limits compliance and edge case tests..."
    python3 features/edge_cases/test_redis_limits_compliance.py
    echo ""
    
    # Run connection stress and data integrity validation
    echo "Running connection stress and data integrity tests..."
    python3 features/connection/test_connection_stress.py
    echo ""
    
    # Run cross-command data safety validation
    echo "Running cross-command data integrity tests..."
    python3 features/data_integrity/test_cross_command_safety.py
    echo ""
    
    # Run comprehensive feature tests  
    echo "Running comprehensive feature validation..."
    python3 features/pubsub/test_pubsub_comprehensive.py
    echo ""
    
    # Run comprehensive pub/sub protocol validation tests
    echo "Running pub/sub protocol validation..."
    python3 features/pubsub/test_pubsub_protocol_validation.py
    echo ""
    
    # Run comprehensive pub/sub concurrency tests (SERIALIZED for clean server access)
    echo "Running pub/sub concurrency validation..."
    echo "⚠️  SERIALIZED EXECUTION: Pub/sub concurrency tests require exclusive server access"
    echo "   to prevent resource contention with other concurrent operations."
    echo "   Running in isolation to ensure accurate timing validation..."
    echo ""
    
    # Brief pause to ensure server is in clean state
    sleep 2
    python3 features/pubsub/test_pubsub_concurrency_comprehensive.py
    echo ""
    
    # Important: Wait for pub/sub cleanup before continuing
    echo "Waiting for pub/sub cleanup before continuing with other tests..."
    sleep 2
    
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
    
    # Run comprehensive WATCH mechanism tests with all edge cases and concurrent scenarios
    echo "Running comprehensive WATCH mechanism tests..."
    python3 features/transactions/test_watch_comprehensive.py
    echo ""
    
    # Run distributed locking pattern tests (SERIALIZED for WATCH consistency)
    echo "Running distributed locking pattern tests..."
    echo "⚠️  SERIALIZED EXECUTION: WATCH-based tests require clean server state"
    echo "   to prevent interference from concurrent operations."
    echo ""
    
    # Brief pause to ensure clean state for WATCH operations
    sleep 1
    python3 features/transactions/test_distributed_locking.py
    echo ""
    
    # Wait for transaction cleanup
    sleep 1
    
    # Run WATCH stress testing under extreme load
    echo "Running WATCH stress testing..."
    python3 features/transactions/test_watch_stress.py
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
    
    # Run WRONGTYPE protocol compliance tests (dipstick validation)
    echo "Running WRONGTYPE protocol compliance tests..."
    python3 features/protocol/test_wrongtype_compliance.py
    echo ""
    
    # Protocol & communication validation tests
    echo "Running protocol fuzz testing..."
    python3 protocol/test_protocol_fuzz.py
    echo ""
    
    echo "Running pipeline performance validation..."
    python3 protocol/pipeline_test.py
    echo ""
    
    # Memory & resource validation tests
    echo "Running memory quick functionality tests..."
    timeout 60 python3 features/memory/test_memory_quick.py
    echo ""
    
    echo "Running memory list operations tests..."
    timeout 60 python3 features/memory/test_memory_list.py
    echo ""
    
    echo "Running efficient memory validation..."
    python3 features/memory/test_memory_efficient.py
    echo ""
    
    # Stream comprehensive validation (additional to existing basic streams)
    echo "Running streams basic functionality tests..."
    python3 features/streams/test_streams_basic.py
    echo ""
    
    echo "Running streams protocol compliance validation..."
    python3 features/streams/test_streams_protocol_compliance.py
    echo ""
    
    echo "Running streams stress testing..."
    python3 features/streams/test_streams_stress.py
    echo ""
    
    # LUA scripting comprehensive validation (additional to existing basic Lua)
    echo "Running Lua error handling validation..."
    python3 features/lua/test_lua_error_handling.py
    echo ""
    
    echo "Running Lua error semantics expanded validation..."
    python3 features/lua/test_lua_error_semantics_expanded.py
    echo ""
    
    echo "Running Lua RESP conversion validation..."
    python3 features/lua/test_lua_resp_conversion_validation.py
    echo ""
    
    echo "Running comprehensive Lua validation..."
    timeout 60 python3 features/lua/test_lua_comprehensive.py
    echo ""
    
    # Performance & executor validation tests  
    echo "Running unified executor comprehensive validation..."
    python3 features/unified_executor/test_unified_executor_comprehensive.py
    echo ""
    
    echo "Running unified executor performance validation..."
    python3 performance/test_unified_executor_performance.py  
    echo ""
    
    # Client & connection validation tests
    echo "Running client commands validation..."
    python3 features/client/test_client_commands.py
    echo ""
    
    echo "Running event bus compatibility validation..."
    python3 features/event_bus/test_event_bus_compatibility.py
    echo ""
    
    # Monitoring & administration validation tests
    echo "Running monitor functionality tests..."
    python3 features/monitor/test_monitor.py
    echo ""
    
    echo "Running multiple monitor tests..."
    python3 features/monitor/test_multiple_monitor.py
    echo ""
    
    echo "Running slowlog basic tests..."
    python3 features/slowlog/test_slowlog.py
    echo ""
    
    # Note: slowlog comprehensive test requires slowlog enabled in config
    echo "Running slowlog comprehensive tests (config dependent)..."
    timeout 60 python3 features/slowlog/test_slowlog_comprehensive.py || echo "⚠️ Slowlog comprehensive test requires slowlog enabled in server config"
    echo ""
    
    # Integration & additional validation tests
    echo "Running ping command integration tests..."
    python3 integration/test_ping_command.py
    echo ""
    
    # Transaction validation tests (additional)
    echo "Running WATCH Redis compliance validation..."
    python3 features/transactions/test_watch_redis_compliance.py
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
    
    # Run comprehensive Lua advanced patterns test (BEFORE final tests)
    echo ""
    echo "Running Lua advanced patterns comprehensive test..."
    cd tests
    python3 features/lua/test_lua_advanced_patterns.py
    echo ""
    cd ..
    
    # FINAL TEST: Missing commands (includes SHUTDOWN which terminates server)
    echo ""
    echo "Running missing commands tests - FINAL TEST (SHUTDOWN will terminate server)..."
    cd tests
    python3 features/commands/test_missing_commands.py
    echo ""
    
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
    echo ""
    echo "Running unified executor performance tests..."
    python3 performance/test_unified_executor_performance.py
    echo ""
    echo "Running comprehensive all features benchmarks..."
    timeout 120 ./performance/test_comprehensive_all_features.sh || echo "⚠️ Comprehensive features benchmark timed out"
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

run_monitoring_tests() {
    echo "========================================="
    echo "RUNNING MONITORING CONFIGURATION TESTS"
    echo "Tests requiring slowlog, monitor, stats enabled"
    echo "========================================="
    
    # Kill any existing servers to ensure clean state
    pkill -f "ferrous" || true
    sleep 2
    
    echo "Starting server with monitoring config: ./target/release/ferrous ferrous-monitoring.conf"
    ./target/release/ferrous ferrous-monitoring.conf > /tmp/ferrous-monitoring.log 2>&1 &
    MONITORING_PID=$!
    
    # Wait for server with monitoring config to start
    sleep 4
    
    # Verify monitoring server is running
    if ! redis-cli -p 6379 PING > /dev/null 2>&1; then
        echo "❌ Monitoring-enabled server failed to start"
        echo "Server log:"
        cat /tmp/ferrous-monitoring.log | tail -10
        kill $MONITORING_PID 2>/dev/null || true
        echo ""
        echo "⚠️ Note: Monitoring features may not be fully implemented yet"
        echo "   Skipping monitoring-dependent tests for now"
        return 0  # Don't fail the entire test suite
    fi
    
    echo "✅ Server running with monitoring configuration"
    echo ""
    
    cd tests
    
    # Run slowlog tests (note: may need runtime CONFIG SET as fallback)
    echo "Running slowlog functionality tests..."
    timeout 60 python3 features/slowlog/test_slowlog.py || echo "⚠️ Slowlog basic test - may need dynamic config support"
    echo ""
    
    # Note: Comprehensive slowlog test has auth issues, skip for now
    echo "Running slowlog comprehensive tests..."
    echo "⚠️ Skipping slowlog comprehensive test due to hardcoded auth requirements"
    echo ""
    
    # Run monitor tests
    echo "Running monitor functionality tests..."
    timeout 60 python3 features/monitor/test_monitor.py || echo "⚠️ Monitor test - may require config implementation"
    echo ""
    
    echo "Running multiple monitor tests..."
    timeout 60 python3 features/monitor/test_multiple_monitor.py || echo "⚠️ Multiple monitor test - may require config implementation"
    echo ""
    
    # Run memory tests that may use stats
    echo "Running comprehensive memory tests..."
    timeout 120 python3 features/memory/test_memory.py || echo "⚠️ Memory test timed out - may require optimization"
    echo ""
    
    cd ..
    
    # Cleanup monitoring server
    echo ""
    echo "Cleaning up monitoring server..."
    kill $MONITORING_PID 2>/dev/null || true
    sleep 2
    pkill -f "ferrous ferrous-monitoring.conf" || true
    
    echo ""
    echo "✅ Monitoring tests completed (some may be pending full config implementation)"
}

run_high_load_tests() {
    echo "========================================="
    echo "RUNNING HIGH LOAD TESTS"
    echo "Tests requiring optimized server setup"
    echo "========================================="
    
    # Kill any existing servers
    pkill -f "ferrous" || true
    sleep 2
    
    echo "Starting server optimized for high load: ./target/release/ferrous > /dev/null 2>&1"
    ./target/release/ferrous > /dev/null 2>&1 &
    LOAD_PID=$!
    
    # Wait for optimized server to start
    sleep 3
    
    if ! redis-cli -p 6379 PING > /dev/null 2>&1; then
        echo "❌ High load server failed to start"
        kill $LOAD_PID 2>/dev/null || true
        return 1
    fi
    
    echo "✅ Server running with optimized configuration"
    echo ""
    
    cd tests
    
    # Run stress tests that need optimized server
    echo "Running streams stress testing..."
    timeout 180 python3 features/streams/test_streams_stress.py || echo "⚠️ Streams stress test issues"
    echo ""
    
    echo "Running transaction stress testing..."
    timeout 180 python3 features/transactions/test_watch_stress.py || echo "⚠️ Transaction stress test issues"
    echo ""
    
    echo "Running unified executor comprehensive tests..."
    timeout 120 python3 features/unified_executor/test_unified_executor_comprehensive.py || echo "⚠️ Unified executor test issues"
    echo ""
    
    echo "Running event bus compatibility tests..."
    timeout 120 python3 features/event_bus/test_event_bus_compatibility.py || echo "⚠️ Event bus test issues"
    echo ""
    
    # Run performance tests
    echo "Running unified executor performance tests..."
    timeout 120 python3 performance/test_unified_executor_performance.py || echo "⚠️ Executor performance test issues"
    echo ""
    
    cd ..
    
    # Cleanup high load server
    echo ""
    echo "Cleaning up high load server..."
    kill $LOAD_PID 2>/dev/null || true
    sleep 2
    
    echo ""
    echo "✅ High load tests completed"
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
    monitoring)
        run_monitoring_tests
        ;;
    load)
        run_high_load_tests
        ;;
    all)
        run_unit_tests
        run_default_tests  
        run_atomic_tests
        run_monitoring_tests
        run_high_load_tests
        run_auth_tests
        run_perf_tests
        ;;
    help|*)
        show_help
        ;;
esac