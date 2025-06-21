//! Error types for the Lua interpreter

use std::fmt;

/// Result type for Lua operations
pub type Result<T> = std::result::Result<T, LuaError>;

/// Errors that can occur in the Lua interpreter
#[derive(Debug, Clone)]
pub enum LuaError {
    /// Syntax error during parsing
    Syntax(String),
    
    /// Runtime error during execution
    Runtime(String),
    
    /// Type mismatch error
    TypeError(String),
    
    /// Memory limit exceeded
    MemoryLimit,
    
    /// Instruction limit exceeded
    InstructionLimit,
    
    /// Stack overflow
    StackOverflow,
    
    /// Attempt to use unsafe/disallowed function
    SecurityViolation(String),
    
    /// Script compilation failed
    CompileError(String),
    
    /// Invalid bytecode
    InvalidBytecode,
    
    /// Script not found in cache (for EVALSHA)
    ScriptNotFound,
}

impl fmt::Display for LuaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LuaError::Syntax(msg) => write!(f, "Lua syntax error: {}", msg),
            LuaError::Runtime(msg) => write!(f, "Lua runtime error: {}", msg),
            LuaError::TypeError(msg) => write!(f, "Lua type error: {}", msg),
            LuaError::MemoryLimit => write!(f, "Lua memory limit exceeded"),
            LuaError::InstructionLimit => write!(f, "Lua instruction limit exceeded"),
            LuaError::StackOverflow => write!(f, "Lua stack overflow"),
            LuaError::SecurityViolation(msg) => write!(f, "Lua security violation: {}", msg),
            LuaError::CompileError(msg) => write!(f, "Lua compile error: {}", msg),
            LuaError::InvalidBytecode => write!(f, "Invalid Lua bytecode"),
            LuaError::ScriptNotFound => write!(f, "NOSCRIPT No matching script. Please use EVAL."),
        }
    }
}

impl std::error::Error for LuaError {}

/// Convert Lua errors to Ferrous errors for integration
impl From<LuaError> for crate::error::FerrousError {
    fn from(err: LuaError) -> Self {
        match err {
            LuaError::ScriptNotFound => crate::error::FerrousError::Script(crate::error::ScriptError::NotFound),
            _ => crate::error::FerrousError::Script(crate::error::ScriptError::ExecutionError(err.to_string())),
        }
    }
}