use ferrous::vm::{Vm, VmConfig};
use ferrous::value::Value;
use ferrous::compiler::opcodes::OpCode;
use ferrous::compiler::chunk::Chunk;

fn main() {
    println!("=== GETGLOBAL Opcode Test ===\n");
    
    // Create VM with standard library
    let config = VmConfig {
        with_standard_library: true,
        debug_mode: true,
    };
    let mut vm = Vm::new(config);
    
    // Verify print is registered
    println!("Initial globals state:");
    vm.debug_verify_globals();
    
    // Create a simple chunk that uses GETGLOBAL to get print
    let mut chunk = Chunk::new("test_getglobal");
    
    // Add "print" as a constant
    let print_const = chunk.add_constant(Value::from("print"));
    println!("\nAdded 'print' as constant at index: {}", print_const);
    
    // Add "Hello, World!" as a constant
    let hello_const = chunk.add_constant(Value::from("Hello, World!"));
    println!("Added 'Hello, World!' as constant at index: {}", hello_const);
    
    // Generate bytecode:
    // GETGLOBAL 0  ; Get global "print" (constant 0)
    // LOADK 1      ; Load constant "Hello, World!" (constant 1)
    // CALL 0 1 0   ; Call print with 1 argument, 0 return values
    // RETURN 0 0   ; Return with 0 values
    
    println!("\nGenerating bytecode:");
    
    // GETGLOBAL instruction
    chunk.write_op(OpCode::GetGlobal as u8, 1);
    chunk.write_byte(0, 1); // Register 0
    chunk.write_byte(print_const as u8, 1); // Constant index for "print"
    println!("  GETGLOBAL R0 K{} (K{} = 'print')", print_const, print_const);
    
    // LOADK instruction
    chunk.write_op(OpCode::LoadK as u8, 2);
    chunk.write_byte(1, 2); // Register 1
    chunk.write_byte(hello_const as u8, 2); // Constant index for "Hello, World!"
    println!("  LOADK R1 K{} (K{} = 'Hello, World!')", hello_const, hello_const);
    
    // CALL instruction
    chunk.write_op(OpCode::Call as u8, 3);
    chunk.write_byte(0, 3); // Function register
    chunk.write_byte(2, 3); // nargs + 1 (1 argument + 1)
    chunk.write_byte(1, 3); // nresults + 1 (0 results + 1)
    println!("  CALL R0 2 1 ; Call print(R1)");
    
    // RETURN instruction
    chunk.write_op(OpCode::Return as u8, 4);
    chunk.write_byte(0, 4); // Start register
    chunk.write_byte(1, 4); // Count + 1 (0 returns + 1)
    println!("  RETURN R0 1 ; Return nothing");
    
    // Load the chunk into VM
    println!("\nLoading chunk into VM...");
    let chunk_id = vm.load_chunk(chunk);
    println!("Chunk loaded with ID: {}", chunk_id);
    
    // Execute with detailed debugging
    println!("\n=== Executing Chunk ===");
    println!("Watch for GETGLOBAL execution:\n");
    
    match vm.run(chunk_id) {
        Ok(results) => {
            println!("\n✓ Execution completed successfully");
            if !results.is_empty() {
                println!("Results: {:?}", results);
            }
        }
        Err(e) => {
            println!("\n✗ Execution failed with error: {}", e);
            
            // Additional diagnostics
            println!("\nStack state at error:");
            vm.debug_stack();
            
            println!("\nGlobals state at error:");
            vm.debug_verify_globals();
        }
    }
}