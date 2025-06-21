//! Tests for the Lua interpreter

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::lua::value::LuaString;
    
    #[test]
    fn test_simple_parse() {
        let script = r#"
            local x = 10
            local y = 20
            return x + y
        "#;
        
        let mut parser = parser::Parser::new(script).unwrap();
        let chunk = parser.parse().unwrap();
        
        // Verify structure of AST
        assert!(chunk.block.statements.len() == 2); // Two local assignments
        assert!(chunk.block.return_stmt.is_some()); // Return statement exists
    }
    
    #[test]
    fn test_simple_compile() {
        let script = "return 1 + 2";
        
        let mut parser = parser::Parser::new(script).unwrap();
        let chunk = parser.parse().unwrap();
        
        let mut compiler = compiler::Compiler::new();
        let proto = compiler.compile_chunk(&chunk).unwrap();
        
        // Verify bytecode
        assert!(proto.constants.len() >= 2);  // Should have at least constants 1 and 2
        assert!(proto.code.len() >= 3);       // LoadK, LoadK, Add, Return
    }
    
    #[test]
    fn test_simple_execution() {
        let mut vm = vm::LuaVm::new();
        
        // Test running a simple script
        let result = vm.run("return 1 + 2 * 3").unwrap();
        
        // Verify result (1 + 2*3 = 7)
        if let LuaValue::Number(n) = result {
            assert_eq!(n, 7.0);
        } else {
            panic!("Expected number result");
        }
    }
    
    #[test]
    fn test_table_operations() {
        let mut vm = vm::LuaVm::new();
        
        // Test table creation and access
        let script = r#"
            local t = {foo = "bar", baz = 42}
            return t.foo .. " " .. t.baz
        "#;
        
        let result = vm.run(script).unwrap();
        
        // Verify result
        if let LuaValue::String(s) = result {
            if let Ok(str_val) = s.to_str() {
                assert_eq!(str_val, "bar 42");
            } else {
                panic!("Expected valid string");
            }
        } else {
            panic!("Expected string result");
        }
    }
    
    #[test]
    fn test_variables_and_scope() {
        let mut vm = vm::LuaVm::new();
        
        // Test variable scoping
        let script = r#"
            local x = 10
            local result
            do
                local x = 20
                result = x
            end
            return {outer = x, inner = result}
        "#;
        
        let result = vm.run(script).unwrap();
        
        // Verify table has correct values for both scopes
        if let LuaValue::Table(t) = result {
            let table = t.borrow();
            
            let outer = table.get(&LuaValue::String(LuaString::from_str("outer")));
            let inner = table.get(&LuaValue::String(LuaString::from_str("inner")));
            
            assert!(matches!(outer, Some(LuaValue::Number(x)) if *x == 10.0));
            assert!(matches!(inner, Some(LuaValue::Number(x)) if *x == 20.0));
        } else {
            panic!("Expected table result");
        }
    }
    
    #[test]
    fn test_control_flow() {
        let mut vm = vm::LuaVm::new();
        
        // Test if/else control flow
        let script = r#"
            local x = 15
            local result
            if x > 10 then
                result = "greater"
            else
                result = "lesser"
            end
            return result
        "#;
        
        let result = vm.run(script).unwrap();
        
        // Verify result
        if let LuaValue::String(s) = result {
            if let Ok(str_val) = s.to_str() {
                assert_eq!(str_val, "greater");
            } else {
                panic!("Expected valid string");
            }
        } else {
            panic!("Expected string result");
        }
    }
    
    #[test]
    fn test_loops() {
        let mut vm = vm::LuaVm::new();
        
        // Test loop functionality
        let script = r#"
            local sum = 0
            for i = 1, 10 do
                sum = sum + i
            end
            return sum
        "#;
        
        let result = vm.run(script).unwrap();
        
        // Verify result (sum of 1-10 = 55)
        if let LuaValue::Number(n) = result {
            assert_eq!(n, 55.0);
        } else {
            panic!("Expected number result");
        }
    }
}