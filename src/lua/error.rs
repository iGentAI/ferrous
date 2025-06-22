//! Error types for the Lua interpreter

use std::fmt;

/// Result type for Lua operations
pub type Result<T> = std::result::Result<T, LuaError>;

/// Source location for error reporting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
    pub func_name: Option<&'static str>,
}

impl Default for SourceLocation {
    fn default() -> Self {
        SourceLocation {
            line: 0,
            column: 0,
            func_name: None,
        }
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(func) = self.func_name {
            write!(f, "{}:{}:{}", func, self.line, self.column)
        } else if self.line > 0 {
            write!(f, "line {}:{}", self.line, self.column)
        } else {
            write!(f, "<unknown location>")
        }
    }
}

/// Error context for rich error messages
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub location: Option<SourceLocation>,
    pub script_snippet: Option<String>,
    pub message: Option<String>,
}

impl Default for ErrorContext {
    fn default() -> Self {
        ErrorContext {
            location: None,
            script_snippet: None,
            message: None,
        }
    }
}

impl ErrorContext {
    pub fn new() -> Self {
        ErrorContext::default()
    }
    
    pub fn with_location(mut self, line: usize, column: usize, func_name: Option<&'static str>) -> Self {
        self.location = Some(SourceLocation {
            line,
            column,
            func_name,
        });
        self
    }
    
    pub fn with_snippet(mut self, snippet: String) -> Self {
        self.script_snippet = Some(snippet);
        self
    }
    
    pub fn with_message(mut self, message: String) -> Self {
        self.message = Some(message);
        self
    }
}

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
    
    /// Extended error with context
    Extended {
        kind: Box<LuaError>,
        context: ErrorContext,
    },
}

impl LuaError {
    /// Add context to an error
    pub fn with_context(self, context: ErrorContext) -> LuaError {
        LuaError::Extended {
            kind: Box::new(self),
            context,
        }
    }
    
    /// Add location to an error
    pub fn with_location(self, line: usize, column: usize, func_name: Option<&'static str>) -> LuaError {
        let context = ErrorContext::new().with_location(line, column, func_name);
        self.with_context(context)
    }
    
    /// Add script snippet to an error
    pub fn with_snippet(self, snippet: String) -> LuaError {
        let context = ErrorContext::new().with_snippet(snippet);
        self.with_context(context)
    }
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
            LuaError::Extended { kind, context } => {
                let base_error = kind.to_string();
                
                if let Some(loc) = &context.location {
                    write!(f, "{} at {}", base_error, loc)?;
                } else {
                    write!(f, "{}", base_error)?;
                }
                
                if let Some(snippet) = &context.script_snippet {
                    write!(f, "\nScript: {}", snippet)?;
                }
                
                if let Some(msg) = &context.message {
                    write!(f, "\n{}", msg)?;
                }
                
                Ok(())
            }
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

/// Convert I/O errors to Lua errors
impl From<std::io::Error> for LuaError {
    fn from(err: std::io::Error) -> Self {
        LuaError::Runtime(format!("I/O error: {}", err))
    }
}