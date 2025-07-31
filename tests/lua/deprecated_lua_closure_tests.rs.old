//! Tests for Lua VM closures, upvalues, and new opcodes
//!
//! This file contains tests that verify the Lua VM implementation
//! handles closures, upvalues, and special opcodes correctly.

#[cfg(test)]
mod closure_tests {
    use ferrous::lua::{LuaVM, Value, value::{FunctionProto, Closure, CallFrame, UpvalueInfo}, 
                   handle::{TableHandle, StringHandle, ClosureHandle, FunctionProtoHandle}, 
                   transaction::HeapTransaction};
    
    /// Helper to encode instructions following Lua 5.1 format
    /// Bits 0-5: opcode, Bits 6-13: A, Bits 14-22: C, Bits 23-31: B
    fn encode_abc(opcode: u32, a: u32, b: u32, c: u32) -> u32 {
        opcode | (a << 6) | (c << 14) | (b << 23)
    }

    /// Helper to encode instructions with Bx argument
    fn encode_abx(opcode: u32, a: u32, bx: u32) -> u32 {
        opcode | (a << 6) | (bx << 14)
    }

    /// Helper to encode instructions with sBx argument (signed, biased by 131071)
    fn encode_asbx(opcode: u32, a: u32, sbx: i32) -> u32 {
        let biased = (sbx + 131071) as u32;
        opcode | (a << 6) | (biased << 14)
    }

    /// Helper to encode ExtraArg instruction
    fn encode_extra_arg(value: u32) -> u32 {
        // OpCode::ExtraArg = 38
        encode_abx(38, 0, value)
    }
    
    #[test]
    fn test_closure_capture_local_variables() {
        let mut vm = LuaVM::new().unwrap();
        
        // Test bytecode for:
        // local x = 42
        // local function f() return x end
        // return f()
        
        // Main function bytecode
        let main_bytecode = vec![
            encode_abx(1, 0, 0),        // 0: LOADK R(0), K(0) ; x = 42
            encode_abx(36, 1, 1),       // 1: CLOSURE R(1), PROTO(1) ; create closure
            encode_abc(0, 0, 0, 0),     // 2: Upvalue initialization (dummy)
            encode_abc(28, 1, 1, 1),    // 3: CALL R(1)() ; call closure
            encode_abc(30, 1, 2, 0),    // 4: RETURN R(1) ; return result
        ];
        
        // Closure function bytecode - captures x as upvalue
        let closure_bytecode = vec![
            encode_abc(4, 0, 0, 0),     // 0: GETUPVAL R(0), U(0) ; get x
            encode_abc(30, 0, 2, 0),    // 1: RETURN R(0) ; return x
        ];
        
        // Create function prototypes
        let closure_proto = {
            let mut tx = HeapTransaction::new(vm.heap_mut());
            let proto = FunctionProto {
                bytecode: closure_bytecode,
                constants: vec![],
                num_params: 0,
                is_vararg: false,
                max_stack_size: 2,
                upvalues: vec![UpvalueInfo { in_stack: true, index: 0 }], // Captures R(0) of parent
            };
            let handle = tx.create_function_proto(proto).unwrap();
            tx.commit().unwrap();
            handle
        };
        
        let main_proto = FunctionProto {
            bytecode: main_bytecode,
            constants: vec![
                Value::Number(42.0),
                Value::FunctionProto(closure_proto),
            ],
            num_params: 0,
            is_vararg: false,
            max_stack_size: 4,
            upvalues: vec![],
        };
        
        // Execute
        let module = ferrous::lua::compiler::CompiledModule {
            main_function: main_proto,
        };
        
        let result = vm.execute_module(&module, &[]).unwrap();
        assert_eq!(result, Value::Number(42.0));
    }
    
    #[test]
    fn test_nested_closures() {
        let mut vm = LuaVM::new().unwrap();
        
        // Test bytecode for:
        // local x = 1
        // local function outer()
        //   local y = 2
        //   local function inner()
        //     return x + y
        //   end
        //   return inner()
        // end
        // return outer()
        
        // Inner function bytecode
        let inner_bytecode = vec![
            encode_abc(4, 0, 0, 0),     // 0: GETUPVAL R(0), U(0) ; get x
            encode_abc(4, 1, 1, 0),     // 1: GETUPVAL R(1), U(1) ; get y
            encode_abc(12, 0, 0, 1),    // 2: ADD R(0) := R(0) + R(1)
            encode_abc(30, 0, 2, 0),    // 3: RETURN R(0)
        ];
        
        // Outer function bytecode
        let outer_bytecode = vec![
            encode_abx(1, 0, 0),        // 0: LOADK R(0), K(0) ; y = 2
            encode_abx(36, 1, 1),       // 1: CLOSURE R(1), PROTO(1) ; create inner
            encode_abc(4, 0, 0, 0),     // 2: Upvalue init for x (parent upvalue)
            encode_abc(0, 0, 0, 0),     // 3: Upvalue init for y (local register)
            encode_abc(28, 1, 1, 1),    // 4: CALL R(1)()
            encode_abc(30, 1, 2, 0),    // 5: RETURN R(1)
        ];
        
        // Main function bytecode
        let main_bytecode = vec![
            encode_abx(1, 0, 0),        // 0: LOADK R(0), K(0) ; x = 1
            encode_abx(36, 1, 1),       // 1: CLOSURE R(1), PROTO(1) ; create outer
            encode_abc(0, 0, 0, 0),     // 2: Upvalue init for x
            encode_abc(28, 1, 1, 1),    // 3: CALL R(1)()
            encode_abc(30, 1, 2, 0),    // 4: RETURN R(1)
        ];
        
        // Create function prototypes
        let inner_proto = {
            let mut tx = HeapTransaction::new(vm.heap_mut());
            let proto = FunctionProto {
                bytecode: inner_bytecode,
                constants: vec![],
                num_params: 0,
                is_vararg: false,
                max_stack_size: 3,
                upvalues: vec![
                    UpvalueInfo { in_stack: false, index: 0 }, // x from outer's upvalues
                    UpvalueInfo { in_stack: true, index: 0 },  // y from outer's stack
                ],
            };
            let handle = tx.create_function_proto(proto).unwrap();
            tx.commit().unwrap();
            handle
        };
        
        let outer_proto = {
            let mut tx = HeapTransaction::new(vm.heap_mut());
            let proto = FunctionProto {
                bytecode: outer_bytecode,
                constants: vec![
                    Value::Number(2.0),
                    Value::FunctionProto(inner_proto),
                ],
                num_params: 0,
                is_vararg: false,
                max_stack_size: 4,
                upvalues: vec![
                    UpvalueInfo { in_stack: true, index: 0 }, // x from main's stack
                ],
            };
            let handle = tx.create_function_proto(proto).unwrap();
            tx.commit().unwrap();
            handle
        };
        
        let main_proto = FunctionProto {
            bytecode: main_bytecode,
            constants: vec![
                Value::Number(1.0),
                Value::FunctionProto(outer_proto),
            ],
            num_params: 0,
            is_vararg: false,
            max_stack_size: 4,
            upvalues: vec![],
        };
        
        // Execute
        let module = ferrous::lua::compiler::CompiledModule {
            main_function: main_proto,
        };
        
        let result = vm.execute_module(&module, &[]).unwrap();
        assert_eq!(result, Value::Number(3.0)); // 1 + 2 = 3
    }
    
    #[test]
    fn test_upvalue_sharing() {
        let mut vm = LuaVM::new().unwrap();
        
        // Test bytecode for:
        // local shared = 10
        // local function inc() shared = shared + 1; return shared end
        // local function dec() shared = shared - 1; return shared end
        // inc(); return dec()
        
        // inc function bytecode
        let inc_bytecode = vec![
            encode_abc(4, 0, 0, 0),     // 0: GETUPVAL R(0), U(0) ; get shared
            encode_abx(1, 1, 0),        // 1: LOADK R(1), K(0) ; load 1
            encode_abc(12, 0, 0, 1),    // 2: ADD R(0) := R(0) + R(1)
            encode_abc(7, 0, 0, 0),     // 3: SETUPVAL U(0), R(0) ; shared = result
            encode_abc(4, 0, 0, 0),     // 4: GETUPVAL R(0), U(0) ; get shared again
            encode_abc(30, 0, 2, 0),    // 5: RETURN R(0)
        ];
        
        // dec function bytecode
        let dec_bytecode = vec![
            encode_abc(4, 0, 0, 0),     // 0: GETUPVAL R(0), U(0) ; get shared
            encode_abx(1, 1, 0),        // 1: LOADK R(1), K(0) ; load 1  
            encode_abc(13, 0, 0, 1),    // 2: SUB R(0) := R(0) - R(1)
            encode_abc(7, 0, 0, 0),     // 3: SETUPVAL U(0), R(0) ; shared = result
            encode_abc(4, 0, 0, 0),     // 4: GETUPVAL R(0), U(0) ; get shared again
            encode_abc(30, 0, 2, 0),    // 5: RETURN R(0)
        ];
        
        // Main function bytecode
        let main_bytecode = vec![
            encode_abx(1, 0, 0),        // 0: LOADK R(0), K(0) ; shared = 10
            encode_abx(36, 1, 1),       // 1: CLOSURE R(1), PROTO(1) ; create inc
            encode_abc(0, 0, 0, 0),     // 2: Upvalue init for shared
            encode_abx(36, 2, 2),       // 3: CLOSURE R(2), PROTO(2) ; create dec
            encode_abc(0, 0, 0, 0),     // 4: Upvalue init for shared (same upvalue!)
            encode_abc(28, 1, 1, 1),    // 5: CALL R(1)() ; call inc
            encode_abc(28, 2, 2, 1),    // 6: CALL R(2)() ; call dec
            encode_abc(30, 2, 2, 0),    // 7: RETURN R(2)
        ];
        
        // Create function prototypes
        let inc_proto = {
            let mut tx = HeapTransaction::new(vm.heap_mut());
            let proto = FunctionProto {
                bytecode: inc_bytecode,
                constants: vec![Value::Number(1.0)],
                num_params: 0,
                is_vararg: false,
                max_stack_size: 3,
                upvalues: vec![UpvalueInfo { in_stack: true, index: 0 }], // shared from main
            };
            let handle = tx.create_function_proto(proto).unwrap();
            tx.commit().unwrap();
            handle
        };
        
        let dec_proto = {
            let mut tx = HeapTransaction::new(vm.heap_mut());
            let proto = FunctionProto {
                bytecode: dec_bytecode,
                constants: vec![Value::Number(1.0)],
                num_params: 0,
                is_vararg: false,
                max_stack_size: 3,
                upvalues: vec![UpvalueInfo { in_stack: true, index: 0 }], // shared from main
            };
            let handle = tx.create_function_proto(proto).unwrap();
            tx.commit().unwrap();
            handle
        };
        
        let main_proto = FunctionProto {
            bytecode: main_bytecode,
            constants: vec![
                Value::Number(10.0),
                Value::FunctionProto(inc_proto),
                Value::FunctionProto(dec_proto),
            ],
            num_params: 0,
            is_vararg: false,
            max_stack_size: 5,
            upvalues: vec![],
        };
        
        // Execute
        let module = ferrous::lua::compiler::CompiledModule {
            main_function: main_proto,
        };
        
        let result = vm.execute_module(&module, &[]).unwrap();
        assert_eq!(result, Value::Number(10.0)); // 10 + 1 - 1 = 10
    }
    
    #[test]
    fn test_close_opcode() {
        let mut vm = LuaVM::new().unwrap();
        
        // Test bytecode that demonstrates closing upvalues
        // local x = 5
        // local function capture() return x end
        // x = 10 -- modify x
        // do end -- scope ends, Close upvalue
        // return capture() -- should return value when closure created
        
        let capture_bytecode = vec![
            encode_abc(4, 0, 0, 0),     // 0: GETUPVAL R(0), U(0) ; get x
            encode_abc(30, 0, 2, 0),    // 1: RETURN R(0)
        ];
        
        // Main function with Close opcode
        let main_bytecode = vec![
            encode_abx(1, 0, 0),        // 0: LOADK R(0), K(0) ; x = 5
            encode_abx(36, 1, 1),       // 1: CLOSURE R(1), PROTO(1) ; create capture
            encode_abc(0, 0, 0, 0),     // 2: Upvalue init for x
            encode_abx(1, 0, 2),        // 3: LOADK R(0), K(2) ; x = 10
            encode_abc(37, 0, 0, 0),    // 4: CLOSE R(0) ; close upvalues >= R(0)
            encode_abc(28, 2, 1, 1),    // 5: CALL R(2) := R(1)() ; Use R(1) for closure
            encode_abc(30, 2, 2, 0),    // 6: RETURN R(2)
        ];
        
        // Create prototypes 
        let capture_proto = {
            let mut tx = HeapTransaction::new(vm.heap_mut());
            let proto = FunctionProto {
                bytecode: capture_bytecode,
                constants: vec![],
                num_params: 0,
                is_vararg: false,
                max_stack_size: 2,
                upvalues: vec![UpvalueInfo { in_stack: true, index: 0 }],
            };
            let handle = tx.create_function_proto(proto).unwrap();
            tx.commit().unwrap();
            handle
        };
        
        let main_proto = FunctionProto {
            bytecode: main_bytecode,
            constants: vec![
                Value::Number(5.0),
                Value::FunctionProto(capture_proto),
                Value::Number(10.0),
            ],
            num_params: 0,
            is_vararg: false,
            max_stack_size: 4,
            upvalues: vec![],
        };
        
        // Execute
        let module = ferrous::lua::compiler::CompiledModule {
            main_function: main_proto,
        };
        
        let result = vm.execute_module(&module, &[]).unwrap();
        
        // In our implementation, the Close opcode happens after updating x to 10,
        // so the upvalue should capture the value 10
        assert_eq!(result, Value::Number(10.0));
    }
    
    #[test]
    fn test_self_opcode() {
        let mut vm = LuaVM::new().unwrap();
        
        // Test bytecode for method call:
        // local obj = { value = 42 }
        // function obj:getValue() return self.value end
        // return obj:getValue()
        
        // Create string constants needed for the test
        let value_key = vm.create_string("value").unwrap();
        let get_value_key = vm.create_string("getValue").unwrap();
        
        // Method function bytecode
        let method_bytecode = vec![
            // R(0) is already the 'self' parameter
            encode_abc(8, 1, 0, 256),       // 0: GETTABLE R(1), R(0), K(0) ; get self.value
            encode_abc(30, 1, 2, 0),        // 1: RETURN R(1)
        ];
        
        // Main function bytecode  
        let main_bytecode = vec![
            encode_abc(10, 0, 0, 0),        // 0: NEWTABLE R(0) ; create object
            encode_abx(1, 1, 0),            // 1: LOADK R(1), K(0) ; load 42
            encode_abc(9, 0, 257, 1),       // 2: SETTABLE R(0), K(1), R(1) ; obj.value = 42
            encode_abx(36, 1, 1),           // 3: CLOSURE R(1), PROTO(1) ; create method
            encode_abc(9, 0, 258, 1),       // 4: SETTABLE R(0), K(2), R(1) ; obj.getValue = method
            encode_abc(11, 2, 0, 258),      // 5: SELF R(2), R(0), K(2) ; R(3)=self, R(2)=method
            encode_abc(28, 2, 1, 1),        // 6: CALL R(2) ; call method (result in R(2))
            encode_abc(30, 2, 2, 0),        // 7: RETURN R(2)
        ];
        
        // Create method prototype
        let method_proto = {
            let mut tx = HeapTransaction::new(vm.heap_mut());
            let proto = FunctionProto {
                bytecode: method_bytecode,
                constants: vec![Value::String(value_key)],
                num_params: 1, // self parameter
                is_vararg: false,
                max_stack_size: 3,
                upvalues: vec![],
            };
            let handle = tx.create_function_proto(proto).unwrap();
            tx.commit().unwrap();
            handle
        };
        
        // Main prototype with constants
        let main_proto = FunctionProto {
            bytecode: main_bytecode,
            constants: vec![
                Value::Number(42.0),
                Value::String(value_key),
                Value::String(get_value_key),
                Value::FunctionProto(method_proto),
            ],
            num_params: 0,
            is_vararg: false,
            max_stack_size: 5,
            upvalues: vec![],
        };
        
        // Execute
        let module = ferrous::lua::compiler::CompiledModule {
            main_function: main_proto,
        };
        
        let result = vm.execute_module(&module, &[]).unwrap();
        assert_eq!(result, Value::Number(42.0));
    }
    
    #[test]
    fn test_vararg_opcode() {
        let mut vm = LuaVM::new().unwrap();
        
        // Test bytecode for vararg function:
        // local function f(...) 
        //   local a, b, c = ...
        //   return a + b + (c or 0)
        // end
        // return f(10, 20, 30)
        
        // Vararg function bytecode
        let vararg_bytecode = vec![
            encode_abc(35, 0, 4, 0),        // 0: VARARG R(0), R(1), R(2) ; get 3 varargs
            encode_abc(26, 2, 0, 0),        // 1: TEST R(2), false ; test if c is nil
            encode_abc(22, 1, 0, 1),        // 2: JMP +1 ; skip if not nil
            encode_abx(1, 2, 0),            // 3: LOADK R(2), K(0) ; c = 0
            encode_abc(12, 3, 0, 1),        // 4: ADD R(3) := R(0) + R(1)
            encode_abc(12, 3, 3, 2),        // 5: ADD R(3) := R(3) + R(2)
            encode_abc(30, 3, 2, 0),        // 6: RETURN R(3)
        ];
        
        // Main function bytecode
        let main_bytecode = vec![
            encode_abx(36, 0, 0),           // 0: CLOSURE R(0), PROTO(0)
            encode_abx(1, 1, 1),            // 1: LOADK R(1), K(1) ; 10
            encode_abx(1, 2, 2),            // 2: LOADK R(2), K(2) ; 20
            encode_abx(1, 3, 3),            // 3: LOADK R(3), K(3) ; 30
            encode_abc(28, 0, 0, 4),        // 4: CALL R(0)(R(1), R(2), R(3))
            encode_abc(30, 0, 2, 0),        // 5: RETURN R(0)
        ];
        
        // Create vararg function prototype
        let vararg_proto = {
            let mut tx = HeapTransaction::new(vm.heap_mut());
            let proto = FunctionProto {
                bytecode: vararg_bytecode,
                constants: vec![Value::Number(0.0)],
                num_params: 0,
                is_vararg: true, // This is a vararg function
                max_stack_size: 5,
                upvalues: vec![],
            };
            let handle = tx.create_function_proto(proto).unwrap();
            tx.commit().unwrap();
            handle
        };
        
        let main_proto = FunctionProto {
            bytecode: main_bytecode,
            constants: vec![
                Value::FunctionProto(vararg_proto),
                Value::Number(10.0),
                Value::Number(20.0),
                Value::Number(30.0),
            ],
            num_params: 0,
            is_vararg: false,
            max_stack_size: 6,
            upvalues: vec![],
        };
        
        // Execute
        let module = ferrous::lua::compiler::CompiledModule {
            main_function: main_proto,
        };
        
        let result = vm.execute_module(&module, &[]).unwrap();
        assert_eq!(result, Value::Number(60.0)); // 10 + 20 + 30 = 60
    }
    
    #[test]
    fn test_extraarg_opcode() {
        let mut vm = LuaVM::new().unwrap();
        
        // Test bytecode for SetList with ExtraArg
        // local t = {}
        // t[1] = 1
        // t[2] = 2
        // return t
        // This uses SetList with C=0 followed by ExtraArg
        
        // Create a table with two elements using SETLIST with C=0 and ExtraArg
        let main_bytecode = vec![
            encode_abc(10, 0, 0, 0),        // 0: NEWTABLE R(0)
            encode_abx(1, 1, 0),            // 1: LOADK R(1), K(0) ; 1
            encode_abx(1, 2, 0),            // 2: LOADK R(2), K(0) ; 1 
            encode_abc(34, 0, 2, 0),        // 3: SETLIST R(0), 2, C=0 ; C=0 means use next instruction
            encode_extra_arg(1),            // 4: EXTRAARG 1 (C value for SETLIST)
            encode_abc(30, 0, 2, 0),        // 5: RETURN R(0)
        ];
        
        let main_proto = FunctionProto {
            bytecode: main_bytecode,
            constants: vec![
                Value::Number(1.0),
            ],
            num_params: 0,
            is_vararg: false,
            max_stack_size: 4,
            upvalues: vec![],
        };
        
        // Execute
        let module = ferrous::lua::compiler::CompiledModule {
            main_function: main_proto,
        };
        
        let result = vm.execute_module(&module, &[]).unwrap();
        
        // Verify the table has correct values
        if let Value::Table(table) = result {
            let mut tx = HeapTransaction::new(vm.heap_mut());
            
            // Check t[1] = 1
            let val1 = tx.read_table_field(table, &Value::Number(1.0)).unwrap();
            assert_eq!(val1, Value::Number(1.0));
            
            // Check t[2] = 1 (we set both to the same value for simplicity)
            let val2 = tx.read_table_field(table, &Value::Number(2.0)).unwrap();
            assert_eq!(val2, Value::Number(1.0));
            
            tx.commit().unwrap();
        } else {
            panic!("Expected table result from SETLIST operation");
        }
    }
    
    #[test]
    fn test_concat_metamethod() {
        let mut vm = LuaVM::new().unwrap();
        
        // First, create a table with __concat metamethod
        let table = {
            let mut tx = HeapTransaction::new(vm.heap_mut());
            
            // Create table
            let table = tx.create_table().unwrap();
            
            // Create metatable
            let metatable = tx.create_table().unwrap();
            
            // Create __concat string key
            let concat_key = tx.create_string("__concat").unwrap();
            
            // Function that concatenates with custom format "[a+b]"
            let concat_fn: ferrous::lua::value::CFunction = |ctx| {
                // Get left and right operands
                let left = ctx.get_arg(0).unwrap();
                let right = ctx.get_arg(1).unwrap();
                
                // Convert to strings
                let left_str = match left {
                    Value::String(handle) => {
                        let mut tx = HeapTransaction::new(ctx.vm_access.heap_mut());
                        let s = tx.get_string_value(handle).unwrap();
                        tx.commit().unwrap();
                        s
                    },
                    _ => format!("{}", left),
                };
                
                let right_str = match right {
                    Value::String(handle) => {
                        let mut tx = HeapTransaction::new(ctx.vm_access.heap_mut());
                        let s = tx.get_string_value(handle).unwrap();
                        tx.commit().unwrap();
                        s
                    },
                    _ => format!("{}", right),
                };
                
                // Create custom result string
                let result = format!("[{}+{}]", left_str, right_str);
                
                // Create string and push to result
                let mut tx = HeapTransaction::new(ctx.vm_access.heap_mut());
                let handle = tx.create_string(&result).unwrap();
                tx.commit().unwrap();
                
                ctx.push_result(Value::String(handle)).unwrap();
                
                Ok(1) // Return 1 value
            };
            
            // Set __concat metamethod
            tx.set_table_field(metatable, Value::String(concat_key), Value::CFunction(concat_fn)).unwrap();
            
            // Set metatable on table
            tx.set_table_metatable(table, Some(metatable)).unwrap();
            
            tx.commit().unwrap();
            
            table
        };
        
        // Now create a test that uses the CONCAT opcode with our table
        
        // Create test strings
        let prefix_str = {
            let mut tx = HeapTransaction::new(vm.heap_mut());
            let handle = tx.create_string("prefix").unwrap();
            tx.commit().unwrap();
            handle
        };
        
        // Create test bytecode:
        // return "prefix" .. table
        let main_bytecode = vec![
            encode_abc(21, 2, 0, 1),    // 0: CONCAT R(2) := R(0) .. R(1)
            encode_abc(30, 2, 2, 0),    // 1: RETURN R(2)
        ];
        
        let main_proto = FunctionProto {
            bytecode: main_bytecode,
            constants: vec![],
            num_params: 0,
            is_vararg: false,
            max_stack_size: 4,
            upvalues: vec![],
        };
        
        // Create a closure and call frame
        let closure = {
            let mut tx = HeapTransaction::new(vm.heap_mut());
            let closure = Closure {
                proto: main_proto,
                upvalues: vec![],
            };
            let handle = tx.create_closure(closure).unwrap();
            tx.commit().unwrap();
            handle
        };
        
        // Set up the VM state for execution
        {
            let mut tx = HeapTransaction::new(vm.heap_mut());
            
            // Push a call frame
            let frame = CallFrame {
                closure,
                pc: 0,
                base_register: 0,
                expected_results: Some(1),
                varargs: None,
            };
            
            tx.push_call_frame(vm.current_thread, frame).unwrap();
            
            // Set up registers
            tx.set_register(vm.current_thread, 0, Value::String(prefix_str)).unwrap();
            tx.set_register(vm.current_thread, 1, Value::Table(table)).unwrap();
            tx.set_register(vm.current_thread, 2, Value::Nil).unwrap(); // For the result
            
            tx.commit().unwrap();
        }
        
        // Execute until complete
        while vm.execution_state == ferrous::lua::vm::ExecutionState::Running {
            vm.step().unwrap();
            // Process any pending operations
            while !vm.pending_operations.is_empty() {
                vm.step().unwrap();
            }
        }
        
        // Get the final result
        let result = {
            let mut tx = HeapTransaction::new(vm.heap_mut());
            let value = tx.read_register(vm.current_thread, 2).unwrap();
            tx.commit().unwrap();
            value
        };
        
        // The metamethod should have formatted a string like "[prefix+table]"
        if let Value::String(handle) = result {
            let value_str = {
                let mut tx = HeapTransaction::new(vm.heap_mut());
                let s = tx.get_string_value(handle).unwrap();
                tx.commit().unwrap();
                s
            };
            
            assert!(value_str.starts_with("[") && value_str.contains("+") && value_str.ends_with("]"),
                   "Expected metamethod formatting, got: {}", value_str);
        } else {
            panic!("Expected string result, got: {:?}", result);
        }
    }
}