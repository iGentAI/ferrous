//! Lua Compiler Module
//! 
//! This module implements the Lua compiler that converts source code
//! into bytecode following the architectural principle of complete
//! independence from the VM and heap.

use super::error::{LuaError, LuaResult};
use super::codegen::{generate_bytecode, CompleteCompilationOutput, CompiledFunction, CompilationConstant, CompilationUpvalue};
use std::collections::HashMap;

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

pub mod loader {
    use super::*;
    use super::super::error::LuaResult;
    use super::super::handle::{StringHandle, FunctionProtoHandle, TableHandle};
    use super::super::transaction::HeapTransaction;
    use super::super::value::{Value, FunctionProto, UpvalueInfo as VMUpvalueInfo};
    

    
    /// Load a compiled module into the heap
    pub fn load_module<'a>(
        tx: &mut HeapTransaction<'a>,
        module: &CompiledModule
    ) -> LuaResult<FunctionProtoHandle> {
        // VALIDATION: Before processing, check that the string table isn't empty
        // if constants reference strings
        let has_string_constants = module.constants.iter().any(|c| matches!(c, CompilationConstant::String(_)));
        
        if has_string_constants && module.strings.is_empty() {
            return Err(LuaError::CompileError(
                "Module contains string constants but has empty string table".to_string()
            ));
        }
        
        // Step 1: Create all string handles
        let mut string_handles = Vec::with_capacity(module.strings.len());
        for s in &module.strings {
            string_handles.push(tx.create_string(s)?);
        }
        
        println!("DEBUG LOADER: Created {} string handles", string_handles.len());
        
        // VALIDATION: If about to process constants, validate all string indexes first
        for constant in &module.constants {
            if let CompilationConstant::String(idx) = constant {
                if *idx >= string_handles.len() {
                    return Err(LuaError::CompileError(format!(
                        "Invalid string index: {} (string table size: {})", 
                        idx, string_handles.len()
                    )));
                }
            }
        }
        
        // Print debug info about constants
        println!("DEBUG LOADER: Module has {} constants", module.constants.len());
        for (i, constant) in module.constants.iter().enumerate() {
            println!("DEBUG LOADER:   Constant {}: {:?}", i, constant);
        }
        
        // NEW APPROACH: Two-pass function prototype loading
        
        // Step 2a: First pass - Create all prototypes with placeholder constants
        println!("DEBUG LOADER: Processing {} nested prototypes", module.prototypes.len());
        
        // Create all prototype handles first (with placeholder Nil for FunctionProto constants)
        let mut proto_handles = Vec::with_capacity(module.prototypes.len());
        let mut proto_constants = Vec::with_capacity(module.prototypes.len());
        
        for (proto_idx, proto) in module.prototypes.iter().enumerate() {
            println!("DEBUG LOADER: Processing prototype {} - {} constants, {} bytecode, {} nested prototypes", 
                    proto_idx, proto.constants.len(), proto.bytecode.len(), proto.prototypes.len());
            
            // Convert upvalues
            let mut vm_upvalues = Vec::with_capacity(proto.upvalues.len());
            for (upval_idx, upvalue) in proto.upvalues.iter().enumerate() {
                println!("DEBUG LOADER:   Proto {} upvalue {}: in_stack={}, index={}", 
                         proto_idx, upval_idx, upvalue.in_stack, upvalue.index);
                vm_upvalues.push(VMUpvalueInfo {
                    in_stack: upvalue.in_stack,
                    index: upvalue.index,
                });
            }
            
            // Create temporary constants with Nil for FunctionProto references
            let mut temp_constants = Vec::with_capacity(proto.constants.len());
            for (const_idx, constant) in proto.constants.iter().enumerate() {
                match constant {
                    CompilationConstant::FunctionProto(idx) => {
                        println!("DEBUG LOADER:   Proto {} has FunctionProto const {} = proto index {}", 
                                proto_idx, const_idx, idx);
                        // Use Nil as placeholder for function prototypes
                        temp_constants.push(Value::Nil);
                    },
                    CompilationConstant::Table(_) => {
                        println!("DEBUG LOADER:   Proto {} has Table const {} (placeholder Nil)", 
                                proto_idx, const_idx);
                        // Use Nil as placeholder for tables (will be created in second pass)
                        temp_constants.push(Value::Nil);
                    },
                    CompilationConstant::Nil => {
                        temp_constants.push(Value::Nil);
                    },
                    CompilationConstant::Boolean(b) => {
                        temp_constants.push(Value::Boolean(*b));
                    },
                    CompilationConstant::Number(n) => {
                        temp_constants.push(Value::Number(*n));
                    },
                    CompilationConstant::String(idx) => {
                        // String handles are already created so we can resolve them now
                        if *idx < string_handles.len() {
                            temp_constants.push(Value::String(string_handles[*idx]));
                        } else {
                            return Err(LuaError::CompileError(format!(
                                "Invalid string index: {}", idx
                            )));
                        }
                    },
                }
            }
            
            // Create the prototype with temporary constants
            let temp_proto = FunctionProto {
                bytecode: proto.bytecode.clone(),
                constants: temp_constants,
                num_params: proto.num_params,
                is_vararg: proto.is_vararg,
                max_stack_size: proto.max_stack_size,
                upvalues: vm_upvalues,
            };
            
            // Create function prototype in heap
            let proto_handle = tx.create_function_proto(temp_proto)?;
            println!("DEBUG LOADER:   Created prototype handle for proto {}", proto_idx);
            proto_handles.push(proto_handle);
            
            // Remember original constants for second pass
            proto_constants.push(proto.constants.clone());
        }
        
        // Step 2b: Second pass - Update all FunctionProto constants now that all handles exist
        println!("DEBUG LOADER: Processing second pass for prototype constants");
        for (i, constants) in proto_constants.iter().enumerate() {
            println!("DEBUG LOADER:   Updating prototype {}", i);
            let proto_handle = proto_handles[i];
            
            // Get the current prototype 
            let mut proto = tx.get_function_proto_copy(proto_handle)?;
            
            // Update constants that are FunctionProto or Table references
            for (j, constant) in constants.iter().enumerate() {
                match constant {
                    CompilationConstant::FunctionProto(proto_idx) => {
                        println!("DEBUG LOADER:     Updating const {} to FunctionProto {}", j, proto_idx);
                        // Now we can resolve the function prototype
                        if *proto_idx < proto_handles.len() {
                            proto.constants[j] = Value::FunctionProto(proto_handles[*proto_idx]);
                        } else {
                            return Err(LuaError::CompileError(format!(
                                "Invalid function prototype index: {} (only {} prototypes)", proto_idx, proto_handles.len()
                            )));
                        }
                    },
                    CompilationConstant::Table(entries) => {
                        println!("DEBUG LOADER:     Creating table const {} with {} entries", j, entries.len());
                        // Create the table constant
                        let table_handle = create_table_constant(tx, entries, &string_handles, &proto_handles)?;
                        proto.constants[j] = Value::Table(table_handle);
                    },
                    _ => {
                        // Other constants are already set correctly
                    }
                }
            }
            
            // Store the updated prototype
            let updated_proto_handle = tx.replace_function_proto(proto_handle, proto)?;
            proto_handles[i] = updated_proto_handle;
        }
        
        // Step 3: Convert the main function
        println!("DEBUG LOADER: Processing main function - {} constants, {} bytecode", 
                module.constants.len(), module.bytecode.len());
        
        // Create constants with proper function prototype references
        let mut vm_constants = Vec::with_capacity(module.constants.len());
        
        for (i, constant) in module.constants.iter().enumerate() {
            println!("DEBUG LOADER:   Main function constant {}: {:?}", i, constant);
            let value = match constant {
                CompilationConstant::Nil => Value::Nil,
                CompilationConstant::Boolean(b) => Value::Boolean(*b),
                CompilationConstant::Number(n) => Value::Number(*n),
                CompilationConstant::String(idx) => {
                    // Use the string handle from the table
                    if *idx < string_handles.len() {
                        Value::String(string_handles[*idx])
                    } else {
                        return Err(LuaError::CompileError(format!(
                            "Invalid string index: {}", idx
                        )));
                    }
                },
                CompilationConstant::FunctionProto(idx) => {
                    // Use the proto handle from the table
                    println!("DEBUG LOADER:     Main function has FunctionProto const {} = proto index {}", 
                            i, idx);
                    if *idx < proto_handles.len() {
                        Value::FunctionProto(proto_handles[*idx])
                    } else {
                        return Err(LuaError::CompileError(format!(
                            "Invalid function prototype index: {}", idx
                        )));
                    }
                },
                CompilationConstant::Table(entries) => {
                    println!("DEBUG LOADER:     Creating table const {} with {} entries", 
                            i, entries.len());
                    // Create the table constant
                    let table_handle = create_table_constant(tx, entries, &string_handles, &proto_handles)?;
                    Value::Table(table_handle)
                },
            };
            
            vm_constants.push(value);
        }
        
        // Convert upvalues
        let mut vm_upvalues = Vec::with_capacity(module.upvalues.len());
        for (upval_idx, upvalue) in module.upvalues.iter().enumerate() {
            println!("DEBUG LOADER:   Main function upvalue {}: in_stack={}, index={}", 
                     upval_idx, upvalue.in_stack, upvalue.index);
            vm_upvalues.push(VMUpvalueInfo {
                in_stack: upvalue.in_stack,
                index: upvalue.index,
            });
        }
        
        // Create the final function prototype
        let proto = FunctionProto {
            bytecode: module.bytecode.clone(),
            constants: vm_constants,
            num_params: module.num_params,
            is_vararg: module.is_vararg,
            max_stack_size: module.max_stack_size,
            upvalues: vm_upvalues,
        };
        
        println!("DEBUG LOADER: Creating main function prototype");
        tx.create_function_proto(proto)
    }
    
    /// Helper function to create a table constant with proper string interning
    fn create_table_constant<'a>(
        tx: &mut HeapTransaction<'a>,
        entries: &[(CompilationConstant, CompilationConstant)],
        string_handles: &[StringHandle],
        proto_handles: &[FunctionProtoHandle],
    ) -> LuaResult<TableHandle> {
        // Create the table
        let table_handle = tx.create_table()?;
        
        // Populate the table with entries
        for (key_const, value_const) in entries {
            // Convert key constant to Value
            let key = constant_to_value(key_const, string_handles, proto_handles, tx)?;
            
            // Convert value constant to Value
            let value = constant_to_value(value_const, string_handles, proto_handles, tx)?;
            
            // Use the transaction's set_table_field which properly handles HashableValue
            tx.set_table_field(table_handle, key, value)?;
        }
        
        Ok(table_handle)
    }
    
    /// Helper to convert CompilationConstant to Value
    fn constant_to_value<'a>(
        constant: &CompilationConstant,
        string_handles: &[StringHandle],
        proto_handles: &[FunctionProtoHandle],
        tx: &mut HeapTransaction<'a>,
    ) -> LuaResult<Value> {
        match constant {
            CompilationConstant::Nil => Ok(Value::Nil),
            CompilationConstant::Boolean(b) => Ok(Value::Boolean(*b)),
            CompilationConstant::Number(n) => Ok(Value::Number(*n)),
            CompilationConstant::String(idx) => {
                if *idx < string_handles.len() {
                    Ok(Value::String(string_handles[*idx]))
                } else {
                    Err(LuaError::CompileError(format!("Invalid string index: {}", idx)))
                }
            },
            CompilationConstant::FunctionProto(idx) => {
                if *idx < proto_handles.len() {
                    Ok(Value::FunctionProto(proto_handles[*idx]))
                } else {
                    Err(LuaError::CompileError(format!("Invalid prototype index: {}", idx)))
                }
            },
            CompilationConstant::Table(entries) => {
                // Recursively create table
                let table = create_table_constant(tx, entries, string_handles, proto_handles)?;
                Ok(Value::Table(table))
            },
        }
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