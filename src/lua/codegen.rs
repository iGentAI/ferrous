//! Lua Bytecode Generation Module
//!
//! This module implements bytecode generation for Lua 5.1, converting AST
//! into executable bytecode following the Lua 5.1 virtual machine specification.

use super::ast::*;
use super::error::{LuaError, LuaResult};
use std::collections::HashMap;

/// Constant value during compilation
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

/// Code generation context
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
    
    /// Free registers down to a certain level
    fn free_registers_to(&mut self, level: u8) {
        self.free_register = level;
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
    match statement {
        Statement::LocalDeclaration(decl) => compile_local_declaration(ctx, decl),
        Statement::Assignment(assign) => compile_assignment(ctx, assign),
        Statement::FunctionCall(call) => {
            compile_function_call(ctx, call)?;
            Ok(())
        }
        Statement::Return { expressions } => compile_return_statement(ctx, expressions),
        Statement::If { condition, body, else_ifs, else_block } => {
            compile_if_statement(ctx, condition, body, else_ifs, else_block)
        }
        Statement::While { condition, body } => compile_while_loop(ctx, condition, body),
        Statement::Do(block) => compile_block(ctx, block),
        Statement::ForLoop { variable, initial, limit, step, body } => {
            compile_for_loop(ctx, variable, initial, limit, step.as_ref(), body)
        }
        Statement::LocalFunctionDefinition { name, parameters, is_vararg, body } => {
            compile_local_function_definition(ctx, name, parameters, *is_vararg, body)
        }
        _ => Err(LuaError::NotImplemented(format!("Statement type: {:?}", statement))),
    }
}

/// Compile a local variable declaration
fn compile_local_declaration(ctx: &mut CodeGenContext, decl: &LocalDeclaration) -> LuaResult<()> {
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
                compile_expression_to_register(ctx, expr, base_reg + i as u8)?;
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
    // For now, handle simple local variable assignments
    if assign.variables.len() == 1 && assign.expressions.len() == 1 {
        match &assign.variables[0] {
            Variable::Name(name) => {
                if let Some(reg) = ctx.lookup_local(name) {
                    // Assign to existing local
                    compile_expression_to_register(ctx, &assign.expressions[0], reg)?;
                    return Ok(());
                } else {
                    // Global assignment
                    let value_reg = ctx.allocate_register()?;
                    compile_expression_to_register(ctx, &assign.expressions[0], value_reg)?;
                    
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
                compile_expression_to_register(ctx, table, table_reg)?;
                
                let value_reg = ctx.allocate_register()?;
                compile_expression_to_register(ctx, &assign.expressions[0], value_reg)?;
                
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
                compile_expression_to_register(ctx, table, table_reg)?;
                
                let key_reg = ctx.allocate_register()?;
                compile_expression_to_register(ctx, key, key_reg)?;
                
                let value_reg = ctx.allocate_register()?;
                compile_expression_to_register(ctx, &assign.expressions[0], value_reg)?;
                
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
            compile_variable_to_register(ctx, var, target)?;
        }
        Expression::BinaryOp { left, operator, right } => {
            compile_binary_op(ctx, left, operator, right, target)?;
        }
        Expression::UnaryOp { operator, operand } => {
            compile_unary_op(ctx, operator, operand, target)?;
        }
        Expression::FunctionCall(call) => {
            let base = compile_function_call(ctx, call)?;
            if base != target {
                ctx.emit(Instruction::create_ABC(OpCode::Move, target as u32, base as u32, 0));
            }
        }
        Expression::TableConstructor(tc) => {
            compile_table_constructor(ctx, tc, target)?;
        }
        Expression::FunctionDef { parameters, is_vararg, body } => {
            compile_function_expression(ctx, parameters, *is_vararg, body, target)?;
        }
        _ => return Err(LuaError::NotImplemented(format!("Expression type: {:?}", expr))),
    }
    
    Ok(())
}

/// Compile a table constructor
fn compile_table_constructor(ctx: &mut CodeGenContext, tc: &TableConstructor, target: u8) -> LuaResult<()> {
    // Create the table with size hints (B = array size, C = hash size)
    // For now, we'll use 0, 0 and let the VM resize as needed
    ctx.emit(Instruction::create_ABC(OpCode::NewTable, target as u32, 0, 0));
    
    // Track array index for list-style entries
    let mut array_index = 1u32;
    
    // Compile each field
    for field in &tc.fields {
        match field {
            TableField::List(expr) => {
                // Array-style entry: t[array_index] = expr
                // Allocate register for the value
                let value_reg = ctx.allocate_register()?;
                compile_expression_to_register(ctx, expr, value_reg)?;
                
                // Create constant for the array index
                let index_const = ctx.add_constant(CompilationConstant::Number(array_index as f64))?;
                
                // SETTABLE: table[index] = value
                ctx.emit(Instruction::create_ABC(
                    OpCode::SetTable, 
                    target as u32,
                    Instruction::encode_constant(index_const),  // RK(B) = index as constant
                    value_reg as u32  // RK(C) = value register
                ));
                
                ctx.free_registers_to(value_reg);
                array_index += 1;
            }
            
            TableField::Record { key, value } => {
                // Record-style entry: t.key = value
                // Allocate register for the value
                let value_reg = ctx.allocate_register()?;
                compile_expression_to_register(ctx, value, value_reg)?;
                
                // Create string constant for the key
                let key_string_idx = ctx.add_string(key);
                let key_const = ctx.add_constant(CompilationConstant::String(key_string_idx))?;
                
                // SETTABLE: table[key] = value
                ctx.emit(Instruction::create_ABC(
                    OpCode::SetTable,
                    target as u32,
                    Instruction::encode_constant(key_const),  // RK(B) = key as constant
                    value_reg as u32  // RK(C) = value register
                ));
                
                ctx.free_registers_to(value_reg);
            }
            
            TableField::Index { key, value } => {
                // Computed index entry: t[key_expr] = value
                // Allocate registers for key and value
                let key_reg = ctx.allocate_register()?;
                compile_expression_to_register(ctx, key, key_reg)?;
                
                let value_reg = ctx.allocate_register()?;
                compile_expression_to_register(ctx, value, value_reg)?;
                
                // SETTABLE: table[key] = value
                ctx.emit(Instruction::create_ABC(
                    OpCode::SetTable,
                    target as u32,
                    key_reg as u32,    // RK(B) = key register
                    value_reg as u32   // RK(C) = value register
                ));
                
                ctx.free_registers_to(key_reg);
            }
        }
    }
    
    Ok(())
}

/// Compile a variable access to a register
fn compile_variable_to_register(ctx: &mut CodeGenContext, var: &Variable, target: u8) -> LuaResult<()> {
    match var {
        Variable::Name(name) => {
            if let Some(reg) = ctx.lookup_local(name) {
                // Local variable
                if reg != target {
                    ctx.emit(Instruction::create_ABC(OpCode::Move, target as u32, reg as u32, 0));
                }
            } else {
                // Global variable
                let string_idx = ctx.add_string(name);
                let const_idx = ctx.add_constant(CompilationConstant::String(string_idx))?;
                ctx.emit(Instruction::create_ABx(OpCode::GetGlobal, target as u32, const_idx));
            }
        }
        Variable::Member { table, field } => {
            // Table member access: table.field
            let table_reg = ctx.allocate_register()?;
            compile_expression_to_register(ctx, table, table_reg)?;
            
            // Create string constant for the field name
            let field_idx = ctx.add_string(field);
            let field_const = ctx.add_constant(CompilationConstant::String(field_idx))?;
            
            // GETTABLE: R(target) = table[field]
            ctx.emit(Instruction::create_ABC(
                OpCode::GetTable,
                target as u32,
                table_reg as u32,
                Instruction::encode_constant(field_const)
            ));
            
            ctx.free_registers_to(target + 1);
        }
        Variable::Index { table, key } => {
            // Table index access: table[key]
            let table_reg = ctx.allocate_register()?;
            compile_expression_to_register(ctx, table, table_reg)?;
            
            let key_reg = ctx.allocate_register()?;
            compile_expression_to_register(ctx, key, key_reg)?;
            
            // GETTABLE: R(target) = table[key]
            ctx.emit(Instruction::create_ABC(
                OpCode::GetTable,
                target as u32,
                table_reg as u32,
                key_reg as u32
            ));
            
            ctx.free_registers_to(target + 1);
        }
    }
    
    Ok(())
}

/// Compile a binary operation
fn compile_binary_op(ctx: &mut CodeGenContext, left: &Expression, op: &BinaryOperator, right: &Expression, target: u8) -> LuaResult<()> {
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
            
            // Compile operands
            let left_reg = ctx.allocate_register()?;
            compile_expression_to_register(ctx, left, left_reg)?;
            
            let right_reg = ctx.allocate_register()?;
            compile_expression_to_register(ctx, right, right_reg)?;
            
            // Emit operation
            ctx.emit(Instruction::create_ABC(opcode, target as u32, left_reg as u32, right_reg as u32));
            
            // Free temporary registers
            ctx.free_registers_to(target + 1);
        }
        
        BinaryOperator::Concat => {
            // Compile operands
            let left_reg = ctx.allocate_register()?;
            compile_expression_to_register(ctx, left, left_reg)?;
            
            let right_reg = ctx.allocate_register()?;
            compile_expression_to_register(ctx, right, right_reg)?;
            
            // Emit operation
            ctx.emit(Instruction::create_ABC(OpCode::Concat, target as u32, left_reg as u32, right_reg as u32));
            
            // Free temporary registers
            ctx.free_registers_to(target + 1);
        }
        
        BinaryOperator::Or => {
            // Short-circuit OR: if left is true, use left, else use right
            // First compile left operand to target
            compile_expression_to_register(ctx, left, target)?;
            
            // TESTSET: if R(target) is truthy, skip next instruction, else R(target) := R(right_reg)
            // We need to compile right first to a temp register
            let skip_pc = ctx.current_pc();
            // Placeholder TESTSET - will patch after compiling right
            ctx.emit(Instruction::create_ABC(OpCode::TestSet, target as u32, target as u32, 1)); // C=1 means test for truthy
            
            // Compile right operand to target (only executed if left was falsey)
            compile_expression_to_register(ctx, right, target)?;
            
            // Patch the TESTSET instruction
            ctx.current_function.bytecode[skip_pc] = Instruction::create_ABC(OpCode::TestSet, target as u32, target as u32, 1).0;
            
            ctx.free_registers_to(target + 1);
        }
        
        BinaryOperator::And => {
            // Short-circuit AND: if left is false, use left, else use right
            // First compile left operand to target
            compile_expression_to_register(ctx, left, target)?;
            
            // TESTSET: if R(target) is falsey, skip next instruction, else R(target) := R(right_reg)
            ctx.emit(Instruction::create_ABC(OpCode::TestSet, target as u32, target as u32, 0)); // C=0 means test for falsey
            
            // Compile right operand to target (only executed if left was truthy)
            compile_expression_to_register(ctx, right, target)?;
            
            ctx.free_registers_to(target + 1);
        }
        
        BinaryOperator::Eq | BinaryOperator::Ne | BinaryOperator::Lt | 
        BinaryOperator::Le | BinaryOperator::Gt | BinaryOperator::Ge => {
            // Comparisons are more complex - they use conditional jumps
            // For now, compile to a boolean result
            
            // Compile operands
            let left_reg = ctx.allocate_register()?;
            compile_expression_to_register(ctx, left, left_reg)?;
            
            let right_reg = ctx.allocate_register()?;
            compile_expression_to_register(ctx, right, right_reg)?;
            
            // The comparison instructions work by skipping the next instruction if the test fails
            // So we need to use LOADBOOL with the skip flag
            
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
            
            ctx.free_registers_to(target + 1);
        }
        
        _ => return Err(LuaError::NotImplemented(format!("Binary operator: {:?}", op))),
    }
    
    Ok(())
}

/// Compile a unary operation
fn compile_unary_op(ctx: &mut CodeGenContext, op: &UnaryOperator, operand: &Expression, target: u8) -> LuaResult<()> {
    let opcode = match op {
        UnaryOperator::Not => OpCode::Not,
        UnaryOperator::Minus => OpCode::Unm,
        UnaryOperator::Length => OpCode::Len,
    };
    
    let operand_reg = ctx.allocate_register()?;
    compile_expression_to_register(ctx, operand, operand_reg)?;
    
    ctx.emit(Instruction::create_ABC(opcode, target as u32, operand_reg as u32, 0));
    
    ctx.free_registers_to(target + 1);
    
    Ok(())
}

/// Compile a function call
fn compile_function_call(ctx: &mut CodeGenContext, call: &FunctionCall) -> LuaResult<u8> {
    // Allocate register for function
    let func_reg = ctx.allocate_register()?;
    
    // Compile function expression
    compile_expression_to_register(ctx, &call.function, func_reg)?;
    
    // Compile arguments
    let args = match &call.args {
        CallArgs::Args(exprs) => exprs,
        _ => return Err(LuaError::NotImplemented("Special call syntax".to_string())),
    };
    
    for arg in args {
        let arg_reg = ctx.allocate_register()?;
        compile_expression_to_register(ctx, arg, arg_reg)?;
    }
    
    // Emit CALL instruction
    let nargs = args.len() as u32 + 1; // +1 because B includes the function
    let nresults = 2; // 1 result + 1
    ctx.emit(Instruction::create_ABC(OpCode::Call, func_reg as u32, nargs, nresults));
    
    // Free registers except result
    ctx.free_registers_to(func_reg + 1);
    
    Ok(func_reg)
}

/// Compile return statement
fn compile_return_statement(ctx: &mut CodeGenContext, expressions: &[Expression]) -> LuaResult<()> {
    let base_reg = ctx.free_register;
    
    // Compile return values
    for expr in expressions {
        let reg = ctx.allocate_register()?;
        compile_expression_to_register(ctx, expr, reg)?;
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
    let mut jump_to_end = Vec::new();
    
    // Compile main if condition
    let cond_reg = ctx.allocate_register()?;
    compile_expression_to_register(ctx, condition, cond_reg)?;
    
    // TEST instruction - skip next instruction if false
    ctx.emit(Instruction::create_ABC(OpCode::Test, cond_reg as u32, 0, 1)); // C=1 means skip if false
    let jump_false = ctx.current_pc();
    ctx.emit(Instruction::create_AsBx(OpCode::Jmp, 0, 0)); // Placeholder
    
    ctx.free_registers_to(cond_reg);
    
    // Compile then body
    compile_block(ctx, body)?;
    
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
    let loop_start = ctx.current_pc();
    
    // Compile condition
    let cond_reg = ctx.allocate_register()?;
    compile_expression_to_register(ctx, condition, cond_reg)?;
    
    // TEST instruction - skip next instruction if false
    ctx.emit(Instruction::create_ABC(OpCode::Test, cond_reg as u32, 0, 1));
    let jump_false = ctx.current_pc();
    ctx.emit(Instruction::create_AsBx(OpCode::Jmp, 0, 0)); // Placeholder
    
    ctx.free_registers_to(cond_reg);
    
    // Compile body
    compile_block(ctx, body)?;
    
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
    // For loops use 4 consecutive registers:
    // R(A): internal loop index
    // R(A+1): limit value
    // R(A+2): step value
    // R(A+3): external loop index (user variable)
    
    // Allocate 4 consecutive registers
    let loop_base = ctx.allocate_register()?;
    ctx.allocate_register()?; // limit
    ctx.allocate_register()?; // step
    let user_var = ctx.allocate_register()?;
    
    // Compile initial value to R(A)
    compile_expression_to_register(ctx, initial, loop_base)?;
    
    // Compile limit to R(A+1)
    compile_expression_to_register(ctx, limit, loop_base + 1)?;
    
    // Compile step to R(A+2) (default to 1 if not specified)
    if let Some(step_expr) = step {
        compile_expression_to_register(ctx, step_expr, loop_base + 2)?;
    } else {
        // Load constant 1 as default step
        let const_idx = ctx.add_constant(CompilationConstant::Number(1.0))?;
        ctx.emit(Instruction::create_ABx(OpCode::LoadK, (loop_base + 2) as u32, const_idx));
    }
    
    // FORPREP: prepare the loop
    let forprep_pc = ctx.current_pc();
    ctx.emit(Instruction::create_AsBx(OpCode::ForPrep, loop_base as u32, 0)); // Placeholder jump
    
    // Register the loop variable as a local
    let saved_locals = ctx.locals.clone();
    ctx.add_local(variable, user_var);
    
    // Compile loop body
    let loop_start = ctx.current_pc();
    compile_block(ctx, body)?;
    
    // FORLOOP: increment and test
    // Convert to signed i32 first, then apply negation
    let current_pc = ctx.current_pc() as i32;
    let loop_start_i32 = loop_start as i32;
    let loop_offset = -(current_pc - loop_start_i32 + 1);
    ctx.emit(Instruction::create_AsBx(OpCode::ForLoop, loop_base as u32, loop_offset));
    
    // Patch FORPREP jump to skip loop if initial condition fails
    let prep_offset = (ctx.current_pc() - forprep_pc - 1) as i32;
    ctx.current_function.bytecode[forprep_pc] = 
        Instruction::create_AsBx(OpCode::ForPrep, loop_base as u32, prep_offset).0;
    
    // Restore locals (remove loop variable)
    ctx.locals = saved_locals;
    
    // Free loop registers
    ctx.free_registers_to(loop_base);
    
    Ok(())
}

/// Compile a block
fn compile_block(ctx: &mut CodeGenContext, block: &Block) -> LuaResult<()> {
    // TODO: Implement proper scoping
    for statement in &block.statements {
        compile_statement(ctx, statement)?;
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
    
    // Compile the function body
    compile_block(&mut child_ctx, body)?;
    
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
    
    // TODO: Handle upvalues here when upvalue support is implemented
    // For now, functions without upvalues work fine
    
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
    // Allocate a register for the function
    let func_reg = ctx.allocate_register()?;
    
    // Compile the function expression to that register
    compile_function_expression(ctx, parameters, is_vararg, body, func_reg)?;
    
    // Register the function name as a local variable
    ctx.add_local(name, func_reg);
    
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
}