//! Comprehensive Lua 5.1 Specification Test Suite
//! 
//! This suite validates the Ferrous Lua implementation against the full Lua 5.1
//! specification as required for Redis compatibility.

use std::sync::Arc;
use ferrous::error::{Result, FerrousError};
use ferrous::protocol::resp::RespFrame;
use ferrous::storage::StorageEngine;
use ferrous::storage::commands::*;

/// Test category
pub enum TestCategory {
    BasicTypes,
    Operators,
    ControlFlow,
    Functions,
    Tables,
    Metatables,
    Iterators,
    Closures,
    StandardLibrary,
    ErrorHandling,
    RedisApi,
}

/// Test result
pub struct TestResult {
    pub name: String,
    pub category: TestCategory,
    pub passed: bool,
    pub expected: String,
    pub actual: Option<String>,
    pub error: Option<String>,
}

/// Execute a Lua script and return the result
fn eval_lua(script: &str, keys: &[Vec<u8>], args: &[Vec<u8>]) -> Result<RespFrame> {
    // Create in-memory storage engine for testing
    let storage = StorageEngine::new_in_memory();
    
    // Execute script
    match handle_eval(
        &storage,
        &[
            RespFrame::from_string("EVAL"),
            RespFrame::from_string(script),
            RespFrame::Integer(keys.len() as i64),
        ]
        .iter()
        .chain(keys.iter().map(|k| RespFrame::from_bytes(k.clone())))
        .chain(args.iter().map(|a| RespFrame::from_bytes(a.clone())))
        .collect::<Vec<_>>(),
    ) {
        Ok(resp) => Ok(resp),
        Err(e) => Err(e),
    }
}

/// Convert RespFrame to string for comparison
fn resp_to_string(resp: &RespFrame) -> String {
    match resp {
        RespFrame::SimpleString(bytes) => {
            String::from_utf8_lossy(bytes).to_string()
        }
        RespFrame::Error(bytes) => {
            format!("ERR {}", String::from_utf8_lossy(bytes))
        }
        RespFrame::Integer(n) => {
            n.to_string()
        }
        RespFrame::BulkString(Some(bytes)) => {
            String::from_utf8_lossy(bytes).to_string()
        }
        RespFrame::BulkString(None) => {
            "nil".to_string()
        }
        RespFrame::Array(Some(frames)) => {
            let items: Vec<String> = frames.iter().map(resp_to_string).collect();
            format!("[{}]", items.join(", "))
        }
        RespFrame::Array(None) => {
            "nil".to_string()
        }
        RespFrame::Null => {
            "null".to_string()
        }
        RespFrame::Boolean(b) => {
            b.to_string()
        }
        RespFrame::Double(d) => {
            d.to_string()
        }
        RespFrame::Map(pairs) => {
            let items: Vec<String> = pairs
                .iter()
                .map(|(k, v)| format!("{}: {}", resp_to_string(k), resp_to_string(v)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
        RespFrame::Set(items) => {
            let items: Vec<String> = items.iter().map(resp_to_string).collect();
            format!("set({})", items.join(", "))
        }
    }
}

/// Run a Lua test and return the result
fn run_test(name: &str, category: TestCategory, script: &str, expected: &str) -> TestResult {
    match eval_lua(script, &[], &[]) {
        Ok(resp) => {
            let actual = resp_to_string(&resp);
            let passed = actual == expected;
            TestResult {
                name: name.to_string(),
                category,
                passed,
                expected: expected.to_string(),
                actual: Some(actual),
                error: None,
            }
        }
        Err(e) => {
            let error = format!("ERR {}", e);
            let passed = error == expected;
            TestResult {
                name: name.to_string(),
                category,
                passed,
                expected: expected.to_string(),
                actual: None,
                error: Some(error),
            }
        }
    }
}

/// Execute all tests in the test suite and return results
pub fn run_lua_specification_tests() -> Vec<TestResult> {
    let mut results = Vec::new();
    
    // 1. Basic Types and Literals
    results.push(run_test(
        "Nil type",
        TestCategory::BasicTypes,
        "return nil",
        "nil",
    ));
    
    results.push(run_test(
        "Boolean true",
        TestCategory::BasicTypes,
        "return true",
        "true",
    ));
    
    results.push(run_test(
        "Boolean false",
        TestCategory::BasicTypes,
        "return false",
        "false",
    ));
    
    results.push(run_test(
        "Integer number",
        TestCategory::BasicTypes,
        "return 42",
        "42",
    ));
    
    results.push(run_test(
        "Float number",
        TestCategory::BasicTypes,
        "return 3.14159",
        "3.14159",
    ));
    
    results.push(run_test(
        "String literal",
        TestCategory::BasicTypes,
        "return 'hello'",
        "hello",
    ));
    
    results.push(run_test(
        "String with escapes",
        TestCategory::BasicTypes,
        "return 'hello\\nworld'",
        "hello\nworld",
    ));
    
    results.push(run_test(
        "Type function",
        TestCategory::BasicTypes,
        "return type(nil)",
        "nil",
    ));
    
    results.push(run_test(
        "Type of string",
        TestCategory::BasicTypes,
        "return type('hello')",
        "string",
    ));
    
    results.push(run_test(
        "Type of table",
        TestCategory::BasicTypes,
        "return type({})",
        "table",
    ));
    
    // 2. Operators
    results.push(run_test(
        "Addition",
        TestCategory::Operators,
        "return 5 + 3",
        "8",
    ));
    
    results.push(run_test(
        "Subtraction",
        TestCategory::Operators,
        "return 5 - 3",
        "2",
    ));
    
    results.push(run_test(
        "Multiplication",
        TestCategory::Operators,
        "return 5 * 3",
        "15",
    ));
    
    results.push(run_test(
        "Division",
        TestCategory::Operators,
        "return 15 / 3",
        "5",
    ));
    
    results.push(run_test(
        "Modulo",
        TestCategory::Operators,
        "return 17 % 5",
        "2",
    ));
    
    results.push(run_test(
        "Power",
        TestCategory::Operators,
        "return 2 ^ 3",
        "8",
    ));
    
    results.push(run_test(
        "Unary minus",
        TestCategory::Operators,
        "return -5",
        "-5",
    ));
    
    results.push(run_test(
        "String concatenation",
        TestCategory::Operators,
        "return 'hello' .. ' ' .. 'world'",
        "hello world",
    ));
    
    results.push(run_test(
        "Number to string concat",
        TestCategory::Operators,
        "return 'The answer is ' .. 42",
        "The answer is 42",
    ));
    
    results.push(run_test(
        "Equality",
        TestCategory::Operators,
        "return 5 == 5",
        "true",
    ));
    
    results.push(run_test(
        "Inequality",
        TestCategory::Operators,
        "return 5 ~= 6",
        "true",
    ));
    
    results.push(run_test(
        "Less than",
        TestCategory::Operators,
        "return 5 < 10",
        "true",
    ));
    
    results.push(run_test(
        "Greater than",
        TestCategory::Operators,
        "return 5 > 3",
        "true",
    ));
    
    results.push(run_test(
        "Less or equal",
        TestCategory::Operators,
        "return 5 <= 5",
        "true",
    ));
    
    results.push(run_test(
        "Greater or equal",
        TestCategory::Operators,
        "return 5 >= 5",
        "true",
    ));
    
    results.push(run_test(
        "Logical and",
        TestCategory::Operators,
        "return true and true",
        "true",
    ));
    
    results.push(run_test(
        "Logical or",
        TestCategory::Operators,
        "return false or true",
        "true",
    ));
    
    results.push(run_test(
        "Logical not",
        TestCategory::Operators,
        "return not false",
        "true",
    ));
    
    results.push(run_test(
        "String length",
        TestCategory::Operators,
        "return #'hello'",
        "5",
    ));
    
    results.push(run_test(
        "Table length",
        TestCategory::Operators,
        "return #{1, 2, 3, 4, 5}",
        "5",
    ));
    
    // 3. Control Flow
    results.push(run_test(
        "Simple if",
        TestCategory::ControlFlow,
        "if true then return 'true' else return 'false' end",
        "true",
    ));
    
    results.push(run_test(
        "If-else",
        TestCategory::ControlFlow,
        "if false then return 'true' else return 'false' end",
        "false",
    ));
    
    results.push(run_test(
        "If-elseif-else",
        TestCategory::ControlFlow,
        "local x = 5; if x < 5 then return 'less' elseif x > 5 then return 'greater' else return 'equal' end",
        "equal",
    ));
    
    results.push(run_test(
        "While loop",
        TestCategory::ControlFlow,
        "local i, sum = 1, 0; while i <= 5 do sum = sum + i; i = i + 1; end; return sum",
        "15",
    ));
    
    results.push(run_test(
        "Repeat-until",
        TestCategory::ControlFlow,
        "local i, sum = 1, 0; repeat sum = sum + i; i = i + 1; until i > 5; return sum",
        "15",
    ));
    
    results.push(run_test(
        "Numeric for loop",
        TestCategory::ControlFlow,
        "local sum = 0; for i=1,5 do sum = sum + i end; return sum",
        "15",
    ));
    
    results.push(run_test(
        "Numeric for with step",
        TestCategory::ControlFlow,
        "local sum = 0; for i=1,10,2 do sum = sum + i end; return sum",
        "25",
    ));
    
    results.push(run_test(
        "Break in loop",
        TestCategory::ControlFlow,
        "local i, sum = 0, 0; while true do i = i + 1; if i > 5 then break end; sum = sum + i; end; return sum",
        "15",
    ));
    
    // 4. Functions
    results.push(run_test(
        "Simple function",
        TestCategory::Functions,
        "local function add(a, b) return a + b end; return add(3, 4)",
        "7",
    ));
    
    results.push(run_test(
        "Anonymous function",
        TestCategory::Functions,
        "local add = function(a, b) return a + b end; return add(3, 4)",
        "7",
    ));
    
    results.push(run_test(
        "Multiple returns",
        TestCategory::Functions,
        "local function foo() return 1, 2, 3 end; return {foo()}",
        "[1, 2, 3]",
    ));
    
    results.push(run_test(
        "Variable arguments",
        TestCategory::Functions,
        "local function sum(...) local s = 0; for _, v in ipairs({...}) do s = s + v end; return s end; return sum(1, 2, 3, 4, 5)",
        "15",
    ));
    
    results.push(run_test(
        "Local vs global",
        TestCategory::Functions,
        "local function outer() local x = 10; local function inner() return x end; return inner() end; return outer()",
        "10",
    ));
    
    results.push(run_test(
        "Nested functions",
        TestCategory::Functions,
        "local function factorial(n) if n <= 1 then return 1 else return n * factorial(n - 1) end end; return factorial(5)",
        "120",
    ));
    
    // 5. Tables
    results.push(run_test(
        "Empty table",
        TestCategory::Tables,
        "return {}",
        "[]",
    ));
    
    results.push(run_test(
        "Array-style table",
        TestCategory::Tables,
        "return {1, 2, 3, 4, 5}",
        "[1, 2, 3, 4, 5]",
    ));
    
    results.push(run_test(
        "Hash-style table",
        TestCategory::Tables,
        "local t = {x=1, y=2, z=3}; return t.y",
        "2",
    ));
    
    results.push(run_test(
        "Mixed table",
        TestCategory::Tables,
        "local t = {10, 20, x=30, y=40}; return t[1] .. ' ' .. t.x",
        "10 30",
    ));
    
    results.push(run_test(
        "Nested tables",
        TestCategory::Tables,
        "local t = {a={b={c=3}}}; return t.a.b.c",
        "3",
    ));
    
    results.push(run_test(
        "Table as array",
        TestCategory::Tables,
        "local t = {1, 2, 3}; local result = ''; for i=1,#t do result = result .. t[i] end; return result",
        "123",
    ));
    
    results.push(run_test(
        "Table length",
        TestCategory::Tables,
        "return #{1, 2, 3, 4, 5}",
        "5",
    ));
    
    results.push(run_test(
        "Table modification",
        TestCategory::Tables,
        "local t = {a=1, b=2}; t.a = 10; return t.a",
        "10",
    ));
    
    // 6. Metatables
    results.push(run_test(
        "__index metamethod",
        TestCategory::Metatables,
        "local t = {}; local mt = {__index = {x=5}}; setmetatable(t, mt); return t.x",
        "5",
    ));
    
    results.push(run_test(
        "__newindex metamethod",
        TestCategory::Metatables,
        "local t = {}; local log = {}; local mt = {__newindex = function(t, k, v) log[k] = v end}; setmetatable(t, mt); t.x = 10; return log.x",
        "10",
    ));
    
    results.push(run_test(
        "__add metamethod",
        TestCategory::Metatables,
        "local mt = {__add = function(a, b) return {value = a.value + b.value} end}; local a = {value = 5}; local b = {value = 3}; setmetatable(a, mt); setmetatable(b, mt); return (a + b).value",
        "8",
    ));
    
    results.push(run_test(
        "__tostring metamethod",
        TestCategory::Metatables,
        "local t = {}; local mt = {__tostring = function() return 'custom string' end}; setmetatable(t, mt); return tostring(t)",
        "custom string",
    ));
    
    // 7. Iterators
    results.push(run_test(
        "pairs() iterator",
        TestCategory::Iterators,
        "local t = {a=1, b=2, c=3}; local result = ''; for k, v in pairs(t) do result = result .. k .. v end; return result",
        "a1b2c3",
    ));
    
    results.push(run_test(
        "ipairs() iterator",
        TestCategory::Iterators,
        "local t = {10, 20, 30}; local sum = 0; for i, v in ipairs(t) do sum = sum + v end; return sum",
        "60",
    ));
    
    results.push(run_test(
        "next() function",
        TestCategory::Iterators,
        "local t = {a=1, b=2}; local k, v = next(t); local result = k .. v; k, v = next(t, k); result = result .. k .. v; return result",
        "a1b2",
    ));
    
    results.push(run_test(
        "Custom iterator",
        TestCategory::Iterators,
        "local function values(t) local i = 0; return function() i = i + 1; return t[i] end end; local t = {10, 20, 30}; local sum = 0; for v in values(t) do sum = sum + v end; return sum",
        "60",
    ));
    
    // 8. Closures and Upvalues
    results.push(run_test(
        "Simple closure",
        TestCategory::Closures,
        "local function counter() local i = 0; return function() i = i + 1; return i end end; local c = counter(); c(); c(); return c()",
        "3",
    ));
    
    results.push(run_test(
        "Multiple closures",
        TestCategory::Closures,
        "local function counter() local i = 0; return function() i = i + 1; return i end end; local c1 = counter(); local c2 = counter(); c1(); c2(); c2(); return c1() .. ' ' .. c2()",
        "2 3",
    ));
    
    results.push(run_test(
        "Upvalue modification",
        TestCategory::Closures,
        "local x = 10; local function f() x = x + 1; return x end; f(); f(); return f()",
        "13",
    ));
    
    // 9. Standard Library
    results.push(run_test(
        "assert",
        TestCategory::StandardLibrary,
        "return assert(true, 'should not error')",
        "true",
    ));
    
    results.push(run_test(
        "type",
        TestCategory::StandardLibrary,
        "return type('hello')",
        "string",
    ));
    
    results.push(run_test(
        "tonumber",
        TestCategory::StandardLibrary,
        "return tonumber('42')",
        "42",
    ));
    
    results.push(run_test(
        "tostring",
        TestCategory::StandardLibrary,
        "return tostring(42)",
        "42",
    ));
    
    results.push(run_test(
        "pcall success",
        TestCategory::StandardLibrary,
        "local status, result = pcall(function() return 'success' end); return status, result",
        "true success",
    ));
    
    results.push(run_test(
        "string.len",
        TestCategory::StandardLibrary,
        "return string.len('hello')",
        "5",
    ));
    
    results.push(run_test(
        "string.sub",
        TestCategory::StandardLibrary,
        "return string.sub('hello', 2, 4)",
        "ell",
    ));
    
    results.push(run_test(
        "string.upper",
        TestCategory::StandardLibrary,
        "return string.upper('hello')",
        "HELLO",
    ));
    
    results.push(run_test(
        "string.lower",
        TestCategory::StandardLibrary,
        "return string.lower('HELLO')",
        "hello",
    ));
    
    results.push(run_test(
        "string.find",
        TestCategory::StandardLibrary,
        "local s, e = string.find('hello world', 'world'); return s",
        "7",
    ));
    
    results.push(run_test(
        "string.gsub",
        TestCategory::StandardLibrary,
        "return string.gsub('hello world', 'world', 'lua')",
        "hello lua",
    ));
    
    results.push(run_test(
        "table.insert",
        TestCategory::StandardLibrary,
        "local t = {1, 2, 3}; table.insert(t, 4); return t[4]",
        "4",
    ));
    
    results.push(run_test(
        "table.remove",
        TestCategory::StandardLibrary,
        "local t = {1, 2, 3, 4}; local removed = table.remove(t, 2); return removed, t[2]",
        "2 3",
    ));
    
    results.push(run_test(
        "table.concat",
        TestCategory::StandardLibrary,
        "return table.concat({'a', 'b', 'c'}, '-')",
        "a-b-c",
    ));
    
    results.push(run_test(
        "table.sort",
        TestCategory::StandardLibrary,
        "local t = {3, 1, 4, 2}; table.sort(t); return table.concat(t, '')",
        "1234",
    ));
    
    results.push(run_test(
        "math.abs",
        TestCategory::StandardLibrary,
        "return math.abs(-5)",
        "5",
    ));
    
    results.push(run_test(
        "math.floor",
        TestCategory::StandardLibrary,
        "return math.floor(3.7)",
        "3",
    ));
    
    results.push(run_test(
        "math.ceil",
        TestCategory::StandardLibrary,
        "return math.ceil(3.2)",
        "4",
    ));
    
    results.push(run_test(
        "math.max",
        TestCategory::StandardLibrary,
        "return math.max(1, 3, 2, 5, 4)",
        "5",
    ));
    
    results.push(run_test(
        "math.min",
        TestCategory::StandardLibrary,
        "return math.min(1, 3, 2, 5, 4)",
        "1",
    ));
    
    // 10. Error Handling
    results.push(run_test(
        "pcall with error",
        TestCategory::ErrorHandling,
        "local status, err = pcall(function() error('test error') end); return status, string.match(err, 'test error') ~= nil",
        "false true",
    ));
    
    results.push(run_test(
        "Protected arithmetic",
        TestCategory::ErrorHandling,
        "local status = pcall(function() return 1/0 end); return status",
        "false",
    ));
    
    // 11. Redis API
    results.push(run_test(
        "KEYS table exists",
        TestCategory::RedisApi,
        "return type(KEYS)",
        "table",
    ));
    
    results.push(run_test(
        "ARGV table exists",
        TestCategory::RedisApi,
        "return type(ARGV)",
        "table",
    ));
    
    results.push(run_test(
        "redis.call SET/GET",
        TestCategory::RedisApi,
        "redis.call('SET', 'mykey', 'myvalue'); return redis.call('GET', 'mykey')",
        "myvalue",
    ));
    
    results.push(run_test(
        "redis.pcall",
        TestCategory::RedisApi,
        "local status = redis.pcall('HSET', 'myhash', 'field', 'value'); return redis.pcall('HGET', 'myhash', 'field')",
        "value",
    ));
    
    results.push(run_test(
        "cjson.encode",
        TestCategory::RedisApi,
        "return cjson.encode({a=1, b=2})",
        "{\"a\":1,\"b\":2}",
    ));
    
    results.push(run_test(
        "cjson.decode",
        TestCategory::RedisApi,
        "local t = cjson.decode('{\"a\":1,\"b\":2}'); return t.a + t.b",
        "3",
    ));
    
    results
}

/// Run the test suite and print results
pub fn main() {
    println!("=== Lua 5.1 Specification Test Suite for Ferrous ===\n");
    
    let results = run_lua_specification_tests();
    
    let mut passed_count = 0;
    let mut failed_tests_by_category = std::collections::HashMap::new();
    
    // Print results by category
    for category in [
        TestCategory::BasicTypes,
        TestCategory::Operators,
        TestCategory::ControlFlow,
        TestCategory::Functions,
        TestCategory::Tables,
        TestCategory::Metatables,
        TestCategory::Iterators,
        TestCategory::Closures,
        TestCategory::StandardLibrary,
        TestCategory::ErrorHandling,
        TestCategory::RedisApi,
    ] {
        let category_name = match category {
            TestCategory::BasicTypes => "Basic Types and Literals",
            TestCategory::Operators => "Operators",
            TestCategory::ControlFlow => "Control Flow",
            TestCategory::Functions => "Functions",
            TestCategory::Tables => "Tables",
            TestCategory::Metatables => "Metatables",
            TestCategory::Iterators => "Iterators and Generic For",
            TestCategory::Closures => "Closures and Upvalues",
            TestCategory::StandardLibrary => "Standard Library",
            TestCategory::ErrorHandling => "Error Handling",
            TestCategory::RedisApi => "Redis API",
        };
        
        println!("\n{}\n----------------------------------------", category_name);
        
        for result in &results {
            if std::mem::discriminant(&result.category) == std::mem::discriminant(&category) {
                if result.passed {
                    println!("  ✓ {}", result.name);
                    passed_count += 1;
                } else {
                    let error_msg = match &result.error {
                        Some(err) => err.clone(),
                        None => match &result.actual {
                            Some(actual) => actual.clone(),
                            None => "Unknown error".to_string(),
                        },
                    };
                    
                    println!("  ✗ {}: {}", result.name, error_msg);
                    
                    failed_tests_by_category
                        .entry(std::mem::discriminant(&category))
                        .or_insert_with(Vec::new)
                        .push((result.name.clone(), error_msg));
                }
            }
        }
    }
    
    // Print summary
    println!("\n============================================================");
    println!("SUMMARY");
    println!("============================================================\n");
    println!("Total Tests: {}", results.len());
    println!("Passed: {} ({}%)", passed_count, (passed_count * 100) / results.len());
    println!("Failed: {} ({}%)\n", results.len() - passed_count, ((results.len() - passed_count) * 100) / results.len());
    
    println!("Results by Category:\n");
    
    for category in [
        TestCategory::BasicTypes,
        TestCategory::Operators,
        TestCategory::ControlFlow,
        TestCategory::Functions,
        TestCategory::Tables,
        TestCategory::Metatables,
        TestCategory::Iterators,
        TestCategory::Closures,
        TestCategory::StandardLibrary,
        TestCategory::ErrorHandling,
        TestCategory::RedisApi,
    ] {
        let category_name = match category {
            TestCategory::BasicTypes => "basic_types",
            TestCategory::Operators => "operators",
            TestCategory::ControlFlow => "control_flow",
            TestCategory::Functions => "functions",
            TestCategory::Tables => "tables",
            TestCategory::Metatables => "metatables",
            TestCategory::Iterators => "iterators",
            TestCategory::Closures => "closures",
            TestCategory::StandardLibrary => "stdlib",
            TestCategory::ErrorHandling => "errors",
            TestCategory::RedisApi => "redis_api",
        };
        
        let passed_in_category = results
            .iter()
            .filter(|r| std::mem::discriminant(&r.category) == std::mem::discriminant(&category) && r.passed)
            .count();
            
        let total_in_category = results
            .iter()
            .filter(|r| std::mem::discriminant(&r.category) == std::mem::discriminant(&category))
            .count();
            
        println!("{}:", category_name);
        println!("  Passed: {}", passed_in_category);
        println!("  Failed: {}", total_in_category - passed_in_category);
        
        if let Some(failed) = failed_tests_by_category.get(&std::mem::discriminant(&category)) {
            println!("  Failed tests:");
            for (name, error) in failed {
                println!("    - {}: {}", name, error);
            }
        }
        
        println!();
    }
}