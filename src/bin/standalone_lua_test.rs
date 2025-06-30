//! Standalone Lua VM Test
//!
//! This executable demonstrates the Lua VM running outside of Redis
//! using our state machine architecture.

use std::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use ferrous::lua::error::{LuaError, Result};
use ferrous::lua::value::{Value, StringHandle, TableHandle};
use ferrous::lua::vm::LuaVM;
use ferrous::lua::heap::LuaHeap;
use ferrous::lua::compiler::Compiler;

/// Run a simple Lua script
fn run_script(vm: &mut LuaVM, script: &str) -> Result<Value> {
    // Initialize standard library if not already done
    vm.init_stdlib()?;
    
    // Compile the script
    let mut compiler = Compiler::new();
    let closure = compiler.compile_and_load(script, &mut vm.heap)?;
    
    // Execute the script
    vm.execute_function(closure, &[])
}

/// Print a value
fn print_value(vm: &LuaVM, value: &Value) -> String {
    match value {
        Value::Nil => "nil".to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(h) => {
            if let Ok(s) = vm.heap.get_string_value(*h) {
                format!("\"{}\"", s)
            } else {
                "\"<invalid string>\"".to_string()
            }
        },
        Value::Table(h) => format!("table: {:?}", h),
        Value::Closure(h) => format!("function: {:?}", h),
        Value::Thread(h) => format!("thread: {:?}", h),
        Value::CFunction(_) => "function: C".to_string(),
        Value::UserData(h) => format!("userdata: {:?}", h),
    }
}

/// Run a test script
fn test_script(name: &str, script: &str, expected: &str) {
    println!("\nTesting script: {}", name);
    println!("Script:\n{}", script);
    
    // Create VM
    let mut vm = LuaVM::new().expect("Failed to create VM");
    
    // Set up a kill flag with timeout (5 seconds)
    let kill_flag = Arc::new(AtomicBool::new(false));
    let kill_flag_clone = kill_flag.clone();
    
    let timeout_handle = thread::spawn(move || {
        thread::sleep(Duration::from_secs(5));
        kill_flag_clone.store(true, Ordering::SeqCst);
        println!("Script timeout after 5 seconds!");
    });
    
    vm.set_kill_flag(kill_flag);
    
    // Run the script
    match run_script(&mut vm, script) {
        Ok(result) => {
            let result_str = print_value(&vm, &result);
            println!("Result: {}", result_str);
            
            if result_str == expected {
                println!("✅ PASS - Got expected result: {}", expected);
            } else {
                println!("❌ FAIL - Expected: {}, Got: {}", expected, result_str);
            }
        },
        Err(e) => {
            if expected.starts_with("error:") && expected[7..].trim() == format!("{}", e) {
                println!("✅ PASS - Got expected error: {}", e);
            } else {
                println!("❌ FAIL - Script failed: {}", e);
            }
        },
    }
    
    // Cancel the timeout thread
    timeout_handle.join().unwrap();
}

fn main() {
    println!("=== Standalone Lua VM Test ===");
    
    // Test 1: Basic arithmetic
    test_script(
        "Basic arithmetic",
        "return 2 + 3 * 4",
        "14",
    );
    
    // Test 2: Variable assignments
    test_script(
        "Variable assignments",
        "local x = 10; local y = 20; return x + y",
        "30",
    );
    
    // Test 3: If statement
    test_script(
        "If statement",
        "local x = 10; if x > 5 then return 'greater' else return 'less' end",
        "\"greater\"",
    );
    
    // Test 4: While loop
    test_script(
        "While loop",
        "local sum = 0; local i = 1; while i <= 5 do sum = sum + i; i = i + 1 end; return sum",
        "15",
    );
    
    // Test 5: Table operations
    test_script(
        "Table operations",
        "local t = {a=1, b=2}; t.c = 3; return t.a + t.b + t.c",
        "6",
    );
    
    // Test 6: Function definition
    test_script(
        "Function definition",
        "local function add(a, b) return a + b end; return add(5, 7)",
        "12",
    );
    
    // Test 7: Closure
    test_script(
        "Closure",
        "local function counter() local i = 0; return function() i = i + 1; return i end end; local c = counter(); c(); c(); return c()",
        "3",
    );
    
    // Test 8: String concatenation
    test_script(
        "String concatenation",
        "return 'Hello, ' .. 'world!'",
        "\"Hello, world!\"",
    );
    
    // Test 9: Numeric for loop
    test_script(
        "Numeric for loop",
        "local sum = 0; for i = 1, 10, 2 do sum = sum + i end; return sum",
        "25", // 1+3+5+7+9=25
    );
    
    // Test 10: Generic for loop with pairs
    test_script(
        "Generic for loop with pairs",
        "local t = {a=1, b=2, c=3}; local sum = 0; for k, v in pairs(t) do sum = sum + v end; return sum",
        "6",
    );
    
    // Test 11: Math library functions
    test_script(
        "Math library",
        "return math.sqrt(16) + math.floor(3.7) + math.ceil(2.1)",
        "10",
    );
    
    // Test 12: String library functions
    test_script(
        "String library",
        "return string.upper('test') .. string.sub('hello', 2, 4)",
        "\"TESTell\"",
    );
    
    // Test 13: Table library functions
    test_script(
        "Table library",
        "local t = {10, 20, 30}; table.insert(t, 40); return table.concat(t, '-')",
        "\"10-20-30-40\"",
    );
    
    // Test 14: Nested for loops
    test_script(
        "Nested for loops",
        "local sum = 0; for i = 1, 3 do for j = 1, 2 do sum = sum + (i * j) end end; return sum",
        "12",
    );
    
    // Test 15: Error handling with pcall
    test_script(
        "Error handling with pcall",
        "local status, result = pcall(function() error('test error') end); return status, type(result)",
        "false \"string\"",
    );
    
    println!("\n=== Tests Complete ===");
}