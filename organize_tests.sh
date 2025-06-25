#!/bin/bash

# Script to organize test files into appropriate directories
echo "Organizing test scripts into proper directories..."

# Create main test directories
mkdir -p tests/integration
mkdir -p tests/unit
mkdir -p tests/lua
mkdir -p tests/protocol
mkdir -p tests/performance
mkdir -p tests/scripts

# Create feature-specific directories
mkdir -p tests/features/memory
mkdir -p tests/features/monitor
mkdir -p tests/features/slowlog
mkdir -p tests/features/client
mkdir -p tests/features/auth

# Move Lua test files
echo "Moving Lua test files..."
if [ -f test_lua.py ]; then mv test_lua.py tests/lua/; fi
if [ -f test_lua_fixed.py ]; then mv test_lua_fixed.py tests/lua/; fi
if [ -f test_complete_lua.py ]; then mv test_complete_lua.py tests/lua/; fi
if [ -f test_simple_lua.py ]; then mv test_simple_lua.py tests/lua/; fi
if [ -f minimal_eval.py ]; then mv minimal_eval.py tests/lua/; fi
if [ -f minimal_eval_test.py ]; then mv minimal_eval_test.py tests/lua/; fi
if [ -f test_table_concat.py ]; then mv test_table_concat.py tests/lua/; fi
if [ -f debug_concat.py ]; then mv debug_concat.py tests/lua/; fi
if [ -f robust_eval_test.py ]; then mv robust_eval_test.py tests/lua/; fi
if [ -f test_valid_lua.lua ]; then mv test_valid_lua.lua tests/lua/; fi

# Move protocol test files
echo "Moving protocol test files..."
if [ -f test_comprehensive.py ]; then mv test_comprehensive.py tests/protocol/; fi
if [ -f test_protocol_fuzz.py ]; then mv test_protocol_fuzz.py tests/protocol/; fi
if [ -f pipeline_test.py ]; then mv pipeline_test.py tests/protocol/; fi
if [ -f fixed_resp_test.py ]; then mv fixed_resp_test.py tests/protocol/; fi

# Move performance test files
echo "Moving performance test files..."
if [ -f test_benchmark.sh ]; then mv test_benchmark.sh tests/performance/; fi

# Move feature test files
echo "Moving feature test files..."
# Memory tests
if [ -f test_memory.py ]; then mv test_memory.py tests/features/memory/; fi
if [ -f test_memory_list.py ]; then mv test_memory_list.py tests/features/memory/; fi
if [ -f test_memory_quick.py ]; then mv test_memory_quick.py tests/features/memory/; fi

# Monitor tests
if [ -f test_monitor.py ]; then mv test_monitor.py tests/features/monitor/; fi
if [ -f test_multiple_monitor.py ]; then mv test_multiple_monitor.py tests/features/monitor/; fi

# Slowlog tests
if [ -f test_slowlog.py ]; then mv test_slowlog.py tests/features/slowlog/; fi
if [ -f test_slowlog_comprehensive.py ]; then mv test_slowlog_comprehensive.py tests/features/slowlog/; fi

# Client command tests
if [ -f test_client_commands.py ]; then mv test_client_commands.py tests/features/client/; fi

# Auth tests
if [ -f test_auth.rs ]; then mv test_auth.rs tests/features/auth/; fi
if [ -f test_no_auth.py ]; then mv test_no_auth.py tests/features/auth/; fi

# Move integration test files
echo "Moving integration test files..."
if [ -f test_replication.sh ]; then mv test_replication.sh tests/integration/; fi
if [ -f test_basic.sh ]; then mv test_basic.sh tests/integration/; fi
if [ -f test_commands.sh ]; then mv test_commands.sh tests/integration/; fi
if [ -f test_ping_command.py ]; then mv test_ping_command.py tests/integration/; fi

# Move unit test files (any other *_test.rs files)
echo "Moving unit test files..."
for file in $(find . -maxdepth 1 -name '*_test.rs' -not -name 'test_auth.rs'); do
    if [ -f "$file" ]; then
        mv "$file" tests/unit/
    fi
done

# Move utility scripts
echo "Moving utility scripts..."
if [ -f test_sleep.py ]; then mv test_sleep.py tests/scripts/; fi
if [ -f test_fixed.py ]; then mv test_fixed.py tests/scripts/; fi
if [ -f correct_test.py ]; then mv correct_test.py tests/scripts/; fi
if [ -f fixed_lua.py ]; then mv fixed_lua.py tests/scripts/; fi
if [ -f redis_test.py ]; then mv redis_test.py tests/scripts/; fi
if [ -f final_test.py ]; then mv final_test.py tests/scripts/; fi

echo "Test organization complete!"
echo "Summary of test organization:"
echo "------------------------"
echo "Lua tests: $(find tests/lua -type f | wc -l) files"
echo "Protocol tests: $(find tests/protocol -type f | wc -l) files"
echo "Performance tests: $(find tests/performance -type f | wc -l) files"
echo "Feature tests: $(find tests/features -type f -o -type d | grep -v "^tests/features$" | wc -l) files/directories"
echo "Integration tests: $(find tests/integration -type f | wc -l) files"
echo "Unit tests: $(find tests/unit -type f | wc -l) files"
echo "Utility scripts: $(find tests/scripts -type f | wc -l) files"