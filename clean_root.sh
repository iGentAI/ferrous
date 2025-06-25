#!/bin/bash

# Script to clean up test files from the root directory that have been moved to tests/
# This will only remove files that exist in both the root and the tests directory

echo "Cleaning up duplicate test files from root directory..."

# Check and remove Lua test files
echo "Checking Lua test files..."
for file in test_lua.py test_lua_fixed.py test_complete_lua.py test_simple_lua.py minimal_eval.py minimal_eval_test.py test_table_concat.py debug_concat.py robust_eval_test.py test_valid_lua.lua; do
    if [ -f "$file" ] && [ -f "tests/lua/$file" ]; then
        echo "Removing duplicate: $file"
        rm "$file"
    fi
done

# Check and remove protocol test files
echo "Checking protocol test files..."
for file in test_comprehensive.py test_protocol_fuzz.py pipeline_test.py fixed_resp_test.py; do
    if [ -f "$file" ] && [ -f "tests/protocol/$file" ]; then
        echo "Removing duplicate: $file"
        rm "$file"
    fi
done

# Check and remove performance test files
echo "Checking performance test files..."
for file in test_benchmark.sh; do
    if [ -f "$file" ] && [ -f "tests/performance/$file" ]; then
        echo "Removing duplicate: $file"
        rm "$file"
    fi
done

# Check and remove feature test files
echo "Checking feature test files..."
# Memory tests
for file in test_memory.py test_memory_list.py test_memory_quick.py; do
    if [ -f "$file" ] && [ -f "tests/features/memory/$file" ]; then
        echo "Removing duplicate: $file"
        rm "$file"
    fi
done

# Monitor tests
for file in test_monitor.py test_multiple_monitor.py; do
    if [ -f "$file" ] && [ -f "tests/features/monitor/$file" ]; then
        echo "Removing duplicate: $file"
        rm "$file"
    fi
done

# Slowlog tests
for file in test_slowlog.py test_slowlog_comprehensive.py; do
    if [ -f "$file" ] && [ -f "tests/features/slowlog/$file" ]; then
        echo "Removing duplicate: $file"
        rm "$file"
    fi
done

# Client command tests
for file in test_client_commands.py; do
    if [ -f "$file" ] && [ -f "tests/features/client/$file" ]; then
        echo "Removing duplicate: $file"
        rm "$file"
    fi
done

# Auth tests
for file in test_auth.rs test_no_auth.py; do
    if [ -f "$file" ] && [ -f "tests/features/auth/$file" ]; then
        echo "Removing duplicate: $file"
        rm "$file"
    fi
done

# Check and remove integration test files
echo "Checking integration test files..."
for file in test_replication.sh test_basic.sh test_commands.sh test_ping_command.py; do
    if [ -f "$file" ] && [ -f "tests/integration/$file" ]; then
        echo "Removing duplicate: $file"
        rm "$file"
    fi
done

# Check and remove utility script files
echo "Checking utility script files..."
for file in test_sleep.py test_fixed.py correct_test.py fixed_lua.py redis_test.py final_test.py; do
    if [ -f "$file" ] && [ -f "tests/scripts/$file" ]; then
        echo "Removing duplicate: $file"
        rm "$file"
    fi
done

echo "Cleanup complete!"

# List any remaining test files in the root directory
remaining=$(find . -maxdepth 1 -name "test_*.py" -o -name "test_*.sh" -o -name "*_test.py")
if [ -n "$remaining" ]; then
    echo "Warning: Some test files still remain in the root directory:"
    echo "$remaining"
else
    echo "All test files have been successfully moved to the tests directory."
fi