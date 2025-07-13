// Test runner for TFORLOOP implementation verification

use std::path::Path;
use std::fs;

use ferrous::lua::vm::LuaVM;
use ferrous::lua::compiler::compile;
use ferrous::lua::error::LuaResult;

fn main() -> LuaResult<()> {
    println!("Running TFORLOOP tests...");
    
    // Initialize VM with fixed implementation
    let mut vm = LuaVM::new()?;
    
    // Initialize standard library to ensure pairs() and ipairs() are available
    vm.init_stdlib()?;
    
    // Load and run the test script
    let script_path = Path::new("tests/lua/test_tforloop_fix.lua");
    let source = fs::read_to_string(script_path).expect("Could not read test script");
    
    println!("Compiling test script...");
    let module = compile(&source)?;
    
    println!("Running test script...");
    let result = vm.execute_module(&module, &[])?;
    
    println!("Tests completed successfully!");
    println!("Result: {:?}", result);
    
    Ok(())
}