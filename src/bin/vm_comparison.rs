//! VM Comparison Demo
//! 
//! This binary demonstrates that the RefCellVM correctly handles for loops
//! while the transaction-based VM suffers from register corruption.

use ferrous::lua::{compile, LuaVM, RefCellVM, Value};

fn main() {
    println!("=== Comparing VM Implementations for For Loop Handling ===\n");
    
    // Simple for loop test that sums numbers 1 to 5
    let script = r#"
        -- Simple for loop test that should sum 1+2+3+4+5 = 15
        local sum = 0
        for i = 1, 5 do
            sum = sum + i
        end
        return sum
    "#;
    
    // Compile the script
    let module = match compile(script) {
        Ok(m) => {
            println!("✓ Successfully compiled test script");
            m
        },
        Err(e) => {
            println!("❌ Failed to compile script: {}", e);
            return;
        }
    };
    
    // Test with transaction-based VM
    println!("\n=== Testing Transaction-Based VM ===");
    let tx_result = run_with_transaction_vm(&module);
    
    // Test with RefCell-based VM
    println!("\n=== Testing RefCell-Based VM ===");
    let rc_result = run_with_refcell_vm(&module);
    
    // Display comparison result
    println!("\n=== Comparison Results ===");
    if tx_result.is_ok() && rc_result.is_ok() {
        let tx_value = tx_result.unwrap();
        let rc_value = rc_result.unwrap();
        
        println!("Transaction VM result: {}", display_value(&tx_value));
        println!("RefCell VM result: {}", display_value(&rc_value));
        
        match (tx_value, rc_value) {
            (Value::Number(tx_n), Value::Number(rc_n)) => {
                let expected = 15.0;
                
                if tx_n == expected && rc_n == expected {
                    println!("\nBoth implementations succeeded! No difference detected.");
                } else if tx_n != expected && rc_n == expected {
                    println!("\n✓ RefCell VM correctly calculated the sum ({})", expected);
                    println!("❌ Transaction VM calculated the wrong answer ({} instead of {})", tx_n, expected);
                    println!("\nThis confirms that the RefCellVM fixes the register corruption bug in the");
                    println!("transaction-based VM. The direct register access in the RefCellVM maintains");
                    println!("register state across opcodes, while the transaction-based VM loses values.");
                } else if tx_n == expected && rc_n != expected {
                    println!("\n❌ Unexpected result: Transaction VM succeeded but RefCell VM failed");
                    println!("Expected: {}, Transaction VM: {}, RefCell VM: {}", expected, tx_n, rc_n);
                } else {
                    println!("\n❌ Both implementations failed with different results");
                    println!("Expected: {}, Transaction VM: {}, RefCell VM: {}", expected, tx_n, rc_n);
                }
            },
            _ => {
                println!("\nUnable to compare numeric results - returned non-numeric values");
            }
        }
    } else {
        if tx_result.is_err() {
            println!("❌ Transaction VM failed: {}", tx_result.unwrap_err());
        }
        
        if rc_result.is_err() {
            println!("❌ RefCell VM failed: {}", rc_result.unwrap_err());
        }
        
        if tx_result.is_err() && rc_result.is_ok() {
            println!("\n✓ RefCell VM succeeded while Transaction VM failed!");
            println!("RefCell VM result: {}", display_value(&rc_result.unwrap()));
            println!("\nThis confirms that the RefCellVM fixes a critical issue in the");
            println!("transaction-based VM's handling of for loops.");
        }
    }
}

/// Helper to display a Value
fn display_value(value: &Value) -> String {
    match value {
        Value::Nil => "nil".to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(_) => "<string>".to_string(),
        Value::Table(_) => "<table>".to_string(),
        Value::Closure(_) => "<function>".to_string(),
        Value::CFunction(_) => "<C function>".to_string(),
        _ => format!("{:?}", value),
    }
}

/// Run test with transaction-based VM
fn run_with_transaction_vm(module: &ferrous::lua::compiler::CompiledModule) -> Result<Value, String> {
    // Create VM
    let mut vm = match LuaVM::new() {
        Ok(vm) => vm,
        Err(e) => return Err(format!("Error creating Transaction VM: {}", e)),
    };
    
    // Initialize standard library
    if let Err(e) = vm.init_stdlib() {
        return Err(format!("Error initializing stdlib: {}", e));
    }
    
    // Execute module
    println!("Executing for loop with Transaction VM...");
    match vm.execute_module(module, &[]) {
        Ok(value) => Ok(value),
        Err(e) => Err(format!("Execution error: {}", e)),
    }
}

/// Run test with RefCell-based VM
fn run_with_refcell_vm(module: &ferrous::lua::compiler::CompiledModule) -> Result<Value, String> {
    // Create VM
    let mut vm = match RefCellVM::new() {
        Ok(vm) => vm,
        Err(e) => return Err(format!("Error creating RefCell VM: {}", e)),
    };
    
    // Initialize standard library
    if let Err(e) = vm.init_stdlib() {
        return Err(format!("Error initializing stdlib: {}", e));
    }
    
    // Execute module
    println!("Executing for loop with RefCell VM...");
    match vm.execute_module(module, &[]) {
        Ok(value) => Ok(value),
        Err(e) => Err(format!("Execution error: {}", e)),
    }
}