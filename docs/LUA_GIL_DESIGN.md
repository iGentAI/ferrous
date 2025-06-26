# Ferrous Lua Global Interpreter Lock (GIL) Design

## Executive Summary

This document outlines a comprehensive redesign of the Lua execution model in Ferrous Redis to address critical stability issues, particularly the failures when accessing KEYS/ARGV arrays or using redis.call()/redis.pcall() functions. The core approach is to implement a Global Interpreter Lock (GIL) with transaction-like semantics to ensure atomic script execution and robust state management, directly matching Redis's approach to Lua scripting.

The current implementation has made significant progress with the generational arena architecture and VM isolation improvements. However, persistent issues at the interface between the Lua VM and the Redis storage engine continue to cause crashes. These issues stem from context preservation problems during execution lifecycle, particularly when crossing boundary layers between VM execution and storage operations.

The GIL-based design resolves these issues by ensuring complete atomicity of script execution, proper context preservation throughout the lifecycle, and transaction-like semantics for all Redis operations.

## Implementation Status (Updated June 2025)

The GIL implementation has been successfully integrated into Ferrous, with the following accomplishments:

1. ✅ **Core GIL Infrastructure** - Implemented the basic locking mechanism to ensure atomic script execution
2. ✅ **VM Isolation** - Each script now executes in a clean VM environment
3. ✅ **Context Preservation** - Fixed the context management issues that were causing crashes
4. ✅ **KEYS/ARGV Access** - Successfully implemented stable access to KEYS and ARGV arrays
5. ✅ **redis.call/pcall** - Fixed the implementation of Redis API functions
6. ✅ **Error Handling** - Improved error propagation and script kill functionality
7. 🟡 **Transaction Semantics** - Basic transaction support implemented, but rollback on error needs improvement
8. 🟡 **Timeout Handling** - Implemented but needs refinement for edge cases

## 1. Design Principles

The GIL implementation is guided by the following principles:

1. **Complete Atomicity**: Lua scripts must execute atomically, with no interleaving of commands from different clients.
2. **State Consistency**: All Redis state must remain consistent throughout script execution.
3. **Transparent Transactions**: Scripts should have transaction-like semantics without requiring explicit MULTI/EXEC.
4. **Graceful Error Handling**: Errors should be properly propagated without crashing the server.
5. **Predictable Timeouts**: Script execution should be time-bound with clean termination.

## 2. Core Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Script Executor                           │
├─────────────────────────────────────────────────────────────┤
│                     Global Interpreter Lock                  │
├─────────────────────────────────────────────────────────────┤
│                    Transaction Manager                       │
├─────────────────────────────────────────────────────────────┤
│ ┌─────────────┐ ┌─────────────┐ ┌─────────────────────────┐ │
│ │ Script      │ │ LuaVM       │ │ Transactional Storage   │ │
│ │ Cache       │ │ Instance    │ │ Proxy                   │ │
│ └─────────────┘ └─────────────┘ └─────────────────────────┘ │
├─────────────────────────────────────────────────────────────┤
│                    Redis Storage Engine                      │
└─────────────────────────────────────────────────────────────┘
```

### 2.1 Global Interpreter Lock (GIL)

The GIL is a mutex ensuring exclusive access to the Redis instance during script execution, making all script operations atomic.

```rust
pub struct LuaGIL {
    // Mutex protecting script execution
    execution_lock: Arc<Mutex<()>>,
    
    // Currently executing script info
    current_execution: Arc<RwLock<Option<ExecutionInfo>>>,
    
    // Script execution queue (optional)
    execution_queue: Arc<Mutex<VecDeque<ExecutionRequest>>>,
    
    // VM instance (single, protected)
    vm_instance: Arc<Mutex<LuaVMInstance>>,
    
    // Transaction manager
    transaction_manager: Arc<TransactionManager>,
}
```

### 2.2 Transaction Manager

The Transaction Manager provides transaction-like semantics for Redis operations in a script:

```rust
pub struct TransactionManager {
    active_transaction: Arc<RwLock<Option<Transaction>>>,
    storage_engine: Arc<StorageEngine>,
    operation_log: Arc<Mutex<Vec<Operation>>>,
}

pub struct Transaction {
    id: Uuid,
    db: usize,
    operations: Vec<Operation>,
    start_time: Instant,
}

pub enum Operation {
    Set { db: usize, key: Vec<u8>, value: Vec<u8>, old_value: Option<Vec<u8>> },
    Delete { db: usize, key: Vec<u8>, old_value: Option<Vec<u8>> },
    Incr { db: usize, key: Vec<u8>, delta: i64, old_value: i64 },
    // ... other operations
}
```

### 2.3 LuaVM Instance

Instead of VM pooling, a single VM instance is reused with proper cleanup between operations:

```rust
pub struct LuaVMInstance {
    vm: LuaVM,
    context_stack: Vec<ScriptContext>,
    global_state: GlobalVMState,
}

pub struct ScriptContext {
    keys: Vec<Vec<u8>>,
    args: Vec<Vec<u8>>,
    db: usize,
    transaction_id: Uuid,
    storage_proxy: TransactionalStorageProxy,
}
```

### 2.4 Transactional Storage Proxy

The proxy intercepts all Redis operations from the script and routes them through the transaction manager:

```rust
pub struct TransactionalStorageProxy {
    transaction_manager: Arc<TransactionManager>,
    transaction: Transaction,
    storage: Arc<StorageEngine>,
}
```

## Remaining Work

While the implementation is nearly complete, there are still a few items to address:

1. **Transaction Rollback Refinement**: Improve the rollback mechanism to handle all error cases properly
2. **Timeout Configuration**: Make timeouts configurable through the Redis configuration system
3. **Performance Optimization**: Fine-tune the transaction logging for better performance
4. **Edge Case Testing**: Additional testing for complex scenarios and error conditions

## Conclusion

The GIL-based implementation has successfully resolved the critical issues with the Lua script execution in Ferrous, providing a robust and Redis-compatible scripting environment. With the remaining items addressed, the Lua layer will be complete and ready for production use.