//! Test program for the Lua parser and compiler
//! 
//! This program tests if we can successfully parse and compile a Lua script.

use ferrous::lua::Parser;
use ferrous::lua::compiler::Compiler;

fn main() {
    println!("--- Ferrous Lua Parser and Compiler Test ---\n");
    
    let input = r#"
        -- A simple Lua script to test parsing and compilation
        local x = 10
        local y = 20
        
        local function add(a, b) 
            return a + b
        end
        
        local result = add(x, y)
        
        if result > 25 then
            return "big result: " .. result
        else
            return "small result: " .. result
        end
    "#;
    
    println!("Test script:");
    println!("-----------");
    println!("{}", input);
    println!("-----------\n");
    
    println!("Parsing...");
    
    // Parse the script
    let mut parser = match Parser::new(input) {
        Ok(parser) => {
            println!("✅ Created parser successfully");
            parser
        },
        Err(e) => {
            println!("❌ Failed to create parser: {}", e);
            std::process::exit(1);
        }
    };
    
    // Parse into an AST
    let chunk = match parser.parse() {
        Ok(chunk) => {
            println!("✅ Parsed script into AST successfully");
            chunk
        },
        Err(e) => {
            println!("❌ Failed to parse script: {}", e);
            std::process::exit(1);
        }
    };
    
    println!("\nCompiling...");
    
    // Compile into bytecode
    let mut compiler = Compiler::new();
    match compiler.compile_chunk(&chunk) {
        Ok(proto) => {
            println!("✅ Compiled script successfully");
            println!("  - Code size: {} instructions", proto.code.len());
            println!("  - Constants: {} values", proto.constants.len());
            println!("  - Max stack: {} slots", proto.max_stack_size);
        },
        Err(e) => {
            println!("❌ Failed to compile script: {}", e);
            std::process::exit(1);
        }
    }
    
    println!("\n✨ All tests passed!");
}