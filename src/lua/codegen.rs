//! Lua Bytecode Generation Module
//!
//! This module implements bytecode generation for Lua 5.1, converting AST
//! into executable bytecode following the Lua 5.1 virtual machine specification.

use super::ast::*;
use super::error::{LuaError, LuaResult};
use std::collections::HashMap;

/// Compilation Constant types
#[derive(Debug, Clone, PartialEq)]
pub enum CompilationConstant {
    /// Nil constant
    Nil,
    
    /// Boolean constant 
    Boolean(bool),
    
    /// Number constant
    Number(f64),
    
    /// String constant (index into string table)
    String(usize),
    
    /// Function prototype constant (index into prototype table)
    FunctionProto(usize),
    
    /// Table constant (array of key-value pairs)
    Table(Vec<(CompilationConstant, CompilationConstant)>),
}

/// Compilation-time upvalue information
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompilationUpvalue {
    /// Is the upvalue in the current function's stack?
    pub in_stack: bool,
    
    /// Index (in stack if in_stack, otherwise in parent's upvalues)
    pub index: u8,
}

/// Compiled function representation
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledFunction {
    /// Bytecode instructions
    pub bytecode: Vec<u32>,
    
    /// Constants used by the function
    pub constants: Vec<CompilationConstant>,
    
    /// Number of parameters
    pub num_params: u8,
    
    /// Is the function variadic?
    pub is_vararg: bool,
    
    /// Maximum stack size needed
    pub max_stack_size: u8,
    
    /// Upvalue information
    pub upvalues: Vec<CompilationUpvalue>,
    
    /// Nested function prototypes (immediate children only)
    pub prototypes: Vec<CompiledFunction>,
    
    /// Debug information
    pub debug_info: Option<DebugInfo>,
}

/// Debug information for a compiled function
#[derive(Debug, Clone, PartialEq)]
pub struct DebugInfo {
    /// Source file name
    pub source: String,
    
    /// Line information (PC -> line number)
    pub line_info: Vec<u32>,
    
    /// Local variable names
    pub local_names: Vec<String>,
}

/// Complete compilation output
#[derive(Debug, Clone, PartialEq)]
pub struct CompleteCompilationOutput {
    /// The main function
    pub main: CompiledFunction,
    
    /// String table (shared across all functions)
    pub strings: Vec<String>,
}

/// Lua 5.1 Opcodes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OpCode {
    Move = 0,
    LoadK = 1,
    LoadBool = 2,
    LoadNil = 3,
    GetUpval = 4,
    GetGlobal = 5,
    GetTable = 6,
    SetGlobal = 7,
    SetUpval = 8,
    SetTable = 9,
    NewTable = 10,
    SelfOp = 11,
    Add = 12,
    Sub = 13,
    Mul = 14,
    Div = 15,
    Mod = 16,
    Pow = 17,
    Unm = 18,
    Not = 19,
    Len = 20,
    Concat = 21,
    Jmp = 22,
    Eq = 23,
    Lt = 24,
    Le = 25,
    Test = 26,
    TestSet = 27,
    Call = 28,
    TailCall = 29,
    Return = 30,
    ForLoop = 31,
    ForPrep = 32,
    TForLoop = 33,
    SetList = 34,
    Close = 35,
    Closure = 36,
    VarArg = 37,
}

/// Instruction format for Lua 5.1
#[derive(Debug, Clone, Copy)]
pub struct Instruction(pub u32);

impl Instruction {
    /// Size of opcode field (bits)
    const SIZE_OP: u32 = 6;
    
    /// Size of A field (bits)
    const SIZE_A: u32 = 8;
    
    /// Size of B/C fields (bits)
    const SIZE_B: u32 = 9;
    const SIZE_C: u32 = 9;
    
    /// Size of Bx field (bits)
    const SIZE_BX: u32 = Self::SIZE_B + Self::SIZE_C;
    
    /// Position of fields
    const POS_OP: u32 = 0;
    const POS_A: u32 = Self::POS_OP + Self::SIZE_OP;
    const POS_B: u32 = Self::POS_A + Self::SIZE_A;
    const POS_C: u32 = Self::POS_B + Self::SIZE_B;
    
    /// Maximum values
    const MAXARG_A: u32 = (1 << Self::SIZE_A) - 1;
    const MAXARG_B: u32 = (1 << Self::SIZE_B) - 1;
    const MAXARG_C: u32 = (1 << Self::SIZE_C) - 1;
    const MAXARG_BX: u32 = (1 << Self::SIZE_BX) - 1;
    const MAXARG_SBX: i32 = (Self::MAXARG_BX >> 1) as i32;
    
    /// Bit flag for constants
    const BITRK: u32 = 1 << (Self::SIZE_B - 1);
    
    /// Create instruction with ABC format
    pub fn create_ABC(op: OpCode, a: u32, b: u32, c: u32) -> Self {
        let mut inst = 0u32;
        inst |= (op as u32) << Self::POS_OP;
        inst |= a << Self::POS_A;
        inst |= b << Self::POS_B;
        inst |= c << Self::POS_C;
        Instruction(inst)
    }
    
    /// Create instruction with ABx format
    pub fn create_ABx(op: OpCode, a: u32, bx: u32) -> Self {
        let mut inst = 0u32;
        inst |= (op as u32) << Self::POS_OP;
        inst |= a << Self::POS_A;
        inst |= bx << Self::POS_B;
        Instruction(inst)
    }
    
    /// Create instruction with AsBx format
    pub fn create_AsBx(op: OpCode, a: u32, sbx: i32) -> Self {
        let bx = (sbx + Self::MAXARG_SBX) as u32;
        Self::create_ABx(op, a, bx)
    }
    
    /// Get opcode
    pub fn get_opcode(&self) -> OpCode {
        let op = (self.0 >> Self::POS_OP) & ((1 << Self::SIZE_OP) - 1);
        unsafe { std::mem::transmute(op as u8) }
    }
    
    /// Get A field
    pub fn get_a(&self) -> u32 {
        (self.0 >> Self::POS_A) & Self::MAXARG_A
    }
    
    /// Get B field
    pub fn get_b(&self) -> u32 {
        (self.0 >> Self::POS_B) & Self::MAXARG_B
    }
    
    /// Get C field
    pub fn get_c(&self) -> u32 {
        (self.0 >> Self::POS_C) & Self::MAXARG_C
    }
    
    /// Get Bx field
    pub fn get_bx(&self) -> u32 {
        (self.0 >> Self::POS_B) & Self::MAXARG_BX
    }
    
    /// Get sBx field
    pub fn get_sbx(&self) -> i32 {
        (self.get_bx() as i32) - Self::MAXARG_SBX
    }
    
    /// Check if B is a constant
    pub fn is_b_constant(&self) -> bool {
        self.get_b() & Self::BITRK != 0
    }
    
    /// Check if C is a constant
    pub fn is_c_constant(&self) -> bool {
        self.get_c() & Self::BITRK != 0
    }
    
    /// Get B as register or constant index
    pub fn get_rk_b(&self) -> (bool, u32) {
        let b = self.get_b();
        (b & Self::BITRK != 0, b & !Self::BITRK)
    }
    
    /// Get C as register or constant index
    pub fn get_rk_c(&self) -> (bool, u32) {
        let c = self.get_c();
        (c & Self::BITRK != 0, c & !Self::BITRK)
    }
    
    /// Encode constant index for B/C fields
    pub fn encode_constant(index: u32) -> u32 {
        index | Self::BITRK
    }
}

struct CodeGenContext {
    /// Current function being compiled
    current_function: CompiledFunction,
    
    /// Local variable mapping (name -> register)
    locals: HashMap<String, u8>,
    
    /// Next free register
    free_register: u8,
    
    /// String table (global)
    strings: Vec<String>,
    
    /// Parent context (for nested functions)
    parent: Option<Box<CodeGenContext>>,
    
    /// Break jump targets (for loop control)
    break_targets: Vec<usize>,
    
    /// Continue jump targets (for loop control)
    continue_targets: Vec<usize>,
}

impl CodeGenContext {
    /// Create a new context for the main function
    fn new() -> Self {
        CodeGenContext {
            current_function: CompiledFunction {
                bytecode: Vec::new(),
                constants: Vec::new(),
                num_params: 0,
                is_vararg: true, // Main chunk is always vararg
                max_stack_size: 2, // Minimum stack size
                upvalues: Vec::new(),
                prototypes: Vec::new(),
                debug_info: None,
            },
            locals: HashMap::new(),
            free_register: 0,
            strings: Vec::new(),
            parent: None,
            break_targets: Vec::new(),
            continue_targets: Vec::new(),
        }
    }
    
    /// Create a child context for nested function
    fn child(&mut self) -> Self {
        CodeGenContext {
            current_function: CompiledFunction {
                bytecode: Vec::new(),
                constants: Vec::new(),
                num_params: 0,
                is_vararg: false,
                max_stack_size: 2,
                upvalues: Vec::new(),
                prototypes: Vec::new(),
                debug_info: None,
            },
            locals: HashMap::new(),
            free_register: 0,
            strings: Vec::new(), // Will be moved to parent later
            parent: None, // Will be set properly later
            break_targets: Vec::new(),
            continue_targets: Vec::new(),
        }
    }
    
    /// Free registers down to a certain level
    fn free_registers_to(&mut self, level: u8) {
        self.free_register = level;
    }
    
    /// Emit an instruction
    fn emit(&mut self, instruction: Instruction) {
        self.current_function.bytecode.push(instruction.0);
    }
    
    /// Get current PC
    fn current_pc(&self) -> usize {
        self.current_function.bytecode.len()
    }
    
    /// Allocate a register
    fn allocate_register(&mut self) -> LuaResult<u8> {
        if self.free_register >= 250 {
            return Err(LuaError::CompileError("Too many registers".to_string()));
        }
        let reg = self.free_register;
        self.free_register += 1;
        
        // Update max stack size
        if self.free_register > self.current_function.max_stack_size {
            self.current_function.max_stack_size = self.free_register;
        }
        
        Ok(reg)
    }
    
    /// Add a constant and return its index
    fn add_constant(&mut self, constant: CompilationConstant) -> LuaResult<u32> {
        // Check if constant already exists
        for (i, existing) in self.current_function.constants.iter().enumerate() {
            if existing == &constant {
                return Ok(i as u32);
            }
        }
        
        // Add new constant
        let index = self.current_function.constants.len();
        if index > 0x1FFFF { // Max constant index for Bx format
            return Err(LuaError::CompileError("Too many constants".to_string()));
        }
        
        self.current_function.constants.push(constant);
        Ok(index as u32)
    }
    
    /// Add a string to the string table and return its index
    fn add_string(&mut self, s: &str) -> usize {
        // Check if string already exists
        for (i, existing) in self.strings.iter().enumerate() {
            if existing == s {
                return i;
            }
        }
        
        // Add new string
        let index = self.strings.len();
        self.strings.push(s.to_string());
        index
    }
    
    /// Add a local variable
    fn add_local(&mut self, name: &str, reg: u8) {
        self.locals.insert(name.to_string(), reg);
    }
    
    /// Look up a local variable
    fn lookup_local(&self, name: &str) -> Option<u8> {
        self.locals.get(name).copied()
    }
}

/// Resolve an upvalue through parent contexts
fn resolve_upvalue(ctx: &mut CodeGenContext, name: &str, parent: Option<&CodeGenContext>) -> Option<u8> {
    // If we have no parent, we can't have upvalues
    let parent_ctx = parent?;
    
    // First, check if the parent has this as a local
    if let Some(parent_local_reg) = parent_ctx.lookup_local(name) {
        // The parent has this variable as a local
        // We need to create an upvalue that references the parent's local
        
        // Check if we already have this upvalue
        for (i, upval) in ctx.current_function.upvalues.iter().enumerate() {
            if upval.in_stack && upval.index == parent_local_reg {
                return Some(i as u8);
            }
        }
        
        // Create a new upvalue
        let upval_idx = ctx.current_function.upvalues.len() as u8;
        ctx.current_function.upvalues.push(CompilationUpvalue {
            in_stack: true,
            index: parent_local_reg,
        });
        
        return Some(upval_idx);
    }
    
    // If the parent doesn't have it as a local, check if it has it as an upvalue
    // This would require recursively checking the parent's parent
    // For now, we'll leave this unimplemented as the basic case should work
    
    None
}

/// Generate bytecode from AST
pub fn generate_bytecode(chunk: &Chunk) -> LuaResult<CompleteCompilationOutput> {
    let mut ctx = CodeGenContext::new();
    
    // Compile the chunk
    compile_chunk(&mut ctx, chunk)?;
    
    // If there's no explicit return, add a default return
    if chunk.return_statement.is_none() && !ends_with_return(&chunk.statements) {
        ctx.emit(Instruction::create_ABC(OpCode::Return, 0, 1, 0));
    }
    
    Ok(CompleteCompilationOutput {
        main: ctx.current_function,
        strings: ctx.strings,
    })
}

/// Check if statements end with a return
fn ends_with_return(statements: &[Statement]) -> bool {
    if let Some(last) = statements.last() {
        matches!(last, Statement::Return { .. })
    } else {
        false
    }
}

/// Compile a chunk
fn compile_chunk(ctx: &mut CodeGenContext, chunk: &Chunk) -> LuaResult<()> {
    // Compile all statements
    for statement in &chunk.statements {
        compile_statement(ctx, statement)?;
    }
    
    // Compile return statement if present
    if let Some(ret) = &chunk.return_statement {
        compile_return_statement(ctx, &ret.expressions)?;
    }
    
    Ok(())
}

/// Compile a statement
fn compile_statement(ctx: &mut CodeGenContext, statement: &Statement) -> LuaResult<()> {
    compile_statement_with_parent(ctx, statement, None)
}

/// Compile a statement with parent context
fn compile_statement_with_parent(
    ctx: &mut CodeGenContext, 
    statement: &Statement,
    parent: Option<&CodeGenContext>
) -> LuaResult<()> {
    match statement {
        Statement::LocalDeclaration(decl) => compile_local_declaration_with_parent(ctx, decl, parent),
        Statement::Assignment(assign) => compile_assignment_with_parent(ctx, assign, parent),
        Statement::FunctionCall(call) => {
            compile_function_call_with_parent(ctx, call, parent)?;
            Ok(())
        }
        Statement::Return { expressions } => compile_return_statement_with_parent(ctx, expressions, parent),
        Statement::If { condition, body, else_ifs, else_block } => {
            compile_if_statement_with_parent(ctx, condition, body, else_ifs, else_block, parent)
        }
        Statement::While { condition, body } => compile_while_loop_with_parent(ctx, condition, body, parent),
        Statement::Do(block) => compile_block_with_parent(ctx, block, parent),
        Statement::ForLoop { variable, initial, limit, step, body } => {
            compile_for_loop_with_parent(ctx, variable, initial, limit, step.as_ref(), body, parent)
        }
        Statement::ForInLoop { variables, iterators, body } => {
            compile_for_in_loop_with_parent(ctx, variables, iterators, body, parent)
        }
        Statement::LocalFunctionDefinition { name, parameters, is_vararg, body } => {
            compile_local_function_definition_with_parent(ctx, name, parameters, *is_vararg, body)
        }
        Statement::FunctionDefinition { name, parameters, is_vararg, body } => {
            compile_function_definition_with_parent(ctx, name, parameters, *is_vararg, body)
        }
        _ => Err(LuaError::NotImplemented(format!("Statement type: {:?}", statement))),
    }
}

/// Compile a local variable declaration
fn compile_local_declaration(ctx: &mut CodeGenContext, decl: &LocalDeclaration) -> LuaResult<()> {
    compile_local_declaration_with_parent(ctx, decl, None)
}

/// Compile a local variable declaration with parent context
fn compile_local_declaration_with_parent(
    ctx: &mut CodeGenContext, 
    decl: &LocalDeclaration,
    parent: Option<&CodeGenContext>
) -> LuaResult<()> {
    let num_names = decl.names.len();
    let num_exprs = decl.expressions.len();
    
    // Allocate registers for locals
    let base_reg = ctx.allocate_register()?;
    for i in 1..num_names {
        ctx.allocate_register()?;
    }
    
    // Compile expressions
    if num_exprs > 0 {
        for (i, expr) in decl.expressions.iter().enumerate() {
            if i < num_names {
                compile_expression_to_register_with_parent(ctx, expr, base_reg + i as u8, parent)?;
            }
        }
    }
    
    // Initialize remaining locals to nil
    if num_exprs < num_names {
        let start = base_reg + num_exprs as u8;
        let count = (num_names - num_exprs) as u8;
        ctx.emit(Instruction::create_ABC(OpCode::LoadNil, start as u32, (start + count - 1) as u32, 0));
    }
    
    // Register local names
    for (i, name) in decl.names.iter().enumerate() {
        ctx.add_local(name, base_reg + i as u8);
    }
    
    Ok(())
}

/// Compile an assignment
fn compile_assignment(ctx: &mut CodeGenContext, assign: &Assignment) -> LuaResult<()> {
    compile_assignment_with_parent(ctx, assign, None)
}

/// Compile an assignment with parent context
fn compile_assignment_with_parent(
    ctx: &mut CodeGenContext, 
    assign: &Assignment,
    parent: Option<&CodeGenContext>
) -> LuaResult<()> {
    // For now, handle simple local variable assignments
    if assign.variables.len() == 1 && assign.expressions.len() == 1 {
        match &assign.variables[0] {
            Variable::Name(name) => {
                if let Some(reg) = ctx.lookup_local(name) {
                    // Assign to existing local
                    compile_expression_to_register_with_parent(ctx, &assign.expressions[0], reg, parent)?;
                    return Ok(());
                } else if let Some(upval_idx) = resolve_upvalue(ctx, name, parent) {
                    // Assign to upvalue
                    let value_reg = ctx.allocate_register()?;
                    compile_expression_to_register_with_parent(ctx, &assign.expressions[0], value_reg, parent)?;
                    
                    ctx.emit(Instruction::create_ABC(OpCode::SetUpval, value_reg as u32, upval_idx as u32, 0));
                    ctx.free_registers_to(value_reg);
                    return Ok(());
                } else {
                    // Global assignment
                    let value_reg = ctx.allocate_register()?;
                    compile_expression_to_register_with_parent(ctx, &assign.expressions[0], value_reg, parent)?;
                    
                    let string_idx = ctx.add_string(name);
                    let const_idx = ctx.add_constant(CompilationConstant::String(string_idx))?;
                    
                    ctx.emit(Instruction::create_ABx(OpCode::SetGlobal, value_reg as u32, const_idx));
                    ctx.free_registers_to(value_reg);
                    return Ok(());
                }
            }
            Variable::Member { table, field } => {
                // Table member assignment: table.field = value
                let table_reg = ctx.allocate_register()?;
                compile_expression_to_register_with_parent(ctx, table, table_reg, parent)?;
                
                let value_reg = ctx.allocate_register()?;
                compile_expression_to_register_with_parent(ctx, &assign.expressions[0], value_reg, parent)?;
                
                // Create string constant for the field
                let field_idx = ctx.add_string(field);
                let field_const = ctx.add_constant(CompilationConstant::String(field_idx))?;
                
                // SETTABLE: table[field] = value
                ctx.emit(Instruction::create_ABC(
                    OpCode::SetTable,
                    table_reg as u32,
                    Instruction::encode_constant(field_const),
                    value_reg as u32
                ));
                
                ctx.free_registers_to(table_reg);
                return Ok(());
            }
            Variable::Index { table, key } => {
                // Table index assignment: table[key] = value
                let table_reg = ctx.allocate_register()?;
                compile_expression_to_register_with_parent(ctx, table, table_reg, parent)?;
                
                let key_reg = ctx.allocate_register()?;
                compile_expression_to_register_with_parent(ctx, key, key_reg, parent)?;
                
                let value_reg = ctx.allocate_register()?;
                compile_expression_to_register_with_parent(ctx, &assign.expressions[0], value_reg, parent)?;
                
                // SETTABLE: table[key] = value
                ctx.emit(Instruction::create_ABC(
                    OpCode::SetTable,
                    table_reg as u32,
                    key_reg as u32,
                    value_reg as u32
                ));
                
                ctx.free_registers_to(table_reg);
                return Ok(());
            }
        }
    }
    
    Err(LuaError::NotImplemented("Complex assignments".to_string()))
}

/// Compile an expression to a specific register
fn compile_expression_to_register(ctx: &mut CodeGenContext, expr: &Expression, target: u8) -> LuaResult<()> {
    compile_expression_to_register_with_parent(ctx, expr, target, None)
}

/// Compile an expression to a specific register with parent context for upvalue resolution
fn compile_expression_to_register_with_parent(
    ctx: &mut CodeGenContext, 
    expr: &Expression, 
    target: u8,
    parent: Option<&CodeGenContext>
) -> LuaResult<()> {
    match expr {
        Expression::Nil => {
            ctx.emit(Instruction::create_ABC(OpCode::LoadNil, target as u32, target as u32, 0));
        }
        Expression::Boolean(b) => {
            ctx.emit(Instruction::create_ABC(OpCode::LoadBool, target as u32, if *b { 1 } else { 0 }, 0));
        }
        Expression::Number(n) => {
            let const_idx = ctx.add_constant(CompilationConstant::Number(*n))?;
            ctx.emit(Instruction::create_ABx(OpCode::LoadK, target as u32, const_idx));
        }
        Expression::String(s) => {
            let string_idx = ctx.add_string(s);
            let const_idx = ctx.add_constant(CompilationConstant::String(string_idx))?;
            ctx.emit(Instruction::create_ABx(OpCode::LoadK, target as u32, const_idx));
        }
        Expression::Variable(var) => {
            compile_variable_to_register_with_parent(ctx, var, target, parent)?;
        }
        Expression::BinaryOp { left, operator, right } => {
            compile_binary_op_with_parent(ctx, left, operator, right, target, parent)?;
        }
        Expression::UnaryOp { operator, operand } => {
            compile_unary_op_with_parent(ctx, operator, operand, target, parent)?;
        }
        Expression::FunctionCall(call) => {
            let base = compile_function_call_with_parent(ctx, call, parent)?;
            if base != target {
                ctx.emit(Instruction::create_ABC(OpCode::Move, target as u32, base as u32, 0));
            }
        }
        Expression::TableConstructor(tc) => {
            compile_table_constructor_with_parent(ctx, tc, target, parent)?;
        }
        Expression::FunctionDef { parameters, is_vararg, body } => {
            compile_function_expression_with_parent(ctx, parameters, *is_vararg, body, target)?;
        }
        Expression::VarArg => {
            ctx.emit(Instruction::create_ABC(OpCode::VarArg, target as u32, 0, 0));
        }
        _ => return Err(LuaError::NotImplemented(format!("Expression type: {:?}", expr))),
    }
    
    Ok(())
}

/// Compile a table constructor
fn compile_table_constructor(ctx: &mut CodeGenContext, tc: &TableConstructor, target: u8) -> LuaResult<()> {
    compile_table_constructor_with_parent(ctx, tc, target, None)
}

/// Compile a table constructor with parent context
fn compile_table_constructor_with_parent(
    ctx: &mut CodeGenContext, 
    tc: &TableConstructor, 
    target: u8,
    parent: Option<&CodeGenContext>
) -> LuaResult<()> {
    eprintln!("DEBUG: compile_table_constructor - target: {}, free_register: {}", target, ctx.free_register);
    
    // Count array and hash parts for size hints
    let mut array_count = 0;
    let mut hash_count = 0;
    
    for field in &tc.fields {
        match field {
            TableField::List(_) => array_count += 1,
            _ => hash_count += 1,
        }
    }
    
    // Create the table with size hints (B = array size, C = hash size)
    // Simply use 0, 0 for now - we'll optimize later
    ctx.emit(Instruction::create_ABC(OpCode::NewTable, target as u32, 0, 0));
    
    // Separate array fields and hash fields
    let mut array_fields = Vec::new();
    
    // First pass: collect array fields and process hash fields
    // Process hash fields immediately
    for field in &tc.fields {
        match field {
            TableField::List(expr) => {
                // Collect for later batch processing using SETLIST
                array_fields.push(expr);
            },
            
            TableField::Record { key, value } => {
                // Record-style entry: t.key = value
                // Allocate register for the value
                let value_reg = ctx.allocate_register()?;
                eprintln!("DEBUG: Table record field '{}' - compiling value to register {}", key, value_reg);
                compile_expression_to_register_with_parent(ctx, value, value_reg, parent)?;
                
                // Create string constant for the key
                let key_string_idx = ctx.add_string(key);
                let key_const = ctx.add_constant(CompilationConstant::String(key_string_idx))?;
                
                // SETTABLE: table[key] = value
                eprintln!("DEBUG: Emitting SETTABLE for record field - R({})[const({})] = R({})", target, key_const, value_reg);
                ctx.emit(Instruction::create_ABC(
                    OpCode::SetTable,
                    target as u32,
                    Instruction::encode_constant(key_const),
                    value_reg as u32
                ));
                
                ctx.free_registers_to(value_reg);
            },
            
            TableField::Index { key, value } => {
                // Computed index entry: t[key_expr] = value
                // Allocate registers for key and value
                let key_reg = ctx.allocate_register()?;
                eprintln!("DEBUG: Table index field - compiling key to register {}", key_reg);
                compile_expression_to_register_with_parent(ctx, key, key_reg, parent)?;
                
                let value_reg = ctx.allocate_register()?;
                eprintln!("DEBUG: Table index field - compiling value to register {}", value_reg);
                compile_expression_to_register_with_parent(ctx, value, value_reg, parent)?;
                
                // SETTABLE: table[key] = value
                eprintln!("DEBUG: Emitting SETTABLE for index field - R({})[R({})] = R({})", target, key_reg, value_reg);
                ctx.emit(Instruction::create_ABC(
                    OpCode::SetTable,
                    target as u32,
                    key_reg as u32,
                    value_reg as u32
                ));
                
                ctx.free_registers_to(key_reg);
            }
        }
    }
    
    // Second pass: If we have array fields, use SETLIST to batch them
    if !array_fields.is_empty() {
        eprintln!("DEBUG: Processing {} array fields with SETLIST", array_fields.len());
        
        // In Lua 5.1, SETLIST processes elements in batches of max 50 elements
        const FIELDS_PER_FLUSH: usize = 50;
        
        // Process array fields in batches
        for (batch_idx, batch) in array_fields.chunks(FIELDS_PER_FLUSH).enumerate() {
            eprintln!("DEBUG: Processing SETLIST batch {} with {} elements", batch_idx + 1, batch.len());
            
            // Reserve registers for the values
            let base_value_reg = ctx.free_register;
            
            // Compile values into consecutive registers
            for expr in batch {
                let reg = ctx.allocate_register()?;
                eprintln!("DEBUG: SETLIST batch {} - compiling value to register {}", batch_idx + 1, reg);
                compile_expression_to_register_with_parent(ctx, expr, reg, parent)?;
            }
            
            // Emit SETLIST
            let c = batch_idx + 1; // C is 1-based batch index where (C-1)*FPF+1 is starting array index
            eprintln!("DEBUG: Emitting SETLIST - R({})[{}..{}] = R({}..{})", 
                     target, c*FIELDS_PER_FLUSH - FIELDS_PER_FLUSH + 1, 
                     c*FIELDS_PER_FLUSH, 
                     base_value_reg, 
                     base_value_reg + batch.len() as u8 - 1);
                     
            ctx.emit(Instruction::create_ABC(
                OpCode::SetList,
                target as u32,
                batch.len() as u32, // B = number of elements in this batch
                c as u32            // C = batch index
            ));
            
            // Free registers used for values
            ctx.free_registers_to(base_value_reg);
        }
    }
    
    eprintln!("DEBUG: Table constructor complete, final free_register: {}", ctx.free_register);
    
    Ok(())
}

/// Compile a variable access to a register
fn compile_variable_to_register(ctx: &mut CodeGenContext, var: &Variable, target: u8) -> LuaResult<()> {
    compile_variable_to_register_with_parent(ctx, var, target, None)
}

/// Compile a variable access to a register with parent context
fn compile_variable_to_register_with_parent(
    ctx: &mut CodeGenContext, 
    var: &Variable, 
    target: u8,
    parent: Option<&CodeGenContext>
) -> LuaResult<()> {
    match var {
        Variable::Name(name) => {
            if let Some(reg) = ctx.lookup_local(name) {
                // Local variable
                if reg != target {
                    ctx.emit(Instruction::create_ABC(OpCode::Move, target as u32, reg as u32, 0));
                }
            } else if let Some(upval_idx) = resolve_upvalue(ctx, name, parent) {
                // Upvalue access
                ctx.emit(Instruction::create_ABC(OpCode::GetUpval, target as u32, upval_idx as u32, 0));
            } else {
                // Global variable
                let string_idx = ctx.add_string(name);
                let const_idx = ctx.add_constant(CompilationConstant::String(string_idx))?;
                ctx.emit(Instruction::create_ABx(OpCode::GetGlobal, target as u32, const_idx));
            }
        }
        Variable::Member { table, field } => {
            // Table member access: table.field
            eprintln!("DEBUG: compile_variable_to_register Member - target: {}, free_register: {}", target, ctx.free_register);
            
            // Always allocate a new register for table to avoid potential corruption
            let table_reg = ctx.allocate_register()?;
            
            eprintln!("DEBUG: Using table_reg: {} for table expression", table_reg);
            compile_expression_to_register_with_parent(ctx, table, table_reg, parent)?;
            
            // Create string constant for the field name
            let field_idx = ctx.add_string(field);
            let field_const = ctx.add_constant(CompilationConstant::String(field_idx))?;
            
            // GETTABLE: R(target) = table[field]
            eprintln!("DEBUG: Emitting GETTABLE - R({}) = R({})[const({})]", target, table_reg, field_const);
            ctx.emit(Instruction::create_ABC(
                OpCode::GetTable,
                target as u32,
                table_reg as u32,
                Instruction::encode_constant(field_const)
            ));
            
            // Free temporary registers
            ctx.free_registers_to(table_reg);
        }
        Variable::Index { table, key } => {
            // Table index access: table[key]
            eprintln!("DEBUG: compile_variable_to_register Index - target: {}, free_register: {}", target, ctx.free_register);
            
            // Always allocate new registers for table and key to avoid corruption
            let table_reg = ctx.allocate_register()?;
            compile_expression_to_register_with_parent(ctx, table, table_reg, parent)?;
            
            // Allocate key register - must be after table register
            let key_reg = ctx.allocate_register()?;
            compile_expression_to_register_with_parent(ctx, key, key_reg, parent)?;
            
            // GETTABLE: R(target) = table[key]
            eprintln!("DEBUG: Emitting GETTABLE - R({}) = R({})[R({})]", target, table_reg, key_reg);
            ctx.emit(Instruction::create_ABC(
                OpCode::GetTable,
                target as u32,
                table_reg as u32,
                key_reg as u32
            ));
            
            // Free temporary registers
            ctx.free_registers_to(table_reg);
        }
    }
    
    Ok(())
}

/// Compile a binary operation
fn compile_binary_op(ctx: &mut CodeGenContext, left: &Expression, op: &BinaryOperator, right: &Expression, target: u8) -> LuaResult<()> {
    compile_binary_op_with_parent(ctx, left, op, right, target, None)
}

/// Compile a binary operation with parent context
fn compile_binary_op_with_parent(
    ctx: &mut CodeGenContext, 
    left: &Expression, 
    op: &BinaryOperator, 
    right: &Expression, 
    target: u8,
    parent: Option<&CodeGenContext>
) -> LuaResult<()> {
    match op {
        // Arithmetic operations
        BinaryOperator::Add | BinaryOperator::Sub | BinaryOperator::Mul | 
        BinaryOperator::Div | BinaryOperator::Mod | BinaryOperator::Pow => {
            let opcode = match op {
                BinaryOperator::Add => OpCode::Add,
                BinaryOperator::Sub => OpCode::Sub,
                BinaryOperator::Mul => OpCode::Mul,
                BinaryOperator::Div => OpCode::Div,
                BinaryOperator::Mod => OpCode::Mod,
                BinaryOperator::Pow => OpCode::Pow,
                _ => unreachable!(),
            };
            
            eprintln!("DEBUG: compile_binary_op {:?} - target: {}, free_register: {}", op, target, ctx.free_register);
            
            // Always allocate new registers for operands to avoid corruption
            let left_reg = ctx.allocate_register()?;
            eprintln!("DEBUG: Compiling left operand to register {}", left_reg);
            compile_expression_to_register_with_parent(ctx, left, left_reg, parent)?;
            
            let right_reg = ctx.allocate_register()?;
            eprintln!("DEBUG: Compiling right operand to register {}", right_reg);
            compile_expression_to_register_with_parent(ctx, right, right_reg, parent)?;
            
            // Emit operation
            eprintln!("DEBUG: Emitting {:?} - R({}) = R({}) op R({})", opcode, target, left_reg, right_reg);
            ctx.emit(Instruction::create_ABC(opcode, target as u32, left_reg as u32, right_reg as u32));
            
            // Free temporary registers
            ctx.free_registers_to(left_reg);
        }
        
        BinaryOperator::Concat => {
            eprintln!("DEBUG: compile_binary_op Concat - target: {}, free_register: {}", target, ctx.free_register);
             
            // Always allocate new registers for operands to avoid corruption
            let left_reg = ctx.allocate_register()?;
            compile_expression_to_register_with_parent(ctx, left, left_reg, parent)?;
            
            let right_reg = ctx.allocate_register()?;
            compile_expression_to_register_with_parent(ctx, right, right_reg, parent)?;
            
            // Emit operation
            eprintln!("DEBUG: Emitting CONCAT - R({}) = R({})..R({})", target, left_reg, right_reg);
            ctx.emit(Instruction::create_ABC(OpCode::Concat, target as u32, left_reg as u32, right_reg as u32));
            
            // Free temporary registers
            ctx.free_registers_to(left_reg);
        }
        
        BinaryOperator::Or => {
            // Short-circuit OR: if left is true, use left, else use right
            // First compile left operand to target
            compile_expression_to_register_with_parent(ctx, left, target, parent)?;
            
            // TESTSET: if R(target) is truthy, skip next instruction, else R(target) := R(right_reg)
            // We need to compile right first to a temp register
            let skip_pc = ctx.current_pc();
            // Placeholder TESTSET - will patch after compiling right
            ctx.emit(Instruction::create_ABC(OpCode::TestSet, target as u32, target as u32, 1)); // C=1 means test for truthy
            
            // Compile right operand to target (only executed if left was falsey)
            compile_expression_to_register_with_parent(ctx, right, target, parent)?;
            
            // Patch the TESTSET instruction
            ctx.current_function.bytecode[skip_pc] = Instruction::create_ABC(OpCode::TestSet, target as u32, target as u32, 1).0;
            
            ctx.free_registers_to(target + 1);
        }
        
        BinaryOperator::And => {
            // Short-circuit AND: if left is false, use left, else use right
            // First compile left operand to target
            compile_expression_to_register_with_parent(ctx, left, target, parent)?;
            
            // TESTSET: if R(target) is falsey, skip next instruction, else R(target) := R(right_reg)
            ctx.emit(Instruction::create_ABC(OpCode::TestSet, target as u32, target as u32, 0)); // C=0 means test for falsey
            
            // Compile right operand to target (only executed if left was truthy)
            compile_expression_to_register_with_parent(ctx, right, target, parent)?;
            
            ctx.free_registers_to(target + 1);
        }
        
        BinaryOperator::Eq | BinaryOperator::Ne | BinaryOperator::Lt | 
        BinaryOperator::Le | BinaryOperator::Gt | BinaryOperator::Ge => {
            eprintln!("DEBUG: compile_binary_op comparison {:?} - target: {}, free_register: {}", op, target, ctx.free_register);
            
            // Always allocate new registers for operands to avoid corruption
            let left_reg = ctx.allocate_register()?;
            compile_expression_to_register_with_parent(ctx, left, left_reg, parent)?;
            
            let right_reg = ctx.allocate_register()?;
            compile_expression_to_register_with_parent(ctx, right, right_reg, parent)?;
            
            // The comparison instructions work by skipping the next instruction if the test fails
            let (opcode, invert) = match op {
                BinaryOperator::Eq => (OpCode::Eq, false),
                BinaryOperator::Ne => (OpCode::Eq, true), // NE is inverted EQ
                BinaryOperator::Lt => (OpCode::Lt, false),
                BinaryOperator::Le => (OpCode::Le, false),
                BinaryOperator::Gt => (OpCode::Lt, true), // GT is inverted LT with swapped operands
                BinaryOperator::Ge => (OpCode::Le, true), // GE is inverted LE with swapped operands
                _ => unreachable!(),
            };
            
            // For GT and GE, we need to swap operands
            let (left_op, right_op) = match op {
                BinaryOperator::Gt | BinaryOperator::Ge => (right_reg as u32, left_reg as u32),
                _ => (left_reg as u32, right_reg as u32),
            };
            
            // Emit comparison (A = 1 if we want the test to succeed for true, 0 for false)
            ctx.emit(Instruction::create_ABC(opcode, if invert { 0 } else { 1 }, left_op, right_op));
            
            // Load false (will be skipped if comparison matches expectation)
            ctx.emit(Instruction::create_ABC(OpCode::LoadBool, target as u32, 0, 1)); // C=1 means skip next
            
            // Load true
            ctx.emit(Instruction::create_ABC(OpCode::LoadBool, target as u32, 1, 0));
            
            // Free temporary registers
            ctx.free_registers_to(left_reg);
        }
        
        _ => return Err(LuaError::NotImplemented(format!("Binary operator: {:?}", op))),
    }
    
    Ok(())
}

/// Compile a unary operation
fn compile_unary_op(ctx: &mut CodeGenContext, op: &UnaryOperator, operand: &Expression, target: u8) -> LuaResult<()> {
    compile_unary_op_with_parent(ctx, op, operand, target, None)
}

/// Compile a unary operation with parent context
fn compile_unary_op_with_parent(
    ctx: &mut CodeGenContext, 
    op: &UnaryOperator, 
    operand: &Expression, 
    target: u8,
    parent: Option<&CodeGenContext>
) -> LuaResult<()> {
    let opcode = match op {
        UnaryOperator::Not => OpCode::Not,
        UnaryOperator::Minus => OpCode::Unm,
        UnaryOperator::Length => OpCode::Len,
    };
    
    eprintln!("DEBUG: compile_unary_op {:?} - target: {}, free_register: {}", op, target, ctx.free_register);
    
    // Always allocate a new register for the operand to avoid corruption
    let operand_reg = ctx.allocate_register()?;
    
    eprintln!("DEBUG: Compiling operand to register {}", operand_reg);
    compile_expression_to_register_with_parent(ctx, operand, operand_reg, parent)?;
    
    eprintln!("DEBUG: Emitting {:?} - R({}) = op R({})", opcode, target, operand_reg);
    ctx.emit(Instruction::create_ABC(opcode, target as u32, operand_reg as u32, 0));
    
    // Free temporary registers
    ctx.free_registers_to(operand_reg);
    
    Ok(())
}

/// Compile a function call
fn compile_function_call(ctx: &mut CodeGenContext, call: &FunctionCall) -> LuaResult<u8> {
    compile_function_call_with_parent(ctx, call, None)
}

/// Compile a function call with parent context
fn compile_function_call_with_parent(
    ctx: &mut CodeGenContext, 
    call: &FunctionCall,
    parent: Option<&CodeGenContext>
) -> LuaResult<u8> {
    // Check if this is a method call (table:method())
    if let Some(method_name) = &call.method {
        // Method call - use SELF instruction
        
        // Allocate registers for function and self (must be consecutive)
        let func_reg = ctx.allocate_register()?;
        let self_reg = ctx.allocate_register()?; // This should be func_reg + 1
        
        debug_assert_eq!(self_reg, func_reg + 1, "SELF requires consecutive registers");
        
        // Compile the table expression to get the object
        compile_expression_to_register_with_parent(ctx, &call.function, func_reg, parent)?;
        
        // Create string constant for the method name
        let method_idx = ctx.add_string(method_name);
        let method_const = ctx.add_constant(CompilationConstant::String(method_idx))?;
        
        // Emit SELF instruction: R(A+1) := R(B); R(A) := R(B)[RK(C)]
        // This sets R(func_reg) = table[method] and R(func_reg+1) = table
        ctx.emit(Instruction::create_ABC(
            OpCode::SelfOp,
            func_reg as u32,
            func_reg as u32,  // B is the table register (same as func_reg before SELF)
            Instruction::encode_constant(method_const),
        ));
        
        // Compile arguments (starting after the implicit self parameter)
        let args = match &call.args {
            CallArgs::Args(exprs) => exprs,
            _ => return Err(LuaError::NotImplemented("Special call syntax".to_string())),
        };
        
        for arg in args {
            let arg_reg = ctx.allocate_register()?;
            compile_expression_to_register_with_parent(ctx, arg, arg_reg, parent)?;
        }
        
        // Emit CALL instruction
        // For method calls, nargs includes the implicit self parameter
        let nargs = args.len() as u32 + 2; // +1 for function, +1 for self
        let nresults = 2; // 1 result + 1
        ctx.emit(Instruction::create_ABC(OpCode::Call, func_reg as u32, nargs, nresults));
        
        // Free registers except result
        ctx.free_registers_to(func_reg + 1);
        
        Ok(func_reg)
    } else {
        // Regular function call
        
        // Allocate register for function
        let func_reg = ctx.allocate_register()?;
        
        // Compile function expression
        compile_expression_to_register_with_parent(ctx, &call.function, func_reg, parent)?;
        
        // Compile arguments
        let args = match &call.args {
            CallArgs::Args(exprs) => exprs,
            _ => return Err(LuaError::NotImplemented("Special call syntax".to_string())),
        };
        
        for arg in args {
            let arg_reg = ctx.allocate_register()?;
            compile_expression_to_register_with_parent(ctx, arg, arg_reg, parent)?;
        }
        
        // Emit CALL instruction
        let nargs = args.len() as u32 + 1; // +1 because B includes the function
        let nresults = 2; // 1 result + 1
        ctx.emit(Instruction::create_ABC(OpCode::Call, func_reg as u32, nargs, nresults));
        
        // Free registers except result
        ctx.free_registers_to(func_reg + 1);
        
        Ok(func_reg)
    }
}

/// Compile return statement
fn compile_return_statement(ctx: &mut CodeGenContext, expressions: &[Expression]) -> LuaResult<()> {
    compile_return_statement_with_parent(ctx, expressions, None)
}

/// Compile return statement with parent context
fn compile_return_statement_with_parent(
    ctx: &mut CodeGenContext, 
    expressions: &[Expression],
    parent: Option<&CodeGenContext>
) -> LuaResult<()> {
    let base_reg = ctx.free_register;
    
    // Compile return values
    for expr in expressions {
        let reg = ctx.allocate_register()?;
        compile_expression_to_register_with_parent(ctx, expr, reg, parent)?;
    }
    
    // Emit RETURN instruction
    let nresults = expressions.len() as u32 + 1; // +1 because B includes base
    ctx.emit(Instruction::create_ABC(OpCode::Return, base_reg as u32, nresults, 0));
    
    Ok(())
}

/// Compile if statement
fn compile_if_statement(
    ctx: &mut CodeGenContext,
    condition: &Expression,
    body: &Block,
    else_ifs: &[(Expression, Block)],
    else_block: &Option<Block>,
) -> LuaResult<()> {
    compile_if_statement_with_parent(ctx, condition, body, else_ifs, else_block, None)
}

/// Compile if statement with parent context
fn compile_if_statement_with_parent(
    ctx: &mut CodeGenContext,
    condition: &Expression,
    body: &Block,
    else_ifs: &[(Expression, Block)],
    else_block: &Option<Block>,
    parent: Option<&CodeGenContext>,
) -> LuaResult<()> {
    let mut jump_to_end = Vec::new();
    
    // Compile main if condition
    let cond_reg = ctx.allocate_register()?;
    compile_expression_to_register_with_parent(ctx, condition, cond_reg, parent)?;
    
    // TEST instruction - skip next instruction if false
    ctx.emit(Instruction::create_ABC(OpCode::Test, cond_reg as u32, 0, 1)); // C=1 means skip if false
    let jump_false = ctx.current_pc();
    ctx.emit(Instruction::create_AsBx(OpCode::Jmp, 0, 0)); // Placeholder
    
    ctx.free_registers_to(cond_reg);
    
    // Compile then body
    compile_block_with_parent(ctx, body, parent)?;
    
    // Jump to end
    jump_to_end.push(ctx.current_pc());
    ctx.emit(Instruction::create_AsBx(OpCode::Jmp, 0, 0)); // Placeholder
    
    // Patch jump for false condition
    let jump_offset = (ctx.current_pc() - jump_false - 1) as i32;
    ctx.current_function.bytecode[jump_false] = Instruction::create_AsBx(OpCode::Jmp, 0, jump_offset).0;
    
    // TODO: Compile else-ifs and else block
    
    // Patch all jumps to end
    for jump_pc in jump_to_end {
        let jump_offset = (ctx.current_pc() - jump_pc - 1) as i32;
        ctx.current_function.bytecode[jump_pc] = Instruction::create_AsBx(OpCode::Jmp, 0, jump_offset).0;
    }
    
    Ok(())
}

/// Compile while loop
fn compile_while_loop(ctx: &mut CodeGenContext, condition: &Expression, body: &Block) -> LuaResult<()> {
    compile_while_loop_with_parent(ctx, condition, body, None)
}

/// Compile while loop with parent context
fn compile_while_loop_with_parent(
    ctx: &mut CodeGenContext, 
    condition: &Expression, 
    body: &Block,
    parent: Option<&CodeGenContext>
) -> LuaResult<()> {
    let loop_start = ctx.current_pc();
    
    // Compile condition
    let cond_reg = ctx.allocate_register()?;
    compile_expression_to_register_with_parent(ctx, condition, cond_reg, parent)?;
    
    // TEST instruction - skip next instruction if false
    ctx.emit(Instruction::create_ABC(OpCode::Test, cond_reg as u32, 0, 1));
    let jump_false = ctx.current_pc();
    ctx.emit(Instruction::create_AsBx(OpCode::Jmp, 0, 0)); // Placeholder
    
    ctx.free_registers_to(cond_reg);
    
    // Compile body
    compile_block_with_parent(ctx, body, parent)?;
    
    // Jump back to start
    let jump_offset = -(ctx.current_pc() as i32 - loop_start as i32 + 1);
    ctx.emit(Instruction::create_AsBx(OpCode::Jmp, 0, jump_offset));
    
    // Patch jump for false condition
    let jump_offset = (ctx.current_pc() - jump_false - 1) as i32;
    ctx.current_function.bytecode[jump_false] = Instruction::create_AsBx(OpCode::Jmp, 0, jump_offset).0;
    
    Ok(())
}

/// Compile a numerical for loop
fn compile_for_loop(
    ctx: &mut CodeGenContext,
    variable: &str,
    initial: &Expression,
    limit: &Expression,
    step: Option<&Expression>,
    body: &Block,
) -> LuaResult<()> {
    compile_for_loop_with_parent(ctx, variable, initial, limit, step, body, None)
}

/// Compile a numerical for loop with parent context
fn compile_for_loop_with_parent(
    ctx: &mut CodeGenContext,
    variable: &str,
    initial: &Expression,
    limit: &Expression,
    step: Option<&Expression>,
    body: &Block,
    parent: Option<&CodeGenContext>,
) -> LuaResult<()> {
    // Per Lua 5.1 spec, for loops use 4 consecutive registers:
    // R(A): internal loop index
    // R(A+1): limit value
    // R(A+2): step value
    // R(A+3): external loop index (user variable)
    
    // 1. CRITICAL: First, allocate all 4 consecutive registers BEFORE compiling any expressions
    let loop_base = ctx.allocate_register()?;      // R(A) - internal index
    let loop_limit_reg = ctx.allocate_register()?; // R(A+1) - limit
    let loop_step_reg = ctx.allocate_register()?;  // R(A+2) - step  
    let user_var = ctx.allocate_register()?;       // R(A+3) - user variable
    
    // Save current free register for later restoration
    let saved_free_register = ctx.free_register;
    
    // 2. Ensure next temporary register allocation doesn't interfere with loop registers
    // by setting free_register past our allocated registers
    ctx.free_register = user_var + 1;
    
    // 3. Compile expressions directly to target registers
    compile_expression_to_register_with_parent(ctx, initial, loop_base, parent)?;
    compile_expression_to_register_with_parent(ctx, limit, loop_limit_reg, parent)?;
    
    // Compile step or use constant 1
    if let Some(step_expr) = step {
        compile_expression_to_register_with_parent(ctx, step_expr, loop_step_reg, parent)?;
    } else {
        // Default step is 1
        let const_idx = ctx.add_constant(CompilationConstant::Number(1.0))?;
        ctx.emit(Instruction::create_ABx(OpCode::LoadK, loop_step_reg as u32, const_idx));
    }
    
    // 4. Emit FORPREP with correct base register
    let forprep_pc = ctx.current_pc();
    ctx.emit(Instruction::create_AsBx(OpCode::ForPrep, loop_base as u32, 0)); // Placeholder
    
    // 5. Register the loop variable as a local
    let saved_locals = ctx.locals.clone();
    ctx.add_local(variable, user_var);
    
    // 6. Compile loop body
    let loop_start = ctx.current_pc();
    compile_block_with_parent(ctx, body, parent)?;
    
    // 7. Emit FORLOOP that jumps back to loop start
    let current_pc = ctx.current_pc() as i32;
    let loop_start_i32 = loop_start as i32;
    let loop_offset = -(current_pc - loop_start_i32 + 1);
    ctx.emit(Instruction::create_AsBx(OpCode::ForLoop, loop_base as u32, loop_offset));
    
    // 8. Patch FORPREP to jump to end if init condition fails
    let prep_offset = (ctx.current_pc() - forprep_pc - 1) as i32;
    ctx.current_function.bytecode[forprep_pc] = 
        Instruction::create_AsBx(OpCode::ForPrep, loop_base as u32, prep_offset).0;
    
    // 9. Restore locals and free register state
    ctx.locals = saved_locals;
    ctx.free_register = saved_free_register;
    
    Ok(())
}

/// Compile a generic for-in loop
fn compile_for_in_loop_with_parent(
    ctx: &mut CodeGenContext,
    variables: &[String],
    iterators: &[Expression],
    body: &Block,
    parent: Option<&CodeGenContext>,
) -> LuaResult<()> {
    // Generic for loops use the iterator protocol with TFORLOOP:
    // This requires a specific register layout for the loop variables:
    // R(A) = iterator function
    // R(A+1) = state 
    // R(A+2) = control variable
    // R(A+3...A+2+n) = loop variables (i, v, etc.)
    
    if iterators.len() != 1 {
        return Err(LuaError::NotImplemented(
            "Multiple iterator expressions not yet supported".to_string()
        ));
    }
    
    // 1. FIRST allocate the iterator registers
    let iter_func_reg = ctx.allocate_register()?;  // R(A)
    let state_reg = ctx.allocate_register()?;      // R(A+1)
    let control_reg = ctx.allocate_register()?;    // R(A+2)
    
    // 2. Allocate registers for each loop variable
    let mut var_regs = Vec::with_capacity(variables.len());
    for _ in 0..variables.len() {
        var_regs.push(ctx.allocate_register()?);  // R(A+3), R(A+4), etc.
    }
    
    // 3. NOW compile the iterator expression and store results in the correct registers
    match &iterators[0] {
        Expression::FunctionCall(call) => {
            // This is a call like ipairs(t) or pairs(t), so we need to call it
            // and get the three return values: iterator function, state, control variable
            
            // Use temporary registers for the call
            let temp_func_reg = ctx.allocate_register()?;
            
            // Compile the function expression
            compile_expression_to_register_with_parent(ctx, &call.function, temp_func_reg, parent)?;
            
            // Compile the arguments
            let args = match &call.args {
                CallArgs::Args(exprs) => exprs,
                _ => return Err(LuaError::NotImplemented("Special call args in iterator not supported".to_string())),
            };
            
            let mut arg_regs = Vec::with_capacity(args.len());
            for arg in args {
                let arg_reg = ctx.allocate_register()?;
                compile_expression_to_register_with_parent(ctx, arg, arg_reg, parent)?;
                arg_regs.push(arg_reg);
            }
            
            // Call the function to get the iterator triplet
            let nargs = args.len() as u32 + 1; // +1 for function itself
            let nresults = 4; // 3 return values + 1 (Lua's indexing)
            ctx.emit(Instruction::create_ABC(OpCode::Call, temp_func_reg as u32, nargs, nresults));
            
            // Move results to our loop registers
            ctx.emit(Instruction::create_ABC(OpCode::Move, iter_func_reg as u32, temp_func_reg as u32, 0));
            ctx.emit(Instruction::create_ABC(OpCode::Move, state_reg as u32, (temp_func_reg + 1) as u32, 0));
            ctx.emit(Instruction::create_ABC(OpCode::Move, control_reg as u32, (temp_func_reg + 2) as u32, 0));
            
            // Free temporary registers
            ctx.free_registers_to(temp_func_reg);
        },
        _ => {
            // If it's not a function call, compile directly to iterator register
            compile_expression_to_register_with_parent(ctx, &iterators[0], iter_func_reg, parent)?;
            
            // Initialize state and control variables to nil
            ctx.emit(Instruction::create_ABC(OpCode::LoadNil, state_reg as u32, control_reg as u32, 0));
        }
    };
    
    // Register loop variables as locals
    let saved_locals = ctx.locals.clone();
    for (i, var_name) in variables.iter().enumerate() {
        if let Some(reg) = var_regs.get(i) {
            ctx.add_local(var_name, *reg);
        }
    }
    
    // The start of the loop - this is where we'll jump back to
    let loop_start = ctx.current_pc();
    
    // TFORLOOP instruction
    // A = register of the iterator function
    // C = number of loop variables
    ctx.emit(Instruction::create_ABC(OpCode::TForLoop, iter_func_reg as u32, 0, variables.len() as u32));
    
    // If iterator returns nil (end of loop), skip the JMP at end of loop body
    // This JMP instruction will be executed only if the TFORLOOP condition fails
    let exit_jump_pc = ctx.current_pc();
    ctx.emit(Instruction::create_AsBx(OpCode::Jmp, 0, 0)); // Placeholder, will be filled in later
    
    // Compile the loop body
    compile_block_with_parent(ctx, body, parent)?;
    
    // Jump back to the start of the loop for the next iteration
    let loop_offset = -(ctx.current_pc() as i32 - loop_start as i32 + 1);
    ctx.emit(Instruction::create_AsBx(OpCode::Jmp, 0, loop_offset));
    
    // Patch the exit jump to skip over the loop
    let exit_offset = ctx.current_pc() as i32 - exit_jump_pc as i32;
    ctx.current_function.bytecode[exit_jump_pc] = 
        Instruction::create_AsBx(OpCode::Jmp, 0, exit_offset).0;
    
    // Restore locals (remove loop variables)
    ctx.locals = saved_locals;
    
    // Free all registers allocated for the loop
    ctx.free_registers_to(iter_func_reg);
    
    Ok(())
}

/// Compile a block
fn compile_block(ctx: &mut CodeGenContext, block: &Block) -> LuaResult<()> {
    compile_block_with_parent(ctx, block, None)
}

/// Compile a block with parent context
fn compile_block_with_parent(
    ctx: &mut CodeGenContext, 
    block: &Block,
    parent: Option<&CodeGenContext>
) -> LuaResult<()> {
    // TODO: Implement proper scoping
    for statement in &block.statements {
        compile_statement_with_parent(ctx, statement, parent)?;
    }
    Ok(())
}

/// Compile a function expression
fn compile_function_expression(
    ctx: &mut CodeGenContext,
    parameters: &[String],
    is_vararg: bool,
    body: &Block,
    target: u8
) -> LuaResult<()> {
    compile_function_expression_with_parent(ctx, parameters, is_vararg, body, target)
}

/// Compile a function expression with proper parent context setup
fn compile_function_expression_with_parent(
    ctx: &mut CodeGenContext,
    parameters: &[String],
    is_vararg: bool,
    body: &Block,
    target: u8
) -> LuaResult<()> {
    // Create child context for the function
    let mut child_ctx = ctx.child();
    
    // Set up function metadata
    child_ctx.current_function.num_params = parameters.len() as u8;
    child_ctx.current_function.is_vararg = is_vararg;
    
    // Register parameters as locals
    for (i, param) in parameters.iter().enumerate() {
        let reg = child_ctx.allocate_register()?;
        child_ctx.add_local(param, reg);
    }
    
    // Compile the function body with parent context
    compile_block_with_parent(&mut child_ctx, body, Some(ctx))?;
    
    // Add implicit return if not present
    if !ends_with_return(&body.statements) {
        child_ctx.emit(Instruction::create_ABC(OpCode::Return, 0, 1, 0));
    }
    
    // Add the compiled function as a prototype to the parent
    let proto_idx = ctx.current_function.prototypes.len();
    ctx.current_function.prototypes.push(child_ctx.current_function);
    
    // Merge string tables from child to parent
    for string in child_ctx.strings {
        ctx.add_string(&string);
    }
    
    // Create a constant for this function prototype
    let const_idx = ctx.add_constant(CompilationConstant::FunctionProto(proto_idx))?;
    
    // Emit CLOSURE instruction to create the function at runtime
    ctx.emit(Instruction::create_ABx(OpCode::Closure, target as u32, const_idx));
    
    // Emit instructions to set up upvalues if any
    let num_upvalues = ctx.current_function.prototypes[proto_idx].upvalues.len();
    for i in 0..num_upvalues {
        let upval = ctx.current_function.prototypes[proto_idx].upvalues[i];
        if upval.in_stack {
            // MOVE: upvalue refers to a local variable in the enclosing function
            ctx.emit(Instruction::create_ABC(OpCode::Move, 0, upval.index as u32, 0));
        } else {
            // GETUPVAL: upvalue refers to an upvalue in the enclosing function
            ctx.emit(Instruction::create_ABC(OpCode::GetUpval, 0, upval.index as u32, 0));
        }
    }
    
    Ok(())
}

/// Compile a local function definition
fn compile_local_function_definition(
    ctx: &mut CodeGenContext,
    name: &str,
    parameters: &[String],
    is_vararg: bool,
    body: &Block
) -> LuaResult<()> {
    compile_local_function_definition_with_parent(ctx, name, parameters, is_vararg, body)
}

/// Compile a local function definition with parent context
fn compile_local_function_definition_with_parent(
    ctx: &mut CodeGenContext,
    name: &str,
    parameters: &[String],
    is_vararg: bool,
    body: &Block
) -> LuaResult<()> {
    // Allocate a register for the function
    let func_reg = ctx.allocate_register()?;
    
    // Compile the function expression to that register
    compile_function_expression_with_parent(ctx, parameters, is_vararg, body, func_reg)?;
    
    // Register the function name as a local variable
    ctx.add_local(name, func_reg);
    
    Ok(())
}

/// Compile a global function definition
fn compile_function_definition(
    ctx: &mut CodeGenContext,
    name: &FunctionName,
    parameters: &[String],
    is_vararg: bool,
    body: &Block
) -> LuaResult<()> {
    compile_function_definition_with_parent(ctx, name, parameters, is_vararg, body)
}

/// Compile a global function definition with parent context
fn compile_function_definition_with_parent(
    ctx: &mut CodeGenContext,
    name: &FunctionName,
    parameters: &[String],
    is_vararg: bool,
    body: &Block
) -> LuaResult<()> {
    // First compile the function expression
    let func_reg = ctx.allocate_register()?;
    
    // If it's a method (has : syntax), add 'self' as first parameter
    let mut params = parameters.to_vec();
    if name.method.is_some() {
        params.insert(0, "self".to_string());
    }
    
    compile_function_expression_with_parent(ctx, &params, is_vararg, body, func_reg)?;
    
    // Now we need to store it in the right place
    if name.names.len() == 1 && name.method.is_none() {
        // Simple global function: function f() end
        let func_name = &name.names[0];
        let string_idx = ctx.add_string(func_name);
        let const_idx = ctx.add_constant(CompilationConstant::String(string_idx))?;
        
        ctx.emit(Instruction::create_ABx(OpCode::SetGlobal, func_reg as u32, const_idx));
    } else {
        // Table member function: function a.b.c() or a.b:method()
        
        // Get the first table - check if it's local or global
        let first_name = &name.names[0];
        let mut table_reg = ctx.allocate_register()?;
        
        if let Some(local_reg) = ctx.lookup_local(first_name) {
            // It's a local variable - use MOVE
            ctx.emit(Instruction::create_ABC(OpCode::Move, table_reg as u32, local_reg as u32, 0));
        } else {
            // It's a global variable - use GETGLOBAL
            let string_idx = ctx.add_string(first_name);
            let const_idx = ctx.add_constant(CompilationConstant::String(string_idx))?;
            ctx.emit(Instruction::create_ABx(OpCode::GetGlobal, table_reg as u32, const_idx));
        }
        
        // Navigate through the table chain (a.b.c)
        for i in 1..name.names.len() {
            let field_name = &name.names[i];
            let field_idx = ctx.add_string(field_name);
            let field_const = ctx.add_constant(CompilationConstant::String(field_idx))?;
            
            // Allocate new register for next table
            let next_reg = ctx.allocate_register()?;
            
            // Get the next table: next_reg = table_reg[field]
            ctx.emit(Instruction::create_ABC(
                OpCode::GetTable,
                next_reg as u32,
                table_reg as u32,
                Instruction::encode_constant(field_const)
            ));
            
            // Update table_reg to the new register
            table_reg = next_reg;
        }
        
        // Now set the function in the final table
        // The field name is either the method name (if method syntax) or would have been handled in the loop
        if let Some(method) = &name.method {
            let field_idx = ctx.add_string(method);
            let field_const = ctx.add_constant(CompilationConstant::String(field_idx))?;
            
            // Set table[field] = function
            ctx.emit(Instruction::create_ABC(
                OpCode::SetTable,
                table_reg as u32,
                Instruction::encode_constant(field_const),
                func_reg as u32
            ));
        }
        
        ctx.free_registers_to(table_reg);
    }
    
    // Free function register
    ctx.free_registers_to(func_reg);
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_instruction_encoding() {
        // Test ABC format
        let inst = Instruction::create_ABC(OpCode::Move, 1, 2, 0);
        assert_eq!(inst.get_opcode(), OpCode::Move);
        assert_eq!(inst.get_a(), 1);
        assert_eq!(inst.get_b(), 2);
        assert_eq!(inst.get_c(), 0);
        
        // Test ABx format
        let inst = Instruction::create_ABx(OpCode::LoadK, 5, 100);
        assert_eq!(inst.get_opcode(), OpCode::LoadK);
        assert_eq!(inst.get_a(), 5);
        assert_eq!(inst.get_bx(), 100);
        
        // Test AsBx format
        let inst = Instruction::create_AsBx(OpCode::Jmp, 0, -10);
        assert_eq!(inst.get_opcode(), OpCode::Jmp);
        assert_eq!(inst.get_a(), 0);
        assert_eq!(inst.get_sbx(), -10);
    }
    
    #[test]
    fn test_constant_encoding() {
        let const_idx = Instruction::encode_constant(42);
        assert!(const_idx & Instruction::BITRK != 0);
        assert_eq!(const_idx & !Instruction::BITRK, 42);
    }
    
    #[test]
    fn test_simple_codegen() {
        let chunk = Chunk::new();
        let output = generate_bytecode(&chunk).unwrap();
        
        // Empty chunk should have return instruction
        assert_eq!(output.main.bytecode.len(), 1);
        let inst = Instruction(output.main.bytecode[0]);
        assert_eq!(inst.get_opcode(), OpCode::Return);
    }
    
    #[test]
    fn test_function_compilation() {
        use super::ast::*;
        
        // Test local function definition
        let mut chunk = Chunk::new();
        chunk.statements.push(Statement::LocalFunctionDefinition {
            name: "add".to_string(),
            parameters: vec!["a".to_string(), "b".to_string()],
            is_vararg: false,
            body: Block {
                statements: vec![Statement::Return {
                    expressions: vec![Expression::BinaryOp {
                        left: Box::new(Expression::Variable(Variable::Name("a".to_string()))),
                        operator: BinaryOperator::Add,
                        right: Box::new(Expression::Variable(Variable::Name("b".to_string()))),
                    }],
                }],
            },
        });
        
        let output = generate_bytecode(&chunk).unwrap();
        
        // Should have the main function with one prototype
        assert_eq!(output.main.prototypes.len(), 1);
        
        // The nested function should have 2 parameters
        assert_eq!(output.main.prototypes[0].num_params, 2);
        
        // Main function should have CLOSURE instruction
        let closure_found = output.main.bytecode.iter().any(|&instr| {
            Instruction(instr).get_opcode() == OpCode::Closure
        });
        assert!(closure_found);
    }
    
    #[test]
    fn test_global_function_definition() {
        use super::ast::*;
        
        // Test simple global function: function f() return 1 end
        let mut chunk = Chunk::new();
        chunk.statements.push(Statement::FunctionDefinition {
            name: FunctionName {
                names: vec!["f".to_string()],
                method: None,
            },
            parameters: vec![],
            is_vararg: false,
            body: Block {
                statements: vec![Statement::Return {
                    expressions: vec![Expression::Number(1.0)],
                }],
            },
        });
        
        let output = generate_bytecode(&chunk).unwrap();
        
        // Should have a CLOSURE instruction followed by SETGLOBAL
        let mut found_closure = false;
        let mut found_setglobal = false;
        
        for &instr in &output.main.bytecode {
            let inst = Instruction(instr);
            if inst.get_opcode() == OpCode::Closure {
                found_closure = true;
            } else if inst.get_opcode() == OpCode::SetGlobal {
                found_setglobal = true;
            }
        }
        
        assert!(found_closure, "Should have CLOSURE instruction");
        assert!(found_setglobal, "Should have SETGLOBAL instruction");
    }
    
    #[test]
    fn test_method_function_definition() {
        use super::ast::*;
        
        // Test method definition: function obj:method(x) return x end
        let mut chunk = Chunk::new();
        chunk.statements.push(Statement::FunctionDefinition {
            name: FunctionName {
                names: vec!["obj".to_string()],
                method: Some("method".to_string()),
            },
            parameters: vec!["x".to_string()],
            is_vararg: false,
            body: Block {
                statements: vec![Statement::Return {
                    expressions: vec![Expression::Variable(Variable::Name("x".to_string()))],
                }],
            },
        });
        
        let output = generate_bytecode(&chunk).unwrap();
        
        // The compiled function should have 2 parameters (self + x)
        assert_eq!(output.main.prototypes.len(), 1);
        assert_eq!(output.main.prototypes[0].num_params, 2);
        
        // Should have instructions to get obj, then set method on it
        let mut found_getglobal = false;
        let mut found_settable = false;
        
        for &instr in &output.main.bytecode {
            let inst = Instruction(instr);
            if inst.get_opcode() == OpCode::GetGlobal {
                found_getglobal = true;
            } else if inst.get_opcode() == OpCode::SetTable {
                found_settable = true;
            }
        }
        
        assert!(found_getglobal, "Should have GETGLOBAL for obj");
        assert!(found_settable, "Should have SETTABLE for method");
    }
    
    #[test]
    fn test_method_call_compilation() {
        use super::ast::*;
        
        // Test method call: obj:method(x, y)
        let mut chunk = Chunk::new();
        chunk.statements.push(Statement::FunctionCall(FunctionCall {
            function: Expression::Variable(Variable::Name("obj".to_string())),
            method: Some("method".to_string()),
            args: CallArgs::Args(vec![
                Expression::Variable(Variable::Name("x".to_string())),
                Expression::Variable(Variable::Name("y".to_string())),
            ]),
        }));
        
        let output = generate_bytecode(&chunk).unwrap();
        
        // Should have GETGLOBAL for obj, GETGLOBAL for x, GETGLOBAL for y,
        // SELF for method setup, and CALL
        let mut found_getglobal = 0;
        let mut found_self = false;
        let mut found_call = false;
        
        for &instr in &output.main.bytecode {
            let inst = Instruction(instr);
            match inst.get_opcode() {
                OpCode::GetGlobal => found_getglobal += 1,
                OpCode::SelfOp => found_self = true,
                OpCode::Call => {
                    found_call = true;
                    // Check that B (nargs) is 4 (function + self + 2 args)
                    assert_eq!(inst.get_b(), 4, "CALL should have 4 arguments (func, self, x, y)");
                }
                _ => {}
            }
        }
        
        assert_eq!(found_getglobal, 3, "Should have GETGLOBAL for obj, x, and y");
        assert!(found_self, "Should have SELF instruction for method call");
        assert!(found_call, "Should have CALL instruction");
        
        // Verify the string "method" is in the string table
        assert!(output.strings.iter().any(|s| s == "method"), 
                "Method name should be in string table");
    }
    
    #[test]
    fn test_regular_vs_method_call() {
        use super::ast::*;
        
        // Test 1: Regular function call obj.method(x)
        let mut chunk1 = Chunk::new();
        chunk1.statements.push(Statement::FunctionCall(FunctionCall {
            function: Expression::Variable(Variable::Member {
                table: Box::new(Expression::Variable(Variable::Name("obj".to_string()))),
                field: "method".to_string(),
            }),
            method: None,
            args: CallArgs::Args(vec![
                Expression::Variable(Variable::Name("x".to_string())),
            ]),
        }));
        
        let output1 = generate_bytecode(&chunk1).unwrap();
        
        // Test 2: Method call obj:method(x)
        let mut chunk2 = Chunk::new();
        chunk2.statements.push(Statement::FunctionCall(FunctionCall {
            function: Expression::Variable(Variable::Name("obj".to_string())),
            method: Some("method".to_string()),
            args: CallArgs::Args(vec![
                Expression::Variable(Variable::Name("x".to_string())),
            ]),
        }));
        
        let output2 = generate_bytecode(&chunk2).unwrap();
        
        // Regular call should have GETTABLE, method call should have SELF
        let has_gettable = output1.main.bytecode.iter()
            .any(|&instr| Instruction(instr).get_opcode() == OpCode::GetTable);
        let has_self = output2.main.bytecode.iter()
            .any(|&instr| Instruction(instr).get_opcode() == OpCode::SelfOp);
        
        assert!(has_gettable, "Regular call should use GETTABLE");
        assert!(has_self, "Method call should use SELF");
        assert!(!output1.main.bytecode.iter()
            .any(|&instr| Instruction(instr).get_opcode() == OpCode::SelfOp),
            "Regular call should not use SELF");
        
        // Check CALL instruction argument counts
        for &instr in &output1.main.bytecode {
            let inst = Instruction(instr);
            if inst.get_opcode() == OpCode::Call {
                assert_eq!(inst.get_b(), 2, "Regular call should have 2 args (func + x)");
            }
        }
        
        for &instr in &output2.main.bytecode {
            let inst = Instruction(instr);
            if inst.get_opcode() == OpCode::Call {
                assert_eq!(inst.get_b(), 3, "Method call should have 3 args (func + self + x)");
            }
        }
    }
    
    #[test]
    fn test_nested_table_function_definition() {
        use super::ast::*;
        
        // Test nested table function: function a.b.c.d() return 1 end
        let mut chunk = Chunk::new();
        chunk.statements.push(Statement::FunctionDefinition {
            name: FunctionName {
                names: vec!["a".to_string(), "b".to_string(), "c".to_string()],
                method: Some("d".to_string()),
            },
            parameters: vec![],
            is_vararg: false,
            body: Block {
                statements: vec![Statement::Return {
                    expressions: vec![Expression::Number(1.0)],
                }],
            },
        });
        
        let output = generate_bytecode(&chunk).unwrap();
        
        // Should have instructions to navigate the table chain
        let mut found_getglobal = false;
        let mut gettable_count = 0;
        let mut found_settable = false;
        let mut found_closure = false;
        
        for &instr in &output.main.bytecode {
            let inst = Instruction(instr);
            match inst.get_opcode() {
                OpCode::GetGlobal => found_getglobal = true,
                OpCode::GetTable => gettable_count += 1,
                OpCode::SetTable => found_settable = true,
                OpCode::Closure => found_closure = true,
                _ => {}
            }
        }
        
        assert!(found_getglobal, "Should have GETGLOBAL for 'a'");
        assert_eq!(gettable_count, 2, "Should have 2 GETTABLE instructions for b and c");
        assert!(found_closure, "Should have CLOSURE instruction");
        assert!(found_settable, "Should have SETTABLE for method 'd'");
    }
    
    #[test]
    fn test_method_call_register_layout() {
        use super::ast::*;
        
        // Test that method calls set up registers correctly: obj:method(x)
        let mut chunk = Chunk::new();
        chunk.statements.push(Statement::FunctionCall(FunctionCall {
            function: Expression::Variable(Variable::Name("obj".to_string())),
            method: Some("method".to_string()),
            args: CallArgs::Args(vec![
                Expression::Variable(Variable::Name("x".to_string())),
            ]),
        }));
        
        let output = generate_bytecode(&chunk).unwrap();
        
        // Find SELF and CALL instructions
        let mut self_instr = None;
        let mut call_instr = None;
        
        for (i, &instr) in output.main.bytecode.iter().enumerate() {
            let inst = Instruction(instr);
            match inst.get_opcode() {
                OpCode::SelfOp => self_instr = Some((i, inst)),
                OpCode::Call => call_instr = Some((i, inst)),
                _ => {}
            }
        }
        
        assert!(self_instr.is_some(), "Should have SELF instruction");
        assert!(call_instr.is_some(), "Should have CALL instruction");
        
        // Verify SELF instruction setup
        let (_, self_inst) = self_instr.unwrap();
        let self_a = self_inst.get_a();
        let self_b = self_inst.get_b();
        
        // SELF should use same register for A and B (the table)
        assert_eq!(self_a, self_b, "SELF should use same register for table");
        
        // Verify CALL instruction has correct argument count
        let (_, call_inst) = call_instr.unwrap();
        let call_a = call_inst.get_a();
        let call_b = call_inst.get_b();
        
        // CALL A should match SELF A (function register)
        assert_eq!(call_a, self_a, "CALL should use same base register as SELF");
        
        // CALL B should be 3 (function + self + 1 arg)
        assert_eq!(call_b, 3, "CALL should have 3 arguments total");
    }
    
    #[test]
    fn test_simple_global_function_registers() {
        use super::ast::*;
        
        // Test that simple global functions use minimal registers
        let mut chunk = Chunk::new();
        chunk.statements.push(Statement::FunctionDefinition {
            name: FunctionName {
                names: vec!["add".to_string()],
                method: None,
            },
            parameters: vec!["a".to_string(), "b".to_string()],
            is_vararg: false,
            body: Block {
                statements: vec![Statement::Return {
                    expressions: vec![Expression::BinaryOp {
                        left: Box::new(Expression::Variable(Variable::Name("a".to_string()))),
                        operator: BinaryOperator::Add,
                        right: Box::new(Expression::Variable(Variable::Name("b".to_string()))),
                    }],
                }],
            },
        });
        
        let output = generate_bytecode(&chunk).unwrap();
        
        // Should have CLOSURE followed by SETGLOBAL
        let mut found_sequence = false;
        for i in 0..output.main.bytecode.len()-1 {
            let inst1 = Instruction(output.main.bytecode[i]);
            let inst2 = Instruction(output.main.bytecode[i+1]);
            
            if inst1.get_opcode() == OpCode::Closure && inst2.get_opcode() == OpCode::SetGlobal {
                // Both should use the same register
                assert_eq!(inst1.get_a(), inst2.get_a(), "CLOSURE and SETGLOBAL should use same register");
                found_sequence = true;
                break;
            }
        }
        
        assert!(found_sequence, "Should have CLOSURE immediately followed by SETGLOBAL");
    }
}