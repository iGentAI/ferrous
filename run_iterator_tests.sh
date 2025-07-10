#!/bin/bash
# Script to run iterator and loop tests for the Ferrous Lua VM

set -e  # Exit on any error

echo "===== Building standalone_lua_test ====="
cargo build --bin standalone_lua_test

echo -e "\n===== Running Basic ipairs Test ====="
./target/debug/standalone_lua_test test_ipairs.lua

echo -e "\n===== Running Comprehensive ipairs Test ====="
./target/debug/standalone_lua_test tests/lua/ipairs_comprehensive.lua

echo -e "\n===== Running Comprehensive pairs Test ====="
./target/debug/standalone_lua_test tests/lua/pairs_comprehensive.lua

echo -e "\n===== Running For Loop Edge Cases Test ====="
./target/debug/standalone_lua_test tests/lua/for_loop_edge_cases.lua

echo -e "\n===== All Tests Completed ====="