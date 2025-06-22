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
        let result = vm.run_simple("return 1 + 2 * 3").unwrap();
        
        // Verify result (1 + 2*3 = 7)
        if let LuaValue::Number(n) = result {
            assert_eq!(n, 7.0);
        } else {
            panic!("Expected number result");
        }
    }
    
    #[test]
    fn test_pure_lua_operations() {
        let mut vm = vm::LuaVm::new();
        
        // Test a more complex script with local variables, functions, and operations
        let script = r#"
            local a = 10
            local b = 20
            local function add(x, y)
                return x + y
            end
            return add(a, b)
        "#;
        
        let result = vm.run_simple(script).unwrap();
        
        // Verify result (10 + 20 = 30)
        if let LuaValue::Number(n) = result {
            assert_eq!(n, 30.0);
        } else {
            panic!("Expected number result");
        }
    }
    
    #[test]
    fn test_lua_table_creation() {
        let mut vm = vm::LuaVm::new();
        
        // Test table creation and access
        let script = r#"
            local t = {foo = "bar", baz = 42}
            return t.foo .. " " .. t.baz
        "#;
        
        let result = vm.run_simple(script).unwrap();
        
        // Verify result
        if let LuaValue::String(s) = result {
            assert_eq!(s.to_str().unwrap(), "bar 42");
        } else {
            panic!("Expected string result");
        }
    }
}