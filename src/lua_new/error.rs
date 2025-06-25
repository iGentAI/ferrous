//! Error handling for the Lua interpreter

use std::fmt;
use std::error::Error;

/// Result type for Lua operations
pub type Result<T> = std::result::Result<T, LuaError>;

/// Comprehensive error types for Lua operations
#[derive(Debug, Clone)]
pub enum LuaError {
    /// Error raised by Lua script
    Runtime(String),
    
    /// Type error
    TypeError(String),
    
    /// Syntax error during parsing
    SyntaxError {
        message: String,
        line: usize,
        column: usize,
    },
    
    /// Memory limit exceeded
    MemoryLimit,
    
    /// Instruction limit exceeded
    InstructionLimit,
    
    /// Stack overflow
    StackOverflow,
    
    /// Stack underflow
    StackUnderflow,
    
    /// Invalid handle reference
    InvalidHandle,
    
    /// Invalid operation
    InvalidOperation(String),
    
    /// Invalid program counter
    InvalidProgramCounter,
    
    /// Invalid opcode
    InvalidOpcode(u8),
    
    /// Invalid constant index
    InvalidConstant(usize),
    
    /// Invalid upvalue index
    InvalidUpvalue(usize),
    
    /// Invalid encoding (e.g., non-UTF8 string)
    InvalidEncoding,
    
    /// Feature not implemented
    NotImplemented(&'static str),
    
    /// Resource limit exceeded
    ResourceLimit(String),
    
    /// Script killed
    ScriptKilled,
    
    /// Compilation error
    CompileError(String),
    
    /// Script execution timeout
    Timeout,
    
    /// No context available
    NoContext,
}

impl std::fmt::Display for LuaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LuaError::Runtime(msg) => write!(f, "runtime error: {}", msg),
            LuaError::TypeError(msg) => write!(f, "type error: {}", msg),
            LuaError::SyntaxError { message, line, column } => {
                write!(f, "syntax error at {}:{}: {}", line, column, message)
            }
            LuaError::MemoryLimit => write!(f, "memory limit exceeded"),
            LuaError::InstructionLimit => write!(f, "instruction limit exceeded"),
            LuaError::StackOverflow => write!(f, "stack overflow"),
            LuaError::StackUnderflow => write!(f, "stack underflow"),
            LuaError::InvalidHandle => write!(f, "invalid handle"),
            LuaError::InvalidOperation(msg) => write!(f, "invalid operation: {}", msg),
            LuaError::InvalidProgramCounter => write!(f, "invalid program counter"),
            LuaError::InvalidOpcode(op) => write!(f, "invalid opcode: {}", op),
            LuaError::InvalidConstant(idx) => write!(f, "invalid constant index: {}", idx),
            LuaError::InvalidUpvalue(idx) => write!(f, "invalid upvalue index: {}", idx),
            LuaError::InvalidEncoding => write!(f, "invalid encoding"),
            LuaError::NotImplemented(feature) => write!(f, "not implemented: {}", feature),
            LuaError::ResourceLimit(msg) => write!(f, "resource limit: {}", msg),
            LuaError::ScriptKilled => write!(f, "script killed"),
            LuaError::CompileError(msg) => write!(f, "compile error: {}", msg),
            LuaError::Timeout => write!(f, "script execution timeout"),
            LuaError::NoContext => write!(f, "no script context available"),
        }
    }
}

impl Error for LuaError {}

/// Error context for better debugging
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// Error type and message
    pub error: LuaError,
    
    /// Call stack trace
    pub stack_trace: Vec<StackFrame>,
    
    /// Source location
    pub location: Option<SourceLocation>,
}

/// Stack frame information
#[derive(Debug, Clone)]
pub struct StackFrame {
    /// Function name (if available)
    pub function: Option<String>,
    
    /// Program counter
    pub pc: usize,
    
    /// Function type
    pub function_type: FunctionType,
    
    /// Source location
    pub location: Option<SourceLocation>,
}

/// Function type for stack frames
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionType {
    /// Lua function
    Lua,
    
    /// C function
    C,
    
    /// Main chunk
    Main,
}

/// Source location information
#[derive(Debug, Clone)]
pub struct SourceLocation {
    /// Source file name
    pub file: Option<String>,
    
    /// Line number
    pub line: usize,
    
    /// Column number
    pub column: usize,
}

impl ErrorContext {
    /// Create a new error context
    pub fn new(error: LuaError) -> Self {
        ErrorContext {
            error,
            stack_trace: Vec::new(),
            location: None,
        }
    }
    
    /// Add a stack frame
    pub fn push_frame(&mut self, frame: StackFrame) {
        self.stack_trace.push(frame);
    }
    
    /// Set source location
    pub fn set_location(&mut self, location: SourceLocation) {
        self.location = Some(location);
    }
}

impl fmt::Display for ErrorContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Display main error
        writeln!(f, "Error: {}", self.error)?;
        
        // Display location if available
        if let Some(ref loc) = self.location {
            if let Some(ref file) = loc.file {
                writeln!(f, "  at {}:{}:{}", file, loc.line, loc.column)?;
            } else {
                writeln!(f, "  at line {}:{}", loc.line, loc.column)?;
            }
        }
        
        // Display stack trace
        if !self.stack_trace.is_empty() {
            writeln!(f, "\nStack trace:")?;
            for (i, frame) in self.stack_trace.iter().enumerate() {
                write!(f, "  [{}] ", i)?;
                
                if let Some(ref func) = frame.function {
                    write!(f, "in {}", func)?;
                } else {
                    write!(f, "in <?>")?;
                }
                
                match frame.function_type {
                    FunctionType::Lua => write!(f, " (Lua)")?,
                    FunctionType::C => write!(f, " (C)")?,
                    FunctionType::Main => write!(f, " (main)")?,
                }
                
                if let Some(ref loc) = frame.location {
                    if let Some(ref file) = loc.file {
                        write!(f, " at {}:{}:{}", file, loc.line, loc.column)?;
                    } else {
                        write!(f, " at line {}:{}", loc.line, loc.column)?;
                    }
                }
                
                writeln!(f)?;
            }
        }
        
        Ok(())
    }
}

/// Extension trait for converting standard errors
pub trait ErrorExt {
    /// Convert to LuaError
    fn to_lua_error(self) -> LuaError;
}

impl ErrorExt for std::str::Utf8Error {
    fn to_lua_error(self) -> LuaError {
        LuaError::InvalidEncoding
    }
}

impl ErrorExt for std::num::ParseFloatError {
    fn to_lua_error(self) -> LuaError {
        LuaError::TypeError(format!("invalid number: {}", self))
    }
}

impl ErrorExt for std::num::ParseIntError {
    fn to_lua_error(self) -> LuaError {
        LuaError::TypeError(format!("invalid integer: {}", self))
    }
}