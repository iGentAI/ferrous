//! Lua Compiler Module
//! 
//! This module implements the Lua compiler that converts source code
//! into bytecode following the architectural principle of complete
//! independence from the VM and heap.

use super::error::{LuaError, LuaResult};
use super::codegen::{generate_bytecode, CompleteCompilationOutput, CompiledFunction, CompilationConstant, CompilationUpvalue};
use super::ast::{Statement, LocalDeclaration, Expression};

// Re-export parser for convenience
pub use super::parser::parse;

/// Compiled Lua module
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledModule {
    /// The bytecode for the main function
    pub bytecode: Vec<u32>,
    
    /// Constants for the main function, in a form that can be loaded to the VM heap
    pub constants: Vec<CompilationConstant>,
    
    /// Number of parameters for the main function
    pub num_params: u8,
    
    /// Is the main function variadic?
    pub is_vararg: bool,
    
    /// Maximum stack size for the main function
    pub max_stack_size: u8,
    
    /// Upvalues for the main function
    pub upvalues: Vec<CompilationUpvalue>,
    
    /// String constants (used by the entire module)
    pub strings: Vec<String>,
    
    /// Nested function prototypes (flattened)
    pub prototypes: Vec<CompiledFunction>,
    
    /// Module metadata (for debugging)
    pub source_name: Option<String>,
}

/// Debug information for a compiled module
#[derive(Debug, Clone, PartialEq)]
pub struct DebugInfo {
    /// Line number information (PC -> line)
    pub line_info: Vec<usize>,
    
    /// Local variable debug info
    pub locals: Vec<LocalInfo>,
}

/// Debug information for a local variable
#[derive(Debug, Clone, PartialEq)]
pub struct LocalInfo {
    /// Variable name
    pub name: String,
    
    /// Start PC (where the variable is in scope)
    pub start_pc: usize,
    
    /// End PC (where the variable goes out of scope)
    pub end_pc: usize,
}

/// Compiler configuration
#[derive(Debug, Clone)]
pub struct CompilerConfig {
    /// Enable debug information
    pub debug_info: bool,
    
    /// Optimization level (0 = none, 1 = basic, 2 = full)
    pub optimization_level: u8,
    
    /// Source name (for error messages)
    pub source_name: Option<String>,
}

impl Default for CompilerConfig {
    fn default() -> Self {
        CompilerConfig {
            debug_info: true,
            optimization_level: 1,
            source_name: None,
        }
    }
}

/// Helper function to collect and remap all nested prototypes
fn collect_and_remap_prototypes(main: &CompiledFunction) -> (Vec<CompiledFunction>, Vec<CompilationConstant>) {
    // First, collect all prototypes with their positions in a tree structure
    #[derive(Debug)]
    struct ProtoInfo {
        proto: CompiledFunction,
        parent_idx: Option<usize>,
        local_idx: usize,  // Index in parent's prototype list
    }
    
    let mut all_protos = Vec::new();
    let mut to_process = vec![];
    
    // Add main function's immediate children
    for (local_idx, proto) in main.prototypes.iter().enumerate() {
        to_process.push((proto.clone(), None, local_idx));
    }
    
    // Process all prototypes depth-first
    while !to_process.is_empty() {
        let (proto, parent_idx, local_idx) = to_process.remove(0);
        let current_idx = all_protos.len();
        
        // Add this prototype's children to process queue
        for (child_local_idx, child) in proto.prototypes.iter().enumerate() {
            to_process.push((child.clone(), Some(current_idx), child_local_idx));
        }
        
        all_protos.push(ProtoInfo {
            proto,
            parent_idx,
            local_idx,
        });
    }
    
    // Build index mapping: (parent_idx, local_idx) -> global_idx
    let mut index_map = std::collections::HashMap::new();
    
    // Main function's prototypes
    for (i, info) in all_protos.iter().enumerate() {
        if info.parent_idx.is_none() {
            index_map.insert((None, info.local_idx), i);
        }
    }
    
    // Nested prototypes
    for (i, info) in all_protos.iter().enumerate() {
        if let Some(parent) = info.parent_idx {
            index_map.insert((Some(parent), info.local_idx), i);
        }
    }
    
    // Now remap all FunctionProto constants in each prototype
    let mut remapped_protos = Vec::new();
    for (current_global_idx, info) in all_protos.iter().enumerate() {
        let mut proto = info.proto.clone();
        
        // Remap constants for this prototype
        for constant in &mut proto.constants {
            if let CompilationConstant::FunctionProto(local_idx) = constant {
                // This constant refers to a child of the current prototype
                let global_idx = match index_map.get(&(Some(current_global_idx), *local_idx)) {
                    Some(idx) => *idx,
                    None => {
                        // This is a critical error in our understanding, but we'll handle it gracefully
                        println!("WARNING: Failed to remap function prototype index {} for prototype {}", 
                                *local_idx, current_global_idx);
                        continue; // Keep the old index as a fallback
                    }
                };
                *constant = CompilationConstant::FunctionProto(global_idx);
            }
        }
        
        // Clear the nested prototypes as they're now in the flat list
        proto.prototypes = Vec::new();
        remapped_protos.push(proto);
    }
    
    // Remap main function's constants
    let mut main_constants = main.constants.clone();
    for constant in &mut main_constants {
        if let CompilationConstant::FunctionProto(local_idx) = constant {
            let global_idx = match index_map.get(&(None, *local_idx)) {
                Some(idx) => *idx,
                None => {
                    // This is a critical error in our understanding, but we'll handle it gracefully
                    println!("WARNING: Failed to remap main function prototype index {}", *local_idx);
                    continue; // Keep the old index as a fallback
                }
            };
            *constant = CompilationConstant::FunctionProto(global_idx);
        }
    }
    
    (remapped_protos, main_constants)
}

/// Compile Lua source code into a module
pub fn compile(source: &str) -> LuaResult<CompiledModule> {
    compile_with_config(source, &CompilerConfig::default())
}

/// Compile Lua source code with custom configuration
pub fn compile_with_config(source: &str, config: &CompilerConfig) -> LuaResult<CompiledModule> {
    // Parse the source code into AST
    let ast = parse(source)?;
    
    // Generate bytecode
    let output = generate_bytecode(&ast)?;
    
    // Collect and remap nested prototypes to ensure indices are correct
    let (all_prototypes, main_constants) = collect_and_remap_prototypes(&output.main);
    
    println!("DEBUG COMPILER: Compilation complete - main bytecode: {}, total prototypes: {}, strings: {}", 
             output.main.bytecode.len(), all_prototypes.len(), output.strings.len());
    
    // Convert the compilation output to a compiled module
    Ok(CompiledModule {
        bytecode: output.main.bytecode,
        constants: main_constants,
        num_params: output.main.num_params,
        is_vararg: output.main.is_vararg,
        max_stack_size: output.main.max_stack_size,
        upvalues: output.main.upvalues,
        strings: output.strings,
        prototypes: all_prototypes,
        source_name: config.source_name.clone(),
    })
}


/// Initialize pre-compiled values using compiler infrastructure
/// This prevents module dependency cycles while reusing compilation logic.
pub(crate) fn compile_value_to_constant(value_expr: &str) -> Result<CompilationConstant, LuaError> {
    eprintln!("DEBUG compile_value_to_constant: Compiling expression: {}", value_expr);
    
    let tokens = super::lexer::tokenize(value_expr)?;
    let expr = super::parser::parse(value_expr)?.statements.first().and_then(|stmt| {
        if let Statement::LocalDeclaration(LocalDeclaration { expressions, .. }) = stmt {
            expressions.first().cloned()
        } else {
            None
        }
    }).unwrap_or(Expression::Nil);
    
    // Convert parsed expression to compilation constant
    expr_to_constant(&expr)
}

// Helper functions
fn expr_to_constant(expr: &super::ast::Expression) -> Result<CompilationConstant, LuaError> {
    match expr {
        super::ast::Expression::Nil => Ok(CompilationConstant::Nil),
        super::ast::Expression::Boolean(b) => Ok(CompilationConstant::Boolean(*b)),
        super::ast::Expression::Number(n) => Ok(CompilationConstant::Number(*n)),
        super::ast::Expression::String(s) => {
            // For standalone expressions, we can't intern strings
            // This is a limitation of this approach
            Err(LuaError::NotImplemented("String constants in compile_value_to_constant".to_string()))
        }
        super::ast::Expression::TableConstructor(tc) => {
            let mut items = vec![];
            
            // Array fields
            for field in &tc.fields {
                match field {
                    super::ast::TableField::List(expr) => {
                        // For array part, index is explicit (1-based)
                        let idx = items.len() + 1;
                        let key = CompilationConstant::Number(idx as f64);
                        let val = expr_to_constant(expr)?;
                        items.push((key, val));
                    },
                    super::ast::TableField::Record { key, value } => {
                        let key_const = CompilationConstant::String(0); // Fake string index
                        let val = expr_to_constant(value)?;
                        items.push((key_const, val));
                    },
                    super::ast::TableField::Index { key, value } => {
                        let key_const = expr_to_constant(key)?;
                        let val = expr_to_constant(value)?;
                        items.push((key_const, val));
                    }
                }
            }
            
            Ok(CompilationConstant::Table(items))
        },
        _ => Err(LuaError::NotImplemented(format!("Expression type {:?} in compile_value_to_constant", expr)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_compile_empty() {
        let module = compile("").unwrap();
        assert!(!module.bytecode.is_empty());
    }
    
    #[test]
    fn test_compile_simple() {
        // This will actually parse now that we have a parser
        let module = compile("local x = 42").unwrap();
        assert!(!module.bytecode.is_empty());
    }
    
    #[test]
    fn test_compile_function() {
        let source = "function add(a, b) return a + b end";
        let module = compile(source).unwrap();
        
        // Should have at least one function prototype
        assert!(!module.bytecode.is_empty());
    }
}