//! Lua VM Error Types
//! 
//! This module defines error types specific to the Lua VM implementation.

use std::fmt;
use std::error::Error;

/// Result type for Lua operations
pub type LuaResult<T> = Result<T, LuaError>;

/// Errors that can occur in the Lua VM
#[derive(Debug, Clone, PartialEq)]
pub enum LuaError {
    // Syntax and compilation errors
    SyntaxError { message: String, line: usize, column: usize },
    CompileError(String),
    
    // Runtime errors
    RuntimeError(String),
    TypeError { expected: String, got: String },
    ArithmeticError(String),
    ConcatenationError(String),
    ComparisonError(String),
    
    // Memory and resource errors
    OutOfMemory,
    StackOverflow,
    InstructionLimitExceeded,
    Timeout,
    
    // Handle and validation errors
    InvalidHandle,
    StaleHandle,
    
    // Table and field access errors
    TableIndexError(String),
    MetamethodError(String),
    
    // Function call errors
    ArgumentError { expected: usize, got: usize },
    CallError(String),
    UndefinedGlobal(String),
    
    // Transaction errors
    TransactionError(String),
    InvalidTransactionState,
    
    // C function errors
    CFunctionError(String),
    
    // Internal errors
    InternalError(String),
    NotImplemented(String),
    
    // Script execution control
    ScriptKilled,
}

impl fmt::Display for LuaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LuaError::SyntaxError { message, line, column } => {
                write!(f, "syntax error at line {}:{}: {}", line, column, message)
            }
            LuaError::CompileError(msg) => write!(f, "compile error: {}", msg),
            
            LuaError::RuntimeError(msg) => write!(f, "runtime error: {}", msg),
            LuaError::TypeError { expected, got } => {
                write!(f, "type error: expected {}, got {}", expected, got)
            }
            LuaError::ArithmeticError(msg) => write!(f, "arithmetic error: {}", msg),
            LuaError::ConcatenationError(msg) => write!(f, "concatenation error: {}", msg),
            LuaError::ComparisonError(msg) => write!(f, "comparison error: {}", msg),
            
            LuaError::OutOfMemory => write!(f, "out of memory"),
            LuaError::StackOverflow => write!(f, "stack overflow"),
            LuaError::InstructionLimitExceeded => write!(f, "instruction limit exceeded"),
            LuaError::Timeout => write!(f, "script execution timeout"),
            
            LuaError::InvalidHandle => write!(f, "invalid handle"),
            LuaError::StaleHandle => write!(f, "stale handle (generation mismatch)"),
            
            LuaError::TableIndexError(msg) => write!(f, "table index error: {}", msg),
            LuaError::MetamethodError(msg) => write!(f, "metamethod error: {}", msg),
            
            LuaError::ArgumentError { expected, got } => {
                write!(f, "wrong number of arguments: expected {}, got {}", expected, got)
            }
            LuaError::CallError(msg) => write!(f, "call error: {}", msg),
            LuaError::UndefinedGlobal(name) => write!(f, "undefined global '{}'", name),
            
            LuaError::TransactionError(msg) => write!(f, "transaction error: {}", msg),
            LuaError::InvalidTransactionState => write!(f, "invalid transaction state"),
            
            LuaError::CFunctionError(msg) => write!(f, "C function error: {}", msg),
            
            LuaError::InternalError(msg) => write!(f, "internal error: {}", msg),
            LuaError::NotImplemented(feature) => write!(f, "not implemented: {}", feature),
            
            LuaError::ScriptKilled => write!(f, "script killed by user"),
        }
    }
}

impl Error for LuaError {}

// Conversion to FerrousError
impl From<LuaError> for crate::error::FerrousError {
    fn from(err: LuaError) -> Self {
        match err {
            LuaError::SyntaxError { message, line, column } => {
                crate::error::FerrousError::LuaCompilationError(
                    format!("line {}:{}: {}", line, column, message)
                )
            }
            LuaError::CompileError(msg) => {
                crate::error::FerrousError::LuaCompilationError(msg)
            }
            LuaError::RuntimeError(msg) => {
                crate::error::FerrousError::LuaRuntimeError(msg)
            }
            LuaError::Timeout => {
                crate::error::FerrousError::LuaTimeout
            }
            LuaError::ScriptKilled => {
                crate::error::FerrousError::ScriptKilled
            }
            _ => {
                crate::error::FerrousError::LuaError(err.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_display() {
        let err = LuaError::SyntaxError {
            message: "unexpected token".to_string(),
            line: 10,
            column: 5,
        };
        assert_eq!(
            err.to_string(),
            "syntax error at line 10:5: unexpected token"
        );
        
        let err = LuaError::TypeError {
            expected: "number".to_string(),
            got: "string".to_string(),
        };
        assert_eq!(err.to_string(), "type error: expected number, got string");
        
        let err = LuaError::ArgumentError {
            expected: 2,
            got: 3,
        };
        assert_eq!(
            err.to_string(),
            "wrong number of arguments: expected 2, got 3"
        );
    }
}