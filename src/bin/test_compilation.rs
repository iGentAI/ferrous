//! Test program for the new compilation-based Lua execution.
//! 
//! This binary verifies that the new architecture with separated compilation
//! and execution phases works correctly.

use ferrous::lua_new::{compiler, vm, VMConfig};
use ferrous::lua_new::value::Value;

fn main() {
    println!("=== Testing New Compilation-Based Lua Implementation ===");
    
    // Create a VM with default configuration
    let mut vm = vm::LuaVM::new(VMConfig::default());
    
    // Create a compiler
    let mut compiler = compiler::Compiler::new();
    compiler.set_heap(&mut vm.heap as *mut _);
    
    // Test scripts of increasing complexity
    let scripts = [
        ("return 'hello, world'", "hello, world"),
        ("local x = 10; local y = 20; return x + y", "30"),
        ("return 2 + 2 * 3", "8"),
        ("local t = {foo = 'bar'}; return t.foo", "bar"),
        ("local function add(a, b) return a + b end; return add(10, 20)", "30")
    ];
    
    let mut success_count = 0;
    let total_tests = scripts.len();
    
    for (i, (script, expected)) in scripts.iter().enumerate() {
        println!("\nTest {} - Script: {}", i + 1, script);
        println!("Expected result: {}", expected);
        
        // Compile the script
        match compiler.compile(script) {
            Ok(compilation) => {
                println!("Compilation succeeded:");
                println!("  String pool: {} entries", compilation.string_pool.len());
                println!("  Constants: {} entries", compilation.main_proto.constants.len());
                println!("  Code: {} instructions", compilation.main_proto.code.len());
                println!("  Nested prototypes: {}", compilation.main_proto.nested_protos.len());
                
                // Try loading the compilation
                match vm.load_compilation_script(&compilation) {
                    Ok(closure) => {
                        println!("Loading succeeded, closure handle: {:?}", closure);
                        
                        // Execute the script
                        match vm.execute_function(closure, &[]) {
                            Ok(result) => {
                                let result_str = match result {
                                    Value::String(handle) => {
                                        if let Ok(bytes) = vm.heap.get_string(handle) {
                                            String::from_utf8_lossy(bytes).to_string()
                                        } else {
                                            "<invalid string handle>".to_string()
                                        }
                                    }
                                    Value::Number(n) => n.to_string(),
                                    _ => format!("{:?}", result)
                                };
                                
                                println!("Execution succeeded, result: {}", result_str);
                                
                                // Check against expected
                                if result_str == *expected {
                                    println!("✅ PASS: Result matches expected");
                                    success_count += 1;
                                } else {
                                    println!("❌ FAIL: Result doesn't match expected");
                                }
                            }
                            Err(e) => {
                                println!("❌ FAIL: Execution failed: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        println!("❌ FAIL: Loading failed: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("❌ FAIL: Compilation failed: {}", e);
            }
        }
    }
    
    println!("\n=== Test Results ===");
    println!("Passed: {}/{} tests", success_count, total_tests);
    println!("Success rate: {}%", (success_count as f64 / total_tests as f64) * 100.0);
    
    // This is just a test program, so exit with appropriate code
    if success_count == total_tests {
        println!("All tests passed!");
        std::process::exit(0);
    } else {
        println!("Some tests failed!");
        std::process::exit(1);
    }
}