#!/bin/bash

# Main test runner script for Ferrous project
# Usage: ./run_tests.sh [category] [specific_test]

print_help() {
    echo "Ferrous Test Runner"
    echo "------------------"
    echo "Usage: ./run_tests.sh [category] [specific_test]"
    echo ""
    echo "Categories:"
    echo "  all         - Run all tests"
    echo "  lua         - Run Lua implementation tests"
    echo "  protocol    - Run protocol tests"
    echo "  performance - Run performance tests"
    echo "  integration - Run integration tests"
    echo "  features    - Run feature-specific tests"
    echo "  memory      - Run memory tracking tests"
    echo "  monitor     - Run monitor command tests"
    echo "  slowlog     - Run slowlog tests"
    echo "  client      - Run client command tests"
    echo "  auth        - Run authentication tests"
    echo ""
    echo "Examples:"
    echo "  ./run_tests.sh lua                      # Run all Lua tests"
    echo "  ./run_tests.sh lua robust_eval_test.py  # Run specific Lua test"
    echo "  ./run_tests.sh all                      # Run all tests"
}

run_lua_tests() {
    echo "=== Running Lua Tests ==="
    if [ -n "$1" ]; then
        if [ -f "tests/lua/$1" ]; then
            echo "Running tests/lua/$1"
            if [[ "$1" == *.py ]]; then
                python3 "tests/lua/$1"
            elif [[ "$1" == *.sh ]]; then
                bash "tests/lua/$1"
            fi
        else
            echo "Test file not found: tests/lua/$1"
            exit 1
        fi
    else
        # Run all Lua tests
        echo "Running all Lua tests"
        for test in tests/lua/*.py; do
            echo "Running $test..."
            python3 "$test"
        done
    fi
}

run_protocol_tests() {
    echo "=== Running Protocol Tests ==="
    if [ -n "$1" ]; then
        if [ -f "tests/protocol/$1" ]; then
            echo "Running tests/protocol/$1"
            if [[ "$1" == *.py ]]; then
                python3 "tests/protocol/$1"
            elif [[ "$1" == *.sh ]]; then
                bash "tests/protocol/$1"
            fi
        else
            echo "Test file not found: tests/protocol/$1"
            exit 1
        fi
    else
        # Run all protocol tests
        echo "Running all protocol tests"
        for test in tests/protocol/*.py; do
            echo "Running $test..."
            python3 "$test"
        done
    fi
}

run_performance_tests() {
    echo "=== Running Performance Tests ==="
    if [ -n "$1" ]; then
        if [ -f "tests/performance/$1" ]; then
            echo "Running tests/performance/$1"
            if [[ "$1" == *.py ]]; then
                python3 "tests/performance/$1"
            elif [[ "$1" == *.sh ]]; then
                bash "tests/performance/$1"
            fi
        else
            echo "Test file not found: tests/performance/$1"
            exit 1
        fi
    else
        # Run all performance tests
        echo "Running all performance tests"
        for test in tests/performance/*.sh; do
            echo "Running $test..."
            bash "$test"
        done
    fi
}

run_integration_tests() {
    echo "=== Running Integration Tests ==="
    if [ -n "$1" ]; then
        if [ -f "tests/integration/$1" ]; then
            echo "Running tests/integration/$1"
            if [[ "$1" == *.py ]]; then
                python3 "tests/integration/$1"
            elif [[ "$1" == *.sh ]]; then
                bash "tests/integration/$1"
            fi
        else
            echo "Test file not found: tests/integration/$1"
            exit 1
        fi
    else
        # Run all integration tests
        echo "Running all integration tests"
        for test in tests/integration/*.sh; do
            echo "Running $test..."
            bash "$test"
        done
        for test in tests/integration/*.py; do
            echo "Running $test..."
            python3 "$test"
        done
    fi
}

run_feature_tests() {
    local feature=$1
    local test_file=$2
    
    if [ -n "$feature" ] && [ -n "$test_file" ]; then
        # Run specific feature test
        if [ -f "tests/features/$feature/$test_file" ]; then
            echo "Running tests/features/$feature/$test_file"
            if [[ "$test_file" == *.py ]]; then
                python3 "tests/features/$feature/$test_file"
            elif [[ "$test_file" == *.sh ]]; then
                bash "tests/features/$feature/$test_file"
            elif [[ "$test_file" == *.rs ]]; then
                echo "Running Rust test file... (not implemented)"
                # This would require cargo test - future improvement
            fi
        else
            echo "Test file not found: tests/features/$feature/$test_file"
            exit 1
        fi
    elif [ -n "$feature" ]; then
        # Run all tests for specific feature
        echo "=== Running $feature Tests ==="
        if [ -d "tests/features/$feature" ]; then
            for test in tests/features/$feature/*.py tests/features/$feature/*.sh; do
                if [ -f "$test" ]; then
                    echo "Running $test..."
                    if [[ "$test" == *.py ]]; then
                        python3 "$test"
                    elif [[ "$test" == *.sh ]]; then
                        bash "$test"
                    fi
                fi
            done
        else
            echo "Feature directory not found: tests/features/$feature"
            exit 1
        fi
    else
        # Run all feature tests
        echo "=== Running All Feature Tests ==="
        for feature_dir in tests/features/*; do
            if [ -d "$feature_dir" ]; then
                feature_name=$(basename "$feature_dir")
                echo "Running $feature_name tests..."
                for test in "$feature_dir"/*.py "$feature_dir"/*.sh; do
                    if [ -f "$test" ]; then
                        echo "Running $test..."
                        if [[ "$test" == *.py ]]; then
                            python3 "$test"
                        elif [[ "$test" == *.sh ]]; then
                            bash "$test"
                        fi
                    fi
                done
            fi
        done
    fi
}

run_all_tests() {
    echo "=== Running All Tests ==="
    
    # Run all Lua tests
    run_lua_tests
    
    # Run all protocol tests
    run_protocol_tests
    
    # Run all integration tests
    run_integration_tests
    
    # Run all feature tests
    run_feature_tests
    
    # Run all performance tests
    run_performance_tests
}

# Main script logic
if [ $# -eq 0 ]; then
    # No arguments, print help
    print_help
    exit 0
fi

category=$1
specific_test=$2

case $category in
    help|--help|-h)
        print_help
        ;;
    lua)
        run_lua_tests "$specific_test"
        ;;
    protocol)
        run_protocol_tests "$specific_test"
        ;;
    performance)
        run_performance_tests "$specific_test"
        ;;
    integration)
        run_integration_tests "$specific_test"
        ;;
    features)
        run_feature_tests
        ;;
    memory)
        run_feature_tests "memory" "$specific_test"
        ;;
    monitor)
        run_feature_tests "monitor" "$specific_test"
        ;;
    slowlog)
        run_feature_tests "slowlog" "$specific_test"
        ;;
    client)
        run_feature_tests "client" "$specific_test"
        ;;
    auth)
        run_feature_tests "auth" "$specific_test"
        ;;
    all)
        run_all_tests
        ;;
    *)
        echo "Unknown category: $category"
        print_help
        exit 1
        ;;
esac

exit 0