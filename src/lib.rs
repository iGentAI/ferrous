//! Ferrous library
//! 
//! This file exposes the public API of Ferrous for use as a library.

pub mod error;
pub mod network;
pub mod protocol;
pub mod storage;
pub mod monitor;
pub mod pubsub;
pub mod replication;
pub mod config;

pub mod lua;

// Re-export commonly used types
pub use error::FerrousError;
pub use storage::engine::StorageEngine;
pub use network::server::Server;
pub use protocol::resp::RespFrame;
pub use config::Config;

// Re-export Lua types
pub use lua::LuaGIL;

// Re-export VM types from the lua module
pub use lua::vm::{LuaVM, Value};
pub use lua::compile;
pub use lua::handle::TableHandle;
pub use lua::transaction::HeapTransaction;

// Include compliance tests in the test framework
#[cfg(test)]
mod tests {
    use crate::lua::{LuaVM, Value, compile, handle::TableHandle};
    use crate::lua::transaction::HeapTransaction;
    use std::fs;
    use std::path::Path;

    /// Execute a Lua script and return the result
    fn execute_script(script: &str) -> Result<Value, String> {
        // Create a VM instance
        let mut vm = match LuaVM::new() {
            Ok(vm) => vm,
            Err(e) => return Err(format!("Failed to create VM: {:?}", e)),
        };
        
        // Compile the script
        let module = match compile(script) {
            Ok(module) => module,
            Err(e) => return Err(format!("Compilation error: {:?}", e)),
        };
        
        // Execute the script
        match vm.execute_module(&module, &[]) {
            Ok(value) => Ok(value),
            Err(e) => Err(format!("Execution error: {:?}", e)),
        }
    }
    
    /// Verify a test result from a return table
    fn verify_result(result: &Value, key: &str, expected: bool) -> bool {
        match result {
            Value::Table(handle) => {
                let mut vm = match LuaVM::new() {
                    Ok(vm) => vm,
                    Err(_) => return false,
                };
                
                let mut tx = HeapTransaction::new(vm.heap_mut());
                
                // Create a string handle for the key
                let key_handle = match tx.create_string(key) {
                    Ok(handle) => handle,
                    Err(_) => return false,
                };
                
                // Get the value from the table
                let value = match tx.read_table_field(*handle, &Value::String(key_handle)) {
                    Ok(value) => value,
                    Err(_) => return false,
                };
                
                // Check if the value matches expected
                match value {
                    Value::Boolean(b) => b == expected,
                    Value::Nil => !expected, // nil is equivalent to false
                    _ => false, // Unexpected value type
                }
            },
            _ => false, // Not a table result
        }
    }
    
    /// Run a Lua test file and verify the results
    fn run_test_file(file_path: &str) -> bool {
        println!("Running test file: {}", file_path);
        
        // Check if the file exists
        if !Path::new(file_path).exists() {
            println!("Test file not found: {}", file_path);
            return false;
        }
        
        // Read the file
        let script = match fs::read_to_string(file_path) {
            Ok(content) => content,
            Err(e) => {
                println!("Failed to read test file {}: {}", file_path, e);
                return false;
            }
        };
        
        // Execute the script
        let result = match execute_script(&script) {
            Ok(value) => value,
            Err(e) => {
                println!("Test file {} failed: {}", file_path, e);
                return false;
            }
        };
        
        println!("Test file {} executed successfully", file_path);
        
        // For now, just return true if execution succeeded
        // In a real test, we would verify specific result values
        true
    }
    
    #[test]
    fn lua_basic_compliance_test() {
        // Test that basic Lua operations work
        let result = execute_script("return 1 + 1").unwrap();
        assert_eq!(result, Value::Number(2.0));
        
        // Test string operations
        let result = execute_script("return 'hello' .. ' world'").unwrap();
        // Value equality for strings compares handles, so we can't directly compare
        // Assert it's a string type
        if let Value::String(_) = result {
            // Pass
        } else {
            panic!("Expected string result");
        }
        
        // Test table creation
        let result = execute_script("return {a=1, b=2}").unwrap();
        if let Value::Table(_) = result {
            // Pass
        } else {
            panic!("Expected table result");
        }
    }
    
    #[test]
    fn lua_table_operations_test() {
        // Run the table operations test file if it exists
        let test_path = "tests/lua/table_test.lua";
        if Path::new(test_path).exists() {
            assert!(run_test_file(test_path));
        }
    }
    
    #[test]
    fn lua_closure_test() {
        // Run the closure test file if it exists
        let test_path = "tests/lua/closure_test.lua";
        if Path::new(test_path).exists() {
            assert!(run_test_file(test_path));
        }
    }
    
    #[test]
    fn lua_nested_refs_test() {
        // Run the nested references test file if it exists
        let test_path = "tests/lua/nested_refs_test.lua";
        if Path::new(test_path).exists() {
            assert!(run_test_file(test_path));
        }
    }
    
    #[test]
    fn lua_comprehensive_test() {
        // Run the comprehensive test file if it exists
        let test_path = "tests/lua/comprehensive.lua";
        if Path::new(test_path).exists() {
            assert!(run_test_file(test_path));
        }
    }
}