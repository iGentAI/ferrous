//! Test specifically for table field access and concatenation
//! 
//! This test focuses on the issue where table field access was not working correctly
//! in concatenation expressions, producing "bar baz42" instead of "bar 42"

use ferrous::lua::vm::LuaVm;
use ferrous::lua::value::{LuaValue, LuaString};
use ferrous::lua::error::Result;

fn main() -> Result<()> {
    println!("=== Ferrous Lua Table Field Access Test ===\n");
    
    // Create a VM and properly initialize it
    let mut vm = LuaVm::new();
    vm.init_std_libs()?;
    
    // Test 1: Basic table field access
    println!("Test 1: Basic table field access");
    let script = r#"
        local t = {foo = "bar", baz = 42}
        return t.foo
    "#;
    
    match vm.run(script) {
        Ok(result) => {
            if let LuaValue::String(s) = &result {
                if let Ok(s_str) = s.to_str() {
                    println!("Result: \"{}\" (Expected: \"bar\") - {}", 
                           s_str, if s_str == "bar" { "PASS" } else { "FAIL" });
                } else {
                    println!("FAIL: String conversion error");
                }
            } else {
                println!("FAIL: Expected string, got {:?}", result);
            }
        },
        Err(e) => println!("ERROR: {}", e),
    }
    
    // Test 2: Table field with number value
    println!("\nTest 2: Table field with number value");
    let script = r#"
        local t = {foo = "bar", baz = 42}
        return t.baz
    "#;
    
    match vm.run(script) {
        Ok(result) => {
            if let LuaValue::Number(n) = result {
                println!("Result: {} (Expected: 42) - {}", 
                         n, if n == 42.0 { "PASS" } else { "FAIL" });
            } else {
                println!("FAIL: Expected number, got {:?}", result);
            }
        },
        Err(e) => println!("ERROR: {}", e),
    }
    
    // Test 3: Table field concatenation (the issue)
    println!("\nTest 3: Table field concatenation (previously failing)");
    let script = r#"
        local t = {foo = "bar", baz = 42}
        return t.foo .. " " .. t.baz
    "#;
    
    match vm.run(script) {
        Ok(result) => {
            if let LuaValue::String(s) = &result {
                if let Ok(s_str) = s.to_str() {
                    println!("Result: \"{}\" (Expected: \"bar 42\") - {}", 
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
    
    // Test 4: Table field concatenation with more fields
    println!("\nTest 4: Multiple table field concatenation");
    let script = r#"
        local t = {first = "Hello", second = "beautiful", third = "world"}
        return t.first .. " " .. t.second .. " " .. t.third
    "#;
    
    match vm.run(script) {
        Ok(result) => {
            if let LuaValue::String(s) = &result {
                if let Ok(s_str) = s.to_str() {
                    println!("Result: \"{}\" (Expected: \"Hello beautiful world\") - {}", 
                           s_str, if s_str == "Hello beautiful world" { "PASS" } else { "FAIL" });
                } else {
                    println!("FAIL: String conversion error");
                }
            } else {
                println!("FAIL: Expected string, got {:?}", result);
            }
        },
        Err(e) => println!("ERROR: {}", e),
    }
    
    println!("\n=== Tests Complete ===");
    
    Ok(())
}