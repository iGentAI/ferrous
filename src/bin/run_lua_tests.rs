//! Lua Test Runner
//!
//! This binary provides a simple CLI to run Lua compliance tests,
//! testing all opcodes and language features.

use ferrous::lua::{RcVM, Value, compile, rc_value};
use std::env;
use std::fs;
use std::process;
use std::path::Path;

/// Run a single Lua script and return its result
fn run_script(script_path: &str) -> Result<Value, String> {
    // Read the script file
    let script = match fs::read_to_string(script_path) {
        Ok(content) => content,
        Err(e) => return Err(format!("Failed to read script file: {}", e)),
    };
    
    println!("Running script: {}...", script_path);
    
    // Create a VM instance
    let mut vm = match RcVM::new() {
        Ok(vm) => vm,
        Err(e) => return Err(format!("Failed to create VM: {:?}", e)),
    };
    
    // Initialize the standard library
    match vm.init_stdlib() {
        Ok(_) => {},
        Err(e) => return Err(format!("Failed to initialize standard library: {:?}", e)),
    };
    
    // Compile the script
    let module = match compile(&script) {
        Ok(module) => module,
        Err(e) => return Err(format!("Compilation error: {:?}", e)),
    };
    
    // Debug dump of the compiled module to help identify issues
    println!("Compiled module: {} bytecode instructions, {} constants, {} strings, {} prototypes",
             module.bytecode.len(), module.constants.len(), module.strings.len(), module.prototypes.len());
    
    if !module.upvalues.is_empty() {
        println!("Main function has {} upvalues:", module.upvalues.len());
        for (i, upval) in module.upvalues.iter().enumerate() {
            println!("  Upvalue {}: in_stack={}, index={}", i, upval.in_stack, upval.index);
        }
    }
    
    // Execute the module
    match vm.execute_module(&module, &[]) {
        Ok(value) => {
            // Convert rc_value::Value to value::Value for interface compatibility
            let value = match value {
                rc_value::Value::Nil => Value::Nil,
                rc_value::Value::Boolean(b) => Value::Boolean(b),
                rc_value::Value::Number(n) => Value::Number(n),
                rc_value::Value::String(handle) => {
                    // Try to extract the string value, if we can borrow it safely
                    let str_ref = handle.borrow();
                    if let Ok(s) = str_ref.to_str() {
                        println!("Result string: {}", s);
                    }
                    Value::Nil // We can't create a real StringHandle here
                },
                _ => Value::Nil, // For more complex types, just return Nil for now
            };
            
            Ok(value)
        },
        Err(e) => {
            println!("FAILURE\nError: {:?}", e);
            Err(format!("Execution error: {:?}", e))
        }
    }
}

/// Print usage information
fn print_usage(program_name: &str) {
    println!("Usage: {} [OPTIONS] SCRIPT_PATH", program_name);
    println!();
    println!("Options:");
    println!("  --help        Show this help message");
    println!("  --run-all     Run all compliance tests");
    println!();
    println!("Examples:");
    println!("  {} tests/lua/basic/arithmetic.lua", program_name);
    println!("  {} --run-all", program_name);
}

/// Run all compliance tests
fn run_all_tests() -> bool {
    // Categorize tests for better reporting
    let basic_tests = [
        "tests/lua/basic/arithmetic.lua",
        "tests/lua/basic/assignment.lua",
        "tests/lua/basic/comparisons.lua",
        "tests/lua/basic/concat.lua",
        "tests/lua/basic/print.lua",
        "tests/lua/basic/tostring.lua",
        "tests/lua/basic/type.lua",
    ];
    
    let control_tests = [
        "tests/lua/control/numeric_for.lua",
        "tests/lua/control/pairs_simple.lua",
        "tests/lua/control/tforloop_minimal.lua",
    ];
    
    let function_tests = [
        "tests/lua/functions/definition.lua",
        "tests/lua/functions/upvalue_simple.lua",
        "tests/lua/functions/closure.lua",
    ];
    
    let table_tests = [
        "tests/lua/tables/create.lua",
    ];
    
    println!("\n=== Basic Language Features ===\n");
    let basic_success = run_test_category(&basic_tests);
    
    println!("\n=== Control Flow Features ===\n");
    let control_success = run_test_category(&control_tests);
    
    println!("\n=== Function Features ===\n");
    let function_success = run_test_category(&function_tests);
    
    println!("\n=== Table Features ===\n");
    let table_success = run_test_category(&table_tests);
    
    println!("\n=== Summary ===\n");
    println!("Basic Language Features: {}", if basic_success { "PASS" } else { "FAIL" });
    println!("Control Flow Features: {}", if control_success { "PASS" } else { "FAIL" });
    println!("Function Features: {}", if function_success { "PASS" } else { "FAIL" });
    println!("Table Features: {}", if table_success { "PASS" } else { "FAIL" });
    println!();
    
    let overall_success = basic_success && control_success && function_success && table_success;
    if overall_success {
        println!("All tests passed successfully!");
    } else {
        println!("Some tests failed.");
    }
    
    overall_success
}

/// Run a category of tests
fn run_test_category(test_files: &[&str]) -> bool {
    let mut success = true;
    
    for file in test_files {
        print!("Running test: {}... ", file);
        
        if Path::new(file).exists() {
            match run_script(file) {
                Ok(_) => println!("SUCCESS"),
                Err(e) => {
                    println!("FAILURE");
                    println!("  Error: {}", e);
                    success = false;
                }
            }
        } else {
            println!("SKIPPED (file not found)");
        }
    }
    
    success
}

/// Format an instruction for human-readable output
fn disassemble_instruction(inst: u32) -> String {
    use ferrous::lua::codegen::{Instruction, OpCode};
    
    let instruction = Instruction(inst);
    let opcode = instruction.get_opcode();
    
    match opcode {
        OpCode::Move => format!("MOVE R({}) := R({})", instruction.get_a(), instruction.get_b()),
        OpCode::LoadK => format!("LOADK R({}) := K({})", instruction.get_a(), instruction.get_bx()),
        OpCode::LoadBool => format!("LOADBOOL R({}) := {}, {}", 
                                   instruction.get_a(), 
                                   if instruction.get_b() != 0 { "true" } else { "false" },
                                   if instruction.get_c() != 0 { "skip" } else { "no-skip" }),
        OpCode::LoadNil => format!("LOADNIL R({})...R({}) := nil", 
                                  instruction.get_a(), instruction.get_b()),
        OpCode::GetUpval => format!("GETUPVAL R({}) := UV[{}]", 
                                   instruction.get_a(), instruction.get_b()),
        OpCode::GetGlobal => format!("GETGLOBAL R({}) := _ENV[K({})]", 
                                    instruction.get_a(), instruction.get_bx()),
        OpCode::SetGlobal => format!("SETGLOBAL _ENV[K({})] := R({})", 
                                    instruction.get_bx(), instruction.get_a()),
        OpCode::SetUpval => format!("SETUPVAL UV[{}] := R({})", 
                                   instruction.get_a(), instruction.get_b()),
        OpCode::GetTable => {
            let (c_is_const, c_idx) = instruction.get_rk_c();
            if c_is_const {
                format!("GETTABLE R({}) := R({})[K({})]", 
                       instruction.get_a(), instruction.get_b(), c_idx)
            } else {
                format!("GETTABLE R({}) := R({})[R({})]", 
                       instruction.get_a(), instruction.get_b(), c_idx)
            }
        },
        OpCode::SetTable => {
            let (b_is_const, b_idx) = instruction.get_rk_b();
            let (c_is_const, c_idx) = instruction.get_rk_c();
            
            let b_source = if b_is_const { format!("K({})", b_idx) } else { format!("R({})", b_idx) };
            let c_source = if c_is_const { format!("K({})", c_idx) } else { format!("R({})", c_idx) };
            
            format!("SETTABLE R({})[{}] := {}", instruction.get_a(), b_source, c_source)
        },
        OpCode::Add => {
            let (b_is_const, b_idx) = instruction.get_rk_b();
            let (c_is_const, c_idx) = instruction.get_rk_c();
            
            let b_source = if b_is_const { format!("K({})", b_idx) } else { format!("R({})", b_idx) };
            let c_source = if c_is_const { format!("K({})", c_idx) } else { format!("R({})", c_idx) };
            
            format!("ADD R({}) := {} + {}", instruction.get_a(), b_source, c_source)
        },
        OpCode::Closure => format!("CLOSURE R({}) := function K({})", 
                                 instruction.get_a(), instruction.get_bx()),
        _ => format!("{:?} A={} B={} C={}", 
                    opcode, instruction.get_a(), instruction.get_b(), instruction.get_c()),
    }
}

/// Run a specific test file with debug disassembly
fn run_specific_test(script_path: &str) {
    print!("Running script: {}... ", script_path);
    
    // Add detailed bytecode disassembly for upvalue_simple.lua
    if script_path.contains("upvalue_simple.lua") {
        println!("\nDETAILED DISASSEMBLY FOR UPVALUE_SIMPLE.LUA:");
        
        // Read the script
        if let Ok(script) = fs::read_to_string(script_path) {
            // Compile it
            if let Ok(module) = compile(&script) {
                println!("\nMAIN FUNCTION BYTECODE (upvalues: {}):", module.upvalues.len());
                for (i, inst) in module.bytecode.iter().enumerate() {
                    println!("{:04}: 0x{:08x} {}", i, inst, disassemble_instruction(*inst));
                }
                
                println!("\nCONSTANTS:");
                for (i, constant) in module.constants.iter().enumerate() {
                    println!("  Constant {}: {:?}", i, constant);
                }
                
                println!("\nNESTED FUNCTIONS:");
                for (i, proto) in module.prototypes.iter().enumerate() {
                    println!("  Function {}: {} params, {} upvalues, {} bytecode instructions", 
                            i, proto.num_params, proto.upvalues.len(), proto.bytecode.len());
                    
                    if !proto.upvalues.is_empty() {
                        println!("    Upvalues:");
                        for (j, upval) in proto.upvalues.iter().enumerate() {
                            println!("      Upvalue {}: in_stack={}, index={}", 
                                    j, upval.in_stack, upval.index);
                        }
                    }
                    
                    println!("    BYTECODE:");
                    for (j, inst) in proto.bytecode.iter().enumerate() {
                        println!("    {:04}: 0x{:08x} {}", j, inst, disassemble_instruction(*inst));
                    }
                    
                    println!("    CONSTANTS:");
                    for (j, constant) in proto.constants.iter().enumerate() {
                        println!("      Constant {}: {:?}", j, constant);
                    }
                }
            }
        }
    }
    
    match run_script(script_path) {
        Ok(result) => {
            println!("SUCCESS");
            println!("Result: {:?}", result);
        },
        Err(e) => {
            println!("FAILURE");
            println!("Error: {}", e);
            process::exit(1);
        }
    }
}

/// Entry point
fn main() {
    let args: Vec<String> = env::args().collect();
    let program_name = args[0].clone();
    
    if args.len() < 2 {
        print_usage(&program_name);
        process::exit(1);
    }
    
    match args[1].as_str() {
        "--help" => {
            print_usage(&program_name);
            process::exit(0);
        },
        "--run-all" => {
            println!("Running all compliance tests...\n");
            if run_all_tests() {
                process::exit(0);
            } else {
                process::exit(1);
            }
        },
        script_path => {
            if script_path.starts_with("-") {
                println!("Unknown option: {}", script_path);
                print_usage(&program_name);
                process::exit(1);
            }
            
            run_specific_test(script_path);
            process::exit(0);
        }
    }
}