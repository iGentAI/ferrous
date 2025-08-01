# Persistence Tests Documentation

## Overview
The persistence tests verify RDB (Redis Database) and AOF (Append Only File) functionality in Ferrous.

## Race Condition Issue and Resolution

### The Problem
The original `test_persistence_integration.py` had a race condition when tests were executed concurrently:

1. **CONFIG SET Not Supported**: Ferrous doesn't support `CONFIG SET` for dynamic configuration changes
2. **Fixed RDB Path**: The server always creates `dump.rdb` in its working directory
3. **Concurrent Test Interference**: Multiple tests accessing the same RDB file caused failures

### Race Condition Symptoms
- "RDB file not found" errors when tests run concurrently
- Tests passing individually but failing in parallel execution
- Inconsistent test results across different runs

### The Solution
We created `test_persistence_integration_clean.py` which:

1. **Uses Server's Working Directory**: Correctly calculates the RDB file path relative to the server
2. **Thread-Safe Operations**: Uses a global lock for RDB file operations
3. **Proper Cleanup**: Ensures RDB files are cleaned up after each test
4. **No CONFIG Dependencies**: Works with the default RDB configuration

### Key Improvements

```python
# Calculate server directory correctly
server_dir = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
rdb_path = os.path.join(server_dir, "dump.rdb")

# Thread-safe RDB operations
with LOCK:
    # Clean up before test
    if os.path.exists(rdb_path):
        os.remove(rdb_path)
    
    # Run test...
    
    # Clean up after test
    if os.path.exists(rdb_path):
        os.remove(rdb_path)
```

## Running the Tests

### Individual Test
```bash
python3 tests/features/persistence/test_persistence_integration_clean.py
```

### As Part of Full Suite
```bash
./run_tests.sh default
```

## Test Cases

1. **test_rdb_save_load**: Tests synchronous SAVE command
2. **test_background_save**: Tests asynchronous BGSAVE command  
3. **test_save_conflict**: Tests concurrent save operations handling
4. **test_data_types_persistence**: Tests persistence of all Redis data types

## Future Improvements

1. **CONFIG SET Support**: Adding support for dynamic RDB path configuration would allow better test isolation
2. **Unique Test Paths**: Once CONFIG SET is supported, each test could use a unique RDB filename
3. **Parallel Test Execution**: With proper isolation, tests could run fully in parallel

## Notes

- The server must be running before executing tests
- Tests assume the server is on localhost:6379
- All tests perform cleanup to avoid interference
- The lock ensures serial execution of critical sections