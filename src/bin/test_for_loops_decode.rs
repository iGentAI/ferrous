//! Test for Generic For Loops and cjson.decode functionality
//!
//! This program tests our implementation of generic for loops and cjson.decode

use ferrous::lua_new::vm::LuaVM;
use ferrous::lua_new::value::Value;
use ferrous::lua_new::compiler::Compiler;
use ferrous::lua_new::VMConfig;
use ferrous::lua_new::redis_api::RedisApiContext;
use ferrous::lua_new::sandbox::LuaSandbox;
use ferrous::lua_new::cjson;
use std::collections::HashMap;

// Set up the VM with proper memory and resource limits
fn setup_vm() -> Result<LuaVM, Box<dyn std::error::Error>> {
    // Create VM with increased memory limits for testing
    let mut config = VMConfig::default();
    
    // Increase memory limit to 32MB (default is likely much lower)
    config.limits.memory_limit = 32 * 1024 * 1024;
    
    // Increase instruction limit
    config.limits.instruction_limit = 5_000_000;
    
    // Increase stack limits
    config.limits.call_stack_limit = 1000;
    config.limits.value_stack_limit = 10000;
    
    // Enable debug mode for verbose output
    config.debug = true;
    
    let mut vm = LuaVM::new(config);
    
    // Register Redis API and cjson
    println!("Setting up environment...");
    RedisApiContext::register(&mut vm)?;
    cjson::register(&mut vm)?;
    
    // Apply sandbox
    let sandbox = LuaSandbox::redis_compatible();
    sandbox.apply(&mut vm)?;
    
    Ok(vm)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Testing Generic For Loops and cjson.decode ===\n");
    
    // Test scripts
    let test_scripts = [
        ("Generic For Loop", r#"
            local t = {a=1, b=2, c=3}
            local result = {}
            for k, v in pairs(t) do
                result[k] = v
            end
            return result
        "#),
        
        ("Numeric For Loop", r#"
            local result = {}
            for i=1,5 do
                result[i] = i * 10
            end
            return result
        "#),
        
        ("cjson.decode Basic", r#"
            local json_str = '{"name":"test","age":42}'
            return cjson.decode(json_str)
        "#),
        
        ("cjson.decode Array", r#"
            local json_str = '[1,2,3,4,5]'
            return cjson.decode(json_str)
        "#),
        
        ("cjson.decode Complex", r#"
            local json_str = '{"name":"test","values":[1,2,3],"nested":{"key":"value"}}'
            return cjson.decode(json_str)
        "#),
        
        ("cjson Round Trip", r#"
            local original = {
                name = "test",
                values = {1, 2, 3},
                nested = {key = "value"}
            }
            local json_str = cjson.encode(original)
            local decoded = cjson.decode(json_str)
            return {original = original, json = json_str, decoded = decoded}
        "#),
    ];
    
    // Run each test script
    for (name, script) in test_scripts.iter() {
        println!("\n--- Testing: {} ---", name);
        println!("Script:\n{}", script);
        
        // Create a fresh VM for each test to avoid state interference
        let mut test_vm = setup_vm()?;
        
        match execute_script(&mut test_vm, script) {
            Ok(result) => {
                println!("✅ Success!");
                print_result(&mut test_vm, result)?;
            },
            Err(e) => {
                println!("❌ Error: {}", e);
            }
        }
    }
    
    println!("\nAll tests completed.");
    
    Ok(())
}

// Execute a script and return the result
fn execute_script(vm: &mut LuaVM, script: &str) -> Result<Value, Box<dyn std::error::Error>> {
    // Compile the script
    let mut compiler = Compiler::new();
    compiler.set_heap(&mut vm.heap as *mut _);
    let compile_result = compiler.compile(script)?;
    
    // Load the script into the VM
    let closure = vm.load_compilation_script(&compile_result)?;
    
    // Execute the script
    let result = vm.execute_function(closure, &[])?;
    
    Ok(result)
}

// Collector to extract table info from values before printing
struct TableInfo {
    array_values: Vec<(usize, Value)>,
    map_entries: Vec<(Value, Value)>,
}

// Helper function to extract table information first to avoid borrow checker issues
fn collect_table_info(vm: &mut LuaVM, table: ferrous::lua_new::value::TableHandle) -> Result<TableInfo, Box<dyn std::error::Error>> {
    let table_obj = vm.heap.get_table(table)?;
    
    // Collect array part
    let mut array_values = Vec::new();
    for (i, &value) in table_obj.array.iter().enumerate() {
        if !matches!(value, Value::Nil) {
            array_values.push((i + 1, value));
        }
    }
    
    // Collect hash part
    let mut map_entries = Vec::new();
    for (k, &v) in &table_obj.map {
        map_entries.push((*k, v));
    }
    
    Ok(TableInfo { array_values, map_entries })
}

// Helper function to print result values in a readable format
fn print_result(vm: &mut LuaVM, value: Value) -> Result<(), Box<dyn std::error::Error>> {
    match value {
        Value::Nil => println!("nil"),
        Value::Boolean(b) => println!("{}", b),
        Value::Number(n) => println!("{}", n),
        Value::String(s) => {
            let bytes = vm.heap.get_string(s)?;
            let s_str = std::str::from_utf8(bytes)?;
            println!("\"{}\"", s_str);
        },
        Value::Table(t) => print_table(vm, t, 0)?,
        _ => println!("<{}>", value.type_name()),
    }
    
    Ok(())
}

// Print a table with indentation
fn print_table(vm: &mut LuaVM, table: ferrous::lua_new::value::TableHandle, indent: usize) -> Result<(), Box<dyn std::error::Error>> {
    // First collect all table information to avoid borrow checker issues
    let table_info = collect_table_info(vm, table)?;
    
    println!("{{");
    
    let indent_str = "  ".repeat(indent + 1);
    
    // Print array part
    for (i, value) in table_info.array_values {
        print!("{}[{}] = ", indent_str, i);
        
        match value {
            Value::Nil => println!("nil,"),
            Value::Boolean(b) => println!("{},", b),
            Value::Number(n) => println!("{},", n),
            Value::String(s) => {
                let bytes = vm.heap.get_string(s)?;
                let s_str = std::str::from_utf8(bytes)?;
                println!("\"{}\",", s_str);
            },
            Value::Table(t) => {
                print_table(vm, t, indent + 1)?;
                println!(",");
            },
            _ => println!("<{}>,", value.type_name()),
        }
    }
    
    // Print hash part
    for (k, v) in table_info.map_entries {
        print!("{}", indent_str);
        
        // Print key
        match k {
            Value::String(s) => {
                let bytes = vm.heap.get_string(s)?;
                let s_str = std::str::from_utf8(bytes)?;
                print!("[\"{}\"", s_str);
            },
            Value::Number(n) => print!("[{}", n),
            _ => print!("[<{}>", k.type_name()),
        }
        
        print!("] = ");
        
        // Print value
        match v {
            Value::Nil => println!("nil,"),
            Value::Boolean(b) => println!("{},", b),
            Value::Number(n) => println!("{},", n),
            Value::String(s) => {
                let bytes = vm.heap.get_string(s)?;
                let s_str = std::str::from_utf8(bytes)?;
                println!("\"{}\",", s_str);
            },
            Value::Table(t) => {
                print_table(vm, t, indent + 1)?;
                println!(",");
            },
            _ => println!("<{}>,", v.type_name()),
        }
    }
    
    print!("{}}}", "  ".repeat(indent));
    
    Ok(())
}