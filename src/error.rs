//! Error types for Ferrous
//! 
//! This module defines all error types used throughout the Ferrous server.
//! We follow Redis's error conventions where applicable.

use std::fmt;
use std::io;
use std::error::Error as StdError;

/// Main error type for Ferrous operations
#[derive(Debug)]
pub enum FerrousError {
    /// Protocol-related errors (RESP parsing, serialization)
    Protocol(String),
    
    /// Command execution errors
    Command(CommandError),
    
    /// Storage engine errors
    Storage(StorageError),
    
    /// Network/IO errors
    Io(String),
    
    /// Configuration errors
    Config(String),
    
    /// Client connection errors
    Connection(String),
    
    /// Script execution errors
    Script(ScriptError),
    
    /// Lua execution errors
    LuaError(String),
    
    /// Internal server errors
    Internal(String),
}

/// Command-specific errors that map to Redis error responses
#[derive(Debug, Clone)]
pub enum CommandError {
    /// Unknown command
    UnknownCommand(String),
    
    /// Wrong number of arguments for command
    WrongNumberOfArgs(String),
    
    /// Syntax error in command
    SyntaxError(String),
    
    /// Operation against wrong type
    WrongType,
    
    /// Integer overflow
    IntegerOverflow,
    
    /// Value is not an integer or out of range
    NotInteger,
    
    /// Index out of range
    IndexOutOfRange,
    
    /// Key not found
    NoSuchKey,
    
    /// Invalid state for operation
    InvalidState(String),
    
    /// Invalid argument for command
    InvalidArgument,
    
    /// Generic command error with message
    Generic(String),
}

/// Storage-related errors
#[derive(Debug)]
pub enum StorageError {
    /// Out of memory
    OutOfMemory,
    
    /// Key not found
    KeyNotFound,
    
    /// Wrong data type for operation
    WrongType,
    
    /// Database index out of range
    InvalidDatabase,
    
    /// Operation would block but NOWAIT flag was set
    WouldBlock,
}

/// Script execution errors
#[derive(Debug, Clone)]
pub enum ScriptError {
    /// Script not found in cache
    NotFound,
    
    /// Script execution error
    ExecutionError(String),
    
    /// Script compilation error
    CompilationError(String),
    
    /// Script timeout
    Timeout,
    
    /// Script killed
    Killed,
}

/// Type alias for Results throughout Ferrous
pub type Result<T> = std::result::Result<T, FerrousError>;

impl fmt::Display for FerrousError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FerrousError::Protocol(msg) => write!(f, "Protocol error: {}", msg),
            FerrousError::Command(err) => write!(f, "{}", err),
            FerrousError::Storage(err) => write!(f, "{}", err),
            FerrousError::Io(msg) => write!(f, "I/O error: {}", msg),
            FerrousError::Config(msg) => write!(f, "Configuration error: {}", msg),
            FerrousError::Connection(msg) => write!(f, "Connection error: {}", msg),
            FerrousError::Script(err) => write!(f, "{}", err),
            FerrousError::LuaError(msg) => write!(f, "Lua error: {}", msg),
            FerrousError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::UnknownCommand(cmd) => {
                write!(f, "ERR unknown command '{}'", cmd)
            }
            CommandError::WrongNumberOfArgs(cmd) => {
                write!(f, "ERR wrong number of arguments for '{}' command", cmd)
            }
            CommandError::SyntaxError(msg) => write!(f, "ERR syntax error: {}", msg),
            CommandError::WrongType => {
                write!(f, "WRONGTYPE Operation against a key holding the wrong kind of value")
            }
            CommandError::IntegerOverflow => {
                write!(f, "ERR increment or decrement would overflow")
            }
            CommandError::NotInteger => {
                write!(f, "ERR value is not an integer or out of range")
            }
            CommandError::IndexOutOfRange => write!(f, "ERR index out of range"),
            CommandError::NoSuchKey => write!(f, "ERR no such key"),
            CommandError::InvalidState(msg) => {
                write!(f, "ERR {}", msg)
            }
            CommandError::InvalidArgument => {
                write!(f, "ERR invalid argument")
            }
            CommandError::Generic(msg) => {
                write!(f, "ERR {}", msg)
            }
        }
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::OutOfMemory => write!(f, "OOM command not allowed when used memory > 'maxmemory'"),
            StorageError::KeyNotFound => write!(f, "Key not found"),
            StorageError::WrongType => write!(f, "Wrong data type"),
            StorageError::InvalidDatabase => write!(f, "ERR invalid DB index"),
            StorageError::WouldBlock => write!(f, "Would block"),
        }
    }
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptError::NotFound => write!(f, "NOSCRIPT No matching script. Please use EVAL."),
            ScriptError::ExecutionError(msg) => write!(f, "ERR {}", msg),
            ScriptError::CompilationError(msg) => write!(f, "ERR Error compiling script: {}", msg),
            ScriptError::Timeout => write!(f, "ERR Script execution time limit exceeded"),
            ScriptError::Killed => write!(f, "ERR Script killed by user"),
        }
    }
}

impl StdError for FerrousError {}
impl StdError for CommandError {}
impl StdError for StorageError {}
impl StdError for ScriptError {}

// Conversion implementations
impl From<io::Error> for FerrousError {
    fn from(err: io::Error) -> Self {
        FerrousError::Io(err.to_string())
    }
}

impl From<CommandError> for FerrousError {
    fn from(err: CommandError) -> Self {
        FerrousError::Command(err)
    }
}

impl From<StorageError> for FerrousError {
    fn from(err: StorageError) -> Self {
        FerrousError::Storage(err)
    }
}

impl From<ScriptError> for FerrousError {
    fn from(err: ScriptError) -> Self {
        FerrousError::Script(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = CommandError::UnknownCommand("FOOBAR".to_string());
        assert_eq!(err.to_string(), "ERR unknown command 'FOOBAR'");
        
        let err = CommandError::WrongType;
        assert_eq!(err.to_string(), "WRONGTYPE Operation against a key holding the wrong kind of value");
        
        let err = ScriptError::NotFound;
        assert_eq!(err.to_string(), "NOSCRIPT No matching script. Please use EVAL.");
    }
}