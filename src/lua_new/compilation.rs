//! Compilation data structures for Lua
//! 
//! This module provides types for representing compilation results 
//! without dependency on the heap, enabling a clear separation between
//! compilation and execution.

use crate::lua_new::value::Instruction;

/// Represents a value during compilation, without heap dependency
#[derive(Debug, Clone)]
pub enum CompilationValue {
    Nil,
    Boolean(bool),
    Number(f64),
    String(usize), // Index into string pool
    TableConstructor, // Placeholder for table constructors
    FunctionPrototype(usize), // Index into nested_protos
}

/// Represents a function prototype during compilation
#[derive(Debug, Clone)]
pub struct CompilationProto {
    /// Compiled bytecode instructions
    pub code: Vec<Instruction>,
    
    /// Constant values used by this function
    pub constants: Vec<CompilationValue>,
    
    /// Number of parameters 
    pub param_count: u8,
    
    /// Is variadic (...)
    pub is_vararg: bool,
    
    /// Maximum stack size needed
    pub max_stack_size: u8,
    
    /// Number of upvalues
    pub upvalue_count: u8,
    
    /// Nested function prototypes
    pub nested_protos: Vec<CompilationProto>,
    
    /// Line number information (if available)
    pub line_info: Option<Vec<u32>>,
}

impl CompilationProto {
    /// Create a new empty prototype
    pub fn new() -> Self {
        CompilationProto {
            code: Vec::new(),
            constants: Vec::new(),
            param_count: 0,
            is_vararg: false,
            max_stack_size: 2,
            upvalue_count: 0,
            nested_protos: Vec::new(),
            line_info: None,
        }
    }
}

/// Represents the full compilation result
#[derive(Debug, Clone)]
pub struct CompilationScript {
    /// The main function prototype
    pub main_proto: CompilationProto,
    
    /// String pool for all string literals
    pub string_pool: Vec<String>,
    
    /// Source file name (if available)
    pub source_name: Option<String>,
    
    /// SHA1 hash of the script
    pub sha1: String,
}

impl CompilationScript {
    /// Create a new compilation script
    pub fn new(main_proto: CompilationProto, string_pool: Vec<String>, source: Option<String>, sha1: String) -> Self {
        CompilationScript {
            main_proto,
            string_pool,
            source_name: source,
            sha1,
        }
    }
    
    /// Get the main prototype
    pub fn main_proto(&self) -> &CompilationProto {
        &self.main_proto
    }
}

/// Helper function to compute SHA1 hash of a string
pub fn compute_sha1(source: &str) -> String {
    // Re-use existing sha1 computation from the project
    crate::lua_new::sha1::compute_sha1(source)
}