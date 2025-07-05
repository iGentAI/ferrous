//! Lua Code Compilation and Execution Pipeline Test
//!
//! This binary demonstrates the full pipeline from Lua source code
//! to compiled bytecode and execution. It shows detailed debug output
//! for each stage to aid in understanding and debugging the VM.

use std::env;
use std::fs;
use std::io::{self, Write};
use std::time::Instant;

use ferrous::lua::error::LuaResult;
use ferrous::lua::value::Value;
use ferrous::lua::vm::LuaVM;
use ferrous::lua::compiler::compile;
use ferrous::lua::stdlib::init_stdlib;

// Format a Lua value for printing
fn format_value(value: &Value) -> String {
    match value {
        Value::Nil => "nil".to_string(),
        Value::Boolean(b) => format!("{}", b),
        Value::Number(n) => format!("{}", n),
        Value::String(_) => {
            // For now, just show that it's a string
            // Getting actual string value would require VM access
            "<string>".to_string()
        },
        Value::Table(_) => "<table>".to_string(),
        Value::Closure(_) => "<function>".to_string(),
        Value::Thread(_) => "<thread>".to_string(),
        Value::CFunction(_) => "<C function>".to_string(),
        Value::UserData(_) => "<userdata>".to_string(),
        Value::FunctionProto(_) => "<proto>".to_string(),
    }
}

fn print_simple_bytecode(module: &ferrous::lua::compiler::CompiledModule) {
    println!("\n=== Bytecode Instructions ===");
    
    for (i, instr) in module.bytecode.iter().enumerate() {
        // Extract opcode and operands
        let raw = *instr;
        let opcode = raw & 0x3F;
        let a = ((raw >> 6) & 0xFF) as u8;
        let c = ((raw >> 14) & 0x1FF) as u16;
        let b = ((raw >> 23) & 0x1FF) as u16;
        let bx = ((raw >> 14) & 0x3FFFF) as u32;
        
        // Determine opcode name
        let opcode_name = match opcode {
            0 => "Move",
            1 => "LoadK",
            2 => "LoadBool",
            3 => "LoadNil",
            4 => "GetUpval",
            5 => "GetGlobal",
            6 => "SetGlobal",
            7 => "SetUpval",
            8 => "GetTable",
            9 => "SetTable",
            10 => "NewTable",
            11 => "Self",
            12 => "Add",
            13 => "Sub",
            14 => "Mul",
            15 => "Div",
            16 => "Mod",
            17 => "Pow",
            18 => "Unm",
            19 => "Not",
            20 => "Len",
            21 => "Concat",
            22 => "Jmp",
            23 => "Eq",
            24 => "Lt",
            25 => "Le",
            26 => "Test",
            27 => "TestSet",
            28 => "Call",
            29 => "TailCall",
            30 => "Return",
            31 => "ForPrep",
            32 => "ForLoop",
            33 => "TForLoop",
            34 => "SetList",
            35 => "VarArg",
            36 => "Closure",
            37 => "Close",
            38 => "ExtraArg",
            _ => "Unknown",
        };
        
        // Format based on opcode type
        let instr_str = match opcode {
            1 => { // LoadK
                format!("{:<10} R({}) K({})", opcode_name, a, bx)
            },
            5 => { // GetGlobal
                let const_name = if bx < module.constants.len() as u32 {
                    match &module.constants[bx as usize] {
                        ferrous::lua::codegen::CompilationConstant::String(idx) => {
                            if *idx < module.strings.len() {
                                format!("\"{}\"", module.strings[*idx])
                            } else {
                                "<invalid>".to_string()
                            }
                        },
                        _ => "<not string>".to_string()
                    }
                } else {
                    "<invalid>".to_string()
                };
                
                format!("{:<10} R({}) K({}) ; {}", opcode_name, a, bx, const_name)
            },
            21 => { // Concat
                format!("{:<10} R({}) R({}) R({}) ; R({}) = R({})..R({})", 
                        opcode_name, a, b, c, a, b, c)
            },
            28 => { // Call
                format!("{:<10} R({}) {} {} ; call", opcode_name, a, b, c)
            },
            _ => format!("{:<10} {} {} {}", opcode_name, a, b, c)
        };
        
        println!("{:>4}: {}", i, instr_str);
    }
    
    println!("\n=== End Bytecode ===\n");
}

// Run a Lua script with full debug output  
fn run_with_debug(script: &str) -> LuaResult<()> {
    println!("\n=== Running Script ===");
    println!("{}", script);
    
    // Measure compilation time
    let compile_start = Instant::now();
    
    // Compile the script
    let module = compile(script)?;
    
    let compile_duration = compile_start.elapsed();
    println!("\nCompilation completed in {:?}", compile_duration);
    
    // Print debug info
    println!("\nCompiled Module Info:");
    println!("  Bytecode instructions: {}", module.bytecode.len());
    println!("  Constants: {}", module.constants.len());
    println!("  String table size: {}", module.strings.len());
    println!("  Function prototypes: {}", module.prototypes.len());
    
    print_simple_bytecode(&module);
    
    // Create VM
    let mut vm = LuaVM::new()?;
    
    // Initialize standard library
    init_stdlib(&mut vm)?;
    
    // Measure execution time
    let exec_start = Instant::now();
    
    // Execute the module
    let result = vm.execute_module(&module, &[])?;
    
    let exec_duration = exec_start.elapsed();
    
    // Print result
    println!("\n=== Execution Result ===");
    println!("Execution completed in {:?}", exec_duration);
    println!("Result: {}", format_value(&result));
    
    Ok(())
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() > 1 {
        // Run the provided script file
        let script_path = &args[1];
        let script = fs::read_to_string(script_path)?;
        
        match run_with_debug(&script) {
            Ok(_) => {},
            Err(e) => println!("Error: {}", e),
        }
    } else {
        // Interactive mode
        let mut input = String::new();
        
        println!("=== Ferrous Lua Interactive Mode ===");
        println!("Enter Lua code (type 'exit' to quit):");
        
        loop {
            print!("> ");
            io::stdout().flush()?;
            
            input.clear();
            io::stdin().read_line(&mut input)?;
            
            let script = input.trim();
            
            if script == "exit" {
                break;
            }
            
            if script.is_empty() {
                continue;
            }
            
            match run_with_debug(script) {
                Ok(_) => {},
                Err(e) => println!("Error: {}", e),
            }
        }
    }
    
    Ok(())
}