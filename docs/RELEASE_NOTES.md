# Ferrous Release Notes

## Version 0.1.0 (June 25, 2025)

### Major Features

#### Lua Scripting with GIL Implementation
- Added complete Lua scripting support with Redis compatibility
- Implemented Global Interpreter Lock (GIL) for atomic script execution
- Fixed critical issues with KEYS/ARGV access and redis.call/pcall functions
- Added transaction-like semantics for script operations
- Implemented proper error handling and propagation
- Added script kill and timeout functionality
- Created comprehensive test suite for Lua functionality
- Successfully implemented cjson library with encode/decode support
- Added table operations with full concatenation support

#### Master-Slave Replication
- Added complete master-slave replication support
- Implemented REPLICAOF/SLAVEOF command for dynamic role configuration
- Added full synchronization with RDB transfer
- Implemented command propagation from master to replicas
- Added authentication support for secure replication
- Implemented role transitions (master ↔ replica)
- Added replication backlog for command tracking
- Created comprehensive test suite for replication functionality

#### Configuration System
- Added support for both configuration files and command-line arguments
- Created Redis-compatible configuration file parser
- Added CLI argument handling with option overrides
- Implemented multi-instance configurations with separate master/replica configs
- Added proper directory management for data files

#### Improved Protocol Handling
- Enhanced RESP protocol parsing
- Added support for RESP3 protocol features
- Improved error handling for protocol errors
- Added robust handling of bulk strings for replication data

#### SCAN Command Family
- Implemented SCAN for safe key space iteration
- Added HSCAN for hash field scanning
- Added SSCAN for set member scanning
- Added ZSCAN for sorted set scanning

### Performance Improvements
- Optimized command propagation to replicas
- Improved connection handling for concurrent clients
- Enhanced RDB file generation and transfer
- Performance maintained with replication enabled
- Optimized Lua script execution with GIL approach

### Test Suite Enhancements
- Added Lua scripting test suite (test_lua_gil.py)
- Added replication-specific test script (test_replication.sh)
- Enhanced protocol fuzzing tests
- Added comprehensive benchmark tests for replication scenarios
- Added tests for configuration parsing and handling

### Documentation
- Updated documentation to reflect Lua GIL architecture
- Added Lua compatibility report
- Updated documentation to reflect replication architecture
- Added operational guides for multi-instance setup
- Updated roadmap to reflect completion of high-priority features
- Added design details for the replication subsystem

### Bug Fixes
- Fixed critical issues with Lua KEYS/ARGV access
- Fixed crashes in redis.call/redis.pcall functions
- Improved handling of non-blocking I/O in replication client
- Fixed RDB transfer protocol implementation
- Enhanced error recovery with proper backoff

## Next Steps
The next planned features include:
- Refinement of Lua transaction rollback mechanism
- Production monitoring improvements (INFO enhancements)
- Enhanced security features
- Extended data type operations
- Partial synchronization for more efficient replication

## Credits
Lua GIL implementation, replication implementation, and configuration enhancements by the Ferrous team.