// TFORLOOP Implementation Fix - Aligns with Lua 5.1 Specification
// This module shows the corrected implementation for TFORLOOP opcode

use crate::lua::error::LuaResult;
use crate::lua::value::Value;
use crate::lua::transaction::HeapTransaction;
use crate::lua::vm::{StepResult, PendingOperation, ReturnContext};

/// Correct TFORLOOP implementation following Lua 5.1 spec:
/// R(A+3), ... ,R(A+2+C) := R(A)(R(A+1), R(A+2))
/// if R(A+3) ~= nil then 
///   R(A+2) = R(A+3); 
/// else 
///   PC++; 
/// 
/// Key differences from current implementation:
/// 1. No confusing r_a.is_function() check
/// 2. Properly updates all loop variables R(A+3)...R(A+2+C)
/// 3. Uses correct result register range
/// 4. No need for iterator storage/restoration - R(A) is never modified
pub fn handle_tforloop(
    vm: &mut crate::lua::vm::LuaVM,
    window_idx: usize,
    a: usize,
    c: usize,
    frame: &crate::lua::vm::CallFrame,
    tx: &mut HeapTransaction,
) -> LuaResult<StepResult> {
    // Step 0: Validate register bounds BEFORE any operations
    validate_tforloop_bounds(vm, window_idx, a, c)?;
    
    // Step 1: Get iterator function, state, and control variable
    let iterator = vm.register_windows.get_register(window_idx, a)?.clone();
    let state = vm.register_windows.get_register(window_idx, a + 1)?.clone();
    let control = vm.register_windows.get_register(window_idx, a + 2)?.clone();
    
    println!("DEBUG TFORLOOP: Calling iterator with state={:?}, control={:?}", state, control);
    
    // Step 2: Call the iterator function
    match iterator {
        Value::Closure(closure) => {
            // Queue the function call with proper return context
            tx.queue_operation(PendingOperation::FunctionCall {
                closure,
                args: vec![state, control],
                context: ReturnContext::TForLoop {
                    window_idx,
                    base: a,
                    var_count: c,
                    pc: frame.pc,
                },
            })?;
            Ok(StepResult::Continue)
        },
        Value::CFunction(cfunc) => {
            // Queue C function call with proper return context
            tx.queue_operation(PendingOperation::CFunctionCall {
                function: cfunc,
                args: vec![state, control],
                context: ReturnContext::TForLoop {
                    window_idx,
                    base: a,
                    var_count: c,
                    pc: frame.pc,
                },
            })?;
            Ok(StepResult::Continue)
        },
        other => {
            // This should not happen with properly compiled code
            Err(crate::lua::error::LuaError::TypeError {
                expected: "function".to_string(),
                got: other.type_name().to_string(),
            })
        }
    }
}

/// Handle the return from TFORLOOP iterator call
/// This is called when the iterator function returns
pub fn handle_tforloop_return(
    vm: &mut crate::lua::vm::LuaVM,
    window_idx: usize,
    base: usize,
    var_count: usize,
    pc: usize,
    results: Vec<Value>,
    tx: &mut HeapTransaction,
) -> LuaResult<()> {
    println!("DEBUG TFORLOOP RETURN: Got {} results", results.len());
    
    // Validate register bounds again to be completely safe
    validate_tforloop_bounds(vm, window_idx, base, var_count)?;
    
    // Step 3: Store results in R(A+3) through R(A+2+C)
    // Note: We pad with nil if not enough results
    for i in 0..var_count {
        let value = results.get(i).cloned().unwrap_or(Value::Nil);
        let target_reg = base + 3 + i;
        
        // We've already validated bounds, but a double-check doesn't hurt
        if !vm.register_windows.is_register_in_bounds(window_idx, target_reg) {
            return Err(crate::lua::error::LuaError::RuntimeError(
                format!("TFORLOOP would write to out-of-bounds register {}", target_reg)
            ));
        }
        
        vm.register_windows.set_register(window_idx, target_reg, value.clone())?;
        println!("DEBUG TFORLOOP: Set R({}) = {:?}", target_reg, value);
    }
    
    // Step 4: Check if R(A+3) is nil (termination condition)
    let first_result = results.first().cloned().unwrap_or(Value::Nil);
    
    if first_result.is_nil() {
        // End of iteration - skip the JMP
        println!("DEBUG TFORLOOP: First result is nil, ending loop");
        tx.increment_pc(vm.current_thread)?;
    } else {
        // Continue iteration - update control variable
        println!("DEBUG TFORLOOP: Setting control variable R({}) = {:?}", base + 2, first_result);
        vm.register_windows.set_register(window_idx, base + 2, first_result)?;
        // The following JMP will be executed normally
    }
    
    Ok(())
}

/// Validate TFORLOOP register bounds before execution
pub fn validate_tforloop_bounds(
    vm: &crate::lua::vm::LuaVM,
    window_idx: usize,
    base: usize,
    var_count: usize,
) -> LuaResult<()> {
    // Check that all required registers are in bounds
    // We need: R(A), R(A+1), R(A+2) for iterator/state/control
    // Plus: R(A+3) through R(A+2+C) for loop variables
    let required_size = base + 3 + var_count;
    let window_size = vm.register_windows.get_window_size(window_idx)
        .ok_or_else(|| crate::lua::error::LuaError::RuntimeError(
            format!("Invalid window index: {}", window_idx)
        ))?;
    
    if required_size > window_size {
        return Err(crate::lua::error::LuaError::RuntimeError(format!(
            "TFORLOOP would access register {} but window only has {} registers",
            required_size - 1, window_size
        )));
    }
    
    Ok(())
}