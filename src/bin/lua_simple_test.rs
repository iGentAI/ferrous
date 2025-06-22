//! Simple test for the Lua VM implementation
//! 
//! Tests the most important features of the Lua VM in isolation.

use ferrous::lua::vm::LuaVm;
use ferrous::lua::value::{LuaValue, LuaString};
use ferrous::lua::error::Result;

fn main() -> Result<()> {
    // Create a VM and initialize it
    let mut vm = LuaVm::new();
    vm.init_std_libs()?;
    
    println!("=== Ferrous Lua Simple Feature Test ===\n");
    
    // Test 1: Basic arithmetic
    println!("Test 1: Basic addition");
    let script = "return 1 + 2";
    
    match vm.run(script) {
        Ok(result) => {
            if let LuaValue::Number(n) = result {
                println!("Result: {} (Expected: 3) - {}", 
                         n, if n == 3.0 { "PASS" } else { "FAIL" });
            } else {
                println!("FAIL: Expected number, got {:?}", result);
            }
        },
        Err(e) => println!("ERROR: {}", e),
    }
    
    // Test 2: String concatenation
    println!("\nTest 2: String concatenation");
    let script = "return 'hello' .. ' ' .. 'world'";
    
    match vm.run(script) {
        Ok(result) => {
            if let LuaValue::String(s) = &result {
                if let Ok(s_str) = s.to_str() {
                    println!("Result: {} (Expected: hello world) - {}", 
                           s_str, if s_str == "hello world" { "PASS" } else { "FAIL" });
                } else {
                    println!("FAIL: String conversion error");
                }
            } else {
                println!("FAIL: Expected string, got {:?}", result);
            }
        },
        Err(e) => println!("ERROR: {}", e),
    }
    
    // Test 3: Basic function
    println!("\nTest 3: Basic function call");
    let script = r#"
        local function add(a, b)
            return a + b
        end
        return add(10, 20)
    "#;
    
    match vm.run(script) {
        Ok(result) => {
            if let LuaValue::Number(n) = result {
                println!("Result: {} (Expected: 30) - {}", 
                         n, if n == 30.0 { "PASS" } else { "FAIL" });
            } else {
                println!("FAIL: Expected number, got {:?}", result);
            }
        },
        Err(e) => println!("ERROR: {}", e),
    }
    
    // Test 4: Table handling
    println!("\nTest 4: Table handling");
    let script = r#"
        local t = {foo = "bar", baz = 42}
        return t.foo .. " " .. t.baz
    "#;
    
    match vm.run(script) {
        Ok(result) => {
            if let LuaValue::String(s) = &result {
                if let Ok(s_str) = s.to_str() {
                    println!("Result: {} (Expected: bar 42) - {}", 
                           s_str, if s_str == "bar 42" { "PASS" } else { "FAIL" });
                } else {
                    println!("FAIL: String conversion error");
                }
            } else {
                println!("FAIL: Expected string, got {:?}", result);
            }
        },
        Err(e) => println!("ERROR: {}", e),
    }
    
    // Test 5: Simple closure (counter)
    println!("\nTest 5: Simple closure (counter)");
    let script = r#"
        local function make_counter()
            local count = 0
            return function()
                count = count + 1
                return count
            end
        end
        
        local counter = make_counter()
        counter()  -- 1
        return counter()  -- 2
    "#;
    
    match vm.run(script) {
        Ok(result) => {
            if let LuaValue::Number(n) = result {
                println!("Result: {} (Expected: 2) - {}", 
                         n, if n == 2.0 { "PASS" } else { "FAIL" });
            } else {
                println!("FAIL: Expected number, got {:?}", result);
            }
        },
        Err(e) => println!("ERROR: {}", e),
    }
    
    println!("\n=== Tests Complete ===");
    
    Ok(())
}