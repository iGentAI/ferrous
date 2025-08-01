# Ferrous Release Notes

## Version 0.1.1 (July 31, 2025) - Global Lua Script Cache & Performance Improvements

### Major Features

#### Global Lua Script Cache Implementation
- **Fixed SCRIPT LOAD/EVALSHA cross-connection compatibility** - scripts loaded on one connection now available via EVALSHA on any connection
- **Implemented zero-overhead lazy locking** - script cache locks only acquired for Lua operations (EVAL, EVALSHA, SCRIPT commands)
- **Removed per-connection script caches** - replaced with thread-safe global cache using Arc<RwLock<HashMap>>
- **Trait-based ScriptCaching abstraction** - follows same zero-overhead pattern as monitoring system
- **Redis-standard behavior** - full compatibility with Redis Lua script caching semantics

#### Performance Achievements vs Valkey 8.0.4
- **Exceeds Valkey performance** in 8 out of 9 core operations (106-126% throughput)
- **110% GET performance** - 81,301 ops/sec vs 74,074 ops/sec
- **126% LPOP performance** - 78,740 ops/sec vs 62,500 ops/sec  
- **112% PING performance** - 81,967 ops/sec vs 72,993 ops/sec
- **Equal pipelined performance** - both achieve 769,231 ops/sec peak throughput
- **Lower latencies** - p50 latencies 5-10% better than Valkey (0.287ms vs 0.319ms)

#### Test Infrastructure Improvements
- **Aligned test configurations** - removed authentication expectations from default test scripts
- **Fixed test/server configuration mismatch** - all basic tests now run without authentication errors
- **Maintained auth-enabled test scenarios** - replication tests still use authentication as intended
- **Improved test reliability** - comprehensive protocol tests now pass all scenarios including multi-client

### Performance Improvements
- **Zero-overhead global script cache** - no performance impact on non-Lua operations
- **Optimized connection handling** - removed per-connection script cache overhead
- **Enhanced logging performance** - proper log redirection can double SET operation throughput
- **Improved concurrent client performance** - 113% of Valkey performance with 50 concurrent clients

### Bug Fixes
- **Fixed EVALSHA cross-connection failures** - scripts are now globally accessible
- **Resolved script cache visibility issues** - eliminated per-connection cache isolation
- **Fixed test authentication mismatches** - tests now align with default server configuration
- **Enhanced script caching reliability** - proper error handling and lock management

### Technical Architecture
- **Trait-based lazy locking** - ScriptCaching trait provides zero-overhead abstraction
- **Global state management** - Arc<dyn ScriptCaching> shared across all connections
- **Consistent with monitoring system** - follows established zero-overhead patterns
- **Thread-safe implementation** - RwLock provides concurrent read access with exclusive writes

### Validation Results
- **All unit tests pass** - 57 Rust unit/integration tests completed successfully
- **All Lua tests pass** - end-to-end and integration Lua scripting tests pass
- **Protocol compliance verified** - comprehensive RESP protocol tests pass
- **Performance benchmarked** - thorough comparison against Valkey 8.0.4 completed

## Next Steps
- **Cluster support** implementation for horizontal scaling
- **HyperLogLog** data structure implementation  
- **Advanced monitoring** enhancements for production deployment
- **Memory optimization** for large-scale deployments

## Credits
Global Lua script cache implementation, performance optimization, and test infrastructure improvements by the Ferrous team.

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