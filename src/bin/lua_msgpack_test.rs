//! Test program for MessagePack encoding and decoding
//! 
//! This program tests the implementation of cmsgpack.pack and cmsgpack.unpack functions.

use ferrous::lua::vm::LuaVm;
use ferrous::lua::value::{LuaValue, LuaString, LuaTable};
use ferrous::lua::error::Result;
use std::rc::Rc;
use std::cell::RefCell;

fn test_result<T: std::fmt::Debug>(name: &str, expected: &str, result: std::result::Result<T, ferrous::lua::error::LuaError>) {
    match result {
        Ok(value) => {
            println!("{}: {:?}", name, value);
            println!("PASS: {}\n", name);
        },
        Err(e) => {
            println!("{}: ERROR: {}", name, e);
            if expected.contains("ERROR") {
                // If we expected an error, this is a pass
                println!("PASS: {}\n", name);
            } else {
                println!("FAIL: {}\n", name);
            }
        }
    }
}

fn main() -> Result<()> {
    println!("=== Ferrous Lua MessagePack Test ===");
    
    // Create a VM
    let mut vm = LuaVm::new();
    
    // Initialize Redis environment to get cmsgpack library
    vm.init_redis_env()?;
    
    println!("\n## Basic MessagePack Encoding Tests ##");
    
    // Test 1: Pack and unpack a string
    let script = r#"
        local original = "hello"
        local packed = cmsgpack.pack(original)
        local unpacked = cmsgpack.unpack(packed)
        return original == unpacked
    "#;
    
    match vm.run(script) {
        Ok(result) => {
            println!("String pack/unpack test: {:?}", result);
            if let LuaValue::Boolean(b) = result {
                println!("{}: String pack/unpack test\n", if b { "PASS" } else { "FAIL" });
            } else {
                println!("FAIL: String pack/unpack test - Expected boolean\n");
            }
        },
        Err(e) => println!("ERROR: {}\n", e),
    }
    
    // Test 2: Pack and unpack a number
    let script = r#"
        local original = 42
        local packed = cmsgpack.pack(original)
        local unpacked = cmsgpack.unpack(packed)
        return original == unpacked
    "#;
    
    match vm.run(script) {
        Ok(result) => {
            println!("Number pack/unpack test: {:?}", result);
            if let LuaValue::Boolean(b) = result {
                println!("{}: Number pack/unpack test\n", if b { "PASS" } else { "FAIL" });
            } else {
                println!("FAIL: Number pack/unpack test - Expected boolean\n");
            }
        },
        Err(e) => println!("ERROR: {}\n", e),
    }
    
    // Test 3: Pack and unpack a boolean
    let script = r#"
        local original = true
        local packed = cmsgpack.pack(original)
        local unpacked = cmsgpack.unpack(packed)
        return original == unpacked
    "#;
    
    match vm.run(script) {
        Ok(result) => {
            println!("Boolean pack/unpack test: {:?}", result);
            if let LuaValue::Boolean(b) = result {
                println!("{}: Boolean pack/unpack test\n", if b { "PASS" } else { "FAIL" });
            } else {
                println!("FAIL: Boolean pack/unpack test - Expected boolean\n");
            }
        },
        Err(e) => println!("ERROR: {}\n", e),
    }
    
    // Test 4: Pack and unpack nil
    let script = r#"
        local original = nil
        local packed = cmsgpack.pack(original)
        local unpacked = cmsgpack.unpack(packed)
        return original == unpacked
    "#;
    
    match vm.run(script) {
        Ok(result) => {
            println!("Nil pack/unpack test: {:?}", result);
            if let LuaValue::Boolean(b) = result {
                println!("{}: Nil pack/unpack test\n", if b { "PASS" } else { "FAIL" });
            } else {
                println!("FAIL: Nil pack/unpack test - Expected boolean\n");
            }
        },
        Err(e) => println!("ERROR: {}\n", e),
    }
    
    println!("\n## Array MessagePack Tests ##");
    
    // Test 5: Pack and unpack a simple array
    let script = r#"
        local original = {1, 2, 3}
        local packed = cmsgpack.pack(original)
        local unpacked = cmsgpack.unpack(packed)
        return unpacked[1] == 1 and unpacked[2] == 2 and unpacked[3] == 3
    "#;
    
    match vm.run(script) {
        Ok(result) => {
            println!("Simple array pack/unpack test: {:?}", result);
            if let LuaValue::Boolean(b) = result {
                println!("{}: Simple array pack/unpack test\n", if b { "PASS" } else { "FAIL" });
            } else {
                println!("FAIL: Simple array pack/unpack test - Expected boolean\n");
            }
        },
        Err(e) => println!("ERROR: {}\n", e),
    }
    
    // Test 6: Pack and unpack a mixed array
    let script = r#"
        local original = {1, "two", true}
        local packed = cmsgpack.pack(original)
        local unpacked = cmsgpack.unpack(packed)
        return unpacked[1] == 1 and unpacked[2] == "two" and unpacked[3] == true
    "#;
    
    match vm.run(script) {
        Ok(result) => {
            println!("Mixed array pack/unpack test: {:?}", result);
            if let LuaValue::Boolean(b) = result {
                println!("{}: Mixed array pack/unpack test\n", if b { "PASS" } else { "FAIL" });
            } else {
                println!("FAIL: Mixed array pack/unpack test - Expected boolean\n");
            }
        },
        Err(e) => println!("ERROR: {}\n", e),
    }
    
    println!("\n## Map MessagePack Tests ##");
    
    // Test 7: Pack and unpack a simple map
    let script = r#"
        local original = {name = "John", age = 42}
        local packed = cmsgpack.pack(original)
        local unpacked = cmsgpack.unpack(packed)
        return type(unpacked) == "table"
    "#;
    
    match vm.run(script) {
        Ok(result) => {
            println!("Simple map pack/unpack test: {:?}", result);
            if let LuaValue::Boolean(b) = result {
                println!("{}: Simple map pack/unpack test\n", if b { "PASS" } else { "FAIL" });
            } else {
                println!("FAIL: Simple map pack/unpack test - Expected boolean\n");
            }
        },
        Err(e) => println!("ERROR: {}\n", e),
    }
    
    println!("\n## Edge Cases ##");
    
    // Test 8: Ensure empty string packs/unpacks correctly
    let script = r#"
        local original = ""
        local packed = cmsgpack.pack(original)
        local unpacked = cmsgpack.unpack(packed)
        return original == unpacked
    "#;
    
    match vm.run(script) {
        Ok(result) => {
            println!("Empty string pack/unpack test: {:?}", result);
            if let LuaValue::Boolean(b) = result {
                println!("{}: Empty string pack/unpack test\n", if b { "PASS" } else { "FAIL" });
            } else {
                println!("FAIL: Empty string pack/unpack test - Expected boolean\n");
            }
        },
        Err(e) => println!("ERROR: {}\n", e),
    }
    
    // Test 9: Ensure empty table packs/unpacks correctly
    let script = r#"
        local original = {}
        local packed = cmsgpack.pack(original)
        local unpacked = cmsgpack.unpack(packed)
        return type(unpacked) == "table" and next(unpacked) == nil
    "#;
    
    match vm.run(script) {
        Ok(result) => {
            println!("Empty table pack/unpack test: {:?}", result);
            if let LuaValue::Boolean(b) = result {
                println!("{}: Empty table pack/unpack test\n", if b { "PASS" } else { "FAIL" });
            } else {
                println!("FAIL: Empty table pack/unpack test - Expected boolean\n");
            }
        },
        Err(e) => println!("ERROR: {}\n", e),
    }
    
    println!("=== Tests Complete ===");
    
    Ok(())
}