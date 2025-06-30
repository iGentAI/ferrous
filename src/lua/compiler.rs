//! Lua Compiler Implementation
//!
//! This module implements a compiler for Lua 5.1 that follows the non-recursive
//! state machine architecture. It converts Lua source code into bytecode for
//! the VM to execute.

use std::collections::HashMap;
use std::marker::PhantomData;

use super::error::{LuaError, Result};
use super::parser;
use super::value::{Value, StringHandle, FunctionProto, UpvalueDesc, LocalVar};

/// A value that can appear in compiled code
#[derive(Debug, Clone, PartialEq)]
pub enum CompilationValue {
    /// nil
    Nil,
    /// boolean
    Boolean(bool),
    /// number
    Number(f64),
    /// string reference (index into string table)
    StringRef(usize),
}

/// A compiled Lua module
#[derive(Debug, Clone)]
pub struct CompiledModule {
    /// Bytecode instructions
    pub bytecode: Vec<u32>,
    /// Constants used by the module
    pub constants: Vec<CompilationValue>,
    /// String table
    pub strings: Vec<String>,
    /// Upvalue information
    pub upvalues: Vec<UpvalueInfo>,
    /// Debug information
    pub debug_info: DebugInfo,
}

/// Upvalue information
#[derive(Debug, Clone)]
pub struct UpvalueInfo {
    /// Name of the upvalue
    pub name: String,
    /// Is it in stack?
    pub in_stack: bool,
    /// Index
    pub index: u8,
}

/// Debug information
#[derive(Debug, Clone, Default)]
pub struct DebugInfo {
    /// Source file name
    pub source: Option<String>,
    /// Line numbers for each instruction
    pub line_numbers: Vec<u32>,
}

/// Lua opcode
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
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
#[derive(Clone, Copy, Debug)]
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
    
    /// Create a new instruction with sBx format
    pub fn new_sbx(opcode: OpCode, a: u8, sbx: i32) -> Self {
        let op = opcode as u32 & 0x3F;
        let a = (a as u32 & 0xFF) << 6;
        // Convert signed sbx to unsigned bx
        let bx = ((sbx + 131071) as u32 & 0x3FFFF) << 14;
        Instruction(op | a | bx)
    }
    
    /// Create a new instruction with Bx format
    pub fn new_bx(opcode: OpCode, a: u8, bx: u32) -> Self {
        let op = opcode as u32 & 0x3F;
        let a = (a as u32 & 0xFF) << 6;
        let bx = (bx & 0x3FFFF) << 14;
        Instruction(op | a | bx)
    }
    
    /// Get the opcode
    pub fn opcode(&self) -> u8 {
        (self.0 & 0x3F) as u8
    }
    
    /// Get operand A
    pub fn a(&self) -> u8 {
        ((self.0 >> 6) & 0xFF) as u8
    }
    
    /// Get operand B
    pub fn b(&self) -> u16 {
        ((self.0 >> 14) & 0x1FF) as u16
    }
    
    /// Get operand C
    pub fn c(&self) -> u16 {
        ((self.0 >> 23) & 0x1FF) as u16
    }
    
    /// Get Bx (B and C combined)
    pub fn bx(&self) -> u32 {
        (self.0 >> 14) & 0x3FFFF
    }
    
    /// Get sBx (signed Bx)
    pub fn sbx(&self) -> i32 {
        (self.bx() as i32) - 131071
    }
    
    /// Check if B is a constant
    pub fn b_is_constant(&self) -> bool {
        (self.b() & 0x100) != 0
    }
    
    /// Check if C is a constant
    pub fn c_is_constant(&self) -> bool {
        (self.c() & 0x100) != 0
    }
    
    /// Get B as constant index
    pub fn b_as_constant(&self) -> usize {
        (self.b() & 0xFF) as usize
    }
    
    /// Get C as constant index
    pub fn c_as_constant(&self) -> usize {
        (self.c() & 0xFF) as usize
    }
}

/// Register allocator
struct RegisterAllocator {
    /// Next free register
    next: u8,
    /// Free registers for reuse
    free: Vec<u8>,
    /// Maximum register used
    max: u8,
}

impl RegisterAllocator {
    /// Create a new register allocator
    fn new() -> Self {
        RegisterAllocator {
            next: 0,
            free: Vec::new(),
            max: 0,
        }
    }
    
    /// Allocate a register
    fn allocate(&mut self) -> u8 {
        if let Some(reg) = self.free.pop() {
            return reg;
        }
        
        let reg = self.next;
        self.next += 1;
        
        if reg > self.max {
            self.max = reg;
        }
        
        reg
    }
    
    /// Free a register
    fn free(&mut self, reg: u8) {
        if reg < self.next {
            self.free.push(reg);
        }
    }
    
    /// Reset the allocator
    fn reset(&mut self) {
        self.next = 0;
        self.free.clear();
        self.max = 0;
    }
    
    /// Get maximum register used
    fn get_max(&self) -> u8 {
        self.max
    }
}

/// String interner
struct StringInterner {
    /// String to index mapping
    strings: HashMap<String, usize>,
    /// All interned strings
    all_strings: Vec<String>,
}

impl StringInterner {
    /// Create a new string interner
    fn new() -> Self {
        StringInterner {
            strings: HashMap::new(),
            all_strings: Vec::new(),
        }
    }
    
    /// Intern a string
    fn intern(&mut self, s: &str) -> usize {
        if let Some(&index) = self.strings.get(s) {
            return index;
        }
        
        let index = self.all_strings.len();
        self.all_strings.push(s.to_string());
        self.strings.insert(s.to_string(), index);
        index
    }
    
    /// Get all strings
    fn get_all_strings(&self) -> Vec<String> {
        self.all_strings.clone()
    }
}

/// Lua compiler
pub struct Compiler {
    /// Bytecode
    bytecode: Vec<u32>,
    /// Constants
    constants: Vec<CompilationValue>,
    /// String interner
    interner: StringInterner,
    /// Register allocator
    regs: RegisterAllocator,
    /// Jump patches to apply
    jumps: Vec<(usize, usize)>, // (from, to)
    /// Current function locals
    locals: Vec<LocalVar>,
}

impl Compiler {
    /// Create a new compiler
    pub fn new() -> Self {
        Compiler {
            bytecode: Vec::new(),
            constants: Vec::new(),
            interner: StringInterner::new(),
            regs: RegisterAllocator::new(),
            jumps: Vec::new(),
            locals: Vec::new(),
        }
    }
    
    /// Reset the compiler for a new function
    fn reset(&mut self) {
        self.bytecode.clear();
        self.constants.clear();
        self.regs.reset();
        self.jumps.clear();
        self.locals.clear();
    }
    
    /// Add a constant
    fn add_constant(&mut self, value: CompilationValue) -> usize {
        // Check if the constant already exists
        for (i, c) in self.constants.iter().enumerate() {
            if *c == value {
                return i;
            }
        }
        
        let index = self.constants.len();
        self.constants.push(value);
        index
    }
    
    /// Emit an instruction
    fn emit(&mut self, opcode: OpCode, a: u8, b: u16, c: u16) -> usize {
        let instr = Instruction::new(opcode, a, b, c);
        let pos = self.bytecode.len();
        self.bytecode.push(instr.0);
        pos
    }
    
    /// Emit an instruction with sBx format
    fn emit_sbx(&mut self, opcode: OpCode, a: u8, sbx: i32) -> usize {
        let instr = Instruction::new_sbx(opcode, a, sbx);
        let pos = self.bytecode.len();
        self.bytecode.push(instr.0);
        pos
    }
    
    /// Emit an instruction with Bx format
    fn emit_bx(&mut self, opcode: OpCode, a: u8, bx: u32) -> usize {
        let instr = Instruction::new_bx(opcode, a, bx);
        let pos = self.bytecode.len();
        self.bytecode.push(instr.0);
        pos
    }
    
    /// Patch a jump instruction
    fn patch_jump(&mut self, from: usize, to: usize) {
        if from >= self.bytecode.len() {
            return; // Invalid index
        }
        
        let instr = Instruction(self.bytecode[from]);
        let opcode = instr.opcode();
        let a = instr.a();
        
        if opcode != OpCode::Jmp as u8 && opcode != OpCode::ForPrep as u8 && opcode != OpCode::ForLoop as u8 {
            return; // Not a jump instruction
        }
        
        // Calculate the offset
        let offset = to as i32 - from as i32 - 1;
        
        // Create new instruction
        let new_instr = Instruction::new_sbx(
            // Convert raw opcode back to enum
            match opcode {
                22 => OpCode::Jmp,
                31 => OpCode::ForLoop,
                32 => OpCode::ForPrep,
                _ => OpCode::Jmp, // Default in case of error
            },
            a,
            offset
        );
        
        // Update bytecode
        self.bytecode[from] = new_instr.0;
    }
    
    /// Compile a Lua source string
    pub fn compile(&mut self, source: &str) -> Result<CompiledModule> {
        // Reset compiler state
        self.reset();
        
        // Parse source into AST
        let mut parser = parser::Parser::new(source);
        let statements = parser.parse()?;
        
        // Compile statements
        for stmt in &statements {
            self.compile_statement(stmt)?;
        }
        
        // Add final return if not present
        // Check if the last statement is a return
        let has_return = if let Some(statement) = statements.last() {
            matches!(statement.value, parser::Statement::Return(_))
        } else {
            false
        };
        
        if !has_return {
            // Add a "return nil"
            self.emit(OpCode::LoadNil, 0, 0, 0);
            self.emit(OpCode::Return, 0, 1, 0);
        }
        
        // Create a copy of jumps to avoid borrow checker issues
        let jumps_copy = self.jumps.clone();
        
        // Apply jump patches
        for (from, to) in jumps_copy {
            self.patch_jump(from, to);
        }
        
        // Create module
        Ok(CompiledModule {
            bytecode: self.bytecode.clone(),
            constants: self.constants.clone(),
            strings: self.interner.get_all_strings(),
            upvalues: Vec::new(), // No upvalues for main chunk
            debug_info: DebugInfo::default(),
        })
    }
    
    /// Compile a statement
    fn compile_statement(&mut self, stmt: &parser::Node<parser::Statement>) -> Result<()> {
        use parser::Statement;
        
        match &stmt.value {
            Statement::Return(exprs) => {
                if exprs.is_empty() {
                    // Return nil
                    self.emit(OpCode::LoadNil, 0, 0, 0);
                    self.emit(OpCode::Return, 0, 1, 0);
                } else if exprs.len() == 1 {
                    // Single return value
                    let reg = self.compile_expr(&exprs[0])?;
                    // Move to register 0 if not already there
                    if reg != 0 {
                        self.emit(OpCode::Move, 0, reg as u16, 0);
                        self.regs.free(reg);
                    }
                    self.emit(OpCode::Return, 0, 2, 0);
                } else {
                    // Multiple return values - not implemented yet
                    let reg = self.compile_expr(&exprs[0])?;
                    // Move to register 0 if not already there
                    if reg != 0 {
                        self.emit(OpCode::Move, 0, reg as u16, 0);
                        self.regs.free(reg);
                    }
                    self.emit(OpCode::Return, 0, 2, 0);
                }
                Ok(())
            },
            
            Statement::Assignment { variables, expressions } => {
                // Not implemented yet - placeholder
                if variables.len() == 1 && expressions.len() == 1 {
                    // Simple case: a = expr
                    use parser::{LValue, Expression};
                    
                    let expr_reg = self.compile_expr(&expressions[0])?;
                    
                    match &variables[0].value {
                        LValue::Name(name) => {
                            // Convert name to a string constant
                            let name_str = format!("{:?}", name); // Not proper but works for stub
                            let name_idx = self.interner.intern(&name_str);
                            let const_idx = self.add_constant(CompilationValue::StringRef(name_idx));
                            
                            // Emit SetGlobal instruction
                            self.emit_bx(OpCode::SetGlobal, expr_reg, const_idx as u32);
                        },
                        _ => {
                            // Not implemented
                        }
                    }
                    
                    self.regs.free(expr_reg);
                }
                Ok(())
            },
            
            Statement::Call(_expr) => {
                // Not implemented yet - placeholder
                Ok(())
            },
            
            _ => {
                // Not implemented yet - placeholder for other statement types
                Ok(())
            }
        }
    }
    
    /// Compile an expression
    fn compile_expr(&mut self, expr: &parser::Node<parser::Expression>) -> Result<u8> {
        use parser::Expression;
        
        match &expr.value {
            Expression::Nil => {
                let reg = self.regs.allocate();
                self.emit(OpCode::LoadNil, reg, reg as u16, 0);
                Ok(reg)
            },
            
            Expression::Boolean(value) => {
                let reg = self.regs.allocate();
                self.emit(OpCode::LoadBool, reg, if *value { 1 } else { 0 }, 0);
                Ok(reg)
            },
            
            Expression::Number(value) => {
                let reg = self.regs.allocate();
                let const_idx = self.add_constant(CompilationValue::Number(*value));
                self.emit_bx(OpCode::LoadK, reg, const_idx as u32);
                Ok(reg)
            },
            
            Expression::String(handle) => {
                let reg = self.regs.allocate();
                // Convert handle to string - this is not proper, just a stub
                let str_value = format!("{:?}", handle);
                let str_idx = self.interner.intern(&str_value);
                let const_idx = self.add_constant(CompilationValue::StringRef(str_idx));
                self.emit_bx(OpCode::LoadK, reg, const_idx as u32);
                Ok(reg)
            },
            
            Expression::Variable(name) => {
                let reg = self.regs.allocate();
                // Convert name to string - this is not proper, just a stub
                let name_str = format!("{:?}", name);
                let name_idx = self.interner.intern(&name_str);
                let const_idx = self.add_constant(CompilationValue::StringRef(name_idx));
                self.emit_bx(OpCode::GetGlobal, reg, const_idx as u32);
                Ok(reg)
            },
            
            Expression::Binary { op, left, right } => {
                use parser::BinaryOp;
                
                let left_reg = self.compile_expr(left)?;
                let right_reg = self.compile_expr(right)?;
                
                // Destination register (reuse left register)
                let dest_reg = left_reg;
                
                // Emit binary operation
                match op {
                    BinaryOp::Add => self.emit(OpCode::Add, dest_reg, left_reg as u16, right_reg as u16),
                    BinaryOp::Sub => self.emit(OpCode::Sub, dest_reg, left_reg as u16, right_reg as u16),
                    BinaryOp::Mul => self.emit(OpCode::Mul, dest_reg, left_reg as u16, right_reg as u16),
                    BinaryOp::Div => self.emit(OpCode::Div, dest_reg, left_reg as u16, right_reg as u16),
                    BinaryOp::Mod => self.emit(OpCode::Mod, dest_reg, left_reg as u16, right_reg as u16),
                    BinaryOp::Pow => self.emit(OpCode::Pow, dest_reg, left_reg as u16, right_reg as u16),
                    // Other binary operations not implemented yet
                    _ => {
                        // Free registers
                        if dest_reg != left_reg {
                            self.regs.free(left_reg);
                        }
                        self.regs.free(right_reg);
                        
                        // Default to adding - just for placeholder
                        self.emit(OpCode::Add, dest_reg, left_reg as u16, right_reg as u16)
                    }
                };
                
                // Free right register (left is reused)
                if right_reg != dest_reg {
                    self.regs.free(right_reg);
                }
                
                Ok(dest_reg)
            },
            
            _ => {
                // Not implemented yet - placeholder for other expression types
                // Return a dummy value of nil
                let reg = self.regs.allocate();
                self.emit(OpCode::LoadNil, reg, reg as u16, 0);
                Ok(reg)
            }
        }
    }
    
    /// Finish compilation and produce the final bytecode
    fn finish(self) -> CompiledModule {
        CompiledModule {
            bytecode: self.bytecode,
            constants: self.constants,
            strings: self.interner.get_all_strings(),
            upvalues: Vec::new(), // No upvalues for main chunk
            debug_info: DebugInfo::default(),
        }
    }
}

/// Compile Lua source code into a module
pub fn compile(source: &str) -> Result<CompiledModule> {
    // Create a compiler and compile the source
    let mut compiler = Compiler::new();
    compiler.compile(source)
}

// Helper function for manual bytecode creation
fn make_instruction(opcode: u8, a: u8, b: u16, c: u16) -> u32 {
    let mut instr = 0u32;
    instr |= (opcode as u32) & 0x3F;
    instr |= ((a as u32) & 0xFF) << 6;
    instr |= ((b as u32) & 0x1FF) << 14;
    instr |= ((c as u32) & 0x1FF) << 23;
    instr
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_compile_nil() {
        let module = compile("return nil").unwrap();
        assert_eq!(module.bytecode.len(), 2);
        
        // First instruction should be LoadNil
        let instr = Instruction(module.bytecode[0]);
        assert_eq!(instr.opcode(), OpCode::LoadNil as u8);
        
        // Second instruction should be Return
        let instr = Instruction(module.bytecode[1]);
        assert_eq!(instr.opcode(), OpCode::Return as u8);
    }
    
    #[test]
    fn test_compile_number() {
        let module = compile("return 42").unwrap();
        assert!(!module.bytecode.is_empty());
        assert_eq!(module.constants.len(), 1);
        
        // First constant should be 42
        if let CompilationValue::Number(n) = &module.constants[0] {
            assert_eq!(*n, 42.0);
        } else {
            panic!("Expected Number constant");
        }
    }
    
    #[test]
    fn test_compile_binary_op() {
        let module = compile("return 5 + 3").unwrap();
        assert!(module.bytecode.len() >= 4); // Need at least 4 instructions
        
        // Constants should include 5 and 3
        assert_eq!(module.constants.len(), 2);
        
        if let CompilationValue::Number(n1) = &module.constants[0] {
            if let CompilationValue::Number(n2) = &module.constants[1] {
                assert!((*n1 == 5.0 && *n2 == 3.0) || (*n1 == 3.0 && *n2 == 5.0));
            } else {
                panic!("Expected two Number constants");
            }
        } else {
            panic!("Expected Number constant");
        }
    }
}