//! Compiler for Lua scripts
//!
//! This module compiles the abstract syntax tree (AST) into bytecode
//! for the Lua VM to execute.

use crate::lua_new::error::{LuaError, Result};
use crate::lua_new::value::{FunctionProto, Instruction, OpCode, Value, StringHandle};
use crate::lua_new::ast::{
    Chunk, Statement, Expression, Variable, Node, BinaryOperator, UnaryOperator,
    FunctionName, FunctionDefinition, Assignment, LocalAssignment, ReturnStatement,
    FunctionCall, TableConstructor, TableField
};
use crate::lua_new::parser::Parser;
use crate::lua_new::executor::{CompiledScript, ScriptError};
use crate::lua_new::vm::LuaVM;
use crate::lua_new::VMConfig;
use std::collections::HashMap;

/// Register allocator for the compiler
struct RegisterAllocator {
    /// Next available register
    next_reg: u16,
    
    /// Currently allocated registers
    allocated: Vec<bool>,
    
    /// Maximum register used
    max_reg: u16,
}

impl RegisterAllocator {
    /// Create a new register allocator
    fn new() -> Self {
        RegisterAllocator {
            next_reg: 0,
            allocated: vec![false; 256], // Start with reasonable capacity
            max_reg: 0,
        }
    }
    
    /// Allocate a register
    fn allocate(&mut self) -> u16 {
        // Find the next free register
        let mut reg = self.next_reg;
        while reg < self.allocated.len() as u16 && self.allocated[reg as usize] {
            reg += 1;
        }
        
        // If we need to expand our allocation table
        if reg >= self.allocated.len() as u16 {
            self.allocated.resize((reg + 1) as usize, false);
        }
        
        // Mark register as allocated
        self.allocated[reg as usize] = true;
        
        // Update next register and max register
        self.next_reg = reg + 1;
        if reg > self.max_reg {
            self.max_reg = reg;
        }
        
        reg
    }
    
    /// Free a register
    fn free(&mut self, reg: u16) {
        if reg < self.allocated.len() as u16 {
            self.allocated[reg as usize] = false;
            
            // Update next_reg to prefer recently freed registers
            if reg < self.next_reg {
                self.next_reg = reg;
            }
        }
    }
    
    /// Get the maximum register used
    fn max_reg(&self) -> u16 {
        self.max_reg
    }
    
    /// Reset the allocator for a new function
    fn reset(&mut self) {
        self.next_reg = 0;
        self.allocated.iter_mut().for_each(|a| *a = false);
        self.max_reg = 0;
    }
}

/// Scope information for local variables
struct Scope {
    /// Local variables (name -> register)
    locals: HashMap<StringHandle, u16>,
    
    /// Parent scope (for lookup)
    parent: Option<Box<Scope>>,
}

impl Scope {
    /// Create a new empty scope
    fn new() -> Self {
        Scope {
            locals: HashMap::new(),
            parent: None,
        }
    }
    
    /// Create a new child scope
    fn new_child(parent: Scope) -> Self {
        Scope {
            locals: HashMap::new(),
            parent: Some(Box::new(parent)),
        }
    }
    
    /// Define a local variable
    fn define(&mut self, name: StringHandle, register: u16) {
        self.locals.insert(name, register);
    }
    
    /// Look up a local variable
    fn lookup(&self, name: &StringHandle) -> Option<u16> {
        // Check current scope
        if let Some(reg) = self.locals.get(name) {
            return Some(*reg);
        }
        
        // Check parent scope
        if let Some(parent) = &self.parent {
            return parent.lookup(name);
        }
        
        None
    }
}

// Declare Clone implementation for Scope
impl Clone for Scope {
    fn clone(&self) -> Self {
        let mut scope = Scope::new();
        
        // Clone the locals map
        scope.locals = self.locals.clone();
        
        // Clone the parent scope if it exists
        if let Some(ref parent) = self.parent {
            scope.parent = Some(Box::new((**parent).clone()));
        }
        
        scope
    }
}

/// Compiler for Lua code
pub struct Compiler {
    /// Register allocator
    reg_alloc: RegisterAllocator,
    
    /// Current scope
    scope: Scope,
    
    /// Current function's constants
    constants: Vec<Value>,
    
    /// Current function's bytecode
    code: Vec<Instruction>,
    
    /// Upvalue information for closures
    upvalues: Vec<(StringHandle, bool)>, // (name, is_local)
    
    /// Heap for string interning
    heap: *mut crate::lua_new::heap::LuaHeap,
    
    /// VM configuration
    config: VMConfig,
}

impl Compiler {
    /// Create a new compiler
    pub fn new() -> Self {
        Compiler {
            reg_alloc: RegisterAllocator::new(),
            scope: Scope::new(),
            constants: Vec::new(),
            code: Vec::new(),
            upvalues: Vec::new(),
            heap: std::ptr::null_mut(),
            config: VMConfig::default(),
        }
    }
    
    /// Set the heap reference for string interning
    pub fn set_heap(&mut self, heap: *mut crate::lua_new::heap::LuaHeap) {
        self.heap = heap;
    }
    
    /// Safely get the heap reference
    fn heap(&mut self) -> &mut crate::lua_new::heap::LuaHeap {
        unsafe { &mut *self.heap }
    }
    
    /// Add a constant
    fn add_constant(&mut self, value: Value) -> u16 {
        // Check if constant already exists
        for (i, v) in self.constants.iter().enumerate() {
            if *v == value {
                return i as u16;
            }
        }
        
        // Add new constant
        let index = self.constants.len();
        if index > u16::MAX as usize {
            panic!("Too many constants");
        }
        
        self.constants.push(value);
        index as u16
    }
    
    /// Emit an instruction
    fn emit(&mut self, op: OpCode, a: u16, b: u16, c: u16) {
        let instr = (op as u32) | ((a as u32) << 6) | ((b as u32) << 14) | ((c as u32) << 23);
        self.code.push(Instruction::new(instr));
    }
    
    /// Emit an instruction with Bx field
    fn emit_bx(&mut self, op: OpCode, a: u16, bx: u16) {
        let instr = (op as u32) | ((a as u32) << 6) | ((bx as u32) << 14);
        self.code.push(Instruction::new(instr));
    }
    
    /// Emit an instruction with sBx field (signed jump offsets)
    fn emit_sbx(&mut self, op: OpCode, a: u16, sbx: i32) {
        let bx = (sbx + 0x1FFFF) as u32; // Convert to unsigned offset using i32
        let instr = (op as u32) | ((a as u32) << 6) | (bx << 14);
        self.code.push(Instruction::new(instr));
    }
    
    /// Create a patch point for a jump instruction
    fn emit_jump(&mut self) -> usize {
        let pos = self.code.len();
        self.emit_sbx(OpCode::Jmp, 0, 0); // Placeholder jump
        pos
    }
    
    /// Patch a jump instruction with the correct offset
    fn patch_jump(&mut self, jump_pos: usize) {
        let current = self.code.len();
        if jump_pos >= current {
            panic!("Invalid jump position");
        }
        
        // Calculate offset from jump instruction to current position
        let offset = (current - jump_pos - 1) as i32;
        
        // Update the sBx field of the jump instruction
        // Get the existing instruction
        let instr = &mut self.code[jump_pos];
        
        // Create a new instruction with the same opcode and A field but updated sBx
        let op = instr.opcode();
        let a = instr.a() as u16;
        *instr = Instruction::new(
            (op as u32) | ((a as u32) << 6) | (((offset + 0x1FFFF) as u32) << 14)
        );
    }
    
    /// Compile a Lua script
    pub fn compile(&mut self, source: &str) -> Result<FunctionProto> {
        let mut parser = Parser::new(source, self.heap())?;
        let ast = parser.parse()?;
        self.compile_chunk(&ast)
    }
    
    /// Compile a script
    pub fn compile_script(&self, source: &str, sha1: String) -> std::result::Result<CompiledScript, ScriptError> {
        // Create a VM for compilation
        let mut vm = LuaVM::new(self.config.clone());
        
        // Parse the script
        let mut parser = match crate::lua_new::parser::Parser::new(source, &mut vm.heap) {
            Ok(p) => p,
            Err(e) => return Err(ScriptError::CompilationFailed(e.to_string())),
        };
        
        // Parse the AST
        let ast = match parser.parse() {
            Ok(ast) => ast,
            Err(e) => return Err(ScriptError::CompilationFailed(e.to_string())),
        };
        
        // Create compiler and set heap reference
        let mut compiler = crate::lua_new::compiler::Compiler::new();
        compiler.set_heap(&mut vm.heap as *mut _);
        
        // Compile the AST to a function prototype
        let proto = match compiler.compile_chunk(&ast) {
            Ok(p) => p,
            Err(e) => return Err(ScriptError::CompilationFailed(e.to_string())),
        };
        
        // Create a closure from the prototype
        let closure = vm.heap.alloc_closure(proto, Vec::new());
        
        Ok(CompiledScript {
            source: source.to_string(),
            sha1,
            closure,
        })
    }

    /// Compile a chunk of code
    pub fn compile_chunk(&mut self, chunk: &Chunk) -> Result<FunctionProto> {
        // Reset compiler state
        self.reg_alloc = RegisterAllocator::new();
        self.constants.clear();
        self.code.clear();
        self.upvalues.clear();
        self.scope = Scope::new();
        
        // Compile statements
        for stmt in &chunk.statements {
            self.compile_statement(stmt)?;
        }
        
        // Compile return statement if present
        if let Some(ret) = &chunk.ret {
            self.compile_return_statement(ret)?;
        } else {
            // Implicit return nil
            self.emit(OpCode::Return, 0, 1, 0);
        }
        
        // Create the function prototype
        Ok(FunctionProto {
            code: self.code.clone(),
            constants: self.constants.clone(),
            param_count: 0, // Will be set by caller for function definitions
            is_vararg: false, // Will be set by caller for vararg functions
            max_stack_size: (self.reg_alloc.max_reg() + 1).max(2) as u8,
            upvalue_count: self.upvalues.len() as u8,
            source: None, // Source file name not tracked in this implementation
            line_info: None, // Line number info not tracked in this implementation
        })
    }
    
    /// Create a new child scope without borrowing self.scope
    fn new_child_scope(&self) -> Scope {
        // Create a clean clone of the current scope and return a new child scope 
        let parent_scope = self.scope.clone();
        Scope::new_child(parent_scope)
    }

    /// Compile a statement
    fn compile_statement(&mut self, stmt: &Node<Statement>) -> Result<()> {
        match &stmt.node {
            Statement::Empty => {
                // Do nothing for empty statements
            },
            
            Statement::Assignment(assignment) => {
                self.compile_assignment(assignment)?;
            },
            
            Statement::LocalAssignment(local_assignment) => {
                self.compile_local_assignment(local_assignment)?;
            },
            
            Statement::FunctionCall(call) => {
                // Compile function call (result discarded)
                self.compile_function_call(call, true)?;
            },
            
            Statement::FunctionDefinition(func_def) => {
                self.compile_function_definition(func_def, false)?;
            },
            
            Statement::LocalFunction(func_def) => {
                self.compile_function_definition(func_def, true)?;
            },
            
            Statement::DoBlock(block) => {
                // First create the new scope directly
                let parent_scope = self.scope.clone();
                let new_scope = Scope::new_child(parent_scope);
                // Then replace the current scope with it
                let old_scope = std::mem::replace(&mut self.scope, new_scope);
                
                // Compile block
                for stmt in &block.statements {
                    self.compile_statement(stmt)?;
                }
                
                // Compile return statement if present
                if let Some(ret) = &block.ret {
                    self.compile_return_statement(ret)?;
                }
                
                // Restore scope
                self.scope = old_scope;
            },
            
            Statement::WhileLoop { condition, body } => {
                // Start of loop (for jumps)
                let loop_start = self.code.len();
                
                // Compile condition
                let cond_reg = self.compile_expression(condition)?;
                
                // Load false for comparison
                let false_reg = self.reg_alloc.allocate();
                self.emit(OpCode::LoadBool, false_reg, 0, 0);
                
                // Test condition (jump out of loop if condition is false)
                self.emit(OpCode::Eq, 0, cond_reg, false_reg); // A=0 means check if eq is false
                let exit_jump = self.emit_jump();
                
                // Free condition and false registers
                self.reg_alloc.free(cond_reg);
                self.reg_alloc.free(false_reg);
                
                // Compile loop body
                for stmt in &body.statements {
                    self.compile_statement(stmt)?;
                }
                
                // Jump back to start of loop
                let offset = -(self.code.len() as i32 - loop_start as i32 + 1);
                self.emit_sbx(OpCode::Jmp, 0, offset);
                
                // Patch exit jump
                self.patch_jump(exit_jump);
            },
            
            Statement::RepeatLoop { body, condition } => {
                // First create the new scope directly
                let parent_scope = self.scope.clone();
                let new_scope = Scope::new_child(parent_scope);
                // Then replace the current scope with it
                let old_scope = std::mem::replace(&mut self.scope, new_scope);
                
                // Start of loop (for jumps)
                let loop_start = self.code.len();
                
                // Compile loop body
                for stmt in &body.statements {
                    self.compile_statement(stmt)?;
                }
                
                // Compile condition
                let cond_reg = self.compile_expression(condition)?;
                
                // Test the condition - if false (0), jump back to start
                self.emit(OpCode::Test, cond_reg, 0, 0);
                let offset = -(self.code.len() as i32 - loop_start as i32 + 2);
                self.emit_sbx(OpCode::Jmp, 0, offset);
                
                // Free condition register
                self.reg_alloc.free(cond_reg);
                
                // Restore scope
                self.scope = old_scope;
            },
            
            Statement::IfStatement { clauses, else_clause } => {
                let mut end_jumps = Vec::new();
                
                // Compile each if/elseif clause
                for (i, (condition, body)) in clauses.iter().enumerate() {
                    if i > 0 {
                        // For elseif, the previous condition's jump brings us here
                    }
                    
                    // Compile condition
                    let cond_reg = self.compile_expression(condition)?;
                    
                    // Load false for comparison
                    let false_reg = self.reg_alloc.allocate();
                    self.emit(OpCode::LoadBool, false_reg, 0, 0);
                    
                    // Test condition (skip body if condition is false)
                    self.emit(OpCode::Eq, 1, cond_reg, false_reg); // A=1 means check if eq is true
                    let skip_body = self.emit_jump();
                    
                    // Free registers
                    self.reg_alloc.free(cond_reg);
                    self.reg_alloc.free(false_reg);
                    
                    // Compile body
                    for stmt in &body.statements {
                        self.compile_statement(stmt)?;
                    }
                    
                    // Jump to end of if statement after body
                    if i < clauses.len() - 1 || else_clause.is_some() {
                        end_jumps.push(self.emit_jump());
                    }
                    
                    // Patch body jump to here (start of next clause or else)
                    self.patch_jump(skip_body);
                }
                
                // Compile else clause if present
                if let Some(else_body) = else_clause {
                    for stmt in &else_body.statements {
                        self.compile_statement(stmt)?;
                    }
                }
                
                // Patch all end jumps to here
                for jump in end_jumps {
                    self.patch_jump(jump);
                }
            },
            
            Statement::NumericFor { variable, start, limit, step, body } => {
                // Compile initial expressions
                let init_reg = self.compile_expression(start)?;
                let limit_reg = self.compile_expression(limit)?;
                
                // Compile step (default to 1 if not provided)
                let step_reg = if let Some(step_expr) = step {
                    self.compile_expression(step_expr)?
                } else {
                    // Use constant 1
                    let const_idx = self.add_constant(Value::Number(1.0));
                    let reg = self.reg_alloc.allocate();
                    self.emit_bx(OpCode::LoadK, reg, const_idx);
                    reg
                };
                
                // Allocate register for loop variable
                let var_reg = self.reg_alloc.allocate();
                
                // Define loop variable in scope
                self.scope.define(*variable, var_reg);
                
                // ForPrep (subtract step from init)
                self.emit_sbx(OpCode::ForPrep, init_reg, 1); // Jump to ForLoop
                
                // Compile loop body
                for stmt in &body.statements {
                    self.compile_statement(stmt)?;
                }
                
                // ForLoop (increment and check)
                // Jump back to body if condition is true
                let body_size = self.code.len() - (init_reg as usize + 2);
                self.emit_sbx(OpCode::ForLoop, init_reg, -(body_size as i32 + 1));
                
                // Free registers
                self.reg_alloc.free(init_reg);
                self.reg_alloc.free(limit_reg);
                self.reg_alloc.free(step_reg);
                self.reg_alloc.free(var_reg);
            },
            
            // Fix GenericFor case to use ignore patterns for unused variables
            Statement::GenericFor { variables: _, iterators: _, body: _ } => {
                // Not fully implemented in the VM yet
                return Err(LuaError::NotImplemented("generic for loops"));
            },
            
            Statement::Break => {
                // Not fully implemented yet
                return Err(LuaError::NotImplemented("break statement"));
            }
        }
        
        Ok(())
    }
    
    /// Compile an assignment statement
    fn compile_assignment(&mut self, assignment: &Assignment) -> Result<()> {
        // Evaluate all expressions first
        let mut expressions = Vec::with_capacity(assignment.expressions.len());
        for expr in &assignment.expressions {
            expressions.push(self.compile_expression(expr)?);
        }
        
        // Perform the assignments
        for (i, var) in assignment.variables.iter().enumerate() {
            if i < expressions.len() {
                self.compile_variable_assignment(var, expressions[i])?;
            } else {
                // If there are more variables than expressions, assign nil
                let nil_reg = self.reg_alloc.allocate();
                self.emit(OpCode::LoadNil, nil_reg, 0, 0);
                self.compile_variable_assignment(var, nil_reg)?;
                self.reg_alloc.free(nil_reg);
            }
        }
        
        // Free expression registers
        for reg in expressions {
            self.reg_alloc.free(reg);
        }
        
        Ok(())
    }
    
    /// Compile a variable assignment
    fn compile_variable_assignment(&mut self, var: &Node<Variable>, value_reg: u16) -> Result<()> {
        match &var.node {
            Variable::Name(name) => {
                // Check if it's a local variable
                if let Some(reg) = self.scope.lookup(name) {
                    // Local variable - use MOVE instruction
                    self.emit(OpCode::Move, reg, value_reg, 0);
                } else {
                    // Global variable - use SETGLOBAL instruction
                    let const_idx = self.add_constant(Value::String(*name));
                    self.emit_bx(OpCode::SetGlobal, value_reg, const_idx);
                }
            },
            
            Variable::TableField { table, key } => {
                // Create an expression for the table variable
                let table_var = match &table.node {
                    Expression::Variable(var) => var.clone(),
                    _ => return Err(LuaError::TypeError("Expected variable for table field assignment".to_string())),
                };

                let table_expr = Node::new(
                    Expression::Variable(table_var),
                    table.loc
                );
                
                // Compile table expression
                let table_reg = self.compile_expression(&table_expr)?;
                
                // Compile key expression - key is already a Node<Expression>
                let key_reg = self.compile_expression(key)?;
                
                // SetTable instruction
                self.emit(OpCode::SetTable, table_reg, key_reg, value_reg);
                
                // Free registers
                self.reg_alloc.free(table_reg);
                self.reg_alloc.free(key_reg);
            },
            
            Variable::TableDot { table, key } => {
                // Create an expression for the table variable
                let table_var = match &table.node {
                    Expression::Variable(var) => var.clone(),
                    _ => return Err(LuaError::TypeError("Expected variable for table dot assignment".to_string())),
                };

                let table_expr = Node::new(
                    Expression::Variable(table_var),
                    table.loc
                );
                
                // Compile table expression
                let table_reg = self.compile_expression(&table_expr)?;
                
                // Use key as a constant
                let const_idx = self.add_constant(Value::String(*key));
                
                // Ensure constant index is within limits (0-255 for RK)
                if const_idx > 0xFF {
                    // If index is too large, load constant into a register first
                    let key_reg = self.reg_alloc.allocate();
                    self.emit_bx(OpCode::LoadK, key_reg, const_idx);
                    
                    // Then use the register
                    self.emit(OpCode::SetTable, table_reg, key_reg, value_reg);
                    
                    // Free the key register
                    self.reg_alloc.free(key_reg);
                } else {
                    // SetTable with constant key (RK format)
                    self.emit(OpCode::SetTable, table_reg, 0x100 | const_idx, value_reg);
                }
                
                // Free register
                self.reg_alloc.free(table_reg);
            }
        }
        
        Ok(())
    }
    
    /// Compile a local assignment
    fn compile_local_assignment(&mut self, assignment: &LocalAssignment) -> Result<()> {
        // Evaluate all expressions first
        let mut expression_regs = Vec::with_capacity(assignment.expressions.len());
        for expr in &assignment.expressions {
            expression_regs.push(self.compile_expression(expr)?);
        }
        
        // Allocate registers for variables and define them in scope
        let mut var_regs = Vec::with_capacity(assignment.names.len());
        for name in &assignment.names {
            let reg = self.reg_alloc.allocate();
            self.scope.define(*name, reg);
            var_regs.push(reg);
        }
        
        // Assign expressions to variables
        for (i, &var_reg) in var_regs.iter().enumerate() {
            if i < expression_regs.len() {
                // Move expression result to variable
                self.emit(OpCode::Move, var_reg, expression_regs[i], 0);
            } else {
                // If there are more variables than expressions, assign nil
                self.emit(OpCode::LoadNil, var_reg, 0, 0);
            }
        }
        
        // Free expression registers
        for reg in expression_regs {
            self.reg_alloc.free(reg);
        }
        
        Ok(())
    }
    
    /// Compile a function definition
    fn compile_function_definition(&mut self, func_def: &FunctionDefinition, is_local: bool) -> Result<()> {
        // Save current compiler state
        let old_reg_alloc = std::mem::replace(&mut self.reg_alloc, RegisterAllocator::new());
        let old_scope = std::mem::replace(&mut self.scope, Scope::new());
        let old_constants = std::mem::replace(&mut self.constants, Vec::new());
        let old_code = std::mem::replace(&mut self.code, Vec::new());
        let old_upvalues = std::mem::take(&mut self.upvalues);
        
        // Define parameters as local variables
        for (i, &name) in func_def.parameters.names.iter().enumerate() {
            let reg = i as u16; // Parameters are in first registers
            self.scope.define(name, reg);
        }
        
        // Compile function body
        for stmt in &func_def.body.statements {
            self.compile_statement(stmt)?;
        }
        
        // Add return statement
        if let Some(ret) = &func_def.body.ret {
            self.compile_return_statement(ret)?;
        } else {
            // Implicit return nil
            self.emit(OpCode::Return, 0, 1, 0);
        }
        
        // Create function prototype
        let proto = FunctionProto {
            code: self.code.clone(),
            constants: self.constants.clone(),
            param_count: func_def.parameters.names.len() as u8,
            is_vararg: func_def.parameters.is_variadic,
            max_stack_size: (self.reg_alloc.max_reg() + 1).max(2) as u8,
            upvalue_count: self.upvalues.len() as u8,
            source: None, // Source file name not tracked in this implementation
            line_info: None, // Line number info not tracked in this implementation
        };
        
        // Restore compiler state
        let _func_proto = proto;
        self.reg_alloc = old_reg_alloc;
        self.scope = old_scope;
        self.constants = old_constants;
        self.code = old_code;
        self.upvalues = old_upvalues;
        
        // Add the function prototype as a constant
        let proto_idx = self.add_constant(Value::Number(1.0)); // Placeholder - we don't have proper proto constants yet
        
        // Create closure
        let closure_reg = self.reg_alloc.allocate();
        self.emit_bx(OpCode::Closure, closure_reg, proto_idx);
        
        // Handle function name and assignment
        if is_local {
            // Local function - get the simple name
            if let FunctionName::Simple(name) = &func_def.name {
                self.scope.define(*name, closure_reg);
            } else {
                return Err(LuaError::SyntaxError {
                    message: "Local function must have a simple name".to_string(),
                    line: 0, // We don't have line number info
                    column: 0,
                });
            }
        } else {
            // Global function
            match &func_def.name {
                FunctionName::Simple(name) => {
                    // Simple global function name
                    let name_idx = self.add_constant(Value::String(*name));
                    self.emit_bx(OpCode::SetGlobal, closure_reg, name_idx);
                },
                FunctionName::TableField { base, fields } => {
                    // Function in table: base.field1.field2 = function
                    // Get the base table - global or local
                    let base_reg = if let Some(reg) = self.scope.lookup(base) {
                        // Local variable
                        reg
                    } else {
                        // Global variable
                        let base_idx = self.add_constant(Value::String(*base));
                        let reg = self.reg_alloc.allocate();
                        self.emit_bx(OpCode::GetGlobal, reg, base_idx);
                        reg
                    };
                    
                    // Handle nested fields
                    let mut table_reg = base_reg;
                    for (i, field) in fields.iter().enumerate() {
                        let field_idx = self.add_constant(Value::String(*field));
                        
                        if i < fields.len() - 1 {
                            // Intermediate field access
                            let next_reg = self.reg_alloc.allocate();
                            
                            // Ensure constant index is within limits (0-255 for RK)
                            if field_idx > 0xFF {
                                // If index is too large, load constant into a register first
                                let key_reg = self.reg_alloc.allocate();
                                self.emit_bx(OpCode::LoadK, key_reg, field_idx);
                                
                                // Then use the register
                                self.emit(OpCode::GetTable, next_reg, table_reg, key_reg);
                                
                                // Free the key register
                                self.reg_alloc.free(key_reg);
                            } else {
                                // GetTable with constant key (RK format)
                                self.emit(OpCode::GetTable, next_reg, table_reg, 0x100 | field_idx);
                            }
                            
                            // Free previous register if it's not the base variable
                            if i > 0 || !self.scope.lookup(base).is_some() {
                                self.reg_alloc.free(table_reg);
                            }
                            
                            table_reg = next_reg;
                        } else {
                            // Final field assignment
                            
                            // Ensure constant index is within limits (0-255 for RK)
                            if field_idx > 0xFF {
                                // If index is too large, load constant into a register first
                                let key_reg = self.reg_alloc.allocate();
                                self.emit_bx(OpCode::LoadK, key_reg, field_idx);
                                
                                // Then use the register
                                self.emit(OpCode::SetTable, table_reg, key_reg, closure_reg);
                                
                                // Free the key register
                                self.reg_alloc.free(key_reg);
                            } else {
                                // SetTable with constant key (RK format)
                                self.emit(OpCode::SetTable, table_reg, 0x100 | field_idx, closure_reg);
                            }
                            
                            // Free table register if it's not a local variable
                            if i > 0 || !self.scope.lookup(base).is_some() {
                                self.reg_alloc.free(table_reg);
                            }
                        }
                    }
                },
                FunctionName::Method { base: _, fields: _, method: _ } => {
                    // Method: base.field:method = function
                    // This is complex and not fully implemented yet
                    return Err(LuaError::NotImplemented("Method definition"));
                }
            }
        }
        
        // Free closure register (if it's a global function)
        if !is_local {
            self.reg_alloc.free(closure_reg);
        }
        
        Ok(())
    }
    
    /// Compile a return statement
    fn compile_return_statement(&mut self, ret: &Node<ReturnStatement>) -> Result<()> {
        if ret.node.expressions.is_empty() {
            // Return no values
            self.emit(OpCode::Return, 0, 1, 0);
            return Ok(());
        }
        
        // Compile expressions
        let mut regs = Vec::with_capacity(ret.node.expressions.len());
        for expr in &ret.node.expressions {
            regs.push(self.compile_expression(expr)?);
        }
        
        // If there's only one expression, return it
        if regs.len() == 1 {
            self.emit(OpCode::Return, regs[0], 2, 0);
        } else {
            // For multiple expressions, we need to move them to sequential registers
            let base_reg = self.reg_alloc.allocate();
            
            for (i, &reg) in regs.iter().enumerate() {
                if base_reg + i as u16 != reg {
                    self.emit(OpCode::Move, base_reg + i as u16, reg, 0);
                }
            }
            
            // Return multiple values
            self.emit(OpCode::Return, base_reg, regs.len() as u16 + 1, 0);
            
            // Free base register
            self.reg_alloc.free(base_reg);
        }
        
        // Free expression registers
        for reg in regs {
            self.reg_alloc.free(reg);
        }
        
        Ok(())
    }
    
    /// Compile a function call
    fn compile_function_call(&mut self, call: &FunctionCall, discard_result: bool) -> Result<u16> {
        // Compile function expression
        let func_reg = self.compile_expression(&call.function)?;
        
        // Compile arguments
        let mut arg_regs = Vec::with_capacity(call.arguments.len());
        for arg in &call.arguments {
            arg_regs.push(self.compile_expression(arg)?);
        }
        
        // For method calls, we need to adjust arguments
        if call.is_method_call {
            // TODO: Handle method calls
            return Err(LuaError::NotImplemented("method calls"));
        }
        
        // If arguments are not right after function, we need to move them
        let base_reg = func_reg;
        
        for (i, &arg_reg) in arg_regs.iter().enumerate() {
            if base_reg + 1 + i as u16 != arg_reg {
                self.emit(OpCode::Move, base_reg + 1 + i as u16, arg_reg, 0);
            }
        }
        
        // Call function
        let a = base_reg;
        let b = (arg_regs.len() + 1) as u16; // +1 for function itself
        let c = if discard_result { 1 } else { 2 }; // 1 = no results, 2 = 1 result
        
        self.emit(OpCode::Call, a, b, c);
        
        // Free argument registers
        for reg in arg_regs {
            self.reg_alloc.free(reg);
        }
        
        // Result is stored in function register
        Ok(func_reg)
    }
    
    /// Compile an expression
    fn compile_expression(&mut self, expr: &Node<Expression>) -> Result<u16> {
        match &expr.node {
            Expression::Nil => {
                let reg = self.reg_alloc.allocate();
                self.emit(OpCode::LoadNil, reg, 0, 0);
                Ok(reg)
            },
            
            Expression::Boolean(b) => {
                let reg = self.reg_alloc.allocate();
                self.emit(OpCode::LoadBool, reg, if *b { 1 } else { 0 }, 0);
                Ok(reg)
            },
            
            Expression::Number(n) => {
                let const_idx = self.add_constant(Value::Number(*n));
                let reg = self.reg_alloc.allocate();
                self.emit_bx(OpCode::LoadK, reg, const_idx);
                Ok(reg)
            },
            
            Expression::String(s) => {
                let const_idx = self.add_constant(Value::String(*s));
                let reg = self.reg_alloc.allocate();
                self.emit_bx(OpCode::LoadK, reg, const_idx);
                Ok(reg)
            },
            
            Expression::Variable(var) => {
                match var {
                    Variable::Name(name) => {
                        // Check if it's a local variable
                        if let Some(reg) = self.scope.lookup(name) {
                            return Ok(reg);
                        }
                        
                        // Global variable - use GETGLOBAL instruction
                        let const_idx = self.add_constant(Value::String(*name));
                        let reg = self.reg_alloc.allocate();
                        self.emit_bx(OpCode::GetGlobal, reg, const_idx);
                        
                        Ok(reg)
                    },
                    
                    Variable::TableField { table, key } => {
                        // Create an Expression node for the table
                        let table_var = match &table.node {
                            Expression::Variable(var) => var.clone(),
                            _ => return Err(LuaError::TypeError("Expected variable for table access".to_string())),
                        };

                        let table_expr = Node::new(
                            Expression::Variable(table_var),
                            table.loc
                        );
                        
                        let table_reg = self.compile_expression(&table_expr)?;
                        
                        // Compile key expression
                        let key_reg = self.compile_expression(key)?;
                        
                        // GetTable instruction
                        let dest_reg = self.reg_alloc.allocate();
                        self.emit(OpCode::GetTable, dest_reg, table_reg, key_reg);
                        
                        // Free registers
                        self.reg_alloc.free(table_reg);
                        self.reg_alloc.free(key_reg);
                        
                        Ok(dest_reg)
                    },
                    
                    Variable::TableDot { table, key } => {
                        // Create an Expression node for the table
                        let table_var = match &table.node {
                            Expression::Variable(var) => var.clone(),
                            _ => return Err(LuaError::TypeError("Expected variable for table dot access".to_string())),
                        };

                        let table_expr = Node::new(
                            Expression::Variable(table_var),
                            table.loc
                        );
                        
                        let table_reg = self.compile_expression(&table_expr)?;
                        
                        // Use key as a constant
                        let const_idx = self.add_constant(Value::String(*key));
                        
                        // Ensure constant index is within limits (0-255 for RK)
                        let dest_reg = self.reg_alloc.allocate();
                        if const_idx > 0xFF {
                            // If index is too large, load constant into a register first
                            let key_reg = self.reg_alloc.allocate();
                            self.emit_bx(OpCode::LoadK, key_reg, const_idx);
                            
                            // Then use the register
                            self.emit(OpCode::GetTable, dest_reg, table_reg, key_reg);
                            
                            // Free the key register
                            self.reg_alloc.free(key_reg);
                        } else {
                            // GetTable with constant key (RK format)
                            self.emit(OpCode::GetTable, dest_reg, table_reg, 0x100 | const_idx);
                        }
                        
                        // Free register
                        self.reg_alloc.free(table_reg);
                        
                        Ok(dest_reg)
                    }
                }
            },
            
            Expression::Vararg => {
                // Not implemented yet
                Err(LuaError::NotImplemented("vararg expression"))
            },
            
            Expression::FunctionCall(call) => {
                self.compile_function_call(call, false)
            },
            
            Expression::TableConstructor(table) => {
                // Allocate register for the table
                let table_reg = self.reg_alloc.allocate();
                
                // Create table with estimated size
                let array_size = table.fields.iter()
                    .filter(|f| matches!(f, TableField::Array(_)))
                    .count();
                let _hash_size = table.fields.len() - array_size;
                
                // Compute log2 sizes for B and C fields (or just use reasonable defaults)
                let b = 0; // array size log2
                let c = 0; // hash size log2
                
                self.emit(OpCode::NewTable, table_reg, b, c);
                
                // Fill in the fields
                for (i, field) in table.fields.iter().enumerate() {
                    match field {
                        TableField::Array(expr) => {
                            // Compile value expression
                            let value_reg = self.compile_expression(expr)?;
                            
                            // Store in array part (i+1)
                            let array_idx = i + 1;
                            
                            // Create constant for the index
                            let const_idx = self.add_constant(Value::Number(array_idx as f64));
                            
                            // Ensure constant index is within limits (0-255 for RK)
                            if const_idx > 0xFF {
                                // If index is too large, load constant into a register first
                                let key_reg = self.reg_alloc.allocate();
                                self.emit_bx(OpCode::LoadK, key_reg, const_idx);
                                
                                // Then use the register
                                self.emit(OpCode::SetTable, table_reg, key_reg, value_reg);
                                
                                // Free the key register
                                self.reg_alloc.free(key_reg);
                            } else {
                                // SetTable with constant key (RK format)
                                self.emit(OpCode::SetTable, table_reg, 0x100 | const_idx, value_reg);
                            }
                            
                            // Free value register
                            self.reg_alloc.free(value_reg);
                        },
                        
                        TableField::Record { key, value } => {
                            // Compile value expression
                            let value_reg = self.compile_expression(value)?;
                            
                            // Use constant for key
                            let const_idx = self.add_constant(Value::String(*key));
                            
                            // Ensure constant index is within limits (0-255 for RK)
                            if const_idx > 0xFF {
                                // If index is too large, load constant into a register first
                                let key_reg = self.reg_alloc.allocate();
                                self.emit_bx(OpCode::LoadK, key_reg, const_idx);
                                
                                // Then use the register
                                self.emit(OpCode::SetTable, table_reg, key_reg, value_reg);
                                
                                // Free the key register
                                self.reg_alloc.free(key_reg);
                            } else {
                                // SetTable with constant key (RK format)
                                self.emit(OpCode::SetTable, table_reg, 0x100 | const_idx, value_reg);
                            }
                            
                            // Free value register
                            self.reg_alloc.free(value_reg);
                        },
                        
                        TableField::Expression { key, value } => {
                            // Compile key and value expressions
                            let key_reg = self.compile_expression(key)?;
                            let value_reg = self.compile_expression(value)?;
                            
                            // Set the table field
                            self.emit(OpCode::SetTable, table_reg, key_reg, value_reg);
                            
                            // Free registers
                            self.reg_alloc.free(key_reg);
                            self.reg_alloc.free(value_reg);
                        },
                    }
                }
                
                Ok(table_reg)
            },
            
            Expression::AnonymousFunction { parameters, body } => {
                // Save current compiler state
                let old_reg_alloc = std::mem::replace(&mut self.reg_alloc, RegisterAllocator::new());
                let old_scope = std::mem::replace(&mut self.scope, Scope::new());
                let old_constants = std::mem::replace(&mut self.constants, Vec::new());
                let old_code = std::mem::replace(&mut self.code, Vec::new());
                let old_upvalues = std::mem::take(&mut self.upvalues);
                
                // Define parameters as local variables
                for (i, &name) in parameters.names.iter().enumerate() {
                    let reg = i as u16; // Parameters are in first registers
                    self.scope.define(name, reg);
                }
                
                // Compile function body
                for stmt in &body.statements {
                    self.compile_statement(stmt)?;
                }
                
                // Add return statement
                if let Some(ret) = &body.ret {
                    self.compile_return_statement(ret)?;
                } else {
                    // Implicit return nil
                    self.emit(OpCode::Return, 0, 1, 0);
                }
                
                // Create function prototype
                let _proto = FunctionProto {
                    code: self.code.clone(),
                    constants: self.constants.clone(),
                    param_count: parameters.names.len() as u8,
                    is_vararg: parameters.is_variadic,
                    max_stack_size: (self.reg_alloc.max_reg() + 1).max(2) as u8,
                    upvalue_count: self.upvalues.len() as u8,
                    source: None,
                    line_info: None,
                };
                
                // Restore compiler state
                self.reg_alloc = old_reg_alloc;
                self.scope = old_scope;
                self.constants = old_constants;
                self.code = old_code;
                self.upvalues = old_upvalues;
                
                // Add the function prototype as a constant
                let proto_idx = self.add_constant(Value::Number(1.0)); // Placeholder
                
                // Create closure
                let closure_reg = self.reg_alloc.allocate();
                self.emit_bx(OpCode::Closure, closure_reg, proto_idx);
                
                Ok(closure_reg)
            },
            
            Expression::BinaryOp { op, left, right } => {
                // Compile operands
                let left_reg = self.compile_expression(left)?;
                let right_reg = self.compile_expression(right)?;
                
                // Allocate register for result
                let result_reg = self.reg_alloc.allocate();
                
                // Emit appropriate instruction based on operator
                match op {
                    BinaryOperator::Add => {
                        self.emit(OpCode::Add, result_reg, left_reg, right_reg);
                    },
                    BinaryOperator::Sub => {
                        self.emit(OpCode::Sub, result_reg, left_reg, right_reg);
                    },
                    BinaryOperator::Mul => {
                        self.emit(OpCode::Mul, result_reg, left_reg, right_reg);
                    },
                    BinaryOperator::Div => {
                        self.emit(OpCode::Div, result_reg, left_reg, right_reg);
                    },
                    BinaryOperator::Mod => {
                        self.emit(OpCode::Mod, result_reg, left_reg, right_reg);
                    },
                    BinaryOperator::Pow => {
                        self.emit(OpCode::Pow, result_reg, left_reg, right_reg);
                    },
                    BinaryOperator::Concat => {
                        self.emit(OpCode::Concat, result_reg, left_reg, right_reg);
                    },
                    
                    // Comparisons
                    BinaryOperator::LT => {
                        self.emit(OpCode::Lt, 1, left_reg, right_reg); // A=1 means "true if L<R"
                        self.emit(OpCode::LoadBool, result_reg, 1, 1); // Load true, skip next
                        self.emit(OpCode::LoadBool, result_reg, 0, 0); // Load false
                    },
                    BinaryOperator::LE => {
                        self.emit(OpCode::Le, 1, left_reg, right_reg);
                        self.emit(OpCode::LoadBool, result_reg, 1, 1);
                        self.emit(OpCode::LoadBool, result_reg, 0, 0);
                    },
                    BinaryOperator::GT => {
                        self.emit(OpCode::Lt, 1, right_reg, left_reg); // Swap operands for GT
                        self.emit(OpCode::LoadBool, result_reg, 1, 1);
                        self.emit(OpCode::LoadBool, result_reg, 0, 0);
                    },
                    BinaryOperator::GE => {
                        self.emit(OpCode::Le, 1, right_reg, left_reg); // Swap operands for GE
                        self.emit(OpCode::LoadBool, result_reg, 1, 1);
                        self.emit(OpCode::LoadBool, result_reg, 0, 0);
                    },
                    BinaryOperator::EQ => {
                        self.emit(OpCode::Eq, 1, left_reg, right_reg);
                        self.emit(OpCode::LoadBool, result_reg, 1, 1);
                        self.emit(OpCode::LoadBool, result_reg, 0, 0);
                    },
                    BinaryOperator::NE => {
                        self.emit(OpCode::Eq, 0, left_reg, right_reg);
                        self.emit(OpCode::LoadBool, result_reg, 1, 1);
                        self.emit(OpCode::LoadBool, result_reg, 0, 0);
                    },
                    
                    // Logical operations with short-circuit evaluation
                    BinaryOperator::And => {
                        // Move left value to result register
                        self.emit(OpCode::Move, result_reg, left_reg, 0);
                        
                        // Test if left is truthy
                        self.emit(OpCode::Test, result_reg, 0, 0); // If result is falsey, skip
                        
                        // Skip right operand if left is false/nil (short-circuit)
                        let end_jump = self.emit_jump();
                        
                        // Evaluate right operand and store in result register
                        self.emit(OpCode::Move, result_reg, right_reg, 0);
                        
                        // Patch jump to here
                        self.patch_jump(end_jump);
                    },
                    BinaryOperator::Or => {
                        // Move left value to result register
                        self.emit(OpCode::Move, result_reg, left_reg, 0);
                        
                        // Test if left is truthy
                        self.emit(OpCode::Test, result_reg, 1, 0); // If result is truthy, skip
                        
                        // Skip right operand if left is true (short-circuit)
                        let end_jump = self.emit_jump();
                        
                        // Evaluate right operand and store in result register
                        self.emit(OpCode::Move, result_reg, right_reg, 0);
                        
                        // Patch jump to here
                        self.patch_jump(end_jump);
                    },
                }
                
                // Free operand registers
                self.reg_alloc.free(left_reg);
                self.reg_alloc.free(right_reg);
                
                Ok(result_reg)
            },
            
            Expression::UnaryOp { op, operand } => {
                // Compile operand
                let operand_reg = self.compile_expression(operand)?;
                
                // Allocate register for result
                let result_reg = self.reg_alloc.allocate();
                
                // Emit appropriate instruction based on operator
                match op {
                    UnaryOperator::Minus => {
                        self.emit(OpCode::Unm, result_reg, operand_reg, 0);
                    },
                    UnaryOperator::Not => {
                        self.emit(OpCode::Not, result_reg, operand_reg, 0);
                    },
                    UnaryOperator::Len => {
                        self.emit(OpCode::Len, result_reg, operand_reg, 0);
                    },
                }
                
                // Free operand register
                self.reg_alloc.free(operand_reg);
                
                Ok(result_reg)
            },
        }
    }
}