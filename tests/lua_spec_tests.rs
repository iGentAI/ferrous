//! Comprehensive Lua 5.1 Specification Test Suite for Redis
//! 
//! This test suite covers all Lua 5.1 features required by Redis,
//! testing each feature in isolation to ensure correctness.

#[cfg(test)]
mod lua_spec_tests {
    use ferrous::lua::{LuaVM, Value, compiler};
    
    // Helper to execute Lua code and check result
    fn test_lua(script: &str, expected: &str) {
        let mut vm = LuaVM::new().unwrap();
        vm.init_stdlib().unwrap();
        
        let compiled = compiler::compile(script).unwrap();
        let result = vm.execute_module(&compiled, &[]).unwrap();
        
        let actual = lua_value_to_string(&vm, &result);
        assert_eq!(actual, expected, "Script: {}", script);
    }
    
    fn lua_value_to_string(vm: &LuaVM, value: &Value) -> String {
        match value {
            Value::Nil => "nil".to_string(),
            Value::Boolean(b) => b.to_string(),
            Value::Number(n) => {
                // Handle special float values
                if n.is_infinite() {
                    if n.is_sign_positive() {
                        "inf".to_string()
                    } else {
                        "-inf".to_string()
                    }
                } else if n.is_nan() {
                    "nan".to_string()
                } else if n.fract() == 0.0 {
                    format!("{:.0}", n)
                } else {
                    n.to_string()
                }
            },
            Value::String(handle) => {
                // Properly convert string handle to actual string
                match vm.heap.get_string_value(*handle) {
                    Ok(s) => s,
                    Err(_) => "<invalid string>".to_string(),
                }
            },
            Value::Table(_) => "table".to_string(),
            Value::Closure(_) => "function".to_string(),
            Value::Thread(_) => "thread".to_string(),
            Value::CFunction(_) => "function".to_string(),
        }
    }
    
    // Test helper for operations that should error
    fn test_lua_error(script: &str) {
        let mut vm = LuaVM::new().unwrap();
        vm.init_stdlib().unwrap();
        
        let compiled = compiler::compile(script).unwrap();
        let result = vm.execute_module(&compiled, &[]);
        
        assert!(result.is_err(), "Expected error for script: {}", script);
    }

    // Basic Types and Literals
    
    #[test]
    fn test_nil() {
        test_lua("return nil", "nil");
    }
    
    #[test] 
    fn test_boolean_true() {
        test_lua("return true", "true");
    }
    
    #[test]
    fn test_boolean_false() {
        test_lua("return false", "false");
    }
    
    #[test]
    fn test_number_integer() {
        test_lua("return 42", "42");
    }
    
    #[test]
    fn test_number_float() {
        test_lua("return 3.14", "3.14");
    }
    
    #[test]
    fn test_number_negative() {
        test_lua("return -5", "-5");
    }
    
    #[test]
    fn test_string_single_quote() {
        test_lua("return 'hello'", "hello");
    }
    
    #[test]
    fn test_string_double_quote() {
        test_lua("return \"world\"", "world");
    }
    
    #[test]
    fn test_string_with_escape() {
        test_lua("return 'hello\\nworld'", "hello\nworld");
    }

    // Arithmetic Operators
    
    #[test]
    fn test_add() {
        test_lua("return 5 + 3", "8");
    }
    
    #[test]
    fn test_subtract() {
        test_lua("return 10 - 4", "6");
    }
    
    #[test]
    fn test_multiply() {
        test_lua("return 6 * 7", "42");
    }
    
    #[test]
    fn test_divide() {
        test_lua("return 20 / 4", "5");
    }
    
    #[test]
    fn test_modulo() {
        test_lua("return 17 % 5", "2");
    }
    
    #[test]
    fn test_power() {
        test_lua("return 2 ^ 3", "8");
    }
    
    #[test]
    fn test_unary_minus() {
        test_lua("return -7", "-7");
    }
    
    #[test]
    fn test_arithmetic_precedence() {
        test_lua("return 2 + 3 * 4", "14");
    }
    
    #[test]
    fn test_arithmetic_parentheses() {
        test_lua("return (2 + 3) * 4", "20");
    }

    // Comparison Operators
    
    #[test]
    fn test_equal() {
        test_lua("return 5 == 5", "true");
        test_lua("return 5 == 6", "false");
    }
    
    #[test]
    fn test_not_equal() {
        test_lua("return 5 ~= 6", "true");
        test_lua("return 5 ~= 5", "false");
    }
    
    #[test]
    fn test_less_than() {
        test_lua("return 3 < 5", "true");
        test_lua("return 5 < 3", "false");
    }
    
    #[test]
    fn test_greater_than() {
        test_lua("return 5 > 3", "true");
        test_lua("return 3 > 5", "false");
    }
    
    #[test]
    fn test_less_equal() {
        test_lua("return 3 <= 5", "true");
        test_lua("return 5 <= 5", "true");
        test_lua("return 6 <= 5", "false");
    }
    
    #[test]
    fn test_greater_equal() {
        test_lua("return 5 >= 3", "true");
        test_lua("return 5 >= 5", "true");
        test_lua("return 5 >= 6", "false");
    }

    // Logical Operators
    
    #[test]
    fn test_and() {
        test_lua("return true and true", "true");
        test_lua("return true and false", "false");
        test_lua("return false and true", "false");
    }
    
    #[test]
    fn test_or() {
        test_lua("return true or false", "true");
        test_lua("return false or true", "true");
        test_lua("return false or false", "false");
    }
    
    #[test]
    fn test_not() {
        test_lua("return not true", "false");
        test_lua("return not false", "true");
        test_lua("return not nil", "true");
    }
    
    #[test]
    fn test_logical_short_circuit() {
        test_lua("return nil and error('should not reach')", "nil");
        test_lua("return 5 or error('should not reach')", "5");
    }

    // String Operations
    
    #[test]
    fn test_string_concat() {
        test_lua("return 'hello' .. ' ' .. 'world'", "hello world");
    }
    
    #[test]
    fn test_string_concat_number() {
        test_lua("return 'answer: ' .. 42", "answer: 42");
    }
    
    #[test]
    fn test_string_length() {
        test_lua("return #'hello'", "5");
    }

    // Variables and Assignment
    
    #[test]
    fn test_local_variable() {
        test_lua("local x = 10; return x", "10");
    }
    
    #[test]
    fn test_multiple_assignment() {
        test_lua("local a, b = 1, 2; return a + b", "3");
    }
    
    #[test]
    fn test_assignment_mismatch() {
        test_lua("local a, b, c = 1, 2; return tostring(c)", "nil");
    }
    
    #[test]
    fn test_global_variable() {
        test_lua("x = 10; return x", "10");
    }

    // Control Flow - If Statement
    
    #[test]
    fn test_if_true() {
        test_lua("if true then return 1 else return 2 end", "1");
    }
    
    #[test]
    fn test_if_false() {
        test_lua("if false then return 1 else return 2 end", "2");
    }
    
    #[test]
    fn test_if_elseif() {
        test_lua("local x = 5; if x < 5 then return 'less' elseif x > 5 then return 'greater' else return 'equal' end", "equal");
    }
    
    #[test]
    fn test_if_nested() {
        test_lua("if true then if false then return 1 else return 2 end else return 3 end", "2");
    }
    
    #[test]
    fn test_if_no_else() {
        test_lua("if false then return 1 end; return 2", "2");
    }

    // Control Flow - While Loop
    
    #[test]
    fn test_while_loop() {
        test_lua("local i, sum = 0, 0; while i < 5 do i = i + 1; sum = sum + i end; return sum", "15");
    }
    
    #[test]
    fn test_while_break() {
        test_lua("local i = 0; while true do i = i + 1; if i > 3 then break end end; return i", "4");
    }

    // Control Flow - Repeat Until
    
    #[test]
    fn test_repeat_until() {
        test_lua("local i, sum = 0, 0; repeat i = i + 1; sum = sum + i until i >= 5; return sum", "15");
    }

    // Control Flow - For Loop (numeric)
    
    #[test]
    fn test_for_numeric() {
        test_lua("local sum = 0; for i = 1, 5 do sum = sum + i end; return sum", "15");
    }
    
    #[test]
    fn test_for_numeric_step() {
        test_lua("local sum = 0; for i = 1, 10, 2 do sum = sum + i end; return sum", "25");
    }
    
    #[test]
    fn test_for_numeric_negative_step() {
        test_lua("local sum = 0; for i = 5, 1, -1 do sum = sum + i end; return sum", "15");
    }

    // Tables - Basic Operations
    
    #[test]
    fn test_table_empty() {
        test_lua("local t = {}; return type(t)", "table");
    }
    
    #[test]
    fn test_table_array() {
        test_lua("local t = {10, 20, 30}; return t[2]", "20");
    }
    
    #[test]
    fn test_table_hash() {
        test_lua("local t = {a=1, b=2}; return t.a + t.b", "3");
    }
    
    #[test]
    fn test_table_mixed() {
        test_lua("local t = {10, a=20, 30}; return t[1] + t.a + t[2]", "60");
    }
    
    #[test]
    fn test_table_nested() {
        test_lua("local t = {a={b={c=42}}}; return t.a.b.c", "42");
    }
    
    #[test]
    fn test_table_length() {
        test_lua("return #{10, 20, 30, 40, 50}", "5");
    }
    
    #[test]
    fn test_table_assignment() {
        test_lua("local t = {a=1}; t.a = 10; t.b = 20; return t.a + t.b", "30");
    }

    // Functions - Basic
    
    #[test]
    fn test_function_simple() {
        test_lua("local function f() return 42 end; return f()", "42");
    }
    
    #[test]
    fn test_function_params() {
        test_lua("local function add(a, b) return a + b end; return add(3, 4)", "7");
    }
    
    #[test]
    fn test_function_local() {
        test_lua("local function f() local x = 10; return x end; return f()", "10");
    }
    
    #[test]
    fn test_function_multiple_returns() {
        test_lua("local function f() return 1, 2, 3 end; local a, b, c = f(); return a + b + c", "6");
    }
    
    #[test]
    fn test_function_varargs() {
        test_lua("local function sum(...) local s = 0; local args = {...}; for i = 1, #args do s = s + args[i] end; return s end; return sum(1, 2, 3, 4)", "10");
    }

    // Functions - Recursion
    
    #[test]
    fn test_factorial() {
        test_lua("local function fact(n) if n <= 1 then return 1 else return n * fact(n-1) end end; return fact(5)", "120");
    }
    
    #[test]
    fn test_fibonacci() {
        test_lua("local function fib(n) if n <= 1 then return n else return fib(n-1) + fib(n-2) end end; return fib(10)", "55");
    }

    // Closures and Upvalues
    
    #[test]
    fn test_closure_simple() {
        test_lua("local function outer() local x = 10; local function inner() return x end; return inner end; local f = outer(); return f()", "10");
    }
    
    #[test]
    fn test_closure_counter() {
        test_lua("local function counter() local i = 0; return function() i = i + 1; return i end end; local c = counter(); c(); c(); return c()", "3");
    }
    
    #[test]
    fn test_closure_multiple() {
        test_lua("local function counter() local i = 0; return function() i = i + 1; return i end end; local c1 = counter(); local c2 = counter(); c1(); c2(); c2(); return c1() + c2()", "5");
    }

    // Iterators - pairs and ipairs
    
    #[test]
    fn test_pairs() {
        test_lua("local t = {a=1, b=2, c=3}; local sum = 0; for k, v in pairs(t) do sum = sum + v end; return sum", "6");
    }
    
    #[test]
    fn test_ipairs() {
        test_lua("local t = {10, 20, 30}; local sum = 0; for i, v in ipairs(t) do sum = sum + v end; return sum", "60");
    }
    
    #[test]
    fn test_custom_iterator() {
        test_lua(
            "local function iter(t) local i = 0; return function() i = i + 1; if i <= #t then return i, t[i] end end end; local t = {10, 20, 30}; local sum = 0; for i, v in iter(t) do sum = sum + v end; return sum", 
            "60"
        );
    }

    // Standard Library - Type and Conversion
    
    #[test]
    fn test_type() {
        test_lua("return type(nil)", "nil");
        test_lua("return type(true)", "boolean");
        test_lua("return type(42)", "number");
        test_lua("return type('hello')", "string");
        test_lua("return type({})", "table");
        test_lua("return type(function() end)", "function");
    }
    
    #[test]
    fn test_tostring() {
        test_lua("return tostring(42)", "42");
        test_lua("return tostring(true)", "true");
        test_lua("return tostring(nil)", "nil");
    }
    
    #[test]
    fn test_tonumber() {
        test_lua("return tonumber('42')", "42");
        test_lua("return tonumber('3.14')", "3.14");
        test_lua("return tostring(tonumber('invalid'))", "nil");
    }

    // Standard Library - String Functions
    
    #[test]
    fn test_string_len() {
        test_lua("return string.len('hello')", "5");
    }
    
    #[test]
    fn test_string_sub() {
        test_lua("return string.sub('hello', 2, 4)", "ell");
    }
    
    #[test]
    fn test_string_upper() {
        test_lua("return string.upper('hello')", "HELLO");
    }
    
    #[test]
    fn test_string_lower() {
        test_lua("return string.lower('HELLO')", "hello");
    }
    
    #[test]
    fn test_string_rep() {
        test_lua("return string.rep('ab', 3)", "ababab");
    }
    
    #[test]
    fn test_string_reverse() {
        test_lua("return string.reverse('hello')", "olleh");
    }
    
    #[test]
    fn test_string_find() {
        test_lua("local s, e = string.find('hello world', 'world'); return s", "7");
    }
    
    #[test]
    fn test_string_gsub() {
        test_lua("local s = string.gsub('hello world', 'world', 'lua'); return s", "hello lua");
    }
    
    #[test]
    fn test_string_format() {
        test_lua("return string.format('%d %s', 42, 'test')", "42 test");
    }

    // Standard Library - Table Functions
    
    #[test]
    fn test_table_insert() {
        test_lua("local t = {1, 2, 3}; table.insert(t, 4); return t[4]", "4");
        test_lua("local t = {1, 2, 3}; table.insert(t, 2, 5); return t[2]", "5");
    }
    
    #[test]
    fn test_table_remove() {
        test_lua("local t = {1, 2, 3, 4}; local x = table.remove(t); return x", "4");
        test_lua("local t = {1, 2, 3, 4}; local x = table.remove(t, 2); return x", "2");
    }
    
    #[test]
    fn test_table_concat() {
        test_lua("return table.concat({'a', 'b', 'c'})", "abc");
        test_lua("return table.concat({'a', 'b', 'c'}, '-')", "a-b-c");
    }
    
    #[test]
    fn test_table_sort() {
        test_lua("local t = {3, 1, 4, 2}; table.sort(t); return table.concat(t)", "1234");
    }

    // Standard Library - Math Functions
    
    #[test]
    fn test_math_abs() {
        test_lua("return math.abs(-42)", "42");
        test_lua("return math.abs(42)", "42");
    }
    
    #[test]
    fn test_math_floor() {
        test_lua("return math.floor(3.7)", "3");
        test_lua("return math.floor(-3.7)", "-4");
    }
    
    #[test]
    fn test_math_ceil() {
        test_lua("return math.ceil(3.2)", "4");
        test_lua("return math.ceil(-3.2)", "-3");
    }
    
    #[test]
    fn test_math_max() {
        test_lua("return math.max(3, 1, 4, 1, 5)", "5");
    }
    
    #[test]
    fn test_math_min() {
        test_lua("return math.min(3, 1, 4, 1, 5)", "1");
    }
    
    #[test]
    fn test_math_sqrt() {
        test_lua("return math.sqrt(16)", "4");
    }
    
    #[test]
    fn test_math_pow() {
        test_lua("return math.pow(2, 3)", "8");
    }
    
    #[test]
    fn test_math_random() {
        test_lua("math.randomseed(1); local x = math.random(); return type(x)", "number");
    }

    // Standard Library - Other Functions
    
    #[test]
    fn test_assert() {
        test_lua("assert(true); return 'ok'", "ok");
    }
    
    #[test]
    fn test_assert_fail() {
        test_lua_error("assert(false, 'assertion failed')");
    }
    
    #[test]
    fn test_pcall() {
        test_lua("local ok, result = pcall(function() return 42 end); return tostring(ok) .. ' ' .. tostring(result)", "true 42");
    }
    
    #[test]
    fn test_pcall_error() {
        test_lua("local ok, err = pcall(function() error('test') end); return ok", "false");
    }
    
    #[test]
    fn test_select() {
        test_lua("return select(2, 'a', 'b', 'c')", "b");
        test_lua("return select('#', 'a', 'b', 'c')", "3");
    }

    // Metatables - Basic
    
    #[test]
    fn test_metatable_index() {
        test_lua("local t = {}; local mt = {__index = {x = 5}}; setmetatable(t, mt); return t.x", "5");
    }
    
    #[test]
    fn test_metatable_index_function() {
        test_lua("local t = {}; local mt = {__index = function(t, k) return k .. '!' end}; setmetatable(t, mt); return t.hello", "hello!");
    }
    
    #[test]
    fn test_metatable_newindex() {
        test_lua("local t = {}; local log = {}; local mt = {__newindex = function(t, k, v) log[k] = v * 2 end}; setmetatable(t, mt); t.x = 5; return log.x", "10");
    }
    
    #[test]
    fn test_metatable_add() {
        test_lua("local mt = {__add = function(a, b) return a.value + b.value end}; local a = {value = 3}; local b = {value = 4}; setmetatable(a, mt); return a + b", "7");
    }
    
    #[test]
    fn test_metatable_tostring() {
        test_lua("local t = {value = 42}; local mt = {__tostring = function(t) return 'value=' .. t.value end}; setmetatable(t, mt); return tostring(t)", "value=42");
    }
    
    #[test]
    fn test_metatable_len() {
        test_lua("local t = {1, 2, 3}; local mt = {__len = function(t) return 100 end}; setmetatable(t, mt); return #t", "100");
    }

    // Redis-specific tests (KEYS, ARGV, redis.call, etc.)
    
    #[test]
    fn test_keys_argv() {
        let mut vm = LuaVM::new().unwrap();
        vm.init_stdlib().unwrap();
        
        // Set up KEYS and ARGV
        let keys_table = vm.create_table().unwrap();
        let key_str = vm.create_string("key1").unwrap();
        vm.set_table_index(keys_table, 1, Value::String(key_str)).unwrap();
        
        let argv_table = vm.create_table().unwrap();
        let arg_str = vm.create_string("arg1").unwrap();
        vm.set_table_index(argv_table, 1, Value::String(arg_str)).unwrap();
        
        let globals = vm.globals();
        let keys_name = vm.create_string("KEYS").unwrap();
        let argv_name = vm.create_string("ARGV").unwrap();
        vm.set_table(globals, Value::String(keys_name), Value::Table(keys_table)).unwrap();
        vm.set_table(globals, Value::String(argv_name), Value::Table(argv_table)).unwrap();
        
        let compiled = compiler::compile("return KEYS[1] .. ':' .. ARGV[1]").unwrap();
        let result = vm.execute_module(&compiled, &[]).unwrap();
        
        match result {
            Value::String(handle) => {
                let s = vm.heap.get_string_value(handle).unwrap();
                assert_eq!(s, "key1:arg1");
            },
            _ => panic!("Expected string result"),
        }
    }
    
    // Edge cases and error handling
    
    #[test]
    fn test_division_by_zero() {
        // In Lua, division by zero returns inf or -inf
        test_lua("return 1 / 0", "inf");
        test_lua("return -1 / 0", "-inf");
    }
    
    #[test]
    fn test_string_coercion() {
        test_lua("return '5' + 3", "8");
        test_lua("return 3 + '5'", "8");
        test_lua("return '3.14' * 2", "6.28");
    }
    
    #[test]
    fn test_nil_operations() {
        test_lua("local x; return tostring(x)", "nil");
        test_lua("local t = {}; return tostring(t.nonexistent)", "nil");
    }
    
    #[test]
    fn test_table_nil_holes() {
        test_lua("local t = {1, nil, 3}; return #t", "3"); // Lua behavior with nil holes is tricky
    }
}