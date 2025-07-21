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
    
    /// Format instruction for debugging
    pub fn format_instruction(&self, idx: usize) -> String {
        let op = self.get_opcode();
        let a = self.get_a();
        let b = self.get_b();
        let c = self.get_c();
        let bx = self.get_bx();
        let sbx = self.get_sbx();
        
        match op {
            OpCode::Move => format!("[{:04}] MOVE      {} {}", idx, a, b),
            OpCode::LoadK => format!("[{:04}] LOADK     {} {}", idx, a, bx),
            OpCode::LoadBool => format!("[{:04}] LOADBOOL  {} {} {}", idx, a, b, c),
            OpCode::LoadNil => format!("[{:04}] LOADNIL   {} {}", idx, a, b),
            OpCode::GetUpval => format!("[{:04}] GETUPVAL  {} {}", idx, a, b),
            OpCode::GetGlobal => format!("[{:04}] GETGLOBAL {} {}", idx, a, bx),
            OpCode::GetTable => format!("[{:04}] GETTABLE  {} {} {}", idx, a, b, c),
            OpCode::SetGlobal => format!("[{:04}] SETGLOBAL {} {}", idx, a, bx),
            OpCode::SetUpval => format!("[{:04}] SETUPVAL  {} {}", idx, a, b),
            OpCode::SetTable => format!("[{:04}] SETTABLE  {} {} {}", idx, a, b, c),
            OpCode::NewTable => format!("[{:04}] NEWTABLE  {} {} {}", idx, a, b, c),
            OpCode::SelfOp => format!("[{:04}] SELF      {} {} {}", idx, a, b, c),
            OpCode::Add => format!("[{:04}] ADD       {} {} {}", idx, a, b, c),
            OpCode::Sub => format!("[{:04}] SUB       {} {} {}", idx, a, b, c),
            OpCode::Mul => format!("[{:04}] MUL       {} {} {}", idx, a, b, c),
            OpCode::Div => format!("[{:04}] DIV       {} {} {}", idx, a, b, c),
            OpCode::Mod => format!("[{:04}] MOD       {} {} {}", idx, a, b, c),
            OpCode::Pow => format!("[{:04}] POW       {} {} {}", idx, a, b, c),
            OpCode::Unm => format!("[{:04}] UNM       {} {}", idx, a, b),
            OpCode::Not => format!("[{:04}] NOT       {} {}", idx, a, b),
            OpCode::Len => format!("[{:04}] LEN       {} {}", idx, a, b),
            OpCode::Concat => format!("[{:04}] CONCAT    {} {} {}", idx, a, b, c),
            OpCode::Jmp => format!("[{:04}] JMP       {} (to {})", idx, sbx, idx as i32 + sbx + 1),
            OpCode::Eq => format!("[{:04}] EQ        {} {} {}", idx, a, b, c),
            OpCode::Lt => format!("[{:04}] LT        {} {} {}", idx, a, b, c),
            OpCode::Le => format!("[{:04}] LE        {} {} {}", idx, a, b, c),
            OpCode::Test => format!("[{:04}] TEST      {} {}", idx, a, c),
            OpCode::TestSet => format!("[{:04}] TESTSET   {} {} {}", idx, a, b, c),
            OpCode::Call => format!("[{:04}] CALL      {} {} {}", idx, a, b, c),
            OpCode::TailCall => format!("[{:04}] TAILCALL  {} {} {}", idx, a, b, c),
            OpCode::Return => format!("[{:04}] RETURN    {} {}", idx, a, b),
            OpCode::ForLoop => format!("[{:04}] FORLOOP   {} {} (to {})", idx, a, sbx, idx as i32 + sbx + 1),
            OpCode::ForPrep => format!("[{:04}] FORPREP   {} {} (to {})", idx, a, sbx, idx as i32 + sbx + 1),
            OpCode::TForLoop => format!("[{:04}] TFORLOOP  {} {}", idx, a, c),
            OpCode::SetList => format!("[{:04}] SETLIST   {} {} {}", idx, a, b, c),
            OpCode::Close => format!("[{:04}] CLOSE     {}", idx, a),
            OpCode::Closure => format!("[{:04}] CLOSURE   {} {}", idx, a, bx),
            OpCode::VarArg => format!("[{:04}] VARARG    {} {}", idx, a, b),
        }
    }
}

/// Represents a local variable in the compilation context
#[derive(Debug, Clone)]
struct LocalVar {
    /// Variable name
    name: String,
    
    /// Register index (base-relative)
    register: u8,
    
    /// Scope level (for nested blocks)
    scope_level: usize,
}

/// Tracks a register's allocation state
#[derive(Debug, Clone, Copy, PartialEq)]
enum RegisterState {
    /// Register is free to use
    Free,
    
    /// Register holds a local variable
    Local,
    
    /// Register holds a temporary value with a specific lifetime
    Temporary { until_pc: usize },
    
    /// Register is reserved for a specific purpose (e.g., loop variables)
    Reserved,
}

/// Enhanced code generation context with proper register tracking
struct CodeGenContext {
    /// Current function being compiled
    current_function: CompiledFunction,
    
    /// Local variables with their metadata
    locals: Vec<LocalVar>,
    
    /// Register allocation states (indexed by register number)
    register_states: Vec<RegisterState>,
    
    /// Next free register for general allocation
    next_free_register: u8,
    
    /// Current scope level
    scope_level: usize,
    
    /// String table (global)
    strings: Vec<String>,
    
    /// Parent context (for nested functions)
    parent: Option<Box<CodeGenContext>>,
    
    /// Break jump targets (for loop control)
    break_targets: Vec<usize>,
    
    /// Continue jump targets (for loop control)
    continue_targets: Vec<usize>,

    /// Upvalue names tracking (for debugging and resolution)
    upvalue_names: Vec<String>,
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
            locals: Vec::new(),
            register_states: vec![RegisterState::Free; 256],
            next_free_register: 0,
            scope_level: 0,
            strings: Vec::new(),
            parent: None,
            break_targets: Vec::new(),
            continue_targets: Vec::new(),
            upvalue_names: Vec::new(),
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
            locals: Vec::new(),
            register_states: vec![RegisterState::Free; 256],
            next_free_register: 0,
            scope_level: 0,
            strings: Vec::new(), // Will be moved to parent later
            parent: None, // Will be set properly later
            break_targets: Vec::new(),
            continue_targets: Vec::new(),
            upvalue_names: Vec::new(),
        }
    }
    
    /// Enter a new scope
    fn enter_scope(&mut self) {
        self.scope_level += 1;
    }
    
    /// Exit a scope, freeing local variables
    fn exit_scope(&mut self) {
        self.scope_level -= 1;
        
        // Check if we need to emit CLOSE before removing locals
        if let Some(min_register) = self.has_locals_needing_close(self.scope_level + 1) {
            // Emit CLOSE to close any upvalues pointing to locals we're about to remove
            eprintln!("DEBUG exit_scope: Emitting CLOSE for register {} on scope exit", min_register);
            self.emit(Instruction::create_ABC(OpCode::Close, min_register as u32, 0, 0));
        }
        
        // Remove locals from the exited scope and free their registers
        let mut i = 0;
        while i < self.locals.len() {
            if self.locals[i].scope_level > self.scope_level {
                let reg = self.locals[i].register;
                self.register_states[reg as usize] = RegisterState::Free;
                self.locals.remove(i);
                
                // Update next_free_register if we freed a lower register
                if reg < self.next_free_register {
                    self.next_free_register = reg;
                }
            } else {
                i += 1;
            }
        }
        
        // Recalculate next_free_register
        self.recalculate_next_free_register();
    }
    
    /// Recalculate the next free register
    fn recalculate_next_free_register(&mut self) {
        for i in 0..256 {
            if self.register_states[i] == RegisterState::Free {
                self.next_free_register = i as u8;
                return;
            }
        }
        self.next_free_register = 255; // All registers in use
    }
    
    /// Allocate a register for a local variable
    fn allocate_local_register(&mut self, name: &str) -> LuaResult<u8> {
        let reg = self.allocate_register()?;
        
        self.register_states[reg as usize] = RegisterState::Local;
        self.locals.push(LocalVar {
            name: name.to_string(),
            register: reg,
            scope_level: self.scope_level,
        });
        
        Ok(reg)
    }
    
    /// Allocate a temporary register
    fn allocate_register(&mut self) -> LuaResult<u8> {
        // Find the first free register
        for i in self.next_free_register as usize..250 {
            if self.register_states[i] == RegisterState::Free {
                self.register_states[i] = RegisterState::Temporary { until_pc: self.current_pc() + 1 };
                self.next_free_register = (i + 1) as u8;
                
                // Update max stack size
                if (i + 1) as u8 > self.current_function.max_stack_size {
                    self.current_function.max_stack_size = (i + 1) as u8;
                }
                
                eprintln!("DEBUG allocate_register: Allocated temporary register {} (until_pc: {})",
                         i, self.current_pc() + 1);
                
                return Ok(i as u8);
            }
        }
        
        Err(LuaError::CompileError("Too many registers in use".to_string()))
    }
    
    /// Allocate a specific register (for special cases like function parameters)
    fn allocate_specific_register(&mut self, reg: u8) -> LuaResult<()> {
        if reg >= 250 {
            return Err(LuaError::CompileError("Register index too high".to_string()));
        }
        
        if self.register_states[reg as usize] != RegisterState::Free {
            return Err(LuaError::CompileError(format!("Register {} already in use", reg)));
        }
        
        self.register_states[reg as usize] = RegisterState::Local;
        
        // Update max stack size
        if reg + 1 > self.current_function.max_stack_size {
            self.current_function.max_stack_size = reg + 1;
        }
        
        // Update next_free_register if necessary
        if reg >= self.next_free_register {
            self.next_free_register = reg + 1;
        }
        
        Ok(())
    }
    
    /// Reserve a consecutive range of registers
    fn reserve_consecutive_registers(&mut self, count: u8) -> LuaResult<u8> {
        // Find consecutive free registers
        let mut start = None;
        
        for i in 0..=250 - count {
            let mut all_free = true;
            for j in 0..count {
                if self.register_states[(i + j) as usize] != RegisterState::Free {
                    all_free = false;
                    break;
                }
            }
            
            if all_free {
                start = Some(i);
                break;
            }
        }
        
        let start_reg = start.ok_or_else(|| {
            eprintln!("DEBUG reserve_consecutive_registers: Failed to find {} consecutive free registers", count);
            LuaError::CompileError(format!("Cannot allocate {} consecutive registers", count))
        })?;
        
        eprintln!("DEBUG reserve_consecutive_registers: Reserved {} consecutive registers starting at R({})",
                 count, start_reg);
        
        // Reserve the registers
        for i in 0..count {
            self.register_states[(start_reg + i) as usize] = RegisterState::Reserved;
        }
        
        // Update max stack size
        if start_reg + count > self.current_function.max_stack_size {
            self.current_function.max_stack_size = start_reg + count;
        }
        
        // Update next_free_register
        if start_reg + count > self.next_free_register {
            self.next_free_register = start_reg + count;
        }
        
        Ok(start_reg)
    }
    
    /// Free a register
    fn free_register(&mut self, reg: u8) {
        if reg < 250 {
            self.register_states[reg as usize] = RegisterState::Free;
            if reg < self.next_free_register {
                self.next_free_register = reg;
            }
        }
    }
    
    /// Free all temporary registers up to the current PC
    fn free_temporaries(&mut self) {
        let current_pc = self.current_pc();
        
        for i in 0..256 {
            if let RegisterState::Temporary { until_pc } = self.register_states[i] {
                if until_pc <= current_pc {
                    self.register_states[i] = RegisterState::Free;
                    if i < self.next_free_register as usize {
                        self.next_free_register = i as u8;
                    }
                }
            }
        }
    }
    
    /// Check if there are locals that might need upvalue closing
    fn has_locals_needing_close(&self, from_scope: usize) -> Option<u8> {
        // Find the lowest register among locals that would be removed
        let mut min_register = None;
        
        // Consider a local potentially captured if it's in a scope that's about to be exited
        for local in &self.locals {
            if local.scope_level >= from_scope {
                match min_register {
                    None => min_register = Some(local.register),
                    Some(current) if local.register < current => min_register = Some(local.register),
                    _ => {}
                }
            }
        }
        
        // If we have any locals from this scope or deeper, assume they might be captured
        // This is a conservative approach that ensures we don't miss any potential upvalues
        if min_register.is_some() {
            eprintln!("DEBUG has_locals_needing_close: Found locals potentially captured as upvalues at scope level {} or higher", from_scope);
        }
        
        min_register
    }
    
    /// Look up a local variable by name
    fn lookup_local(&self, name: &str) -> Option<u8> {
        // Search from most recent to oldest (for shadowing)
        for local in self.locals.iter().rev() {
            if local.name == name {
                return Some(local.register);
            }
        }
        None
    }
    
    /// Emit an instruction
    fn emit(&mut self, instruction: Instruction) {
        self.current_function.bytecode.push(instruction.0);
        // Free temporaries after each instruction
        self.free_temporaries();
    }
    
    /// Get current PC
    fn current_pc(&self) -> usize {
        self.current_function.bytecode.len()
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
    
    /// Check if we're currently in a statement context (vs expression context)
    /// This affects how many results we expect from function calls
    fn in_statement_context(&self) -> bool {
        // If the register allocation is at the beginning of a free block,
        // we're likely in a statement context where the result isn't used
        self.next_free_register <= 1
    }
}

/// Resolve an upvalue through parent contexts with proper base-relative tracking
fn resolve_upvalue(ctx: &mut CodeGenContext, name: &str, parent: Option<&CodeGenContext>) -> Option<u8> {
    // If we have no parent, we can't have upvalues
    let parent_ctx = parent?;
    
    eprintln!("DEBUG resolve_upvalue: Looking for '{}' in parent context", name);
    
    // First, check if the parent has this as a local
    if let Some(parent_local_reg) = parent_ctx.lookup_local(name) {
        eprintln!("DEBUG resolve_upvalue: Found '{}' as local at parent register {}", 
                 name, parent_local_reg);
        
        // Check if we already have this upvalue
        for (i, upval_name) in ctx.upvalue_names.iter().enumerate() {
            if upval_name == name && ctx.current_function.upvalues[i].in_stack 
                && ctx.current_function.upvalues[i].index == parent_local_reg {
                eprintln!("DEBUG resolve_upvalue: Already have upvalue {} for '{}'", i, name);
                return Some(i as u8);
            }
        }
        
        // Create a new upvalue
        let upval_idx = ctx.current_function.upvalues.len() as u8;
        eprintln!("DEBUG resolve_upvalue: Creating new upvalue {} for '{}', in_stack=true, index={}",
                 upval_idx, name, parent_local_reg);
        
        ctx.current_function.upvalues.push(CompilationUpvalue {
            in_stack: true,
            index: parent_local_reg,
        });
        ctx.upvalue_names.push(name.to_string());
        
        return Some(upval_idx);
    }
    
    // If the parent doesn't have it as a local, check if parent has it as an upvalue
    for (parent_upval_idx, parent_upval_name) in parent_ctx.upvalue_names.iter().enumerate() {
        if parent_upval_name == name {
            eprintln!("DEBUG resolve_upvalue: Found '{}' as upvalue {} in parent", 
                    name, parent_upval_idx);
            
            // Check if we already have this upvalue
            for (i, upval_name) in ctx.upvalue_names.iter().enumerate() {
                if upval_name == name && !ctx.current_function.upvalues[i].in_stack 
                    && ctx.current_function.upvalues[i].index == parent_upval_idx as u8 {
                    eprintln!("DEBUG resolve_upvalue: Already have upvalue {} for '{}'", i, name);
                    return Some(i as u8);
                }
            }
            
            // Create a new upvalue referencing parent's upvalue
            let upval_idx = ctx.current_function.upvalues.len() as u8;
            eprintln!("DEBUG resolve_upvalue: Creating new upvalue {} for '{}', in_stack=false, index={}",
                     upval_idx, name, parent_upval_idx);
            
            ctx.current_function.upvalues.push(CompilationUpvalue {
                in_stack: false,
                index: parent_upval_idx as u8,
            });
            ctx.upvalue_names.push(name.to_string());
            
            return Some(upval_idx);
        }
    }
    
    eprintln!("DEBUG resolve_upvalue: Could not find '{}' in parent context", name);
    None
}

/// Generate bytecode from AST
pub fn generate_bytecode(chunk: &Chunk) -> LuaResult<CompleteCompilationOutput> {
    let mut ctx = CodeGenContext::new();
    
    // Compile the chunk
    compile_chunk(&mut ctx, chunk)?;
    
    // Add implicit return if needed
    let has_explicit_return = chunk.return_statement.is_some() || ends_with_return(&chunk.statements);
    
    if !has_explicit_return {
        // Check if we need to emit CLOSE before implicit return
        if let Some(min_reg) = ctx.has_locals_needing_close(0) {
            ctx.emit(Instruction::create_ABC(OpCode::Close, min_reg as u32, 0, 0));
        }
        
        // Add a default "return" with no values
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
    
    eprintln!("DEBUG compile_local_declaration: {} names, {} expressions", 
             num_names, num_exprs);
    
    // Compile all expressions to temporary registers first
    let mut expr_registers = Vec::with_capacity(num_exprs);
    
    if num_exprs > 0 {
        for (i, expr) in decl.expressions.iter().enumerate() {
            let expr_reg = ctx.allocate_register()?;
            eprintln!("DEBUG compile_local_declaration: Compiling expr {} to temp register {}", i, expr_reg);
            compile_expression_to_register_with_parent(ctx, expr, expr_reg, parent)?;
            expr_registers.push(expr_reg);
        }
    }
    
    // Now allocate registers for locals
    let mut local_registers = Vec::with_capacity(num_names);
    
    for (i, name) in decl.names.iter().enumerate() {
        let local_reg = ctx.allocate_local_register(name)?;
        eprintln!("DEBUG compile_local_declaration: Allocated local '{}' at register {}", name, local_reg);
        local_registers.push(local_reg);
        
        // Move expression result to local register if available
        if i < expr_registers.len() {
            if expr_registers[i] != local_reg {
                ctx.emit(Instruction::create_ABC(OpCode::Move, local_reg as u32, expr_registers[i] as u32, 0));
            }
        }
    }
    
    // Initialize remaining locals to nil
    if num_exprs < num_names {
        let start = local_registers[num_exprs];
        let end = local_registers[num_names - 1];
        eprintln!("DEBUG compile_local_declaration: Initializing registers {}..{} to nil", start, end);
        ctx.emit(Instruction::create_ABC(OpCode::LoadNil, start as u32, end as u32, 0));
    }
    
    // Free temporary expression registers
    for reg in expr_registers {
        if !local_registers.contains(&reg) {
            ctx.free_register(reg);
        }
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
    // Handle multiple assignment (a, b, c = x, y, z)
    if assign.variables.len() > 1 {
        // Step 1: Evaluate all expressions first into temporary registers
        let mut expr_registers = Vec::with_capacity(assign.expressions.len());
        for (i, expr) in assign.expressions.iter().enumerate() {
            let expr_reg = ctx.allocate_register()?;
            compile_expression_to_register_with_parent(ctx, expr, expr_reg, parent)?;
            expr_registers.push(expr_reg);
            
            // Special handling for function calls in multi-assignment
            if i == assign.expressions.len() - 1 && matches!(expr, Expression::FunctionCall(_)) {
                // Last expression is a function call, which may return multiple values
                // Modify the CALL instruction to return enough values
                let frame = ctx.current_function.bytecode.len() - 1;
                if frame > 0 {
                    let instr = Instruction(ctx.current_function.bytecode[frame]);
                    if instr.get_opcode() == OpCode::Call {
                        // For multiple assignment, need more results
                        // Adjust C field: # of variables - expressions already covered + 1
                        let needed = assign.variables.len() - assign.expressions.len() + 1;
                        let nargs = instr.get_b();
                        let new_instr = Instruction::create_ABC(
                            OpCode::Call, 
                            instr.get_a(),
                            nargs,
                            (needed as u32 + 1)  // +1 because C is 1-based
                        );
                        ctx.current_function.bytecode[frame] = new_instr.0;
                    }
                }
            }
        }
        
        // Step 2: Assign to each variable
        for (i, var) in assign.variables.iter().enumerate() {
            match var {
                Variable::Name(name) => {
                    // Handle local/upvalue/global cases for each name
                    if let Some(reg) = ctx.lookup_local(name) {
                        // Local variable
                        if i < expr_registers.len() {
                            // Direct expression available
                            ctx.emit(Instruction::create_ABC(OpCode::Move, reg as u32, expr_registers[i] as u32, 0));
                        } else if !expr_registers.is_empty() {
                            // More variables than expressions, must be from a function call
                            // Results start at expr_registers[0] + i
                            let result_reg = expr_registers[0] + i as u8;
                            ctx.emit(Instruction::create_ABC(OpCode::Move, reg as u32, result_reg as u32, 0));
                        } else {
                            // No expressions, fill with nil
                            ctx.emit(Instruction::create_ABC(OpCode::LoadNil, reg as u32, reg as u32, 0));
                        }
                    } else {
                        // Could be upvalue or global - implement similar logic...
                        return Err(LuaError::NotImplemented("Multi-assignment for upvalue/global not implemented".to_string()));
                    }
                },
                // Handle table member and index cases...
                _ => {
                    return Err(LuaError::NotImplemented("Complex multi-assignment not implemented".to_string()));
                }
            }
        }
        
        // Free temporary expression registers
        for reg in expr_registers {
            ctx.free_register(reg);
        }
        
        return Ok(());
    }
    
    // For simple single assignment cases
    if assign.variables.len() == 1 && assign.expressions.len() == 1 {
        match &assign.variables[0] {
            Variable::Name(name) => {
                eprintln!("DEBUG compile_assignment: Assigning to '{}'", name);
                
                if let Some(reg) = ctx.lookup_local(name) {
                    // Assign to existing local
                    eprintln!("DEBUG compile_assignment: '{}' is local at register {}", name, reg);
                    compile_expression_to_register_with_parent(ctx, &assign.expressions[0], reg, parent)?;
                    return Ok(());
                } else if let Some(upval_idx) = resolve_upvalue(ctx, name, parent) {
                    // Assign to upvalue using SETUPVAL
                    eprintln!("DEBUG compile_assignment: '{}' is upvalue {}", name, upval_idx);
                    
                    let value_reg = ctx.allocate_register()?;
                    compile_expression_to_register_with_parent(ctx, &assign.expressions[0], value_reg, parent)?;
                    
                    ctx.emit(Instruction::create_ABC(OpCode::SetUpval, upval_idx as u32, value_reg as u32, 0));
                    ctx.free_register(value_reg);
                    return Ok(());
                } else {
                    // Global assignment
                    eprintln!("DEBUG compile_assignment: '{}' is global", name);
                    
                    let value_reg = ctx.allocate_register()?;
                    compile_expression_to_register_with_parent(ctx, &assign.expressions[0], value_reg, parent)?;
                    
                    let string_idx = ctx.add_string(name);
                    let const_idx = ctx.add_constant(CompilationConstant::String(string_idx))?;
                    
                    ctx.emit(Instruction::create_ABx(OpCode::SetGlobal, value_reg as u32, const_idx));
                    ctx.free_register(value_reg);
                    return Ok(());
                }
            }
            Variable::Member { table, field } => {
                // Table member assignment
                let table_reg = ctx.allocate_register()?;
                compile_expression_to_register_with_parent(ctx, table, table_reg, parent)?;
                
                let value_reg = ctx.allocate_register()?;
                compile_expression_to_register_with_parent(ctx, &assign.expressions[0], value_reg, parent)?;
                
                let field_idx = ctx.add_string(field);
                let field_const = ctx.add_constant(CompilationConstant::String(field_idx))?;
                
                ctx.emit(Instruction::create_ABC(
                    OpCode::SetTable,
                    table_reg as u32,
                    Instruction::encode_constant(field_const),
                    value_reg as u32
                ));
                
                ctx.free_register(table_reg);
                ctx.free_register(value_reg);
                return Ok(());
            }
            Variable::Index { table, key } => {
                // Table index assignment
                let table_reg = ctx.allocate_register()?;
                compile_expression_to_register_with_parent(ctx, table, table_reg, parent)?;
                
                let key_reg = ctx.allocate_register()?;
                compile_expression_to_register_with_parent(ctx, key, key_reg, parent)?;
                
                let value_reg = ctx.allocate_register()?;
                compile_expression_to_register_with_parent(ctx, &assign.expressions[0], value_reg, parent)?;
                
                ctx.emit(Instruction::create_ABC(
                    OpCode::SetTable,
                    table_reg as u32,
                    key_reg as u32,
                    value_reg as u32
                ));
                
                ctx.free_register(table_reg);
                ctx.free_register(key_reg);
                ctx.free_register(value_reg);
                return Ok(());
            }
        }
    }
    
    // Complex assignment cases go here...
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
            ctx.free_register(base);
        }
        Expression::TableConstructor(tc) => {
            compile_table_constructor_with_parent(ctx, tc, target, parent)?;
        }
        Expression::FunctionDef { parameters, is_vararg, body } => {
            compile_function_expression_with_parent(ctx, parameters, *is_vararg, body, target)?;
        }
        Expression::VarArg => {
            // Make VARARG context-sensitive
            // B=0 means return all varargs
            // B>0 means return a specific number of values
            
            // Determine if we're in a statement context or how many values are needed
            let required_values = if ctx.in_statement_context() {
                0  // Don't need any values in statement context
            } else {
                // In expression context, default to returning just one value
                1
            };
            
            // B = required_values + 1, or 0 for all values
            let b_value = if required_values > 0 {
                required_values + 1
            } else {
                0  // Return all varargs
            };
            
            ctx.emit(Instruction::create_ABC(OpCode::VarArg, target as u32, b_value, 0));
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
    eprintln!("DEBUG: compile_table_constructor - target: {}", target);
    
    // Count array and hash parts for size hints
    let mut array_count = 0;
    let mut hash_count = 0;
    
    for field in &tc.fields {
        match field {
            TableField::List(_) => array_count += 1,
            _ => hash_count += 1,
        }
    }
    
    // Create the table
    ctx.emit(Instruction::create_ABC(OpCode::NewTable, target as u32, 0, 0));
    
    // Separate array fields and hash fields
    let mut array_fields = Vec::new();
    
    // First pass: collect array fields and process hash fields
    for field in &tc.fields {
        match field {
            TableField::List(expr) => {
                array_fields.push(expr);
            },
            
            TableField::Record { key, value } => {
                // Record-style entry
                let value_reg = ctx.allocate_register()?;
                compile_expression_to_register_with_parent(ctx, value, value_reg, parent)?;
                
                let key_string_idx = ctx.add_string(key);
                let key_const = ctx.add_constant(CompilationConstant::String(key_string_idx))?;
                
                ctx.emit(Instruction::create_ABC(
                    OpCode::SetTable,
                    target as u32,
                    Instruction::encode_constant(key_const),
                    value_reg as u32
                ));
                
                ctx.free_register(value_reg);
            },
            
            TableField::Index { key, value } => {
                // Computed index entry
                let key_reg = ctx.allocate_register()?;
                compile_expression_to_register_with_parent(ctx, key, key_reg, parent)?;
                
                let value_reg = ctx.allocate_register()?;
                compile_expression_to_register_with_parent(ctx, value, value_reg, parent)?;
                
                ctx.emit(Instruction::create_ABC(
                    OpCode::SetTable,
                    target as u32,
                    key_reg as u32,
                    value_reg as u32
                ));
                
                ctx.free_register(key_reg);
                ctx.free_register(value_reg);
            }
        }
    }
    
    // Process array fields using SETLIST
    if !array_fields.is_empty() {
        const FIELDS_PER_FLUSH: usize = 50;
        
        for (batch_idx, batch) in array_fields.chunks(FIELDS_PER_FLUSH).enumerate() {
            // SETLIST requires values to be in consecutive registers starting at table + 1
            let batch_size = batch.len() as u8;
            
            // Check if we can allocate consecutive registers starting at target + 1
            let mut can_allocate_after_target = true;
            for i in 0..batch_size {
                let reg = target + 1 + i;
                if reg >= 250 || ctx.register_states[reg as usize] != RegisterState::Free {
                    can_allocate_after_target = false;
                    break;
                }
            }
            
            if can_allocate_after_target {
                // Allocate consecutive registers starting at target + 1
                let values_start = ctx.reserve_consecutive_registers(batch_size)?;
                
                if values_start != target + 1 {
                    // If we didn't get registers starting at target+1, move the table to temp location
                    let temp_base = values_start - 1;  // One register before our consecutive block
                    ctx.emit(Instruction::create_ABC(OpCode::Move, temp_base as u32, target as u32, 0));
                    
                    // Compile values to consecutive registers
                    for (i, expr) in batch.iter().enumerate() {
                        let dest_reg = values_start + i as u8;
                        compile_expression_to_register_with_parent(ctx, expr, dest_reg, parent)?;
                    }
                    
                    // Emit SETLIST with proper handling of batch numbers > 255
                    let c = batch_idx + 1;
                    if c > 255 {
                        // For batch numbers > 255, set C=0 and emit the batch number as the next instruction
                        ctx.emit(Instruction::create_ABC(
                            OpCode::SetList,
                            temp_base as u32,
                            batch.len() as u32,
                            0  // C=0 signals that next instruction contains the real C
                        ));
                        // Next instruction is the batch number (C value)
                        ctx.emit(Instruction(c as u32));
                    } else {
                        // Normal case, batch number fits in C field
                        ctx.emit(Instruction::create_ABC(
                            OpCode::SetList,
                            temp_base as u32,
                            batch.len() as u32,
                            c as u32
                        ));
                    }
                    
                    // Move table back to target
                    ctx.emit(Instruction::create_ABC(OpCode::Move, target as u32, temp_base as u32, 0));
                    
                    // Free registers
                    for i in 0..batch_size {
                        ctx.free_register(values_start + i);
                    }
                    ctx.free_register(temp_base);
                } else {
                    // Compile values to consecutive registers
                    for (i, expr) in batch.iter().enumerate() {
                        let dest_reg = target + 1 + i as u8;
                        compile_expression_to_register_with_parent(ctx, expr, dest_reg, parent)?;
                    }
                    
                    // Emit SETLIST with proper handling of batch numbers > 255
                    let c = batch_idx + 1;
                    if c > 255 {
                        // For batch numbers > 255, set C=0 and emit the batch number as the next instruction
                        ctx.emit(Instruction::create_ABC(
                            OpCode::SetList,
                            target as u32,
                            batch.len() as u32,
                            0  // C=0 signals that next instruction contains the real C
                        ));
                        // Next instruction is the batch number (C value)
                        ctx.emit(Instruction(c as u32));
                    } else {
                        // Normal case, batch number fits in C field
                        ctx.emit(Instruction::create_ABC(
                            OpCode::SetList,
                            target as u32,
                            batch.len() as u32,
                            c as u32
                        ));
                    }
                    
                    // Free the value registers
                    for i in 0..batch_size {
                        ctx.free_register(target + 1 + i);
                    }
                }
            } else {
                // Need to use a temporary location for the table and values
                let temp_base = ctx.reserve_consecutive_registers(batch_size + 1)?;
                
                // Move table to temporary location
                ctx.emit(Instruction::create_ABC(OpCode::Move, temp_base as u32, target as u32, 0));
                
                // Compile values to consecutive registers after temp table
                for (i, expr) in batch.iter().enumerate() {
                    let dest_reg = temp_base + 1 + i as u8;
                    compile_expression_to_register_with_parent(ctx, expr, dest_reg, parent)?;
                }
                
                // Emit SETLIST with proper handling of batch numbers > 255
                let c = batch_idx + 1;
                if c > 255 {
                    // For batch numbers > 255, set C=0 and emit the batch number as the next instruction
                    ctx.emit(Instruction::create_ABC(
                        OpCode::SetList,
                        temp_base as u32,
                        batch.len() as u32,
                        0  // C=0 signals that next instruction contains the real C
                    ));
                    // Next instruction is the batch number (C value)
                    ctx.emit(Instruction(c as u32));
                } else {
                    // Normal case, batch number fits in C field
                    ctx.emit(Instruction::create_ABC(
                        OpCode::SetList,
                        temp_base as u32,
                        batch.len() as u32,
                        c as u32
                    ));
                }
                
                // Move table back to target
                ctx.emit(Instruction::create_ABC(OpCode::Move, target as u32, temp_base as u32, 0));
                
                // Free all temporary registers
                for i in 0..(batch_size + 1) {
                    ctx.free_register(temp_base + i);
                }
            }
        }
    }
    
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
                eprintln!("DEBUG compile_variable: '{}' is local at register {}", name, reg);
                if reg != target {
                    ctx.emit(Instruction::create_ABC(OpCode::Move, target as u32, reg as u32, 0));
                }
            } else if let Some(upval_idx) = resolve_upvalue(ctx, name, parent) {
                eprintln!("DEBUG compile_variable: '{}' is upvalue {}", name, upval_idx);
                ctx.emit(Instruction::create_ABC(OpCode::GetUpval, target as u32, upval_idx as u32, 0));
            } else {
                eprintln!("DEBUG compile_variable: '{}' is global", name);
                let string_idx = ctx.add_string(name);
                let const_idx = ctx.add_constant(CompilationConstant::String(string_idx))?;
                ctx.emit(Instruction::create_ABx(OpCode::GetGlobal, target as u32, const_idx));
            }
        }
        Variable::Member { table, field } => {
            let table_reg = ctx.allocate_register()?;
            compile_expression_to_register_with_parent(ctx, table, table_reg, parent)?;
            
            let field_idx = ctx.add_string(field);
            let field_const = ctx.add_constant(CompilationConstant::String(field_idx))?;
            
            ctx.emit(Instruction::create_ABC(
                OpCode::GetTable,
                target as u32,
                table_reg as u32,
                Instruction::encode_constant(field_const)
            ));
            
            ctx.free_register(table_reg);
        }
        Variable::Index { table, key } => {
            let table_reg = ctx.allocate_register()?;
            compile_expression_to_register_with_parent(ctx, table, table_reg, parent)?;
            
            let key_reg = ctx.allocate_register()?;
            compile_expression_to_register_with_parent(ctx, key, key_reg, parent)?;
            
            ctx.emit(Instruction::create_ABC(
                OpCode::GetTable,
                target as u32,
                table_reg as u32,
                key_reg as u32
            ));
            
            ctx.free_register(table_reg);
            ctx.free_register(key_reg);
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
            
            // Allocate temporary registers for operands
            let left_reg = ctx.allocate_register()?;
            compile_expression_to_register_with_parent(ctx, left, left_reg, parent)?;
            
            let right_reg = ctx.allocate_register()?;
            compile_expression_to_register_with_parent(ctx, right, right_reg, parent)?;
            
            ctx.emit(Instruction::create_ABC(opcode, target as u32, left_reg as u32, right_reg as u32));
            
            ctx.free_register(left_reg);
            ctx.free_register(right_reg);
        }
        
        BinaryOperator::Concat => {
            // CONCAT requires consecutive registers
            // Reserve 2 consecutive registers upfront before any evaluation
            let concat_base = ctx.reserve_consecutive_registers(2)?;
            
            // Compile left and right operands to these reserved registers
            compile_expression_to_register_with_parent(ctx, left, concat_base, parent)?;
            compile_expression_to_register_with_parent(ctx, right, concat_base + 1, parent)?;
            
            // Emit CONCAT instruction
            ctx.emit(Instruction::create_ABC(OpCode::Concat, target as u32, concat_base as u32, (concat_base + 1) as u32));
            
            // If target is different from the base, move the result
            if target != concat_base {
                ctx.emit(Instruction::create_ABC(OpCode::Move, target as u32, concat_base as u32, 0));
            }
            
            // Free the reserved registers
            ctx.free_register(concat_base);
            ctx.free_register(concat_base + 1);
        }
        
        BinaryOperator::Or | BinaryOperator::And => {
            compile_expression_to_register_with_parent(ctx, left, target, parent)?;
            
            let testset_a = if matches!(op, BinaryOperator::Or) { 1 } else { 0 };
            ctx.emit(Instruction::create_ABC(OpCode::TestSet, target as u32, target as u32, testset_a));
            
            let jmp_pc = ctx.current_pc();
            ctx.emit(Instruction::create_AsBx(OpCode::Jmp, 0, 0));
            
            compile_expression_to_register_with_parent(ctx, right, target, parent)?;
            
            let jmp_offset = (ctx.current_pc() - jmp_pc - 1) as i32;
            ctx.current_function.bytecode[jmp_pc] = Instruction::create_AsBx(OpCode::Jmp, 0, jmp_offset).0;
        }
        
        BinaryOperator::Eq | BinaryOperator::Ne | BinaryOperator::Lt | 
        BinaryOperator::Le | BinaryOperator::Gt | BinaryOperator::Ge => {
            let left_reg = ctx.allocate_register()?;
            compile_expression_to_register_with_parent(ctx, left, left_reg, parent)?;
            
            let right_reg = ctx.allocate_register()?;
            compile_expression_to_register_with_parent(ctx, right, right_reg, parent)?;
            
            let opcode = match op {
                BinaryOperator::Eq | BinaryOperator::Ne => OpCode::Eq,
                BinaryOperator::Lt | BinaryOperator::Gt => OpCode::Lt,
                BinaryOperator::Le | BinaryOperator::Ge => OpCode::Le,
                _ => unreachable!(),
            };
            
            let (left_op, right_op) = match op {
                BinaryOperator::Gt | BinaryOperator::Ge => (right_reg as u32, left_reg as u32),
                _ => (left_reg as u32, right_reg as u32),
            };
            
            let a_flag = match op {
                BinaryOperator::Ne | BinaryOperator::Ge | BinaryOperator::Gt => 1,
                _ => 0,
            };
            
            // Emit comparison that skips next instruction if comparison is true
            ctx.emit(Instruction::create_ABC(opcode, a_flag, left_op, right_op));
            
            // If comparison is false, load false and skip over the true load
            ctx.emit(Instruction::create_ABC(OpCode::LoadBool, target as u32, 0, 1));
            
            // If comparison is true, we execute this to load true
            ctx.emit(Instruction::create_ABC(OpCode::LoadBool, target as u32, 1, 0));
            
            ctx.free_register(left_reg);
            ctx.free_register(right_reg);
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
    
    // Allocate temporary register for operand
    let operand_reg = ctx.allocate_register()?;
    compile_expression_to_register_with_parent(ctx, operand, operand_reg, parent)?;
    
    ctx.emit(Instruction::create_ABC(opcode, target as u32, operand_reg as u32, 0));
    
    ctx.free_register(operand_reg);
    
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
    // Check if this is a method call
    if let Some(method_name) = &call.method {
        // Method call - reserve consecutive registers for function and self
        let func_reg = ctx.reserve_consecutive_registers(2)?;
        let self_reg = func_reg + 1;
        
        // Compile the table expression
        compile_expression_to_register_with_parent(ctx, &call.function, func_reg, parent)?;
        
        // Create method name constant
        let method_idx = ctx.add_string(method_name);
        let method_const = ctx.add_constant(CompilationConstant::String(method_idx))?;
        
        // Emit SELF instruction
        ctx.emit(Instruction::create_ABC(
            OpCode::SelfOp,
            func_reg as u32,
            func_reg as u32,
            Instruction::encode_constant(method_const),
        ));
        
        // Get arguments
        let args = match &call.args {
            CallArgs::Args(exprs) => exprs,
            _ => return Err(LuaError::NotImplemented("Special call syntax".to_string())),
        };
        
        // Reserve ALL consecutive registers needed for the full argument list
        // BEFORE compiling any expressions to avoid register corruption in nested calls
        let total_regs_needed = 2 + args.len() as u8; // func + self + args
        
        // If we need more registers than we've already reserved, reserve them now
        if args.len() > 0 {
            // Ensure all argument registers are reserved
            for i in 0..args.len() {
                let arg_reg = func_reg + 2 + i as u8;
                if arg_reg >= 250 {
                    return Err(LuaError::CompileError(format!(
                        "Too many registers required for function call arguments ({})", 
                        total_regs_needed
                    )));
                }
                // Mark this register as Reserved to prevent nested calls from using it
                if ctx.register_states[arg_reg as usize] != RegisterState::Reserved {
                    ctx.register_states[arg_reg as usize] = RegisterState::Reserved;
                }
            }
            
            // Update max stack size
            if func_reg + total_regs_needed > ctx.current_function.max_stack_size {
                ctx.current_function.max_stack_size = func_reg + total_regs_needed;
            }
        }
        
        // Now compile each argument into its reserved register
        for (i, arg) in args.iter().enumerate() {
            let arg_reg = func_reg + 2 + i as u8;
            compile_expression_to_register_with_parent(ctx, arg, arg_reg, parent)?;
        }
        
        // Emit CALL instruction with context-specific result count
        let nargs = args.len() as u32 + 2; // +1 for function, +1 for self
        let is_statement = ctx.in_statement_context();
        let nresults = if is_statement {
            1  // For statement context (0 results + 1)
        } else {
            2  // For expression context (1 result + 1)
        };
        
        ctx.emit(Instruction::create_ABC(OpCode::Call, func_reg as u32, nargs, nresults));
        
        // Free argument registers
        for i in 1..nargs {
            ctx.free_register(func_reg + i as u8);
        }
        
        Ok(func_reg)
    } else {
        // Regular function call
        let func_reg = ctx.allocate_register()?;
        compile_expression_to_register_with_parent(ctx, &call.function, func_reg, parent)?;
        
        // Get arguments
        let args = match &call.args {
            CallArgs::Args(exprs) => exprs,
            _ => return Err(LuaError::NotImplemented("Special call syntax".to_string())),
        };
        
        // Reserve ALL consecutive registers needed for the full argument list
        // BEFORE compiling any expressions to avoid register corruption in nested calls
        let total_regs_needed = 1 + args.len() as u8; // func + args
        
        // If we need more registers than we've already reserved, reserve them now
        if args.len() > 0 {
            // Ensure all argument registers are reserved
            for i in 0..args.len() {
                let arg_reg = func_reg + 1 + i as u8;
                if arg_reg >= 250 {
                    return Err(LuaError::CompileError(format!(
                        "Too many registers required for function call arguments ({})", 
                        total_regs_needed
                    )));
                }
                // Mark this register as Reserved to prevent nested calls from using it
                if ctx.register_states[arg_reg as usize] != RegisterState::Reserved {
                    ctx.register_states[arg_reg as usize] = RegisterState::Reserved;
                }
            }
            
            // Update max stack size if needed
            if func_reg + total_regs_needed > ctx.current_function.max_stack_size {
                ctx.current_function.max_stack_size = func_reg + total_regs_needed;
            }
        }
        
        // Now compile each argument into its reserved register
        for (i, arg) in args.iter().enumerate() {
            let arg_reg = func_reg + 1 + i as u8;
            compile_expression_to_register_with_parent(ctx, arg, arg_reg, parent)?;
        }
        
        // Emit CALL instruction with context-specific result count
        let nargs = args.len() as u32 + 1;
        let is_statement = ctx.in_statement_context();
        let nresults = if is_statement {
            1  // For statement context (0 results + 1)
        } else {
            2  // For expression context (1 result + 1)
        };
        
        ctx.emit(Instruction::create_ABC(OpCode::Call, func_reg as u32, nargs, nresults));
        
        // Free argument registers
        for i in 1..nargs {
            ctx.free_register(func_reg + i as u8);
        }
        
        Ok(func_reg)
    }
}

/// Compile a tail call (function call in return position)
fn compile_tailcall_with_parent(
    ctx: &mut CodeGenContext, 
    call: &FunctionCall,
    parent: Option<&CodeGenContext>
) -> LuaResult<()> {
    eprintln!("DEBUG compile_tailcall: Compiling tail call");
    
    // Check if we need to emit CLOSE for any locals before tail call
    let mut min_local_register = None;
    if !ctx.locals.is_empty() {
        for local in &ctx.locals {
            match min_local_register {
                None => min_local_register = Some(local.register),
                Some(current) if local.register < current => {
                    min_local_register = Some(local.register);
                }
                _ => {}
            }
        }
    }
    
    // Check if this is a method call
    if let Some(method_name) = &call.method {
        // Method call - reserve consecutive registers for function and self
        let func_reg = ctx.reserve_consecutive_registers(2)?;
        let self_reg = func_reg + 1;
        
        eprintln!("DEBUG compile_tailcall: Method call setup, func_reg={}, self_reg={}", func_reg, self_reg);
        
        // Compile the table expression
        compile_expression_to_register_with_parent(ctx, &call.function, func_reg, parent)?;
        
        // Create method name constant
        let method_idx = ctx.add_string(method_name);
        let method_const = ctx.add_constant(CompilationConstant::String(method_idx))?;
        
        // Emit SELF instruction
        ctx.emit(Instruction::create_ABC(
            OpCode::SelfOp,
            func_reg as u32,
            func_reg as u32,
            Instruction::encode_constant(method_const),
        ));
        
        // Get arguments
        let args = match &call.args {
            CallArgs::Args(exprs) => exprs,
            _ => return Err(LuaError::NotImplemented("Special call syntax in tail position".to_string())),
        };
        
        // Reserve ALL consecutive registers needed for the full argument list
        // BEFORE compiling any expressions to avoid register corruption in nested calls
        let total_regs_needed = 2 + args.len() as u8; // func + self + args
        
        // If we need more registers than we've already reserved, reserve them now
        if args.len() > 0 {
            // Ensure all argument registers are reserved
            for i in 0..args.len() {
                let arg_reg = func_reg + 2 + i as u8;
                if arg_reg >= 250 {
                    return Err(LuaError::CompileError(format!(
                        "Too many registers required for function call arguments ({})", 
                        total_regs_needed
                    )));
                }
                // Mark this register as Reserved to prevent nested calls from using it
                if ctx.register_states[arg_reg as usize] != RegisterState::Reserved {
                    ctx.register_states[arg_reg as usize] = RegisterState::Reserved;
                }
            }
            
            // Update max stack size if needed
            if func_reg + total_regs_needed > ctx.current_function.max_stack_size {
                ctx.current_function.max_stack_size = func_reg + total_regs_needed;
            }
        }
        
        // Now compile each argument into its reserved register
        for (i, arg) in args.iter().enumerate() {
            let arg_reg = func_reg + 2 + i as u8;
            compile_expression_to_register_with_parent(ctx, arg, arg_reg, parent)?;
        }
        
        // Emit CLOSE if we have locals
        if let Some(close_reg) = min_local_register {
            ctx.emit(Instruction::create_ABC(OpCode::Close, close_reg as u32, 0, 0));
        }
        
        // Emit TAILCALL instruction
        let nargs = args.len() as u32 + 2; // +1 for function, +1 for self
        eprintln!("DEBUG compile_tailcall: Emitting TAILCALL with func_reg={}, nargs={}", func_reg, nargs);
        ctx.emit(Instruction::create_ABC(OpCode::TailCall, func_reg as u32, nargs, 0)); // C=0 means return all
        
        // Per Lua 5.1 specification, TAILCALL must be followed by a RETURN 0 1
        // This RETURN is never executed but must be present in the bytecode
        ctx.emit(Instruction::create_ABC(OpCode::Return, 0, 1, 0));
        
        eprintln!("DEBUG compile_tailcall: Emitted RETURN 0 1 after TAILCALL (required by spec)");
        
        // Free registers (though not strictly necessary for tail call)
        for i in 0..nargs {
            ctx.free_register(func_reg + i as u8);
        }
    } else {
        // Regular function call
        let func_reg = ctx.allocate_register()?;
        
        eprintln!("DEBUG compile_tailcall: Regular call, func_reg={}", func_reg);
        
        compile_expression_to_register_with_parent(ctx, &call.function, func_reg, parent)?;
        
        // Get arguments
        let args = match &call.args {
            CallArgs::Args(exprs) => exprs,
            _ => return Err(LuaError::NotImplemented("Special call syntax in tail position".to_string())),
        };
        
        // Reserve ALL consecutive registers needed for the full argument list
        // BEFORE compiling any expressions to avoid register corruption in nested calls
        let total_regs_needed = 1 + args.len() as u8; // func + args
        
        // If we need more registers than we've already reserved, reserve them now
        if args.len() > 0 {
            // Ensure all argument registers are reserved
            for i in 0..args.len() {
                let arg_reg = func_reg + 1 + i as u8;
                if arg_reg >= 250 {
                    return Err(LuaError::CompileError(format!(
                        "Too many registers required for function call arguments ({})", 
                        total_regs_needed
                    )));
                }
                // Mark this register as Reserved to prevent nested calls from using it
                if ctx.register_states[arg_reg as usize] != RegisterState::Reserved {
                    ctx.register_states[arg_reg as usize] = RegisterState::Reserved;
                }
            }
            
            // Update max stack size if needed
            if func_reg + total_regs_needed > ctx.current_function.max_stack_size {
                ctx.current_function.max_stack_size = func_reg + total_regs_needed;
            }
        }
        
        // Now compile each argument into its reserved register
        for (i, arg) in args.iter().enumerate() {
            let arg_reg = func_reg + 1 + i as u8;
            compile_expression_to_register_with_parent(ctx, arg, arg_reg, parent)?;
        }
        
        // Emit CLOSE if we have locals
        if let Some(close_reg) = min_local_register {
            ctx.emit(Instruction::create_ABC(OpCode::Close, close_reg as u32, 0, 0));
        }
        
        // Emit TAILCALL instruction
        let nargs = args.len() as u32 + 1;
        eprintln!("DEBUG compile_tailcall: Emitting TAILCALL with func_reg={}, nargs={}", func_reg, nargs);
        ctx.emit(Instruction::create_ABC(OpCode::TailCall, func_reg as u32, nargs, 0)); // C=0 means return all
        
        // Per Lua 5.1 specification, TAILCALL must be followed by a RETURN 0 1
        // This RETURN is never executed but must be present in the bytecode
        ctx.emit(Instruction::create_ABC(OpCode::Return, 0, 1, 0));
        
        eprintln!("DEBUG compile_tailcall: Emitted RETURN 0 1 after TAILCALL (required by spec)");
        
        // Free registers (though not strictly necessary for tail call)
        for i in 0..nargs {
            ctx.free_register(func_reg + i as u8);
        }
    }
    
    Ok(())
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
    eprintln!("DEBUG compile_return_statement: {} expressions", expressions.len());
    
    // Check for tail call pattern: exactly one expression that is a function call
    if expressions.len() == 1 {
        if let Expression::FunctionCall(call) = &expressions[0] {
            eprintln!("DEBUG compile_return_statement: Detected tail call pattern");
            // This is a tail call - use TAILCALL instead of CALL + RETURN
            compile_tailcall_with_parent(ctx, call, parent)?;
            
            // TAILCALL implicitly returns, so no need for RETURN opcode here
            eprintln!("DEBUG compile_return_statement: Using TAILCALL optimization, skipping RETURN");
            return Ok(());
        }
    }
    
    // Regular return statement (not a tail call)
    // Check if we need to emit CLOSE for any locals
    let mut min_local_register = None;
    if !ctx.locals.is_empty() {
        // Find the minimum register among all locals
        for local in &ctx.locals {
            match min_local_register {
                None => min_local_register = Some(local.register),
                Some(current) if local.register < current => {
                    min_local_register = Some(local.register);
                }
                _ => {}
            }
        }
    }
    
    // Allocate consecutive registers for return values
    let base_reg = if expressions.is_empty() {
        0
    } else {
        ctx.reserve_consecutive_registers(expressions.len() as u8)?
    };
    
    // Compile return values
    for (i, expr) in expressions.iter().enumerate() {
        let target_reg = base_reg + i as u8;
        compile_expression_to_register_with_parent(ctx, expr, target_reg, parent)?;
    }
    
    // Emit CLOSE if we have locals that might be captured as upvalues
    if let Some(close_reg) = min_local_register {
        eprintln!("DEBUG compile_return_statement: Emitting CLOSE for register {} before RETURN", close_reg);
        ctx.emit(Instruction::create_ABC(OpCode::Close, close_reg as u32, 0, 0));
    } else {
        eprintln!("DEBUG compile_return_statement: No locals need closing before RETURN");
    }
    
    // Emit RETURN instruction
    let nresults = expressions.len() as u32 + 1;
    eprintln!("DEBUG compile_return_statement: Emitting RETURN with base R({}), nresults={}", base_reg, nresults);
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
    
    ctx.emit(Instruction::create_ABC(OpCode::Test, cond_reg as u32, 0, 1));
    let jump_false = ctx.current_pc();
    ctx.emit(Instruction::create_AsBx(OpCode::Jmp, 0, 0));
    
    ctx.free_register(cond_reg);
    
    // Compile then body
    compile_block_with_parent(ctx, body, parent)?;
    
    // Jump to end
    jump_to_end.push(ctx.current_pc());
    ctx.emit(Instruction::create_AsBx(OpCode::Jmp, 0, 0));
    
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
    
    // Save break targets for this loop
    let saved_break_targets = ctx.break_targets.clone();
    ctx.break_targets.clear();
    
    // Compile condition
    let cond_reg = ctx.allocate_register()?;
    compile_expression_to_register_with_parent(ctx, condition, cond_reg, parent)?;
    
    ctx.emit(Instruction::create_ABC(OpCode::Test, cond_reg as u32, 0, 1));
    let jump_false = ctx.current_pc();
    ctx.emit(Instruction::create_AsBx(OpCode::Jmp, 0, 0));
    
    ctx.free_register(cond_reg);
    
    // Compile body with a new scope
    ctx.enter_scope();
    compile_block_with_parent(ctx, body, parent)?;
    ctx.exit_scope();
    
    // Jump back to start
    let jump_offset = -(ctx.current_pc() as i32 - loop_start as i32 + 1);
    ctx.emit(Instruction::create_AsBx(OpCode::Jmp, 0, jump_offset));
    
    // Patch jump for false condition
    let jump_offset = (ctx.current_pc() - jump_false - 1) as i32;
    ctx.current_function.bytecode[jump_false] = Instruction::create_AsBx(OpCode::Jmp, 0, jump_offset).0;
    
    // Patch break jumps
    for break_target in &ctx.break_targets {
        let jump_offset = (ctx.current_pc() - break_target - 1) as i32;
        ctx.current_function.bytecode[*break_target] = Instruction::create_AsBx(OpCode::Jmp, 0, jump_offset).0;
    }
    
    // Restore break targets
    ctx.break_targets = saved_break_targets;
    
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
    // For loops require 4 consecutive registers
    let loop_base = ctx.reserve_consecutive_registers(4)?;
    
    // Compile initial, limit, and step values
    compile_expression_to_register_with_parent(ctx, initial, loop_base, parent)?;
    compile_expression_to_register_with_parent(ctx, limit, loop_base + 1, parent)?;
    
    if let Some(step_expr) = step {
        compile_expression_to_register_with_parent(ctx, step_expr, loop_base + 2, parent)?;
    } else {
        let const_idx = ctx.add_constant(CompilationConstant::Number(1.0))?;
        ctx.emit(Instruction::create_ABx(OpCode::LoadK, (loop_base + 2) as u32, const_idx));
    }
    
    // Emit FORPREP
    let forprep_pc = ctx.current_pc();
    ctx.emit(Instruction::create_AsBx(OpCode::ForPrep, loop_base as u32, 0));
    
    // Register the loop variable
    ctx.enter_scope();
    ctx.register_states[(loop_base + 3) as usize] = RegisterState::Local;
    ctx.locals.push(LocalVar {
        name: variable.to_string(),
        register: loop_base + 3,
        scope_level: ctx.scope_level,
    });
    
    // Compile loop body
    let loop_start = ctx.current_pc();
    compile_block_with_parent(ctx, body, parent)?;
    
    // Emit FORLOOP
    let loop_offset = -(ctx.current_pc() as i32 - loop_start as i32 + 1);
    ctx.emit(Instruction::create_AsBx(OpCode::ForLoop, loop_base as u32, loop_offset));
    
    // Patch FORPREP
    let prep_offset = (ctx.current_pc() - forprep_pc - 1) as i32;
    ctx.current_function.bytecode[forprep_pc] = 
        Instruction::create_AsBx(OpCode::ForPrep, loop_base as u32, prep_offset).0;
    
    // Exit scope and free loop registers
    ctx.exit_scope();
    for i in 0..4 {
        ctx.free_register(loop_base + i);
    }
    
    Ok(())
}

/// Compile a generic for-in loop with parent context
fn compile_for_in_loop_with_parent(
    ctx: &mut CodeGenContext,
    variables: &[String],
    iterators: &[Expression],
    body: &Block,
    parent: Option<&CodeGenContext>,
) -> LuaResult<()> {
    if iterators.len() != 1 {
        return Err(LuaError::NotImplemented(
            "Multiple iterator expressions not yet supported".to_string()
        ));
    }
    
    eprintln!("DEBUG compile_for_in_loop: Compiling generic for loop with {} variables", variables.len());
    
    // Reserve consecutive registers for iterator state and loop variables
    let iter_func_reg = ctx.reserve_consecutive_registers(3 + variables.len() as u8)?;
    let state_reg = iter_func_reg + 1;
    let control_reg = iter_func_reg + 2;
    let first_var_reg = iter_func_reg + 3;
    
    eprintln!("DEBUG compile_for_in_loop: Reserved registers - iter_func={}, state={}, control={}, first_var={}", 
             iter_func_reg, state_reg, control_reg, first_var_reg);
    
    // Compile iterator expression
    match &iterators[0] {
        Expression::FunctionCall(call) => {
            // Compile function call to get iterator, state, and initial value
            let base_call = compile_function_call_with_parent(ctx, call, parent)?;
            
            // Ensure exactly 3 results for iterator triplet
            let frame = ctx.current_function.bytecode.len() - 1;
            if frame > 0 {
                let instr = Instruction(ctx.current_function.bytecode[frame]);
                if instr.get_opcode() == OpCode::Call {
                    // Modify the CALL instruction to return exactly 3 values (C=4)
                    let nargs = instr.get_b();
                    let new_instr = Instruction::create_ABC(OpCode::Call, instr.get_a(), nargs, 4);
                    ctx.current_function.bytecode[frame] = new_instr.0;
                    
                    eprintln!("DEBUG compile_for_in_loop: Modified CALL to return 3 values (C=4)");
                }
            }
            
            // Move the returned values to our iterator registers
            ctx.emit(Instruction::create_ABC(OpCode::Move, iter_func_reg as u32, base_call as u32, 0));
            ctx.emit(Instruction::create_ABC(OpCode::Move, state_reg as u32, (base_call + 1) as u32, 0));
            ctx.emit(Instruction::create_ABC(OpCode::Move, control_reg as u32, (base_call + 2) as u32, 0));
            
            ctx.free_register(base_call);
        },
        _ => {
            // Single value for iterator function, nil for others
            let iter_base = ctx.allocate_register()?;
            compile_expression_to_register_with_parent(ctx, &iterators[0], iter_base, parent)?;
            ctx.emit(Instruction::create_ABC(OpCode::Move, iter_func_reg as u32, iter_base as u32, 0));
            ctx.emit(Instruction::create_ABC(OpCode::LoadNil, state_reg as u32, control_reg as u32, 0));
            ctx.free_register(iter_base);
        }
    };
    
    // Register loop variables
    ctx.enter_scope();
    for (i, var_name) in variables.iter().enumerate() {
        let var_reg = first_var_reg + i as u8;
        ctx.register_states[var_reg as usize] = RegisterState::Local;
        ctx.locals.push(LocalVar {
            name: var_name.clone(),
            register: var_reg,
            scope_level: ctx.scope_level,
        });
        
        eprintln!("DEBUG compile_for_in_loop: Registered loop variable '{}' at register {}", 
                 var_name, var_reg);
    }
    
    // Compile loop body first, so we know where to jump
    let body_start = ctx.current_pc();
    compile_block_with_parent(ctx, body, parent)?;
    
    eprintln!("DEBUG compile_for_in_loop: Body compiled, body_start={}, current_pc={}", 
             body_start, ctx.current_pc());
    
    // TFORLOOP must be at the end of the loop body
    // It calls the iterator and does the following:
    // - If iterator returns nil, skip next instruction (exit loop)
    // - If iterator returns a value, set control variable, update loop vars, and continue
    
    // Call iterator function and update control/loop variables in one instruction
    ctx.emit(Instruction::create_ABC(OpCode::TForLoop, iter_func_reg as u32, 0, variables.len() as u32));
    
    // Jump back to the start of the loop body
    // This jump is only executed when the iterator returns a value (not nil)
    // When the iterator returns nil, TFORLOOP will skip this instruction
    let loop_back_offset = -(ctx.current_pc() as i32 - body_start as i32);
    ctx.emit(Instruction::create_AsBx(OpCode::Jmp, 0, loop_back_offset));
    
    eprintln!("DEBUG compile_for_in_loop: Emitted TFORLOOP and JMP back with offset {}", 
             loop_back_offset);
    
    // Exit scope and free iterator registers
    ctx.exit_scope();
    for i in 0..(3 + variables.len() as u8) {
        ctx.free_register(iter_func_reg + i);
    }
    
    eprintln!("DEBUG compile_for_in_loop: Complete");
    
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
    ctx.enter_scope();
    
    for statement in &block.statements {
        compile_statement_with_parent(ctx, statement, parent)?;
    }
    
    ctx.exit_scope();
    
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
    // Create child context
    let mut child_ctx = ctx.child();
    child_ctx.current_function.num_params = parameters.len() as u8;
    child_ctx.current_function.is_vararg = is_vararg;
    
    // Register parameters
    for (i, param) in parameters.iter().enumerate() {
        child_ctx.allocate_specific_register(i as u8)?;
        child_ctx.locals.push(LocalVar {
            name: param.clone(),
            register: i as u8,
            scope_level: 0,
        });
    }
    
    // Update next free register
    child_ctx.next_free_register = parameters.len() as u8;
    
    // Compile the function body
    compile_block_with_parent(&mut child_ctx, body, Some(ctx))?;
    
    // Add implicit return if not present
    if !ends_with_return(&body.statements) {
        // Check if we need to emit CLOSE before implicit return
        if let Some(min_reg) = child_ctx.has_locals_needing_close(0) {
            child_ctx.emit(Instruction::create_ABC(OpCode::Close, min_reg as u32, 0, 0));
        }
        
        child_ctx.emit(Instruction::create_ABC(OpCode::Return, 0, 1, 0));
    }
    
    // Add the compiled function as a prototype
    let proto_idx = ctx.current_function.prototypes.len();
    ctx.current_function.prototypes.push(child_ctx.current_function);
    
    // Merge string tables
    for string in child_ctx.strings {
        ctx.add_string(&string);
    }
    
    // Create a constant for this function prototype
    let const_idx = ctx.add_constant(CompilationConstant::FunctionProto(proto_idx))?;
    
    // Emit CLOSURE instruction
    ctx.emit(Instruction::create_ABx(OpCode::Closure, target as u32, const_idx));
    
    // Emit pseudo-instructions for upvalues
    let upvalues: Vec<CompilationUpvalue> = ctx.current_function.prototypes[proto_idx].upvalues.clone();
    
    for upval in upvalues {
        if upval.in_stack {
            // MOVE pseudo-instruction
            ctx.emit(Instruction::create_ABC(OpCode::Move, 0, upval.index as u32, 0));
        } else {
            // GETUPVAL pseudo-instruction
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
    let func_reg = ctx.allocate_local_register(name)?;
    
    // Compile the function expression
    compile_function_expression_with_parent(ctx, parameters, is_vararg, body, func_reg)?;
    
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
    // Compile the function expression
    let func_reg = ctx.allocate_register()?;
    
    // Add 'self' parameter for methods
    let mut params = parameters.to_vec();
    if name.method.is_some() {
        params.insert(0, "self".to_string());
    }
    
    compile_function_expression_with_parent(ctx, &params, is_vararg, body, func_reg)?;
    
    // Store the function
    if name.names.len() == 1 && name.method.is_none() {
        // Simple global function
        let func_name = &name.names[0];
        let string_idx = ctx.add_string(func_name);
        let const_idx = ctx.add_constant(CompilationConstant::String(string_idx))?;
        
        ctx.emit(Instruction::create_ABx(OpCode::SetGlobal, func_reg as u32, const_idx));
    } else {
        // Table member function
        let mut table_reg = ctx.allocate_register()?;
        
        // Get the first table
        if let Some(local_reg) = ctx.lookup_local(&name.names[0]) {
            ctx.emit(Instruction::create_ABC(OpCode::Move, table_reg as u32, local_reg as u32, 0));
        } else {
            let string_idx = ctx.add_string(&name.names[0]);
            let const_idx = ctx.add_constant(CompilationConstant::String(string_idx))?;
            ctx.emit(Instruction::create_ABx(OpCode::GetGlobal, table_reg as u32, const_idx));
        }
        
        // Navigate through table chain
        for i in 1..name.names.len() {
            let field_idx = ctx.add_string(&name.names[i]);
            let field_const = ctx.add_constant(CompilationConstant::String(field_idx))?;
            
            let next_reg = ctx.allocate_register()?;
            ctx.emit(Instruction::create_ABC(
                OpCode::GetTable,
                next_reg as u32,
                table_reg as u32,
                Instruction::encode_constant(field_const)
            ));
            
            ctx.free_register(table_reg);
            table_reg = next_reg;
        }
        
        // Set the function in the final table
        if let Some(method) = &name.method {
            let field_idx = ctx.add_string(method);
            let field_const = ctx.add_constant(CompilationConstant::String(field_idx))?;
            
            ctx.emit(Instruction::create_ABC(
                OpCode::SetTable,
                table_reg as u32,
                Instruction::encode_constant(field_const),
                func_reg as u32
            ));
        }
        
        ctx.free_register(table_reg);
    }
    
    ctx.free_register(func_reg);
    
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