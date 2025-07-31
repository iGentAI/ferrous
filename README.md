# ferrous
A Redis-compatible in-memory database server written in pure Rust with minimal pure Rust dependencies

## Project Status

Ferrous has achieved **significant architectural progress** with a working Lua 5.1 interpreter implementation, but has encountered **fundamental architectural constraints** that prevent completion without redesign.

### Current Status (17/27 Tests Passing - 63% Success Rate):
- ✅ TCP Server with connection handling
- ✅ Full RESP2 protocol implementation
- ✅ Core data structures: Strings, Lists, Sets, Hashes, Sorted Sets
- ✅ Basic key operations: GET, SET, DEL, EXISTS, EXPIRE, TTL, etc.
- ✅ RDB persistence (SAVE, BGSAVE)
- ✅ Pub/Sub messaging system
- ✅ Transaction support (MULTI/EXEC/DISCARD/WATCH)
- ✅ AOF persistence
- ⚠️ **Lua VM with architectural limitations** (see Critical Limitation below)

### Lua 5.1 Implementation Achievements:
- ✅ Complete bytecode generation and VM execution pipeline
- ✅ Table constructor value preservation (fixed corruption issues)
- ✅ Iterator protocol implementation (TFORLOOP opcode working)
- ✅ Canonical register allocation (specification-compliant)
- ✅ RETURN opcode with proper upvalue closing
- ✅ Environment handling and global variable access
- ✅ Basic closure support and upvalue capture
- ✅ Standard library core functions (print, type, tostring, etc.)
- ✅ Function definitions, calls, and returns
- ✅ Control flow: for loops, while loops, conditionals

### ⚠️ **CRITICAL LIMITATION: Architectural Constraint**

**The current Lua implementation has hit a fundamental architectural barrier that prevents completion:**

The VM design uses an **ExecutionContext pattern that requires simultaneous mutable access** to VM state from both:
1. VM execution (during opcode processing)
2. Standard library functions (for `pcall`, `table_next`, etc.)

This creates a **dual-mutability anti-pattern** that violates Rust's ownership model, causing compilation errors:
```
error[E0596]: cannot borrow `*self.vm` as mutable, as it is behind a `&` reference
```

**This is not a bug—it's a fundamental design flaw** that prevents completion of:
- Advanced standard library functions (`pcall`, `xpcall`, complex metamethods)
- VM-stdlib integration scenarios (where remaining test failures occur)
- Full Lua 5.1 specification compliance

### Architectural Solutions Required

Research into successful pure Rust Lua interpreters (Piccolo, mlua) reveals proven patterns:

1. **VM-Mediated Operations** (Piccolo's Sequence pattern)
   - Standard library functions return operation descriptions
   - VM executes operations with exclusive mutable control
   - Eliminates dual-mutability deadlock

2. **Proxy Patterns** (mlua's approach)
   - Controlled access through proxy objects
   - Cell-based interior mutability with runtime checking

3. **Callback-Based Architecture**
   - VM provides callbacks to standard library functions
   - Avoids direct state mutation conflicts

### Known Limitations:
- ❌ **Architectural redesign required** for full Lua 5.1 compliance
- ❌ Standard library functions requiring VM interaction incomplete
- ❌ Complex closure scenarios show contamination in integration tests
- ❌ Cannot complete ExecutionContext implementation in current architecture

### For Developers and Contributors

**If you're considering using or contributing to this project:**

1. **Current State**: The interpreter has extraordinary individual component success but cannot achieve full system integration
2. **Root Cause**: Fundamental architecture violates Rust's ownership constraints
3. **Solution**: Requires architectural redesign using proven patterns from successful Rust Lua interpreters
4. **Effort**: Significant refactoring needed, not incremental fixes

**The project demonstrates that Lua 5.1 implementation in Rust is absolutely possible** (evidenced by successful projects like Piccolo), but requires architectures that work **with** Rust's ownership model, not against it.

## Performance

Current benchmarks show Ferrous achieving impressive performance for the implemented features:

### Production Build Performance (vs Valkey 8.0.3):

| Operation | Ferrous (Release) | Valkey | Ratio |
|-----------|-------------------|---------|-------|
| **PING_INLINE** | 84,961 ops/sec | 73,637 ops/sec | **115%** ✅ |
| **PING_MBULK** | 86,880 ops/sec | 74,128 ops/sec | **117%** ✅ |
| **SET** | 84,889 ops/sec | 74,515 ops/sec | **114%** ✅ |
| **GET** | 69,881 ops/sec | 63,451 ops/sec | **110%** ✅ |

*Note: Performance testing focused on core Redis operations. Lua performance constrained by architectural limitations.*

## Dependencies

Ferrous uses only two minimal pure Rust dependencies:
- `rand` - For skip list level generation and random eviction in Redis SET commands
- `thiserror` - For ergonomic error handling

Both dependencies are 100% pure Rust with no C/C++ bindings, maintaining the safety and portability benefits of Rust.

## Building and Running

```bash
# Build the project
cargo build

# Run the server
cargo run

# Build with optimizations for better performance
cargo build --release
```

## Testing

### Core Database Testing
```bash
# Run basic functionality tests
./test_basic.sh

# Run comprehensive protocol compliance tests
python3 test_comprehensive.py

# Run Redis command tests
./test_commands.sh

# Test pipeline and concurrent client performance
python3 pipeline_test.py

# Run performance benchmarks
./test_benchmark.sh
```

### Lua Implementation Testing
```bash
# Run Lua test suite (current: 17/27 tests passing)
./test_suite.sh

# Run enhanced validation suite
./run_validation_suite.sh

# Test specific Lua functionality
./target/release/compile_and_execute tests/lua/basic/arithmetic.lua
```

## Contributing

**Before contributing to the Lua implementation:**

1. **Understand the architectural constraint** - review `docs/ARCHITECTURE.md` for details
2. **Consider architectural solutions** - research Piccolo's Sequence pattern or mlua's proxy patterns
3. **Focus on architectural redesign** rather than incremental fixes
4. **Review existing analysis** - extensive debugging documentation in `desktop/complex_reasoning/`

The project needs **architectural leadership** more than incremental bug fixes.

## Documentation

- `docs/ARCHITECTURE.md` - Detailed architectural analysis including limitations
- `docs/LUA_IMPLEMENTATION_STATUS.md` - Current Lua implementation state and test results
- `docs/LUA_C_FUNCTION_INTERFACE_CONVENTIONS.md` - Interface design (currently blocked)

## Running Multiple Instances for Replication

```bash
# Start the master
./target/release/ferrous master.conf

# Start the replica
./target/release/ferrous replica.conf
```

## Project Vision

Ferrous demonstrates that **high-performance, pure Rust Redis implementation is achievable**, but completing the Lua integration requires architectural patterns that respect Rust's ownership model. The project serves as both a working Redis server and a case study in Rust language interpreter design challenges and solutions.