//! Integration tests for upvalue capture in nested functions

use ferrous::lua::parser::parse_program;
use ferrous::lua::codegen::generate_bytecode;
use ferrous::lua::codegen::{Instruction, OpCode};

#[test]
fn test_simple_upvalue_capture() {
    let code = r#"
        local x = 10
        function get_x()
            return x
        end
    "#;
    
    let chunk = parse_program(code).expect("Failed to parse");
    let output = generate_bytecode(&chunk).expect("Failed to generate bytecode");
    
    // Verify function prototype has upvalue
    assert_eq!(output.main.prototypes.len(), 1, "Should have one function prototype");
    let func_proto = &output.main.prototypes[0];
    assert_eq!(func_proto.upvalues.len(), 1, "Function should have one upvalue");
    
    // Verify upvalue info
    let upval = &func_proto.upvalues[0];
    assert!(upval.in_stack, "Upvalue should be in stack");
    assert_eq!(upval.index, 0, "Upvalue should reference register 0 (where x is stored)");
    
    // Verify bytecode uses GETUPVAL
    let has_getupval = func_proto.bytecode.iter()
        .any(|&instr| Instruction(instr).get_opcode() == OpCode::GetUpval);
    assert!(has_getupval, "Function should use GETUPVAL to access x");
}

#[test]
fn test_upvalue_assignment() {
    let code = r#"
        local x = 10
        function set_x(val)
            x = val
        end
    "#;
    
    let chunk = parse_program(code).expect("Failed to parse");
    let output = generate_bytecode(&chunk).expect("Failed to generate bytecode");
    
    // Verify function prototype has upvalue
    let func_proto = &output.main.prototypes[0];
    assert_eq!(func_proto.upvalues.len(), 1, "Function should have one upvalue");
    
    // Verify bytecode uses SETUPVAL
    let has_setupval = func_proto.bytecode.iter()
        .any(|&instr| Instruction(instr).get_opcode() == OpCode::SetUpval);
    assert!(has_setupval, "Function should use SETUPVAL to modify x");
}

#[test]
fn test_nested_upvalue_capture() {
    let code = r#"
        local x = 10
        function outer()
            local y = 20
            function inner()
                return x + y
            end
            return inner
        end
    "#;
    
    let chunk = parse_program(code).expect("Failed to parse");
    let output = generate_bytecode(&chunk).expect("Failed to generate bytecode");
    
    // Check outer function
    let outer = &output.main.prototypes[0];
    assert_eq!(outer.upvalues.len(), 1, "Outer should have one upvalue (x)");
    assert!(outer.upvalues[0].in_stack, "x should be captured from main's stack");
    
    // Check inner function
    assert_eq!(outer.prototypes.len(), 1, "Outer should have one nested function");
    let inner = &outer.prototypes[0];
    assert_eq!(inner.upvalues.len(), 2, "Inner should have two upvalues (x and y)");
    
    // First upvalue (x) should come from outer's upvalues
    assert!(!inner.upvalues[0].in_stack, "x should come from outer's upvalues");
    assert_eq!(inner.upvalues[0].index, 0, "x should be outer's first upvalue");
    
    // Second upvalue (y) should come from outer's stack
    assert!(inner.upvalues[1].in_stack, "y should come from outer's stack");
}

#[test] 
fn test_multiple_functions_sharing_upvalue() {
    let code = r#"
        local shared = 0
        
        function increment()
            shared = shared + 1
        end
        
        function get_value()
            return shared
        end
    "#;
    
    let chunk = parse_program(code).expect("Failed to parse");
    let output = generate_bytecode(&chunk).expect("Failed to generate bytecode");
    
    // Both functions should capture the same variable
    assert_eq!(output.main.prototypes.len(), 2, "Should have two functions");
    
    // increment function
    let increment = &output.main.prototypes[0];
    assert_eq!(increment.upvalues.len(), 1, "increment should have one upvalue");
    assert!(increment.upvalues[0].in_stack);
    assert_eq!(increment.upvalues[0].index, 0);
    
    // get_value function
    let get_value = &output.main.prototypes[1];
    assert_eq!(get_value.upvalues.len(), 1, "get_value should have one upvalue");
    assert!(get_value.upvalues[0].in_stack);
    assert_eq!(get_value.upvalues[0].index, 0); // Same register as increment
}

#[test]
fn test_upvalue_not_created_for_globals() {
    let code = r#"
        global_var = 10  -- This is a global
        
        function use_global()
            return global_var
        end
    "#;
    
    let chunk = parse_program(code).expect("Failed to parse");
    let output = generate_bytecode(&chunk).expect("Failed to generate bytecode");
    
    // Function should not have upvalues for global variables
    let func = &output.main.prototypes[0];
    assert_eq!(func.upvalues.len(), 0, "Function should have no upvalues");
    
    // Should use GETGLOBAL instead
    let has_getglobal = func.bytecode.iter()
        .any(|&instr| Instruction(instr).get_opcode() == OpCode::GetGlobal);
    assert!(has_getglobal, "Function should use GETGLOBAL for global variable");
}

#[test]
fn test_deeply_nested_upvalue_chain() {
    let code = r#"
        local a = 1
        function level1()
            local b = 2
            function level2()
                local c = 3
                function level3()
                    return a + b + c
                end
                return level3
            end
            return level2
        end
    "#;
    
    let chunk = parse_program(code).expect("Failed to parse");
    let output = generate_bytecode(&chunk).expect("Failed to generate bytecode");
    
    // Check level1
    let level1 = &output.main.prototypes[0];
    assert_eq!(level1.upvalues.len(), 1, "level1 captures a");
    
    // Check level2
    let level2 = &level1.prototypes[0];
    assert_eq!(level2.upvalues.len(), 2, "level2 captures a and b");
    
    // Check level3
    let level3 = &level2.prototypes[0];
    assert_eq!(level3.upvalues.len(), 3, "level3 captures a, b, and c");
}

#[test]
fn test_upvalue_in_conditional() {
    let code = r#"
        local x = 10
        
        if true then
            function get_x()
                return x
            end
        end
    "#;
    
    let chunk = parse_program(code).expect("Failed to parse");
    let output = generate_bytecode(&chunk).expect("Failed to generate bytecode");
    
    // Function inside conditional should still capture upvalue
    let func = &output.main.prototypes[0];
    assert_eq!(func.upvalues.len(), 1, "Function should capture x");
    assert!(func.upvalues[0].in_stack);
}