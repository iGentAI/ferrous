//! TFORLOOP Compiler Implementation - Lua 5.1 Specification Compliant
//! 
//! This module provides the correct implementation for compiling for-in loops
//! to TFORLOOP bytecode. It follows the exact semantics specified in Lua 5.1:
//! 
//! for v1, v2, ... in exp do body end
//! 
//! Translates to:
//! 1. Evaluate exp to get iterator triplet (function, state, control)
//! 2. Place triplet in R(A), R(A+1), R(A+2)
//! 3. Allocate registers R(A+3)...R(A+2+C) for loop variables
//! 4. Generate bytecode: JMP to TFORLOOP, body, TFORLOOP, JMP back
//!
//! Key differences from incorrect implementation:
//! - No separate CALL instruction before TFORLOOP
//! - Proper register allocation for the iterator triplet
//! - Correct jump structure
//! - No function preservation complexity

use crate::lua::ast::{Statement, Expression};
use crate::lua::codegen::{Compiler, RegisterStatus};
use crate::lua::error::{LuaError, LuaResult};
use crate::lua::opcode::OpCode;

impl Compiler {
    /// Compile a for-in loop statement to TFORLOOP bytecode
    /// 
    /// # Arguments
    /// * `variables` - Loop variable names (e.g., ["k", "v"])
    /// * `iterators` - Iterator expressions (e.g., [pairs(t)])
    /// * `body` - Loop body statements
    /// 
    /// # Returns
    /// Ok(()) if compilation succeeds, error otherwise
    pub fn compile_for_in_loop(
        &mut self,
        variables: &[String],
        iterators: &[Expression],
        body: &[Statement],
    ) -> LuaResult<()> {
        // Enter new scope for loop variables
        self.enter_scope();
        
        // Step 1: Allocate registers for the iterator triplet
        // R(base) = iterator function
        // R(base+1) = state  
        // R(base+2) = control variable
        let base_reg = self.allocate_registers_for_iterator_triplet()?;
        
        // Step 2: Evaluate iterator expressions to fill the triplet
        self.evaluate_iterator_expressions(iterators, base_reg)?;
        
        // Step 3: Allocate registers for loop variables and add to scope
        // These go in R(base+3) onwards
        let var_count = variables.len();
        if var_count == 0 {
            return Err(LuaError::CompileError(
                "for-in loop requires at least one loop variable".to_string()
            ));
        }
        
        for (i, var_name) in variables.iter().enumerate() {
            let reg = self.registers.allocate();
            if reg != base_reg + 3 + i {
                return Err(LuaError::InternalError(format!(
                    "Register allocation mismatch in for-in loop: expected {}, got {}",
                    base_reg + 3 + i,
                    reg
                )));
            }
            self.add_local(var_name, reg);
        }
        
        // Step 4: Generate jump to TFORLOOP (skip body on first entry)
        let jmp_to_tforloop = self.current_pc();
        self.emit_jump(0); // Placeholder, will be patched
        
        // Step 5: Mark start of loop body for break statements
        let loop_body_start = self.current_pc();
        let breaks_before = self.break_jumps.len();
        self.push_loop_context(loop_body_start);
        
        // Compile loop body
        for stmt in body {
            self.statement(stmt)?;
        }
        
        self.pop_loop_context();
        
        // Step 6: Emit TFORLOOP instruction
        let tforloop_pc = self.current_pc();
        self.emit_tforloop(base_reg, var_count)?;
        
        // Step 7: Jump back to loop body (if continuing)
        let jump_back_offset = loop_body_start as i32 - self.current_pc() as i32 - 1;
        self.emit_jump(jump_back_offset);
        
        // Step 8: Patch the initial jump to TFORLOOP
        let initial_jump_offset = tforloop_pc as i32 - jmp_to_tforloop as i32 - 1;
        self.patch_jump(jmp_to_tforloop, initial_jump_offset);
        
        // Patch break jumps to jump here (after the loop)
        self.patch_break_jumps(breaks_before);
        
        // Clean up scope
        self.leave_scope();
        
        Ok(())
    }
    
    /// Allocate registers for the iterator triplet
    fn allocate_registers_for_iterator_triplet(&mut self) -> LuaResult<usize> {
        let base_reg = self.registers.level();
        
        // Allocate three consecutive registers
        let r1 = self.registers.allocate(); // R(A) for iterator function
        let r2 = self.registers.allocate(); // R(A+1) for state
        let r3 = self.registers.allocate(); // R(A+2) for control
        
        // Verify they are consecutive
        if r1 != base_reg || r2 != base_reg + 1 || r3 != base_reg + 2 {
            return Err(LuaError::InternalError(
                "Failed to allocate consecutive registers for iterator triplet".to_string()
            ));
        }
        
        Ok(base_reg)
    }
    
    /// Evaluate iterator expressions and place results in iterator triplet registers
    fn evaluate_iterator_expressions(
        &mut self,
        iterators: &[Expression],
        base_reg: usize,
    ) -> LuaResult<()> {
        match iterators.len() {
            0 => {
                return Err(LuaError::CompileError(
                    "for-in loop requires at least one iterator expression".to_string()
                ));
            }
            1 => {
                // Single expression case: evaluate and expect 3 values
                // This handles cases like: for k,v in pairs(t) do ... end
                // pairs(t) should return: iterator, state, control
                self.expression_with_results(&iterators[0], base_reg, 3)?;
            }
            _ => {
                // Multiple expressions: evaluate each to its register
                // This handles cases like: for v in f, s, var do ... end
                for (i, expr) in iterators.iter().enumerate() {
                    if i < 3 {
                        self.expression_with_results(expr, base_reg + i, 1)?;
                    }
                    // Ignore extra expressions beyond the first 3
                }
                
                // Fill missing slots with nil if less than 3 expressions
                for i in iterators.len()..3 {
                    self.emit_loadnil(base_reg + i, base_reg + i);
                }
            }
        }
        
        Ok(())
    }
    
    /// Emit TFORLOOP instruction
    fn emit_tforloop(&mut self, base_reg: usize, var_count: usize) -> LuaResult<()> {
        // Validate var_count fits in C field (16 bits in practice, but Lua uses less)
        if var_count > 200 {
            return Err(LuaError::CompileError(
                format!("Too many loop variables: {} (maximum 200)", var_count)
            ));
        }
        
        // TFORLOOP A C: 
        // R(A+3), ..., R(A+2+C) := R(A)(R(A+1), R(A+2))
        // if R(A+3) ~= nil then R(A+2) = R(A+3) else PC++
        let instruction = Self::encode_ABC(
            OpCode::TForLoop,
            base_reg as u8,
            0, // B field is unused in Lua 5.1 TFORLOOP
            var_count as u16, // C = number of loop variables
        );
        
        self.emit(instruction);
        Ok(())
    }
    
    /// Emit a JMP instruction
    fn emit_jump(&mut self, offset: i32) {
        let instruction = Self::encode_AsBx(OpCode::Jmp, 0, offset);
        self.emit(instruction);
    }
    
    /// Emit LOADNIL instruction
    fn emit_loadnil(&mut self, from: usize, to: usize) {
        if from > 255 || to > 255 {
            panic!("Register index out of range for LOADNIL");
        }
        
        let instruction = Self::encode_ABC(
            OpCode::LoadNil,
            from as u8,
            to as u16,
            0,
        );
        self.emit(instruction);
    }
    
    /// Patch a previously emitted jump instruction with the correct offset
    fn patch_jump(&mut self, pc: usize, offset: i32) {
        if pc >= self.instructions.len() {
            panic!("Invalid PC for jump patching: {}", pc);
        }
        
        // Re-encode the jump with the correct offset
        self.instructions[pc] = Self::encode_AsBx(OpCode::Jmp, 0, offset);
    }
    
    /// Push a new loop context for break statement tracking
    fn push_loop_context(&mut self, loop_start: usize) {
        self.loop_stack.push(LoopContext {
            start_pc: loop_start,
            break_list_start: self.break_jumps.len(),
        });
        self.inside_loop = true;
    }
    
    /// Pop the current loop context
    fn pop_loop_context(&mut self) {
        self.loop_stack.pop();
        self.inside_loop = !self.loop_stack.is_empty();
    }
    
    /// Patch break jumps from a loop
    fn patch_break_jumps(&mut self, break_start_idx: usize) {
        let current_pc = self.current_pc();
        
        // Patch all break jumps from break_start_idx to end
        for &break_pc in &self.break_jumps[break_start_idx..] {
            let offset = current_pc as i32 - break_pc as i32 - 1;
            self.patch_jump(break_pc, offset);
        }
        
        // Remove patched jumps
        self.break_jumps.truncate(break_start_idx);
    }
}

/// Loop context for tracking break statements
#[derive(Debug)]
struct LoopContext {
    /// PC of the loop start
    start_pc: usize,
    /// Index in break_jumps where this loop's breaks start
    break_list_start: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lua::lexer::Lexer;
    use crate::lua::parser::Parser;
    
    fn compile_for_in(source: &str) -> Result<Vec<u8>, String> {
        let mut lexer = Lexer::new(source, "<test>");
        let tokens = lexer.scan_tokens()
            .map_err(|e| format!("Lexer error: {:?}", e))?;
        
        let parser = Parser::new(tokens);
        let ast = parser.parse()
            .map_err(|e| format!("Parser error: {:?}", e))?;
        
        let mut compiler = Compiler::new();
        let proto = compiler.compile(&ast, "<test>")
            .map_err(|e| format!("Compiler error: {:?}", e))?;
        
        Ok(proto.code.clone())
    }
    
    #[test]
    fn test_simple_for_in_loop() {
        let source = r#"
            for k, v in pairs(t) do
                print(k, v)
            end
        "#;
        
        let result = compile_for_in(source);
        assert!(result.is_ok(), "Compilation failed: {:?}", result);
        
        let code = result.unwrap();
        
        // Verify bytecode structure
        let mut found_tforloop = false;
        let mut found_jmp_before_tforloop = false;
        let mut found_jmp_after_tforloop = false;
        let mut last_was_tforloop = false;
        
        for i in 0..code.len() / 4 {
            let instruction = u32::from_le_bytes([
                code[i * 4],
                code[i * 4 + 1],
                code[i * 4 + 2],
                code[i * 4 + 3],
            ]);
            
            let opcode = instruction & 0x3F;
            
            if opcode == OpCode::Jmp as u8 && !found_tforloop {
                found_jmp_before_tforloop = true;
            }
            
            if opcode == OpCode::TForLoop as u8 {
                found_tforloop = true;
                last_was_tforloop = true;
            } else if last_was_tforloop && opcode == OpCode::Jmp as u8 {
                found_jmp_after_tforloop = true;
                last_was_tforloop = false;
            } else {
                last_was_tforloop = false;
            }
        }
        
        assert!(found_jmp_before_tforloop, "Missing initial JMP to TFORLOOP");
        assert!(found_tforloop, "Missing TFORLOOP instruction");
        assert!(found_jmp_after_tforloop, "Missing JMP after TFORLOOP");
    }
    
    #[test]
    fn test_for_in_with_multiple_iterators() {
        let source = r#"
            for v in next, t, nil do
                print(v)
            end
        "#;
        
        let result = compile_for_in(source);
        assert!(result.is_ok(), "Compilation failed: {:?}", result);
    }
    
    #[test]
    fn test_for_in_with_single_variable() {
        let source = r#"
            for i in ipairs(t) do
                print(i)
            end
        "#;
        
        let result = compile_for_in(source);
        assert!(result.is_ok(), "Compilation failed: {:?}", result);
    }
}