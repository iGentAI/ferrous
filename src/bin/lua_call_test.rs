//! Test for Lua function calls
//!
//! This test creates a minimal environment to test function calls specifically,
//! with detailed debugging information to track the VM state during execution.

use ferrous::lua_new::vm::{LuaVM, ExecutionContext};
use ferrous::lua_new::value::{Value, StringHandle, TableHandle, ClosureHandle, FunctionProto, Instruction, OpCode};
use ferrous::lua_new::VMConfig;
use ferrous::lua_new::error::Result;
use std::fmt;

// Custom debug handler for tracking VM state
struct DebugHandler {
    call_depth: usize,
}

impl DebugHandler {
    fn new() -> Self {
        DebugHandler {
            call_depth: 0,
        }
    }
    
    fn enter_call(&mut self) {
        self.call_depth += 1;
        let indent = " ".repeat(self.call_depth * 2);
        println!("{}Call depth: {} - Entering function", indent, self.call_depth);
    }
    
    fn exit_call(&mut self, value: &Value) {
        let indent = " ".repeat(self.call_depth * 2);
        println!("{}Call depth: {} - Exiting function with value: {:?}", indent, self.call_depth, value);
        self.call_depth -= 1;
    }
    
    fn log_instruction(&self, op: OpCode, a: u8, b: u16, c: u16) {
        let indent = " ".repeat(self.call_depth * 2);
        println!("{}Executing: {:?}(A={}, B={}, C={})", indent, op, a, b, c);
    }
}

// Convert any Value into a debug-friendly string
fn value_to_string(vm: &mut LuaVM, value: Value) -> String {
    match value {
        Value::Nil => "nil".to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(handle) => {
            if let Ok(bytes) = vm.heap.get_string(handle) {
                if let Ok(s) = std::str::from_utf8(bytes) {
                    format!("\"{}\"", s)
                } else {
                    format!("\"<binary>\"")
                }
            } else {
                format!("\"<invalid string handle>\"")
            }
        }
        Value::Table(_) => "{...}".to_string(),
        Value::Closure(_) => "<function>".to_string(),
        Value::Thread(_) => "<thread>".to_string(),
        Value::CFunction(_) => "<C function>".to_string(),
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("=== Lua Function Call Test ===\n");
    
    // Create VM with debug output enabled
    let mut config = VMConfig::default();
    config.debug = true;
    let mut vm = LuaVM::new(config);
    
    // Create debug handler
    let mut debug_handler = DebugHandler::new();
    
    // Test 1: Simple function call with return value
    println!("\nTest 1: Simple function call with return value");
    
    // Create a function that returns a string
    let inner_fn_proto = create_return_string_function(&mut vm)?;
    let inner_fn_closure = vm.heap.alloc_closure(inner_fn_proto, Vec::new());
    
    // Create a main function that calls the inner function
    let main_fn_proto = create_call_function(&mut vm, inner_fn_closure)?;
    let main_closure = vm.heap.alloc_closure(main_fn_proto, Vec::new());
    
    // Execute the main function
    debug_handler.enter_call();
    match vm.execute_function(main_closure, &[]) {
        Ok(result) => {
            debug_handler.exit_call(&result);
            println!("✅ Success: {}", value_to_string(&mut vm, result));
        }
        Err(e) => {
            println!("❌ Error: {}", e);
        }
    }
    
    // Test 2: Manually create a minimal function to debug more closely
    println!("\nTest 2: Minimal function call with manual bytecode");
    test_minimal_function_call()?;

    Ok(())
}

// Create a function that returns the string "hello"
fn create_return_string_function(vm: &mut LuaVM) -> Result<FunctionProto> {
    let mut proto = FunctionProto::default();
    
    // 1. Add a string constant "hello"
    let hello_str = vm.heap.create_string("hello");
    proto.constants.push(Value::String(hello_str));
    
    // 2. LOADK R(0) <- K(0)  - Load "hello" into register 0
    let loadk = (OpCode::LoadK as u32) | ((0u32) << 6) | ((0u32) << 14);
    proto.code.push(Instruction::new(loadk));
    
    // 3. RETURN R(0) 2       - Return 1 value (in R0)
    let ret = (OpCode::Return as u32) | ((0u32) << 6) | ((2u32) << 14);
    proto.code.push(Instruction::new(ret));
    
    // Set stack size and metadata
    proto.max_stack_size = 1;
    
    Ok(proto)
}

// Create a function that calls another function
fn create_call_function(vm: &mut LuaVM, callee: ClosureHandle) -> Result<FunctionProto> {
    let mut proto = FunctionProto::default();
    
    // 1. Add callee as a constant
    proto.constants.push(Value::Closure(callee));
    
    // 2. LOADK R(0) <- K(0)  - Load callee into register 0
    let loadk = (OpCode::LoadK as u32) | ((0u32) << 6) | ((0u32) << 14);
    proto.code.push(Instruction::new(loadk));
    
    // 3. CALL R(0) 1 2       - Call with 0 args, expecting 1 return value
    let call = (OpCode::Call as u32) | ((0u32) << 6) | ((1u32) << 14) | ((2u32) << 23);
    proto.code.push(Instruction::new(call));
    
    // 4. RETURN R(0) 2       - Return 1 value (return value from the call)
    let ret = (OpCode::Return as u32) | ((0u32) << 6) | ((2u32) << 14);
    proto.code.push(Instruction::new(ret));
    
    // Set stack size and metadata
    proto.max_stack_size = 1;
    
    Ok(proto)
}

// Test with ultra-minimal function call to debug the issue
fn test_minimal_function_call() -> Result<()> {
    // Create a new VM
    let mut config = VMConfig::default();
    config.debug = true;
    let mut vm = LuaVM::new(config);
    
    println!("Creating minimal test functions...");
    
    // Inner function: loads a number and returns it
    let mut inner_proto = FunctionProto::default();
    inner_proto.constants.push(Value::Number(42.0));
    
    // LOADK R(0) <- K(0)
    let loadk = (OpCode::LoadK as u32) | ((0u32) << 6) | ((0u32) << 14);
    inner_proto.code.push(Instruction::new(loadk));
    
    // RETURN R(0) 2  (one value, which is in R(0))
    let ret = (OpCode::Return as u32) | ((0u32) << 6) | ((2u32) << 14);
    inner_proto.code.push(Instruction::new(ret));
    
    // Set stack size
    inner_proto.max_stack_size = 1;
    
    // Create the inner function closure
    let inner_closure = vm.heap.alloc_closure(inner_proto, Vec::new());
    println!("  Inner function created with handle: {:?}", inner_closure);
    
    // Outer function that calls the inner function
    let mut outer_proto = FunctionProto::default();
    outer_proto.constants.push(Value::Closure(inner_closure));
    
    // LOADK R(0) <- K(0)  (inner function)
    let loadk = (OpCode::LoadK as u32) | ((0u32) << 6) | ((0u32) << 14);
    outer_proto.code.push(Instruction::new(loadk));
    
    // CALL R(0), 1, 2  (no args, expect 1 return value)
    let call = (OpCode::Call as u32) | ((0u32) << 6) | ((1u32) << 14) | ((2u32) << 23);
    outer_proto.code.push(Instruction::new(call));
    
    // RETURN R(0) 2  (return the value from the inner function)
    let ret = (OpCode::Return as u32) | ((0u32) << 6) | ((2u32) << 14);
    outer_proto.code.push(Instruction::new(ret));
    
    // Set stack size
    outer_proto.max_stack_size = 1;
    
    // Create the outer function closure
    let outer_closure = vm.heap.alloc_closure(outer_proto, Vec::new());
    println!("  Outer function created with handle: {:?}", outer_closure);
    
    // Execute the outer function
    println!("Executing outer function...");
    match vm.execute_function(outer_closure, &[]) {
        Ok(result) => {
            println!("✅ Success: Value returned: {:?}", result);
            Ok(())
        }
        Err(e) => {
            println!("❌ Error: {}", e);
            Err(e)
        }
    }
}