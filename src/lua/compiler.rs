//! Lua bytecode compiler
//!
//! This module compiles a Lua AST into bytecode instructions for the VM.

use super::ast::*;
use super::error::{LuaError, Result};
use super::value::{FunctionProto, Instruction, LuaValue, LuaString};
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
}

impl From<ConstantValue> for LuaValue {
    fn from(value: ConstantValue) -> Self {
        match value {
            ConstantValue::Nil => LuaValue::Nil,
            ConstantValue::Boolean(b) => LuaValue::Boolean(b),
            ConstantValue::Number(n) => LuaValue::Number(n),
            ConstantValue::String(s) => LuaValue::String(s),
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
        }
    }
    
    /// Compile a chunk into a function prototype
    pub fn compile_chunk(&mut self, chunk: &Chunk) -> Result<FunctionProto> {
        // Clear any existing state
        self.proto.code.clear();
        self.proto.constants.clear();
        self.locals.clear();
        self.constants.clear();
        
        self.compile_block(&chunk.block)?;
        
        // Add a return instruction to the end
        self.emit_return(0, 1);
        
        // Convert constants to LuaValue
        let constants = self.constants.drain(..)
            .map(LuaValue::from)
            .collect();
        
        // Update the prototype's constants
        self.proto.constants = constants;
        
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
                // Compile values
                let mut regs = Vec::new();
                for value in values {
                    let reg = self.compile_expression(value)?;
                    regs.push(reg);
                }
                
                // Define locals
                let start_reg = self.alloc_register();
                for (i, name) in names.iter().enumerate() {
                    if i < regs.len() {
                        self.emit_move(start_reg + i, regs[i]);
                    } else {
                        self.emit_load_nil(start_reg + i, start_reg + i);
                    }
                    self.locals.push(name.clone());
                }
                
                Ok(())
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
        self.emit_move(base_reg, func_reg);
        
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
            }
        }
        
        // Compile arguments
        for (i, arg) in call.args.iter().enumerate() {
            let arg_reg = self.compile_expression(arg)?;
            self.emit_move(base_reg + i + 1, arg_reg);
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
                let left_reg = self.compile_expression(left)?;
                let right_reg = self.compile_expression(right)?;
                let result_reg = self.alloc_register();
                
                match op {
                    BinaryOp::Add => self.emit_inst(OpCode::Add, result_reg, left_reg as u16, right_reg as u16),
                    BinaryOp::Sub => self.emit_inst(OpCode::Sub, result_reg, left_reg as u16, right_reg as u16),
                    BinaryOp::Mul => self.emit_inst(OpCode::Mul, result_reg, left_reg as u16, right_reg as u16),
                    BinaryOp::Div => self.emit_inst(OpCode::Div, result_reg, left_reg as u16, right_reg as u16),
                    BinaryOp::Mod => self.emit_inst(OpCode::Mod, result_reg, left_reg as u16, right_reg as u16),
                    BinaryOp::Pow => self.emit_inst(OpCode::Pow, result_reg, left_reg as u16, right_reg as u16),
                    BinaryOp::Concat => self.emit_inst(OpCode::Concat, result_reg, left_reg as u16, right_reg as u16),
                    
                    // Comparison operators
                    BinaryOp::Eq | BinaryOp::NotEqual | 
                    BinaryOp::Less | BinaryOp::LessEqual |
                    BinaryOp::Greater | BinaryOp::GreaterEqual => {
                        // For comparison operators, we need to emit a comparison followed by a jump
                        let is_equal = *op == BinaryOp::Eq;
                        let is_less = *op == BinaryOp::Less;
                        let is_less_equal = *op == BinaryOp::LessEqual;
                        
                        let is_not_equal = *op == BinaryOp::NotEqual;
                        let is_greater = *op == BinaryOp::Greater;
                        let is_greater_equal = *op == BinaryOp::GreaterEqual;
                        
                        if is_equal || is_not_equal {
                            self.emit_inst(
                                OpCode::Eq,
                                if is_equal { 1 } else { 0 },
                                left_reg as u16,
                                right_reg as u16,
                            );
                        } else if is_less || is_greater {
                            self.emit_inst(
                                OpCode::Lt,
                                if is_less { 1 } else { 0 },
                                if is_less { left_reg } else { right_reg } as u16,
                                if is_less { right_reg } else { left_reg } as u16,
                            );
                        } else if is_less_equal || is_greater_equal {
                            self.emit_inst(
                                OpCode::Le,
                                if is_less_equal { 1 } else { 0 },
                                if is_less_equal { left_reg } else { right_reg } as u16,
                                if is_less_equal { right_reg } else { left_reg } as u16,
                            );
                        }
                        
                        // Skip the next instruction if comparison is false
                        self.emit_jump(1);
                        
                        // Load result
                        self.emit_load_bool(result_reg, true, true);
                        self.emit_load_bool(result_reg, false, false);
                    },
                    
                    // Logical operators
                    BinaryOp::And => {
                        // If left operand is false, result is false, otherwise result is right operand
                        self.emit_move(result_reg, left_reg);
                        self.emit_test(result_reg, false);
                        let jump = self.proto.code.len();
                        self.emit_jump(0); // Skip right if left is false
                        
                        // Right operand
                        self.emit_move(result_reg, right_reg);
                        
                        // Patch jump
                        let end = self.proto.code.len();
                        self.patch_jump(jump, end);
                    },
                    BinaryOp::Or => {
                        // If left operand is true, result is left, otherwise result is right
                        self.emit_move(result_reg, left_reg);
                        self.emit_test(result_reg, true);
                        let jump = self.proto.code.len();
                        self.emit_jump(0); // Skip right if left is true
                        
                        // Right operand
                        self.emit_move(result_reg, right_reg);
                        
                        // Patch jump
                        let end = self.proto.code.len();
                        self.patch_jump(jump, end);
                    },
                }
                
                Ok(result_reg)
            },
            
            Expression::UnaryOp { op, operand } => {
                let operand_reg = self.compile_expression(operand)?;
                let result_reg = self.alloc_register();
                
                match op {
                    UnaryOp::Neg => self.emit_inst(OpCode::Unm, result_reg, operand_reg as u16, 0),
                    UnaryOp::Not => self.emit_inst(OpCode::Not, result_reg, operand_reg as u16, 0),
                    UnaryOp::Len => self.emit_inst(OpCode::Len, result_reg, operand_reg as u16, 0),
                }
                
                Ok(result_reg)
            },
            
            Expression::Function(func) => self.compile_function_definition(func),
            
            Expression::Table(fields) => {
                let table_reg = self.alloc_register();
                
                // Create empty table
                // B and C are log(array size) and log(hash size)
                // For now, hardcode to just 0,0
                self.emit_inst(OpCode::NewTable, table_reg, 0, 0);
                
                // Fill table fields
                for (i, field) in fields.iter().enumerate() {
                    match field {
                        TableField::Value(value) => {
                            // Array part (implicit index i+1)
                            let value_reg = self.compile_expression(value)?;
                            
                            // SetList instruction will be emitted after all array fields
                            self.emit_set_table_array(table_reg, i + 1, value_reg);
                        },
                        TableField::KeyValue { key, value } => {
                            // Hash part (explicit key)
                            let key_reg = self.compile_expression(key)?;
                            let value_reg = self.compile_expression(value)?;
                            
                            self.emit_set_table(value_reg, table_reg, key_reg);
                        },
                        TableField::NamedField { name, value } => {
                            // Hash part with string key
                            let key_const = self.add_constant(ConstantValue::String(LuaString::from_str(name)));
                            let key_reg = self.alloc_register();
                            self.emit_load_k(key_reg, key_const);
                            
                            let value_reg = self.compile_expression(value)?;
                            
                            self.emit_set_table(value_reg, table_reg, key_reg);
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
                    // Local variable
                    Ok(local_idx)
                } else {
                    // Global variable
                    let const_idx = self.add_constant(ConstantValue::String(LuaString::from_str(name)));
                    let reg = self.alloc_register();
                    self.emit_get_global(reg, const_idx);
                    Ok(reg)
                }
            },
            Variable::Field { table, key } => {
                // First compile the table expression to get a register with the table
                // Don't rewrap as Expression::Variable, table is already an Expression
                let table_reg = self.compile_expression(table)?;
                let key_reg = self.compile_expression(key)?;
                let result_reg = self.alloc_register();
                
                self.emit_get_table(result_reg, table_reg, key_reg);
                
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
    
    /// Allocate a register
    fn alloc_register(&mut self) -> usize {
        let reg = self.locals.len();
        if reg as u8 > self.proto.max_stack_size {
            self.proto.max_stack_size = reg as u8;
        }
        reg
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
        // TODO: In a full implementation, this would add the prototype to a list of
        // prototypes in the parent function, but for now we just return 0 as a placeholder
        0
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