#!/bin/bash
# Comprehensive Atomic Operations Test Runner for Ferrous

set -e

SCRIPT_DIR="$(dirname "${BASH_SOURCE[0]}")"
cd "$SCRIPT_DIR"

show_help() {
    echo "Ferrous Atomic Operations Test Runner"
    echo ""
    echo "Usage: $0 [OPTION]"
    echo ""
    echo "Options:"
    echo "  atomic      Run atomic operations tests only"
    echo "  regression  Run regression prevention tests only"  
    echo "  integration Run atomic integration tests only"
    echo "  all         Run all atomic tests (default)"
    echo "  help        Show this help"
    echo ""
    echo "These tests are designed to catch:"
    echo "- SET NX hanging bugs"
    echo "- Lua socket timeout issues"
    echo "- Blocking operation failures"
    echo "- Merge failure regressions"
    echo ""
}

run_atomic_tests() {
    echo "========================================="
    echo "RUNNING ATOMIC OPERATIONS TESTS"
    echo "========================================="
    
    # Check if server is running
    if ! redis-cli -p 6379 PING > /dev/null 2>&1; then
        echo "❌ Error: Ferrous server not running on port 6379"
        echo "Start the server first: ./target/release/ferrous"
        exit 1
    fi
    
    echo "✅ Server detected on port 6379"
    echo ""
    
    echo "Running comprehensive atomic operations tests..."
    python3 features/atomic_operations/test_atomic_operations_comprehensive.py
    
    echo ""
}

run_regression_tests() {
    echo "========================================="
    echo "RUNNING REGRESSION PREVENTION TESTS"
    echo "========================================="
    
    # Check if server is running
    if ! redis-cli -p 6379 PING > /dev/null 2>&1; then
        echo "❌ Error: Ferrous server not running on port 6379"
        exit 1
    fi
    
    echo "✅ Server detected on port 6379"
    echo ""
    
    echo "Running merge failure regression prevention tests..."
    python3 features/regression/test_merge_failure_prevention.py
    
    echo ""
}

run_integration_tests() {
    echo "========================================="
    echo "RUNNING ATOMIC INTEGRATION TESTS"
    echo "========================================="
    
    # Check if server is running
    if ! redis-cli -p 6379 PING > /dev/null 2>&1; then
        echo "❌ Error: Ferrous server not running on port 6379"
        exit 1
    fi
    
    echo "✅ Server detected on port 6379"
    echo ""
    
    echo "Running atomic operations integration tests..."
    ./integration/test_atomic_operations_integration.sh
    
    echo ""
}

run_all_atomic() {
    run_atomic_tests
    run_regression_tests
    run_integration_tests
    
    echo "========================================="
    echo "ALL ATOMIC OPERATIONS TESTS COMPLETE"
    echo "========================================="
    echo "✅ SET NX hanging prevention verified"
    echo "✅ Lua socket handling validated"
    echo "✅ Blocking operations confirmed reliable"
    echo "✅ Regression prevention active"
    echo "========================================="
}

case "${1:-all}" in
    atomic)
        run_atomic_tests
        ;;
    regression)
        run_regression_tests
        ;;
    integration)
        run_integration_tests
        ;;
    all)
        run_all_atomic
        ;;
    help|*)
        show_help
        ;;
esac