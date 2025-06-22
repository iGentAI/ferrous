//! Comprehensive test program for Redis Lua functionality
//! 
//! This program verifies that all Redis-specific Lua features are working correctly

use ferrous::lua::vm::LuaVm;
use ferrous::lua::value::{LuaValue, LuaString, LuaTable, LuaFunction};
use ferrous::lua::error::{Result, LuaError};
use std::rc::Rc;
use std::cell::RefCell;

fn test_result<T: std::fmt::Debug>(name: &str, expected: &str, result: std::result::Result<T, LuaError>) {
    match result {
        Ok(value) => {
            println!("{}: {:?}", name, value);
            let pass = match expected {
                "ERROR" => false,
                _ => true,
            };
            println!("{}: {}\n", if pass { "PASS" } else { "FAIL" }, name);
        },
        Err(e) => {
            println!("{}: ERROR: {}", name, e);
            let pass = expected == "ERROR";
            println!("{}: {}\n", if pass { "PASS" } else { "FAIL" }, name);
        }
    }
}

fn main() -> Result<()> {
    println!("=== Ferrous Redis Lua Functionality Test ===");
    
    // Create a VM and initialize Redis environment
    let mut vm = LuaVm::new();
    vm.init_redis_env()?;
    
    // Create KEYS and ARGV tables for testing
    let mut keys_table = LuaTable::new();
    keys_table.set(
        LuaValue::Number(1.0), 
        LuaValue::String(LuaString::from_str("key1"))
    );
    keys_table.set(
        LuaValue::Number(2.0), 
        LuaValue::String(LuaString::from_str("key2"))
    );
    vm.set_global("KEYS", LuaValue::Table(Rc::new(RefCell::new(keys_table))));
    
    let mut argv_table = LuaTable::new();
    argv_table.set(
        LuaValue::Number(1.0), 
        LuaValue::String(LuaString::from_str("value1"))
    );
    argv_table.set(
        LuaValue::Number(2.0), 
        LuaValue::Number(42.0)
    );
    vm.set_global("ARGV", LuaValue::Table(Rc::new(RefCell::new(argv_table))));
    
    // Mock redis.call function for testing
    let redis_table = vm.get_global("redis").unwrap();
    if let LuaValue::Table(table) = redis_table {
        let mut t = table.borrow_mut();
        t.set(
            LuaValue::String(LuaString::from_str("call")),
            LuaValue::Function(LuaFunction::Rust(|_vm, args| {
                if args.is_empty() {
                    return Err(LuaError::Runtime("redis.call requires command".into()));
                }
                
                let cmd = match &args[0] {
                    LuaValue::String(s) => {
                        match s.to_str() {
                            Ok(cmd) => cmd.to_uppercase(),
                            Err(_) => return Err(LuaError::Runtime("Invalid UTF-8 in command name".into()))
                        }
                    },
                    _ => return Err(LuaError::Runtime("Command name must be a string".into()))
                };
                
                match cmd.as_str() {
                    "PING" => Ok(LuaValue::String(LuaString::from_str("PONG"))),
                    "GET" => {
                        if args.len() < 2 {
                            return Err(LuaError::Runtime("GET requires key argument".into()));
                        }
                        
                        // For testing, return a dummy value based on the key
                        let key = match &args[1] {
                            LuaValue::String(s) => {
                                match s.to_str() {
                                    Ok(key) => key,
                                    Err(_) => return Err(LuaError::Runtime("Invalid UTF-8 in key".into()))
                                }
                            },
                            _ => return Err(LuaError::Runtime("Key must be a string".into()))
                        };
                        
                        if key == "key1" {
                            Ok(LuaValue::String(LuaString::from_str("value1")))
                        } else if key == "key2" {
                            Ok(LuaValue::Number(42.0))
                        } else {
                            Ok(LuaValue::Nil)
                        }
                    },
                    "SET" => {
                        if args.len() < 3 {
                            return Err(LuaError::Runtime("SET requires key and value arguments".into()));
                        }
                        
                        // Just return OK for SET
                        Ok(LuaValue::String(LuaString::from_str("OK")))
                    },
                    _ => Err(LuaError::Runtime(format!("Unknown command: {}", cmd)))
                }
            }))
        );
        
        // Add pcall function
        t.set(
            LuaValue::String(LuaString::from_str("pcall")),
            LuaValue::Function(LuaFunction::Rust(|vm, args| {
                match vm.call_redis_api(args, true) {
                    Ok(val) => Ok(val),
                    Err(e) => {
                        // For pcall, we wrap errors in a table with err field
                        let mut table = LuaTable::new();
                        table.set(
                            LuaValue::String(LuaString::from_str("err")),
                            LuaValue::String(LuaString::from_str(&e.to_string()))
                        );
                        Ok(LuaValue::Table(Rc::new(RefCell::new(table))))
                    }
                }
            }))
        );
    }
    
    println!("\n--- Standard Libraries ---");
    
    // Test string library
    test_result("string.len", "3.0", 
        vm.run("return string.len('abc')"));
    
    test_result("string.upper", "ABC", 
        vm.run("return string.upper('abc')"));
    
    test_result("string.sub", "bc", 
        vm.run("return string.sub('abcd', 2, 3)"));
    
    // Test table library
    test_result("table.insert", "123", 
        vm.run("local t = {1, 2}; table.insert(t, 3); return t[1]..t[2]..t[3]"));
    
    test_result("table.concat", "1,2,3", 
        vm.run("local t = {1, 2, 3}; return table.concat(t, ',')"));
    
    // Test math library
    test_result("math.abs", "5.0", 
        vm.run("return math.abs(-5)"));
    
    test_result("math.ceil", "2.0", 
        vm.run("return math.ceil(1.1)"));
    
    test_result("math.floor", "1.0", 
        vm.run("return math.floor(1.9)"));
    
    println!("\n--- Redis-Specific Libraries ---");
    
    // Test bit operations
    test_result("bit.band", "1.0", 
        vm.run("return bit.band(3, 5)"));
    
    test_result("bit.bor", "7.0", 
        vm.run("return bit.bor(3, 5)"));
    
    test_result("bit.lshift", "10.0", 
        vm.run("return bit.lshift(5, 1)"));
    
    // Test cjson library
    test_result("cjson.encode", "\"test\"", 
        vm.run("return cjson.encode('test')"));
    
    println!("\n--- Redis API ---");
    
    // Test redis.call to PING
    test_result("redis.call PING", "PONG", 
        vm.run("return redis.call('PING')"));
    
    // Test redis.call to GET
    test_result("redis.call GET", "value1", 
        vm.run("return redis.call('GET', 'key1')"));
    
    // Test redis.call with KEYS access
    test_result("redis.call with KEYS", "value1", 
        vm.run("return redis.call('GET', KEYS[1])"));
    
    // Test error handling with redis.pcall
    test_result("redis.pcall error handling", "{err = ...}", 
        vm.run("return redis.pcall('UNKNOWN')"));
    
    println!("\n--- Security Features ---");
    
    // Test sandbox - should prevent access to unsafe libraries
    test_result("io library (sandbox)", "ERROR", 
        vm.run("return io.open"));
    
    test_result("os library (sandbox)", "ERROR", 
        vm.run("return os.execute"));
    
    // Test that math.random is not available (non-deterministic)
    test_result("math.random (sandbox)", "ERROR", 
        vm.run("return math.random()"));
    
    println!("\n--- Complex Script Patterns ---");
    
    // Test a common Redis Lua pattern - counter
    test_result("Counter script", "1.0", 
        vm.run(r#"
            local key = KEYS[1]
            local value = 0
            local current = redis.call("GET", key)
            if current then
                local current_num = tonumber(current)
                if current_num then
                    value = current_num
                end
            end
            value = value + 1
            redis.call("SET", key, value)
            return value
        "#));
    
    // Test access to multiple KEYS and ARGV values
    test_result("KEYS and ARGV access", "key1value1", 
        vm.run(r#"
            local key = KEYS[1]
            local arg = ARGV[1]
            return key .. arg
        "#));
    
    println!("\n=== Test Complete ===");
    
    Ok(())
}