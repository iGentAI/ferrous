//! Comparison of FOR loop execution between Transaction-based and RefCell-based VMs
//! 
//! This example demonstrates the register corruption bug that occurs in the 
//! transaction-based VM during FOR loop execution, while the RefCell-based VM
//! handles it correctly.

use ferrous::lua::{
    LuaVM,
    RefCellVM,
    Instruction,
    OpCode,
    Value,
    Closure,
    FunctionProto,
    LuaError,
    LuaResult,
};
use std::io::{self, Write};

/// Create a simple for loop function manually
/// 
/// This creates bytecode equivalent to:
/// ```lua
/// local sum = 0
/// for i = 1, 5 do
///     sum = sum + i
/// end
/// return sum
/// ```
fn create_for_loop_function() -> FunctionProto {
    let mut bytecode = Vec::new();
    let mut constants = Vec::new();
    
    // Constants we'll need
    constants.push(Value::Number(0.0));  // [0] initial sum
    constants.push(Value::Number(1.0));  // [1] loop start
    constants.push(Value::Number(5.0));  // [2] loop limit
    constants.push(Value::Number(1.0));  // [3] loop step
    
    // R(0) = 0 (sum)
    bytecode.push(Instruction::create(OpCode::LoadK, 0, 0, 0).0);
    
    // R(1) = 1 (loop start)
    bytecode.push(Instruction::create(OpCode::LoadK, 1, 1, 0).0);
    
    // R(2) = 5 (loop limit)
    bytecode.push(Instruction::create(OpCode::LoadK, 2, 2, 0).0);
    
    // R(3) = 1 (loop step)
    bytecode.push(Instruction::create(OpCode::LoadK, 3, 3, 0).0);
    
    // FORPREP R(1), 4 (jump to end if loop shouldn't run)
    // This subtracts step from initial and checks if loop should execute
    bytecode.push(Instruction::create_sBx(OpCode::ForPrep, 1, 4).0);
    
    // Loop body starts here
    // R(0) = R(0) + R(4) (sum = sum + i)
    // Note: R(4) is the user-visible loop variable set by FORLOOP
    bytecode.push(Instruction::create_ABC(OpCode::Add, 0, 0, 4).0);
    
    // FORLOOP R(1), -2 (increment counter and jump back if continuing)
    bytecode.push(Instruction::create_sBx(OpCode::ForLoop, 1, -2).0);
    
    // End of loop - return sum
    bytecode.push(Instruction::create_ABC(OpCode::Return, 0, 2, 0).0);
    
    FunctionProto {
        bytecode,
        constants,
        num_params: 0,
        is_vararg: false,
        max_stack_size: 5, // Need 5 registers: sum + 4 for loop
        upvalues: vec![],
    }
}

/// Execute the function on the transaction-based VM
fn execute_on_transaction_vm() -> LuaResult<Value> {
    println!("\n=== Executing on Transaction-based VM ===");
    
    // Create VM
    let mut vm = LuaVM::new()?;
    
    // Create the for loop closure
    let proto = create_for_loop_function();
    let closure = Closure {
        proto,
        upvalues: vec![],
    };
    
    // Create closure in heap using a transaction
    let closure_handle = {
        let mut tx = vm.create_debug_transaction(1000);
        let handle = tx.create_closure(closure)?;
        tx.commit()?;
        handle
    };
    
    println!("Created closure, starting execution...");
    
    // Execute with detailed logging
    let results = match vm.execute(closure_handle) {
        Ok(results) => results,
        Err(e) => {
            println!("ERROR during execution: {}", e);
            return Err(e);
        }
    };
    
    // Get the return value (sum)
    let sum = results.get(0).cloned().unwrap_or(Value::Nil);
    println!("Execution completed. Result: {:?}", sum);
    
    Ok(sum)
}

/// Execute the function on the RefCell-based VM
fn execute_on_refcell_vm() -> LuaResult<Value> {
    println!("\n=== Executing on RefCell-based VM ===");
    
    // Create VM
    let mut vm = RefCellVM::new()?;
    
    // Create the for loop closure
    let proto = create_for_loop_function();
    let closure = Closure {
        proto,
        upvalues: vec![],
    };
    
    // Create closure in heap
    let closure_handle = vm.heap().create_closure(closure)?;
    
    println!("Created closure, starting execution...");
    
    // Execute
    let results = match vm.execute(closure_handle) {
        Ok(results) => results,
        Err(e) => {
            println!("ERROR during execution: {}", e);
            return Err(e);
        }
    };
    
    // Get the return value (sum)
    let sum = results.get(0).cloned().unwrap_or(Value::Nil);
    println!("Execution completed. Result: {:?}", sum);
    
    Ok(sum)
}

/// Create a simpler test that just counts iterations
fn create_simple_counter_function() -> FunctionProto {
    let mut bytecode = Vec::new();
    let mut constants = Vec::new();
    
    // Constants
    constants.push(Value::Number(0.0));  // [0] counter
    constants.push(Value::Number(1.0));  // [1] loop start  
    constants.push(Value::Number(3.0));  // [2] loop limit
    // Note: No explicit step constant - testing default step behavior
    
    // R(0) = 0 (counter)
    bytecode.push(Instruction::create(OpCode::LoadK, 0, 0, 0).0);
    
    // R(1) = 1 (loop start)
    bytecode.push(Instruction::create(OpCode::LoadK, 1, 1, 0).0);
    
    // R(2) = 3 (loop limit)
    bytecode.push(Instruction::create(OpCode::LoadK, 2, 2, 0).0);
    
    // R(3) = nil (step will be initialized by FORPREP)
    bytecode.push(Instruction::create_ABC(OpCode::LoadNil, 3, 3, 0).0);
    
    // FORPREP R(1), 3 (prepare loop)
    bytecode.push(Instruction::create_sBx(OpCode::ForPrep, 1, 3).0);
    
    // Loop body: increment counter
    // R(0) = R(0) + 1
    bytecode.push(Instruction::create(OpCode::LoadK, 5, 1, 0).0); // R(5) = 1
    bytecode.push(Instruction::create_ABC(OpCode::Add, 0, 0, 5).0);
    
    // FORLOOP R(1), -3
    bytecode.push(Instruction::create_sBx(OpCode::ForLoop, 1, -3).0);
    
    // Return counter
    bytecode.push(Instruction::create_ABC(OpCode::Return, 0, 2, 0).0);
    
    FunctionProto {
        bytecode,
        constants,
        num_params: 0,
        is_vararg: false,
        max_stack_size: 6,
        upvalues: vec![],
    }
}

/// Test the simple counter on both VMs
fn test_simple_counter() -> LuaResult<()> {
    println!("\n=== Testing Simple Counter (for i = 1, 3) ===");
    println!("Expected: Counter should increment 3 times, returning 3.0");
    
    // Transaction VM
    {
        println!("\n--- Transaction VM ---");
        let mut vm = LuaVM::new()?;
        let proto = create_simple_counter_function();
        let closure = Closure { proto, upvalues: vec![] };
        
        let closure_handle = {
            let mut tx = vm.create_debug_transaction(1000);
            let handle = tx.create_closure(closure)?;
            tx.commit()?;
            handle
        };
        
        match vm.execute(closure_handle) {
            Ok(results) => {
                let result = results.get(0).cloned().unwrap_or(Value::Nil);
                println!("Result: {:?}", result);
                
                if let Value::Number(n) = result {
                    if n == 3.0 {
                        println!("✓ CORRECT: Loop executed 3 times");
                    } else {
                        println!("✗ WRONG: Expected 3.0, got {}", n);
                    }
                } else {
                    println!("✗ ERROR: Expected number, got {:?}", result);
                }
            }
            Err(e) => {
                println!("✗ FAILED with error: {}", e);
            }
        }
    }
    
    // RefCell VM
    {
        println!("\n--- RefCell VM ---");
        let mut vm = RefCellVM::new()?;
        let proto = create_simple_counter_function();
        let closure = Closure { proto, upvalues: vec![] };
        
        let closure_handle = vm.heap().create_closure(closure)?;
        
        match vm.execute(closure_handle) {
            Ok(results) => {
                let result = results.get(0).cloned().unwrap_or(Value::Nil);
                println!("Result: {:?}", result);
                
                if let Value::Number(n) = result {
                    if n == 3.0 {
                        println!("✓ CORRECT: Loop executed 3 times");
                    } else {
                        println!("✗ WRONG: Expected 3.0, got {}", n);
                    }
                } else {
                    println!("✗ ERROR: Expected number, got {:?}", result);
                }
            }
            Err(e) => {
                println!("✗ FAILED with error: {}", e);
            }
        }
    }
    
    Ok(())
}

fn main() {
    println!("FOR Loop VM Comparison");
    println!("======================");
    println!("\nThis demonstrates the FOR loop register corruption issue in the");
    println!("transaction-based VM compared to the RefCell-based VM.");
    
    // First test: Simple counter
    if let Err(e) = test_simple_counter() {
        eprintln!("Simple counter test failed: {}", e);
    }
    
    println!("\n\n=== Testing Sum Calculation (for i = 1, 5) ===");
    println!("Expected: 1 + 2 + 3 + 4 + 5 = 15.0");
    
    // Test on transaction VM
    match execute_on_transaction_vm() {
        Ok(Value::Number(n)) => {
            if n == 15.0 {
                println!("✓ Transaction VM: CORRECT result = {}", n);
            } else {
                println!("✗ Transaction VM: WRONG result = {} (expected 15.0)", n);
                println!("  This indicates register corruption during FOR loop execution!");
            }
        }
        Ok(other) => {
            println!("✗ Transaction VM: WRONG type = {:?} (expected Number)", other);
        }
        Err(e) => {
            println!("✗ Transaction VM: FAILED with error: {}", e);
            println!("  This likely indicates the FOR loop failed due to register issues.");
        }
    }
    
    // Test on RefCell VM
    match execute_on_refcell_vm() {
        Ok(Value::Number(n)) => {
            if n == 15.0 {
                println!("✓ RefCell VM: CORRECT result = {}", n);
            } else {
                println!("✗ RefCell VM: WRONG result = {} (expected 15.0)", n);
            }
        }
        Ok(other) => {
            println!("✗ RefCell VM: WRONG type = {:?} (expected Number)", other);
        }
        Err(e) => {
            println!("✗ RefCell VM: FAILED with error: {}", e);
        }
    }
    
    println!("\n=== Summary ===");
    println!("The RefCell-based VM correctly handles FOR loops because it writes");
    println!("register values directly to memory, ensuring they persist across");
    println!("instruction boundaries.");
    println!("\nThe transaction-based VM has issues with FOR loops because register");
    println!("values (particularly the step value) can be lost or corrupted when");
    println!("crossing transaction boundaries between FORPREP and FORLOOP opcodes.");
}