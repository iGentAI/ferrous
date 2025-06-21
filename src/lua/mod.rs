//! Lua 5.1 interpreter for Ferrous
//! 
//! This module provides a Redis-compatible Lua scripting engine with:
//! - Zero external dependencies beyond standard library
//! - Full sandboxing and security restrictions
//! - Redis API compatibility (redis.call, redis.pcall, etc.)
//! - Memory and CPU usage limits

pub mod ast;
pub mod command;
pub mod compiler;
pub mod error;
pub mod executor;
pub mod lexer;
pub mod parser;
pub mod simple_test;
pub mod tests;
pub mod value;
pub mod vm;

pub use error::{LuaError, Result};
pub use value::LuaValue;
pub use parser::Parser;
pub use ast::Chunk;
pub use vm::LuaVm;
pub use executor::ScriptExecutor;

/// Resource limits for Lua scripts
#[derive(Debug, Clone)]
pub struct LuaLimits {
    /// Maximum memory in bytes (default: 64MB)
    pub memory_limit: usize,
    
    /// Maximum instructions to execute (default: 100M)
    pub instruction_limit: u64,
    
    /// Stack size limit to prevent overflow
    pub stack_limit: usize,
}

impl Default for LuaLimits {
    fn default() -> Self {
        LuaLimits {
            memory_limit: 64 * 1024 * 1024,    // 64MB
            instruction_limit: 100_000_000,     // 100M instructions
            stack_limit: 1024,                  // 1024 stack slots
        }
    }
}