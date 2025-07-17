//! Compare VM Implementations
//! 
//! This binary demonstrates the difference between the transaction-based VM
//! and the RefCell-based VM when handling FOR loops.

use ferrous::lua::{compile, LuaVM, RefCellVM, Value, loader::load_module};
use ferrous::lua::value::{FunctionProto, Closure};
use ferrous::lua::error::LuaResult;

fn main() {
    println!("╔══════════════════════════════════════════════════════╗");
    println!("║        Lua VM FOR Loop Implementation Comparison      ║");
    println!("╚══════════════════════════════════════════════════════╝");
    println!();
    
    // Simple test script
    let script = r#"
-- Minimal for loop test
local count = 0
for i = 1, 3 do
    count = count + 1
end
return count  -- Should return 3
"#;
    
    println!("Test Script:");
    println!("```lua");
    println!("{}", script.trim());
    println!("```");
    println!();
    println!("Expected Result: 3 (three iterations)");
    println!();
    
    // Compile the script
    let module = match compile(&script) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("❌ Compilation Error: {}", e);
            std::process::exit(1);
        }
    };
    
    println!("✓ Script compiled successfully");
    println!();
    println!("─────────────────────────────────────────────────────────");
    println!();
    
    // Test with transaction-based VM
    println!("1. TRANSACTION-BASED VM (Original Implementation)");
    println!("   ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    let tx_result = test_transaction_vm(&module);
    
    println!();
    println!("─────────────────────────────────────────────────────────");
    println!();
    
    // Test with RefCell VM  
    println!("2. REFCELL-BASED VM (Fixed Implementation)");
    println!("   ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    let ref_result = test_refcell_vm(&module);
    
    println!();
    println!("═════════════════════════════════════════════════════════");
    println!();
    println!("ANALYSIS:");
    println!("─────────");
    
    match (tx_result, ref_result) {
        (Err(_), Ok(Value::Number(3.0))) => {
            println!("✅ RefCell VM correctly executes the FOR loop");
            println!("❌ Transaction VM fails due to register corruption");
            println!();
            println!("ROOT CAUSE:");
            println!("• In the transaction-based VM, FORPREP writes the step value");
            println!("• When the transaction commits, these writes are applied"); 
            println!("• FORLOOP executes in a new transaction and can't see the step");
            println!("• This causes a nil value type error when accessing the step");
            println!();
            println!("THE FIX:");
            println!("• RefCell VM uses interior mutability (RefCell<T>)");
            println!("• All register writes are immediately visible");
            println!("• No transaction boundaries between instructions");
            println!("• FORLOOP can always see FORPREP's register writes");
        }
        (Ok(v1), Ok(v2)) if v1 == v2 => {
            println!("⚠️  Both VMs produced the same result: {:?}", v1);
            println!("    This is unexpected - the issue may not be reproducing");
        }
        _ => {
            println!("🤔 Unexpected results:");
            println!("   Transaction VM: {:?}", tx_result);
            println!("   RefCell VM: {:?}", ref_result);
        }
    }
    println!();
}

fn test_transaction_vm(module: &ferrous::lua::CompiledModule) -> Result<Value, String> {
    print!("   Creating VM instance... ");
    
    let mut vm = match LuaVM::new() {
        Ok(vm) => {
            println!("✓");
            vm
        }
        Err(e) => {
            println!("❌");
            return Err(format!("VM creation failed: {}", e));
        }
    };
    
    print!("   Executing FOR loop... ");
    
    match vm.execute_module(module, &[]) {
        Ok(result) => {
            println!("✓");
            println!("   Result: {:?}", result);
            Ok(result)
        }
        Err(e) => {
            println!("❌");
            println!("   Error: {}", e);
            println!();
            println!("   🔍 Debug Info:");
            println!("      • Error occurs in FORLOOP instruction");
            println!("      • Step register contains nil instead of 1");
            println!("      • Register state lost across transaction boundary");
            Err(e.to_string())
        }
    }
}

fn test_refcell_vm(module: &ferrous::lua::CompiledModule) -> Result<Value, String> {
    print!("   Creating VM instance... ");
    
    let mut vm = match RefCellVM::new() {
        Ok(vm) => {
            println!("✓");
            vm
        }
        Err(e) => {
            println!("❌");
            return Err(format!("VM creation failed: {}", e));
        }
    };
    
    print!("   Loading function prototype... ");
    
    // Load the module into the RefCellVM's heap
    let proto_handle = {
        let heap = vm.heap_mut();
        let mut tx = heap.begin_transaction();
        
        match load_module(&mut tx, module) {
            Ok(handle) => {
                tx.commit().map_err(|e| format!("Transaction commit failed: {}", e))?;
                println!("✓");
                handle
            }
            Err(e) => {
                println!("❌");
                return Err(format!("Module loading failed: {}", e));
            }
        }
    };
    
    print!("   Creating closure... ");
    
    // Create closure from the loaded prototype
    let closure_handle = {
        let heap = vm.heap_mut();
        let mut tx = heap.begin_transaction();
        
        let closure_result: LuaResult<_> = (|| {
            let closure = Closure {
                proto: proto_handle,
                upvalues: vec![],
            };
            tx.create_closure(closure)
        })();
        
        match closure_result {
            Ok(handle) => {
                tx.commit().map_err(|e| format!("Transaction commit failed: {}", e))?;
                println!("✓");
                handle
            }
            Err(e) => {
                println!("❌");
                return Err(format!("Closure creation failed: {}", e));
            }
        }
    };
    
    print!("   Executing FOR loop... ");
    
    match vm.execute(closure_handle) {
        Ok(results) => {
            println!("✓");
            
            // Get the first (and only) return value
            let result = results.into_iter().next().unwrap_or(Value::Nil);
            println!("   Result: {:?}", result);
            
            if let Value::Number(n) = result {
                if n == 3.0 {
                    println!("   ✅ Correct! Loop executed 3 times");
                } else {
                    println!("   ⚠️  Unexpected count: {}", n);
                }
            }
            
            Ok(result)
        }
        Err(e) => {
            println!("❌");
            println!("   Error: {}", e);
            Err(e.to_string())
        }
    }
}