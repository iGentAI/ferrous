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
pub use lua::RcVM as LuaVM;
pub use lua::rc_heap::RcHeap as LuaHeap;
pub use lua::Value;
pub use lua::compile;
pub use lua::handle::TableHandle;

// Include compliance tests in the test framework
#[cfg(test)]
mod tests {
    use crate::lua::{RcVM as LuaVM, Value, compile};
    use std::fs;
    use std::path::Path;

    /// Execute a Lua script and return the result
    fn execute_script(script: &str) -> Result<lua::rc_value::Value, String> {
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
    
    #[test]
    fn lua_basic_compliance_test() {
        // Test that basic Lua operations work
        let result = execute_script("return 1 + 1").unwrap();
        assert_eq!(result, lua::rc_value::Value::Number(2.0));
    }
}