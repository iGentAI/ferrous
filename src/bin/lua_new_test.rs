//! Test for the new Lua implementation with generational arena architecture

use ferrous::lua_new::{LuaVM, VMConfig, Value, LuaHeap};
use ferrous::lua_new::value::{FunctionProto, Instruction, OpCode};
use ferrous::lua_new::error::Result;

fn main() -> Result<()> {
    println!("Testing new Lua VM implementation");
    
    // Create VM
    let mut vm = create_vm()?;
    
    // Test simple expression
    test_simple_expression(&mut vm)?;
    
    // Test table operations
    test_table_operations(&mut vm)?;
    
    println!("All tests passed!");
    
    Ok(())
}

fn create_vm() -> Result<LuaVM> {
    let config = VMConfig::default();
    let mut vm = LuaVM::new(config);
    
    // Set up globals
    let globals = vm.globals();
    let print_func = vm.heap.create_string("print");
    vm.heap.get_table_mut(globals)?.set(
        Value::String(print_func),
        Value::CFunction(print_function)
    );
    
    Ok(vm)
}

fn test_simple_expression(vm: &mut LuaVM) -> Result<()> {
    println!("\nTesting simple expression: 1 + 2 * 3 = 7");
    
    // Create a function that computes 1 + 2 * 3
    let mut proto = FunctionProto::default();
    
    // Add constants: 1, 2, 3
    proto.constants.push(Value::Number(1.0));
    proto.constants.push(Value::Number(2.0));
    proto.constants.push(Value::Number(3.0));
    
    // Instructions:
    // R0 = 2 (constant 1)
    // R1 = 3 (constant 2)
    // R0 = R0 * R1 (R0 = 6)
    // R1 = 1 (constant 0)
    // R0 = R1 + R0 (R0 = 7)
    // return R0
    
    // First, load the constants
    proto.code.push(Instruction::new(
        (OpCode::LoadK as u32) | (0 << 6) | (1 << 14)
    )); // R0 = K1 (2)
    
    proto.code.push(Instruction::new(
        (OpCode::LoadK as u32) | (1 << 6) | (2 << 14)
    )); // R1 = K2 (3)
    
    proto.code.push(Instruction::new(
        (OpCode::Mul as u32) | (0 << 6) | (0 << 14) | (1 << 23)
    )); // R0 = R0 * R1 (6)
    
    proto.code.push(Instruction::new(
        (OpCode::LoadK as u32) | (1 << 6) | (0 << 14)
    )); // R1 = K0 (1)
    
    proto.code.push(Instruction::new(
        (OpCode::Add as u32) | (0 << 6) | (1 << 14) | (0 << 23)
    )); // R0 = R1 + R0 (7)
    
    proto.code.push(Instruction::new(
        (OpCode::Return as u32) | (0 << 6) | (2 << 14)
    )); // return R0
    
    proto.max_stack_size = 2;
    
    // Create closure
    let closure = vm.heap.alloc_closure(proto, vec![]);
    
    // Execute
    let result = vm.execute_function(closure, &[])?;
    
    // Check result
    match result {
        Value::Number(n) => {
            println!("Result: {}", n);
            assert_eq!(n, 7.0);
        }
        _ => panic!("Expected number, got {:?}", result),
    }
    
    Ok(())
}

fn test_table_operations(vm: &mut LuaVM) -> Result<()> {
    println!("\nTesting table operations");
    
    // Create a table
    let table = vm.heap.alloc_table();
    
    // Set some values
    let key1 = vm.heap.create_string("foo");
    let key2 = vm.heap.create_string("bar");
    let value1 = vm.heap.create_string("hello");
    
    vm.heap.get_table_mut(table)?.set(Value::String(key1), Value::String(value1));
    vm.heap.get_table_mut(table)?.set(Value::Number(1.0), Value::Number(42.0));
    vm.heap.get_table_mut(table)?.set(Value::Number(2.0), Value::String(key2));
    
    // Get values
    let result1 = vm.table_get(table, Value::String(key1))?;
    let result2 = vm.table_get(table, Value::Number(1.0))?;
    let result3 = vm.table_get(table, Value::Number(2.0))?;
    
    // Check results
    match result1 {
        Value::String(s) => {
            let s_str = vm.heap.get_string_utf8(s)?;
            println!("table.foo = {}", s_str);
            assert_eq!(s_str, "hello");
        }
        _ => panic!("Expected string, got {:?}", result1),
    }
    
    match result2 {
        Value::Number(n) => {
            println!("table[1] = {}", n);
            assert_eq!(n, 42.0);
        }
        _ => panic!("Expected number, got {:?}", result2),
    }
    
    match result3 {
        Value::String(s) => {
            let s_str = vm.heap.get_string_utf8(s)?;
            println!("table[2] = {}", s_str);
            assert_eq!(s_str, "bar");
        }
        _ => panic!("Expected string, got {:?}", result3),
    }
    
    println!("All table tests passed!");
    
    Ok(())
}

/// print function implementation
fn print_function(exec_ctx: &mut ferrous::lua_new::vm::ExecutionContext) -> Result<i32> {
    // Collect all arguments and print them
    for i in 0..exec_ctx.get_arg_count() {
        let value = exec_ctx.get_arg(i)?;
        
        match value {
            Value::Nil => print!("nil"),
            Value::Boolean(b) => print!("{}", b),
            Value::Number(n) => print!("{}", n),
            Value::String(s) => {
                let s_str = exec_ctx.vm.heap.get_string_utf8(s)?;
                print!("{}", s_str);
            }
            _ => print!("<{}>", value.type_name()),
        }
        
        if i < exec_ctx.get_arg_count() - 1 {
            print!("\t");
        }
    }
    
    println!();
    
    Ok(0) // No return values
}