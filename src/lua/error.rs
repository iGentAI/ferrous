//! Lua VM Error Types
//! 
//! This module defines error types specific to the Lua VM implementation.

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

impl std::fmt::Display for LuaTraceback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
    
    // Type errors
    TypeError { expected: String, got: String },
    TypeErrorWithTrace { expected: String, got: String, traceback: LuaTraceback },
    ArithmeticError(String),
    ConcatenationError(String),
    ComparisonError(String),
    
    // Argument errors
    ArgumentError { expected: usize, got: usize },
    BadArgument { func: Option<String>, arg: i32, msg: String },
    
    // Memory and resource errors
    OutOfMemory,
    /// Stack overflow error (exceeding Lua's max stack size)
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
    CallError(String),
    UndefinedGlobal(String),
    
    // Not implemented or internal errors
    NotImplemented(String),
    InternalError(String),
    
    // RefCell borrow error (for RC RefCell heap)
    BorrowError(String),
    
    // Script execution control
    ScriptKilled,
}

impl std::fmt::Display for LuaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
            
            LuaError::ArgumentError { expected, got } => {
                write!(f, "wrong number of arguments: expected {}, got {}", expected, got)
            }
            LuaError::BadArgument { func, arg, msg } => {
                if let Some(func_name) = func {
                    write!(f, "bad argument #{} to '{}': {}", arg, func_name, msg)
                } else {
                    write!(f, "bad argument #{}: {}", arg, msg)
                }
            }
            
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
            
            LuaError::CallError(msg) => write!(f, "call error: {}", msg),
            LuaError::UndefinedGlobal(name) => write!(f, "undefined global '{}'", name),
            
            LuaError::NotImplemented(feature) => write!(f, "not implemented: {}", feature),
            LuaError::InternalError(msg) => write!(f, "internal error: {}", msg),
            LuaError::BorrowError(msg) => write!(f, "borrow error: {}", msg),
            
            LuaError::ScriptKilled => write!(f, "script killed by user"),
        }
    }
}

impl std::error::Error for LuaError {}

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