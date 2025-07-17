#!/bin/bash

# Ferrous Lua VM Test Suite Runner
# This script runs a consolidated set of tests to validate the VM implementation state
# Focusing on Lua 5.1 features needed for Redis integration

# Color codes for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

BINARY="./target/release/compile_and_execute"

if [ ! -f "$BINARY" ]; then
    echo -e "${RED}Error: Binary $BINARY not found. Please build first.${NC}"
    exit 1
fi

# Track results
passed=0
failed=0
skipped=0

run_test() {
    local test_file=$1
    local description=$2
    
    echo -n "Running $test_file - $description... "
    
    if [ ! -f "$test_file" ]; then
        echo -e "${YELLOW}SKIPPED${NC} (file not found)"
        ((skipped++))
        return
    fi
    
    # Run test and capture result
    if timeout 10s $BINARY $test_file > /tmp/test_output.txt 2>&1; then
        echo -e "${GREEN}PASSED${NC}"
        ((passed++))
    else
        echo -e "${RED}FAILED${NC}"
        echo "  Output (first 10 lines):"
        head -10 /tmp/test_output.txt | sed 's/^/    /'
        ((failed++))
    fi
}

print_header() {
    echo -e "\n${BLUE}$1:${NC}"
    echo "------------------------"
}

print_section_results() {
    local section=$1
    local total=$2
    local passed=$3
    local failed=$4
    local skipped=$5
    
    echo -e "\n${BLUE}$section Summary:${NC}"
    echo -e "  Total: $total tests"
    echo -e "  ${GREEN}Passed: $passed${NC}"
    if [ $failed -gt 0 ]; then
        echo -e "  ${RED}Failed: $failed${NC}"
    else
        echo -e "  Failed: 0"
    fi
    if [ $skipped -gt 0 ]; then
        echo -e "  ${YELLOW}Skipped: $skipped${NC}"
    else
        echo -e "  Skipped: 0"
    fi
}

echo "========================================"
echo -e "${BLUE}Ferrous Lua VM Test Suite${NC}"
echo "========================================"
echo "Testing Lua 5.1 features required for Redis integration"
echo ""

# Basic Language Features
print_header "Basic Language Features"
run_test "tests/lua/basic/assignment.lua" "Variable assignment and return"
run_test "tests/lua/basic/print.lua" "Print function"
run_test "tests/lua/basic/type.lua" "Type function"
run_test "tests/lua/basic/tostring.lua" "ToString function"
run_test "tests/lua/basic/arithmetic.lua" "Basic arithmetic operations"
run_test "tests/lua/basic/concat.lua" "String concatenation"

basic_total=6
basic_passed=$passed
basic_failed=$failed
basic_skipped=$skipped

# Reset counters for next section
section_passed=$passed
section_failed=$failed
section_skipped=$skipped
passed=0
failed=0
skipped=0

# Table Operations
print_header "Table Operations"
run_test "tests/lua/tables/create.lua" "Table creation"
run_test "tests/lua/tables/rawops.lua" "Raw table operations"
run_test "tests/lua/tables/metamethods.lua" "Table metamethods"

table_total=3
table_passed=$passed
table_failed=$failed
table_skipped=$skipped

# Update section totals
total_passed=$(($total_passed + $passed))
total_failed=$(($total_failed + $failed))
total_skipped=$(($total_skipped + $skipped))

# Reset counters for next section
passed=0
failed=0
skipped=0

# Functions and Closures
print_header "Functions and Closures"
run_test "tests/lua/functions/definition.lua" "Function definitions"
run_test "tests/lua/functions/closure.lua" "Closures"
run_test "tests/lua/functions/upvalue_simple.lua" "Simple upvalues"
run_test "tests/lua/functions/varargs.lua" "Variable arguments"
run_test "tests/lua/functions/tailcall.lua" "Tail call optimization"
# Skip coroutine test as Redis doesn't use coroutines
# run_test "tests/lua/functions/coroutine.lua" "Coroutines"

function_total=5
function_passed=$passed
function_failed=$failed
function_skipped=$skipped

# Update section totals
total_passed=$(($total_passed + $passed))
total_failed=$(($total_failed + $failed))
total_skipped=$(($total_skipped + $skipped))

# Reset counters for next section
passed=0
failed=0
skipped=0

# Control Flow
print_header "Control Flow"
run_test "tests/lua/control/numeric_for.lua" "Numeric for loops"
run_test "tests/lua/control/pairs.lua" "Generic pairs loops"
run_test "tests/lua/control/tforloop.lua" "Generic for loop protocol"

control_total=3
control_passed=$passed
control_failed=$failed
control_skipped=$skipped

# Update section totals
total_passed=$(($total_passed + $passed))
total_failed=$(($total_failed + $failed))
total_skipped=$(($total_skipped + $skipped))

# Reset counters for next section
passed=0
failed=0
skipped=0

# Standard Library
print_header "Standard Library"
run_test "tests/lua/stdlib/base.lua" "Base library functions"
run_test "tests/lua/stdlib/metatable.lua" "Metatable operations"
run_test "tests/lua/stdlib/errors.lua" "Error handling"
run_test "tests/lua/stdlib/redis.lua" "Redis-specific functionality"
# Skip module test as Redis doesn't use the module system
# run_test "tests/lua/stdlib/modules.lua" "Module system"

stdlib_total=4
stdlib_passed=$passed
stdlib_failed=$failed
stdlib_skipped=$skipped

# Calculate totals across all sections
total_tests=$((basic_total + table_total + function_total + control_total + stdlib_total))
total_passed=$((basic_passed + table_passed + function_passed + control_passed + stdlib_passed))
total_failed=$((basic_failed + table_failed + function_failed + control_failed + stdlib_failed))
total_skipped=$((basic_skipped + table_skipped + function_skipped + control_skipped + stdlib_skipped))

# Print section summaries
print_section_results "Basic Language Features" $basic_total $basic_passed $basic_failed $basic_skipped
print_section_results "Table Operations" $table_total $table_passed $table_failed $table_skipped
print_section_results "Functions and Closures" $function_total $function_passed $function_failed $function_skipped
print_section_results "Control Flow" $control_total $control_passed $control_failed $control_skipped
print_section_results "Standard Library" $stdlib_total $stdlib_passed $stdlib_failed $stdlib_skipped

echo -e "\n========================================"
echo -e "${BLUE}Test Results Summary:${NC}"
echo "========================================"
echo -e "Total Tests: ${total_tests}"
echo -e "${GREEN}Passed: ${total_passed}${NC}"
echo -e "${RED}Failed: ${total_failed}${NC}"
if [ $total_skipped -gt 0 ]; then
    echo -e "${YELLOW}Skipped: ${total_skipped}${NC}"
else
    echo -e "Skipped: 0"
fi
echo ""

if [ $total_failed -eq 0 ]; then
    if [ $total_skipped -eq 0 ]; then
        echo -e "${GREEN}All tests passed!${NC}"
        exit 0
    else
        echo -e "${YELLOW}All tests passed, but some tests were skipped.${NC}"
        exit 0
    fi
else
    echo -e "${RED}Some tests failed.${NC}"
    exit 1
fi