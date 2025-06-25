//! Diagnostic test for checking if VM functions as expected
//!
//! This test runs a sequence of Lua scripts through the VM to verify handling of
//! different values (nil, strings, numbers, tables) and function calls.

use ferrous::lua_new::vm::LuaVM;
use ferrous::lua_new::value::{Value, StringHandle, TableHandle};
use ferrous::lua_new::heap::LuaHeap;
use ferrous::lua_new::parser::Parser;
use ferrous::lua_new::compiler::Compiler;
use ferrous::lua_new::VMConfig;
use ferrous::lua_new::redis_api::RedisApiContext;
use ferrous::lua_new::sandbox::LuaSandbox;
use ferrous::lua_new::cjson;
use std::rc::Rc;
use std::cell::RefCell;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Lua VM Diagnostic Test ===");
    println!("Testing the full Lua pipeline to identify the 'invalid handle' error\n");
    
    let config = VMConfig::default();
    let mut vm = LuaVM::new(config);
    
    println!("Step 1: Registering libraries...");
    // Register the Redis API functions
    if let Err(e) = RedisApiContext::register(&mut vm) {
        println!("❌ Failed to register Redis API: {}", e);
        return Ok(());
    }
    
    // Register cjson library
    if let Err(e) = cjson::register(&mut vm) {
        println!("❌ Failed to register cjson: {}", e);
        return Ok(());
    }
    
    // Register sandbox (for security)
    let sandbox = LuaSandbox::redis_compatible();
    if let Err(e) = sandbox.apply(&mut vm) {
        println!("❌ Failed to apply sandbox: {}", e);
        return Ok(());
    }
    
    println!("✅ Libraries registered successfully\n");
    
    // Test cases
    let test_scripts = [
        ("Empty return", "return"),
        ("Nil return", "return nil"),
        ("Number return", "return 42"),
        ("String return", "return 'hello'"),
        ("Table return", "return {1, 2, 3}"),
        ("Nested table", "return {a={b={c=1}}}"),
        ("Local variable", "local x = 10; return x"),
        ("Function call", "local function f() return 'test' end; return f()"),
        ("Table dot access", "local t = {a='test'}; return t.a"),  // Simple table access
        ("Table concatenation 1", "local t = {foo='bar'}; return t.foo .. ' test'"),  // Simple concat
        ("Table concatenation 2", "local str='bar '; local t = {baz=42}; return str .. t.baz"),  // Concat string and number
        ("Full table concatenation", "local t = {foo='bar', baz=42}; return t.foo .. ' ' .. t.baz"),  // Full test
        ("CJson test", "return cjson.encode({test='value'})"),
    ];
    
    // Run tests
    for (name, script) in &test_scripts {
        println!("=====================================");
        println!("Testing script: {}", name);
        println!("Script: {}", script);
        
        // IMPORTANT: Reset the VM state before each test to prevent state accumulation
        vm.reset();
        
        match test_script(&mut vm, script) {
            Ok(result) => {
                println!("✅ Script executed successfully");
                println!("Result: {:?}", result);
            },
            Err(e) => {
                println!("❌ Script execution failed");
                println!("Error: {}", e);
            }
        }
        println!("=====================================\n");
    }
    
    Ok(())
}

// Test a single script
fn test_script(vm: &mut LuaVM, script: &str) -> Result<Value, Box<dyn std::error::Error>> {
    // Phase 1: Parse the script
    println!("Phase 1: Parsing...");
    let mut parser = Parser::new(script, &mut vm.heap)?;
    let ast = parser.parse()?;
    println!("✅ Parsing successful");
    
    // Phase 2: Compile to bytecode
    println!("Phase 2: Compilation...");
    let mut compiler = Compiler::new();
    compiler.set_heap(&mut vm.heap as *mut _);
    let proto = compiler.compile_chunk(&ast)?;
    println!("✅ Compilation successful");
    println!("    - Bytecode instructions: {}", proto.code.len());
    println!("    - Constants: {}", proto.constants.len());
    println!("    - Max stack: {}", proto.max_stack_size);
    
    // Phase 3: Create closure
    println!("Phase 3: Creating closure...");
    let closure = vm.heap.alloc_closure(proto, vec![]);
    println!("✅ Closure created successfully");
    
    // Phase 4: Execute the script
    println!("Phase 4: Executing script...");
    let result = vm.execute_function(closure, &[])?;
    println!("✅ Execution successful");
    
    Ok(result)
}

