//! Lua Compiler Implementation
//!
//! This module implements a Lua 5.1 compiler that converts the AST
//! from the parser into bytecode that can be executed by our VM.
//! The compiler follows our handle-based architecture with no raw pointers.

use std::collections::HashMap;
use std::marker::PhantomData;

use super::error::{LuaError, Result, syntax_error};
use super::value::{StringHandle, FunctionProto, UpvalueDesc, LocalVarInfo};
use super::parser::{Node, Expression, Statement, BinaryOp, UnaryOp, TableField, FunctionName, LValue};
use super::heap::LuaHeap;

/// Lua opcode
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum OpCode {
    Move = 0,       // A B     R(A) := R(B)
    LoadK = 1,      // A Bx    R(A) := Kst(Bx)
    LoadBool = 2,   // A B C   R(A) := (Bool)B; if (C) PC++
    LoadNil = 3,    // A B     R(A) ... R(B) := nil
    GetUpVal = 4,   // A B     R(A) := UpValue[B]
    GetGlobal = 5,  // A Bx    R(A) := Gbl[Kst(Bx)]
    GetTable = 6,   // A B C   R(A) := R(B)[RK(C)]
    SetGlobal = 7,  // A Bx    Gbl[Kst(Bx)] := R(A)
    SetUpVal = 8,   // A B     UpValue[B] := R(A)
    SetTable = 9,   // A B C   R(A)[RK(B)] := RK(C)
    NewTable = 10,  // A B C   R(A) := {} (size = B,C)
    Self_ = 11,     // A B C   R(A+1) := R(B); R(A) := R(B)[RK(C)]
    Add = 12,       // A B C   R(A) := RK(B) + RK(C)
    Sub = 13,       // A B C   R(A) := RK(B) - RK(C)
    Mul = 14,       // A B C   R(A) := RK(B) * RK(C)
    Div = 15,       // A B C   R(A) := RK(B) / RK(C)
    Mod = 16,       // A B C   R(A) := RK(B) % RK(C)
    Pow = 17,       // A B C   R(A) := RK(B) ^ RK(C)
    Unm = 18,       // A B     R(A) := -R(B)
    Not = 19,       // A B     R(A) := not R(B)
    Len = 20,       // A B     R(A) := length of R(B)
    Concat = 21,    // A B C   R(A) := R(B).. ... ..R(C)
    Jmp = 22,       // sBx     PC += sBx
    Eq = 23,        // A B C   if ((RK(B) == RK(C)) ~= A) then PC++
    Lt = 24,        // A B C   if ((RK(B) <  RK(C)) ~= A) then PC++
    Le = 25,        // A B C   if ((RK(B) <= RK(C)) ~= A) then PC++
    Test = 26,      // A C     if not (R(A) <=> C) then PC++
    TestSet = 27,   // A B C   if (R(B) <=> C) then R(A) := R(B) else PC++
    Call = 28,      // A B C   R(A), ... ,R(A+C-2) := R(A)(R(A+1), ... ,R(A+B-1))
    TailCall = 29,  // A B C   return R(A)(R(A+1), ... ,R(A+B-1))
    Return = 30,    // A B     return R(A), ... ,R(A+B-2)
    ForLoop = 31,   // A sBx   R(A)+=R(A+2); if R(A) <?= R(A+1) then { PC+=sBx; R(A+3)=R(A) }
    ForPrep = 32,   // A sBx   R(A)-=R(A+2); PC+=sBx
    TForLoop = 33,  // A C     R(A+3), ... ,R(A+2+C) := R(A)(R(A+1), R(A+2)); if R(A+3) ~= nil then R(A+2)=R(A+3) else PC++
    SetList = 34,   // A B C   R(A)[(C-1)*FPF+i] := R(A+i), 1 <= i <= B
    Close = 35,     // A       close all variables in the stack up to (>=) R(A)
    Closure = 36,   // A Bx    R(A) := closure(KPROTO[Bx], R(A), ... ,R(A+n))
    VarArg = 37,    // A B     R(A), R(A+1), ..., R(A+B-1) = vararg
}

/// Lua instruction format
#[derive(Debug, Clone, Copy)]
pub struct Instruction(pub u32);

impl Instruction {
    /// Create a new instruction
    pub fn new(opcode: OpCode, a: u8, b: u16, c: u16) -> Self {
        let op = opcode as u32 & 0x3F;
        let a = (a as u32 & 0xFF) << 6;
        let b = (b as u32 & 0x1FF) << 14;
        let c = (c as u32 & 0x1FF) << 23;
        Instruction(op | a | b | c)
    }
    
    /// Create a new instruction with A, Bx format
    pub fn new_abx(opcode: OpCode, a: u8, bx: u32) -> Self {
        let op = opcode as u32 & 0x3F;
        let a = (a as u32 & 0xFF) << 6;
        let bx = (bx & 0x3FFFF) << 14;
        Instruction(op | a | bx)
    }
    
    /// Create a new instruction with A, sBx format
    pub fn new_asbx(opcode: OpCode, a: u8, sbx: i32) -> Self {
        let op = opcode as u32 & 0x3F;
        let a = (a as u32 & 0xFF) << 6;
        let bx = ((sbx + 131071) as u32 & 0x3FFFF) << 14;
        Instruction(op | a | bx)
    }
}

/// A register allocator for the Lua compiler
pub struct RegisterAllocator {
    /// Current register index
    next_register: u8,
    
    /// Maximum register used
    max_used: u8,
    
    /// Free registers (temporary)
    free: Vec<u8>,
    
    /// Registers in use
    in_use: Vec<bool>,
}

impl RegisterAllocator {
    /// Create a new register allocator
    pub fn new() -> Self {
        RegisterAllocator {
            next_register: 0,
            max_used: 0,
            free: Vec::new(),
            in_use: vec![false; 256],
        }
    }
    
    /// Allocate a new register
    pub fn allocate(&mut self) -> u8 {
        if let Some(reg) = self.free.pop() {
            self.in_use[reg as usize] = true;
            return reg;
        }
        
        let reg = self.next_register;
        self.next_register += 1;
        self.in_use[reg as usize] = true;
        
        if reg > self.max_used {
            self.max_used = reg;
        }
        
        reg
    }
    
    /// Free a register
    pub fn free(&mut self, reg: u8) {
        if reg < self.next_register && self.in_use[reg as usize] {
            self.in_use[reg as usize] = false;
            self.free.push(reg);
        }
    }
    
    /// Allocate multiple registers
    pub fn allocate_many(&mut self, count: usize) -> Vec<u8> {
        let mut registers = Vec::with_capacity(count);
        for _ in 0..count {
            registers.push(self.allocate());
        }
        registers
    }
    
    /// Free multiple registers
    pub fn free_many(&mut self, registers: &[u8]) {
        for &reg in registers {
            self.free(reg);
        }
    }
    
    /// Get maximum register used
    pub fn max_used(&self) -> u8 {
        self.max_used
    }
    
    /// Reset the allocator
    pub fn reset(&mut self) {
        self.next_register = 0;
        self.max_used = 0;
        self.free.clear();
        self.in_use = vec![false; 256];
    }
}

/// A variable scope for the Lua compiler
#[derive(Debug, Clone)]
pub struct Scope {
    /// Local variables
    locals: HashMap<StringHandle, u8>,
    
    /// Upvalues
    upvalues: HashMap<StringHandle, usize>,
    
    /// Parent scope
    parent: Option<Box<Scope>>,
}

impl Scope {
    /// Create a new scope
    pub fn new() -> Self {
        Scope {
            locals: HashMap::new(),
            upvalues: HashMap::new(),
            parent: None,
        }
    }
    
    /// Create a new scope with a parent
    pub fn with_parent(parent: Scope) -> Self {
        Scope {
            locals: HashMap::new(),
            upvalues: HashMap::new(),
            parent: Some(Box::new(parent)),
        }
    }
    
    /// Add a local variable
    pub fn add_local(&mut self, name: StringHandle, reg: u8) {
        self.locals.insert(name, reg);
    }
    
    /// Look up a local variable
    pub fn lookup_local(&self, name: &StringHandle) -> Option<u8> {
        if let Some(reg) = self.locals.get(name) {
            Some(*reg)
        } else if let Some(parent) = &self.parent {
            parent.lookup_local(name)
        } else {
            None
        }
    }
    
    /// Add an upvalue
    pub fn add_upvalue(&mut self, name: StringHandle, index: usize) {
        self.upvalues.insert(name, index);
    }
    
    /// Look up an upvalue
    pub fn lookup_upvalue(&self, name: &StringHandle) -> Option<usize> {
        if let Some(idx) = self.upvalues.get(name) {
            Some(*idx)
        } else if let Some(parent) = &self.parent {
            parent.lookup_upvalue(name)
        } else {
            None
        }
    }
}

/// A string interner for the Lua compiler
#[derive(Debug, Clone)]
pub struct StringInterner {
    /// Strings by content
    strings: HashMap<String, StringHandle>,
    
    /// Strings by handle
    by_handle: HashMap<u32, String>,
    
    /// Next ID
    next_id: u32,
}

impl StringInterner {
    /// Create a new string interner
    pub fn new() -> Self {
        StringInterner {
            strings: HashMap::new(),
            by_handle: HashMap::new(),
            next_id: 0,
        }
    }
    
    /// Intern a string
    pub fn intern(&mut self, s: &str) -> StringHandle {
        if let Some(handle) = self.strings.get(s) {
            return *handle;
        }
        
        // Create a placeholder handle (no raw pointers!)
        let id = self.next_id;
        self.next_id += 1;
        
        let handle = StringHandle(super::arena::Handle {
            index: id,
            generation: 0,
            _phantom: PhantomData,
        });
        
        self.strings.insert(s.to_string(), handle);
        self.by_handle.insert(id, s.to_string());
        
        handle
    }
    
    /// Get a string by handle
    pub fn get(&self, handle: StringHandle) -> Option<&str> {
        self.by_handle.get(&handle.0.index).map(|s| s.as_str())
    }
    
    /// Export all interned strings as a vector
    pub fn export_strings(&self) -> Vec<String> {
        let mut strings = Vec::with_capacity(self.strings.len());
        for i in 0..self.next_id {
            if let Some(s) = self.by_handle.get(&i) {
                strings.push(s.clone());
            } else {
                strings.push(String::new());
            }
        }
        strings
    }
}

/// A constant value used during compilation
#[derive(Debug, Clone)]
pub enum ConstantValue {
    /// Nil value
    Nil,
    
    /// Boolean value
    Boolean(bool),
    
    /// Number value
    Number(f64),
    
    /// String value
    String(StringHandle),
}

/// Jump patching information
#[derive(Debug, Clone)]
pub struct JumpPatch {
    /// Instruction index
    pc: usize,
    
    /// Type of patch
    kind: JumpPatchKind,
}

/// Type of jump patch
#[derive(Debug, Clone)]
pub enum JumpPatchKind {
    /// Break statement - needs to be patched with the end of the loop
    Break,
    
    /// Forward jump - needs to be patched with the destination PC
    Forward,
    
    /// Continue statement - needs to be patched with the loop test
    Continue,
}

/// Debug information for a function
#[derive(Debug, Clone)]
pub struct DebugInfo {
    /// Line numbers
    pub line_info: Vec<u32>,
    
    /// Local variables
    pub locals: Vec<LocalVarInfo>,
    
    /// Function name
    pub name: Option<StringHandle>,
    
    /// Source filename
    pub source: Option<StringHandle>,
}

/// Compilation context for a single function
pub struct FunctionCompiler<'a> {
    /// String interner
    interner: &'a mut StringInterner,
    
    /// Register allocator
    reg_alloc: RegisterAllocator,
    
    /// Current scope
    scope: Scope,
    
    /// Jump patches to apply
    jumps: Vec<JumpPatch>,
    
    /// Constants
    constants: Vec<ConstantValue>,
    
    /// Bytecode
    bytecode: Vec<Instruction>,
    
    /// Debug info
    line_info: Vec<u32>,
    
    /// Local variables
    locals: Vec<LocalVarInfo>,
    
    /// Parameters
    params: Vec<StringHandle>,
    
    /// Is vararg function?
    is_vararg: bool,
    
    /// Upvalues
    upvalues: Vec<UpvalueDesc>,
    
    /// Current line
    current_line: u32,
    
    /// Function defined at line
    line_defined: u32,
    
    /// Function last line defined
    last_line_defined: u32,
    
    /// Loop nesting level
    loop_level: usize,
    
    /// Loop scopes
    loop_scopes: Vec<LoopScope>,
    
    /// Global function name
    name: Option<StringHandle>,
}

/// Information about a loop scope
#[derive(Debug, Clone)]
pub struct LoopScope {
    /// Start PC
    start_pc: usize,
    
    /// Break patches
    breaks: Vec<usize>,
    
    /// Continue patches
    continues: Vec<usize>,
}

impl<'a> FunctionCompiler<'a> {
    /// Create a new function compiler
    pub fn new(interner: &'a mut StringInterner) -> Self {
        FunctionCompiler {
            interner,
            reg_alloc: RegisterAllocator::new(),
            scope: Scope::new(),
            jumps: Vec::new(),
            constants: Vec::new(),
            bytecode: Vec::new(),
            line_info: Vec::new(),
            locals: Vec::new(),
            params: Vec::new(),
            is_vararg: false,
            upvalues: Vec::new(),
            current_line: 1,
            line_defined: 1,
            last_line_defined: 1,
            loop_level: 0,
            loop_scopes: Vec::new(),
            name: None,
        }
    }
    
    /// Emit an instruction
    pub fn emit(&mut self, opcode: OpCode, a: u8, b: u16, c: u16) -> usize {
        let instr = Instruction::new(opcode, a, b, c);
        self.bytecode.push(instr);
        self.line_info.push(self.current_line);
        self.bytecode.len() - 1
    }
    
    /// Emit an instruction with A, Bx format
    pub fn emit_abx(&mut self, opcode: OpCode, a: u8, bx: u32) -> usize {
        let instr = Instruction::new_abx(opcode, a, bx);
        self.bytecode.push(instr);
        self.line_info.push(self.current_line);
        self.bytecode.len() - 1
    }
    
    /// Emit an instruction with A, sBx format
    pub fn emit_asbx(&mut self, opcode: OpCode, a: u8, sbx: i32) -> usize {
        let instr = Instruction::new_asbx(opcode, a, sbx);
        self.bytecode.push(instr);
        self.line_info.push(self.current_line);
        self.bytecode.len() - 1
    }
    
    /// Add a constant
    pub fn add_constant(&mut self, value: ConstantValue) -> usize {
        // Check if constant already exists
        for (i, c) in self.constants.iter().enumerate() {
            match (c, &value) {
                (ConstantValue::Nil, ConstantValue::Nil) => return i,
                (ConstantValue::Boolean(a), ConstantValue::Boolean(b)) if a == b => return i,
                (ConstantValue::Number(a), ConstantValue::Number(b)) if a == b => return i,
                (ConstantValue::String(a), ConstantValue::String(b)) if a == b => return i,
                _ => {}
            }
        }
        
        // Add new constant
        let index = self.constants.len();
        self.constants.push(value);
        index
    }
    
    /// Compile a function
    pub fn compile_function(&mut self, params: Vec<StringHandle>, body: &[Node<Statement>], is_vararg: bool, line: u32) -> Result<()> {
        // Set up function info
        self.params = params.clone();
        self.is_vararg = is_vararg;
        self.line_defined = line;
        
        // Set up parameters as locals
        for (i, param) in params.iter().enumerate() {
            self.scope.add_local(*param, i as u8);
            
            // Add local info for debugging
            self.locals.push(LocalVarInfo {
                name: *param,
                start_pc: 0,
                end_pc: self.bytecode.len() as u32, // Will be updated at end
            });
        }
        
        // Compile body
        for stmt in body {
            self.compile_statement(stmt)?;
        }
        
        // Add final return
        self.emit(OpCode::Return, 0, 1, 0);
        
        Ok(())
    }
    
    /// Compile a statement
    pub fn compile_statement(&mut self, stmt: &Node<Statement>) -> Result<()> {
        // Update current line
        self.current_line = stmt.line as u32;
        
        match &stmt.value {
            Statement::Assignment { variables, expressions } => {
                self.compile_assignment(variables, expressions)?;
            },
            Statement::LocalDecl { names, values } => {
                self.compile_local_declaration(names, values)?;
            },
            Statement::Call(expr) => {
                // Compile function call (result is ignored)
                self.compile_expression(expr)?;
                
                // Free the result register
                self.reg_alloc.free(0); // Assumes call result is in register 0, simplification
            },
            Statement::Do(body) => {
                // Create new scope
                let parent_scope = self.scope.clone();
                self.scope = Scope::with_parent(parent_scope);
                
                // Compile body
                for stmt in body {
                    self.compile_statement(stmt)?;
                }
                
                // Restore scope
                if let Some(parent) = self.scope.parent.take() {
                    self.scope = *parent;
                }
            },
            Statement::While { condition, body } => {
                self.compile_while(condition, body)?;
            },
            Statement::Repeat { body, condition } => {
                self.compile_repeat(body, condition)?;
            },
            Statement::If { condition, then_block, elseif_clauses, else_block } => {
                self.compile_if(condition, then_block, elseif_clauses, else_block)?;
            },
            Statement::ForNum { var, start, limit, step, body } => {
                self.compile_numeric_for(var, start, limit, step.as_ref(), body)?;
            },
            Statement::ForIn { vars, iterators, body } => {
                self.compile_generic_for(vars, iterators, body)?;
            },
            Statement::FunctionDef { name, params, body, is_vararg, is_local } => {
                self.compile_function_def(name, params, body, *is_vararg, *is_local)?;
            },
            Statement::Return(exprs) => {
                self.compile_return(exprs)?;
            },
            Statement::Break => {
                if self.loop_level == 0 {
                    return Err(LuaError::SyntaxError {
                        message: "break outside of loop".to_string(),
                        line: stmt.line,
                        column: stmt.column,
                    });
                }
                
                // Emit jump instruction - will be patched later
                let pc = self.emit_asbx(OpCode::Jmp, 0, 0);
                
                // Add to current loop's break list
                if let Some(loop_scope) = self.loop_scopes.last_mut() {
                    loop_scope.breaks.push(pc);
                }
            },
        }
        
        Ok(())
    }
    
    /// Compile an assignment
    pub fn compile_assignment(&mut self, variables: &[Node<LValue>], expressions: &[Node<Expression>]) -> Result<()> {
        // Special case: empty assignment
        if variables.is_empty() {
            return Ok(());
        }
        
        // Simplification: only handle simple assignments
        // A complete implementation would handle complex assignments with
        // temporary registers and multiple values
        
        if variables.len() == 1 && expressions.len() == 1 {
            let var = &variables[0].value;
            let expr = &expressions[0];
            
            match var {
                LValue::Name(name) => {
                    // Check if local variable
                    if let Some(reg) = self.scope.lookup_local(name) {
                        // Local assignment
                        let expr_reg = self.compile_expression(expr)?;
                        self.emit(OpCode::Move, reg, expr_reg.into(), 0);
                        self.reg_alloc.free(expr_reg);
                    } else {
                        // Global assignment
                        let expr_reg = self.compile_expression(expr)?;
                        let const_idx = self.add_constant(ConstantValue::String(*name));
                        self.emit_abx(OpCode::SetGlobal, expr_reg, const_idx as u32);
                        self.reg_alloc.free(expr_reg);
                    }
                },
                LValue::FieldAccess { object, field } => {
                    // Table field assignment
                    let object_reg = self.compile_expression(&object)?;
                    let expr_reg = self.compile_expression(expr)?;
                    
                    // Get constant for field name
                    let field_const = self.add_constant(ConstantValue::String(*field)) | 0x100;
                    
                    // SetTable instruction (A, B, C): t[k] = v, where t is in register A, k is constant at index B, v is in register C
                    self.emit(OpCode::SetTable, object_reg, field_const as u16, expr_reg.into());
                    
                    // Free temporary registers
                    self.reg_alloc.free(object_reg);
                    self.reg_alloc.free(expr_reg);
                },
                LValue::IndexAccess { object, index } => {
                    // Table index assignment
                    let object_reg = self.compile_expression(&object)?;
                    let index_reg = self.compile_expression(&index)?;
                    let expr_reg = self.compile_expression(expr)?;
                    
                    // SetTable instruction (A, B, C): t[k] = v, where t is in register A, k is in register B, v is in register C
                    self.emit(OpCode::SetTable, object_reg, index_reg.into(), expr_reg.into());
                    
                    // Free temporary registers
                    self.reg_alloc.free(object_reg);
                    self.reg_alloc.free(index_reg);
                    self.reg_alloc.free(expr_reg);
                },
            }
        } else {
            // Multiple assignments
            // For simplicity, evaluate expressions first, then assign
            let mut expr_regs = Vec::with_capacity(expressions.len());
            
            // Evaluate all expressions
            for expr in expressions {
                let reg = self.compile_expression(expr)?;
                expr_regs.push(reg);
            }
            
            // Assign to variables
            for (i, var) in variables.iter().enumerate() {
                if i >= expr_regs.len() {
                    // More variables than expressions - assign nil
                    let var = &var.value;
                    
                    match var {
                        LValue::Name(name) => {
                            // Check if local variable
                            if let Some(reg) = self.scope.lookup_local(name) {
                                // Local assignment
                                self.emit(OpCode::LoadNil, reg, reg.into(), 0);
                            } else {
                                // Global assignment
                                let nil_reg = self.reg_alloc.allocate();
                                self.emit(OpCode::LoadNil, nil_reg, nil_reg.into(), 0);
                                let const_idx = self.add_constant(ConstantValue::String(*name));
                                self.emit_abx(OpCode::SetGlobal, nil_reg, const_idx as u32);
                                self.reg_alloc.free(nil_reg);
                            }
                        },
                        // Other variable types would be handled similarly
                        _ => {
                            // For simplicity, only handle local variables in this example
                            return Err(LuaError::NotImplemented("complex multiple assignment".to_string()));
                        }
                    }
                } else {
                    // Assign expression result
                    let expr_reg = expr_regs[i];
                    let var = &var.value;
                    
                    match var {
                        LValue::Name(name) => {
                            // Check if local variable
                            if let Some(reg) = self.scope.lookup_local(name) {
                                // Local assignment
                                self.emit(OpCode::Move, reg, expr_reg.into(), 0);
                            } else {
                                // Global assignment
                                let const_idx = self.add_constant(ConstantValue::String(*name));
                                self.emit_abx(OpCode::SetGlobal, expr_reg, const_idx as u32);
                            }
                        },
                        // Other variable types would be handled similarly
                        _ => {
                            // For simplicity, only handle local variables in this example
                            return Err(LuaError::NotImplemented("complex multiple assignment".to_string()));
                        }
                    }
                }
            }
            
            // Free temporary registers
            for reg in expr_regs {
                self.reg_alloc.free(reg);
            }
        }
        
        Ok(())
    }
    
    /// Compile a local declaration
    pub fn compile_local_declaration(&mut self, names: &[StringHandle], values: &[Node<Expression>]) -> Result<()> {
        // Evaluate expressions first
        let mut registers = Vec::with_capacity(values.len());
        
        for expr in values {
            let reg = self.compile_expression(expr)?;
            registers.push(reg);
        }
        
        // Assign to local variables
        for (i, name) in names.iter().enumerate() {
            let reg = if i < registers.len() {
                // Use the register with the expression result
                registers[i]
            } else {
                // More names than values - assign nil
                let reg = self.reg_alloc.allocate();
                self.emit(OpCode::LoadNil, reg, reg.into(), 0);
                reg
            };
            
            // Add to locals table
            self.scope.add_local(*name, reg);
            
            // Add local variable info for debugging
            self.locals.push(LocalVarInfo {
                name: *name,
                start_pc: self.bytecode.len() as u32,
                end_pc: self.bytecode.len() as u32, // Will be updated at scope end
            });
        }
        
        // Don't free registers - they're now used by the local variables
        
        Ok(())
    }
    
    /// Compile a while loop
    pub fn compile_while(&mut self, condition: &Node<Expression>, body: &[Node<Statement>]) -> Result<()> {
        // Record loop start position
        let loop_start = self.bytecode.len();
        
        // Increase loop level
        self.loop_level += 1;
        
        // Create loop scope
        self.loop_scopes.push(LoopScope {
            start_pc: loop_start,
            breaks: Vec::new(),
            continues: Vec::new(),
        });
        
        // Compile condition
        let cond_reg = self.compile_expression(condition)?;
        
        // Test condition
        let exit_jump = self.emit(OpCode::Test, cond_reg, 0, 0);
        self.emit_asbx(OpCode::Jmp, 0, 0); // Skip body if condition is false
        
        // Free condition register
        self.reg_alloc.free(cond_reg);
        
        // Create new scope for body
        let parent_scope = self.scope.clone();
        self.scope = Scope::with_parent(parent_scope);
        
        // Compile body
        for stmt in body {
            self.compile_statement(stmt)?;
        }
        
        // Jump back to start
        self.emit_asbx(OpCode::Jmp, 0, -(self.bytecode.len() as i32 - loop_start as i32 + 1));
        
        // Patch exit jump
        let exit_pc = self.bytecode.len();
        if let Instruction(instr) = self.bytecode[exit_jump + 1] {
            let opcode = instr & 0x3F;
            let a = (instr >> 6) & 0xFF;
            let sbx = exit_pc as i32 - (exit_jump + 1) as i32 - 1;
            self.bytecode[exit_jump + 1] = Instruction::new_asbx(
                // Convert raw opcode back to enum
                match opcode {
                    22 => OpCode::Jmp,
                    _ => OpCode::Jmp, // Default in case of error
                },
                a as u8,
                sbx
            );
        }
        
        // Patch any breaks
        let loop_scope = self.loop_scopes.pop().unwrap();
        for break_pc in loop_scope.breaks {
            if let Instruction(instr) = self.bytecode[break_pc] {
                let opcode = instr & 0x3F;
                let a = (instr >> 6) & 0xFF;
                let sbx = exit_pc as i32 - break_pc as i32 - 1;
                self.bytecode[break_pc] = Instruction::new_asbx(
                    // Convert raw opcode back to enum
                    match opcode {
                        22 => OpCode::Jmp,
                        _ => OpCode::Jmp, // Default in case of error
                    },
                    a as u8,
                    sbx
                );
            }
        }
        
        // Restore scope
        if let Some(parent) = self.scope.parent.take() {
            self.scope = *parent;
        }
        
        // Decrease loop level
        self.loop_level -= 1;
        
        Ok(())
    }
    
    /// Compile a repeat-until loop
    pub fn compile_repeat(&mut self, body: &[Node<Statement>], condition: &Node<Expression>) -> Result<()> {
        // Record loop start position
        let loop_start = self.bytecode.len();
        
        // Increase loop level
        self.loop_level += 1;
        
        // Create loop scope
        self.loop_scopes.push(LoopScope {
            start_pc: loop_start,
            breaks: Vec::new(),
            continues: Vec::new(),
        });
        
        // Create new scope for body
        let parent_scope = self.scope.clone();
        self.scope = Scope::with_parent(parent_scope);
        
        // Compile body
        for stmt in body {
            self.compile_statement(stmt)?;
        }
        
        // Compile condition
        let cond_reg = self.compile_expression(condition)?;
        
        // Test condition (jump to start if false)
        self.emit(OpCode::Test, cond_reg, 0, 1); // Invert condition
        self.emit_asbx(OpCode::Jmp, 0, -(self.bytecode.len() as i32 - loop_start as i32 + 1));
        
        // Free condition register
        self.reg_alloc.free(cond_reg);
        
        // Patch any breaks
        let loop_scope = self.loop_scopes.pop().unwrap();
        let exit_pc = self.bytecode.len();
        for break_pc in loop_scope.breaks {
            if let Instruction(instr) = self.bytecode[break_pc] {
                let opcode = instr & 0x3F;
                let a = (instr >> 6) & 0xFF;
                let sbx = exit_pc as i32 - break_pc as i32 - 1;
                self.bytecode[break_pc] = Instruction::new_asbx(
                    // Convert raw opcode back to enum
                    match opcode {
                        22 => OpCode::Jmp,
                        _ => OpCode::Jmp, // Default in case of error
                    },
                    a as u8,
                    sbx
                );
            }
        }
        
        // Restore scope
        if let Some(parent) = self.scope.parent.take() {
            self.scope = *parent;
        }
        
        // Decrease loop level
        self.loop_level -= 1;
        
        Ok(())
    }
    
    /// Compile an if statement
    pub fn compile_if(&mut self, condition: &Node<Expression>, then_block: &[Node<Statement>], 
                     elseif_clauses: &[(Node<Expression>, Vec<Node<Statement>>)],
                     else_block: &Option<Vec<Node<Statement>>>) -> Result<()> {
        
        // Compile condition
        let cond_reg = self.compile_expression(condition)?;
        
        // Test condition
        self.emit(OpCode::Test, cond_reg, 0, 0);
        let then_jmp = self.emit_asbx(OpCode::Jmp, 0, 0); // To be patched
        
        // Free condition register
        self.reg_alloc.free(cond_reg);
        
        // Create new scope for then block
        let parent_scope = self.scope.clone();
        self.scope = Scope::with_parent(parent_scope);
        
        // Compile then block
        for stmt in then_block {
            self.compile_statement(stmt)?;
        }
        
        // Jump to end (skip else blocks)
        let end_jmp = self.emit_asbx(OpCode::Jmp, 0, 0); // To be patched
        
        // Restore scope
        if let Some(parent) = self.scope.parent.take() {
            self.scope = *parent;
        }
        
        // Patch then jump to skip then block
        let elseif_start = self.bytecode.len();
        if let Instruction(instr) = self.bytecode[then_jmp] {
            let opcode = instr & 0x3F;
            let a = (instr >> 6) & 0xFF;
            let sbx = elseif_start as i32 - then_jmp as i32 - 1;
            self.bytecode[then_jmp] = Instruction::new_asbx(
                // Convert raw opcode back to enum
                match opcode {
                    22 => OpCode::Jmp,
                    _ => OpCode::Jmp, // Default in case of error
                },
                a as u8,
                sbx
            );
        }
        
        // Compile elseif clauses
        let mut end_jumps = vec![end_jmp];
        
        for (condition, block) in elseif_clauses {
            // Compile condition
            let cond_reg = self.compile_expression(condition)?;
            
            // Test condition
            self.emit(OpCode::Test, cond_reg, 0, 0);
            let elseif_jmp = self.emit_asbx(OpCode::Jmp, 0, 0); // To be patched
            
            // Free condition register
            self.reg_alloc.free(cond_reg);
            
            // Create new scope for elseif block
            let parent_scope = self.scope.clone();
            self.scope = Scope::with_parent(parent_scope);
            
            // Compile elseif block
            for stmt in block {
                self.compile_statement(stmt)?;
            }
            
            // Jump to end (skip else blocks)
            end_jumps.push(self.emit_asbx(OpCode::Jmp, 0, 0)); // To be patched
            
            // Restore scope
            if let Some(parent) = self.scope.parent.take() {
                self.scope = *parent;
            }
            
            // Patch elseif jump to skip elseif block
            let next_clause = self.bytecode.len();
            if let Instruction(instr) = self.bytecode[elseif_jmp] {
                let opcode = instr & 0x3F;
                let a = (instr >> 6) & 0xFF;
                let sbx = next_clause as i32 - elseif_jmp as i32 - 1;
                self.bytecode[elseif_jmp] = Instruction::new_asbx(
                    // Convert raw opcode back to enum
                    match opcode {
                        22 => OpCode::Jmp,
                        _ => OpCode::Jmp, // Default in case of error
                    },
                    a as u8,
                    sbx
                );
            }
        }
        
        // Compile else block
        if let Some(block) = else_block {
            // Create new scope for else block
            let parent_scope = self.scope.clone();
            self.scope = Scope::with_parent(parent_scope);
            
            // Compile else block
            for stmt in block {
                self.compile_statement(stmt)?;
            }
            
            // Restore scope
            if let Some(parent) = self.scope.parent.take() {
                self.scope = *parent;
            }
        }
        
        // Patch end jumps
        let end_pc = self.bytecode.len();
        for jmp in end_jumps {
            if let Instruction(instr) = self.bytecode[jmp] {
                let opcode = instr & 0x3F;
                let a = (instr >> 6) & 0xFF;
                let sbx = end_pc as i32 - jmp as i32 - 1;
                self.bytecode[jmp] = Instruction::new_asbx(
                    // Convert raw opcode back to enum
                    match opcode {
                        22 => OpCode::Jmp,
                        _ => OpCode::Jmp, // Default in case of error
                    },
                    a as u8,
                    sbx
                );
            }
        }
        
        Ok(())
    }
    
    /// Compile a numeric for loop
    pub fn compile_numeric_for(&mut self, var: &StringHandle, start: &Node<Expression>, 
                              limit: &Node<Expression>, step: Option<&Node<Expression>>, 
                              body: &[Node<Statement>]) -> Result<()> {
        // Create new scope for loop
        let parent_scope = self.scope.clone();
        self.scope = Scope::with_parent(parent_scope);
        
        // Allocate registers for loop variables
        let var_reg = self.reg_alloc.allocate();
        let limit_reg = self.reg_alloc.allocate();
        let step_reg = self.reg_alloc.allocate();
        let index_reg = self.reg_alloc.allocate();
        
        // Add loop variable to scope
        self.scope.add_local(*var, index_reg);
        
        // Compile loop initialization
        // Evaluate expressions and store in registers
        let start_reg = self.compile_expression(start)?;
        self.emit(OpCode::Move, var_reg, start_reg.into(), 0);
        self.reg_alloc.free(start_reg);
        
        let limit_expr_reg = self.compile_expression(limit)?;
        self.emit(OpCode::Move, limit_reg, limit_expr_reg.into(), 0);
        self.reg_alloc.free(limit_expr_reg);
        
        if let Some(step_expr) = step {
            let step_expr_reg = self.compile_expression(step_expr)?;
            self.emit(OpCode::Move, step_reg, step_expr_reg.into(), 0);
            self.reg_alloc.free(step_expr_reg);
        } else {
            // Default step is 1
            let const_idx = self.add_constant(ConstantValue::Number(1.0));
            self.emit_abx(OpCode::LoadK, step_reg, const_idx as u32);
        }
        
        // Emit FORPREP
        let forprep = self.emit_asbx(OpCode::ForPrep, var_reg, 0); // To be patched
        
        // Increase loop level
        self.loop_level += 1;
        
        // Create loop scope
        self.loop_scopes.push(LoopScope {
            start_pc: forprep,
            breaks: Vec::new(),
            continues: Vec::new(),
        });
        
        // Compile body
        for stmt in body {
            self.compile_statement(stmt)?;
        }
        
        // Emit FORLOOP
        let forloop = self.emit_asbx(OpCode::ForLoop, var_reg, -(self.bytecode.len() as i32 - forprep as i32));
        
        // Patch FORPREP to jump to after the body
        if let Instruction(instr) = self.bytecode[forprep] {
            let opcode = instr & 0x3F;
            let a = (instr >> 6) & 0xFF;
            let sbx = forloop as i32 - forprep as i32 + 1;
            self.bytecode[forprep] = Instruction::new_asbx(
                // Convert raw opcode back to enum
                match opcode {
                    32 => OpCode::ForPrep,
                    _ => OpCode::ForPrep, // Default in case of error
                },
                a as u8,
                sbx
            );
        }
        
        // Patch breaks
        let loop_scope = self.loop_scopes.pop().unwrap();
        let exit_pc = self.bytecode.len();
        for break_pc in loop_scope.breaks {
            if let Instruction(instr) = self.bytecode[break_pc] {
                let opcode = instr & 0x3F;
                let a = (instr >> 6) & 0xFF;
                let sbx = exit_pc as i32 - break_pc as i32 - 1;
                self.bytecode[break_pc] = Instruction::new_asbx(
                    // Convert raw opcode back to enum
                    match opcode {
                        22 => OpCode::Jmp,
                        _ => OpCode::Jmp, // Default in case of error
                    },
                    a as u8,
                    sbx
                );
            }
        }
        
        // Free registers
        self.reg_alloc.free(var_reg);
        self.reg_alloc.free(limit_reg);
        self.reg_alloc.free(step_reg);
        self.reg_alloc.free(index_reg);
        
        // Restore scope
        if let Some(parent) = self.scope.parent.take() {
            self.scope = *parent;
        }
        
        // Decrease loop level
        self.loop_level -= 1;
        
        Ok(())
    }
    
    /// Compile a generic for loop
    pub fn compile_generic_for(&mut self, vars: &[StringHandle], iterators: &[Node<Expression>], body: &[Node<Statement>]) -> Result<()> {
        // Create new scope for loop
        let parent_scope = self.scope.clone();
        self.scope = Scope::with_parent(parent_scope);
        
        // Allocate registers for loop variables
        let mut var_regs = Vec::new();
        for _ in 0..vars.len() {
            var_regs.push(self.reg_alloc.allocate());
        }
        
        // Add loop variables to scope
        for (i, var) in vars.iter().enumerate() {
            self.scope.add_local(*var, var_regs[i]);
        }
        
        // Compile iterator initialization
        // Evaluate expressions and store in registers
        let mut iter_regs = Vec::new();
        for iter in iterators {
            let reg = self.compile_expression(iter)?;
            iter_regs.push(reg);
        }
        
        // Set up iterator, state, and control variable
        let iter_func_reg = self.reg_alloc.allocate();
        let state_reg = self.reg_alloc.allocate();
        let control_var_reg = self.reg_alloc.allocate();
        
        if iter_regs.len() >= 3 {
            // All three values provided
            self.emit(OpCode::Move, iter_func_reg, iter_regs[0].into(), 0);
            self.emit(OpCode::Move, state_reg, iter_regs[1].into(), 0);
            self.emit(OpCode::Move, control_var_reg, iter_regs[2].into(), 0);
        } else if iter_regs.len() == 2 {
            // Function and state provided
            self.emit(OpCode::Move, iter_func_reg, iter_regs[0].into(), 0);
            self.emit(OpCode::Move, state_reg, iter_regs[1].into(), 0);
            self.emit(OpCode::LoadNil, control_var_reg, control_var_reg.into(), 0);
        } else if iter_regs.len() == 1 {
            // Only function provided
            self.emit(OpCode::Move, iter_func_reg, iter_regs[0].into(), 0);
            self.emit(OpCode::LoadNil, state_reg, state_reg.into(), 0);
            self.emit(OpCode::LoadNil, control_var_reg, control_var_reg.into(), 0);
        } else {
            // No iterators provided - error
            return Err(LuaError::SyntaxError {
                message: "no iterators provided for generic for loop".to_string(),
                line: 0,
                column: 0,
            });
        }
        
        // Free iterator registers
        for reg in iter_regs {
            self.reg_alloc.free(reg);
        }
        
        // Jump to first call
        let init_jmp = self.emit_asbx(OpCode::Jmp, 0, 0); // To be patched
        
        // Record loop start (after the for loop variables)
        let loop_start = self.bytecode.len();
        
        // Increase loop level
        self.loop_level += 1;
        
        // Create loop scope
        self.loop_scopes.push(LoopScope {
            start_pc: loop_start,
            breaks: Vec::new(),
            continues: Vec::new(),
        });
        
        // Compile body
        for stmt in body {
            self.compile_statement(stmt)?;
        }
        
        // Call iterator function
        let call_pc = self.bytecode.len();
        self.emit(OpCode::TForLoop, iter_func_reg, 0, vars.len() as u16);
        
        // Patch initial jump
        if let Instruction(instr) = self.bytecode[init_jmp] {
            let opcode = instr & 0x3F;
            let a = (instr >> 6) & 0xFF;
            let sbx = call_pc as i32 - init_jmp as i32 - 1;
            self.bytecode[init_jmp] = Instruction::new_asbx(
                // Convert raw opcode back to enum
                match opcode {
                    22 => OpCode::Jmp,
                    _ => OpCode::Jmp, // Default in case of error
                },
                a as u8,
                sbx
            );
        }
        
        // Jump back to loop start
        self.emit_asbx(OpCode::Jmp, 0, -(self.bytecode.len() as i32 - loop_start as i32 + 1));
        
        // Patch breaks
        let loop_scope = self.loop_scopes.pop().unwrap();
        let exit_pc = self.bytecode.len();
        for break_pc in loop_scope.breaks {
            if let Instruction(instr) = self.bytecode[break_pc] {
                let opcode = instr & 0x3F;
                let a = (instr >> 6) & 0xFF;
                let sbx = exit_pc as i32 - break_pc as i32 - 1;
                self.bytecode[break_pc] = Instruction::new_asbx(
                    // Convert raw opcode back to enum
                    match opcode {
                        22 => OpCode::Jmp,
                        _ => OpCode::Jmp, // Default in case of error
                    },
                    a as u8,
                    sbx
                );
            }
        }
        
        // Free registers
        self.reg_alloc.free(iter_func_reg);
        self.reg_alloc.free(state_reg);
        self.reg_alloc.free(control_var_reg);
        for reg in var_regs {
            self.reg_alloc.free(reg);
        }
        
        // Restore scope
        if let Some(parent) = self.scope.parent.take() {
            self.scope = *parent;
        }
        
        // Decrease loop level
        self.loop_level -= 1;
        
        Ok(())
    }
    
    /// Compile a function definition
    pub fn compile_function_def(&mut self, name: &Node<FunctionName>, params: &[StringHandle], 
                               body: &[Node<Statement>], is_vararg: bool, is_local: bool) -> Result<()> {
        // Collect string parts first to avoid borrowing conflicts
        let mut path_parts = Vec::new();
        
        match &name.value {
            FunctionName::Name(name) => {
                // Just one name component
                let name_str = self.interner.get(*name).unwrap_or("unknown").to_string();
                path_parts.push(name_str);
            },
            FunctionName::Path { base, fields } => {
                // Table field path: base.field1.field2...
                let base_str = self.interner.get(*base).unwrap_or("unknown").to_string();
                path_parts.push(base_str);
                
                for field in fields {
                    let field_str = self.interner.get(*field).unwrap_or("unknown").to_string();
                    path_parts.push(field_str);
                }
            },
            FunctionName::Method { base, fields, method } => {
                // Method: base.field1.field2:method
                let base_str = self.interner.get(*base).unwrap_or("unknown").to_string();
                path_parts.push(base_str);
                
                for field in fields {
                    let field_str = self.interner.get(*field).unwrap_or("unknown").to_string();
                    path_parts.push(field_str);
                }
                
                let method_str = self.interner.get(*method).unwrap_or("unknown").to_string();
                path_parts.push(format!(":{}", method_str));
            }
        }
        
        // Now construct path string
        let full_path = if path_parts.len() > 1 {
            // Multiple components, need to join
            let mut result = path_parts[0].clone();
            for i in 1..path_parts.len() {
                let part = &path_parts[i];
                if part.starts_with(':') {
                    // This is a method component
                    result.push_str(part);
                } else {
                    // Normal field
                    result.push('.');
                    result.push_str(part);
                }
            }
            result
        } else if !path_parts.is_empty() {
            // Just one component
            path_parts[0].clone()
        } else {
            "anonymous".to_string()
        };
        
        // Intern the full path
        let path_handle = self.interner.intern(&full_path);
        
        // Create a sub-compiler passing in the interner reference
        let mut sub_compiler = FunctionCompiler::new(self.interner);
        
        // Set function name if a string handle is needed 
        match &name.value {
            FunctionName::Name(_) => {
                sub_compiler.name = Some(path_handle);
            },
            FunctionName::Path { .. } => {
                sub_compiler.name = Some(path_handle);
            },
            FunctionName::Method { .. } => {
                sub_compiler.name = Some(path_handle);
            }
        }
        
        // Compile the function with the sub-compiler
        sub_compiler.compile_function(params.to_vec(), body, is_vararg, name.line as u32)?;
        
        // Bind the function properly based on type
        if is_local {
            // Local function
            if let FunctionName::Name(func_name) = &name.value {
                // Allocate register for function
                let func_reg = self.reg_alloc.allocate();
                
                // Create closure
                self.emit_abx(OpCode::Closure, func_reg, 0); // Dummy index for now
                
                // Add to scope
                self.scope.add_local(*func_name, func_reg);
                
                // Don't free the register - it's now a local variable
            } else {
                return Err(LuaError::SyntaxError {
                    message: "local function name must be a simple name".to_string(),
                    line: name.line,
                    column: name.column,
                });
            }
        } else {
            // Global function
            match &name.value {
                FunctionName::Name(func_name) => {
                    // Allocate register for function
                    let func_reg = self.reg_alloc.allocate();
                    
                    // Create closure
                    self.emit_abx(OpCode::Closure, func_reg, 0); // Dummy index for now
                    
                    // Set global
                    let const_idx = self.add_constant(ConstantValue::String(*func_name));
                    self.emit_abx(OpCode::SetGlobal, func_reg, const_idx as u32);
                    
                    // Free register
                    self.reg_alloc.free(func_reg);
                },
                FunctionName::Path { base, fields } => {
                    // Handle table path - t.f.g = function() ... end
                    let base_reg = self.reg_alloc.allocate();
                    
                    // Get base table
                    let base_const = self.add_constant(ConstantValue::String(*base));
                    self.emit_abx(OpCode::GetGlobal, base_reg, base_const as u32);
                    
                    // Follow fields
                    let mut current_reg = base_reg;
                    for (i, field) in fields.iter().enumerate() {
                        if i < fields.len() - 1 {
                            // Intermediate field - get subtable
                            let field_const = self.add_constant(ConstantValue::String(*field));
                            let subtable_reg = self.reg_alloc.allocate();
                            
                            self.emit(OpCode::GetTable, subtable_reg, current_reg.into(), (field_const | 0x100) as u16);
                            
                            if current_reg != base_reg {
                                self.reg_alloc.free(current_reg);
                            }
                            
                            current_reg = subtable_reg;
                        } else {
                            // Last field - set function
                            let func_reg = self.reg_alloc.allocate();
                            let field_const = self.add_constant(ConstantValue::String(*field));
                            
                            // Create closure
                            self.emit_abx(OpCode::Closure, func_reg, 0); // Dummy index for now
                            
                            // Set table field
                            self.emit(OpCode::SetTable, current_reg, (field_const | 0x100) as u16, func_reg.into());
                            
                            // Free registers
                            self.reg_alloc.free(func_reg);
                            self.reg_alloc.free(current_reg);
                        }
                    }
                },
                FunctionName::Method { base, fields, method } => {
                    // Handle method definition - t:m() means function t.m(self, ...) ... end
                    let base_reg = self.reg_alloc.allocate();
                    
                    // Get base table
                    let base_const = self.add_constant(ConstantValue::String(*base));
                    self.emit_abx(OpCode::GetGlobal, base_reg, base_const as u32);
                    
                    // Follow fields
                    let mut current_reg = base_reg;
                    for field in fields {
                        // Intermediate field - get subtable
                        let field_const = self.add_constant(ConstantValue::String(*field));
                        let subtable_reg = self.reg_alloc.allocate();
                        
                        self.emit(OpCode::GetTable, subtable_reg, current_reg.into(), (field_const | 0x100) as u16);
                        
                        if current_reg != base_reg {
                            self.reg_alloc.free(current_reg);
                        }
                        
                        current_reg = subtable_reg;
                    }
                    
                    // Set method
                    let func_reg = self.reg_alloc.allocate();
                    let method_const = self.add_constant(ConstantValue::String(*method));
                    
                    // Create closure
                    self.emit_abx(OpCode::Closure, func_reg, 0); // Dummy index for now
                    
                    // Set table field
                    self.emit(OpCode::SetTable, current_reg, (method_const | 0x100) as u16, func_reg.into());
                    
                    // Free registers
                    self.reg_alloc.free(func_reg);
                    self.reg_alloc.free(current_reg);
                }
            }
        }
        
        Ok(())
    }
    
    /// Compile a return statement
    pub fn compile_return(&mut self, exprs: &[Node<Expression>]) -> Result<()> {
        if exprs.is_empty() {
            // Return nil
            self.emit(OpCode::Return, 0, 1, 0);
            return Ok(());
        }
        
        // Compile expressions
        let mut regs = Vec::with_capacity(exprs.len());
        for expr in exprs {
            let reg = self.compile_expression(expr)?;
            regs.push(reg);
        }
        
        // Move results to consecutive registers starting from 0
        for (i, reg) in regs.iter().enumerate() {
            if *reg != i as u8 {
                self.emit(OpCode::Move, i as u8, (*reg).into(), 0);
            }
        }
        
        // Return values
        self.emit(OpCode::Return, 0, exprs.len() as u16 + 1, 0);
        
        // Free registers
        for reg in regs {
            self.reg_alloc.free(reg);
        }
        
        Ok(())
    }
    
    /// Compile an expression
    pub fn compile_expression(&mut self, expr: &Node<Expression>) -> Result<u8> {
        // Update current line
        self.current_line = expr.line as u32;
        
        // Compile the expression and return the register containing the result
        match &expr.value {
            Expression::Nil => {
                let reg = self.reg_alloc.allocate();
                self.emit(OpCode::LoadNil, reg, reg.into(), 0);
                Ok(reg)
            },
            Expression::Boolean(b) => {
                let reg = self.reg_alloc.allocate();
                self.emit(OpCode::LoadBool, reg, if *b { 1 } else { 0 }, 0);
                Ok(reg)
            },
            Expression::Number(n) => {
                let reg = self.reg_alloc.allocate();
                let const_idx = self.add_constant(ConstantValue::Number(*n));
                self.emit_abx(OpCode::LoadK, reg, const_idx as u32);
                Ok(reg)
            },
            Expression::String(s) => {
                let reg = self.reg_alloc.allocate();
                let const_idx = self.add_constant(ConstantValue::String(*s));
                self.emit_abx(OpCode::LoadK, reg, const_idx as u32);
                Ok(reg)
            },
            Expression::Variable(name) => {
                // Check if local variable
                if let Some(reg) = self.scope.lookup_local(name) {
                    return Ok(reg);
                }
                
                // Not a local, must be global
                let reg = self.reg_alloc.allocate();
                let const_idx = self.add_constant(ConstantValue::String(*name));
                self.emit_abx(OpCode::GetGlobal, reg, const_idx as u32);
                Ok(reg)
            },
            Expression::Table(fields) => {
                let reg = self.reg_alloc.allocate();
                
                // Create table
                self.emit(OpCode::NewTable, reg, 0, 0);
                
                // Set fields
                for (i, field) in fields.iter().enumerate() {
                    match field {
                        TableField::Array(value) => {
                            // Array entry - [i] = value
                            let value_reg = self.compile_expression(value)?;
                            
                            // Use SetList for array entries
                            self.emit(OpCode::SetTable, reg, (i + 1) as u16, value_reg.into());
                            
                            // Free value register
                            self.reg_alloc.free(value_reg);
                        },
                        TableField::Hash { key, value } => {
                            // Hash entry - [key] = value
                            let key_reg = self.compile_expression(key)?;
                            let value_reg = self.compile_expression(value)?;
                            
                            self.emit(OpCode::SetTable, reg, key_reg.into(), value_reg.into());
                            
                            // Free registers
                            self.reg_alloc.free(key_reg);
                            self.reg_alloc.free(value_reg);
                        },
                        TableField::Field { name, value } => {
                            // Field entry - name = value
                            let value_reg = self.compile_expression(value)?;
                            let const_idx = self.add_constant(ConstantValue::String(*name));
                            
                            self.emit(OpCode::SetTable, reg, (const_idx | 0x100) as u16, value_reg.into());
                            
                            // Free value register
                            self.reg_alloc.free(value_reg);
                        },
                    }
                }
                
                Ok(reg)
            },
            Expression::Call { func, args } => {
                let func_reg = self.compile_expression(func)?;
                
                // Allocate consecutive registers for arguments
                let mut arg_regs = Vec::with_capacity(args.len());
                for arg in args {
                    let arg_reg = self.compile_expression(arg)?;
                    arg_regs.push(arg_reg);
                }
                
                // Move arguments to consecutive registers if needed
                let base_reg = func_reg + 1;
                for (i, arg_reg) in arg_regs.iter().enumerate() {
                    if *arg_reg != base_reg + i as u8 {
                        self.emit(OpCode::Move, base_reg + i as u8, (*arg_reg).into(), 0);
                    }
                }
                
                // Call function
                self.emit(OpCode::Call, func_reg, args.len() as u16 + 1, 2);
                
                // Free registers
                for reg in arg_regs {
                    self.reg_alloc.free(reg);
                }
                
                // Result is in func_reg
                Ok(func_reg)
            },
            Expression::MethodCall { object, method, args } => {
                // Compile object expression
                let object_reg = self.compile_expression(object)?;
                
                // Allocate consecutive registers for method receive and arguments
                let method_reg = self.reg_alloc.allocate();
                
                // Get method
                let method_const = self.add_constant(ConstantValue::String(*method));
                self.emit(OpCode::Self_, method_reg, object_reg.into(), (method_const | 0x100) as u16);
                
                // Allocate consecutive registers for arguments
                let mut arg_regs = Vec::with_capacity(args.len());
                for arg in args {
                    let arg_reg = self.compile_expression(arg)?;
                    arg_regs.push(arg_reg);
                }
                
                // Move arguments to consecutive registers if needed
                let base_reg = method_reg + 1;
                for (i, arg_reg) in arg_regs.iter().enumerate() {
                    if *arg_reg != base_reg + i as u8 {
                        self.emit(OpCode::Move, base_reg + i as u8, (*arg_reg).into(), 0);
                    }
                }
                
                // Call method
                self.emit(OpCode::Call, method_reg, args.len() as u16 + 1, 2);
                
                // Free registers
                self.reg_alloc.free(object_reg);
                for reg in arg_regs {
                    self.reg_alloc.free(reg);
                }
                
                // Result is in method_reg
                Ok(method_reg)
            },
            Expression::FieldAccess { object, field } => {
                // Compile object expression
                let object_reg = self.compile_expression(object)?;
                
                // Allocate register for field
                let field_reg = self.reg_alloc.allocate();
                
                // Get field
                let field_const = self.add_constant(ConstantValue::String(*field));
                self.emit(OpCode::GetTable, field_reg, object_reg.into(), (field_const | 0x100) as u16);
                
                // Free object register
                self.reg_alloc.free(object_reg);
                
                Ok(field_reg)
            },
            Expression::IndexAccess { object, index } => {
                // Compile object expression
                let object_reg = self.compile_expression(object)?;
                
                // Compile index expression
                let index_reg = self.compile_expression(index)?;
                
                // Allocate register for result
                let result_reg = self.reg_alloc.allocate();
                
                // Get indexed value
                self.emit(OpCode::GetTable, result_reg, object_reg.into(), index_reg.into());
                
                // Free registers
                self.reg_alloc.free(object_reg);
                self.reg_alloc.free(index_reg);
                
                Ok(result_reg)
            },
            Expression::Function { params, body, is_vararg } => {
                // Create sub-compiler for the function
                let mut sub_compiler = FunctionCompiler::new(self.interner);
                
                // Compile the function
                sub_compiler.compile_function(params.clone(), body, *is_vararg, expr.line as u32)?;
                
                // Allocate register for the closure
                let closure_reg = self.reg_alloc.allocate();
                
                // Create closure
                self.emit_abx(OpCode::Closure, closure_reg, 0); // Dummy index for now
                
                Ok(closure_reg)
            },
            Expression::Binary { op, left, right } => {
                // Compile operands
                let left_reg = self.compile_expression(left)?;
                let right_reg = self.compile_expression(right)?;
                
                // Allocate register for result
                let result_reg = self.reg_alloc.allocate();
                
                // Emit appropriate instruction
                match op {
                    BinaryOp::Add => { 
                        self.emit(OpCode::Add, result_reg, left_reg.into(), right_reg as u16);
                        result_reg 
                    },
                    BinaryOp::Sub => { 
                        self.emit(OpCode::Sub, result_reg, left_reg.into(), right_reg as u16);
                        result_reg 
                    },
                    BinaryOp::Mul => { 
                        self.emit(OpCode::Mul, result_reg, left_reg.into(), right_reg as u16);
                        result_reg 
                    },
                    BinaryOp::Div => { 
                        self.emit(OpCode::Div, result_reg, left_reg.into(), right_reg as u16);
                        result_reg 
                    },
                    BinaryOp::Mod => { 
                        self.emit(OpCode::Mod, result_reg, left_reg.into(), right_reg as u16);
                        result_reg 
                    },
                    BinaryOp::Pow => { 
                        self.emit(OpCode::Pow, result_reg, left_reg.into(), right_reg as u16);
                        result_reg 
                    },
                    BinaryOp::Concat => {
                        self.emit(OpCode::Concat, result_reg, left_reg.into(), right_reg as u16);
                        result_reg
                    },
                    BinaryOp::Eq => {
                        self.emit(OpCode::Eq, 0, left_reg.into(), right_reg as u16);
                        self.emit_asbx(OpCode::Jmp, 1, 1); // Skip next instruction if equal
                        self.emit(OpCode::LoadBool, result_reg, 0, 1); // false and skip
                        self.emit(OpCode::LoadBool, result_reg, 1, 0); // true
                        result_reg
                    },
                    BinaryOp::Ne => {
                        self.emit(OpCode::Eq, 1, left_reg.into(), right_reg as u16);
                        self.emit_asbx(OpCode::Jmp, 1, 1); // Skip next instruction if not equal
                        self.emit(OpCode::LoadBool, result_reg, 0, 1); // false and skip
                        self.emit(OpCode::LoadBool, result_reg, 1, 0); // true
                        result_reg
                    },
                    BinaryOp::Lt => {
                        self.emit(OpCode::Lt, 0, left_reg.into(), right_reg as u16);
                        self.emit_asbx(OpCode::Jmp, 1, 1); // Skip next instruction if less than
                        self.emit(OpCode::LoadBool, result_reg, 0, 1); // false and skip
                        self.emit(OpCode::LoadBool, result_reg, 1, 0); // true
                        result_reg
                    },
                    BinaryOp::Le => {
                        self.emit(OpCode::Le, 0, left_reg.into(), right_reg as u16);
                        self.emit_asbx(OpCode::Jmp, 1, 1); // Skip next instruction if less or equal
                        self.emit(OpCode::LoadBool, result_reg, 0, 1); // false and skip
                        self.emit(OpCode::LoadBool, result_reg, 1, 0); // true
                        result_reg
                    },
                    BinaryOp::Gt => {
                        self.emit(OpCode::Le, 1, left_reg.into(), right_reg as u16);
                        self.emit_asbx(OpCode::Jmp, 1, 1); // Skip next instruction if greater
                        self.emit(OpCode::LoadBool, result_reg, 0, 1); // false and skip
                        self.emit(OpCode::LoadBool, result_reg, 1, 0); // true
                        result_reg
                    },
                    BinaryOp::Ge => {
                        self.emit(OpCode::Lt, 1, left_reg.into(), right_reg as u16);
                        self.emit_asbx(OpCode::Jmp, 1, 1); // Skip next instruction if greater or equal
                        self.emit(OpCode::LoadBool, result_reg, 0, 1); // false and skip
                        self.emit(OpCode::LoadBool, result_reg, 1, 0); // true
                        result_reg
                    },
                    BinaryOp::And => {
                        self.emit(OpCode::Test, left_reg, 0, 0);
                        let jmp = self.emit_asbx(OpCode::Jmp, 0, 0); // Skip loading B if A is false
                        self.emit(OpCode::Move, result_reg, right_reg.into(), 0); // Load B
                        let end = self.emit_asbx(OpCode::Jmp, 0, 1); // Skip loading A
                        
                        // Patch jump to skip loading B
                        if let Instruction(instr) = self.bytecode[jmp] {
                            let opcode = instr & 0x3F;
                            let a = (instr >> 6) & 0xFF;
                            let sbx = end as i32 - jmp as i32 + 1;
                            self.bytecode[jmp] = Instruction::new_asbx(
                                // Convert raw opcode back to enum
                                match opcode {
                                    22 => OpCode::Jmp,
                                    _ => OpCode::Jmp, // Default in case of error
                                },
                                a as u8,
                                sbx
                            );
                        }
                        
                        self.emit(OpCode::Move, result_reg, left_reg.into(), 0); // Load A
                        result_reg
                    },
                    BinaryOp::Or => {
                        self.emit(OpCode::Test, left_reg, 0, 1);
                        let jmp = self.emit_asbx(OpCode::Jmp, 0, 0); // Skip loading B if A is true
                        self.emit(OpCode::Move, result_reg, right_reg.into(), 0); // Load B
                        let end = self.emit_asbx(OpCode::Jmp, 0, 1); // Skip loading A
                        
                        // Patch jump to skip loading B
                        if let Instruction(instr) = self.bytecode[jmp] {
                            let opcode = instr & 0x3F;
                            let a = (instr >> 6) & 0xFF;
                            let sbx = end as i32 - jmp as i32 + 1;
                            self.bytecode[jmp] = Instruction::new_asbx(
                                // Convert raw opcode back to enum
                                match opcode {
                                    22 => OpCode::Jmp,
                                    _ => OpCode::Jmp, // Default in case of error
                                },
                                a as u8,
                                sbx
                            );
                        }
                        
                        self.emit(OpCode::Move, result_reg, left_reg.into(), 0); // Load A
                        result_reg
                    },
                };
                
                // Free operand registers
                self.reg_alloc.free(left_reg);
                self.reg_alloc.free(right_reg);
                
                Ok(result_reg)
            },
            Expression::Unary { op, operand } => {
                // Compile operand
                let operand_reg = self.compile_expression(operand)?;
                
                // Allocate register for result
                let result_reg = self.reg_alloc.allocate();
                
                // Emit appropriate instruction
                match op {
                    UnaryOp::Minus => {
                        self.emit(OpCode::Unm, result_reg, operand_reg.into(), 0);
                        result_reg
                    },
                    UnaryOp::Not => {
                        self.emit(OpCode::Not, result_reg, operand_reg.into(), 0);
                        result_reg
                    },
                    UnaryOp::Len => {
                        self.emit(OpCode::Len, result_reg, operand_reg.into(), 0);
                        result_reg
                    },
                };
                
                // Free operand register
                self.reg_alloc.free(operand_reg);
                
                Ok(result_reg)
            },
            Expression::Vararg => {
                // Check if we're in a vararg function
                if !self.is_vararg {
                    return Err(LuaError::SyntaxError {
                        message: "cannot use '...' outside a vararg function".to_string(),
                        line: expr.line,
                        column: expr.column,
                    });
                }
                
                // Allocate register for result
                let result_reg = self.reg_alloc.allocate();
                
                // Emit VARARG instruction
                self.emit(OpCode::VarArg, result_reg, 1, 0); // Get single value
                
                Ok(result_reg)
            },
        }
    }
    
    /// Finish compilation and get the function prototype
    pub fn finish(self) -> FunctionProto {
        // Convert constants to Value
        let constants = self.constants.into_iter().map(|c| match c {
            ConstantValue::Nil => super::value::Value::Nil,
            ConstantValue::Boolean(b) => super::value::Value::Boolean(b),
            ConstantValue::Number(n) => super::value::Value::Number(n),
            ConstantValue::String(s) => super::value::Value::String(s),
        }).collect();
        
        FunctionProto {
            bytecode: self.bytecode.into_iter().map(|i| i.0).collect(),
            constants,
            upvalues: self.upvalues,
            param_count: self.params.len(),
            is_vararg: self.is_vararg,
            source: None, // Would be set externally
            line_defined: self.line_defined,
            last_line_defined: self.last_line_defined,
            line_info: self.line_info,
            locals: self.locals,
        }
    }
}

/// The main compiler
pub struct Compiler {
    /// String interner
    interner: StringInterner,
}

impl Compiler {
    /// Create a new compiler
    pub fn new() -> Self {
        Compiler {
            interner: StringInterner::new(),
        }
    }
    
    /// Compile a chunk of Lua code
    pub fn compile(&mut self, source: &str) -> Result<(FunctionProto, Vec<String>)> {
        // Parse the source code
        let mut parser = super::parser::Parser::new(source);
        let statements = parser.parse()?;
        
        // Compile the main chunk
        let mut compiler = FunctionCompiler::new(&mut self.interner);
        compiler.compile_function(Vec::new(), &statements, true, 1)?;
        
        // Finish compilation
        let proto = compiler.finish();
        let strings = self.interner.export_strings();
        
        Ok((proto, strings))
    }
    
    /// Compile a chunk and load it into a heap
    pub fn compile_and_load(&mut self, source: &str, heap: &mut LuaHeap) -> Result<super::value::ClosureHandle> {
        let (proto, strings) = self.compile(source)?;
        
        // Load strings into heap
        let mut string_map = HashMap::new();
        for (i, s) in strings.iter().enumerate() {
            let handle = heap.create_string(s)?;
            string_map.insert(i, handle);
        }
        
        // Create function proto
        // In a real implementation, this would handle nested functions and upvalues
        
        // Create closure
        heap.create_closure(proto, Vec::new())
    }
}

// Unit tests for the compiler
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_register_allocator() {
        let mut alloc = RegisterAllocator::new();
        
        let r1 = alloc.allocate();
        let r2 = alloc.allocate();
        
        assert_eq!(r1, 0);
        assert_eq!(r2, 1);
        
        alloc.free(r1);
        
        let r3 = alloc.allocate();
        
        assert_eq!(r3, 0); // Should reuse the freed register
    }
    
    #[test]
    fn test_instruction() {
        let instr = Instruction::new(OpCode::Move, 1, 2, 3);
        
        assert_eq!(instr.0 & 0x3F, OpCode::Move as u32);
        assert_eq!((instr.0 >> 6) & 0xFF, 1);
        assert_eq!((instr.0 >> 14) & 0x1FF, 2);
        assert_eq!((instr.0 >> 23) & 0x1FF, 3);
    }
    
    #[test]
    fn test_string_interner() {
        let mut interner = StringInterner::new();
        
        let s1 = interner.intern("hello");
        let s2 = interner.intern("world");
        let s3 = interner.intern("hello");
        
        assert_eq!(interner.get(s1), Some("hello"));
        assert_eq!(interner.get(s2), Some("world"));
        assert_eq!(s1, s3); // Same handle for same string
        assert_ne!(s1, s2); // Different handles for different strings
    }
}