//! Lua VM Error Types
//! 
//! This module defines error types specific to the Lua VM implementation.

use std::fmt;
use std::error::Error;

/// Result type for Lua operations
pub type LuaResult<T> = Result<T, LuaError>;

/// Information about a stack frame for error reporting
#[derive(Debug, Clone, PartialEq)]
pub struct CallInfo {
    /// Name of the function if available
    pub function_name: Option<String>,
    
    /// Source file name if available
    pub source_file: Option<String>,
    
    /// Line number if available
    pub line_number: Option<usize>,
    
    /// PC (program counter) value
    pub pc: usize,
}

/// Traceback information for error reporting
#[derive(Debug, Clone, PartialEq)]
pub struct LuaTraceback {
    /// Call frames in the traceback
    pub frames: Vec<CallInfo>,
}

impl fmt::Display for LuaTraceback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "stack traceback:")?;
        for (i, frame) in self.frames.iter().rev().enumerate() {
            let location = if let (Some(file), Some(line)) = (&frame.source_file, frame.line_number) {
                format!("{}:{}", file, line)
            } else if let Some(file) = &frame.source_file {
                file.clone()
            } else {
                format!("[pc={}]", frame.pc)
            };
            
            let name = frame.function_name.as_deref().unwrap_or("<anonymous>");
            writeln!(f, "\t{}: in function '{}'", location, name)?;
        }
        Ok(())
    }
}

/// Errors that can occur in the Lua VM
#[derive(Debug, Clone, PartialEq)]
pub enum LuaError {
    // Syntax and compilation errors
    SyntaxError { message: String, line: usize, column: usize },
    CompileError(String),
    
    // Runtime errors
    RuntimeError(String),
    RuntimeErrorWithTrace { message: String, traceback: LuaTraceback },
    TypeError { expected: String, got: String },
    TypeErrorWithTrace { expected: String, got: String, traceback: LuaTraceback },
    ArithmeticError(String),
    ConcatenationError(String),
    ComparisonError(String),
    
    // Memory and resource errors
    OutOfMemory,
    StackOverflow,
    InstructionLimitExceeded,
    Timeout,
    ResourceExhausted { 
        resource: String, 
        limit: usize, 
        attempted: usize 
    },
    ResourceLimit {
        resource: String,
        limit: usize,
        used: usize,
        context: String,
    },
    
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
    DetailedTransactionStateError {
        current_state: String,
        expected_state: String,
        operation: String,
        location: String,
    },
    
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
            LuaError::RuntimeErrorWithTrace { message, traceback } => {
                writeln!(f, "runtime error: {}", message)?;
                write!(f, "{}", traceback)
            }
            LuaError::TypeError { expected, got } => {
                write!(f, "type error: expected {}, got {}", expected, got)
            }
            LuaError::TypeErrorWithTrace { expected, got, traceback } => {
                writeln!(f, "type error: expected {}, got {}", expected, got)?;
                write!(f, "{}", traceback)
            }
            LuaError::ArithmeticError(msg) => write!(f, "arithmetic error: {}", msg),
            LuaError::ConcatenationError(msg) => write!(f, "concatenation error: {}", msg),
            LuaError::ComparisonError(msg) => write!(f, "comparison error: {}", msg),
            
            LuaError::OutOfMemory => write!(f, "out of memory"),
            LuaError::StackOverflow => write!(f, "stack overflow"),
            LuaError::InstructionLimitExceeded => write!(f, "instruction limit exceeded"),
            LuaError::Timeout => write!(f, "script execution timeout"),
            LuaError::ResourceExhausted { resource, limit, attempted } => {
                write!(f, "resource exhausted: {} limit {} exceeded (attempted {})", 
                       resource, limit, attempted)
            }
            LuaError::ResourceLimit { resource, limit, used, context } => {
                write!(f, "resource limit exceeded: {} (limit: {}, used: {}) - {}", 
                       resource, limit, used, context)
            }
            
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
            LuaError::DetailedTransactionStateError { current_state, expected_state, operation, location } => {
                write!(f, "transaction state error: expected state '{}' but was '{}' when performing '{}' at '{}'", 
                       expected_state, current_state, operation, location)
            }
            
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
            LuaError::RuntimeErrorWithTrace { message, traceback } => {
                crate::error::FerrousError::LuaRuntimeError(format!("{}\n{}", message, traceback))
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
    
    #[test]
    fn test_traceback_display() {
        let frames = vec![
            CallInfo {
                function_name: Some("main".to_string()),
                source_file: Some("test.lua".to_string()),
                line_number: Some(10),
                pc: 5,
            },
            CallInfo {
                function_name: Some("helper".to_string()),
                source_file: Some("utils.lua".to_string()),
                line_number: Some(42),
                pc: 12,
            },
        ];
        
        let traceback = LuaTraceback { frames };
        
        let err = LuaError::RuntimeErrorWithTrace {
            message: "test error".to_string(),
            traceback,
        };
        
        // Assert that the traceback format is as expected
        let err_string = err.to_string();
        assert!(err_string.contains("runtime error: test error"));
        assert!(err_string.contains("stack traceback:"));
        assert!(err_string.contains("utils.lua:42: in function 'helper'"));
        assert!(err_string.contains("test.lua:10: in function 'main'"));
    }
    
    #[test]
    fn test_type_error_with_trace() {
        let frames = vec![
            CallInfo {
                function_name: None,
                source_file: None,
                line_number: None,
                pc: 7,
            },
        ];
        
        let traceback = LuaTraceback { frames };
        
        let err = LuaError::TypeErrorWithTrace {
            expected: "table".to_string(),
            got: "nil".to_string(),
            traceback,
        };
        
        let err_string = err.to_string();
        assert!(err_string.contains("type error: expected table, got nil"));
        assert!(err_string.contains("stack traceback:"));
        assert!(err_string.contains("[pc=7]")); // When no file/line available
    }
    
    #[test]
    fn test_detailed_transaction_state_error() {
        let err = LuaError::DetailedTransactionStateError {
            current_state: "Committed".to_string(),
            expected_state: "Active".to_string(),
            operation: "set_register".to_string(),
            location: "vm.rs:1234".to_string(),
        };
        
        let err_string = err.to_string();
        assert!(err_string.contains("transaction state error:"));
        assert!(err_string.contains("expected state 'Active'"));
        assert!(err_string.contains("was 'Committed'"));
        assert!(err_string.contains("performing 'set_register'"));
        assert!(err_string.contains("at 'vm.rs:1234'"));
    }
}