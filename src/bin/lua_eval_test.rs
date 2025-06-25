//! EVAL Command Simulation Test
//!
//! This test program simulates the EVAL command flow to reproduce and diagnose
//! the "invalid handle" error.

use ferrous::lua_new::vm::LuaVM;
use ferrous::lua_new::value::Value;
use ferrous::lua_new::VMConfig;
use ferrous::lua_new::redis_api::RedisApiContext;
use ferrous::lua_new::error::Result;
use ferrous::lua_new::sandbox::LuaSandbox;
use ferrous::lua_new::parser::Parser;
use ferrous::lua_new::compiler::Compiler;
use ferrous::lua_new::cjson;
use ferrous::protocol::resp::RespFrame;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("=== EVAL Command Flow Simulation ===\n");

    // Test various scripts in order of complexity
    let test_scripts = [
        ("Empty", ""),
        ("Return nil", "return nil"),
        ("Return number", "return 42"),
        ("Return string", "return 'hello'"),
        ("Return table", "return {1, 2, 3}"),
        ("Set/get local", "local x = 42; return x"),
        ("Function definition", "local function test() return 'test' end; return test()"),
        ("Table dot access", "local t = {a='test'}; return t.a"),
        ("CJson usage", "local t = {name='test', value=123}; return cjson.encode(t)"),
    ];

    // Run each script through the exact flow that EVAL uses
    for (name, script) in &test_scripts {
        println!("\n--- Testing: {} ---", name);
        println!("Script: {}", script);
        
        match simulate_eval_command(script) {
            Ok(resp) => {
                println!("✅ Success: {:?}", resp);
            },
            Err(e) => {
                println!("❌ Error: {}", e);
                
                // If invalid handle, provide more details
                if e.to_string().contains("invalid handle") {
                    println!("\nDETAILED ERROR ANALYSIS:");
                    diagnose_invalid_handle(script)?;
                }
            }
        }
        println!("----------------------------");
    }
    
    Ok(())
}

/// Simulates the EVAL command flow from the Ferrous server
fn simulate_eval_command(script: &str) -> std::result::Result<RespFrame, Box<dyn std::error::Error>> {
    println!("Step 1: Creating VM...");
    let config = VMConfig::default();
    let mut vm = LuaVM::new(config);
    
    println!("Step 2: Setting up environment...");
    // Create dummy keys and args for the test
    let keys: Vec<Vec<u8>> = vec![b"key1".to_vec(), b"key2".to_vec()];
    let args: Vec<Vec<u8>> = vec![b"arg1".to_vec(), b"arg2".to_vec()];
    
    // Setup libraries without the real StorageEngine
    println!("  - Registering Redis API...");
    RedisApiContext::register(&mut vm)?;
    
    // Register cjson library
    println!("  - Registering cjson library...");
    cjson::register(&mut vm)?;
    
    // Apply sandbox
    println!("  - Applying sandbox...");
    let sandbox = LuaSandbox::redis_compatible();
    sandbox.apply(&mut vm)?;
    
    // Create KEYS and ARGV tables
    println!("  - Setting up KEYS and ARGV tables...");
    setup_keys_argv(&mut vm, &keys, &args)?;
    
    println!("Step 3: Compiling script...");
    // Parse the script
    let mut parser = Parser::new(script, &mut vm.heap)?;
    let ast = parser.parse()?;
    
    // Compile to bytecode
    let mut compiler = Compiler::new();
    compiler.set_heap(&mut vm.heap as *mut _);
    let proto = compiler.compile_chunk(&ast)?;
    println!("  Bytecode instructions: {}", proto.code.len());
    println!("  Constants: {}", proto.constants.len());
    
    // Create closure - clone the proto since we need it for debugging
    let closure = vm.heap.alloc_closure(proto.clone(), Vec::new());
    println!("  Closure created with handle: {:?}", closure);
    
    println!("Step 4: Executing script...");
    // Setup kill flag
    let kill_flag = Arc::new(AtomicBool::new(false));
    
    // Execute with no arguments (empty Vec)
    let result = vm.execute_with_limits(closure, &[], kill_flag)?;
    println!("  Execution result: {:?}", result);
    
    println!("Step 5: Converting result to RESP...");
    // Convert to RESP
    let resp = RedisApiContext::lua_to_resp(&mut vm, result)?;
    println!("  RESP result: {:?}", resp);
    
    Ok(resp)
}

/// Setup KEYS and ARGV tables in the Lua environment
fn setup_keys_argv(vm: &mut LuaVM, keys: &[Vec<u8>], args: &[Vec<u8>]) -> Result<()> {
    // Create KEYS table
    let keys_table = vm.heap.alloc_table();
    for (i, key) in keys.iter().enumerate() {
        let idx = Value::Number((i + 1) as f64);
        let val = Value::String(vm.heap.alloc_string(key));
        vm.heap.get_table_mut(keys_table)?.set(idx, val);
    }
    
    // Create ARGV table
    let argv_table = vm.heap.alloc_table();
    for (i, arg) in args.iter().enumerate() {
        let idx = Value::Number((i + 1) as f64);
        let val = Value::String(vm.heap.alloc_string(arg));
        vm.heap.get_table_mut(argv_table)?.set(idx, val);
    }
    
    // Set in globals
    let globals = vm.globals();
    let keys_name = vm.heap.create_string("KEYS");
    let argv_name = vm.heap.create_string("ARGV");
    
    vm.heap.get_table_mut(globals)?.set(Value::String(keys_name), Value::Table(keys_table));
    vm.heap.get_table_mut(globals)?.set(Value::String(argv_name), Value::Table(argv_table));
    
    Ok(())
}

/// Provide detailed diagnostics for the invalid handle error
fn diagnose_invalid_handle(script: &str) -> Result<()> {
    println!("Creating new VM for debugging...");
    let config = VMConfig::default();
    let mut vm = LuaVM::new(config);
    
    // Register libraries
    RedisApiContext::register(&mut vm)?;
    cjson::register(&mut vm)?;
    
    // Parse script
    println!("Parsing script...");
    let mut parser = Parser::new(script, &mut vm.heap)?;
    let ast = parser.parse()?;
    
    // Compile to bytecode
    println!("Compiling script...");
    let mut compiler = Compiler::new();
    compiler.set_heap(&mut vm.heap as *mut _);
    let proto = compiler.compile_chunk(&ast)?;
    
    // Check bytecode
    println!("Bytecode analysis:");
    for (i, instr) in proto.code.iter().enumerate() {
        // Print instruction details
        let op = instr.opcode();
        let a = instr.a();
        let b = instr.b();
        let c = instr.c();
        
        println!("  [{:03}] Op: {}, A: {}, B: {}, C: {}", i, op, a, b, c);
        
        // Check if instruction uses any constants
        if op == 1 {  // LOADK
            let bx = instr.bx() as usize;
            if bx < proto.constants.len() {
                println!("      Loading constant {}: {:?}", bx, proto.constants[bx]);
                
                // Check if the constant is a valid handle
                if let Value::String(handle) = proto.constants[bx] {
                    match vm.heap.get_string(handle) {
                        Ok(s) => println!("      String value: {:?}", std::str::from_utf8(s)),
                        Err(e) => println!("      ❌ INVALID STRING HANDLE: {}", e),
                    }
                }
            } else {
                println!("      ❌ INVALID CONSTANT INDEX: {}", bx);
            }
        }
    }
    
    // Check string constants specifically
    println!("\nString constant validation:");
    for (i, constant) in proto.constants.iter().enumerate() {
        if let Value::String(handle) = constant {
            print!("  Constant[{}] String handle: {:?}", i, handle);
            match vm.heap.get_string(*handle) {
                Ok(bytes) => {
                    println!(" - Valid, content: {:?}", std::str::from_utf8(bytes));
                }
                Err(e) => {
                    println!(" - ❌ INVALID: {}", e);
                }
            }
        }
    }
    
    // Analyze execution until failure
    println!("\nStep-by-step execution:");
    // Create closure - clone proto before it's moved
    let proto_clone = proto.clone();
    let closure = vm.heap.alloc_closure(proto, Vec::new());
    
    // Try executing step by step
    for idx in 0..proto_clone.code.len() {
        println!("  Executing instruction {}...", idx);
        
        // Get current instruction
        let instruction = &proto_clone.code[idx];
        let op = instruction.opcode();
        println!("  Opcode: {}", op);
        
        // Execute manually one step at a time would require access to VM internals
        // This is just a placeholder to show the approach
        println!("  (Execution details require VM internal access)");
        
        // For diagnostic purposes, we'll try to see if the VM can execute at least the first instruction
        if idx == 0 {
            match vm.execute_function(closure, &[]) {
                Ok(result) => {
                    println!("  Full execution succeeded with result: {:?}", result);
                    // If execution succeeded, the issue might be in a different part
                    println!("  ⚠️ No error triggered during execution, suggesting the issue may be elsewhere");
                },
                Err(e) => {
                    println!("  ❌ Error: {}", e);
                    
                    // Check for specific error types
                    if e.to_string().contains("invalid handle") {
                        println!("  ✅ Successfully reproduced the invalid handle error");
                    }
                }
            }
            
            // No need to continue after attempting a full execution
            break;
        }
    }
    
    Ok(())
}