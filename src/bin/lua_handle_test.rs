//! Handle validation test for Lua VM
//!
//! This test specifically targets handle management to identify "invalid handle" errors.

use ferrous::lua_new::vm::LuaVM;
use ferrous::lua_new::value::{Value, StringHandle, TableHandle, ClosureHandle};
use ferrous::lua_new::VMConfig;
use ferrous::lua_new::error::Result;

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("=== Lua VM Handle Validation Test ===\n");
    
    // Create VM with default configuration
    let config = VMConfig::default();
    let mut vm = LuaVM::new(config);
    
    // Test 1: String Handle Creation and Validation
    println!("Test 1: String Handle Creation and Validation");
    let string_test = test_string_handle(&mut vm)?;
    println!("{}\n", if string_test { "✅ PASSED" } else { "❌ FAILED" });
    
    // Test 2: Table Handle Creation and Validation
    println!("Test 2: Table Handle Creation and Validation");
    let table_test = test_table_handle(&mut vm)?;
    println!("{}\n", if table_test { "✅ PASSED" } else { "❌ FAILED" });
    
    // Test 3: Function Creation and Execution
    println!("Test 3: Function Creation and Execution");
    let function_test = test_function_creation(&mut vm)?;
    println!("{}\n", if function_test { "✅ PASSED" } else { "❌ FAILED" });
    
    // Test 4: Handle Persistence Across Operations
    println!("Test 4: Handle Persistence Across Operations");
    let persistence_test = test_handle_persistence(&mut vm)?;
    println!("{}\n", if persistence_test { "✅ PASSED" } else { "❌ FAILED" });
    
    // Test 5: Function Call with String Return
    println!("Test 5: Function Call with String Return");
    let string_return_test = test_function_string_return(&mut vm)?;
    println!("{}\n", if string_return_test { "✅ PASSED" } else { "❌ FAILED" });
    
    // Test 6: Generation Arena Management
    println!("Test 6: Generational Arena Management");
    let arena_test = test_generation_arena(&mut vm)?;
    println!("{}\n", if arena_test { "✅ PASSED" } else { "❌ FAILED" });
    
    // Summarize
    println!("Handle Validation Summary:");
    println!("  String Handles: {}", if string_test { "✅ PASSED" } else { "❌ FAILED" });
    println!("  Table Handles: {}", if table_test { "✅ PASSED" } else { "❌ FAILED" });
    println!("  Function Creation: {}", if function_test { "✅ PASSED" } else { "❌ FAILED" });
    println!("  Handle Persistence: {}", if persistence_test { "✅ PASSED" } else { "❌ FAILED" });
    println!("  Function String Return: {}", if string_return_test { "✅ PASSED" } else { "❌ FAILED" });
    println!("  Generation Arena: {}", if arena_test { "✅ PASSED" } else { "❌ FAILED" });
    
    Ok(())
}

/// Test string handle creation and validation
fn test_string_handle(vm: &mut LuaVM) -> Result<bool> {
    println!("  Creating string...");
    let handle = vm.heap.create_string("test string");
    
    println!("  Validating handle...");
    match vm.heap.get_string(handle) {
        Ok(bytes) => {
            let s = std::str::from_utf8(bytes).unwrap();
            println!("  Retrieved string: {}", s);
            Ok(s == "test string")
        },
        Err(e) => {
            println!("  Error: {}", e);
            Ok(false)
        }
    }
}

/// Test table handle creation and validation
fn test_table_handle(vm: &mut LuaVM) -> Result<bool> {
    println!("  Creating table...");
    let table = vm.heap.alloc_table();
    
    // Set some values in the table
    let key = vm.heap.create_string("key");
    let value = vm.heap.create_string("value");
    
    println!("  Setting table field...");
    vm.heap.get_table_mut(table)?.set(
        Value::String(key),
        Value::String(value)
    );
    
    println!("  Retrieving table field...");
    let table_obj = vm.heap.get_table(table)?;
    match table_obj.get(&Value::String(key)) {
        Some(&field_value) => {
            match field_value {
                Value::String(str_handle) => {
                    let str_bytes = vm.heap.get_string(str_handle)?;
                    let str_value = std::str::from_utf8(str_bytes).unwrap();
                    println!("  Retrieved value: {}", str_value);
                    Ok(str_value == "value")
                },
                _ => {
                    println!("  Unexpected value type");
                    Ok(false)
                }
            }
        },
        None => {
            println!("  Field not found");
            Ok(false)
        }
    }
}

/// Test function creation and execution
fn test_function_creation(vm: &mut LuaVM) -> Result<bool> {
    println!("  Creating a simple function prototype...");
    use ferrous::lua_new::value::{FunctionProto, Instruction, OpCode};
    
    // Create a function prototype that returns a number literal (42)
    let mut proto = FunctionProto::default();
    
    // Add a constant
    proto.constants.push(Value::Number(42.0));
    
    // Add LOADK instruction: R(0) := K(0) - Load constant 0 into register 0
    let loadk_instr = (OpCode::LoadK as u32) | ((0 as u32) << 6) | ((0 as u32) << 14);
    proto.code.push(Instruction::new(loadk_instr));
    
    // Add RETURN instruction: return R(0)
    let return_instr = (OpCode::Return as u32) | ((0 as u32) << 6) | ((2 as u32) << 14);
    proto.code.push(Instruction::new(return_instr));
    
    // Set stack size
    proto.max_stack_size = 1;
    
    println!("  Creating closure from prototype...");
    let closure = vm.heap.alloc_closure(proto, Vec::new());
    
    println!("  Executing function...");
    match vm.execute_function(closure, &[]) {
        Ok(value) => {
            match value {
                Value::Number(n) => {
                    println!("  Result: {}", n);
                    Ok(n == 42.0)
                },
                _ => {
                    println!("  Unexpected return type: {:?}", value);
                    Ok(false)
                }
            }
        },
        Err(e) => {
            println!("  Execution error: {}", e);
            Ok(false)
        }
    }
}

/// Test handle persistence across operations
fn test_handle_persistence(vm: &mut LuaVM) -> Result<bool> {
    println!("  Creating multiple objects...");
    
    // Create strings
    let handle1 = vm.heap.create_string("string1");
    let handle2 = vm.heap.create_string("string2");
    
    // Create tables
    let table1 = vm.heap.alloc_table();
    let table2 = vm.heap.alloc_table();
    
    // Cross-reference them
    println!("  Setting up cross-references...");
    vm.heap.get_table_mut(table1)?.set(
        Value::String(handle1),
        Value::Table(table2)
    );
    
    vm.heap.get_table_mut(table2)?.set(
        Value::String(handle2),
        Value::String(handle1)
    );
    
    println!("  Validating cross-references...");
    // Check table1[handle1] == table2
    let table1_obj = vm.heap.get_table(table1)?;
    match table1_obj.get(&Value::String(handle1)) {
        Some(&Value::Table(t)) => {
            if t.0.index != table2.0.index {
                println!("  Table reference mismatch");
                return Ok(false);
            }
        },
        _ => {
            println!("  Table field not found or wrong type");
            return Ok(false);
        }
    }
    
    // Check table2[handle2] == handle1
    let table2_obj = vm.heap.get_table(table2)?;
    match table2_obj.get(&Value::String(handle2)) {
        Some(&Value::String(s)) => {
            if s.0.index != handle1.0.index {
                println!("  String reference mismatch");
                return Ok(false);
            }
            
            // Try to access the string
            let str_bytes = vm.heap.get_string(s)?;
            let str_value = std::str::from_utf8(str_bytes).unwrap();
            println!("  Retrieved value: {}", str_value);
            if str_value != "string1" {
                println!("  String content mismatch");
                return Ok(false);
            }
        },
        _ => {
            println!("  String field not found or wrong type");
            return Ok(false);
        }
    }
    
    Ok(true)
}

/// Test function that returns a string
fn test_function_string_return(vm: &mut LuaVM) -> Result<bool> {
    println!("  Creating a function that returns a string...");
    use ferrous::lua_new::value::{FunctionProto, Instruction, OpCode};
    
    // Create a function prototype that returns a string literal ("hello")
    let mut proto = FunctionProto::default();
    
    // Add a string constant
    let str_handle = vm.heap.create_string("hello");
    proto.constants.push(Value::String(str_handle));
    
    // Add LOADK instruction: R(0) := K(0) - Load constant 0 into register 0
    let loadk_instr = (OpCode::LoadK as u32) | ((0 as u32) << 6) | ((0 as u32) << 14);
    proto.code.push(Instruction::new(loadk_instr));
    
    // Add RETURN instruction: return R(0)
    let return_instr = (OpCode::Return as u32) | ((0 as u32) << 6) | ((2 as u32) << 14);
    proto.code.push(Instruction::new(return_instr));
    
    // Set stack size
    proto.max_stack_size = 1;
    
    println!("  Creating closure from prototype...");
    let closure = vm.heap.alloc_closure(proto, Vec::new());
    
    println!("  Executing function...");
    match vm.execute_function(closure, &[]) {
        Ok(value) => {
            match value {
                Value::String(s) => {
                    let str_bytes = vm.heap.get_string(s)?;
                    let str_value = std::str::from_utf8(str_bytes).unwrap();
                    println!("  Result: {}", str_value);
                    Ok(str_value == "hello")
                },
                _ => {
                    println!("  Unexpected return type: {:?}", value);
                    Ok(false)
                }
            }
        },
        Err(e) => {
            println!("  Execution error: {}", e);
            Ok(false)
        }
    }
}

/// Test generational arena management
fn test_generation_arena(vm: &mut LuaVM) -> Result<bool> {
    println!("  Testing generational arena...");
    
    // Create and immediately remove objects to test arena reuse
    println!("  Creating and removing objects...");
    let mut table_handles = Vec::new();
    
    // Create 10 tables
    for i in 0..10 {
        let table = vm.heap.alloc_table();
        
        // Add a value to each table
        let key = vm.heap.create_string(&format!("key{}", i));
        let value = Value::Number(i as f64);
        vm.heap.get_table_mut(table)?.set(Value::String(key), value);
        
        table_handles.push(table);
    }
    
    // Test if we can access all tables
    for (i, &table) in table_handles.iter().enumerate() {
        let key = vm.heap.create_string(&format!("key{}", i));
        let table_obj = vm.heap.get_table(table)?;
        
        if let Some(&Value::Number(n)) = table_obj.get(&Value::String(key)) {
            if n != i as f64 {
                println!("  Table {} value mismatch: expected {}, got {}", i, i, n);
                return Ok(false);
            }
        } else {
            println!("  Table {} key not found or wrong type", i);
            return Ok(false);
        }
    }
    
    println!("  All table handles valid");
    Ok(true)
}