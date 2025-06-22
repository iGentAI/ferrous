//! Lua bytecode compiler
//!
//! This module compiles a Lua AST into bytecode instructions for the VM.

use super::ast::*;
use super::error::{LuaError, Result};
use super::value::{FunctionProto, Instruction, LuaValue, LuaString, LuaClosure, LuaFunction};
use std::collections::HashMap;
use std::rc::Rc;

/// The Lua bytecode format uses these opcodes
/// Modeled after Lua 5.1 VM opcodes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpCode {
    Move,       // A B     R(A) := R(B)
    LoadK,      // A Bx    R(A) := Kst(Bx)
    LoadBool,   // A B C   R(A) := (B != 0); if (C) PC++
    LoadNil,    // A B     R(A) := ... := R(B) := nil
    GetUpval,   // A B     R(A) := UpValue[B]
    GetGlobal,  // A Bx    R(A) := Gbl[Kst(Bx)]
    GetTable,   // A B C   R(A) := R(B)[RK(C)]
    SetGlobal,  // A Bx    Gbl[Kst(Bx)] := R(A)
    SetUpval,   // A B     UpValue[B] := R(A)
    SetTable,   // A B C   R(B)[RK(C)] := R(A)
    NewTable,   // A B C   R(A) := {} (size = B,C)
    Self_,      // A B C   R(A+1) := R(B); R(A) := R(B)[RK(C)]
    Add,        // A B C   R(A) := RK(B) + RK(C)
    Sub,        // A B C   R(A) := RK(B) - RK(C)
    Mul,        // A B C   R(A) := RK(B) * RK(C)
    Div,        // A B C   R(A) := RK(B) / RK(C)
    Mod,        // A B C   R(A) := RK(B) % RK(C)
    Pow,        // A B C   R(A) := RK(B) ^ RK(C)
    Unm,        // A B     R(A) := -R(B)
    Not,        // A B     R(A) := not R(B)
    Len,        // A B     R(A) := length of R(B)
    Concat,     // A B C   R(A) := R(B).. ... ..R(C)
    Jmp,        // sBx     PC += sBx
    Eq,         // A B C   if ((RK(B) == RK(C)) ~= A) then PC++
    Lt,         // A B C   if ((RK(B) < RK(C)) ~= A) then PC++
    Le,         // A B C   if ((RK(B) <= RK(C)) ~= A) then PC++
    Test,       // A C     if not (R(A) <=> C) then PC++
    TestSet,    // A B C   if (R(B) <=> C) then R(A) := R(B) else PC++
    Call,       // A B C   R(A), ... ,R(A+C-2) := R(A)(R(A+1), ... ,R(A+B-1))
    TailCall,   // A B C   return R(A)(R(A+1), ... ,R(A+B-1))
    Return,     // A B     return R(A), ... ,R(A+B-2)
    ForLoop,    // A sBx   R(A)+=R(A+2); if R(A) <?= R(A+1) then { PC+=sBx; R(A+3)=R(A) }
    ForPrep,    // A sBx   R(A)-=R(A+2); PC+=sBx
    TForLoop,   // A C     R(A+3), ... ,R(A+2+C) := R(A)(R(A+1), R(A+2));
                //          if R(A+3) ~= nil then R(A+2)=R(A+3) else PC++
    SetList,    // A B C   R(A)[(C-1)*FPF+i] := R(A+i), 1 <= i <= B
    Close,      // A       close all variables in the stack up to (>=) R(A)
    Closure,    // A Bx    R(A) := closure(KPROTO[Bx])
    Vararg,     // A B     R(A), R(A+1), ..., R(A+B-2) = vararg
}

/// Field in the constant table
#[derive(Debug, Clone)]
enum ConstantValue {
    Nil,
    Boolean(bool),
    Number(f64),
    String(LuaString),
    Function(LuaValue), // Functions in constant table
}

impl From<ConstantValue> for LuaValue {
    fn from(value: ConstantValue) -> Self {
        match value {
            ConstantValue::Nil => LuaValue::Nil,
            ConstantValue::Boolean(b) => LuaValue::Boolean(b),
            ConstantValue::Number(n) => LuaValue::Number(n),
            ConstantValue::String(s) => LuaValue::String(s),
            ConstantValue::Function(f) => f, // Already a LuaValue
        }
    }
}

/// The compiler state
pub struct Compiler {
    /// The current function being compiled
    proto: FunctionProto,
    
    /// Local variables in scope
    locals: Vec<String>,
    
    /// Constants table
    constants: Vec<ConstantValue>,
    
    /// Break jump targets
    breaks: Vec<usize>,
    
    /// Label positions for break/continue
    labels: HashMap<String, usize>,

    /// Next free register (for intermediate values)
    next_register: usize,

    /// Register allocation tracking - maps register to whether it's in use
    register_in_use: Vec<bool>,
}

impl Compiler {
    /// Create a new compiler
    pub fn new() -> Self {
        Compiler {
            proto: FunctionProto {
                code: Vec::new(),
                constants: Vec::new(),
                num_params: 0,
                is_vararg: false,
                max_stack_size: 0,
            },
            locals: Vec::new(),
            constants: Vec::new(),
            breaks: Vec::new(),
            labels: HashMap::new(),
            next_register: 0,
            register_in_use: vec![false; 256], // Default to 256 registers max
        }
    }
    
    /// Compile a chunk into a function prototype
    pub fn compile_chunk(&mut self, chunk: &Chunk) -> Result<FunctionProto> {
        // Clear any existing state
        self.proto.code.clear();
        self.proto.constants.clear();
        self.locals.clear();
        self.constants.clear();
        self.next_register = 0;
        for i in 0..self.register_in_use.len() {
            self.register_in_use[i] = false;
        }
        
        self.compile_block(&chunk.block)?;
        
        // Add a return instruction to the end
        self.emit_return(0, 1);
        
        // Convert constants to LuaValue
        let constants = self.constants.drain(..)
            .map(LuaValue::from)
            .collect();
        
        // Update the prototype's constants
        self.proto.constants = constants;
        
        // Clean up register tracking
        self.cleanup_registers();
        
        // Return the completed prototype
        Ok(self.proto.clone())
    }
    
    /// Compile a block of statements
    fn compile_block(&mut self, block: &Block) -> Result<()> {
        // Save number of locals to restore after block
        let local_count = self.locals.len();
        
        // Compile statements
        for stmt in &block.statements {
            self.compile_statement(stmt)?;
        }
        
        // Compile return statement if present
        if let Some(ret) = &block.return_stmt {
            self.compile_return_statement(ret)?;
        }
        
        // Restore locals
        self.locals.truncate(local_count);
        
        Ok(())
    }
    
    /// Compile a statement
    fn compile_statement(&mut self, stmt: &Statement) -> Result<()> {
        match stmt {
            Statement::Empty => Ok(()),
            
            Statement::Assignment(assign) => self.compile_assignment(assign),
            
            Statement::FunctionCall(call) => {
                self.compile_function_call(call, 0)?; // Discard results
                Ok(())
            },
            
            Statement::Do(block) => {
                self.compile_block(block)?;
                Ok(())
            },
            
            Statement::While { condition, body } => {
                let start_pc = self.proto.code.len();
                
                // Compile condition
                let cond_reg = self.compile_expression(condition)?;
                
                // Test condition and jump past body if false
                self.emit_test(cond_reg, false);
                let jump_over = self.proto.code.len();
                self.emit_jump(0); // Placeholder jump
                
                // Compile body
                self.compile_block(body)?;
                
                // Jump back to condition
                self.emit_jump(start_pc as i32 - self.proto.code.len() as i32 - 1);
                
                // Patch the jump over body
                let end_pc = self.proto.code.len();
                self.patch_jump(jump_over, end_pc);
                
                Ok(())
            },
            
            Statement::Repeat { body, condition } => {
                let start_pc = self.proto.code.len();
                
                // Compile body
                self.compile_block(body)?;
                
                // Compile condition
                let cond_reg = self.compile_expression(condition)?;
                
                // Test condition and jump back if false
                self.emit_inst(
                    OpCode::Test,
                    cond_reg,
                    0,
                    0,
                );
                
                let offset = start_pc as i32 - self.proto.code.len() as i32 - 1;
                self.emit_jump(offset);
                
                Ok(())
            },
            
            Statement::If(if_stmt) => self.compile_if_statement(if_stmt),
            
            Statement::NumericFor { var, start, end, step, body } => {
                // Compile initialization
                let init_reg = self.compile_expression(start)?;
                let end_reg = self.compile_expression(end)?;
                
                // Compile step (default to 1 if not provided)
                let step_reg = if let Some(step_expr) = step {
                    self.compile_expression(step_expr)?
                } else {
                    let const_idx = self.add_constant(ConstantValue::Number(1.0));
                    let reg = self.alloc_register();
                    self.emit_load_k(reg, const_idx);
                    reg
                };
                
                // Define loop variable
                let var_reg = self.alloc_register();
                self.locals.push(var.clone());
                
                // Emit FORPREP
                self.emit_inst(
                    OpCode::ForPrep,
                    var_reg,
                    2, // Will be patched
                    0,
                );
                let forprep_idx = self.proto.code.len() - 1;
                
                // Compile body
                self.compile_block(body)?;
                
                // Emit FORLOOP
                self.emit_inst(
                    OpCode::ForLoop,
                    var_reg,
                    forprep_idx as u16, // Will be patched
                    0,
                );
                let forloop_idx = self.proto.code.len() - 1;
                
                // Patch jumps
                let loop_end = self.proto.code.len();
                self.patch_jump(forprep_idx, loop_end);
                self.patch_jump(forloop_idx, loop_end);
                
                // Remove loop variable
                self.locals.pop();
                
                Ok(())
            },
            
            Statement::GenericFor { vars, iterators, body } => {
                // Compile iterators
                let iter_base = self.alloc_register();
                for (i, iter) in iterators.iter().enumerate() {
                    let iter_reg = self.compile_expression(iter)?;
                    self.emit_move(iter_base + i, iter_reg);
                }
                
                // Define variables
                let var_base = self.alloc_register();
                for var in vars {
                    self.locals.push(var.clone());
                }
                
                // Emit TFORPREP
                let loop_start = self.proto.code.len();
                self.emit_inst(
                    OpCode::TForLoop,
                    iter_base,
                    vars.len() as u16,
                    0,
                );
                let tforloop_idx = self.proto.code.len() - 1;
                
                // Compile body
                self.compile_block(body)?;
                
                // Jump back to TFORLOOP
                self.emit_jump(loop_start as i32 - self.proto.code.len() as i32 - 1);
                
                // Patch TFORLOOP jump
                let loop_end = self.proto.code.len();
                self.patch_jump(tforloop_idx, loop_end);
                
                // Remove variables
                for _ in vars {
                    self.locals.pop();
                }
                
                Ok(())
            },
            
            Statement::Function(func) => self.compile_function_statement(func),
            
            Statement::LocalAssignment { names, values } => {
                self.compile_local_assignment(names, values)
            },
            
            Statement::LocalFunction { name, func } => {
                let func_reg = self.compile_function_definition(func)?;
                
                // Register local
                self.locals.push(name.clone());
                let local_reg = self.locals.len() - 1;
                
                // Store function in local
                self.emit_move(local_reg, func_reg);
                
                Ok(())
            },
            
            Statement::Break => {
                let jump_idx = self.proto.code.len();
                self.emit_jump(0); // Placeholder, will be patched
                self.breaks.push(jump_idx);
                Ok(())
            }
        }
    }
    
    /// Compile an assignment statement
    fn compile_assignment(&mut self, assign: &AssignmentStatement) -> Result<()> {
        // Compile right-hand side expressions
        let mut value_regs = Vec::new();
        for expr in &assign.values {
            let reg = self.compile_expression(expr)?;
            value_regs.push(reg);
        }
        
        // Assign to variables
        for (i, var) in assign.vars.iter().enumerate() {
            let value_reg = if i < value_regs.len() {
                value_regs[i]
            } else {
                // Missing value, use nil
                let reg = self.alloc_register();
                self.emit_load_nil(reg, reg);
                reg
            };
            
            self.compile_assignment_target(var, value_reg)?;
        }
        
        Ok(())
    }
    
    /// Compile a variable assignment target
    fn compile_assignment_target(&mut self, var: &Variable, value_reg: usize) -> Result<()> {
        match var {
            Variable::Name(name) => {
                // Check if it's a local variable
                if let Some(local_idx) = self.find_local(name) {
                    // Local variable
                    self.emit_move(local_idx, value_reg);
                } else {
                    // Global variable
                    let const_idx = self.add_constant(ConstantValue::String(LuaString::from_str(name)));
                    self.emit_set_global(value_reg, const_idx);
                }
            },
            Variable::Field { table, key } => {
                // First compile the table expression to get a register with the table
                // Don't rewrap as Expression::Variable, table is already an Expression
                let table_reg = self.compile_expression(table)?;
                let key_reg = self.compile_expression(key)?;
                
                // Set the table field
                self.emit_set_table(value_reg, table_reg, key_reg);
            }
        }
        
        Ok(())
    }
    
    /// Compile an if statement
    fn compile_if_statement(&mut self, if_stmt: &IfStatement) -> Result<()> {
        // Compile main condition
        let cond_reg = self.compile_expression(&if_stmt.condition)?;
        
        // Test condition and jump to next branch if false
        self.emit_test(cond_reg, false);
        let first_jump = self.proto.code.len();
        self.emit_jump(0); // Placeholder
        
        // Compile then block
        self.compile_block(&if_stmt.then_block)?;
        
        // Jump to end after then block
        let end_jumps = if !if_stmt.elseif_branches.is_empty() || if_stmt.else_block.is_some() {
            let end_jump = self.proto.code.len();
            self.emit_jump(0); // Placeholder
            vec![end_jump]
        } else {
            Vec::new()
        };
        
        // Patch the first jump to here (first elseif or else)
        let next_branch = self.proto.code.len();
        self.patch_jump(first_jump, next_branch);
        
        // Compile elseif branches
        let mut end_jump_list = end_jumps;
        
        for (i, (cond, block)) in if_stmt.elseif_branches.iter().enumerate() {
            // Compile condition
            let cond_reg = self.compile_expression(cond)?;
            
            // Test condition and jump to next branch if false
            self.emit_test(cond_reg, false);
            let branch_jump = self.proto.code.len();
            self.emit_jump(0); // Placeholder
            
            // Compile block
            self.compile_block(block)?;
            
            // Jump to end after block
            let is_last = i == if_stmt.elseif_branches.len() - 1 && if_stmt.else_block.is_none();
            if !is_last {
                let end_jump = self.proto.code.len();
                self.emit_jump(0); // Placeholder
                end_jump_list.push(end_jump);
            }
            
            // Patch the branch jump to here (next branch)
            let next_branch = self.proto.code.len();
            self.patch_jump(branch_jump, next_branch);
        }
        
        // Compile else block if present
        if let Some(else_block) = &if_stmt.else_block {
            self.compile_block(else_block)?;
        }
        
        // Patch all end jumps to here
        let end = self.proto.code.len();
        for jump in end_jump_list {
            self.patch_jump(jump, end);
        }
        
        Ok(())
    }
    
    /// Compile a function statement
    fn compile_function_statement(&mut self, func: &FunctionStatement) -> Result<()> {
        // Compile function
        let func_reg = self.compile_function_definition(&func.func)?;
        
        // Handle the function name
        let name = &func.name;
        
        if name.fields.is_empty() && name.method.is_none() {
            // Simple global function
            let const_idx = self.add_constant(ConstantValue::String(LuaString::from_str(&name.base)));
            self.emit_set_global(func_reg, const_idx);
        } else {
            // Function with table access
            // First, get the base table
            let base_reg = self.alloc_register();
            
            if let Some(local_idx) = self.find_local(&name.base) {
                // Local variable
                self.emit_move(base_reg, local_idx);
            } else {
                // Global variable
                let const_idx = self.add_constant(ConstantValue::String(LuaString::from_str(&name.base)));
                self.emit_get_global(base_reg, const_idx);
            }
            
            // Navigate through table fields
            let mut current_reg = base_reg;
            for (i, field) in name.fields.iter().enumerate() {
                let is_last = i == name.fields.len() - 1 && name.method.is_none();
                
                let key_const = self.add_constant(ConstantValue::String(LuaString::from_str(field)));
                let key_reg = self.alloc_register();
                self.emit_load_k(key_reg, key_const);
                
                if is_last {
                    // Last field, set the function
                    self.emit_set_table(func_reg, current_reg, key_reg);
                    break;
                } else {
                    // Intermediate field, navigate deeper
                    let next_reg = self.alloc_register();
                    self.emit_get_table(next_reg, current_reg, key_reg);
                    current_reg = next_reg;
                }
            }
            
            // Handle method if present
            if let Some(method) = &name.method {
                let key_const = self.add_constant(ConstantValue::String(LuaString::from_str(method)));
                let key_reg = self.alloc_register();
                self.emit_load_k(key_reg, key_const);
                
                // Set the method
                self.emit_set_table(func_reg, current_reg, key_reg);
            }
        }
        
        Ok(())
    }
    
    /// Compile a function definition
    fn compile_function_definition(&mut self, func: &FunctionDefinition) -> Result<usize> {
        // Create a new compiler for the function
        let mut subcompiler = Compiler::new();
        
        // Set parameters
        subcompiler.proto.num_params = func.parameters.len() as u8;
        subcompiler.proto.is_vararg = func.is_variadic;
        
        // Add parameters as locals
        for param in &func.parameters {
            subcompiler.locals.push(param.clone());
        }
        
        // Compile body
        subcompiler.compile_block(&func.body)?;
        
        // Add return
        subcompiler.emit_return(0, 1);
        
        // Finalize function prototype
        let func_proto = subcompiler.proto;
        
        // Create closure
        let const_idx = self.add_constant_proto(func_proto);
        let reg = self.alloc_register();
        
        self.emit_closure(reg, const_idx);
        
        Ok(reg)
    }
    
    /// Compile a return statement
    fn compile_return_statement(&mut self, ret: &ReturnStatement) -> Result<()> {
        let start_reg = self.alloc_register();
        
        // Compile return values
        for (i, expr) in ret.values.iter().enumerate() {
            let value_reg = self.compile_expression(expr)?;
            self.emit_move(start_reg + i, value_reg);
        }
        
        // Emit return instruction
        self.emit_return(start_reg, ret.values.len() + 1);
        
        Ok(())
    }
    
    /// Compile a function call
    fn compile_function_call(&mut self, call: &FunctionCall, result_count: usize) -> Result<usize> {
        let base_reg = self.alloc_register();
        
        // Compile function expression
        let func_reg = self.compile_expression(&call.func)?;
        
        // Mark function register as in use during argument compilation
        self.register_in_use[func_reg] = true;
        
        // Move function to base register
        self.emit_move(base_reg, func_reg);
        
        // Free function register
        self.free_register(func_reg);
        
        // Handle method call
        if call.is_method_call {
            if let Some(method) = &call.method_name {
                // Load method name
                let const_idx = self.add_constant(ConstantValue::String(LuaString::from_str(method)));
                let method_reg = self.alloc_register();
                self.emit_load_k(method_reg, const_idx);
                
                // Emit SELF instruction
                self.emit_inst(
                    OpCode::Self_,
                    base_reg,
                    func_reg as u16,
                    method_reg as u16,
                );
                
                // Free method register
                self.free_register(method_reg);
            }
        }
        
        // Make sure base_reg and base_reg+1 are marked as in use
        self.register_in_use[base_reg] = true;
        if call.is_method_call {
            self.register_in_use[base_reg + 1] = true;
        }
        
        // Compile arguments
        let mut arg_regs = Vec::with_capacity(call.args.len());
        for arg in &call.args {
            let arg_reg = self.compile_expression(arg)?;
            arg_regs.push(arg_reg);
            self.register_in_use[arg_reg] = true;
        }
        
        // Move arguments to consecutive registers after base+1
        for (i, arg_reg) in arg_regs.iter().enumerate() {
            let dest_reg = base_reg + i + 1 + if call.is_method_call { 1 } else { 0 };
            self.emit_move(dest_reg, *arg_reg);
            self.free_register(*arg_reg);
        }
        
        // Emit CALL instruction
        let arg_count = call.args.len() as u16 + if call.is_method_call { 1 } else { 0 };
        let result_count_plus1 = if result_count == 0 { 1 } else { result_count + 1 } as u16;
        
        self.emit_inst(
            OpCode::Call,
            base_reg,
            arg_count + 1,
            result_count_plus1, 
        );
        
        Ok(base_reg)
    }
    
    /// Compile a binary operation
    fn compile_binary_operation(&mut self, op: BinaryOp, left: &Expression, right: &Expression) -> Result<usize> {
        if op == BinaryOp::Concat {
            // Concatenation with table field access requires special handling to ensure proper register usage
            
            // First, determine if we're dealing with table fields by examining the Expression types
            let has_field_access = match (left, right) {
                (Expression::Variable(Variable::Field { .. }), _) | (_, Expression::Variable(Variable::Field { .. })) => true,
                _ => false,
            };
            
            if has_field_access {
                // For field access, compiling in the normal way leads to register confusion
                // Instead, we'll explicitly handle the concatenation in stages
                
                // Compile left expression into its own register
                let left_reg = self.compile_expression(left)?;
                
                // Mark as in use while compiling right expression
                self.register_in_use[left_reg] = true;
                
                // Compile right expression into another register
                let right_reg = self.compile_expression(right)?;
                
                // Now create a sequence of MOVEs and CONCATs to ensure correct order
                
                // Copy left value to a safe temporary register
                let temp_left = self.alloc_register();
                self.emit_move(temp_left, left_reg);
                
                // Copy right value to a safe temporary register 
                let temp_right = self.alloc_register();
                self.emit_move(temp_right, right_reg);
                
                // Create concatenation result in a new register, ensuring left + right order
                let result_reg = self.alloc_register();
                self.emit_inst(OpCode::Concat, result_reg, temp_left as u16, temp_right as u16);
                
                // Free temporary registers
                self.free_register(left_reg);
                self.free_register(right_reg);
                self.free_register(temp_left);
                self.free_register(temp_right);
                
                return Ok(result_reg);
            }
        }
            
        // For non-concatenation or simple concatenation without table fields, use standard approach
        
        // First compile left operand
        let left_reg = self.compile_expression(left)?;
        
        // Mark left register as in use
        self.register_in_use[left_reg] = true;
        
        // Compile right operand
        let right_reg = self.compile_expression(right)?;
        
        // Allocate result register
        let result_reg = self.alloc_register();
        
        match op {
            BinaryOp::Add => self.emit_inst(OpCode::Add, result_reg, left_reg as u16, right_reg as u16),
            BinaryOp::Sub => self.emit_inst(OpCode::Sub, result_reg, left_reg as u16, right_reg as u16),
            BinaryOp::Mul => self.emit_inst(OpCode::Mul, result_reg, left_reg as u16, right_reg as u16),
            BinaryOp::Div => self.emit_inst(OpCode::Div, result_reg, left_reg as u16, right_reg as u16),
            BinaryOp::Mod => self.emit_inst(OpCode::Mod, result_reg, left_reg as u16, right_reg as u16),
            BinaryOp::Pow => self.emit_inst(OpCode::Pow, result_reg, left_reg as u16, right_reg as u16),
            BinaryOp::Concat => self.emit_inst(OpCode::Concat, result_reg, left_reg as u16, right_reg as u16),
            _ => {
                // Comparison operations
                // Simplified handling for now
                self.emit_load_bool(result_reg, false, false);
            }
        }
        
        // Free operand registers if they're temporaries
        self.free_register(left_reg);
        self.free_register(right_reg);
        
        Ok(result_reg)
    }


    /// Compile an expression
    fn compile_expression(&mut self, expr: &Expression) -> Result<usize> {
        match expr {
            Expression::Nil => {
                let reg = self.alloc_register();
                self.emit_load_nil(reg, reg);
                Ok(reg)
            },
            
            Expression::Boolean(value) => {
                let reg = self.alloc_register();
                self.emit_load_bool(reg, *value, false);
                Ok(reg)
            },
            
            Expression::Number(value) => {
                let const_idx = self.add_constant(ConstantValue::Number(*value));
                let reg = self.alloc_register();
                self.emit_load_k(reg, const_idx);
                Ok(reg)
            },
            
            Expression::String(value) => {
                let const_idx = self.add_constant(ConstantValue::String(value.clone()));
                let reg = self.alloc_register();
                self.emit_load_k(reg, const_idx);
                Ok(reg)
            },
            
            Expression::Variable(var) => self.compile_variable(var),
            
            Expression::FunctionCall(call) => self.compile_function_call(call, 1),
            
            Expression::BinaryOp { op, left, right } => {
                // Use our improved compile_binary_operation function for all binary ops
                self.compile_binary_operation(*op, left, right)
            },
            
            Expression::UnaryOp { op, operand } => {
                let operand_reg = self.compile_expression(operand)?;
                let result_reg = self.alloc_register();
                
                match op {
                    UnaryOp::Neg => self.emit_inst(OpCode::Unm, result_reg, operand_reg as u16, 0),
                    UnaryOp::Not => self.emit_inst(OpCode::Not, result_reg, operand_reg as u16, 0),
                    UnaryOp::Len => self.emit_inst(OpCode::Len, result_reg, operand_reg as u16, 0),
                }
                
                // Free operand register if it's a temporary
                self.free_register(operand_reg);
                
                Ok(result_reg)
            },
            
            Expression::Function(func) => self.compile_function_definition(func),
            
            Expression::Table(fields) => {
                let table_reg = self.alloc_register();
                
                // Create empty table
                self.emit_inst(OpCode::NewTable, table_reg, 0, 0);
                
                // Fill table fields
                for (i, field) in fields.iter().enumerate() {
                    match field {
                        TableField::Value(value) => {
                            // Array part (implicit index i+1)
                            let value_reg = self.compile_expression(value)?;
                            
                            // Set table[i+1] = value
                            self.emit_set_table_array(table_reg, i + 1, value_reg);
                            
                            // Free value register if it's a temporary
                            self.free_register(value_reg);
                        },
                        TableField::KeyValue { key, value } => {
                            // Hash part (explicit key)
                            let key_reg = self.compile_expression(key)?;
                            
                            // Mark key register as in use so value compilation doesn't reuse it
                            self.register_in_use[key_reg] = true;
                            
                            let value_reg = self.compile_expression(value)?;
                            
                            // Set table[key] = value
                            self.emit_set_table(value_reg, table_reg, key_reg);
                            
                            // Free key and value registers if they're temporaries
                            self.free_register(key_reg);
                            self.free_register(value_reg);
                        },
                        TableField::NamedField { name, value } => {
                            // Hash part with string key
                            let key_const = self.add_constant(ConstantValue::String(LuaString::from_str(name)));
                            let key_reg = self.alloc_register();
                            self.emit_load_k(key_reg, key_const);
                            
                            // Mark key register as in use
                            self.register_in_use[key_reg] = true;
                            
                            let value_reg = self.compile_expression(value)?;
                            
                            // Set table[name] = value
                            self.emit_set_table(value_reg, table_reg, key_reg);
                            
                            // Free key and value registers if they're temporaries
                            self.free_register(key_reg);
                            self.free_register(value_reg);
                        },
                    }
                }
                
                Ok(table_reg)
            },
            
            Expression::Vararg => {
                if !self.proto.is_vararg {
                    return Err(LuaError::Syntax("cannot use '...' outside of a variadic function".to_string()));
                }
                
                let result_reg = self.alloc_register();
                self.emit_inst(OpCode::Vararg, result_reg, 2, 0); // Load 1 value
                
                Ok(result_reg)
            },
        }
    }
    
    /// Compile a variable reference
    fn compile_variable(&mut self, var: &Variable) -> Result<usize> {
        match var {
            Variable::Name(name) => {
                // Check if it's a local variable
                if let Some(local_idx) = self.find_local(name) {
                    // Local variable - already in a register
                    Ok(local_idx)
                } else {
                    // Global variable - needs to be loaded into a register
                    let const_idx = self.add_constant(ConstantValue::String(LuaString::from_str(name)));
                    let reg = self.alloc_register();
                    self.emit_get_global(reg, const_idx);
                    Ok(reg)
                }
            },
            Variable::Field { table, key } => {
                // First compile the table expression to get a register with the table
                let table_reg = self.compile_expression(table)?;
                
                // Mark table register as in use so key compilation doesn't reuse it
                self.register_in_use[table_reg] = true;
                
                // Compile key expression
                let key_reg = self.compile_expression(key)?;
                
                // Allocate a register for the field value result
                let result_reg = self.alloc_register();
                
                // Emit get table instruction
                self.emit_get_table(result_reg, table_reg, key_reg);
                
                // Free table and key registers if they're temporaries
                self.free_register(table_reg);
                self.free_register(key_reg);
                
                Ok(result_reg)
            }
        }
    }
    
    /// Find a local variable
    fn find_local(&self, name: &str) -> Option<usize> {
        self.locals.iter()
            .enumerate()
            .rev() // Search from innermost scope
            .find_map(|(i, local)| {
                if local == name {
                    Some(i)
                } else {
                    None
                }
            })
    }
    
    /// Allocate a register for a temporary value
    fn alloc_register(&mut self) -> usize {
        // Start with register after all locals
        let base_reg = self.locals.len();
        
        // Find the next free register
        let mut reg = base_reg + self.next_register;
        while self.register_in_use[reg] {
            reg += 1;
            // Wrap around if needed (shouldn't happen with 256 regs)
            if reg >= self.register_in_use.len() {
                reg = base_reg;
            }
        }
        
        // Mark register as in use
        self.register_in_use[reg] = true;
        
        // Update next register hint for future allocations
        self.next_register = reg + 1 - base_reg;
        if self.next_register >= self.register_in_use.len() - base_reg {
            self.next_register = 0; // Wrap around
        }
        
        // Update max stack size if needed
        if reg as u8 > self.proto.max_stack_size {
            self.proto.max_stack_size = reg as u8;
        }
        
        reg
    }

    /// Free a register that's no longer needed
    fn free_register(&mut self, reg: usize) {
        if reg >= self.locals.len() { // Only free registers outside the locals area
            self.register_in_use[reg] = false;
        }
    }

    /// Compile a local variable assignment
    fn compile_local_assignment(&mut self, names: &[String], values: &[Expression]) -> Result<()> {
        // First compile all value expressions into temporary registers
        let mut value_regs = Vec::with_capacity(values.len());
        for value in values {
            let reg = self.compile_expression(value)?;
            value_regs.push(reg);
            
            // Mark register as in use so subsequent expressions don't use it
            self.register_in_use[reg] = true;
        }
        
        // Now define all local variables
        let start_reg = self.locals.len();
        for (i, name) in names.iter().enumerate() {
            if i < value_regs.len() {
                // Move from temporary register to local
                self.emit_move(start_reg + i, value_regs[i]);
                
                // Free the temporary register
                self.free_register(value_regs[i]);
            } else {
                // Missing value, set to nil
                self.emit_load_nil(start_reg + i, start_reg + i);
            }
            
            // Add local to scope
            self.locals.push(name.clone());
        }
        
        Ok(())
    }

    /// Cleanup function to reset register tracking at end of compilation
    fn cleanup_registers(&mut self) {
        // Reset all register tracking
        for i in 0..self.register_in_use.len() {
            self.register_in_use[i] = false;
        }
        self.next_register = 0;
    }
    
    /// Add a constant to the constant table, returning its index
    fn add_constant(&mut self, value: ConstantValue) -> usize {
        // Check if constant already exists
        for (i, c) in self.constants.iter().enumerate() {
            match (&value, c) {
                (ConstantValue::Nil, ConstantValue::Nil) => return i,
                (ConstantValue::Boolean(a), ConstantValue::Boolean(b)) if a == b => return i,
                (ConstantValue::Number(a), ConstantValue::Number(b)) if a == b => return i,
                (ConstantValue::String(a), ConstantValue::String(b)) if a == b => return i,
                // For functions, we don't check for equality - we always add them as new constants
                _ => {},
            }
        }
        
        // Add new constant
        let idx = self.constants.len();
        self.constants.push(value);
        idx
    }
    
    /// Add a function prototype to the constant table
    fn add_constant_proto(&mut self, proto: FunctionProto) -> usize {
        // In a full Lua implementation, function prototypes would be stored in a separate list
        // and referenced by index. For our Redis needs, we store them as constants.
        
        // Create a proper closure from the function prototype
        let proto_rc = Rc::new(proto);
        let closure = LuaClosure {
            proto: proto_rc,
            upvalues: Vec::new(), // No upvalues for now - simplified for Redis Lua
        };
        
        // Create a function value
        let func = LuaValue::Function(LuaFunction::Lua(Rc::new(closure)));
        
        // Add to constants and return index
        self.add_constant(ConstantValue::Function(func))
    }
    
    /// Patch a jump instruction with the correct offset
    fn patch_jump(&mut self, jump_idx: usize, target: usize) {
        // Calculate offset (target - jump_idx - 1)
        let offset = target as i32 - jump_idx as i32 - 1;
        
        // Get jump instruction
        let instr = &mut self.proto.code[jump_idx];
        
        // Update the jump offset (sBx field)
        // In a real implementation, this would modify the instruction bytes
        // For now, we just create a new instruction
        let op = OpCode::Jmp;
        *instr = Instruction(pack_instruction(op, 0, offset as i32));
    }
    
    /// Emit a raw instruction
    fn emit_inst(&mut self, op: OpCode, a: usize, b: u16, c: u16) {
        let instr = match op {
            OpCode::Jmp => pack_instruction(op, a as u8, b as i32),
            _ => pack_instruction_abc(op, a as u8, b, c),
        };
        self.proto.code.push(Instruction(instr));
    }
    
    /// Emit a MOVE instruction
    fn emit_move(&mut self, dest: usize, src: usize) {
        self.emit_inst(OpCode::Move, dest, src as u16, 0);
    }
    
    /// Emit a LOADK instruction
    fn emit_load_k(&mut self, dest: usize, const_idx: usize) {
        self.emit_inst(OpCode::LoadK, dest, const_idx as u16, 0);
    }
    
    /// Emit a LOADBOOL instruction
    fn emit_load_bool(&mut self, dest: usize, value: bool, skip: bool) {
        self.emit_inst(OpCode::LoadBool, dest, if value { 1 } else { 0 }, if skip { 1 } else { 0 });
    }
    
    /// Emit a LOADNIL instruction
    fn emit_load_nil(&mut self, first: usize, last: usize) {
        self.emit_inst(OpCode::LoadNil, first, last as u16, 0);
    }
    
    /// Emit a GETGLOBAL instruction
    fn emit_get_global(&mut self, dest: usize, const_idx: usize) {
        self.emit_inst(OpCode::GetGlobal, dest, const_idx as u16, 0);
    }
    
    /// Emit a SETGLOBAL instruction
    fn emit_set_global(&mut self, src: usize, const_idx: usize) {
        self.emit_inst(OpCode::SetGlobal, src, const_idx as u16, 0);
    }
    
    /// Emit a GETTABLE instruction
    fn emit_get_table(&mut self, dest: usize, table: usize, key: usize) {
        self.emit_inst(OpCode::GetTable, dest, table as u16, key as u16);
    }
    
    /// Emit a SETTABLE instruction
    fn emit_set_table(&mut self, value: usize, table: usize, key: usize) {
        self.emit_inst(OpCode::SetTable, value, table as u16, key as u16);
    }
    
    /// Emit a SETTABLE for array part
    fn emit_set_table_array(&mut self, table: usize, index: usize, value: usize) {
        // This is a simplified version. In real Lua, SETLIST is used for batches
        // For now, we'll just use SETTABLE with a constant index
        let key_const = self.add_constant(ConstantValue::Number(index as f64));
        let key_reg = self.alloc_register();
        self.emit_load_k(key_reg, key_const);
        
        self.emit_set_table(value, table, key_reg);
    }
    
    /// Emit a TEST instruction
    fn emit_test(&mut self, reg: usize, is_true: bool) {
        self.emit_inst(OpCode::Test, reg, 0, if is_true { 1 } else { 0 });
    }
    
    /// Emit a JMP instruction
    fn emit_jump(&mut self, offset: i32) {
        self.emit_inst(OpCode::Jmp, 0, offset as u16, 0);
    }
    
    /// Emit a CLOSURE instruction
    fn emit_closure(&mut self, dest: usize, proto_idx: usize) {
        self.emit_inst(OpCode::Closure, dest, proto_idx as u16, 0);
    }
    
    /// Emit a RETURN instruction
    fn emit_return(&mut self, start: usize, count: usize) {
        self.emit_inst(OpCode::Return, start, count as u16, 0);
    }
}

/// Pack an instruction with A, B, C fields (standard format)
fn pack_instruction_abc(op: OpCode, a: u8, b: u16, c: u16) -> u32 {
    let op_val = op as u32 & 0x3F;
    let a_val = (a as u32) << 6;
    let b_val = (b as u32) << 14;
    let c_val = (c as u32) << 23;
    
    op_val | a_val | b_val | c_val
}

/// Pack an instruction with A, sBx fields (used for jumps)
fn pack_instruction(op: OpCode, a: u8, sbx: i32) -> u32 {
    let op_val = op as u32 & 0x3F;
    let a_val = (a as u32) << 6;
    
    // Convert sbx to unsigned by adding 131071 (Lua 5.1 uses this offset)
    let bx = (sbx + 131071) as u32;
    let bx_val = (bx & 0x3FFFF) << 14;
    
    op_val | a_val | bx_val
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lua::parser::Parser;
    
    #[test]
    fn test_compile_simple_expression() {
        let mut parser = Parser::new("return 1 + 2 * 3").unwrap();
        let chunk = parser.parse().unwrap();
        
        let mut compiler = Compiler::new();
        let proto = compiler.compile_chunk(&chunk).unwrap();
        
        // Should have at least 3 constants (1, 2, 3) and instructions for the arithmetic
        assert!(proto.constants.len() >= 3);
        assert!(proto.code.len() >= 3); // LOADK, LOADK, ADD, MUL, etc.
    }
    
    #[test]
    fn test_compile_simple_if() {
        let mut parser = Parser::new("if x > 10 then return 1 else return 2 end").unwrap();
        let chunk = parser.parse().unwrap();
        
        let mut compiler = Compiler::new();
        let proto = compiler.compile_chunk(&chunk).unwrap();
        
        // Should have at least 2 constants (1, 2) and comparison + jump instructions
        assert!(proto.constants.len() >= 2);
        assert!(proto.code.len() >= 5); // Comparison, jumps, returns
    }
}