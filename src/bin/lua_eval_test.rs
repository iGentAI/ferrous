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
use ferrous::lua_new::compiler::Compiler;
use ferrous::lua_new::compilation::CompilationValue;
use ferrous::lua_new::cjson;
use ferrous::protocol::resp::RespFrame;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

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
    // Use the public compile method
    let mut compiler = Compiler::new();
    compiler.set_heap(&mut vm.heap as *mut _);
    let compile_result = compiler.compile(script)?;
    
    println!("  Bytecode instructions: {}", compile_result.main_proto().code.len());
    println!("  Constants: {}", compile_result.main_proto().constants.len());
    
    // Load the compiled script into the VM
    let closure = vm.load_compilation_script(&compile_result)?;
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
    
    // Compile script
    println!("Compiling script...");
    let mut compiler = Compiler::new();
    compiler.set_heap(&mut vm.heap as *mut _);
    let compile_result = compiler.compile(script)?;
    let proto = compile_result.main_proto().clone();
    
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
                
                // Constants in CompilationProto are CompilationValue, not Value
                // We can't directly check string handles here anymore
            } else {
                println!("      ❌ INVALID CONSTANT INDEX: {}", bx);
            }
        }
    }
    
    // Instead of checking string constants directly, load the script and then analyze
    println!("\nLoading compiled script into VM...");
    let closure = vm.load_compilation_script(&compile_result)?;
    
    // Check string constants specifically
    println!("\nString constant validation:");
    for (i, constant) in proto.constants.iter().enumerate() {
        match constant {
            ferrous::lua_new::compilation::CompilationValue::String(idx) => {
                println!("  Constant[{}] String pool index: {}", i, idx);
                // We can check if the index is valid in the string pool
                if *idx < compile_result.string_pool.len() {
                    println!("    String value: {:?}", compile_result.string_pool[*idx]);
                } else {
                    println!("    ❌ INVALID STRING POOL INDEX");
                }
            }
            _ => {}
        }
    }
    
    // Analyze execution until failure
    println!("\nStep-by-step execution:");
    
    // Try executing the closure we loaded
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
    
    Ok(())
}