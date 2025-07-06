//! Lua Bytecode Generator
//!
//! This module implements the bytecode generator for Lua, converting
//! the AST into bytecode following the architectural principle of
//! complete independence from the VM and heap.

use std::collections::{HashMap, HashSet};
use super::ast::*;
use super::error::{LuaError, LuaResult};

/// Opcodes for Lua bytecode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpCode {
    Move,       // R(A) := R(B)
    LoadK,      // R(A) := Kst(Bx)
    LoadBool,   // R(A) := (Bool)B; if (C) pc++
    LoadNil,    // R(A), R(A+1), ..., R(A+B) := nil
    GetUpval,   // R(A) := UpValue[B]
    GetGlobal,  // R(A) := Gbl[Kst(Bx)]
    SetGlobal,  // Gbl[Kst(Bx)] := R(A)
    SetUpval,   // UpValue[B] := R(A)
    GetTable,   // R(A) := R(B)[RK(C)]
    SetTable,   // R(A)[RK(B)] := RK(C)
    NewTable,   // R(A) := {} (size = B,C)
    Self_,      // R(A+1) := R(B); R(A) := R(B)[RK(C)]
    SetList,    // R(A)[(C-1)*FPF+i] := R(A+i), 1 <= i <= B
    Add,        // R(A) := RK(B) + RK(C)
    Sub,        // R(A) := RK(B) - RK(C)
    Mul,        // R(A) := RK(B) * RK(C)
    Div,        // R(A) := RK(B) / RK(C)
    Mod,        // R(A) := RK(B) % RK(C)
    Pow,        // R(A) := RK(B) ^ RK(C)
    Unm,        // R(A) := -R(B)
    Not,        // R(A) := not R(B)
    Len,        // R(A) := length of R(B)
    Concat,     // R(A) := R(B).. ... ..R(C)
    Jmp,        // pc += sBx
    Eq,         // if ((RK(B) == RK(C)) ~= A) then pc++
    Lt,         // if ((RK(B) <  RK(C)) ~= A) then pc++
    Le,         // if ((RK(B) <= RK(C)) ~= A) then pc++
    Test,       // if not (R(A) <=> C) then pc++
    TestSet,    // if (R(B) <=> C) then R(A) := R(B) else pc++
    Call,       // R(A), ..., R(A+C-2) := R(A)(R(A+1), ..., R(A+B-1))
    TailCall,   // return R(A)(R(A+1), ..., R(A+B-1))
    Return,     // return R(A), ..., R(A+B-2)
    ForLoop,    // R(A) += R(A+2); if R(A) <?= R(A+1) then { pc+=sBx; R(A+3) = R(A) }
    ForPrep,    // R(A) -= R(A+2); pc += sBx
    TForLoop,   // R(A+3), ... , R(A+3+C) := R(A)(R(A+1), R(A+2)); if !Nil then R(A+2)=R(A+3), pc+=sBx
    VarArg,     // R(A), R(A+1), ..., R(A+B-2) = vararg
    Closure,    // R(A) := closure(KPROTO[Bx])
    Close,      // close all upvalues >= R(A)
    ExtraArg,   // Extra argument for previous instruction
}

/// Information about a variable in the compiler
#[derive(Debug, Clone)]
struct VarInfo {
    /// Register holding the variable
    register: usize,
    
    /// Scope level where the variable is defined
    level: usize,
    
    /// Is this variable captured by a closure
    captured: bool,
    
    /// The last instruction where this variable is used
    /// Used for register allocation optimization
    last_use: Option<usize>,
}

/// Information about a label
#[derive(Debug, Clone)]
struct LabelInfo {
    /// PC where the label is defined
    pc: usize,
    /// Scope level of the label
    level: usize,
}

/// Information about a break statement
#[derive(Debug, Clone)]
struct BreakJump {
    /// PC of the jump instruction
    pc: usize,
    /// Scope level
    level: usize,
}

/// Information about a pending goto
#[derive(Debug, Clone)]
struct PendingGoto(String, usize, usize); // Label, PC, level

/// Helper for register allocation with proper lifetime tracking
#[derive(Debug, Clone)]
struct RegisterAllocator {
    /// Number of registers currently in use
    used: usize,
    
    /// Maximum number of registers used
    max_used: usize,
    
    /// Free registers that can be reused
    free_registers: Vec<usize>,
    
    /// Track active registers and their assignment to variables
    register_to_variable: HashMap<usize, String>,
    
    /// Current instruction index
    current_instruction: usize,
    
    /// Registers that need to be preserved during state restoration
    preserved_registers: HashSet<usize>,
}

impl RegisterAllocator {
    /// Create a new register allocator
    fn new() -> Self {
        RegisterAllocator {
            used: 0,
            max_used: 0,
            free_registers: Vec::new(),
            register_to_variable: HashMap::new(),
            current_instruction: 0,
            preserved_registers: HashSet::new(),
        }
    }
    
    /// Save the current allocation state for later restoration
    /// This is the key to proper scoped register allocation
    pub fn save_state(&self) -> usize {
        self.used
    }
    
    /// Mark a register to be preserved when restoring state
    fn preserve_register(&mut self, reg: usize) {
        self.preserved_registers.insert(reg);
    }
    
    /// Mark multiple registers to be preserved
    fn preserve_registers(&mut self, regs: &[usize]) {
        for &reg in regs {
            self.preserve_register(reg);
        }
    }
    
    /// Clear all preserved register markings
    fn clear_preserved(&mut self) {
        self.preserved_registers.clear();
    }
    
    /// Restore state to a previously saved point, preserving registers
    /// allocated by parent contexts
    pub fn restore_state(&mut self, saved_state: usize) {
        // Free all registers allocated since saved_state
        // EXCEPT those marked as preserved
        for reg in saved_state..self.used {
            // Skip preserved registers
            if self.preserved_registers.contains(&reg) {
                continue;
            }
            
            if !self.free_registers.contains(&reg) {
                self.free_registers.push(reg);
                
                // Also remove from variable mapping if present
                self.register_to_variable.remove(&reg);
            }
        }
        
        // Reset allocation pointer to saved state
        self.used = saved_state;
        
        // Sort free registers for better allocation patterns
        self.free_registers.sort_unstable();
    }
    
    /// The problematic function causing register conflicts
    /// NOTE: This method is preserved for backwards compatibility but
    /// now delegates to restore_state() for better scoping
    fn free_to(&mut self, level: usize) {
        self.restore_state(level);
    }
    
    /// Allocate a register, preferring to reuse a free register if possible
    fn allocate(&mut self) -> usize {
        // Try to reuse a free register first
        if let Some(reg) = self.free_registers.pop() {
            // Update max_used if needed
            if reg >= self.max_used {
                self.max_used = reg + 1;
            }
            return reg;
        }
        
        // No free registers, allocate a new one
        let reg = self.used;
        self.used += 1;
        
        // Update max_used
        if self.used > self.max_used {
            self.max_used = self.used;
        }
        
        reg
    }
    
    /// Free registers for variables at a specific scope level
    /// Updates the free_registers list for reuse
    fn free_scope(&mut self, level: usize, variables: &HashMap<String, VarInfo>) {
        // Find all variables at this scope level that are not captured
        let mut to_free = Vec::new();
        
        // First collect all registers/names to free to avoid borrow conflicts
        for (reg, var_name) in &self.register_to_variable {
            if let Some(var_info) = variables.get(var_name) {
                if var_info.level == level && !var_info.captured {
                    to_free.push((*reg, var_name.clone()));
                }
            }
        }
        
        // Now free them - avoid borrow checker issues
        for (reg, _) in to_free {
            // Skip preserved registers
            if !self.preserved_registers.contains(&reg) {
                self.free_registers.push(reg);
                self.register_to_variable.remove(&reg);
            }
        }
        
        // Sort free registers to prefer lower numbers
        self.free_registers.sort_unstable();
    }
    
    /// Mark a register as containing a specific variable
    fn mark_register(&mut self, register: usize, variable: String) {
        self.register_to_variable.insert(register, variable);
    }
    
    /// Check if a register might be holding a function or critical value
    fn is_register_used_for_function(&self, reg: usize) -> bool {
        // Heuristic: If the register is already marked with a variable, it might be a function
        self.register_to_variable.contains_key(&reg)
    }
    
    /// Get the current register usage level
    fn level(&self) -> usize {
        self.used
    }
    
    /// Increment the current instruction counter
    fn increment_instruction(&mut self) {
        self.current_instruction += 1;
    }
    
    /// Mark a variable as being used at the current instruction
    fn mark_variable_use(&mut self, variables: &mut HashMap<String, VarInfo>, name: &str) {
        if let Some(var_info) = variables.get_mut(name) {
            var_info.last_use = Some(self.current_instruction);
        }
    }
}

/// Upvalue information for compilation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompilationUpvalue {
    /// Upvalue name (for debugging)
    pub name: String,
    
    /// Is the upvalue in the stack?
    pub in_stack: bool,
    
    /// Index in stack or outer upvalues
    pub index: u8,
}

/// Compilation constant type - independent from VM Value type
#[derive(Debug, Clone, PartialEq)]
pub enum CompilationConstant {
    /// Nil value
    Nil,
    
    /// Boolean value
    Boolean(bool),
    
    /// Number value
    Number(f64),
    
    /// String value (index into the string table)
    String(usize),
    
    /// Function prototype (index into the prototype table)
    FunctionProto(usize),
}

/// Key for constant deduplication
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ConstantKey {
    /// Nil
    Nil,
    
    /// Boolean
    Boolean(bool),
    
    /// Number
    Number(i64),
    
    /// String
    String(String),
    
    /// Function prototype
    FunctionProto(usize),
}

/// Function prototype for compilation - independent from VM FunctionProto type
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledFunction {
    /// Bytecode instructions
    pub bytecode: Vec<u32>,
    
    /// Constant values (indices into string/proto tables)
    pub constants: Vec<CompilationConstant>,
    
    /// Number of parameters
    pub num_params: u8,
    
    /// Is variadic
    pub is_vararg: bool,
    
    /// Maximum stack size
    pub max_stack_size: u8,
    
    /// Upvalue information
    pub upvalues: Vec<CompilationUpvalue>,
    
    /// Nested function prototypes (only populated for the main function)
    pub prototypes: Vec<CompiledFunction>,
}

fn process_array_batch(
    codegen: &mut CodeGenerator,
    target: usize,
    fields: &[&Expression],
    start_index: usize,
) -> LuaResult<()> {
    const FIELDS_PER_FLUSH: usize = 50;
    
    let batch_state = codegen.registers.save_state();
    
    // Allocate registers for all fields in this batch
    let mut field_regs = Vec::with_capacity(fields.len());
    
    for _ in 0..fields.len() {
        field_regs.push(codegen.registers.allocate());
    }
    
    // Evaluate each expression in its own scope
    for (i, &expr) in fields.iter().enumerate() {
        let expr_state = codegen.registers.save_state();
        
        // Evaluate expression
        codegen.expression(expr, field_regs[i], 1)?;
        
        // Preserve the field register
        codegen.registers.preserve_register(field_regs[i]);
        codegen.registers.restore_state(expr_state);
    }
    
    // Emit SETLIST instruction for this batch
    let batch_index = (start_index - 1) / FIELDS_PER_FLUSH + 1;
    codegen.emit(CodeGenerator::encode_ABC(
        OpCode::SetList,
        target as u8,
        fields.len() as u16,
        batch_index as u16
    ));
    
    // Clear preserved registers for this batch and restore
    for &reg in &field_regs {
        codegen.registers.preserved_registers.remove(&reg);
    }
    codegen.registers.restore_state(batch_state);
    
    Ok(())
}

/// Bytecode generator for Lua
#[derive(Debug)]
pub struct CodeGenerator {
    /// Variables in the current scope
    variables: HashMap<String, VarInfo>,
    
    /// Upvalue list
    upvalues: Vec<CompilationUpvalue>,
    
    /// Constants list
    constants: Vec<CompilationConstant>,
    
    /// String table
    strings: Vec<String>,
    
    /// String table deduplication
    string_map: HashMap<String, usize>,
    
    /// Map for constant deduplication
    constant_map: HashMap<ConstantKey, usize>,
    
    /// Register allocator
    registers: RegisterAllocator,
    
    /// Current scope level
    scope_level: usize,
    
    /// Generated bytecode
    code: Vec<u32>,
    
    /// Nested function prototypes
    prototypes: Vec<CompiledFunction>,
    
    /// Parent generator (for nested functions)
    parent: Option<Box<CodeGenerator>>,
    
    /// Labels in the current function
    labels: HashMap<String, LabelInfo>,
    
    /// Pending gotos to resolve
    pending_gotos: Vec<PendingGoto>,
    
    /// Break jumps to patch
    break_jumps: Vec<BreakJump>,
    
    /// Is inside a loop?
    inside_loop: bool,
}

impl CodeGenerator {
    /// Create a new code generator
    pub fn new() -> Self {
        CodeGenerator {
            variables: HashMap::new(),
            upvalues: Vec::new(),
            constants: Vec::new(),
            strings: Vec::new(),
            string_map: HashMap::new(),
            constant_map: HashMap::new(),
            registers: RegisterAllocator::new(),
            scope_level: 0,
            code: Vec::new(),
            prototypes: Vec::new(),
            parent: None,
            labels: HashMap::new(),
            pending_gotos: Vec::new(),
            break_jumps: Vec::new(),
            inside_loop: false,
        }
    }
    
    /// Create a child code generator for a nested function
    pub fn child(&self) -> Self {
        CodeGenerator {
            variables: HashMap::new(),
            upvalues: Vec::new(),
            constants: Vec::new(),
            strings: self.strings.clone(),
            string_map: self.string_map.clone(),
            constant_map: HashMap::new(),
            registers: RegisterAllocator::new(),
            scope_level: 0,
            code: Vec::new(),
            prototypes: Vec::new(),
            parent: None, // Will be set by the caller
            labels: HashMap::new(),
            pending_gotos: Vec::new(),
            break_jumps: Vec::new(),
            inside_loop: false,
        }
    }
    
    /// Generate bytecode from a chunk, returning the main function with nested prototypes
    pub fn generate(mut self, chunk: &Chunk) -> LuaResult<(CompiledFunction, Vec<String>)> {
        // Enter the main scope
        self.enter_scope();
        
        // Generate code for each statement
        for stmt in &chunk.statements {
            self.statement(stmt)?;
        }
        
        // Generate code for return statement
        if let Some(ret) = &chunk.return_statement {
            self.emit_return(&ret.expressions)?;
        } else {
            // Implicit return
            self.emit(Self::encode_ABC(OpCode::Return, 0, 1, 0));
        }
        
        // Leave the main scope
        self.leave_scope();
        
        // Verify that all gotos have been resolved
        if !self.pending_gotos.is_empty() {
            let goto = &self.pending_gotos[0];
            let label = &goto.0;
            
            return Err(LuaError::CompileError(
                format!("No visible label '{}' for goto", label)
            ));
        }
        
        println!("DEBUG GENERATOR: generate returning with {} prototypes", self.prototypes.len());
        
        let main = CompiledFunction {
            bytecode: self.code,
            constants: self.constants,
            num_params: 0,
            is_vararg: false,
            max_stack_size: self.registers.max_used as u8,
            upvalues: self.upvalues,
            prototypes: self.prototypes,
        };
        
        Ok((main, self.strings))
    }
    
    /// Add a string to the string table
    fn add_string(&mut self, s: &str) -> LuaResult<usize> {
        // Check if we already have this string
        if let Some(&index) = self.string_map.get(s) {
            return Ok(index);
        }
        
        let index = self.strings.len();
        self.strings.push(s.to_string());
        self.string_map.insert(s.to_string(), index);
        
        Ok(index)
    }
    
    /// Add a string constant (adds to string table and creates a constant)
    fn add_string_constant(&mut self, s: &str) -> LuaResult<usize> {
        // First add the string to the string table
        let string_idx = self.add_string(s)?;
        
        // Then add a constant referencing that string
        let key = ConstantKey::String(s.to_string());
        if let Some(&index) = self.constant_map.get(&key) {
            return Ok(index);
        }
        
        let const_idx = self.constants.len();
        if const_idx > 0x3FFFF {
            return Err(LuaError::CompileError("Too many constants".to_string()));
        }
        
        self.constants.push(CompilationConstant::String(string_idx));
        self.constant_map.insert(key, const_idx);
        
        Ok(const_idx)
    }
    
    /// Emit a bytecode instruction with instruction counter update
    fn emit(&mut self, instruction: u32) {
        self.code.push(instruction);
        self.registers.increment_instruction();
    }
    
    /// Get the current PC
    fn current_pc(&self) -> usize {
        self.code.len()
    }
    
    /// Enter a new scope
    fn enter_scope(&mut self) {
        self.scope_level += 1;
    }
    
    /// Leave scope with proper register management
    fn leave_scope(&mut self) {
        // Find variables that need to be closed (upvalues)
        let mut to_close = Vec::new();
        
        for (name, var) in &self.variables {
            if var.level == self.scope_level && var.captured {
                to_close.push((name.clone(), var.register));
            }
        }
        
        // Emit CLOSE instruction if needed
        if !to_close.is_empty() {
            let min_reg = to_close.iter().map(|(_, reg)| *reg).min().unwrap();
            self.emit(Self::encode_ABC(OpCode::Close, min_reg as u8, 0, 0));
        }
        
        // Free registers for variables leaving scope
        self.registers.free_scope(self.scope_level, &self.variables);
        
        // Remove variables at this scope level from tracking
        self.variables.retain(|_, var| var.level != self.scope_level);
        
        self.scope_level -= 1;
    }
    
    /// Declare a local variable
    fn declare_local(&mut self, name: String) -> LuaResult<()> {
        if self.variables.contains_key(&name) && self.variables[&name].level == self.scope_level {
            return Err(LuaError::CompileError(
                format!("Duplicate local variable '{}'", name)
            ));
        }
        
        let register = self.registers.allocate();
        
        // Create variable info with proper last_use field
        self.variables.insert(name.clone(), VarInfo {
            register,
            level: self.scope_level,
            captured: false,
            last_use: Some(self.registers.current_instruction),
        });
        
        // Mark register as containing this variable
        self.registers.mark_register(register, name);
        
        Ok(())
    }
    
    /// Resolve a variable with proper usage tracking
    fn resolve_variable(&mut self, name: &str) -> Option<&VarInfo> {
        // Mark this variable as used at the current instruction
        self.registers.mark_variable_use(&mut self.variables, name);
        
        // Return variable info
        self.variables.get(name)
    }
    
    /// Resolve a variable and return a cloned copy (to avoid borrow issues)
    fn resolve_variable_and_clone(&self, name: &str) -> Option<VarInfo> {
        self.variables.get(name).cloned()
    }
    
    /// Find or create an upvalue
    fn find_or_create_upvalue(&mut self, name: &str) -> Option<usize> {
        // First, check if we already have this upvalue
        for (i, upvalue) in self.upvalues.iter().enumerate() {
            if upvalue.name == name {
                return Some(i);
            }
        }
        
        // If this is a local variable in a parent function, create an upvalue
        if let Some(ref mut parent) = self.parent {
            if let Some(var) = parent.variables.get_mut(name) {
                // Mark the variable as captured
                var.captured = true;
                
                // Add a new upvalue
                self.upvalues.push(CompilationUpvalue {
                    name: name.to_string(),
                    in_stack: true,
                    index: var.register as u8,
                });
                
                return Some(self.upvalues.len() - 1);
            }
            
            // If it's an upvalue in the parent, create an upvalue reference
            if let Some(upvalue_idx) = parent.find_or_create_upvalue(name) {
                self.upvalues.push(CompilationUpvalue {
                    name: name.to_string(),
                    in_stack: false,
                    index: upvalue_idx as u8,
                });
                
                return Some(self.upvalues.len() - 1);
            }
        }
        
        // Not found
        None
    }
    
    /// Add a constant to the constant pool
    fn add_constant(&mut self, constant: CompilationConstant) -> LuaResult<usize> {
        // Create a key for deduplication
        let key = match &constant {
            CompilationConstant::Nil => ConstantKey::Nil,
            CompilationConstant::Boolean(b) => ConstantKey::Boolean(*b),
            CompilationConstant::Number(n) => {
                if n.fract() == 0.0 && *n >= i64::MIN as f64 && *n <= i64::MAX as f64 {
                    ConstantKey::Number(*n as i64)
                } else {
                    ConstantKey::String(n.to_string())
                }
            },
            CompilationConstant::String(idx) => {
                let s = &self.strings[*idx];
                ConstantKey::String(s.clone())
            },
            CompilationConstant::FunctionProto(idx) => {
                ConstantKey::FunctionProto(*idx)
            },
        };
        
        // Check if we already have this constant
        if let Some(&index) = self.constant_map.get(&key) {
            return Ok(index);
        }
        
        // Add the constant
        let index = self.constants.len();
        if index > 0x3FFFF {
            return Err(LuaError::CompileError("Too many constants".to_string()));
        }
        
        self.constants.push(constant);
        self.constant_map.insert(key, index);
        
        Ok(index)
    }
    
    /// Add a number constant
    fn add_number_constant(&mut self, n: f64) -> LuaResult<usize> {
        self.add_constant(CompilationConstant::Number(n))
    }
    
    /// Add a boolean constant
    fn add_boolean_constant(&mut self, b: bool) -> LuaResult<usize> {
        self.add_constant(CompilationConstant::Boolean(b))
    }
    
    /// Encode an ABC instruction
    fn encode_ABC(opcode: OpCode, a: u8, b: u16, c: u16) -> u32 {
        let op = opcode_to_u8(opcode) as u32 & 0x3F;
        let a = (a as u32) << 6;
        let c = (c as u32) << 14;
        let b = (b as u32) << 23;
        
        op | a | c | b
    }
    
    /// Encode an ABx instruction
    fn encode_ABx(opcode: OpCode, a: u8, bx: u32) -> u32 {
        let op = opcode_to_u8(opcode) as u32 & 0x3F;
        let a = (a as u32) << 6;
        let bx = (bx & 0x3FFFF) << 14;
        
        op | a | bx
    }
    
    /// Encode an AsBx instruction
    fn encode_AsBx(opcode: OpCode, a: u8, sbx: i32) -> u32 {
        // Check if the signed offset is within bounds
        let clamped_sbx = if sbx < -131071 {
            // Clamp to minimum value with a warning
            println!("Warning: Jump offset {} out of range, clamping to -131071", sbx);
            -131071
        } else if sbx > 131070 {
            // Clamp to maximum value with a warning
            println!("Warning: Jump offset {} out of range, clamping to 131070", sbx);
            131070
        } else {
            sbx
        };
        
        let op = opcode_to_u8(opcode) as u32 & 0x3F;
        let a = (a as u32) << 6;
        
        // Add the bias value to convert to unsigned value for encoding
        // The bias is 2^17-1 = 131071
        let sbx_biased = ((clamped_sbx + 131071) as u32) & 0x3FFFF; // Ensure we don't exceed 18 bits
        let sbx_field = sbx_biased << 14;
        
        op | a | sbx_field
    }
    
    /// Compile a statement
    fn statement(&mut self, stmt: &Statement) -> LuaResult<()> {
        // Update instruction counter for register lifetime tracking
        self.registers.increment_instruction();
        
        match stmt {
            Statement::Assignment(assignment) => {
                self.assignment(assignment)?;
            }
            Statement::LocalDeclaration(local) => {
                self.local_declaration(local)?;
            }
            Statement::FunctionCall(call) => {
                // Allocate a register for the result, but discard it
                let result_reg = self.registers.allocate();
                self.function_call(call, result_reg, 1)?;
                self.registers.free_to(result_reg);
            }
            Statement::LabelDefinition(label) => {
                let pc = self.current_pc();
                
                // Check for duplicate label
                if self.labels.contains_key(label) {
                    return Err(LuaError::CompileError(
                        format!("Duplicate label '{}'", label)
                    ));
                }
                
                // Register the label
                self.labels.insert(label.clone(), LabelInfo {
                    pc,
                    level: self.scope_level,
                });
                
                // Resolve any pending gotos to this label
                self.resolve_gotos(label)?;
            }
            Statement::Break => {
                if !self.inside_loop {
                    return Err(LuaError::CompileError(
                        "Break statement not inside a loop".to_string()
                    ));
                }
                
                // Emit a jump that will be patched later
                self.emit(Self::encode_AsBx(OpCode::Jmp, 0, 0));
                
                // Remember the jump for patching
                self.break_jumps.push(BreakJump {
                    pc: self.current_pc() - 1,
                    level: self.scope_level,
                });
            }
            Statement::Goto(label) => {
                // Emit a jump that will be patched later
                self.emit(Self::encode_AsBx(OpCode::Jmp, 0, 0));
                
                // Remember the goto for resolution
                self.pending_gotos.push(PendingGoto(
                    label.clone(),
                    self.current_pc() - 1,
                    self.scope_level
                ));
            }
            Statement::Do(block) => {
                self.enter_scope();
                self.block(block)?;
                self.leave_scope();
            }
            Statement::While { condition, body } => {
                // Record the start of the loop
                let loop_start = self.current_pc();
                
                // Compile condition
                let cond_reg = self.registers.allocate();
                self.expression(condition, cond_reg, 1)?;
                
                // Emit test and jump
                self.emit(Self::encode_ABC(OpCode::Test, cond_reg as u8, 0, 0));
                let jump_out = self.current_pc();
                self.emit(Self::encode_AsBx(OpCode::Jmp, 0, 0)); // Placeholder
                
                self.registers.free_to(cond_reg);
                
                // Compile body
                self.inside_loop = true;
                self.enter_scope();
                let breaks_before = self.break_jumps.len();
                self.block(body)?;
                self.leave_scope();
                self.inside_loop = false;
                
                // Emit jump back to the start
                self.emit(Self::encode_AsBx(
                    OpCode::Jmp, 
                    0, 
                    loop_start as i32 - self.current_pc() as i32 - 1 // Relative offset
                ));
                
                // Patch the jump out
                let after_loop = self.current_pc();
                self.code[jump_out] = Self::encode_AsBx(
                    OpCode::Jmp,
                    0,
                    after_loop as i32 - jump_out as i32 - 1 // Relative offset
                );
                
                // Patch any break statements
                self.patch_breaks(breaks_before);
            }
            Statement::Repeat { body, condition } => {
                // Record the start of the loop
                let loop_start = self.current_pc();
                
                // Compile body
                self.inside_loop = true;
                self.enter_scope();
                let breaks_before = self.break_jumps.len();
                self.block(body)?;
                
                // Compile condition
                let cond_reg = self.registers.allocate();
                self.expression(condition, cond_reg, 1)?;
                
                // Emit test and jump (with inverted condition)
                self.emit(Self::encode_ABC(OpCode::Test, cond_reg as u8, 0, 0));
                self.emit(Self::encode_AsBx(
                    OpCode::Jmp, 
                    0, 
                    loop_start as i32 - self.current_pc() as i32 - 1 // Jump back to start if false
                ));
                
                self.registers.free_to(cond_reg);
                
                // End loop
                self.leave_scope();
                self.inside_loop = false;
                
                // Patch any break statements
                self.patch_breaks(breaks_before);
            }
            Statement::Return { expressions } => {
                // Handle return statement
                self.emit_return(expressions)?;
            }
            // Handle other statement types using our separate method
            _ => self.handle_other_statements(stmt)?,
        }
        
        Ok(())
    }
    
    fn handle_other_statements(&mut self, stmt: &Statement) -> LuaResult<()> {
        match stmt {
            Statement::If { condition, body, else_ifs, else_block } => {
                // Compile condition
                let cond_reg = self.registers.allocate();
                self.expression(condition, cond_reg, 1)?;
                
                // Test and jump to else part if false
                self.emit(Self::encode_ABC(OpCode::Test, cond_reg as u8, 0, 0));
                let jump_to_else = self.current_pc();
                self.emit(Self::encode_AsBx(OpCode::Jmp, 0, 0)); // Placeholder
                
                self.registers.free_to(cond_reg);
                
                // Compile then part
                self.enter_scope();
                self.block(body)?;
                self.leave_scope();
                
                // Jump to end (skip else part)
                let jump_to_end = self.current_pc();
                self.emit(Self::encode_AsBx(OpCode::Jmp, 0, 0)); // Placeholder
                
                // Patch jump to else part
                let else_start = self.current_pc();
                self.code[jump_to_else] = Self::encode_AsBx(
                    OpCode::Jmp,
                    0,
                    else_start as i32 - jump_to_else as i32 - 1 // Relative offset
                );
                
                // Compile else-if parts
                let mut jumps_to_end = vec![jump_to_end];
                
                for (else_if_cond, else_if_body) in else_ifs {
                    // Compile condition
                    let cond_reg = self.registers.allocate();
                    self.expression(else_if_cond, cond_reg, 1)?;
                    
                    // Test and jump to next else-if/else if false
                    self.emit(Self::encode_ABC(OpCode::Test, cond_reg as u8, 0, 0));
                    let jump_to_next = self.current_pc();
                    self.emit(Self::encode_AsBx(OpCode::Jmp, 0, 0)); // Placeholder
                    
                    self.registers.free_to(cond_reg);
                    
                    // Compile then part
                    self.enter_scope();
                    self.block(else_if_body)?;
                    self.leave_scope();
                    
                    // Jump to end
                    jumps_to_end.push(self.current_pc());
                    self.emit(Self::encode_AsBx(OpCode::Jmp, 0, 0)); // Placeholder
                    
                    // Patch jump to next
                    let next_start = self.current_pc();
                    self.code[jump_to_next] = Self::encode_AsBx(
                        OpCode::Jmp,
                        0,
                        next_start as i32 - jump_to_next as i32 - 1 // Relative offset
                    );
                }
                
                // Compile else part if any
                if let Some(else_body) = else_block {
                    self.enter_scope();
                    self.block(else_body)?;
                    self.leave_scope();
                }
                
                // Patch all jumps to end
                let end = self.current_pc();
                for jump in jumps_to_end {
                    self.code[jump] = Self::encode_AsBx(
                        OpCode::Jmp,
                        0,
                        end as i32 - jump as i32 - 1 // Relative offset
                    );
                }
            }
            
            Statement::ForLoop { variable, initial, limit, step, body } => {
                self.enter_scope();
                
                // Allocate registers: R(base), R(base+1), R(base+2), R(base+3) = var, limit, step, loop var
                let base_reg = self.registers.level();
                let var_reg = self.registers.allocate(); // R(base) - internal counter
                let limit_reg = self.registers.allocate(); // R(base+1) - limit
                let step_reg = self.registers.allocate(); // R(base+2) - step
                let loop_var_reg = self.registers.allocate(); // R(base+3) - Lua variable
                
                // Declare the variable
                self.variables.insert(variable.clone(), VarInfo {
                    register: loop_var_reg,
                    level: self.scope_level,
                    captured: false,
                    last_use: Some(self.registers.current_instruction),
                });
                
                // Mark register as containing this variable
                self.registers.mark_register(loop_var_reg, variable.clone());
                
                // Compile initial, limit, and step expressions
                self.expression(initial, var_reg, 1)?;
                self.expression(limit, limit_reg, 1)?;
                
                if let Some(step_expr) = step {
                    self.expression(step_expr, step_reg, 1)?;
                } else {
                    // Default step = 1
                    let const_idx = self.add_number_constant(1.0)?;
                    self.emit(Self::encode_ABx(OpCode::LoadK, step_reg as u8, const_idx as u32));
                }
                
                // Emit FORPREP
                let forprep = self.current_pc();
                self.emit(Self::encode_AsBx(OpCode::ForPrep, var_reg as u8, 0)); // Placeholder
                
                // Compile loop body
                self.inside_loop = true;
                let breaks_before = self.break_jumps.len();
                self.block(body)?;
                self.inside_loop = false;
                
                // Emit FORLOOP
                let forloop = self.current_pc();
                self.emit(Self::encode_AsBx(OpCode::ForLoop, var_reg as u8, 0)); // Placeholder
                
                // Patch jumps
                let _end = self.current_pc();
                
                // Patch FORPREP to jump to FORLOOP
                self.code[forprep] = Self::encode_AsBx(
                    OpCode::ForPrep,
                    var_reg as u8,
                    forloop as i32 - forprep as i32 - 1 // Jump to FORLOOP
                );
                
                // Patch FORLOOP to jump back to end of FORPREP
                self.code[forloop] = Self::encode_AsBx(
                    OpCode::ForLoop,
                    var_reg as u8,
                    forprep as i32 + 1 - forloop as i32 - 1 // Jump to instruction after FORPREP
                );
                
                // Patch any break statements
                self.patch_breaks(breaks_before);
                
                self.leave_scope();
            }
            
            Statement::ForInLoop { variables, iterators, body } => {
                self.enter_scope();
                
                // Need at least 3 registers for the for-in loop
                // R(base), R(base+1), R(base+2) = iterator, state, control
                let base_reg = self.registers.level();
                
                // Compile iterator expressions (usually pairs/ipairs)
                for (i, expr) in iterators.iter().enumerate() {
                    let reg = base_reg + i;
                    if reg >= self.registers.level() {
                        self.registers.allocate();
                    }
                    self.expression(expr, reg, 1)?;
                }
                
                // Allocate additional registers for loop variables
                let var_base = base_reg + 3; // First variable is at R(base+3)
                for var in variables {
                    let reg = if self.registers.level() > var_base {
                        self.registers.level()
                    } else {
                        var_base + (self.registers.level() - var_base)
                    };
                    
                    if reg >= self.registers.level() {
                        self.registers.allocate();
                    }
                    
                    // Declare the variable with proper last_use tracking
                    self.variables.insert(var.clone(), VarInfo {
                        register: reg,
                        level: self.scope_level,
                        captured: false,
                        last_use: Some(self.registers.current_instruction),
                    });
                    
                    // Mark register as containing this variable
                    self.registers.mark_register(reg, var.clone());
                }
                
                // Emit JMP to the TFORCALL instruction
                self.emit(Self::encode_AsBx(OpCode::Jmp, 0, 1)); // Skip to TFORCALL
                
                // The TFORCALL instruction will be here (Lua 5.2+).
                // In Lua 5.1, there's no TFORCALL instruction, so we simulate it with a CALL
                let loop_start = self.current_pc();
                
                // Call the iterator: R(a), R(a+1), R(a+2) = f(s, var)
                self.emit(Self::encode_ABC(
                    OpCode::Call,
                    base_reg as u8,
                    3, // 2 arguments
                    variables.len() as u16 + 1 // N results + 1
                ));
                
                // Check if iteration is done with TForLoop
                // In Lua 5.1, TForLoop combines both the iterator call and the loop check
                let _tforloop = self.current_pc();
                self.emit(Self::encode_ABC(
                    OpCode::TForLoop,
                    base_reg as u8,
                    0,
                    variables.len() as u16
                ));
                
                // Emit JMP to loop body
                self.emit(Self::encode_AsBx(OpCode::Jmp, 0, 1)); // Skip to loop body
                
                // Compile loop body
                self.inside_loop = true;
                let breaks_before = self.break_jumps.len();
                self.block(body)?;
                self.inside_loop = false;
                
                // Jump back to the call
                self.emit(Self::encode_AsBx(
                    OpCode::Jmp,
                    0,
                    loop_start as i32 - self.current_pc() as i32 - 1 // Jump to loop start
                ));
                
                // Patch any break statements
                self.patch_breaks(breaks_before);
                
                self.leave_scope();
            }
            
            Statement::FunctionDefinition { name, parameters, is_vararg, body } => {
                // Compile function body
                let proto = self.compile_function(parameters, *is_vararg, body)?;
                
                // Create function prototype
                let proto_idx = self.add_proto(proto)?;
                
                // Allocate register for function
                let func_reg = self.registers.allocate();
                
                // Create closure
                self.emit(Self::encode_ABx(OpCode::Closure, func_reg as u8, proto_idx as u32));
                
                // Set function name
                self.assign_function_name(name, func_reg)?;
                
                self.registers.free_to(func_reg);
            }
            
            Statement::LocalFunctionDefinition { name, parameters, is_vararg, body } => {
                // Declare the function name first
                self.declare_local(name.clone())?;
                
                // Find the register
                let func_reg = if let Some(var) = self.variables.get(name) {
                    var.register
                } else {
                    // This should never happen
                    return Err(LuaError::InternalError(
                        "Failed to find local function register".to_string()
                    ));
                };
                
                // Compile function body
                let proto = self.compile_function(parameters, *is_vararg, body)?;
                
                // Create function prototype
                let proto_idx = self.add_proto(proto)?;
                
                // Create closure
                self.emit(Self::encode_ABx(OpCode::Closure, func_reg as u8, proto_idx as u32));
            }
            
            _ => unreachable!("Statement type should be handled in main statement method"),
        }
        
        Ok(())
    }
    
    /// Compile a block of statements
    fn block(&mut self, block: &Block) -> LuaResult<()> {
        for stmt in &block.statements {
            self.statement(stmt)?;
        }
        
        Ok(())
    }
    
    /// Compile an assignment statement
    fn assignment(&mut self, assignment: &Assignment) -> LuaResult<()> {
        // Special case for simple assignment
        if assignment.variables.len() == 1 && assignment.expressions.len() == 1 {
            let saved_state = self.registers.save_state();
            
            // Allocate a register for the value
            let value_reg = self.registers.allocate();
            
            // Generate code for the expression
            self.expression(&assignment.expressions[0], value_reg, 1)?;
            
            // Assign to the variable
            self.assign_to_variable(&assignment.variables[0], value_reg)?;
            
            self.registers.restore_state(saved_state);
            return Ok(());
        }
        
        // General case: multiple variables or expressions
        // In Lua, we evaluate all expressions before any assignments
        
        let saved_state = self.registers.save_state();
        
        let num_exprs = assignment.expressions.len();
        
        // Allocate registers for all expression results
        let mut expr_regs = Vec::with_capacity(num_exprs);
        for _ in 0..num_exprs {
            expr_regs.push(self.registers.allocate());
        }
        
        // Evaluate each expression in its own scope
        for (i, expr) in assignment.expressions.iter().enumerate() {
            let want = if i == num_exprs - 1 && num_exprs < assignment.variables.len() {
                // Last expression might produce multiple values
                0
            } else {
                1
            };
            
            let expr_state = self.registers.save_state();
            
            // Evaluate expression
            self.expression(expr, expr_regs[i], want)?;
            
            self.registers.restore_state(expr_state);
        }
        
        // Perform assignments
        for (i, var) in assignment.variables.iter().enumerate() {
            if i < num_exprs {
                // Direct assignment from expression result
                self.assign_to_variable(var, expr_regs[i])?;
            } else {
                // Assign nil for missing expressions
                let nil_reg = self.registers.allocate();
                self.emit(Self::encode_ABC(OpCode::LoadNil, nil_reg as u8, 0, 0));
                self.assign_to_variable(var, nil_reg)?;
            }
        }
        
        self.registers.restore_state(saved_state);
        
        Ok(())
    }
    
    /// Compile a local declaration
    fn local_declaration(&mut self, decl: &LocalDeclaration) -> LuaResult<()> {
        // Special case for simple declaration
        if decl.names.len() == 1 && decl.expressions.len() == 1 {
            // Allocate a register for the value
            let var_reg = self.registers.allocate();
            
            // Generate code for the expression
            self.expression(&decl.expressions[0], var_reg, 1)?;
            
            // Declare the variable using the same register
            self.variables.insert(decl.names[0].clone(), VarInfo {
                register: var_reg,
                level: self.scope_level,
                captured: false,
                last_use: Some(self.registers.current_instruction),
            });
            
            // Mark register as containing this variable
            self.registers.mark_register(var_reg, decl.names[0].clone());
            
            return Ok(());
        }
        
        // Evaluate all expressions first
        let expr_base = self.registers.level();
        let num_exprs = decl.expressions.len();
        
        for (i, expr) in decl.expressions.iter().enumerate() {
            let want = if i == num_exprs - 1 && num_exprs < decl.names.len() {
                // Last expression might produce multiple values
                0
            } else {
                1
            };
            
            let expr_reg = self.registers.allocate();
            self.expression(expr, expr_reg, want)?;
        }
        
        // Declare variables and copy values
        for (i, name) in decl.names.iter().enumerate() {
            let var_reg = self.registers.allocate();

            if i < num_exprs {
                // Copy from expression registers
                if expr_base + i != var_reg {
                    self.emit(Self::encode_ABC(OpCode::Move, var_reg as u8, (expr_base + i) as u16, 0));
                }
            } else {
                // Not enough expressions, initialize with nil
                self.emit(Self::encode_ABC(OpCode::LoadNil, var_reg as u8, 0, 0));
            }
            
            // Declare the variable with proper last_use tracking
            self.variables.insert(name.clone(), VarInfo {
                register: var_reg,
                level: self.scope_level,
                captured: false,
                last_use: Some(self.registers.current_instruction),
            });
            
            // Mark register as containing this variable
            self.registers.mark_register(var_reg, name.clone());
        }
        
        Ok(())
    }
    
    fn expression(&mut self, expr: &Expression, target: usize, want: usize) -> LuaResult<()> {
        match expr {
            Expression::Nil => {
                self.emit(Self::encode_ABC(OpCode::LoadNil, target as u8, 0, 0));
            }
            Expression::Boolean(b) => {
                self.emit(Self::encode_ABC(OpCode::LoadBool, target as u8, *b as u16, 0));
            }
            Expression::Number(n) => {
                let const_idx = self.add_number_constant(*n)?;
                self.emit(Self::encode_ABx(OpCode::LoadK, target as u8, const_idx as u32));
            }
            Expression::String(s) => {
                let const_idx = self.add_string_constant(s)?;
                self.emit(Self::encode_ABx(OpCode::LoadK, target as u8, const_idx as u32));
            }
            Expression::VarArg => {
                self.emit(Self::encode_ABC(OpCode::VarArg, target as u8, 0, want as u16));
            }
            Expression::FunctionDef { parameters, is_vararg, body } => {
                let saved_state = self.registers.save_state();
                
                // Compile function
                let proto = self.compile_function(parameters, *is_vararg, body)?;
                
                // Add to prototypes
                let proto_idx = self.add_proto(proto)?;
                
                // Create closure
                self.emit(Self::encode_ABx(OpCode::Closure, target as u8, proto_idx as u32));
                
                self.registers.restore_state(saved_state);
            }
            Expression::TableConstructor(table) => {
                // Table constructor has its own scoped allocation
                self.table_constructor(table, target)?;
            }
            Expression::BinaryOp { left, operator, right } => {
                self.binary_op(*operator, left, right, target)?;
            }
            Expression::UnaryOp { operator, operand } => {
                let saved_state = self.registers.save_state();
                
                // Allocate register for operand
                let op_reg = self.registers.allocate();
                self.expression(operand, op_reg, 1)?;
                
                // Preserve the operand register
                self.registers.preserve_register(op_reg);
                
                // Generate unary operation
                match operator {
                    UnaryOperator::Not => {
                        self.emit(Self::encode_ABC(OpCode::Not, target as u8, op_reg as u16, 0));
                    }
                    UnaryOperator::Minus => {
                        self.emit(Self::encode_ABC(OpCode::Unm, target as u8, op_reg as u16, 0));
                    }
                    UnaryOperator::Length => {
                        self.emit(Self::encode_ABC(OpCode::Len, target as u8, op_reg as u16, 0));
                    }
                }
                
                self.registers.restore_state(saved_state);
                
                // Clear the preserved flag for this register
                self.registers.preserved_registers.remove(&op_reg);
            }
            Expression::Variable(var) => {
                self.compile_variable(var, target)?;
            }
            Expression::FunctionCall(call) => {
                // Function call has its own scoped allocation
                self.function_call(call, target, want)?;
            }
        }
        
        Ok(())
    }

    fn binary_op(
        &mut self,
        op: BinaryOperator,
        left: &Expression,
        right: &Expression,
        target: usize,
    ) -> LuaResult<()> {
        match op {
            BinaryOperator::And | BinaryOperator::Or => {
                // Logic operations have special execution patterns, not changed
                if op == BinaryOperator::And {
                    // Evaluate left operand
                    self.expression(left, target, 1)?;
                    
                    // Test and skip right operand if false
                    self.emit(Self::encode_ABC(OpCode::Test, target as u8, 0, 0));
                    let jmp = self.current_pc();
                    self.emit(Self::encode_AsBx(OpCode::Jmp, 0, 0)); // Placeholder
                    
                    // Evaluate right operand into same register
                    self.expression(right, target, 1)?;
                    
                    // Patch jump
                    let end = self.current_pc();
                    self.code[jmp] = Self::encode_AsBx(
                        OpCode::Jmp,
                        0,
                        end as i32 - jmp as i32 - 1 // Relative offset
                    );
                } else { // Or
                    // Evaluate left operand
                    self.expression(left, target, 1)?;
                    
                    // Test and skip right operand if true
                    self.emit(Self::encode_ABC(OpCode::Test, target as u8, 0, 1));
                    let jmp = self.current_pc();
                    self.emit(Self::encode_AsBx(OpCode::Jmp, 0, 0)); // Placeholder
                    
                    // Evaluate right operand into same register
                    self.expression(right, target, 1)?;
                    
                    // Patch jump
                    let end = self.current_pc();
                    self.code[jmp] = Self::encode_AsBx(
                        OpCode::Jmp,
                        0,
                        end as i32 - jmp as i32 - 1 // Relative offset
                    );
                }
            },
            _ => {
                // Arithmetic, concatenation, or comparison operations
                
                let saved_state = self.registers.save_state();
                
                // Allocate registers for operands
                let left_reg = self.registers.allocate();
                self.expression(left, left_reg, 1)?;
                
                let right_reg = self.registers.allocate();
                self.expression(right, right_reg, 1)?;
                
                // Mark the operand registers as preserved
                // This ensures they're not freed during restore_state()
                self.registers.preserve_register(left_reg);
                self.registers.preserve_register(right_reg);
                
                // Generate the specific opcode based on operation type
                match op {
                    BinaryOperator::Add => {
                        self.emit(Self::encode_ABC(OpCode::Add, target as u8, left_reg as u16, right_reg as u16));
                    },
                    BinaryOperator::Sub => {
                        self.emit(Self::encode_ABC(OpCode::Sub, target as u8, left_reg as u16, right_reg as u16));
                    },
                    BinaryOperator::Mul => {
                        self.emit(Self::encode_ABC(OpCode::Mul, target as u8, left_reg as u16, right_reg as u16));
                    },
                    BinaryOperator::Div => {
                        self.emit(Self::encode_ABC(OpCode::Div, target as u8, left_reg as u16, right_reg as u16));
                    },
                    BinaryOperator::Mod => {
                        self.emit(Self::encode_ABC(OpCode::Mod, target as u8, left_reg as u16, right_reg as u16));
                    },
                    BinaryOperator::Pow => {
                        self.emit(Self::encode_ABC(OpCode::Pow, target as u8, left_reg as u16, right_reg as u16));
                    },
                    BinaryOperator::Concat => {
                        self.emit(Self::encode_ABC(OpCode::Concat, target as u8, left_reg as u16, right_reg as u16));
                    },
                    BinaryOperator::Eq | BinaryOperator::Ne | BinaryOperator::Lt | 
                    BinaryOperator::Le | BinaryOperator::Gt | BinaryOperator::Ge => {
                        // Comparisons need special handling
                        let cmp_op = match op {
                            BinaryOperator::Eq => OpCode::Eq,
                            BinaryOperator::Ne => OpCode::Eq, // Invert test
                            BinaryOperator::Lt => OpCode::Lt,
                            BinaryOperator::Gt => OpCode::Lt, // Swap operands
                            BinaryOperator::Le => OpCode::Le,
                            BinaryOperator::Ge => OpCode::Le, // Swap operands
                            _ => unreachable!(),
                        };
                        
                        let invert = matches!(op, BinaryOperator::Ne);
                        let swap = matches!(op, BinaryOperator::Gt | BinaryOperator::Ge);
                        
                        let a = if invert { 1 } else { 0 };
                        let b = if swap { right_reg } else { left_reg };
                        let c = if swap { left_reg } else { right_reg };
                        
                        // Load true initially
                        self.emit(Self::encode_ABC(OpCode::LoadBool, target as u8, 1, 0));
                        
                        // Compare and skip next instruction if false
                        self.emit(Self::encode_ABC(cmp_op, a as u8, b as u16, c as u16));
                        self.emit(Self::encode_AsBx(OpCode::Jmp, 0, 1));
                        
                        // Load false if comparison failed
                        self.emit(Self::encode_ABC(OpCode::LoadBool, target as u8, 0, 0));
                    },
                    _ => return Err(LuaError::CompileError("Invalid binary operator".to_string())),
                }
                
                // Restore state with preserved registers
                self.registers.restore_state(saved_state);
            }
        }
        
        Ok(())
    }
    

    
    fn compile_variable(&mut self, var: &Variable, target: usize) -> LuaResult<()> {
        match var {
            Variable::Name(name) => {
                // Check for local variable - pattern match in a way that avoids double mutable borrows
                let var_info = self.resolve_variable_and_clone(name);
                
                if let Some(v) = var_info {
                    self.emit(Self::encode_ABC(OpCode::Move, target as u8, v.register as u16, 0));
                    return Ok(());
                }
                
                // Check for upvalue
                if let Some(upvalue_idx) = self.find_or_create_upvalue(name) {
                    self.emit(Self::encode_ABC(OpCode::GetUpval, target as u8, upvalue_idx as u16, 0));
                    return Ok(());
                }
                
                // Global variable
                let key_idx = self.add_string_constant(name)?;
                self.emit(Self::encode_ABx(OpCode::GetGlobal, target as u8, key_idx as u32));
            }
            Variable::Index { table, key } => {
                let saved_state = self.registers.save_state();
                
                let table_reg = self.registers.allocate();
                self.expression(table, table_reg, 1)?;
                
                // Preserve the table register
                self.registers.preserve_register(table_reg);
                
                let key_reg = self.registers.allocate();
                self.expression(key, key_reg, 1)?;
                
                // Preserve the key register
                self.registers.preserve_register(key_reg);
                
                self.emit(Self::encode_ABC(OpCode::GetTable, target as u8, table_reg as u16, key_reg as u16));
                
                self.registers.restore_state(saved_state);
                
                // Clear preservation flags for these registers
                self.registers.preserved_registers.remove(&table_reg);
                self.registers.preserved_registers.remove(&key_reg);
            }
            Variable::Member { table, field } => {
                let saved_state = self.registers.save_state();
                
                let table_reg = self.registers.allocate();
                self.expression(table, table_reg, 1)?;
                
                // Preserve the table register
                self.registers.preserve_register(table_reg);
                
                let key_idx = self.add_string_constant(field)?;
                let key_const = key_idx | 0x100; // Mark as constant (high bit set)
                
                self.emit(Self::encode_ABC(
                    OpCode::GetTable, 
                    target as u8, 
                    table_reg as u16,
                    key_const as u16
                ));
                
                self.registers.restore_state(saved_state);
                
                // Clear preservation flag for the table register
                self.registers.preserved_registers.remove(&table_reg);
            }
        }
        
        Ok(())
    }
    
    fn assign_to_variable(&mut self, var: &Variable, value_reg: usize) -> LuaResult<()> {
        // Preserve the value register for the entire assignment
        self.registers.preserve_register(value_reg);
        
        match var {
            Variable::Name(name) => {
                // Check for local - pattern match in a way that avoids double mutable borrows
                let var_info = self.resolve_variable_and_clone(name);
                
                if let Some(v) = var_info {
                    self.emit(Self::encode_ABC(OpCode::Move, v.register as u8, value_reg as u16, 0));
                    return Ok(());
                }
                
                // Check for upvalue
                if let Some(upvalue_idx) = self.find_or_create_upvalue(name) {
                    self.emit(Self::encode_ABC(OpCode::SetUpval, value_reg as u8, upvalue_idx as u16, 0));
                    return Ok(());
                }
                
                // Global variable
                let key_idx = self.add_string_constant(name)?;
                self.emit(Self::encode_ABx(OpCode::SetGlobal, value_reg as u8, key_idx as u32));
            }
            Variable::Index { table, key } => {
                let saved_state = self.registers.save_state();
                
                let table_reg = self.registers.allocate();
                self.expression(table, table_reg, 1)?;
                
                // Preserve the table register
                self.registers.preserve_register(table_reg);
                
                let key_reg = self.registers.allocate();
                self.expression(key, key_reg, 1)?;
                
                // Preserve the key register
                self.registers.preserve_register(key_reg);
                
                self.emit(Self::encode_ABC(OpCode::SetTable, table_reg as u8, key_reg as u16, value_reg as u16));
                
                self.registers.restore_state(saved_state);
                
                // Clear preservation flags
                self.registers.preserved_registers.remove(&table_reg);
                self.registers.preserved_registers.remove(&key_reg);
            }
            Variable::Member { table, field } => {
                let saved_state = self.registers.save_state();
                
                let table_reg = self.registers.allocate();
                self.expression(table, table_reg, 1)?;
                
                // Preserve the table register
                self.registers.preserve_register(table_reg);
                
                let key_idx = self.add_string_constant(field)?;
                let key_const = key_idx | 0x100; // Mark as constant
                
                self.emit(Self::encode_ABC(
                    OpCode::SetTable, 
                    table_reg as u8, 
                    key_const as u16,
                    value_reg as u16
                ));
                
                self.registers.restore_state(saved_state);
                
                // Clear preservation flag
                self.registers.preserved_registers.remove(&table_reg);
            }
        }
        
        // Clear preservation flag for the value register
        self.registers.preserved_registers.remove(&value_reg);
        
        Ok(())
    }
    
    fn function_call(
        &mut self,
        call: &FunctionCall,
        target: usize,
        want: usize,
    ) -> LuaResult<()> {
        // Save the allocation state for later restoration
        let saved_state = self.registers.save_state();
        
        // Allocate register for function and mark it preserved
        let func_reg = self.registers.allocate();
        self.registers.preserve_register(func_reg);
        
        // Handle method calls (:)
        if let Some(method) = &call.method {
            if let Expression::Variable(Variable::Name(obj_name)) = &call.function {
                // Simple case: x:method(...)
                // First resolve the variable without holding a borrow on self
                let var_info = self.resolve_variable_and_clone(obj_name);
                
                if let Some(v) = var_info {
                    // Get method from object
                    let method_idx = self.add_string_constant(method)?;
                    
                    // Now emit the instruction
                    self.emit(Self::encode_ABC(
                        OpCode::Self_,
                        func_reg as u8,
                        v.register as u16,
                        (method_idx | 0x100) as u16 // Mark as constant
                    ));
                } else {
                    // Global
                    let obj_reg = self.registers.allocate();
                    let obj_idx = self.add_string_constant(obj_name)?;
                    self.emit(Self::encode_ABx(OpCode::GetGlobal, obj_reg as u8, obj_idx as u32));
                    
                    let method_idx = self.add_string_constant(method)?;
                    self.emit(Self::encode_ABC(
                        OpCode::Self_,
                        func_reg as u8,
                        obj_reg as u16,
                        (method_idx | 0x100) as u16 // Mark as constant
                    ));
                }
            } else {
                // Complex case: expr:method(...)
                let obj_reg = self.registers.allocate();
                
                // Compile with proper scoping
                let obj_state = self.registers.save_state();
                self.expression(&call.function, obj_reg, 1)?;
                
                // Preserve the object register
                self.registers.preserve_register(obj_reg);
                self.registers.restore_state(obj_state);
                
                let method_idx = self.add_string_constant(method)?;
                self.emit(Self::encode_ABC(
                    OpCode::Self_,
                    func_reg as u8,
                    obj_reg as u16,
                    (method_idx | 0x100) as u16 // Mark as constant
                ));
            }
        } else {
            // Regular function call
            let func_state = self.registers.save_state();
            self.expression(&call.function, func_reg, 1)?;
            
            // Preserve the function register
            // This ensures it won't be overwritten during argument evaluation
            self.registers.preserve_register(func_reg);
            self.registers.restore_state(func_state);
        }
        
        // Process arguments
        let mut arg_regs = Vec::new();
        let num_args = match &call.args {
            CallArgs::Args(args) => {
                for arg in args {
                    // Allocate a NEW register for each argument
                    // Don't reuse the function register
                    let arg_reg = self.registers.allocate();
                    
                    if arg_reg == func_reg {
                        // This should never happen since func_reg is preserved,
                        // but just to be extra safe
                        println!("WARNING: Argument register {} conflicts with function register", arg_reg);
                        continue;
                    }
                    
                    arg_regs.push(arg_reg);
                    
                    // Save state before compiling expression
                    let arg_state = self.registers.save_state();
                    
                    // Mark function register as preserved before any subexpression
                    self.registers.preserve_register(func_reg);
                    
                    // Compile expression
                    self.expression(arg, arg_reg, 1)?;
                    
                    // Restore state, preserving function and this argument register
                    self.registers.preserve_register(arg_reg);
                    self.registers.restore_state(arg_state);
                }
                
                args.len()
            },
            CallArgs::Table(table) => {
                let arg_reg = self.registers.allocate();
                arg_regs.push(arg_reg);
                
                // Save state before compiling
                let table_state = self.registers.save_state();
                
                // Ensure function register is preserved
                self.registers.preserve_register(func_reg);
                
                // Compile table constructor
                self.table_constructor(table, arg_reg)?;
                
                // Restore state, preserving both function and argument registers
                self.registers.preserve_register(arg_reg);
                self.registers.restore_state(table_state);
                
                1
            },
            CallArgs::String(s) => {
                let arg_reg = self.registers.allocate();
                arg_regs.push(arg_reg);
                let k = self.add_string_constant(s)?;
                self.emit(Self::encode_ABx(OpCode::LoadK, arg_reg as u8, k as u32));
                1
            }
        };
        
        // Emit CALL instruction
        let b = if num_args == 0 { 1 } else { num_args + 1 }; // +1 for function itself
        let c = if want == 0 { 1 } else { want + 1 }; // +1 for Lua's 1-based results
        
        self.emit(Self::encode_ABC(OpCode::Call, func_reg as u8, b as u16, c as u16));
        
        // Move result to target if needed
        if target != func_reg {
            for i in 0..want {
                if target + i < func_reg + want {
                    self.emit(Self::encode_ABC(OpCode::Move, (target + i) as u8, (func_reg + i) as u16, 0));
                }
            }
        }
        
        // Now we can restore state without preserving registers
        // The CALL and MOVE instructions have been emitted
        self.registers.clear_preserved();  // Clear the preserved set
        self.registers.restore_state(saved_state);
        
        Ok(())
    }
    
    fn table_constructor(&mut self, table: &TableConstructor, target: usize) -> LuaResult<()> {
        let saved_state = self.registers.save_state();
        
        // Estimate array and hash sizes
        let array_size = table.fields.iter()
            .filter(|f| matches!(f, TableField::List(_)))
            .count();
            
        let hash_size = table.fields.iter()
            .filter(|f| !matches!(f, TableField::List(_)))
            .count();
        
        // Create table
        self.emit(Self::encode_ABC(OpCode::NewTable, target as u8, array_size as u16, hash_size as u16));
        
        // Extract all list fields for array part processing
        let list_fields: Vec<&Expression> = table.fields.iter()
            .filter_map(|field| {
                if let TableField::List(expr) = field {
                    Some(expr)
                } else {
                    None
                }
            })
            .collect();
        
        // Process list fields in batches of FIELDS_PER_FLUSH (50)
        const FIELDS_PER_FLUSH: usize = 50;
        let mut idx = 0;
        while idx < list_fields.len() {
            let end = std::cmp::min(idx + FIELDS_PER_FLUSH, list_fields.len());
            let batch = &list_fields[idx..end];
            
            // Process this batch
            process_array_batch(self, target, batch, idx + 1)?;
            
            idx = end;
        }
        
        // Process hash fields (record and indexed fields)
        for field in &table.fields {
            match field {
                TableField::Record { key, value } => {
                    let expr_state = self.registers.save_state();
                    
                    let key_idx = self.add_string_constant(key)?;
                    let val_reg = self.registers.allocate();
                    
                    self.expression(value, val_reg, 1)?;
                    
                    // Preserve the value register
                    self.registers.preserve_register(val_reg);
                    
                    self.emit(Self::encode_ABC(
                        OpCode::SetTable,
                        target as u8,
                        (key_idx | 0x100) as u16, // Mark as constant
                        val_reg as u16
                    ));
                    
                    self.registers.restore_state(expr_state);
                },
                TableField::Index { key, value } => {
                    let expr_state = self.registers.save_state();
                    
                    let key_reg = self.registers.allocate();
                    self.expression(key, key_reg, 1)?;
                    
                    // Preserve key register
                    self.registers.preserve_register(key_reg);
                    
                    let val_reg = self.registers.allocate();
                    self.expression(value, val_reg, 1)?;
                    
                    // Preserve value register
                    self.registers.preserve_register(val_reg);
                    
                    self.emit(Self::encode_ABC(
                        OpCode::SetTable,
                        target as u8,
                        key_reg as u16,
                        val_reg as u16
                    ));
                    
                    self.registers.restore_state(expr_state);
                },
                TableField::List(_) => {
                    // Already processed in batches
                },
            }
        }
        
        // Clean up all preserved registers from this table constructor
        self.registers.clear_preserved();
        self.registers.restore_state(saved_state);
        
        Ok(())
    }
    
    /// Emit a return instruction with expressions
    fn emit_return(&mut self, exprs: &[Expression]) -> LuaResult<()> {
        let start_reg = self.registers.level();
        
        println!("DEBUG COMPILER_EMIT_RETURN: Start with {} expressions at register level {}", 
                 exprs.len(), start_reg);
        
        if exprs.is_empty() {
            // Return no values - RETURN R(0), 1, 0 means return 0 values
            println!("DEBUG COMPILER_EMIT_RETURN: Empty return - encoding RETURN R(0), 1, 0");
            self.emit(Self::encode_ABC(OpCode::Return, 0, 1, 0));
        } else {
            // Result registers start at the current level
            for (i, expr) in exprs.iter().enumerate() {
                let want = if i == exprs.len() - 1 {
                    0 // Last expression can return multiple values
                } else {
                    1
                };
                
                // Allocate a register for this expression
                let reg = self.registers.allocate();
                println!("DEBUG COMPILER_EMIT_RETURN: Allocated register {} for expression {}", reg, i);
                
                // Compile expression to the allocated register
                self.expression(expr, reg, want)?;
            }
            
            // Calculate how many values to return
            let first = start_reg;
            let count = self.registers.level() - start_reg;
            
            // CRITICAL FIX: Ensure we're returning at least one value if expressions exist
            // If count is 0 (no registers allocated), force it to 1
            let actual_count = if count == 0 && !exprs.is_empty() {
                println!("DEBUG COMPILER_EMIT_RETURN: No registers allocated but have expressions, forcing count=1");
                1
            } else {
                count
            };
            
            // Calculate B parameter (B=1 means return 0 values, B=2 means return 1 value)
            let b_param = actual_count + 1;
            
            println!("DEBUG COMPILER_EMIT_RETURN: Generated values - first_reg: {}, count: {}, actual_count: {}, B param: {}", 
                    first, count, actual_count, b_param);
            println!("DEBUG COMPILER_EMIT_RETURN: Encoding RETURN R({}), {}, 0", first, b_param);
            
            let instruction = Self::encode_ABC(OpCode::Return, first as u8, b_param as u16, 0);
            println!("DEBUG COMPILER_EMIT_RETURN: Final instruction: 0x{:08x}", instruction);
            println!("DEBUG COMPILER_EMIT_RETURN: Parsed back A={}, B={}, C={}", 
                     Instruction(instruction).a(), 
                     Instruction(instruction).b(), 
                     Instruction(instruction).c());
            
            self.emit(instruction);
            
            // Free the temporary registers used for return values
            self.registers.free_to(start_reg);
        }
        
        Ok(())
    }
    
    // Helper methods
    
    fn compile_function(
        &mut self,
        params: &[String],
        is_vararg: bool,
        body: &Block,
    ) -> LuaResult<CompiledFunction> {
        // Create a child generator
        let mut child = self.child();
        
        // Set parent reference
        child.parent = Some(Box::new(CodeGenerator::new()));
        
        // Declare parameters
        for param in params {
            child.declare_local(param.clone())?;
        }
        
        // Generate body
        child.block(body)?;
        
        // Add final return if not present
        if !child.has_return() {
            child.emit(Self::encode_ABC(OpCode::Return, 0, 1, 0));
        }
        
        Ok(CompiledFunction {
            bytecode: child.code,
            constants: child.constants,
            num_params: params.len() as u8,
            is_vararg,
            max_stack_size: child.registers.max_used as u8,
            upvalues: child.upvalues,
            prototypes: Vec::new(),
        })
    }
    
    fn add_proto(&mut self, proto: CompiledFunction) -> LuaResult<usize> {
        let index = self.prototypes.len();
        println!("DEBUG: Adding prototype {} with {} bytecode instructions", index, proto.bytecode.len());
        self.prototypes.push(proto);
        
        // Create a constant reference to this proto
        let const_index = self.constants.len();
        self.constants.push(CompilationConstant::FunctionProto(index));
        
        // Add to constant map
        self.constant_map.insert(ConstantKey::FunctionProto(index), const_index);
        
        println!("DEBUG: Created FunctionProto constant at index {}, referencing prototype {}", const_index, index);
        
        Ok(const_index)
    }
    
    fn has_return(&self) -> bool {
        if let Some(&last) = self.code.last() {
            let instr = Instruction(last);
            matches!(instr.opcode(), OpCode::Return)
        } else {
            false
        }
    }
    
    fn resolve_gotos(&mut self, label: &str) -> LuaResult<()> {
        let mut i = 0;
        while i < self.pending_gotos.len() {
            if self.pending_gotos[i].0 == label {
                // Extract information from the PendingGoto before removing it
                let goto_pc = self.pending_gotos[i].1;
                let goto_level = self.pending_gotos[i].2;
                
                // Remove the pending goto
                self.pending_gotos.remove(i);
                
                // Get label information
                let label_info = self.labels.get(label).unwrap();
                
                // Check scope rules
                if label_info.level > goto_level {
                    return Err(LuaError::CompileError(
                        format!("Goto to label '{}' jumps into scope", label)
                    ));
                }
                
                // Calculate offset with overflow check
                let offset = match (label_info.pc as i64).checked_sub(goto_pc as i64) {
                    Some(diff) => diff - 1,  // Safe subtraction
                    None => {
                        return Err(LuaError::CompileError(
                            format!("Jump offset too large from PC {} to label '{}' at PC {}", 
                                    goto_pc, label, label_info.pc)
                        ));
                    }
                };
                
                // Check if offset is within the allowed range for AsBx encoding
                if offset < -131071 || offset > 131070 {
                    return Err(LuaError::CompileError(
                        format!("Jump offset {} out of range (must be between -131071 and 131070)", offset)
                    ));
                }
                
                // Encode the instruction
                self.code[goto_pc] = Self::encode_AsBx(OpCode::Jmp, 0, offset as i32);
            } else {
                i += 1;
            }
        }
        
        Ok(())
    }
    
    fn patch_breaks(&mut self, start_idx: usize) {
        let end_pc = self.current_pc();
        
        for i in start_idx..self.break_jumps.len() {
            let break_jump = &self.break_jumps[i];
            
            // Calculate offset with overflow check
            let offset = match (end_pc as i64).checked_sub(break_jump.pc as i64) {
                Some(diff) => (diff - 1) as i32,  // Safe subtraction
                None => {
                    // This shouldn't happen in practice, but handle it gracefully
                    println!("Warning: Break jump offset overflow detected, using maximum offset");
                    131070  // Use max positive offset
                }
            };
            
            // Check if offset is within the allowed range for AsBx encoding
            if offset < -131071 || offset > 131070 {
                println!("Warning: Break jump offset {} out of range, clamping to valid range", offset);
                let clamped_offset = if offset < 0 { -131071 } else { 131070 };
                self.code[break_jump.pc] = Self::encode_AsBx(OpCode::Jmp, 0, clamped_offset);
            } else {
                self.code[break_jump.pc] = Self::encode_AsBx(OpCode::Jmp, 0, offset);
            }
        }
        
        self.break_jumps.truncate(start_idx);
    }

    fn assign_function_name(&mut self, name: &FunctionName, func_reg: usize) -> LuaResult<()> {
        let name_path = &name.names;
        
        if name_path.len() == 1 {
            // Simple assignment
            let var = Variable::Name(name_path[0].clone());
            self.assign_to_variable(&var, func_reg)?;
        } else {
            // Complex path: a.b.c.d
            let base_name = &name_path[0];
            let base_reg = self.registers.allocate();
            
            // Get base object
            if let Some(v) = self.resolve_variable_and_clone(base_name) {
                self.emit(Self::encode_ABC(OpCode::Move, base_reg as u8, v.register as u16, 0));
            } else {
                // Global
                let const_idx = self.add_string_constant(base_name)?;
                self.emit(Self::encode_ABx(OpCode::GetGlobal, base_reg as u8, const_idx as u32));
            }
            
            // Navigate the path (a.b.c)
            let mut current_reg = base_reg;
            for i in 1..name_path.len() - 1 {
                let field = &name_path[i];
                let field_idx = self.add_string_constant(field)?;
                
                let next_reg = if i == name_path.len() - 2 {
                    // Last table before final field, reuse register
                    current_reg
                } else {
                    self.registers.allocate()
                };
                
                self.emit(Self::encode_ABC(
                    OpCode::GetTable,
                    next_reg as u8,
                    current_reg as u16,
                    (field_idx | 0x100) as u16 // Mark as constant
                ));
                
                current_reg = next_reg;
            }
            
            // Finally, set the function
            let last_field = &name_path[name_path.len() - 1];
            let field_idx = self.add_string_constant(last_field)?;
            
            self.emit(Self::encode_ABC(
                OpCode::SetTable,
                current_reg as u8,
                (field_idx | 0x100) as u16, // Mark as constant
                func_reg as u16
            ));
            
            self.registers.free_to(base_reg);
        }
        
        Ok(())
    }
}

/// Instruction wrapper
#[derive(Debug, Clone, Copy)]
struct Instruction(pub u32);

impl Instruction {
    /// Get the opcode
    pub fn opcode(&self) -> OpCode {
        let opcode_num = ((self.0) & 0x3F) as u8;
        u8_to_opcode(opcode_num)
    }
    
    /// Get the A field
    pub fn a(&self) -> u8 {
        ((self.0 >> 6) & 0xFF) as u8
    }
    
    /// Get the B field
    pub fn b(&self) -> u16 {
        ((self.0 >> 23) & 0x1FF) as u16
    }
    
    /// Get the C field
    pub fn c(&self) -> u16 {
        ((self.0 >> 14) & 0x1FF) as u16
    }
}

/// Complete compilation result with all necessary data
#[derive(Debug, Clone, PartialEq)]
pub struct CompleteCompilationOutput {
    /// Main function
    pub main: CompiledFunction,
    
    /// String table
    pub strings: Vec<String>,
}

/// Compile an AST into bytecode
pub fn generate_bytecode(chunk: &Chunk) -> LuaResult<CompleteCompilationOutput> {
    let generator = CodeGenerator::new();
    
    // Generate bytecode (consumes the generator but returns a CompiledFunction with all prototypes)
    let (main, strings) = generator.generate(chunk)?;
    
    println!("DEBUG GENERATOR: Final module has {} prototypes", main.prototypes.len());
    
    // Return all necessary data
    Ok(CompleteCompilationOutput {
        main,
        strings,
    })
}

// Opcode conversion for VM/Debug use
pub fn opcode_to_u8(op: OpCode) -> u8 {
    match op {
        OpCode::Move => 0,
        OpCode::LoadK => 1,
        OpCode::LoadBool => 2,
        OpCode::LoadNil => 3,
        OpCode::GetUpval => 4,
        OpCode::GetGlobal => 5,
        OpCode::SetGlobal => 6,
        OpCode::SetUpval => 7,
        OpCode::GetTable => 8,
        OpCode::SetTable => 9,
        OpCode::NewTable => 10,
        OpCode::Self_ => 11,
        OpCode::Add => 12,
        OpCode::Sub => 13,
        OpCode::Mul => 14,
        OpCode::Div => 15,
        OpCode::Mod => 16,
        OpCode::Pow => 17,
        OpCode::Unm => 18,
        OpCode::Not => 19,
        OpCode::Len => 20,
        OpCode::Concat => 21,
        OpCode::Jmp => 22,
        OpCode::Eq => 23,
        OpCode::Lt => 24,
        OpCode::Le => 25,
        OpCode::Test => 26,
        OpCode::TestSet => 27,
        OpCode::Call => 28,
        OpCode::TailCall => 29,
        OpCode::Return => 30,
        OpCode::ForPrep => 31,
        OpCode::ForLoop => 32,
        OpCode::TForLoop => 33,
        OpCode::SetList => 34,
        OpCode::VarArg => 35,
        OpCode::Closure => 36,
        OpCode::Close => 37,
        OpCode::ExtraArg => 38,
    }
}

// Opcode conversion for debug/display
pub fn u8_to_opcode(value: u8) -> OpCode {
    match value {
        0 => OpCode::Move,
        1 => OpCode::LoadK,
        2 => OpCode::LoadBool,
        3 => OpCode::LoadNil,
        4 => OpCode::GetUpval,
        5 => OpCode::GetGlobal,
        6 => OpCode::SetGlobal,
        7 => OpCode::SetUpval,
        8 => OpCode::GetTable,
        9 => OpCode::SetTable,
        10 => OpCode::NewTable,
        11 => OpCode::Self_,
        12 => OpCode::Add,
        13 => OpCode::Sub,
        14 => OpCode::Mul,
        15 => OpCode::Div,
        16 => OpCode::Mod,
        17 => OpCode::Pow,
        18 => OpCode::Unm,
        19 => OpCode::Not,
        20 => OpCode::Len,
        21 => OpCode::Concat,
        22 => OpCode::Jmp,
        23 => OpCode::Eq,
        24 => OpCode::Lt,
        25 => OpCode::Le,
        26 => OpCode::Test,
        27 => OpCode::TestSet,
        28 => OpCode::Call,
        29 => OpCode::TailCall,
        30 => OpCode::Return,
        31 => OpCode::ForPrep,
        32 => OpCode::ForLoop,
        33 => OpCode::TForLoop,
        34 => OpCode::SetList,
        35 => OpCode::VarArg,
        36 => OpCode::Closure,
        37 => OpCode::Close,
        38 => OpCode::ExtraArg,
        _ => OpCode::Move, // Default
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::parser;
    
    #[test]
    fn test_simple_return() {
        let source = "return 42";
        let ast = parser::parse(source).unwrap();
        let output = generate_bytecode(&ast).unwrap();
        
        assert_eq!(output.main.bytecode.len(), 2); // LoadK + Return
        assert_eq!(output.main.constants.len(), 1); // Number constant
    }
    
    #[test]
    fn test_local_assignment() {
        let source = "local x = 1; local y = 2; return x + y";
        let ast = parser::parse(source).unwrap();
        let output = generate_bytecode(&ast).unwrap();
        
        assert!(output.main.bytecode.len() > 3); // At least LoadK+LoadK+Add+Return
        assert_eq!(output.main.constants.len(), 2); // Two number constants
    }
}