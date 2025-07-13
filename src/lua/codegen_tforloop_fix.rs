// TFORLOOP Code Generation Fix - Aligns with Lua 5.1 Specification
// This module shows the corrected compiler code generation for TFORLOOP

use crate::lua::ast::{Statement, Expression};
use crate::lua::error::LuaResult;
use crate::lua::opcode::OpCode;

/// Correct for-in loop compilation following Lua 5.1 spec
/// 
/// Key fixes:
/// 1. Single TFORLOOP instruction (no separate CALL)
/// 2. Proper register allocation for iterator triplet + loop variables
/// 3. Correct JMP structure
/// 4. No register preservation issues - iterator is never overwritten
pub fn compile_for_in_loop(
    compiler: &mut crate::lua::codegen::Compiler,
    variables: &[String],
    iterators: &[Expression],
    body: &[Statement],
) -> LuaResult<()> {
    compiler.enter_scope();
    
    // Step 1: Allocate registers for the iterator triplet
    // R(base) = iterator function
    // R(base+1) = state
    // R(base+2) = control variable
    let base_reg = compiler.registers.level();
    
    // Evaluate iterator expressions to get the triplet
    // pairs(t) returns: next, t, nil
    // ipairs(t) returns: ipairs_iter, t, 0
    // custom() returns: func, state, initial
    compiler.registers.allocate(); // R(A) for iterator
    compiler.registers.allocate(); // R(A+1) for state  
    compiler.registers.allocate(); // R(A+2) for control
    
    // Compile iterator expressions
    match iterators.len() {
        0 => {
            return Err(crate::lua::error::LuaError::CompileError(
                "for-in loop requires at least one iterator expression".to_string()
            ));
        },
        1 => {
            // Single expression case: evaluate and expect 3 values
            compiler.expression(&iterators[0], base_reg, 3)?;
        },
        _ => {
            // Multiple expressions: evaluate each to its register
            for (i, expr) in iterators.iter().enumerate() {
                if i < 3 {
                    compiler.expression(expr, base_reg + i, 1)?;
                }
            }
            // Fill missing with nil
            for i in iterators.len()..3 {
                compiler.emit_loadnil(base_reg + i, base_reg + i);
            }
        }
    }
    
    // Step 2: Allocate and add loop variables to scope
    // These go in R(base+3) onwards
    for (i, var_name) in variables.iter().enumerate() {
        let reg = compiler.registers.allocate();
        if reg != base_reg + 3 + i {
            panic!("Register allocation mismatch in for-in loop");
        }
        compiler.add_local(var_name, reg);
    }
    
    // Step 3: Jump to TFORLOOP (skip body on first entry)
    let jmp_to_tforloop = compiler.current_pc();
    compiler.emit(compiler.encode_AsBx(OpCode::Jmp, 0, 0)); // Placeholder
    
    // Step 4: Mark start of loop body
    let loop_body_start = compiler.current_pc();
    
    // Compile loop body
    compiler.inside_loop = true;
    let breaks_before = compiler.break_jumps.len();
    
    for stmt in body {
        compiler.statement(stmt)?;
    }
    
    compiler.inside_loop = false;
    
    // Step 5: Emit TFORLOOP instruction
    let tforloop_pc = compiler.current_pc();
    compiler.emit(compiler.encode_ABC(
        OpCode::TForLoop,
        base_reg as u8,
        0, // unused in Lua 5.1
        variables.len() as u16, // C = number of loop variables
    ));
    
    // Step 6: Jump back to loop body (if continuing)
    let jump_offset = loop_body_start as i32 - compiler.current_pc() as i32 - 1;
    compiler.emit(compiler.encode_AsBx(OpCode::Jmp, 0, jump_offset));
    
    // Step 7: Patch the initial jump to TFORLOOP
    let offset = tforloop_pc as i32 - jmp_to_tforloop as i32 - 1;
    compiler.patch_jump(jmp_to_tforloop, offset);
    
    // Patch break jumps
    compiler.patch_breaks(breaks_before);
    
    compiler.leave_scope();
    Ok(())
}

impl crate::lua::codegen::Compiler {
    /// Helper to emit LOADNIL 
    fn emit_loadnil(&mut self, from: usize, to: usize) {
        self.emit(Self::encode_ABC(
            OpCode::LoadNil,
            from as u8,
            to as u16,
            0,
        ));
    }
    
    /// Helper to patch a jump instruction
    fn patch_jump(&mut self, pc: usize, offset: i32) {
        // Re-encode the jump with the correct offset
        self.instructions[pc] = Self::encode_AsBx(OpCode::Jmp, 0, offset);
    }
}