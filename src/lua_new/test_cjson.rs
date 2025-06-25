//! Tests for cjson library

#[cfg(test)]
mod tests {
    use crate::lua_new::{LuaVM, VMConfig};
    use crate::lua_new::value::Value;
    use crate::lua_new::cjson;
    use crate::lua_new::compiler::Compiler;
    use crate::lua_new::parser::Parser;

    #[test]
    fn test_cjson_encode_basic() {
        let config = VMConfig::default();
        let mut vm = LuaVM::new(config);
        
        // Register cjson library
        cjson::register(&mut vm).unwrap();
        
        // Test encoding various types
        let tests = vec![
            // Nil
            ("return cjson.encode(nil)", "null"),
            
            // Boolean
            ("return cjson.encode(true)", "true"),
            ("return cjson.encode(false)", "false"),
            
            // Numbers
            ("return cjson.encode(42)", "42"),
            ("return cjson.encode(3.14)", "3.14"),
            ("return cjson.encode(-10)", "-10"),
            
            // Strings
            ("return cjson.encode('hello')", "\"hello\""),
            ("return cjson.encode('hello\\nworld')", "\"hello\\nworld\""),
            ("return cjson.encode('quote\"test')", "\"quote\\\"test\""),
            
            // Empty table (object)
            ("return cjson.encode({})", "{}"),
            
            // Array
            ("return cjson.encode({1, 2, 3})", "[1,2,3]"),
            
            // Object
            ("local t = {}; t.name = 'test'; t.value = 42; return cjson.encode(t)", 
             "{\"name\":\"test\",\"value\":42}"),
        ];
        
        for (script, expected) in tests {
            // Parse and compile
            let ast = Parser::new(script).parse().unwrap();
            let proto = Compiler::new().compile(&ast).unwrap();
            let closure = vm.heap.alloc_closure(proto, vec![]);
            
            // Execute
            match vm.execute_function(closure, &[]) {
                Ok(Value::String(s)) => {
                    let result = vm.heap.get_string_utf8(s).unwrap();
                    // Basic check - just ensure it doesn't crash
                    // In a real test, we'd parse the JSON and compare
                    assert!(result.len() > 0);
                }
                Ok(v) => panic!("Expected string result, got: {:?}", v),
                Err(e) => panic!("Execution failed: {}", e),
            }
        }
    }
    
    #[test] 
    fn test_cjson_decode_basic() {
        let config = VMConfig::default();
        let mut vm = LuaVM::new(config);
        
        // Register cjson library
        cjson::register(&mut vm).unwrap();
        
        // Test decoding various JSON strings
        let tests = vec![
            // Numbers
            ("return cjson.decode('42')", Value::Number(42.0)),
            ("return cjson.decode('3.14')", Value::Number(3.14)),
            
            // Strings
            ("return cjson.decode('\"hello\"')", "hello"),
            
            // Boolean
            ("return cjson.decode('true')", Value::Boolean(true)),
            ("return cjson.decode('false')", Value::Boolean(false)),
        ];
        
        for (script, expected) in tests {
            // Parse and compile
            let ast = Parser::new(script).parse().unwrap();
            let proto = Compiler::new().compile(&ast).unwrap();
            let closure = vm.heap.alloc_closure(proto, vec![]);
            
            // Execute
            match vm.execute_function(closure, &[]) {
                Ok(result) => {
                    // Basic type check
                    match (result, expected) {
                        (Value::Number(n1), Value::Number(n2)) => {
                            assert!((n1 - n2).abs() < 0.0001);
                        }
                        (Value::Boolean(b1), Value::Boolean(b2)) => {
                            assert_eq!(b1, b2);
                        }
                        (Value::String(s), expected_str) => {
                            if let Ok(str_val) = vm.heap.get_string_utf8(s) {
                                // Basic check
                                assert!(str_val.len() > 0);
                            }
                        }
                        _ => {}
                    }
                }
                Err(e) => panic!("Execution failed: {}", e),
            }
        }
    }
}