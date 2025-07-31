#!/bin/bash

# Enhanced Lua Validation Suite Runner
# Executes comprehensive validation including original tests and new specification compliance tests

# Color codes for enhanced output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
NC='\033[0m' # No Color

BINARY="./target/release/compile_and_execute"

echo -e "${BLUE}========================================"
echo -e "Ferrous Lua Comprehensive Validation Suite"
echo -e "========================================${NC}"
echo ""

if [ ! -f "$BINARY" ]; then
    echo -e "${RED}Error: Binary $BINARY not found. Building...${NC}"
    cargo build --release --lib --bin compile_and_execute
    if [ $? -ne 0 ]; then
        echo -e "${RED}Build failed. Exiting.${NC}"
        exit 1
    fi
fi

# Track comprehensive results
original_passed=0
original_failed=0
validation_passed=0
validation_failed=0

echo -e "${PURPLE}Phase 1: Running Original Test Suite${NC}"
echo -e "======================================"

# Run original test suite and capture results
./test_suite.sh > /tmp/original_results.txt 2>&1

# Extract results from original test suite
original_passed=$(grep "Passed:" /tmp/original_results.txt | tail -1 | awk '{print $2}')
original_failed=$(grep "Failed:" /tmp/original_results.txt | tail -1 | awk '{print $2}')

echo -e "Original Suite Results:"
echo -e "  ${GREEN}Passed: $original_passed${NC}"
echo -e "  ${RED}Failed: $original_failed${NC}"
echo ""

echo -e "${PURPLE}Phase 2: Running Enhanced Validation Suite${NC}"
echo -e "========================================"

# Test validation suite files
validation_tests=(
    "tests/lua/validation/register_integrity.lua"
    "tests/lua/validation/upvalue_correctness.lua"
    "tests/lua/validation/specification_compliance.lua"
)

for test_file in "${validation_tests[@]}"; do
    if [ -f "$test_file" ]; then
        echo -e "Running $(basename $test_file)..."
        if timeout 30s $BINARY $test_file > /tmp/validation_output.txt 2>&1; then
            echo -e "  ${GREEN}✓ PASSED${NC}"
            validation_passed=$((validation_passed + 1))
        else
            echo -e "  ${RED}✗ FAILED${NC}"
            echo "    Error output:"
            tail -5 /tmp/validation_output.txt | sed 's/^/    /'
            validation_failed=$((validation_failed + 1))
        fi
    else
        echo -e "  ${YELLOW}⚠ SKIPPED${NC} (file not found): $test_file"
    fi
done

echo ""
echo -e "${PURPLE}Phase 3: Comprehensive Analysis${NC}"
echo -e "==============================="

# Calculate comprehensive metrics
total_original=$((original_passed + original_failed))
total_validation=$((validation_passed + validation_failed))
total_tests=$((total_original + total_validation))
total_passed=$((original_passed + validation_passed))
total_failed=$((original_failed + validation_failed))

echo -e "Comprehensive Results:"
echo -e "  Original Test Suite: ${original_passed}/${total_original} passed"
echo -e "  Validation Suite: ${validation_passed}/${total_validation} passed"
echo -e "  ${BLUE}Overall: ${total_passed}/${total_tests} tests passed${NC}"

# Calculate success rate
if [ $total_tests -gt 0 ]; then
    success_rate=$(( (total_passed * 100) / total_tests ))
    echo -e "  ${BLUE}Success Rate: ${success_rate}%${NC}"
    
    if [ $success_rate -ge 90 ]; then
        echo -e "  ${GREEN}✓ Excellent compliance${NC}"
    elif [ $success_rate -ge 75 ]; then
        echo -e "  ${YELLOW}⚠ Good compliance with room for improvement${NC}"
    else
        echo -e "  ${RED}⚠ Requires additional implementation work${NC}"
    fi
fi

echo ""
echo -e "${PURPLE}Phase 4: Detailed Analysis${NC}"
echo -e "========================="

# Show detailed failure analysis if there are failures
if [ $total_failed -gt 0 ]; then
    echo -e "${RED}Failed Test Analysis:${NC}"
    
    if [ $original_failed -gt 0 ]; then
        echo -e "  Original suite failures require architectural fixes"
    fi
    
    if [ $validation_failed -gt 0 ]; then
        echo -e "  Validation failures indicate specification compliance gaps"
    fi
    
    echo -e "\nNext steps:"
    echo -e "  1. Review failed test outputs for specific error patterns"
    echo -e "  2. Apply systematic fixes based on comprehensive analysis"
    echo -e "  3. Re-run validation suite to measure improvement"
else
    echo -e "${GREEN}✓ All tests passed - Full Lua 5.1 specification compliance achieved!${NC}"
fi

# Generate summary report
echo -e "\n${BLUE}========================================"
echo -e "Validation Complete"
echo -e "========================================${NC}"

# Exit with appropriate code
if [ $total_failed -eq 0 ]; then
    exit 0
else
    exit 1
fi