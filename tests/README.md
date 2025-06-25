# Ferrous Test Suite

This directory contains organized test files for the Ferrous Redis-compatible server. The tests are categorized according to their purpose and focus to make it easier to find and run specific tests.

## Directory Structure

- **`integration/`**: End-to-end tests that verify multiple components working together
  - `test_basic.sh` - Basic functionality tests
  - `test_commands.sh` - Tests for core Redis commands
  - `test_ping_command.py` - PING command test
  - `test_replication.sh` - Tests for master-slave replication

- **`lua/`**: Tests for Lua scripting implementation
  - `debug_concat.py` - Diagnostic tests for table field concatenation
  - `test_table_concat.py` - Table concatenation tests
  - `robust_eval_test.py` - Tests for EVAL command implementation
  - Plus various other Lua-specific tests

- **`protocol/`**: Tests for RESP protocol implementation
  - `test_comprehensive.py` - Protocol compliance tests
  - `test_protocol_fuzz.py` - Fuzzing tests for protocol robustness
  - `pipeline_test.py` - Tests for pipelined commands

- **`performance/`**: Benchmarks and performance tests
  - `test_benchmark.sh` - Performance benchmarking

- **`features/`**: Tests for specific features, organized into subdirectories
  - `auth/` - Authentication tests
  - `client/` - CLIENT command tests
  - `memory/` - Memory usage and tracking tests
  - `monitor/` - MONITOR command tests
  - `slowlog/` - SLOWLOG command tests

- **`unit/`**: Unit tests for individual components

- **`scripts/`**: Utility scripts for testing and diagnostics

## Running Tests

### Basic Tests

To run basic functionality tests:
```
cd ferrous
./tests/integration/test_basic.sh
```

### Protocol Tests

To run protocol compliance tests:
```
cd ferrous
python3 tests/protocol/test_comprehensive.py
```

### Lua Tests

To run Lua implementation tests:
```
cd ferrous
python3 tests/lua/robust_eval_test.py
```

### Performance Tests

To run performance benchmarks:
```
cd ferrous
./tests/performance/test_benchmark.sh
```

### Feature Tests

For feature-specific tests, navigate to the relevant feature directory and run the tests:
```
cd ferrous
python3 tests/features/memory/test_memory.py
```

## Adding New Tests

When adding new tests:

1. Place the test in the appropriate category directory
2. Follow the naming convention: `test_<feature>.<extension>`
3. Update this README if you add a new test category
4. Ensure your test can be run from the root directory

## Known Issues

- Lua table field concatenation tests currently expose a limitation in the VM implementation
- For accurate authentication tests, ensure the server is started with proper configuration