# Ferrous Release Notes

## Version 0.1.0 (June 20, 2025)

### Major Features

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

### Test Suite Enhancements
- Added replication-specific test script (test_replication.sh)
- Enhanced protocol fuzzing tests
- Added comprehensive benchmark tests for replication scenarios
- Added tests for configuration parsing and handling

### Documentation
- Updated documentation to reflect replication architecture
- Added operational guides for multi-instance setup
- Updated roadmap to reflect completion of high-priority features
- Added design details for the replication subsystem

### Bug Fixes
- Fixed issues with authentication in replication protocol
- Improved handling of non-blocking I/O in replication client
- Fixed RDB transfer protocol implementation
- Enhanced error recovery with proper backoff

## Known Issues
- PING command with too many arguments returns the first argument instead of an error message

## Next Steps
The next planned features include:
- Production monitoring (INFO, SLOWLOG)
- Enhanced security features
- Lua scripting support
- Partial synchronization for more efficient replication

## Credits
Replication implementation and configuration enhancements by the Ferrous team.