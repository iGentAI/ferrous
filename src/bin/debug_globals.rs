use lua_vm::{Vm, Value};

fn main() {
    println!("=== Lua VM Globals Debug Test ===\n");
    
    // Step 1: Create and initialize VM
    println!("Step 1: Creating new VM...");
    let mut vm = Vm::new();
    println!("✓ VM created successfully\n");
    
    // Step 2: Register standard library
    println!("Step 2: Registering standard library...");
    vm.register_stdlib();
    println!("✓ Standard library registered\n");
    
    // Step 3: Check globals table directly
    println!("Step 3: Checking if 'print' exists in globals table...");
    match vm.debug_get_global("print") {
        Some(value) => {
            println!("✓ Found 'print' in globals table!");
            match value {
                Value::Function(_) => println!("  Type: Function (as expected)"),
                _ => println!("  Type: {:?} (unexpected!)", value),
            }
        },
        None => {
            println!("✗ 'print' NOT FOUND in globals table!");
        }
    }
    println!();
    
    // Step 4: Try to access print via Lua script
    println!("Step 4: Attempting to execute Lua script that uses print...");
    let test_script = r#"
print("Hello from Lua!")
"#;
    
    match vm.load_string(test_script, "test") {
        Ok(chunk_id) => {
            println!("✓ Script loaded successfully (chunk_id: {})", chunk_id);
            println!("  Executing script...");
            
            match vm.execute(chunk_id) {
                Ok(_) => {
                    println!("✓ Script executed successfully!");
                },
                Err(e) => {
                    println!("✗ Script execution failed: {:?}", e);
                }
            }
        },
        Err(e) => {
            println!("✗ Failed to load script: {:?}", e);
        }
    }
    println!();
    
    // Step 5: Additional diagnostics - check other globals
    println!("Step 5: Additional diagnostics - checking other standard functions...");
    let standard_functions = ["print", "type", "tostring", "tonumber", "assert", "error"];
    
    for func_name in &standard_functions {
        match vm.debug_get_global(func_name) {
            Some(Value::Function(_)) => println!("  ✓ '{}' found (Function)", func_name),
            Some(value) => println!("  ⚠ '{}' found but wrong type: {:?}", func_name, value),
            None => println!("  ✗ '{}' NOT FOUND", func_name),
        }
    }
    println!();
    
    // Step 6: Try accessing print with different methods
    println!("Step 6: Testing different access patterns...");
    
    // Try a simple global access
    let simple_test = "return print";
    match vm.load_string(simple_test, "simple_test") {
        Ok(chunk_id) => {
            println!("  Testing: {}", simple_test);
            match vm.execute(chunk_id) {
                Ok(values) => {
                    println!("  ✓ Execution successful, returned: {:?}", values);
                },
                Err(e) => {
                    println!("  ✗ Execution failed: {:?}", e);
                }
            }
        },
        Err(e) => {
            println!("  ✗ Failed to load simple test: {:?}", e);
        }
    }
    
    // Try storing and retrieving
    let store_test = r#"
_G.test_print = print
return _G.test_print
"#;
    match vm.load_string(store_test, "store_test") {
        Ok(chunk_id) => {
            println!("\n  Testing global table access:");
            match vm.execute(chunk_id) {
                Ok(values) => {
                    println!("  ✓ Store/retrieve successful, returned: {:?}", values);
                },
                Err(e) => {
                    println!("  ✗ Store/retrieve failed: {:?}", e);
                }
            }
        },
        Err(e) => {
            println!("  ✗ Failed to load store test: {:?}", e);
        }
    }
    
    println!("\n=== End of diagnostic test ===");
}