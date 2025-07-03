//! Lua Compiler Test
//!
//! This binary provides a simple test harness for the Lua compiler.
//! It compiles Lua source code and prints the resulting bytecode.

use ferrous::lua::{compile, OpCode, CompiledModule};
use std::env;
use std::fs;

/// Print the bytecode of a compiled module
fn print_module(module: &CompiledModule) {
    println!("Module Information:");
    println!("  Main Function:");
    println!("    Parameters: {}{}", 
        module.num_params,
        if module.is_vararg { " + vararg" } else { "" }
    );
    println!("    Max Stack: {}", module.max_stack_size);
    println!("    Upvalues: {}", module.upvalues.len());
    println!("    Constants: {}", module.constants.len());
    println!("    Bytecode Length: {}", module.bytecode.len());
    println!("    String Table: {} entries", module.strings.len());
    println!("    Function Prototypes: {} entries", module.prototypes.len());
    
    println!("\nString Table:");
    for (i, s) in module.strings.iter().enumerate() {
        println!("  [{}]: \"{}\"", i, s.escape_debug());
    }
    
    println!("\nConstants:");
    for (i, constant) in module.constants.iter().enumerate() {
        println!("  [{}]: {:?}", i, constant);
    }
    
    println!("\nBytecode:");
    for (i, instr) in module.bytecode.iter().enumerate() {
        print_instruction(i, *instr);
    }
    
    if !module.prototypes.is_empty() {
        println!("\nNested Functions:");
        for (i, proto) in module.prototypes.iter().enumerate() {
            println!("\n  Function #{}", i);
            println!("    Parameters: {}{}", 
                proto.num_params, 
                if proto.is_vararg { " + vararg" } else { "" }
            );
            println!("    Max Stack: {}", proto.max_stack_size);
            println!("    Constants: {} entries", proto.constants.len());
            println!("    Bytecode: {} instructions", proto.bytecode.len());
            
            println!("    Constants:");
            for (j, constant) in proto.constants.iter().enumerate() {
                println!("      [{}]: {:?}", j, constant);
            }
            
            println!("    Bytecode:");
            for (j, instr) in proto.bytecode.iter().enumerate() {
                print!("      ");
                print_instruction(j, *instr);
            }
        }
    }
}

/// Print an instruction in a readable format
fn print_instruction(pc: usize, instruction: u32) {
    let opcode = (instruction & 0x3F) as u8;
    let a = ((instruction >> 6) & 0xFF) as u8;
    let c = ((instruction >> 14) & 0x1FF) as u16;
    let b = ((instruction >> 23) & 0x1FF) as u16;
    let bx = ((instruction >> 14) & 0x3FFFF) as u32;
    let sbx = (bx as i32) - 131071;
    
    // Convert opcode to enum
    let opcode = match opcode {
        0 => OpCode::Move,
        1 => OpCode::LoadK,
        2 => OpCode::LoadBool,
        3 => OpCode::LoadNil,
        4 => OpCode::GetUpval,
        5 => OpCode::GetGlobal,
        6 => OpCode::SetGlobal,
        7 => OpCode::SetUpval,
        8 => OpCode::GetTable,
        9 => OpCode::SetTable,
        10 => OpCode::NewTable,
        11 => OpCode::Self_,
        12 => OpCode::Add,
        13 => OpCode::Sub,
        14 => OpCode::Mul,
        15 => OpCode::Div,
        16 => OpCode::Mod,
        17 => OpCode::Pow,
        18 => OpCode::Unm,
        19 => OpCode::Not,
        20 => OpCode::Len,
        21 => OpCode::Concat,
        22 => OpCode::Jmp,
        23 => OpCode::Eq,
        24 => OpCode::Lt,
        25 => OpCode::Le,
        26 => OpCode::Test,
        27 => OpCode::TestSet,
        28 => OpCode::Call,
        29 => OpCode::TailCall,
        30 => OpCode::Return,
        31 => OpCode::ForPrep,
        32 => OpCode::ForLoop,
        33 => OpCode::TForLoop,
        34 => OpCode::SetList,
        35 => OpCode::VarArg,
        36 => OpCode::Closure,
        37 => OpCode::Close,
        38 => OpCode::ExtraArg,
        _ => OpCode::Move, // Default
    };
    
    print!("[{:4}] {:?} ", pc, opcode);
    
    // Format based on instruction type
    match opcode {
        // Instructions that use Bx
        OpCode::LoadK | OpCode::GetGlobal | OpCode::SetGlobal | OpCode::Closure => {
            println!("{}, {}", a, bx);
        },
        
        // Instructions that use sBx
        OpCode::Jmp | OpCode::ForPrep | OpCode::ForLoop => {
            println!("{}, {}", a, sbx);
        },
        
        // Instructions that use A, B, C
        _ => {
            println!("{}, {}, {}", a, b, c);
        },
    }
}

/// Print compilation errors in a user-friendly way
fn print_error(err: ferrous::lua::LuaError) {
    match &err {
        ferrous::lua::LuaError::SyntaxError { message, line, column } => {
            println!("Syntax error at line {}:{}: {}", line, column, message);
        }
        ferrous::lua::LuaError::CompileError(msg) => {
            println!("Compilation error: {}", msg);
        }
        _ => {
            println!("Error: {}", err);
        }
    }
}

/// Entry point
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        println!("Usage: {} <lua_file>", args[0]);
        return Ok(());
    }
    
    let file_path = &args[1];
    let source = fs::read_to_string(file_path)?;
    
    println!("Compiling {}...", file_path);
    
    match compile(&source) {
        Ok(module) => {
            println!("\nCompilation successful!\n");
            print_module(&module);
        }
        Err(err) => {
            println!("\nCompilation failed!\n");
            print_error(err);
        }
    }
    
    Ok(())
}