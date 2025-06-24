//! Lua 5.1 interpreter for Ferrous - Generational Arena Architecture
//! 
//! This module provides a Redis-compatible Lua scripting engine with:
//! - Zero external dependencies beyond standard library
//! - Full sandboxing and security restrictions
//! - Redis API compatibility (redis.call, redis.pcall, etc.)
//! - Memory and CPU usage limits
//! - Generational arena for efficient memory management

pub mod arena;
pub mod error;
pub mod executor;
pub mod heap;
pub mod value;
pub mod vm;
pub mod command;
pub mod redis_api;
pub mod sandbox;
pub mod parser;
pub mod compiler;
pub mod ast;
pub mod lexer;

pub use error::{LuaError, Result};
pub use value::Value;
pub use vm::LuaVM;
pub use executor::ScriptExecutor;
pub use heap::LuaHeap;
pub use sandbox::LuaSandbox;
pub use parser::Parser;
pub use compiler::Compiler;

/// Resource limits for Lua scripts
#[derive(Debug, Clone)]
pub struct LuaLimits {
    /// Maximum memory in bytes (default: 64MB)
    pub memory_limit: usize,
    
    /// Maximum instructions to execute (default: 100M)
    pub instruction_limit: u64,
    
    /// Maximum call stack depth
    pub call_stack_limit: usize,
    
    /// Maximum value stack size
    pub value_stack_limit: usize,
    
    /// Maximum table size
    pub table_limit: usize,
}

impl Default for LuaLimits {
    fn default() -> Self {
        LuaLimits {
            memory_limit: 64 * 1024 * 1024,    // 64MB
            instruction_limit: 100_000_000,     // 100M instructions
            call_stack_limit: 1000,             // 1000 calls max
            value_stack_limit: 100_000,         // 100K stack slots
            table_limit: 1_000_000,             // 1M entries max
        }
    }
}

/// VM configuration
#[derive(Debug, Clone)]
pub struct VMConfig {
    /// Enable deterministic mode (no randomness)
    pub deterministic: bool,
    
    /// Enable debug output
    pub debug: bool,
    
    /// Resource limits
    pub limits: LuaLimits,
}

impl Default for VMConfig {
    fn default() -> Self {
        VMConfig {
            deterministic: true,  // Redis requires determinism
            debug: false,
            limits: LuaLimits::default(),
        }
    }
}