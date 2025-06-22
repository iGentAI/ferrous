//! Test program for Lua metatable support
//! 
//! This test verifies that metatables and metamethods work correctly.

use ferrous::lua::vm::LuaVm;
use ferrous::lua::value::{LuaValue, LuaString};
use ferrous::lua::error::Result;

fn main() -> Result<()> {
    println!("=== Ferrous Lua Metatable Test ===\n");
    
    // Create a VM and properly initialize it
    let mut vm = LuaVm::new();
    vm.init_std_libs()?;
    
    // Test 1: Basic __index metamethod
    println!("Test 1: Basic __index metamethod");
    let script = r#"
        local t = {}
        local mt = {
            __index = function(table, key)
                if key == "test" then
                    return "success"
                else
                    return "unknown key: " .. key
                end
            end
        }
        setmetatable(t, mt)
        
        return t.test -- Should return "success"
    "#;
    
    match vm.run(script) {
        Ok(result) => {
            if let LuaValue::String(s) = &result {
                if let Ok(s_str) = s.to_str() {
                    println!("Result: \"{}\" (Expected: \"success\") - {}", 
                          s_str, if s_str == "success" { "PASS" } else { "FAIL" });
                } else {
                    println!("FAIL: String conversion error");
                }
            } else {
                println!("FAIL: Expected string, got {:?}", result);
            }
        },
        Err(e) => println!("ERROR: {}", e),
    }
    
    // Test 2: Table __index metamethod
    println!("\nTest 2: Table __index metamethod");
    let script = r#"
        local t = {}
        local mt = {
            __index = { foo = "bar", baz = 42 }
        }
        setmetatable(t, mt)
        
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
    
    // Test 3: __newindex metamethod
    println!("\nTest 3: __newindex metamethod");
    let script = r#"
        local t = {}
        local storage = {}
        local mt = {
            __newindex = function(table, key, value)
                storage[key] = value
            end,
            __index = function(table, key)
                return storage[key]
            end
        }
        setmetatable(t, mt)
        
        t.foo = "bar" -- This should go to storage
        t.baz = 42    -- This should go to storage
        
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
    
    // Test 4: Multiple metamethods
    println!("\nTest 4: Multiple metamethods");
    let script = r#"
        local t1 = {value = 10}
        local t2 = {value = 20}
        
        local mt1 = {
            __index = function(t, k)
                if k == "get_value" then
                    return function() return t.value end
                end
            end
        }
        
        local mt2 = {
            __index = function(t, k)
                if k == "get_value" then
                    return function() return t.value end
                end
            end
        }
        
        setmetatable(t1, mt1)
        setmetatable(t2, mt2)
        
        return t1.get_value() + t2.get_value()  -- Should be 30
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
    
    println!("\n=== Tests Complete ===");
    
    Ok(())
}