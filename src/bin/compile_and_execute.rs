//! Lua Code Compilation and Execution Pipeline Test
//!
//! This binary demonstrates the full pipeline from Lua source code
//! to compiled bytecode and execution. It shows detailed debug output
//! for each stage to aid in understanding and debugging the VM.

use std::env;
use std::fs;
use std::io::{self, Write};
use std::time::Instant;

use ferrous::lua::error::{LuaError, Result};
use ferrous::lua::value::Value;
use ferrous::lua::vm::LuaVM;
use ferrous::lua::compiler::Compiler;

// Print bytecode for debugging
fn print_bytecode(bytecode: &[u32], strings: &[String], constants: &[Value]) {
    println!("\n=== Bytecode Dump ===");
    
    println!("\nString Table:");
    for (i, s) in strings.iter().enumerate() {
        println!("  [{:>3}] \"{}\"", i, s);
    }
    
    println!("\nConstants:");
    for (i, c) in constants.iter().enumerate() {
        println!("  [{:>3}] {}", i, match c {
            Value::Nil => "nil".to_string(),
            Value::Boolean(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            Value::String(_) => format!("\"{}\"", strings[i]),
            _ => format!("{:?}", c),
        });
    }
    
    println!("\nInstructions:");
    for (i, instr) in bytecode.iter().enumerate() {
        // Decode instruction
        let opcode = instr & 0x3F;
        let a = (instr >> 6) & 0xFF;
        let bx = (instr >> 14) & 0x3FFFF;
        let b = (instr >> 14) & 0x1FF;
        let c = (instr >> 23) & 0x1FF;
        
        // Get opcode name
        let opcode_name = match opcode {
            0 => "MOVE", 1 => "LOADK", 2 => "LOADBOOL", 3 => "LOADNIL",
            4 => "GETUPVAL", 5 => "GETGLOBAL", 6 => "GETTABLE", 7 => "SETGLOBAL",
            8 => "SETUPVAL", 9 => "SETTABLE", 10 => "NEWTABLE", 11 => "SELF",
            12 => "ADD", 13 => "SUB", 14 => "MUL", 15 => "DIV",
            16 => "MOD", 17 => "POW", 18 => "UNM", 19 => "NOT",
            20 => "LEN", 21 => "CONCAT", 22 => "JMP", 23 => "EQ",
            24 => "LT", 25 => "LE", 26 => "TEST", 27 => "TESTSET",
            28 => "CALL", 29 => "TAILCALL", 30 => "RETURN", 31 => "FORLOOP",
            32 => "FORPREP", 33 => "TFORLOOP", 34 => "SETLIST", 35 => "CLOSE",
            36 => "CLOSURE", 37 => "VARARG",
            _ => "UNKNOWN",
        };
        
        // Format instruction based on type
        let instr_fmt = match opcode {
            // A B format
            0 | 18 | 19 | 20 | 35 | 37 => {
                format!("{:<10} {:>3} {:>3}", opcode_name, a, b)
            },
            // A Bx format
            1 | 5 | 7 | 36 => {
                format!("{:<10} {:>3} {:>3} ; {}", opcode_name, a, bx, if opcode == 1 {
                    // LOADK - show constant
                    if bx < constants.len() as u32 {
                        match &constants[bx as usize] {
                            Value::Nil => "nil".to_string(),
                            Value::Boolean(b) => b.to_string(),
                            Value::Number(n) => n.to_string(),
                            Value::String(_) => {
                                if bx < strings.len() as u32 {
                                    format!("\"{}\"", strings[bx as usize])
                                } else {
                                    "(invalid string)".to_string()
                                }
                            },
                            _ => format!("{:?}", constants[bx as usize]),
                        }
                    } else {
                        "(invalid constant)".to_string()
                    }
                } else if opcode == 5 || opcode == 7 {
                    // GETGLOBAL/SETGLOBAL - show name
                    if bx < strings.len() as u32 {
                        format!("\"{}\"", strings[bx as usize])
                    } else {
                        "(invalid name)".to_string()
                    }
                } else {
                    "".to_string()
                })
            },
            // A sBx format
            22 | 31 | 32 => {
                // Convert bx to signed
                let sbx = if bx & 0x20000 != 0 {
                    (bx | !0x3FFFF) as i32
                } else {
                    bx as i32
                };
                format!("{:<10} {:>3} {:>3}", opcode_name, a, sbx)
            },
            // A B C format
            _ => {
                format!("{:<10} {:>3} {:>3} {:>3}", opcode_name, a, b, c)
            },
        };
        
        println!("{:>4}: {}", i, instr_fmt);
    }
}

// Format a Lua value for printing
fn format_value(vm: &LuaVM, value: &Value) -> String {
    match value {
        Value::Nil => "nil".to_string(),
        Value::Boolean(b) => format!("{}", b),
        Value::Number(n) => format!("{}", n),
        Value::String(h) => {
            match vm.heap.get_string_value(*h) {
                Ok(s) => format!("\"{}\"", s),
                Err(_) => "<invalid string>".to_string(),
            }
        },
        Value::Table(_) => "<table>".to_string(),
        Value::Closure(_) => "<function>".to_string(),
        Value::Thread(_) => "<thread>".to_string(),
        Value::CFunction(_) => "<C function>".to_string(),
        Value::UserData(_) => "<userdata>".to_string(),
    }
}

// Run a Lua script with full debug output
fn run_with_debug(script: &str) -> Result<()> {
    println!("\n=== Running Script ===");
    println!("{}", script);
    
    // Measure compilation time
    let compile_start = Instant::now();
    
    // Create compiler
    let mut compiler = Compiler::new();
    
    // Compile the script
    let (proto, strings) = compiler.compile(script)?;
    
    let compile_duration = compile_start.elapsed();
    println!("\nCompilation completed in {:?}", compile_duration);
    
    // Print bytecode
    print_bytecode(&proto.bytecode, &strings, &proto.constants);
    
    // Create VM
    let mut vm = LuaVM::new()?;
    
    // Load the compiled code
    let closure_handle = vm.heap.create_closure(proto, Vec::new())?;
    
    // Measure execution time
    let exec_start = Instant::now();
    
    // Execute
    let result = vm.execute_function(closure_handle, &[])?;
    
    let exec_duration = exec_start.elapsed();
    
    // Print result
    println!("\n=== Execution Result ===");
    println!("Execution completed in {:?}", exec_duration);
    println!("Result: {}", format_value(&vm, &result));
    
    // Print VM statistics
    println!("\n=== VM Statistics ===");
    println!("Instructions executed: {}", vm.instruction_count);
    
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
            
            match run_with_debug(script) {
                Ok(_) => {},
                Err(e) => println!("Error: {}", e),
            }
        }
    }
    
    Ok(())
}