//! Test program for Lua-to-Lua function calls and closure upvalue handling
//! 
//! This test verifies that functions can call other Lua functions and that 
//! closures can access variables from outer scopes correctly.

use ferrous::lua::vm::LuaVm;
use ferrous::lua::value::{LuaValue, LuaString};
use ferrous::lua::error::Result;

fn main() -> Result<()> {
    println!("=== Ferrous Lua Function Call Test ===\n");
    
    // Create a VM and properly initialize it
    let mut vm = LuaVm::new();
    vm.init_std_libs()?;
    
    // Test 1: Basic function with return value
    println!("Test 1: Basic function with return value");
    let script = r#"
        local function answer()
            return 42
        end
        
        return answer()
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
    
    // Test 2: Function with parameters
    println!("\nTest 2: Function with parameters");
    let script = r#"
        local function add(a, b)
            return a + b
        end
        
        return add(10, 5)  -- Should return 15
    "#;
    
    match vm.run(script) {
        Ok(result) => {
            if let LuaValue::Number(n) = result {
                println!("Result: {} (Expected: 15) - {}", 
                      n, if n == 15.0 { "PASS" } else { "FAIL" });
            } else {
                println!("FAIL: Expected number, got {:?}", result);
            }
        },
        Err(e) => println!("ERROR: {}", e),
    }
    
    // Test 3: Nested function call
    println!("\nTest 3: Nested function call");
    let script = r#"
        local function add(a, b)
            return a + b
        end
        
        local function multiply(a, b)
            return a * b
        end
        
        local function calculate(x, y, z)
            local sum = add(x, y)
            return multiply(sum, z)
        end
        
        return calculate(10, 5, 2)  -- Should return (10+5)*2 = 30
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
    
    // Test 4: Simple closure (upvalue from parent)
    println!("\nTest 4: Simple closure (upvalue from parent)");
    let script = r#"
        local x = 10
        
        local function makeAdder(y)
            return function(z)
                return x + y + z  -- Captures both x and y
            end
        end
        
        local adder = makeAdder(5)
        return adder(3)  -- Should return 10+5+3 = 18
    "#;
    
    match vm.run(script) {
        Ok(result) => {
            if let LuaValue::Number(n) = result {
                println!("Result: {} (Expected: 18) - {}", 
                      n, if n == 18.0 { "PASS" } else { "FAIL" });
            } else {
                println!("FAIL: Expected number, got {:?}", result);
            }
        },
        Err(e) => println!("ERROR: {}", e),
    }
    
    // Test 5: Counter with closure (testing upvalue modification)
    println!("\nTest 5: Counter with closure (testing upvalue modification)");
    let script = r#"
        -- Create a counter factory function to test both closures and function calls
        local function makeCounter(start)
            start = start or 0
            local count = start
            
            return function()
                count = count + 1
                return count
            end
        end
        
        -- Create multiple counters to verify upvalues are properly isolated
        local counter1 = makeCounter(0)
        local counter2 = makeCounter(10)
        
        -- Call each counter multiple times to verify state preservation
        local a1 = counter1() -- 1
        local a2 = counter1() -- 2
        local b1 = counter2() -- 11
        local b2 = counter2() -- 12
        
        -- Using only direct value assignments to avoid any equality issues
        local result = {
            a1 = a1,
            a2 = a2,
            b1 = b1,
            b2 = b2
        }
        
        -- Add type checking information
        local debug = {
            a1_type = type(a1),
            a2_type = type(a2),
            b1_type = type(b1),
            b2_type = type(b2),
            
            a1_value = a1,
            a2_value = a2,
            b1_value = b1, 
            b2_value = b2,
            
            -- Manual comparisons
            a1_eq_1 = (a1 == 1),
            a2_eq_2 = (a2 == 2),
            b1_eq_11 = (b1 == 11),
            b2_eq_12 = (b2 == 12)
        }
        
        -- Store debug info
        result.debug = debug
        
        -- Force a known value for passed to simplify debugging
        -- The actual test verification will be done in Rust code
        result.passed = true
        
        return result
    "#;
    
    match vm.run(script) {
        Ok(result) => {
            if let LuaValue::Table(t) = &result {
                let table = t.borrow();
                
                // Get the counter values
                let a1 = match table.get(&LuaValue::String(LuaString::from_str("a1"))) {
                    Some(v) => format!("{:?}", v),
                    None => "missing".to_string()
                };
                
                let a2 = match table.get(&LuaValue::String(LuaString::from_str("a2"))) {
                    Some(v) => format!("{:?}", v),
                    None => "missing".to_string()
                };
                
                let b1 = match table.get(&LuaValue::String(LuaString::from_str("b1"))) {
                    Some(v) => format!("{:?}", v),
                    None => "missing".to_string()
                };
                
                let b2 = match table.get(&LuaValue::String(LuaString::from_str("b2"))) {
                    Some(v) => format!("{:?}", v),
                    None => "missing".to_string()
                };
                
                // Print debug info 
                if let Some(LuaValue::Table(debug_table)) = table.get(&LuaValue::String(LuaString::from_str("debug"))) {
                    let debug = debug_table.borrow();
                    println!("\nDebug info:");
                    
                    // Print type information
                    if let Some(v) = debug.get(&LuaValue::String(LuaString::from_str("a1_type"))) {
                        println!("  a1 type: {:?}", v);
                    }
                    if let Some(v) = debug.get(&LuaValue::String(LuaString::from_str("a2_type"))) {
                        println!("  a2 type: {:?}", v);
                    }
                    if let Some(v) = debug.get(&LuaValue::String(LuaString::from_str("b1_type"))) {
                        println!("  b1 type: {:?}", v);
                    }
                    if let Some(v) = debug.get(&LuaValue::String(LuaString::from_str("b2_type"))) {
                        println!("  b2 type: {:?}", v);
                    }
                    
                    // Print equality test results 
                    if let Some(v) = debug.get(&LuaValue::String(LuaString::from_str("a1_eq_1"))) {
                        println!("  a1 == 1: {:?}", v);
                    }
                    if let Some(v) = debug.get(&LuaValue::String(LuaString::from_str("a2_eq_2"))) {
                        println!("  a2 == 2: {:?}", v);
                    }
                    if let Some(v) = debug.get(&LuaValue::String(LuaString::from_str("b1_eq_11"))) {
                        println!("  b1 == 11: {:?}", v);
                    }
                    if let Some(v) = debug.get(&LuaValue::String(LuaString::from_str("b2_eq_12"))) {
                        println!("  b2 == 12: {:?}", v);
                    }
                }
                
                // Check values directly in Rust using string representation
                let expected = ["Number(1.0)", "Number(2.0)", "Number(11.0)", "Number(12.0)"];
                let actual = [&a1, &a2, &b1, &b2];
                
                let mut all_match = true;
                for i in 0..4 {
                    if expected[i] != actual[i] {
                        all_match = false;
                        println!("  Value {} mismatch: expected {}, got {}", 
                               i, expected[i], actual[i]);
                    }
                }
                
                if all_match {
                    println!("PASS - All counter values match expected values!");
                    println!("  counter1: {} → {}", a1, a2);
                    println!("  counter2: {} → {}", b1, b2);
                } else {
                    println!("FAIL - Closure state was not properly preserved");
                    println!("Results didn't match expected values (1, 2, 11, 12)");
                    println!("  Got: a1={}, a2={}, b1={}, b2={}", a1, a2, b1, b2);
                }
            } else {
                println!("FAIL: Expected table result, got {:?}", result);
            }
        },
        Err(e) => println!("ERROR: {}", e),
    }
    
    println!("\n=== Tests Complete ===");
    
    Ok(())
}