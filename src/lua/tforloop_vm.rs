//! TFORLOOP VM Execution Implementation - Lua 5.1 Specification Compliant
//!
//! This module provides the correct implementation for executing the TFORLOOP
//! opcode in the Lua VM. It follows the exact semantics specified in Lua 5.1:
//!
//! TFORLOOP A C: R(A+3), ..., R(A+2+C) := R(A)(R(A+1), R(A+2))
//!               if R(A+3) ~= nil then R(A+2) = R(A+3) else PC++
//!
//! Key differences from incorrect implementation:
//! - No iterator storage/restoration complexity
//! - Direct function call without register preservation
//! - Simple control flow for termination
//! - Proper bounds checking before any operations

use crate::lua::error::{LuaError, LuaResult};
use crate::lua::transaction::HeapTransaction;
use crate::lua::value::Value;
use crate::lua::vm::{CallFrame, LuaVM, PendingOperation, ReturnContext, StepResult};
use crate::lua::register_window::{
    TFORLOOP_ITER_OFFSET, TFORLOOP_STATE_OFFSET, 
    TFORLOOP_CONTROL_OFFSET, TFORLOOP_VAR_OFFSET
};

/// Execute TFORLOOP opcode according to Lua 5.1 specification
/// 
/// # Arguments
/// * `vm` - The Lua VM instance
/// * `window_idx` - Current register window index
/// * `a` - Base register (A from instruction)
/// * `c` - Number of loop variables (C from instruction)
/// * `frame` - Current call frame
/// * `tx` - Heap transaction for queueing operations
/// 
/// # Returns
/// StepResult indicating continuation or error
pub fn execute_tforloop(
    vm: &mut LuaVM,
    window_idx: usize,
    a: usize,
    c: usize,
    frame: &CallFrame,
    tx: &mut HeapTransaction,
) -> LuaResult<StepResult> {
    // Step 1: Validate register bounds BEFORE any operations
    validate_tforloop_bounds(vm, window_idx, a, c)?;
    
    // Step 2: Get iterator function, state, and control variable
    let iterator = vm.register_windows.get_register(window_idx, a + TFORLOOP_ITER_OFFSET)?.clone();
    let state = vm.register_windows.get_register(window_idx, a + TFORLOOP_STATE_OFFSET)?.clone();
    let control = vm.register_windows.get_register(window_idx, a + TFORLOOP_CONTROL_OFFSET)?.clone();
    
    #[cfg(debug_assertions)]
    {
        eprintln!("TFORLOOP: Calling iterator");
        eprintln!("  Iterator: {:?}", iterator.type_name());
        eprintln!("  State: {:?}", state);
        eprintln!("  Control: {:?}", control);
        eprintln!("  Base register: {}, Var count: {}", a, c);
    }
    
    // Step 3: Queue the iterator function call based on its type
    match iterator {
        Value::Closure(closure) => {
            // Lua function call
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
        }
        Value::CFunction(cfunc) => {
            // C function call
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
        }
        other => {
            // Not a callable value - this indicates a compiler error
            Err(LuaError::TypeError {
                expected: "function".to_string(),
                got: other.type_name().to_string(),
            })
        }
    }
}

/// Handle the return from a TFORLOOP iterator call
/// 
/// This function is called when the iterator function returns. It processes
/// the results according to TFORLOOP semantics.
/// 
/// # Arguments
/// * `vm` - The Lua VM instance
/// * `window_idx` - Current register window index
/// * `base` - Base register (A from TFORLOOP instruction)
/// * `var_count` - Number of loop variables (C from instruction)
/// * `pc` - Program counter of TFORLOOP instruction
/// * `results` - Results returned by the iterator function
/// * `tx` - Heap transaction
/// 
/// # Returns
/// Ok(()) if successful, error otherwise
pub fn handle_tforloop_return(
    vm: &mut LuaVM,
    window_idx: usize,
    base: usize,
    var_count: usize,
    pc: usize,
    results: Vec<Value>,
    tx: &mut HeapTransaction,
) -> LuaResult<()> {
    #[cfg(debug_assertions)]
    {
        eprintln!("TFORLOOP Return: Got {} results", results.len());
        for (i, val) in results.iter().enumerate() {
            eprintln!("  Result[{}]: {:?}", i, val);
        }
    }
    
    // Validate register bounds again for safety
    validate_tforloop_bounds(vm, window_idx, base, var_count)?;
    
    // Step 1: Store results in loop variable registers R(A+3) through R(A+2+C)
    for i in 0..var_count {
        let value = results.get(i).cloned().unwrap_or(Value::Nil);
        let target_reg = base + TFORLOOP_VAR_OFFSET + i;
        
        vm.register_windows.set_register(window_idx, target_reg, value.clone())?;
        
        #[cfg(debug_assertions)]
        eprintln!("  Set R({}) = {:?}", target_reg, value);
    }
    
    // Step 2: Check if R(A+3) is nil (termination condition)
    let first_result = results.first().cloned().unwrap_or(Value::Nil);
    
    if first_result.is_nil() {
        // End of iteration - increment PC to skip the following JMP
        #[cfg(debug_assertions)]
        eprintln!("TFORLOOP: First result is nil, ending loop");
        
        tx.increment_pc(vm.current_thread)?;
    } else {
        // Continue iteration - update control variable R(A+2) with first result
        #[cfg(debug_assertions)]
        eprintln!("TFORLOOP: Setting control variable R({}) = {:?}", 
                  base + TFORLOOP_CONTROL_OFFSET, first_result);
        
        vm.register_windows.set_register(
            window_idx, 
            base + TFORLOOP_CONTROL_OFFSET, 
            first_result
        )?;
        
        // The following JMP instruction will be executed normally to loop back
    }
    
    Ok(())
}

/// Validate that TFORLOOP can access all required registers
/// 
/// This function ensures that all registers that will be accessed by TFORLOOP
/// are within the bounds of the current window. This prevents runtime panics
/// from out-of-bounds register access.
/// 
/// # Arguments
/// * `vm` - The Lua VM instance
/// * `window_idx` - Current register window index  
/// * `base` - Base register (A from instruction)
/// * `var_count` - Number of loop variables (C from instruction)
/// 
/// # Returns
/// Ok(()) if all registers are accessible, error otherwise
pub fn validate_tforloop_bounds(
    vm: &LuaVM,
    window_idx: usize,
    base: usize,
    var_count: usize,
) -> LuaResult<()> {
    // Get window size
    let window_size = vm.register_windows.get_window_size(window_idx)
        .ok_or_else(|| LuaError::RuntimeError(
            format!("Invalid window index: {}", window_idx)
        ))?;
    
    // Calculate highest register that will be accessed
    // We need:
    // - R(A), R(A+1), R(A+2) for iterator/state/control  
    // - R(A+3) through R(A+2+C) for loop variables
    let highest_register = base + TFORLOOP_VAR_OFFSET + var_count - 1;
    
    if highest_register >= window_size {
        return Err(LuaError::RuntimeError(format!(
            "TFORLOOP would access register {} but window only has {} registers",
            highest_register, window_size
        )));
    }
    
    // Also validate that we have at least one loop variable
    if var_count == 0 {
        return Err(LuaError::RuntimeError(
            "TFORLOOP requires at least one loop variable (C must be >= 1)".to_string()
        ));
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lua::closure::Closure;
    use crate::lua::heap::{Heap, HeapHandle};
    use crate::lua::proto::Prototype;
    use crate::lua::function::CFunction;
    use std::rc::Rc;
    
    fn create_test_vm() -> LuaVM {
        let mut heap = Heap::new(1024 * 1024); // 1MB heap
        LuaVM::new(heap)
    }
    
    fn create_test_closure(heap: &mut Heap) -> HeapHandle {
        let proto = Prototype {
            code: vec![],
            constants: vec![],
            protos: vec![],
            debug_name: Some("test".to_string()),
            upvalue_descs: vec![],
            num_params: 2,
            is_vararg: false,
            max_stack_size: 10,
        };
        
        let proto_handle = heap.allocate_proto(proto).unwrap();
        heap.allocate_closure(proto_handle, vec![]).unwrap()
    }
    
    #[test]
    fn test_validate_bounds_success() {
        let mut vm = create_test_vm();
        let window = vm.register_windows.allocate_window(20).unwrap();
        
        // Valid bounds: base=5, c=3 needs registers 5,6,7 (triplet) and 8,9,10 (vars)
        assert!(validate_tforloop_bounds(&vm, window, 5, 3).is_ok());
        
        // Edge case: exactly fits
        assert!(validate_tforloop_bounds(&vm, window, 14, 3).is_ok()); // Uses up to R(19)
    }
    
    #[test]
    fn test_validate_bounds_failure() {
        let mut vm = create_test_vm();
        let window = vm.register_windows.allocate_window(10).unwrap();
        
        // Out of bounds: base=7, c=3 would need R(12) but window only has 10
        assert!(validate_tforloop_bounds(&vm, window, 7, 3).is_err());
        
        // Zero variables not allowed
        assert!(validate_tforloop_bounds(&vm, window, 0, 0).is_err());
    }
    
    #[test]
    fn test_execute_tforloop_with_closure() {
        let mut vm = create_test_vm();
        let window = vm.register_windows.allocate_window(20).unwrap();
        let mut tx = HeapTransaction::new();
        
        // Create a test closure
        let closure_handle = create_test_closure(&mut vm.heap);
        
        // Set up iterator triplet
        let base = 5;
        vm.register_windows.set_register(window, base, Value::Closure(closure_handle)).unwrap();
        vm.register_windows.set_register(window, base + 1, Value::Number(42.0)).unwrap();
        vm.register_windows.set_register(window, base + 2, Value::Number(0.0)).unwrap();
        
        let frame = CallFrame {
            closure: closure_handle,
            pc: 10,
            base_register: window,
            upvalues: None,
        };
        
        // Execute TFORLOOP
        let result = execute_tforloop(&mut vm, window, base, 2, &frame, &mut tx);
        assert!(result.is_ok());
        
        // Should have queued a function call
        assert_eq!(tx.operations.len(), 1);
        match &tx.operations[0] {
            PendingOperation::FunctionCall { args, context, .. } => {
                assert_eq!(args.len(), 2);
                assert_eq!(args[0], Value::Number(42.0)); // state
                assert_eq!(args[1], Value::Number(0.0));  // control
                
                match context {
                    ReturnContext::TForLoop { base: b, var_count: c, .. } => {
                        assert_eq!(*b, base);
                        assert_eq!(*c, 2);
                    }
                    _ => panic!("Wrong return context"),
                }
            }
            _ => panic!("Wrong operation type"),
        }
    }
    
    #[test]
    fn test_execute_tforloop_with_cfunction() {
        let mut vm = create_test_vm();
        let window = vm.register_windows.allocate_window(20).unwrap();
        let mut tx = HeapTransaction::new();
        
        // Create a test C function
        fn test_iter(_ctx: &mut crate::lua::function::ExecutionContext) -> LuaResult<i32> {
            Ok(0)
        }
        
        let cfunc: CFunction = Rc::new(test_iter);
        
        // Set up iterator triplet
        let base = 5;
        vm.register_windows.set_register(window, base, Value::CFunction(cfunc.clone())).unwrap();
        vm.register_windows.set_register(window, base + 1, Value::Number(42.0)).unwrap();
        vm.register_windows.set_register(window, base + 2, Value::Number(0.0)).unwrap();
        
        let frame = CallFrame {
            closure: create_test_closure(&mut vm.heap), // Dummy closure
            pc: 10,
            base_register: window,
            upvalues: None,
        };
        
        // Execute TFORLOOP
        let result = execute_tforloop(&mut vm, window, base, 2, &frame, &mut tx);
        assert!(result.is_ok());
        
        // Should have queued a C function call
        assert_eq!(tx.operations.len(), 1);
        match &tx.operations[0] {
            PendingOperation::CFunctionCall { args, context, .. } => {
                assert_eq!(args.len(), 2);
                assert_eq!(args[0], Value::Number(42.0)); // state
                assert_eq!(args[1], Value::Number(0.0));  // control
                
                match context {
                    ReturnContext::TForLoop { base: b, var_count: c, .. } => {
                        assert_eq!(*b, base);
                        assert_eq!(*c, 2);
                    }
                    _ => panic!("Wrong return context"),
                }
            }
            _ => panic!("Wrong operation type"),
        }
    }
    
    #[test]
    fn test_execute_tforloop_type_error() {
        let mut vm = create_test_vm();
        let window = vm.register_windows.allocate_window(20).unwrap();
        let mut tx = HeapTransaction::new();
        
        // Set up with non-callable value
        let base = 5;
        vm.register_windows.set_register(window, base, Value::Number(123.0)).unwrap();
        vm.register_windows.set_register(window, base + 1, Value::Number(42.0)).unwrap();
        vm.register_windows.set_register(window, base + 2, Value::Number(0.0)).unwrap();
        
        let frame = CallFrame {
            closure: create_test_closure(&mut vm.heap),
            pc: 10,
            base_register: window,
            upvalues: None,
        };
        
        // Should fail with type error
        let result = execute_tforloop(&mut vm, window, base, 2, &frame, &mut tx);
        assert!(result.is_err());
        
        match result {
            Err(LuaError::TypeError { expected, got }) => {
                assert_eq!(expected, "function");
                assert_eq!(got, "number");
            }
            _ => panic!("Wrong error type"),
        }
    }
    
    #[test]
    fn test_handle_return_continue_iteration() {
        let mut vm = create_test_vm();
        let window = vm.register_windows.allocate_window(20).unwrap();
        let mut tx = HeapTransaction::new();
        
        let base = 5;
        let var_count = 2;
        
        // Set initial control value
        vm.register_windows.set_register(window, base + 2, Value::Number(0.0)).unwrap();
        
        // Simulate iterator returning values
        let results = vec![
            Value::Number(1.0),    // New control value (key/index)
            Value::String("hello".into()), // Value
        ];
        
        let result = handle_tforloop_return(
            &mut vm, window, base, var_count, 10, results, &mut tx
        );
        assert!(result.is_ok());
        
        // Verify loop variables were set
        assert_eq!(
            *vm.register_windows.get_register(window, base + 3).unwrap(),
            Value::Number(1.0)
        );
        assert_eq!(
            *vm.register_windows.get_register(window, base + 4).unwrap(),
            Value::String("hello".into())
        );
        
        // Verify control variable was updated
        assert_eq!(
            *vm.register_windows.get_register(window, base + 2).unwrap(),
            Value::Number(1.0)
        );
        
        // PC should not be incremented (continue loop)
        assert_eq!(tx.operations.len(), 0);
    }
    
    #[test]
    fn test_handle_return_end_iteration() {
        let mut vm = create_test_vm();
        let window = vm.register_windows.allocate_window(20).unwrap();
        let mut tx = HeapTransaction::new();
        
        // Add current thread for PC increment
        vm.current_thread = vm.heap.allocate_thread(create_test_closure(&mut vm.heap)).unwrap();
        
        let base = 5;
        let var_count = 2;
        
        // Simulate iterator returning nil (end of iteration)
        let results = vec![Value::Nil];
        
        let result = handle_tforloop_return(
            &mut vm, window, base, var_count, 10, results, &mut tx
        );
        assert!(result.is_ok());
        
        // Verify first loop variable is nil
        assert_eq!(
            *vm.register_windows.get_register(window, base + 3).unwrap(),
            Value::Nil
        );
        
        // PC should be incremented (via transaction)
        assert!(!tx.operations.is_empty());
        // In a real scenario, the transaction would increment PC
    }
    
    #[test]
    fn test_handle_return_partial_results() {
        let mut vm = create_test_vm();
        let window = vm.register_windows.allocate_window(20).unwrap();
        let mut tx = HeapTransaction::new();
        
        let base = 5;
        let var_count = 3; // Expecting 3 values
        
        // Iterator only returns 1 value
        let results = vec![Value::Number(42.0)];
        
        let result = handle_tforloop_return(
            &mut vm, window, base, var_count, 10, results, &mut tx
        );
        assert!(result.is_ok());
        
        // First variable gets the value
        assert_eq!(
            *vm.register_windows.get_register(window, base + 3).unwrap(),
            Value::Number(42.0)
        );
        
        // Remaining variables get nil
        assert_eq!(
            *vm.register_windows.get_register(window, base + 4).unwrap(),
            Value::Nil
        );
        assert_eq!(
            *vm.register_windows.get_register(window, base + 5).unwrap(),
            Value::Nil
        );
        
        // Control variable updated with first result
        assert_eq!(
            *vm.register_windows.get_register(window, base + 2).unwrap(),
            Value::Number(42.0)
        );
    }
}