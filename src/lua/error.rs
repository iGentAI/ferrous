//! Lua VM Error Types
//!
//! This module defines all the error types that can occur in the Lua VM.

use std::fmt;
use std::error::Error as StdError;

/// Result type for Lua operations
pub type Result<T> = std::result::Result<T, LuaError>;

/// A Lua error
#[derive(Clone, Debug)]
pub enum LuaError {
    /// Syntax error during compilation
    SyntaxError {
        /// Error message
        message: String,
        
        /// Line number
        line: usize,
        
        /// Column number
        column: usize,
    },
    
    /// Type error
    TypeError(String),
    
    /// Runtime error
    RuntimeError(String),
    
    /// Invalid handle
    InvalidHandle,
    
    /// Stale handle
    StaleHandle(u32, u32), // index, generation
    
    /// Out of memory
    OutOfMemory,
    
    /// Stack overflow
    StackOverflow,
    
    /// Instruction limit exceeded
    InstructionLimit,
    
    /// Memory limit exceeded
    MemoryLimit,
    
    /// Table overflow
    TableOverflow,
    
    /// String too long
    StringTooLong,
    
    /// Invalid upvalue
    InvalidUpvalue,
    
    /// Invalid bytecode
    InvalidBytecode(String),
    
    /// Invalid encoding
    InvalidEncoding,
    
    /// Invalid operation
    InvalidOperation(String),
    
    /// Script killed
    ScriptKilled,
    
    /// Not implemented
    NotImplemented(String),
    
    /// Internal error
    InternalError(String),
    
    /// Table key not found
    TableKeyNotFound,
    
    /// Stack empty
    StackEmpty,
    
    /// Argument error
    ArgError(usize, String),
}

impl fmt::Display for LuaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LuaError::SyntaxError { message, line, column } => {
                write!(f, "syntax error at line {}, column {}: {}", line, column, message)
            }
            LuaError::TypeError(msg) => {
                write!(f, "type error: {}", msg)
            }
            LuaError::RuntimeError(msg) => {
                write!(f, "{}", msg)
            }
            LuaError::InvalidHandle => {
                write!(f, "invalid handle")
            }
            LuaError::StaleHandle(index, generation) => {
                write!(f, "stale handle (index: {}, generation: {})", index, generation)
            }
            LuaError::OutOfMemory => {
                write!(f, "out of memory")
            }
            LuaError::StackOverflow => {
                write!(f, "stack overflow")
            }
            LuaError::InstructionLimit => {
                write!(f, "instruction limit exceeded")
            }
            LuaError::MemoryLimit => {
                write!(f, "memory limit exceeded")
            }
            LuaError::TableOverflow => {
                write!(f, "table overflow")
            }
            LuaError::StringTooLong => {
                write!(f, "string too long")
            }
            LuaError::InvalidUpvalue => {
                write!(f, "invalid upvalue")
            }
            LuaError::InvalidBytecode(msg) => {
                write!(f, "invalid bytecode: {}", msg)
            }
            LuaError::InvalidEncoding => {
                write!(f, "invalid encoding")
            }
            LuaError::InvalidOperation(msg) => {
                write!(f, "invalid operation: {}", msg)
            }
            LuaError::ScriptKilled => {
                write!(f, "script killed")
            }
            LuaError::NotImplemented(feature) => {
                write!(f, "not implemented: {}", feature)
            }
            LuaError::InternalError(msg) => {
                write!(f, "internal error: {}", msg)
            }
            LuaError::TableKeyNotFound => {
                write!(f, "table key not found")
            }
            LuaError::StackEmpty => {
                write!(f, "stack empty")
            }
            LuaError::ArgError(index, msg) => {
                write!(f, "bad argument #{}: {}", index, msg)
            }
        }
    }
}

impl StdError for LuaError {}

impl From<LuaError> for crate::error::FerrousError {
    fn from(err: LuaError) -> Self {
        crate::error::FerrousError::LuaError(err.to_string())
    }
}

/// Create a syntax error
pub fn syntax_error(message: &str, line: usize, column: usize) -> LuaError {
    LuaError::SyntaxError {
        message: message.to_string(),
        line,
        column,
    }
}

/// Create a type error
pub fn type_error(expected: &str, got: &str) -> LuaError {
    LuaError::TypeError(format!("expected {}, got {}", expected, got))
}