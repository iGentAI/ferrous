//! Opcode and VM Feature Compliance Tests
//!
//! This module provides comprehensive tests for the Lua VM implementation,
//! verifying that all opcodes and language features behave correctly.

use crate::lua::{LuaVM, Value, compile};
use crate::lua::transaction::HeapTransaction;
use std::sync::Arc;

/// Helper function to create a VM and execute a Lua script
fn execute_lua(script: &str) -> Value {
    // Create a fresh VM
    let mut vm = LuaVM::new().expect("Failed to create VM");
    
    // Compile the script
    let module = compile(script).expect("Failed to compile script");
    
    // Execute the script
    vm.execute_module(&module, &[]).expect("Failed to execute script")
}

/// Test basic arithmetic operations
#[test]
fn test_arithmetic_operations() {
    // Addition
    assert_eq!(execute_lua("return 1 + 2"), Value::Number(3.0));
    
    // Subtraction
    assert_eq!(execute_lua("return 5 - 3"), Value::Number(2.0));
    
    // Multiplication
    assert_eq!(execute_lua("return 2 * 3"), Value::Number(6.0));
    
    // Division
    assert_eq!(execute_lua("return 10 / 2"), Value::Number(5.0));
    
    // Modulo
    assert_eq!(execute_lua("return 10 % 3"), Value::Number(1.0));
    
    // Exponentiation
    assert_eq!(execute_lua("return 2 ^ 3"), Value::Number(8.0));
    
    // Unary minus
    assert_eq!(execute_lua("return -5"), Value::Number(-5.0));
    
    // Mixed operations with precedence
    assert_eq!(execute_lua("return 2 + 3 * 4"), Value::Number(14.0));
    assert_eq!(execute_lua("return (2 + 3) * 4"), Value::Number(20.0));
    assert_eq!(execute_lua("return 2 ^ 3 * 4"), Value::Number(32.0));
}

/// Test logical operations
#[test]
fn test_logical_operations() {
    // NOT
    assert_eq!(execute_lua("return not true"), Value::Boolean(false));
    assert_eq!(execute_lua("return not false"), Value::Boolean(true));
    assert_eq!(execute_lua("return not nil"), Value::Boolean(true));
    assert_eq!(execute_lua("return not 0"), Value::Boolean(false));
    
    // AND
    assert_eq!(execute_lua("return true and true"), Value::Boolean(true));
    assert_eq!(execute_lua("return true and false"), Value::Boolean(false));
    assert_eq!(execute_lua("return false and true"), Value::Boolean(false));
    assert_eq!(execute_lua("return false and false"), Value::Boolean(false));
    
    // OR
    assert_eq!(execute_lua("return true or true"), Value::Boolean(true));
    assert_eq!(execute_lua("return true or false"), Value::Boolean(true));
    assert_eq!(execute_lua("return false or true"), Value::Boolean(true));
    assert_eq!(execute_lua("return false or false"), Value::Boolean(false));
    
    // Short-circuit evaluation
    assert_eq!(execute_lua("local a = 0; false and a=1; return a"), Value::Number(0.0));
    assert_eq!(execute_lua("local a = 0; true or a=1; return a"), Value::Number(0.0));
    assert_eq!(execute_lua("local a = 0; true and a=1; return a"), Value::Number(1.0));
    assert_eq!(execute_lua("local a = 0; false or a=1; return a"), Value::Number(1.0));
}

/// Test comparison operations
#[test]
fn test_comparison_operations() {
    // Equal
    assert_eq!(execute_lua("return 1 == 1"), Value::Boolean(true));
    assert_eq!(execute_lua("return 1 == 2"), Value::Boolean(false));
    assert_eq!(execute_lua("return 'a' == 'a'"), Value::Boolean(true));
    assert_eq!(execute_lua("return 'a' == 'b'"), Value::Boolean(false));
    
    // Not equal
    assert_eq!(execute_lua("return 1 ~= 1"), Value::Boolean(false));
    assert_eq!(execute_lua("return 1 ~= 2"), Value::Boolean(true));
    
    // Less than
    assert_eq!(execute_lua("return 1 < 2"), Value::Boolean(true));
    assert_eq!(execute_lua("return 2 < 2"), Value::Boolean(false));
    assert_eq!(execute_lua("return 3 < 2"), Value::Boolean(false));
    
    // Less than or equal
    assert_eq!(execute_lua("return 1 <= 2"), Value::Boolean(true));
    assert_eq!(execute_lua("return 2 <= 2"), Value::Boolean(true));
    assert_eq!(execute_lua("return 3 <= 2"), Value::Boolean(false));
    
    // Greater than
    assert_eq!(execute_lua("return 1 > 2"), Value::Boolean(false));
    assert_eq!(execute_lua("return 2 > 2"), Value::Boolean(false));
    assert_eq!(execute_lua("return 3 > 2"), Value::Boolean(true));
    
    // Greater than or equal
    assert_eq!(execute_lua("return 1 >= 2"), Value::Boolean(false));
    assert_eq!(execute_lua("return 2 >= 2"), Value::Boolean(true));
    assert_eq!(execute_lua("return 3 >= 2"), Value::Boolean(true));
    
    // Different types
    assert_eq!(execute_lua("return 1 == '1'"), Value::Boolean(false));
    assert_eq!(execute_lua("return nil == false"), Value::Boolean(false));
}

/// Test table operations
#[test]
fn test_table_operations() {
    // Table creation and indexing
    assert_eq!(execute_lua("local t = {10, 20, 30}; return t[2]"), Value::Number(20.0));
    
    // Table field assignment
    assert_eq!(execute_lua("local t = {}; t[1] = 5; return t[1]"), Value::Number(5.0));
    
    // Table field with string keys
    assert_eq!(execute_lua("local t = {}; t.x = 5; return t.x"), Value::Number(5.0));
    assert_eq!(execute_lua("local t = {}; t['x'] = 5; return t['x']"), Value::Number(5.0));
    
    // Mixed key types
    let script = r#"
        local t = {}
        t[1] = 'one'
        t['two'] = 2
        t[true] = 'boolean'
        
        return t[1] .. " " .. t['two'] .. " " .. t[true]
    "#;
    
    // This should return "one 2 boolean"
    // We need to check the string value differently
    let result = execute_lua(script);
    if let Value::String(handle) = result {
        let mut vm = LuaVM::new().expect("Failed to create VM");
        let mut tx = HeapTransaction::new(vm.heap_mut());
        let value = tx.get_string_value(handle).expect("Failed to get string");
        assert_eq!(value, "one 2 boolean");
    } else {
        panic!("Expected string, got {:?}", result);
    }
    
    // Table length
    assert_eq!(execute_lua("return #{1, 2, 3}"), Value::Number(3.0));
    
    // Nested tables
    assert_eq!(execute_lua("local t = {x = {y = 5}}; return t.x.y"), Value::Number(5.0));
}

/// Test control flow operations
#[test]
fn test_control_flow() {
    // If-then
    assert_eq!(execute_lua("local x = 0; if true then x = 1 end; return x"), Value::Number(1.0));
    assert_eq!(execute_lua("local x = 0; if false then x = 1 end; return x"), Value::Number(0.0));
    
    // If-then-else
    assert_eq!(execute_lua("local x = 0; if true then x = 1 else x = 2 end; return x"), Value::Number(1.0));
    assert_eq!(execute_lua("local x = 0; if false then x = 1 else x = 2 end; return x"), Value::Number(2.0));
    
    // If-elseif-else
    let script = r#"
        local x = 2
        local result = 0
        
        if x == 1 then
            result = 10
        elseif x == 2 then
            result = 20
        else
            result = 30
        end
        
        return result
    "#;
    assert_eq!(execute_lua(script), Value::Number(20.0));
    
    // While loop
    assert_eq!(execute_lua("local x = 0; while x < 5 do x = x + 1 end; return x"), Value::Number(5.0));
    
    // Repeat-until loop
    assert_eq!(execute_lua("local x = 0; repeat x = x + 1 until x >= 5; return x"), Value::Number(5.0));
    
    // Break statement
    assert_eq!(execute_lua("local x = 0; while true do x = x + 1; if x >= 5 then break end end; return x"), Value::Number(5.0));
    
    // Numeric for loop
    assert_eq!(execute_lua("local sum = 0; for i = 1, 5 do sum = sum + i end; return sum"), Value::Number(15.0));
    
    // Numeric for loop with step
    assert_eq!(execute_lua("local sum = 0; for i = 1, 10, 2 do sum = sum + i end; return sum"), Value::Number(25.0));
    
    // Generic for loop
    let script = r#"
        local t = {10, 20, 30}
        local sum = 0
        
        for i, v in ipairs(t) do
            sum = sum + v
        end
        
        return sum
    "#;
    
    // This should return 60 (10 + 20 + 30)
    // But we need to implement ipairs first
    // assert_eq!(execute_lua(script), Value::Number(60.0));
}

/// Test function operations
#[test]
fn test_function_operations() {
    // Basic function
    assert_eq!(execute_lua("function add(a, b) return a + b end; return add(2, 3)"), Value::Number(5.0));
    
    // Local function
    assert_eq!(execute_lua("local function add(a, b) return a + b end; return add(2, 3)"), Value::Number(5.0));
    
    // Anonymous function
    assert_eq!(execute_lua("local add = function(a, b) return a + b end; return add(2, 3)"), Value::Number(5.0));
    
    // Recursive function
    let script = r#"
        function factorial(n)
            if n <= 1 then
                return 1
            else
                return n * factorial(n - 1)
            end
        end
        
        return factorial(5)
    "#;
    assert_eq!(execute_lua(script), Value::Number(120.0));
    
    // Multiple return values
    let script = r#"
        function get_values()
            return 1, 2, 3
        end
        
        local a, b, c = get_values()
        return a + b + c
    "#;
    assert_eq!(execute_lua(script), Value::Number(6.0));
    
    // Closures
    let script = r#"
        function make_counter()
            local count = 0
            return function()
                count = count + 1
                return count
            end
        end
        
        local counter = make_counter()
        counter()
        counter()
        return counter()
    "#;
    assert_eq!(execute_lua(script), Value::Number(3.0));
    
    // Upvalues
    let script = r#"
        local x = 10
        
        function get_x()
            return x
        end
        
        function set_x(value)
            x = value
        end
        
        set_x(20)
        return get_x()
    "#;
    assert_eq!(execute_lua(script), Value::Number(20.0));
}

/// Test string operations
#[test]
fn test_string_operations() {
    // String concatenation
    assert_eq!(execute_lua("return 'hello' .. ' ' .. 'world'"), 
               execute_lua("return 'hello world'")); // Compare string values
    
    // String length
    assert_eq!(execute_lua("return #'hello'"), Value::Number(5.0));
    
    // String to number coercion
    assert_eq!(execute_lua("return '10' + 5"), Value::Number(15.0));
    
    // Number to string coercion in concatenation
    let result = execute_lua("return 'The number is ' .. 42");
    if let Value::String(handle) = result {
        let mut vm = LuaVM::new().expect("Failed to create VM");
        let mut tx = HeapTransaction::new(vm.heap_mut());
        let value = tx.get_string_value(handle).expect("Failed to get string");
        assert_eq!(value, "The number is 42");
    } else {
        panic!("Expected string, got {:?}", result);
    }
}

/// Test variable and scope operations
#[test]
fn test_variable_and_scope() {
    // Local variables
    assert_eq!(execute_lua("local x = 5; return x"), Value::Number(5.0));
    
    // Global variables
    assert_eq!(execute_lua("x = 5; return x"), Value::Number(5.0));
    
    // Variable shadowing
    assert_eq!(execute_lua("local x = 5; do local x = 10; end; return x"), Value::Number(5.0));
    
    // Multiple assignment
    assert_eq!(execute_lua("local a, b = 1, 2; return a + b"), Value::Number(3.0));
    
    // Value swapping
    assert_eq!(execute_lua("local a, b = 1, 2; a, b = b, a; return a * 10 + b"), Value::Number(21.0));
    
    // Extra values in assignment are discarded
    assert_eq!(execute_lua("local a = 1, 2, 3; return a"), Value::Number(1.0));
    
    // Missing values in assignment are nil
    let script = r#"
        local a, b, c = 1, 2
        if c == nil then
            return 100
        else
            return 200
        end
    "#;
    assert_eq!(execute_lua(script), Value::Number(100.0));
}

/// Test closures and upvalues
#[test]
fn test_closures_and_upvalues() {
    // Basic closure
    let script = r#"
        function make_adder(x)
            return function(y)
                return x + y
            end
        end
        
        local add5 = make_adder(5)
        return add5(3)
    "#;
    assert_eq!(execute_lua(script), Value::Number(8.0));
    
    // Multiple upvalues
    let script = r#"
        function make_counter(start, step)
            local count = start
            return function()
                count = count + step
                return count
            end
        end
        
        local counter = make_counter(10, 5)
        counter()
        return counter()
    "#;
    assert_eq!(execute_lua(script), Value::Number(20.0));
    
    // Upvalues across multiple closures
    let script = r#"
        local counter = 0
        
        function increment()
            counter = counter + 1
        end
        
        function get_counter()
            return counter
        end
        
        increment()
        increment()
        return get_counter()
    "#;
    assert_eq!(execute_lua(script), Value::Number(2.0));
    
    // Nested closures
    let script = r#"
        function outer()
            local x = 10
            
            return function()
                local y = 20
                return function()
                    return x + y
                end
            end
        end
        
        return outer()()()
    "#;
    assert_eq!(execute_lua(script), Value::Number(30.0));
}

/// Test metatable operations
#[test]
fn test_metatable_operations() {
    // Basic metatable __index
    let script = r#"
        local t1 = {value = 5}
        local t2 = {}
        setmetatable(t2, {__index = t1})
        return t2.value
    "#;
    assert_eq!(execute_lua(script), Value::Number(5.0));
    
    // __index as a function
    let script = r#"
        local t = {}
        setmetatable(t, {
            __index = function(table, key)
                return key * 2
            end
        })
        return t[5]
    "#;
    assert_eq!(execute_lua(script), Value::Number(10.0));
    
    // __newindex
    let script = r#"
        local t1 = {}
        local t2 = {}
        setmetatable(t1, {__newindex = t2})
        t1.x = 10
        return t2.x
    "#;
    assert_eq!(execute_lua(script), Value::Number(10.0));
    
    // Arithmetic metamethods
    let script = r#"
        local mt = {
            __add = function(a, b)
                return a.value + b.value
            end
        }
        
        local t1 = {value = 5}
        local t2 = {value = 10}
        
        setmetatable(t1, mt)
        setmetatable(t2, mt)
        
        return (t1 + t2)
    "#;
    assert_eq!(execute_lua(script), Value::Number(15.0));
}

/// Test error handling
#[test]
fn test_error_handling() {
    // TODO: Implement error handling tests once error propagation is fixed
}

/// Test standard library functions
#[test]
fn test_standard_library() {
    // TODO: Implement standard library tests once the library is implemented
}

// Add more test categories as needed...