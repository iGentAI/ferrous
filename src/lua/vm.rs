//! Lua Virtual Machine Implementation
//! 
//! This module implements the core VM execution engine using a non-recursive
//! state machine approach with transaction-based heap access.

use super::error::{LuaError, LuaResult};
use super::handle::{StringHandle, TableHandle, ClosureHandle, ThreadHandle, UpvalueHandle, FunctionProtoHandle};
use super::heap::LuaHeap;
use super::transaction::HeapTransaction;
use crate::lua::value::{Value, CallFrame, Closure, CFunction, FunctionProto, HashableValue};
use crate::storage::StorageEngine;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Bytecode instruction format (simplified for now)
pub struct Instruction(pub u32);

impl Instruction {
    /// Get the opcode
    pub fn opcode(&self) -> OpCode {
        let op_num = ((self.0) & 0x3F) as u8;
        match op_num {
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
            14 =>  OpCode::Mul,
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
        }
    }
    
    pub fn a(&self) -> u8 {
        ((self.0 >> 6) & 0xFF) as u8
    }
    
    pub fn b(&self) -> u8 {
        ((self.0 >> 23) & 0x1FF) as u8
    }
    
    pub fn c(&self) -> u8 {
        ((self.0 >> 14) & 0x1FF) as u8
    }
    
    pub fn bx(&self) -> u32 {
        (self.0 >> 14) & 0x3FFFF
    }
    
    pub fn sbx(&self) -> i32 {
        (self.bx() as i32) - 131071
    }
}

/// Lua opcodes (subset for initial implementation)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpCode {
    /// MOVE: Copy R(B) to R(A)
    /// A: Target register
    /// B: Source register
    Move,
    
    /// LOADK: Load constant to register
    /// A: Target register
    /// Bx: Constant index in constants table
    /// R(A) := Kst(Bx)
    LoadK,
    
    /// LOADBOOL: Load boolean to register and optionally skip next instruction
    /// A: Target register
    /// B: Boolean value (0 = false, 1 = true)
    /// C: Skip flag (0 = don't skip, 1 = skip next instruction)
    /// R(A) := (Bool)B; if (C) pc++
    LoadBool,
    
    /// LOADNIL: Set multiple registers to nil
    /// A: First register to set
    /// B: Number of registers to set (sets B registers total)
    /// R(A), R(A+1), ..., R(A+B-1) := nil
    LoadNil,
    
    /// GETUPVAL: Load upvalue into register
    /// A: Target register
    /// B: Upvalue index
    /// R(A) := UpValue[B]
    GetUpval,
    
    /// GETGLOBAL: Load global variable into register
    /// A: Target register
    /// Bx: Constant index containing global name
    /// R(A) := Gbl[Kst(Bx)]
    GetGlobal,
    
    /// SETGLOBAL: Set global variable
    /// A: Source register
    /// Bx: Constant index containing global name
    /// Gbl[Kst(Bx)] := R(A)
    SetGlobal,
    
    /// SETUPVAL: Set upvalue
    /// A: Source register
    /// B: Upvalue index
    /// UpValue[B] := R(A)
    SetUpval,
    
    /// GETTABLE: Get table field
    /// A: Target register
    /// B: Table register
    /// C: Key register (or constant if C >= 256)
    /// R(A) := R(B)[RK(C)]
    GetTable,
    
    /// SETTABLE: Set table field
    /// A: Table register
    /// B: Key register (or constant if B >= 256)
    /// C: Value register (or constant if C >= 256)
    /// R(A)[RK(B)] := RK(C)
    SetTable,
    
    /// NEWTABLE: Create new table
    /// A: Target register
    /// B: Array size hint (log2)
    /// C: Hash size hint (log2)
    /// R(A) := {} (size = B,C)
    NewTable,
    
    /// SELF: Prepare for method call
    /// A: Register for function
    /// B: Register with table
    /// C: Key register/constant for method name
    /// R(A+1) := R(B); R(A) := R(B)[RK(C)]
    Self_,
    
    /// SETLIST: Batch set array elements
    /// A: Table register
    /// B: Number of elements to set (0 = all)
    /// C: Block index (starting at 1) * FPF + 1
    /// R(A)[(C-1)*FPF+i] := R(A+i), 1 <= i <= B
    SetList,

    // Arithmetic operations
    Add,        // R(A) := RK(B) + RK(C)
    Sub,        // R(A) := RK(B) - RK(C)
    Mul,        // R(A) := RK(B) * RK(C)
    Div,        // R(A) := RK(B) / RK(C)
    Mod,        // R(A) := RK(B) % RK(C)
    Pow,        // R(A) := RK(B) ^ RK(C)
    Unm,        // R(A) := -R(B)
    Not,        // R(A) := not R(B)
    Len,        // R(A) := length of R(B)
    Concat,     // R(A) := R(B).. ... ..R(C)
    
    /// CLOSURE: Create closure from function prototype
    /// A: Target register for new closure
    /// Bx: Function prototype index
    /// R(A) := closure(KPROTO[Bx])
    Closure,
    
    /// CLOSE: Close all upvalues at or above given register
    /// A: Register threshold
    /// close all upvalues >= R(A)
    Close,
    
    /// JMP: Jump to offset
    /// sBx: Signed offset (added to PC)
    /// PC += sBx
    Jmp,
    
    /// EQ: Equality comparison with conditional skip
    /// A: Expected result (0=false, 1=true)
    /// B: First operand (register or constant)
    /// C: Second operand (register or constant)
    /// if ((RK(B) == RK(C)) ~= A) then pc++
    Eq,
    
    /// LT: Less-than comparison with conditional skip
    /// A: Expected result (0=false, 1=true)
    /// B: First operand (register or constant)
    /// C: Second operand (register or constant)
    /// if ((RK(B) < RK(C)) ~= A) then pc++
    Lt,
    
    /// LE: Less-than-or-equal comparison with conditional skip
    /// A: Expected result (0=false, 1=true)
    /// B: First operand (register or constant)
    /// C: Second operand (register or constant)
    /// if ((RK(B) <= RK(C)) ~= A) then pc++
    Le,
    
    /// TEST: Conditional skip based on register value
    /// A: Value register to test
    /// C: Expected truthiness (0=false, 1=true)
    /// if not (R(A) <=> C) then pc++
    Test,
    
    /// TESTSET: Conditional register set and skip
    /// A: Target register
    /// B: Value register to test
    /// C: Expected truthiness (0=false, 1=true)
    /// if (R(B) <=> C) then R(A) := R(B) else pc++
    TestSet,
    
    /// CALL: Function call
    /// A: Function register
    /// B: Argument count + 1 (B=0: use all values from A+1 to top)
    /// C: Return value count + 1 (C=0: all values returned are saved)
    /// R(A), ..., R(A+C-2) := R(A)(R(A+1), ..., R(A+B-1))
    Call,
    
    /// TAILCALL: Function call with tail call optimization
    /// A: Function register
    /// B: Argument count + 1 (B=0: use all values from A+1 to top)
    /// return R(A)(R(A+1), ..., R(A+B-1))
    TailCall,
    
    /// RETURN: Return values from function
    /// A: First register to return
    /// B: Number of values to return + 1 (B=0: return all values from A to top)
    /// return R(A), ..., R(A+B-2)
    Return,
    
    /// FORPREP: Prepare numeric for loop
    /// A: Index register (R(A) = initial value)
    /// sBx: Offset to jump to loop body
    /// R(A) -= R(A+2); pc += sBx
    ForPrep,
    
    /// FORLOOP: Numeric for loop iteration
    /// A: Index register (R(A) = current value)
    /// sBx: Offset to jump back to loop body
    /// R(A) += R(A+2); if R(A) <?= R(A+1) then { pc+=sBx; R(A+3) = R(A) }
    ForLoop,
    
    /// TFORLOOP: Generic for loop iteration
    /// A: Iterator function register
    /// C: Number of values to return
    /// R(A+3), ... , R(A+3+C) := R(A)(R(A+1), R(A+2)); if !Nil then R(A+2)=R(A+3), pc+=sBx
    TForLoop,
    
    /// VARARG: Load variable arguments
    /// A: Target register for first vararg
    /// B: Number of varargs to load + 1 (B=0: load all)
    /// R(A), R(A+1), ..., R(A+B-2) = vararg
    VarArg,

    /// EXTRAARG: Extra argument for previous instruction
    ExtraArg,   // Extra argument for previous instruction
}

/// Pending operation to be processed by the VM
#[derive(Debug, Clone)]
pub enum PendingOperation {
    /// Function call
    FunctionCall {
        closure: ClosureHandle,
        args: Vec<Value>,
        context: ReturnContext,
    },
    
    /// C function call result
    CFunctionReturn {
        values: Vec<Value>,
        context: ReturnContext,
    },
    
    /// Metamethod call
    MetamethodCall {
        method: StringHandle,
        target: Value,
        args: Vec<Value>,
        context: ReturnContext,
    },
    
    /// Table index operation with metamethod
    TableIndex {
        table: TableHandle,
        key: Value,
        context: ReturnContext,
    },
    
    /// Table newindex operation with metamethod
    TableNewIndex {
        table: TableHandle,
        key: Value,
        value: Value,
    },
    
    /// Arithmetic operation with metamethod
    ArithmeticOp {
        op: ArithmeticOperation,
        left: Value,
        right: Value,
        context: ReturnContext,
    },
    
    /// String concatenation operation
    Concatenation {
        values: Vec<Value>,
        current_index: usize,
        dest_register: u16,
        accumulated: Vec<String>,
    },

    /// Move register values after a certain instruction
    MoveAfterInstruction {
        from_register: usize,
        to_register: usize,
        execution_pc: usize,
    },

    /// Special operation for continuing concatenation after a metamethod call
    ConcatAfterMetamethod {
        values: Vec<Value>,
        current_index: usize,
        dest_register: u16,
        accumulated: Vec<String>,
        result_register: usize,
    },

    /// Special operation for continuing concatenation after a partial concatenation
    ConcatContinuation {
        values: Vec<Value>, 
        dest_register: u16,
    },
    
    /// Special operation to fix register values after a CALL
    /// This is used when a CONCAT result needs to be available both for a CALL and for later use
    PostCallRegisterFix {
        dest_register: u16,
        source_register: u16,
        next_pc: usize,
    },


}

/// Arithmetic operations that may trigger metamethods
#[derive(Debug, Clone, Copy)]
pub enum ArithmeticOperation {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Unm,
}

/// Context for where to return values
#[derive(Debug, Clone)]
pub enum ReturnContext {
    /// Store in register at base + offset
    Register { base: u16, offset: usize },
    
    /// Store as table field
    TableField { table: TableHandle, key: Value },
    
    /// Push to stack
    Stack,
    
    /// Final function result
    FinalResult,
    
    /// Metamethod continuation
    Metamethod { context: crate::lua::metamethod::MetamethodContext },
    
    /// Return from generic for loop iterator
    ForLoop { 
        base: u16,      // Base register of the frame
        a: usize,       // A value from TForLoop instruction
        c: usize,       // C value (number of loop variables)
        pc: usize,      // Original PC for loop continuation
        sbx: i32        // Jump offset for loop continuation
    },
}

/// VM execution state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExecutionState {
    /// Ready to execute
    Ready,
    
    /// Currently executing
    Running,
    
    /// Yielded (coroutine)
    Yielded,
    
    /// Completed successfully
    Completed,
    
    /// Error occurred
    Error,
}

/// Result of executing a single step
#[derive(Debug, Clone)]
pub enum StepResult {
    /// Continue execution
    Continue,
    
    /// Function returned value(s)
    Return(Vec<Value>),
    
    /// Yield (coroutine)
    Yield(Value),
    
    /// Error occurred
    Error(LuaError),
}

/// Execution context for C functions (isolated from VM internals)
pub struct ExecutionContext<'vm> {
    // Stack and argument information
    stack_base: usize,
    arg_count: usize,
    thread: ThreadHandle,
    results_pushed: usize,
    
    // Public handle to VM for controlled access
    pub vm_access: &'vm mut LuaVM,
}

impl<'vm> ExecutionContext<'vm> {
    // Create a new execution context from VM
    fn new(vm: &'vm mut LuaVM, stack_base: usize, arg_count: usize, thread: ThreadHandle) -> Self {
        Self {
            stack_base,
            arg_count,
            thread,
            results_pushed: 0,
            vm_access: vm,
        }
    }
    
    // Get argument count
    pub fn arg_count(&self) -> usize {
        self.arg_count
    }
    
    // Get an argument by index
    pub fn get_arg(&mut self, index: usize) -> LuaResult<Value> {
        if index >= self.arg_count {
            return Err(LuaError::ArgumentError {
                expected: self.arg_count,
                got: index,
            });
        }
        
        // Create a fresh transaction for each operation
        let mut tx = HeapTransaction::new(&mut self.vm_access.heap);
        let value = tx.read_register(self.thread, self.stack_base + index)?;
        tx.commit()?;
        
        Ok(value)
    }
    
    // Get an argument as a string
    pub fn get_arg_str(&mut self, index: usize) -> LuaResult<String> {
        let value = self.get_arg(index)?;
        
        match value {
            Value::String(handle) => {
                // Create a fresh transaction for string access
                let mut tx = HeapTransaction::new(&mut self.vm_access.heap);
                let result = tx.get_string_value(handle)?;
                tx.commit()?;
                Ok(result)
            },
            _ => Err(LuaError::TypeError {
                expected: "string".to_string(),
                got: value.type_name().to_string(),
            }),
        }
    }
    
    // Push a result value
    pub fn push_result(&mut self, value: Value) -> LuaResult<()> {
        // Create a fresh transaction
        let mut tx = HeapTransaction::new(&mut self.vm_access.heap);
        tx.set_register(self.thread, self.stack_base + self.results_pushed, value)?;
        tx.commit()?;
        
        self.results_pushed += 1;
        
        Ok(())
    }
    
    // Get access to the heap for creating strings
    pub fn get_heap(&mut self) -> &mut LuaHeap {
        &mut self.vm_access.heap
    }
    
    // Get the current thread handle
    pub fn get_current_thread(&self) -> LuaResult<ThreadHandle> {
        Ok(self.thread)
    }
    
    // Get the base index for this context
    pub fn get_base_index(&self) -> LuaResult<usize> {
        Ok(self.stack_base)
    }
    
    // Get the number of results pushed by this context
    pub fn get_results_pushed(&self) -> usize {
        self.results_pushed
    }
    
    // Create a string in the heap
    pub fn create_string(&mut self, s: &str) -> LuaResult<StringHandle> {
        let mut tx = HeapTransaction::new(&mut self.vm_access.heap);
        let handle = tx.create_string(s)?;
        tx.commit()?;
        Ok(handle)
    }
    
    // Get a string value from a handle
    pub fn get_string_from_handle(&mut self, handle: StringHandle) -> LuaResult<String> {
        let mut tx = HeapTransaction::new(&mut self.vm_access.heap);
        let value = tx.get_string_value(handle)?;
        tx.commit()?;
        Ok(value)
    }
    
    // Get an argument as a number with proper type checking
    pub fn get_number_arg(&mut self, index: usize) -> LuaResult<f64> {
        let value = self.get_arg(index)?;
        
        match value {
            Value::Number(n) => Ok(n),
            Value::String(handle) => {
                // Try to parse string as number
                let s = self.get_string_from_handle(handle)?;
                match s.trim().parse::<f64>() {
                    Ok(n) => Ok(n),
                    Err(_) => Err(LuaError::TypeError {
                        expected: "number".to_string(),
                        got: "string (not convertible to number)".to_string(),
                    }),
                }
            },
            _ => Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: value.type_name().to_string(),
            }),
        }
    }
    
    // Get an argument as a string, with proper type checking and coercion
    pub fn get_string_arg(&mut self, index: usize) -> LuaResult<String> {
        let value = self.get_arg(index)?;
        
        match value {
            Value::String(handle) => {
                self.get_string_from_handle(handle)
            },
            Value::Number(n) => {
                // Convert number to string
                Ok(n.to_string())
            },
            Value::Boolean(b) => {
                // Convert boolean to string
                Ok(b.to_string())
            },
            Value::Nil => {
                // Nil not allowed for string operations
                Err(LuaError::TypeError {
                    expected: "string".to_string(),
                    got: "nil".to_string(),
                })
            },
            _ => Err(LuaError::TypeError {
                expected: "string".to_string(),
                got: value.type_name().to_string(),
            }),
        }
    }
    
    // Get an argument as a boolean, with proper type handling
    pub fn get_bool_arg(&mut self, index: usize) -> LuaResult<bool> {
        let value = self.get_arg(index)?;
        
        match value {
            Value::Boolean(b) => Ok(b),
            Value::Nil => Ok(false),
            _ => Ok(true),
        }
    }
    
    // Table operations
    pub fn table_next(&mut self, table: TableHandle, key: Value) -> LuaResult<Option<(Value, Value)>> {
        let mut tx = HeapTransaction::new(&mut self.vm_access.heap);
        let result = tx.table_next(table, key)?;
        tx.commit()?;
        Ok(result)
    }
    
    pub fn table_get(&mut self, table: TableHandle, key: Value) -> LuaResult<Value> {
        let mut tx = HeapTransaction::new(&mut self.vm_access.heap);
        let value = tx.read_table_field(table, &key)?;
        tx.commit()?;
        Ok(value)
    }
    
    pub fn table_raw_get(&mut self, table: TableHandle, key: Value) -> LuaResult<Value> {
        // Raw get is same as regular get but without metamethods
        // The transaction layer already provides raw access
        self.table_get(table, key)
    }
    
    pub fn table_raw_set(&mut self, table: TableHandle, key: Value, value: Value) -> LuaResult<()> {
        let mut tx = HeapTransaction::new(&mut self.vm_access.heap);
        tx.set_table_field(table, key, value)?;
        tx.commit()?;
        Ok(())
    }
    
    pub fn table_length(&mut self, table: TableHandle) -> LuaResult<usize> {
        let mut tx = HeapTransaction::new(&mut self.vm_access.heap);
        let table_obj = tx.get_table(table)?;
        
        // Find the length of the array part
        // In Lua, the length is up to the highest numeric index before a nil
        let mut len = 0;
        
        // First check the array part
        let array_len = table_obj.array.len();
        for i in 0..array_len {
            if table_obj.array[i].is_nil() {
                break;
            }
            len = i + 1;
        }
        
        // Check the hash part for numeric indices
        for (k, v) in &table_obj.map {
            if let HashableValue::Number(n) = k {
                let idx = n.0;
                if idx > 0.0 && idx.fract() == 0.0 && !v.is_nil() {
                    let idx_int = idx as usize;
                    if idx_int > len {
                        len = idx_int;
                    }
                }
            }
        }
        
        tx.commit()?;
        Ok(len)
    }
    
    // Metatable operations
    pub fn set_metatable(&mut self, table: TableHandle, metatable: Option<TableHandle>) -> LuaResult<()> {
        let mut tx = HeapTransaction::new(&mut self.vm_access.heap);
        tx.set_table_metatable(table, metatable)?;
        tx.commit()?;
        Ok(())
    }
    
    pub fn get_metatable(&mut self, table: TableHandle) -> LuaResult<Option<TableHandle>> {
        let mut tx = HeapTransaction::new(&mut self.vm_access.heap);
        let mt = tx.get_table_metatable(table)?;
        tx.commit()?;
        Ok(mt)
    }
    
    // Check for a metamethod on a value
    pub fn check_metamethod(&mut self, value: &Value, metamethod: &str) -> Option<Value> {
        let mut tx = HeapTransaction::new(&mut self.vm_access.heap);
        
        // Map metamethod name to type
        let mm_type = match metamethod {
            "__tostring" => crate::lua::metamethod::MetamethodType::ToString,
            "__index" => crate::lua::metamethod::MetamethodType::Index,
            "__newindex" => crate::lua::metamethod::MetamethodType::NewIndex,
            "__add" => crate::lua::metamethod::MetamethodType::Add,
            "__sub" => crate::lua::metamethod::MetamethodType::Sub,
            "__mul" => crate::lua::metamethod::MetamethodType::Mul,
            "__div" => crate::lua::metamethod::MetamethodType::Div,
            "__mod" => crate::lua::metamethod::MetamethodType::Mod,
            "__pow" => crate::lua::metamethod::MetamethodType::Pow,
            "__unm" => crate::lua::metamethod::MetamethodType::Unm,
            "__concat" => crate::lua::metamethod::MetamethodType::Concat,
            "__len" => crate::lua::metamethod::MetamethodType::Len,
            "__eq" => crate::lua::metamethod::MetamethodType::Eq,
            "__lt" => crate::lua::metamethod::MetamethodType::Lt,
            "__le" => crate::lua::metamethod::MetamethodType::Le,
            "__call" => crate::lua::metamethod::MetamethodType::Call,
            _ => return None,
        };
        
        // Resolve the metamethod
        match crate::lua::metamethod::resolve_metamethod(&mut tx, value, mm_type) {
            Ok(mm) => {
                // Commit transaction before returning
                let _ = tx.commit();
                mm
            },
            Err(_) => None,
        }
    }
    
    // Call a metamethod with arguments
    pub fn call_metamethod(&mut self, function: Value, args: Vec<Value>) -> LuaResult<Vec<Value>> {
        match function {
            Value::Closure(closure) => {
                // We need to extract all the necessary data first to avoid aliasing self.vm_access
                let closure_copy = closure;
                let args_copy = args.clone(); // Clone the args to avoid ownership issues
                
                // Execute the closure and return result - use self.vm_access directly
                let result = self.vm_access.execute_function(closure_copy, &args_copy)?;
                Ok(vec![result])
            },
            Value::CFunction(cfunc) => {
                // Call the C function directly with the current context
                // No ownership issues with this branch
                let arg_count = args.len();
                
                // First, move all the arguments to the stack at register positions
                // that won't conflict with the results
                let stack_base = self.stack_base + self.results_pushed;
                
                // Setup arguments
                for (i, arg) in args.iter().enumerate() {
                    // Create a new transaction for each operation to avoid aliasing
                    let mut tx = HeapTransaction::new(&mut self.vm_access.heap);
                    tx.set_register(self.thread, stack_base + i, arg.clone())?;
                    tx.commit()?;
                }
                
                // Now call the C function with a fresh context
                let result_count = cfunc(self)?;
                
                // Collect results
                let mut results = Vec::with_capacity(result_count as usize);
                for i in 0..result_count as usize {
                    let mut tx = HeapTransaction::new(&mut self.vm_access.heap);
                    let value = tx.read_register(self.thread, stack_base + i)?;
                    tx.commit()?;
                    results.push(value);
                }
                
                Ok(results)
            },
            _ => Err(LuaError::TypeError {
                expected: "function".to_string(),
                got: function.type_name().to_string(),
            }),
        }
    }
}

/// Main Lua Virtual Machine
pub struct LuaVM {
    /// Lua heap
    pub(crate) heap: LuaHeap,
    
    /// Current thread handle
    current_thread: ThreadHandle,
    
    /// Queue of pending operations
    pending_operations: VecDeque<PendingOperation>,
    
    /// Current execution state
    execution_state: ExecutionState,
    
    /// Return contexts for active calls
    return_contexts: HashMap<usize, ReturnContext>,
    
    /// Instruction count (for limits)
    instruction_count: u64,
    
    /// Maximum instructions before timeout
    max_instructions: u64,
    
    /// Start time for timeout checking
    start_time: Option<Instant>,
    
    /// Execution timeout
    timeout: Option<Duration>,
    
    /// Script context (Redis integration)
    script_context: Option<crate::lua::ScriptContext>,
}

impl LuaVM {
    /// Get mutable access to the heap for transaction creation
    pub fn heap_mut(&mut self) -> &mut LuaHeap {
        &mut self.heap
    }

    /// Load a function prototype into the heap
    fn load_prototype(
        &mut self,
        tx: &mut HeapTransaction,
        proto: &crate::lua::value::FunctionProto,
        string_handles: &[StringHandle],
    ) -> LuaResult<FunctionProtoHandle> {
        // Process constants
        let mut constants = Vec::with_capacity(proto.constants.len());
        for constant in &proto.constants {
            let new_const = match constant {
                Value::Nil => Value::Nil,
                Value::Boolean(b) => Value::Boolean(*b),
                Value::Number(n) => Value::Number(*n),
                Value::String(_) => {
                    Value::Nil
                },
                Value::FunctionProto(_) => {
                    Value::Nil
                },
                _ => {
                    // Other value types shouldn't be in constants
                    Value::Nil
                }
            };
            constants.push(new_const);
        }
        
        // Create a new function prototype
        let new_proto = crate::lua::value::FunctionProto {
            bytecode: proto.bytecode.clone(),
            constants,
            num_params: proto.num_params,
            is_vararg: proto.is_vararg,
            max_stack_size: proto.max_stack_size,
            upvalues: proto.upvalues.clone(),
        };
        
        tx.create_function_proto(new_proto)
    }
    

    
    /// Handle C function call for TForLoop following the pattern from handle_c_function_call
    fn handle_tforloop_c_function(
        &mut self,
        func: CFunction,
        args: Vec<Value>,
        base_register: u16,
        a: usize,
        c: usize,
        original_pc: usize,
        sbx: i32,
        thread_handle: ThreadHandle,
    ) -> LuaResult<StepResult> {
        // TForLoop registers:
        // R(A) = iterator function
        // R(A+1) = state
        // R(A+2) = control
        // R(A+3)...R(A+3+C-1) = loop variables
        
        // Stack base is where the arguments start (R(A+1))
        let stack_base = base_register as usize + a + 1;
        
        // Create execution context
        let mut ctx = ExecutionContext::new(self, stack_base, args.len(), thread_handle);
        
        // Call C function
        let result_count = match func(&mut ctx) {
            Ok(count) => count as usize,
            Err(e) => return Err(e),
        };
        
        // Process results with a new transaction
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Results are placed at stack_base, stack_base+1, etc.
        // First result is where we placed state (R(A+1))
        let result_start = stack_base;
        
        // Check if first result is nil (end of iteration)
        let first_result = if result_count > 0 {
            tx.read_register(thread_handle, result_start)?
        } else {
            Value::Nil
        };
        
        if !first_result.is_nil() {
            // Loop continues
            
            // Update control variable (R(A+2)) with first result
            tx.set_register(thread_handle, base_register as usize + a + 2, first_result.clone())?;
            
            // Move results to loop variables (R(A+3)...R(A+3+C-1))
            for i in 0..c {
                let value = if i < result_count {
                    tx.read_register(thread_handle, result_start + i)?
                } else {
                    Value::Nil
                };
                
                tx.set_register(thread_handle, base_register as usize + a + 3 + i, value)?;
            }
            
            // In Lua 5.1, we don't modify the PC further if iteration continues
            // The next instruction (usually a JMP) will be executed normally
        } else {
            // Loop is done - skip the next instruction (usually a JMP back)
            tx.increment_pc(thread_handle)?;
        }
        
        tx.commit()?;
        Ok(StepResult::Continue)
    }
    
    /// Example method showing proper handle validation with ValidScope
    /// This demonstrates the recommended pattern for operations on multiple handles
    pub fn example_table_operation(&mut self, table: TableHandle, key: Value) -> LuaResult<Value> {
        // Create a transaction
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Create local variables to store results outside the scope
        let mut result = Value::Nil;
        
        // Perform operations within a scope
        {
            // Create a validation scope
            let mut scope = tx.validation_scope();
            
            // Validate and use the table handle
            let temp_result = scope.use_handle(&table, |scope| {
                // The handle is now validated, we can use it safely
                let tx = scope.transaction();
                
                // Read the table field
                tx.read_table_field(table, &key)
            })?;
            
            result = temp_result;
            
            // If we need to use multiple handles, we can validate them all in sequence
            let metatable_opt = {
                // Get the table's metatable
                let tx = scope.transaction();
                tx.get_table_metatable(table)?
            };
            
            // If metatable exists, we need to validate and use it
            if let Some(metatable) = metatable_opt {
                scope.use_handle(&metatable, |scope| {
                    // Use the metatable...
                    let tx = scope.transaction();
                    
                    // For example, read a metamethod
                    let mm_key = Value::String(tx.create_string("__index")?);
                    let _mm_value = tx.read_table_field(metatable, &mm_key)?;
                    
                    Ok(())
                })?;
            }
        }
        
        // Now we can commit the transaction
        tx.commit()?;
        
        Ok(result)
    }
    
    /// Create a new VM instance
    pub fn new() -> LuaResult<Self> {
        let heap = LuaHeap::new()?;
        let main_thread = heap.main_thread()?;
        
        Ok(LuaVM {
            heap,
            current_thread: main_thread,
            pending_operations: VecDeque::new(),
            execution_state: ExecutionState::Ready,
            return_contexts: HashMap::new(),
            instruction_count: 0,
            max_instructions: 1_000_000,
            start_time: None,
            timeout: None,
            script_context: None,
        })
    }
    
    /// Set the script execution context
    pub fn set_context(&mut self, context: crate::lua::ScriptContext) -> LuaResult<()> {
        self.timeout = Some(context.timeout);
        self.script_context = Some(context);
        
        // TODO: Setup KEYS and ARGV tables
        
        Ok(())
    }
    
    /// Evaluate a script and return the result
    pub fn eval_script(&mut self, _script: &str) -> LuaResult<Value> {
        // TODO: Compile script
        // For now, return a placeholder
        
        self.start_time = Some(Instant::now());
        
        // TODO: Execute compiled script
        
        // Return a placeholder string for now
        let mut tx = HeapTransaction::new(&mut self.heap);
        let handle = tx.create_string("placeholder result")?;
        tx.commit()?;
        
        Ok(Value::String(handle))
    }
    
    /// Execute a function with arguments
    pub fn execute_function(&mut self, closure: ClosureHandle, args: &[Value]) -> LuaResult<Value> {
        // Record initial call depth
        let initial_depth = self.get_call_depth()?;
        
        // Queue initial function call
        self.pending_operations.push_back(PendingOperation::FunctionCall {
            closure,
            args: args.to_vec(),
            context: ReturnContext::FinalResult,
        });
        
        // Set execution state
        self.execution_state = ExecutionState::Running;
        self.start_time = Some(Instant::now());
        
        // Initialize result
        let mut final_result = Value::Nil;
        
        // Main execution loop - NO RECURSION
        loop {
            // Check termination conditions
            if self.should_kill() {
                return Err(LuaError::ScriptKilled);
            }
            
            if self.check_timeout() {
                return Err(LuaError::Timeout);
            }
            
            if self.instruction_count > self.max_instructions {
                return Err(LuaError::InstructionLimitExceeded);
            }
            
            // Process pending operations first
            if !self.pending_operations.is_empty() {
                let op = self.pending_operations.pop_front().unwrap();
                match self.process_pending_operation(op)? {
                    StepResult::Continue => continue,
                    StepResult::Return(values) => {
                        if !values.is_empty() {
                            final_result = values[0].clone();
                            
                            println!("DEBUG VM: Function returned with value type: {}", final_result.type_name());
                            if let Value::Table(handle) = &final_result {
                                println!("DEBUG VM: Returned table with handle: {:?}", handle);
                            }
                        }
                        
                        // Check if we're back to initial depth
                        if self.get_call_depth()? <= initial_depth {
                            println!("DEBUG VM: Back to initial depth, returning final result: {:?}", final_result);
                            break;
                        }
                    }
                    StepResult::Yield(_) => {
                        return Err(LuaError::NotImplemented("coroutines".to_string()));
                    }
                    StepResult::Error(e) => {
                        return Err(e);
                    }
                }
            } else {
                // Execute next instruction
                match self.step()? {
                    StepResult::Continue => continue,
                    StepResult::Return(values) => {
                        if !values.is_empty() {
                            final_result = values[0].clone();
                            
                            println!("DEBUG VM: Instruction returned with value type: {}", final_result.type_name());
                            if let Value::Table(handle) = &final_result {
                                println!("DEBUG VM: Returned table with handle: {:?}", handle);
                            }
                        }
                        
                        // Check if we're back to initial depth
                        if self.get_call_depth()? <= initial_depth {
                            println!("DEBUG VM: Back to initial depth, returning final result: {:?}", final_result);
                            break;
                        }
                    }
                    StepResult::Yield(_) => {
                        return Err(LuaError::NotImplemented("coroutines".to_string()));
                    }
                    StepResult::Error(e) => {
                        return Err(e);
                    }
                }
            }
        }
        
        self.execution_state = ExecutionState::Completed;
        
        println!("DEBUG VM: Final result value type: {}", final_result.type_name());
        
        // Return the result directly without any additional processing or conversions
        Ok(final_result)
    }
    
    fn step(&mut self) -> LuaResult<StepResult> {
        // Increment instruction count
        self.instruction_count += 1;
        
        // Create transaction
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Get current frame
        let frame = tx.get_current_frame(self.current_thread)?;
        
        // Get instruction
        let instr = tx.get_instruction(frame.closure, frame.pc)?;
        let instruction = Instruction(instr);
        
        // Execute instruction inline
        let opcode = instruction.opcode();
        let a = instruction.a() as usize;
        let b = instruction.b() as usize;
        let c = instruction.c() as usize;
        
        // Default: increment PC unless instruction handles it
        let mut should_increment_pc = true;
        
        let result = match opcode {
            OpCode::Move => {
                // R(A) := R(B)
                let base = frame.base_register as usize;
                let value = tx.read_register(self.current_thread, base + b)?;
                tx.set_register(self.current_thread, base + a, value)?;
                StepResult::Continue
            }
            
            OpCode::LoadK => {
                // R(A) := Kst(Bx)
                let base = frame.base_register as usize;
                
                // Inline get_constant to avoid self-borrowing
                let closure_obj = tx.get_closure(frame.closure)?;
                let bx = instruction.bx() as usize;
                let constant = closure_obj.proto.constants.get(bx)
                    .cloned()
                    .ok_or_else(|| LuaError::RuntimeError(format!(
                        "Constant index {} out of bounds",
                        bx
                    )))?;
                
                tx.set_register(self.current_thread, base + a, constant)?;
                StepResult::Continue
            }
            
            OpCode::LoadBool => {
                // R(A) := (Bool)B; if (C) pc++
                let base = frame.base_register as usize;
                let value = Value::Boolean(b != 0);
                tx.set_register(self.current_thread, base + a, value)?;
                
                if c != 0 {
                    // Skip next instruction
                    tx.increment_pc(self.current_thread)?;
                }
                
                StepResult::Continue
            }
            
            OpCode::LoadNil => {
                // R(A), R(A+1), ..., R(A+B-1) := nil
                // Note: Lua 5.1 spec specifies this sets B registers to nil (not B+1)
                let base = frame.base_register as usize;
                for i in 0..b {
                    tx.set_register(self.current_thread, base + a + i, Value::Nil)?;
                }
                StepResult::Continue
            }
            
            OpCode::GetUpval => {
                // R(A) := UpValue[B]
                let base = frame.base_register as usize;
                
                // Phase 1: Extract all needed information
                // Get the upvalue handle and check if it's valid
                let upvalue_info = {
                    let closure_obj = tx.get_closure(frame.closure)?;
                    
                    // Check upvalue bounds
                    if b >= closure_obj.upvalues.len() {
                        return Err(LuaError::RuntimeError(format!(
                            "Upvalue index {} out of bounds (upvalues: {})",
                            b, closure_obj.upvalues.len()
                        )));
                    }
                    
                    // Get the upvalue handle
                    let upvalue_handle = closure_obj.upvalues[b];
                    
                    // Get the upvalue
                    let upvalue = tx.get_upvalue(upvalue_handle)?;
                    
                    // Extract the info we need
                    match upvalue {
                        crate::lua::value::Upvalue { stack_index: Some(idx), value: None } => {
                            // Open upvalue
                            (true, *idx, None)
                        },
                        crate::lua::value::Upvalue { stack_index: None, value: Some(val) } => {
                            // Closed upvalue
                            (false, 0, Some(val.clone()))
                        },
                        _ => {
                            // Invalid state
                            return Err(LuaError::RuntimeError("Invalid upvalue state".to_string()));
                        }
                    }
                };
                
                // Phase 2: Process the information
                let value = match upvalue_info {
                    (true, idx, _) => {
                        // Open upvalue - read from thread stack
                        tx.read_register(self.current_thread, idx)?
                    },
                    (false, _, Some(val)) => {
                        // Closed upvalue - use stored value
                        val
                    },
                    _ => {
                        // Invalid state (shouldn't happen due to pattern match above)
                        return Err(LuaError::InternalError("Invalid upvalue state in phase 2".to_string()));
                    }
                };
                
                // Set register A to upvalue value
                tx.set_register(self.current_thread, base + a, value)?;
                
                StepResult::Continue
            },
            
            OpCode::GetGlobal => {
                // R(A) := Gbl[Kst(Bx)]
                let base = frame.base_register as usize;
                
                // Get the global name from the constant pool
                let bx = instruction.bx() as usize;
                let closure_obj = tx.get_closure(frame.closure)?;
                
                let name_value = closure_obj.proto.constants.get(bx)
                    .cloned()
                    .ok_or_else(|| LuaError::RuntimeError(format!(
                        "Constant index {} out of bounds", bx
                    )))?;
                
                // The name must be a string
                let name_handle = match name_value {
                    Value::String(handle) => handle,
                    _ => {
                        tx.commit()?;
                        return Err(LuaError::RuntimeError("Global name must be a string".to_string()));
                    }
                };
                
                // Add diagnostics for important lookups
                let name_str = tx.get_string_value(name_handle)?;
                if name_str == "print" || name_str == "type" || name_str == "tostring" || name_str == "pairs" {
                    println!("DEBUG LOOKUP: Looking up global '{}' with handle {:?}", name_str, name_handle);
                }
                
                // Get the globals table
                let globals_table = tx.get_globals_table()?;
                
                // Get the value from the globals table
                let value = tx.read_table_field(globals_table, &Value::String(name_handle))?;
                
                // Add diagnostics for result
                if name_str == "print" || name_str == "type" || name_str == "tostring" || name_str == "pairs" {
                    println!("DEBUG LOOKUP: Result for '{}': {:?} ({})", 
                             name_str, value, value.type_name());
                }
                
                // Store the value in register A
                tx.set_register(self.current_thread, base + a, value)?;
                
                StepResult::Continue
            }
            
            OpCode::SetGlobal => {
                // Gbl[Kst(Bx)] := R(A)
                let base = frame.base_register as usize;
                
                // Get the global name from the constant pool
                let bx = instruction.bx() as usize;
                let closure_obj = tx.get_closure(frame.closure)?;
                
                let name_value = closure_obj.proto.constants.get(bx)
                    .cloned()
                    .ok_or_else(|| LuaError::RuntimeError(format!(
                        "Constant index {} out of bounds", bx
                    )))?;
                
                // The name must be a string
                let name_handle = match name_value {
                    Value::String(handle) => handle,
                    _ => {
                        tx.commit()?;
                        return Err(LuaError::RuntimeError("Global name must be a string".to_string()));
                    }
                };
                
                // Get the value to store
                let value = tx.read_register(self.current_thread, base + a)?;
                
                // Get the globals table
                let globals_table = tx.get_globals_table()?;
                
                // Set the value in the globals table
                tx.set_table_field(globals_table, Value::String(name_handle), value)?;
                
                StepResult::Continue
            }
            
            OpCode::SetUpval => {
                // UpValue[B] := R(A)
                let base = frame.base_register as usize;
                
                // Phase 1: Extract all needed information
                // Get the value and upvalue details
                let (upvalue_handle, is_open, stack_idx, value) = {
                    // Get the value to store
                    let value = tx.read_register(self.current_thread, base + a)?;
                    
                    // Get the closure
                    let closure_obj = tx.get_closure(frame.closure)?;
                    
                    // Check upvalue bounds
                    if b >= closure_obj.upvalues.len() {
                        return Err(LuaError::RuntimeError(format!(
                            "Upvalue index {} out of bounds (upvalues: {})",
                            b, closure_obj.upvalues.len()
                        )));
                    }
                    
                    // Get the upvalue handle
                    let upvalue_handle = closure_obj.upvalues[b];
                    
                    // Get the upvalue state
                    let upvalue = tx.get_upvalue(upvalue_handle)?;
                    let is_open = upvalue.stack_index.is_some();
                    let stack_idx = upvalue.stack_index;
                    
                    (upvalue_handle, is_open, stack_idx, value)
                };
                
                // Phase 2: Update the upvalue
                if is_open {
                    if let Some(idx) = stack_idx {
                        // Open upvalue - write to thread stack
                        tx.set_register(self.current_thread, idx, value)?;
                    }
                } else {
                    // Closed upvalue - update stored value
                    tx.set_upvalue(upvalue_handle, value)?;
                }
                
                StepResult::Continue
            },
            
            OpCode::Return => {
                // return R(A), ..., R(A+B-2)
                let base = frame.base_register as usize;
                let mut values = Vec::new();
                
                println!("DEBUG RETURN: Processing Return opcode with A={}, B={}", a, b);
                
                if b == 0 {
                    // Return all values from R(A) to top
                    let top = tx.get_stack_top(self.current_thread)?;
                    println!("DEBUG RETURN: B=0, returning all values from R({}) to top ({})", base + a, top);
                    for i in a..=(top - base) {
                        let val = tx.read_register(self.current_thread, base + i)?;
                        println!("DEBUG RETURN: Adding return value [{}]: {:?} ({})", i, val, val.type_name());
                        values.push(val);
                    }
                } else {
                    // Return B-1 values
                    println!("DEBUG RETURN: B={}, returning {} values", b, b - 1);
                    for i in 0..(b - 1) {
                        let val = tx.read_register(self.current_thread, base + a + i)?;
                        println!("DEBUG RETURN: Adding return value [{}]: {:?} ({})", i, val, val.type_name());
                        values.push(val);
                    }
                }
                
                println!("DEBUG RETURN: Collected {} return values", values.len());
                if !values.is_empty() {
                    println!("DEBUG RETURN: First return value type: {}", values[0].type_name());
                    
                    if let Value::Table(handle) = &values[0] {
                        println!("DEBUG RETURN: Returning table with handle: {:?}", handle);
                    }
                }
                
                // Pop call frame
                tx.pop_call_frame(self.current_thread)?;
                
                // Don't increment PC - we're returning
                should_increment_pc = false;
                
                StepResult::Return(values)
            }
            
            OpCode::Call => {
                // R(A), ..., R(A+C-2) := R(A)(R(A+1), ..., R(A+B-1))
                let base = frame.base_register as usize;
                
                // CRITICAL: Safely copy the function and arguments to avoid register conflicts
                // This is necessary because registers can be overwritten during execution
                let func = tx.read_register(self.current_thread, base + a)?;
                
                println!("DEBUG CALL: Processing CALL instruction A={}, B={}, C={}", a, b, c);
                println!("DEBUG CALL: Function in register {}: {:?}", base + a, func);
                
                // Calculate argument count
                let arg_count = if b == 0 {
                    // Use all values from R(A+1) to top
                    let top = tx.get_stack_top(self.current_thread)?;
                    top - base - a - 1
                } else {
                    b - 1
                };
                
                println!("DEBUG CALL: Gathering {} arguments", arg_count);
                
                // Copy all arguments to a separate array to ensure they're not modified
                let mut args = Vec::with_capacity(arg_count);
                for i in 0..arg_count {
                    let arg_idx = base + a + 1 + i;
                    let arg = tx.read_register(self.current_thread, arg_idx)?;
                    println!("DEBUG CALL: Arg {}: {:?}", i, arg);
                    args.push(arg);
                }
                
                // Process based on function type - using our safely copied values
                match func {
                    Value::Closure(closure) => {
                        // Queue function call for later execution
                        println!("DEBUG CALL: Queueing closure call with {} args", args.len());
                        tx.queue_operation(PendingOperation::FunctionCall {
                            closure,
                            args,  // This is our safe copy
                            context: ReturnContext::Register {
                                base: frame.base_register,
                                offset: a,
                            },
                        })?;
                        
                        StepResult::Continue
                    },
                    Value::CFunction(cfunc) => {
                        println!("DEBUG CALL: Executing C function with {} args", args.len());
                        // For C functions, we need special handling
                        // First increment the PC
                        tx.increment_pc(self.current_thread)?;
                        
                        // Commit the transaction
                        tx.commit()?;
                        
                        // Extract arguments we need to avoid self-borrowing conflicts
                        let func_copy = cfunc;
                        let args_copy = args;
                        let base_register = frame.base_register;
                        let register_a = a;
                        let thread_handle = self.current_thread;
                        
                        // Call the handler with clean borrows
                        return self.handle_c_function_call(
                            func_copy,
                            args_copy,
                            base_register,
                            register_a,
                            thread_handle
                        );
                    },
                    _ => {
                        println!("ERROR: Expected function in register {} but got {:?}", base + a, func);
                        // Not a function - return error after committing
                        tx.commit()?;
                        return Err(LuaError::TypeError { 
                            expected: "function".to_string(), 
                            got: func.type_name().to_string(),
                        });
                    },
                }
            }
            
            OpCode::TailCall => {
                // return R(A)(R(A+1), ..., R(A+B-1))
                // Tail calls are used to optimize tail recursion
                let base = frame.base_register as usize;
                let func = tx.read_register(self.current_thread, base + a)?;
                
                // Gather arguments
                let arg_count = if b == 0 {
                    // Use all values from R(A+1) to top
                    let top = tx.get_stack_top(self.current_thread)?;
                    top - base - a
                } else {
                    b - 1
                };
                
                let mut args = Vec::with_capacity(arg_count);
                for i in 0..arg_count {
                    args.push(tx.read_register(self.current_thread, base + a + 1 + i)?);
                }
                
                // Process based on function type
                match func {
                    Value::Closure(closure) => {
                        // Pop the current frame before pushing a new one
                        tx.pop_call_frame(self.current_thread)?;
                        
                        // Queue function call
                        tx.queue_operation(PendingOperation::FunctionCall {
                            closure,
                            args,
                            context: ReturnContext::FinalResult,
                        })?;
                        
                        // Don't increment PC - we're returning
                        should_increment_pc = false;
                        
                        StepResult::Continue
                    },
                    Value::CFunction(cfunc) => {
                        // For C functions, we could implement a similar optimization,
                        // but for simplicity, just handle it like a normal call followed by return
                        // This isn't completely correct from a tail call optimization standpoint,
                        // but it's adequate for initial implementation
                        
                        // Pop the current frame
                        tx.pop_call_frame(self.current_thread)?;
                        
                        // Commit the transaction
                        tx.commit()?;
                        
                        // Extract arguments we need to avoid self-borrowing conflicts
                        let func_copy = cfunc; // CFunction implements Copy
                        let args_copy = args; // Clone the args
                        let thread_handle = self.current_thread;
                        
                        // Call the C function directly - result will be returned as final result
                        let mut ctx = ExecutionContext::new(self, 0, args_copy.len(), thread_handle);
                        
                        match func_copy(&mut ctx) {
                            Ok(count) => {
                                // For tail calls, we just return the values directly
                                let mut results = Vec::new();
                                let mut tx = HeapTransaction::new(&mut self.heap);
                                
                                for i in 0..count as usize {
                                    if let Ok(value) = tx.read_register(thread_handle, i) {
                                        results.push(value);
                                    }
                                }
                                
                                tx.commit()?;
                                
                                // Return the results directly
                                return Ok(StepResult::Return(results));
                            },
                            Err(e) => return Ok(StepResult::Error(e)),
                        }
                    },
                    _ => {
                        // Not a function - return error after committing
                        tx.commit()?;
                        return Err(LuaError::TypeError { 
                            expected: "function".to_string(), 
                            got: func.type_name().to_string(),
                        });
                    },
                }
            },

            OpCode::GetTable => {
                // R(A) := R(B)[RK(C)]
                let base = frame.base_register as usize;
                let table_val = tx.read_register(self.current_thread, base + b)?;
                let key_val = if c & 0x100 != 0 {
                    // RK(C) is constant
                    let const_idx = c & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    closure_obj.proto.constants.get(const_idx)
                        .cloned()
                        .ok_or_else(|| LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )))?
                } else {
                    // RK(C) is register
                    tx.read_register(self.current_thread, base + c)?
                };
                
                match table_val {
                    Value::Table(table) => {
                        // Try direct table access first
                        let result = tx.read_table_field(table, &key_val)?;
                        
                        if !result.is_nil() {
                            // Found value, store it
                            tx.set_register(self.current_thread, base + a, result)?;
                            StepResult::Continue
                        } else {
                            // Need to check for __index metamethod
                            if let Some(mm) = crate::lua::metamethod::resolve_metamethod(
                                &mut tx, &Value::Table(table), crate::lua::metamethod::MetamethodType::Index
                            )? {
                                match mm {
                                    Value::Table(index_table) => {
                                        // __index is a table, look up the key in it
                                        let mm_result = tx.read_table_field(index_table, &key_val)?;
                                        tx.set_register(self.current_thread, base + a, mm_result)?;
                                        StepResult::Continue
                                    }
                                    Value::Closure(_) | Value::CFunction(_) => {
                                        // __index is a function, queue metamethod call
                                        let method_name = tx.create_string("__index")?;
                                        tx.queue_operation(PendingOperation::MetamethodCall {
                                            method: method_name,
                                            target: Value::Table(table),
                                            args: vec![Value::Table(table), key_val],
                                            context: ReturnContext::Register { base: frame.base_register, offset: a },
                                        })?;
                                        StepResult::Continue
                                    }
                                    _ => {
                                        // Invalid __index metamethod
                                        tx.set_register(self.current_thread, base + a, Value::Nil)?;
                                        StepResult::Continue
                                    }
                                }
                            } else {
                                // No metamethod, result is nil
                                tx.set_register(self.current_thread, base + a, Value::Nil)?;
                                StepResult::Continue
                            }
                        }
                    }
                    _ => {
                        // Not a table - check for metamethod
                        if let Some(mm) = crate::lua::metamethod::resolve_metamethod(
                            &mut tx, &table_val, crate::lua::metamethod::MetamethodType::Index
                        )? {
                            // Queue metamethod call
                            let method_name = tx.create_string("__index")?;
                            let table_val_clone = table_val.clone();
                            tx.queue_operation(PendingOperation::MetamethodCall {
                                method: method_name,
                                target: table_val_clone.clone(),
                                args: vec![table_val_clone, key_val],
                                context: ReturnContext::Register { base: frame.base_register, offset: a },
                            })?;
                            StepResult::Continue
                        } else {
                            // No metamethod, error
                            tx.commit()?;
                            return Err(LuaError::TypeError { 
                                expected: "table".to_string(), 
                                got: table_val.type_name().to_string(),
                            });
                        }
                    }
                }
            }
            
            OpCode::SetTable => {
                // R(A)[RK(B)] := RK(C)
                let base = frame.base_register as usize;
                let table_val = tx.read_register(self.current_thread, base + a)?;
                
                let key_val = if b & 0x100 != 0 {
                    // RK(B) is constant
                    let const_idx = b & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    closure_obj.proto.constants.get(const_idx)
                        .cloned()
                        .ok_or_else(|| LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )))?
                } else {
                    // RK(B) is register
                    tx.read_register(self.current_thread, base + b)?
                };
                
                let value = if c & 0x100 != 0 {
                    // RK(C) is constant
                    let const_idx = c & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    closure_obj.proto.constants.get(const_idx)
                        .cloned()
                        .ok_or_else(|| LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )))?
                } else {
                    // RK(C) is register
                    tx.read_register(self.current_thread, base + c)?
                };
                
                match table_val {
                    Value::Table(table) => {
                        // Check if field already exists
                        let existing = tx.read_table_field(table, &key_val)?;
                        
                        if !existing.is_nil() || value.is_nil() {
                            // Field exists or we're setting nil, do direct assignment
                            tx.set_table_field(table, key_val, value)?;
                            StepResult::Continue
                        } else {
                            // Field doesn't exist, check for __newindex
                            if let Some(mm) = crate::lua::metamethod::resolve_metamethod(
                                &mut tx, &Value::Table(table), crate::lua::metamethod::MetamethodType::NewIndex
                            )? {
                                match mm {
                                    Value::Table(newindex_table) => {
                                        // __newindex is a table, set the field there
                                        tx.set_table_field(newindex_table, key_val, value)?;
                                        StepResult::Continue
                                    }
                                    Value::Closure(_) | Value::CFunction(_) => {
                                        // __newindex is a function, queue metamethod call
                                        let method_name = tx.create_string("__newindex")?;
                                        tx.queue_operation(PendingOperation::MetamethodCall {
                                            method: method_name,
                                            target: Value::Table(table),
                                            args: vec![Value::Table(table), key_val, value],
                                            context: ReturnContext::Stack, // __newindex doesn't return values
                                        })?;
                                        StepResult::Continue
                                    }
                                    _ => {
                                        // Invalid __newindex, do normal assignment
                                        tx.set_table_field(table, key_val, value)?;
                                        StepResult::Continue
                                    }
                                }
                            } else {
                                // No metamethod, do normal assignment
                                tx.set_table_field(table, key_val, value)?;
                                StepResult::Continue
                            }
                        }
                    }
                    _ => {
                        // Not a table - error (no metamethod for non-tables in Lua 5.1)
                        tx.commit()?;
                        return Err(LuaError::TypeError { 
                            expected: "table".to_string(), 
                            got: table_val.type_name().to_string(),
                        });
                    }
                }
            }
            
            OpCode::Add => {
                // R(A) := RK(B) + RK(C)
                let base = frame.base_register as usize;
                
                // Get operands
                let left = if b & 0x100 != 0 {
                    // RK(B) is constant
                    let const_idx = b & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    closure_obj.proto.constants.get(const_idx)
                        .cloned()
                        .ok_or_else(|| LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )))?
                } else {
                    // RK(B) is register
                    tx.read_register(self.current_thread, base + b)?
                };
                
                let right = if c & 0x100 != 0 {
                    // RK(C) is constant
                    let const_idx = c & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    closure_obj.proto.constants.get(const_idx)
                        .cloned()
                        .ok_or_else(|| LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )))?
                } else {
                    // RK(C) is register
                    tx.read_register(self.current_thread, base + c)?
                };
                
                // Try numeric addition first
                if let Some((l, r)) = crate::lua::metamethod::can_coerce_arithmetic(&mut tx, &left, &right)? {
                    // Direct numeric addition
                    tx.set_register(self.current_thread, base + a, Value::Number(l + r))?;
                    StepResult::Continue
                } else {
                    // Try metamethod
                    if let Some((mm, _swapped)) = crate::lua::metamethod::resolve_binary_metamethod(
                        &mut tx, &left, &right, crate::lua::metamethod::MetamethodType::Add
                    )? {
                        // Queue metamethod call
                        let method_name = tx.create_string("__add")?;
                        tx.queue_operation(PendingOperation::MetamethodCall {
                            method: method_name,
                            target: left.clone(),
                            args: vec![left, right],
                            context: ReturnContext::Register { base: frame.base_register, offset: a },
                        })?;
                        StepResult::Continue
                    } else {
                        // Error - cannot add
                        tx.commit()?;
                        return Err(LuaError::TypeError {
                            expected: "number".to_string(),
                            got: format!("{} and {}", left.type_name(), right.type_name()),
                        });
                    }
                }
            }
            
            OpCode::Sub => {
                // R(A) := RK(B) - RK(C)
                let base = frame.base_register as usize;
                
                // Get operands
                let left = if b & 0x100 != 0 {
                    let const_idx = b & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    closure_obj.proto.constants.get(const_idx)
                        .cloned()
                        .ok_or_else(|| LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )))?
                } else {
                    tx.read_register(self.current_thread, base + b)?
                };
                
                let right = if c & 0x100 != 0 {
                    let const_idx = c & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    closure_obj.proto.constants.get(const_idx)
                        .cloned()
                        .ok_or_else(|| LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )))?
                } else {
                    tx.read_register(self.current_thread, base + c)?
                };
                
                // Try numeric subtraction first
                if let Some((l, r)) = crate::lua::metamethod::can_coerce_arithmetic(&mut tx, &left, &right)? {
                    tx.set_register(self.current_thread, base + a, Value::Number(l - r))?;
                    StepResult::Continue
                } else {
                    // Try metamethod
                    if let Some((_mm, _swapped)) = crate::lua::metamethod::resolve_binary_metamethod(
                        &mut tx, &left, &right, crate::lua::metamethod::MetamethodType::Sub
                    )? {
                        // Queue metamethod call
                        let method_name = tx.create_string("__sub")?;
                        tx.queue_operation(PendingOperation::MetamethodCall {
                            method: method_name,
                            target: left.clone(),
                            args: vec![left, right],
                            context: ReturnContext::Register { base: frame.base_register, offset: a },
                        })?;
                        StepResult::Continue
                    } else {
                        tx.commit()?;
                        return Err(LuaError::TypeError {
                            expected: "number".to_string(),
                            got: format!("{} and {}", left.type_name(), right.type_name()),
                        });
                    }
                }
            }
            
            OpCode::Mul => {
                // R(A) := RK(B) * RK(C)
                let base = frame.base_register as usize;
                
                // Get operands
                let left = if b & 0x100 != 0 {
                    let const_idx = b & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    closure_obj.proto.constants.get(const_idx)
                        .cloned()
                        .ok_or_else(|| LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )))?
                } else {
                    tx.read_register(self.current_thread, base + b)?
                };
                
                let right = if c & 0x100 != 0 {
                    let const_idx = c & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    closure_obj.proto.constants.get(const_idx)
                        .cloned()
                        .ok_or_else(|| LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )))?
                } else {
                    tx.read_register(self.current_thread, base + c)?
                };
                
                // Try numeric multiplication first
                if let Some((l, r)) = crate::lua::metamethod::can_coerce_arithmetic(&mut tx, &left, &right)? {
                    tx.set_register(self.current_thread, base + a, Value::Number(l * r))?;
                    StepResult::Continue
                } else {
                    // Try metamethod
                    if let Some((_mm, _swapped)) = crate::lua::metamethod::resolve_binary_metamethod(
                        &mut tx, &left, &right, crate::lua::metamethod::MetamethodType::Mul
                    )? {
                        let method_name = tx.create_string("__mul")?;
                        tx.queue_operation(PendingOperation::MetamethodCall {
                            method: method_name,
                            target: left.clone(),
                            args: vec![left, right],
                            context: ReturnContext::Register { base: frame.base_register, offset: a },
                        })?;
                        StepResult::Continue
                    } else {
                        tx.commit()?;
                        return Err(LuaError::TypeError {
                            expected: "number".to_string(),
                            got: format!("{} and {}", left.type_name(), right.type_name()),
                        });
                    }
                }
            }
            
            OpCode::Div => {
                // R(A) := RK(B) / RK(C)
                let base = frame.base_register as usize;
                
                // Get operands
                let left = if b & 0x100 != 0 {
                    let const_idx = b & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    closure_obj.proto.constants.get(const_idx)
                        .cloned()
                        .ok_or_else(|| LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )))?
                } else {
                    tx.read_register(self.current_thread, base + b)?
                };
                
                let right = if c & 0x100 != 0 {
                    let const_idx = c & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    closure_obj.proto.constants.get(const_idx)
                        .cloned()
                        .ok_or_else(|| LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )))?
                } else {
                    tx.read_register(self.current_thread, base + c)?
                };
                
                // Try numeric division first
                if let Some((l, r)) = crate::lua::metamethod::can_coerce_arithmetic(&mut tx, &left, &right)? {
                    tx.set_register(self.current_thread, base + a, Value::Number(l / r))?;
                    StepResult::Continue
                } else {
                    // Try metamethod
                    if let Some((_mm, _swapped)) = crate::lua::metamethod::resolve_binary_metamethod(
                        &mut tx, &left, &right, crate::lua::metamethod::MetamethodType::Div
                    )? {
                        let method_name = tx.create_string("__div")?;
                        tx.queue_operation(PendingOperation::MetamethodCall {
                            method: method_name,
                            target: left.clone(),
                            args: vec![left, right],
                            context: ReturnContext::Register { base: frame.base_register, offset: a },
                        })?;
                        StepResult::Continue
                    } else {
                        tx.commit()?;
                        return Err(LuaError::TypeError {
                            expected: "number".to_string(),
                            got: format!("{} and {}", left.type_name(), right.type_name()),
                        });
                    }
                }
            }
            
            OpCode::NewTable => {
                // R(A) := {} (size hints = B,C)
                let base = frame.base_register as usize;
                
                // B and C are size hints for array and hash parts respectively
                // B is the size hint for the array part (0-8 logarithmic)
                // C is the size hint for the hash part (0-8 logarithmic)
                
                // Create a new table
                let table = tx.create_table()?;
                
                // Store in register A
                tx.set_register(self.current_thread, base + a, Value::Table(table))?;
                
                StepResult::Continue
            }
            
            OpCode::Mod => {
                // R(A) := RK(B) % RK(C)
                let base = frame.base_register as usize;
                
                // Get operands
                let left = if b & 0x100 != 0 {
                    let const_idx = b & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    closure_obj.proto.constants.get(const_idx)
                        .cloned()
                        .ok_or_else(|| LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )))?
                } else {
                    tx.read_register(self.current_thread, base + b)?
                };
                
                let right = if c & 0x100 != 0 {
                    let const_idx = c & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    closure_obj.proto.constants.get(const_idx)
                        .cloned()
                        .ok_or_else(|| LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )))?
                } else {
                    tx.read_register(self.current_thread, base + c)?
                };
                
                // Try numeric modulo first
                if let Some((l, r)) = crate::lua::metamethod::can_coerce_arithmetic(&mut tx, &left, &right)? {
                    // Lua mod is defined as: a % b == a - math.floor(a/b)*b
                    let quotient = (l / r).floor();
                    let result = l - quotient * r;
                    tx.set_register(self.current_thread, base + a, Value::Number(result))?;
                    StepResult::Continue
                } else {
                    // Try metamethod
                    if let Some((_mm, _swapped)) = crate::lua::metamethod::resolve_binary_metamethod(
                        &mut tx, &left, &right, crate::lua::metamethod::MetamethodType::Mod
                    )? {
                        let method_name = tx.create_string("__mod")?;
                        tx.queue_operation(PendingOperation::MetamethodCall {
                            method: method_name,
                            target: left.clone(),
                            args: vec![left, right],
                            context: ReturnContext::Register { base: frame.base_register, offset: a },
                        })?;
                        StepResult::Continue
                    } else {
                        tx.commit()?;
                        return Err(LuaError::TypeError {
                            expected: "number".to_string(),
                            got: format!("{} and {}", left.type_name(), right.type_name()),
                        });
                    }
                }
            }
            
            OpCode::Pow => {
                // R(A) := RK(B) ^ RK(C)
                let base = frame.base_register as usize;
                
                // Get operands
                let left = if b & 0x100 != 0 {
                    let const_idx = b & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    closure_obj.proto.constants.get(const_idx)
                        .cloned()
                        .ok_or_else(|| LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )))?
                } else {
                    tx.read_register(self.current_thread, base + b)?
                };
                
                let right = if c & 0x100 != 0 {
                    let const_idx = c & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    closure_obj.proto.constants.get(const_idx)
                        .cloned()
                        .ok_or_else(|| LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )))?
                } else {
                    tx.read_register(self.current_thread, base + c)?
                };
                
                // Try numeric power first
                if let Some((l, r)) = crate::lua::metamethod::can_coerce_arithmetic(&mut tx, &left, &right)? {
                    tx.set_register(self.current_thread, base + a, Value::Number(l.powf(r)))?;
                    StepResult::Continue
                } else {
                    // Try metamethod
                    if let Some((_mm, _swapped)) = crate::lua::metamethod::resolve_binary_metamethod(
                        &mut tx, &left, &right, crate::lua::metamethod::MetamethodType::Pow
                    )? {
                        let method_name = tx.create_string("__pow")?;
                        tx.queue_operation(PendingOperation::MetamethodCall {
                            method: method_name,
                            target: left.clone(),
                            args: vec![left, right],
                            context: ReturnContext::Register { base: frame.base_register, offset: a },
                        })?;
                        StepResult::Continue
                    } else {
                        tx.commit()?;
                        return Err(LuaError::TypeError {
                            expected: "number".to_string(),
                            got: format!("{} and {}", left.type_name(), right.type_name()),
                        });
                    }
                }
            }
            
            OpCode::Unm => {
                // R(A) := -R(B)
                let base = frame.base_register as usize;
                let operand = tx.read_register(self.current_thread, base + b)?;
                
                // Try numeric negation first
                if let Some(n) = crate::lua::metamethod::coerce_to_number(&mut tx, &operand)? {
                    tx.set_register(self.current_thread, base + a, Value::Number(-n))?;
                    StepResult::Continue
                } else {
                    // Try metamethod
                    if let Some(_mm) = crate::lua::metamethod::resolve_metamethod(
                        &mut tx, &operand, crate::lua::metamethod::MetamethodType::Unm
                    )? {
                        let method_name = tx.create_string("__unm")?;
                        tx.queue_operation(PendingOperation::MetamethodCall {
                            method: method_name,
                            target: operand.clone(),
                            args: vec![operand],
                            context: ReturnContext::Register { base: frame.base_register, offset: a },
                        })?;
                        StepResult::Continue
                    } else {
                        tx.commit()?;
                        return Err(LuaError::TypeError {
                            expected: "number".to_string(),
                            got: operand.type_name().to_string(),
                        });
                    }
                }
            }
            
            OpCode::Not => {
                // R(A) := not R(B)
                let base = frame.base_register as usize;
                let operand = tx.read_register(self.current_thread, base + b)?;
                
                // Logical not - no metamethods in Lua 5.1
                let result = Value::Boolean(operand.is_falsey());
                tx.set_register(self.current_thread, base + a, result)?;
                
                StepResult::Continue
            }
            
            OpCode::Len => {
                // R(A) := length of R(B)
                let base = frame.base_register as usize;
                let operand = tx.read_register(self.current_thread, base + b)?;
                
                match operand {
                    Value::String(handle) => {
                        // Direct string length
                        let s = tx.get_string_value(handle)?;
                        let len = s.len() as f64;
                        tx.set_register(self.current_thread, base + a, Value::Number(len))?;
                        StepResult::Continue
                    }
                    Value::Table(table) => {
                        // Check for __len metamethod first
                        if let Some(_mm) = crate::lua::metamethod::resolve_metamethod(
                            &mut tx, &Value::Table(table), crate::lua::metamethod::MetamethodType::Len
                        )? {
                            let method_name = tx.create_string("__len")?;
                            tx.queue_operation(PendingOperation::MetamethodCall {
                                method: method_name,
                                target: Value::Table(table),
                                args: vec![Value::Table(table)],
                                context: ReturnContext::Register { base: frame.base_register, offset: a },
                            })?;
                            StepResult::Continue
                        } else {
                            // No metamethod, calculate table length
                            // In Lua 5.1, #t returns the size of the array part
                            // (highest numeric index n such that t[n] is not nil)
                            let table_obj = tx.get_table(table)?;
                            let len = table_obj.array.len() as f64;
                            tx.set_register(self.current_thread, base + a, Value::Number(len))?;
                            StepResult::Continue
                        }
                    }
                    _ => {
                        // Check for __len metamethod
                        if let Some(_mm) = crate::lua::metamethod::resolve_metamethod(
                            &mut tx, &operand, crate::lua::metamethod::MetamethodType::Len
                        )? {
                            let method_name = tx.create_string("__len")?;
                            tx.queue_operation(PendingOperation::MetamethodCall {
                                method: method_name,
                                target: operand.clone(),
                                args: vec![operand],
                                context: ReturnContext::Register { base: frame.base_register, offset: a },
                            })?;
                            StepResult::Continue
                        } else {
                            tx.commit()?;
                            return Err(LuaError::TypeError {
                                expected: "string or table".to_string(),
                                got: operand.type_name().to_string(),
                            });
                        }
                    }
                }
            }
            
OpCode::Concat => {
    // R(A) := R(B).. ... ..R(C)
    let base = frame.base_register as usize;
    
    println!("DEBUG CONCAT: Processing CONCAT instruction A={}, B={}, C={}", a, b, c);
    
    // Create a vector of all operand register values first
    // This ensures we don't modify any registers until after reading them all
    let mut operand_values = Vec::with_capacity((c - b + 1) as usize);
    for i in b..=c {
        let value = tx.read_register(self.current_thread, base + i)?;
        operand_values.push(value);
    }
    
    println!("DEBUG CONCAT: Collected {} operand values", operand_values.len());
    
    // Determine if we need metamethod handling
    let mut needs_metamethod = false;
    let mut mm_index = 0;
    
    for (i, value) in operand_values.iter().enumerate() {
        // Only defer for actual metamethods, not just any non-string value
        if let Some(_) = crate::lua::metamethod::resolve_metamethod(
            &mut tx, value, crate::lua::metamethod::MetamethodType::Concat
        )? {
            println!("DEBUG CONCAT: Found __concat metamethod, will defer operation");
            needs_metamethod = true;
            mm_index = i;
            break;
        }
        
        if let Some(_) = crate::lua::metamethod::resolve_metamethod(
            &mut tx, value, crate::lua::metamethod::MetamethodType::ToString
        )? {
            println!("DEBUG CONCAT: Found __tostring metamethod, will defer operation");
            needs_metamethod = true;
            mm_index = i;
            break;
        }
    }
    
    if needs_metamethod {
        println!("DEBUG CONCAT: Using deferred processing for metamethod at index {}", mm_index);
        // Use the Pending Operation system for metamethod handling
        tx.queue_operation(PendingOperation::Concatenation {
            values: operand_values,
            current_index: mm_index,
            dest_register: frame.base_register + a as u16,
            accumulated: Vec::new(),
        })?;
    } else {
        println!("DEBUG CONCAT: All values can be processed immediately");
        // We can concatenate immediately 
        let mut result = String::new();
        
        // Process all values
        for (i, value) in operand_values.iter().enumerate() {
            match value {
                Value::String(handle) => {
                    let s = tx.get_string_value(*handle)?;
                    println!("DEBUG CONCAT: Adding string: '{}'", s);
                    result.push_str(&s);
                },
                Value::Number(n) => {
                    println!("DEBUG CONCAT: Adding number: {}", n);
                    result.push_str(&n.to_string());
                },
                Value::Boolean(b) => {
                    println!("DEBUG CONCAT: Adding boolean: {}", b);
                    result.push_str(if *b { "true" } else { "false" });
                },
                Value::Nil => {
                    println!("DEBUG CONCAT: Adding nil");
                    result.push_str("nil");
                },
                Value::CFunction(_) => {
                    println!("DEBUG CONCAT: Adding function reference");
                    result.push_str("function");
                },
                Value::Closure(_) => {
                    println!("DEBUG CONCAT: Adding closure reference");
                    result.push_str("function");
                },
                Value::Table(_) => {
                    println!("DEBUG CONCAT: Adding table reference");
                    result.push_str("table");
                },
                Value::Thread(_) => {
                    println!("DEBUG CONCAT: Adding thread reference");
                    result.push_str("thread");
                },
                Value::UserData(_) => {
                    println!("DEBUG CONCAT: Adding userdata reference");
                    result.push_str("userdata");
                },
                Value::FunctionProto(_) => {
                    println!("DEBUG CONCAT: Adding function proto reference");
                    result.push_str("function");
                },
            }
        }
        
        // Create the final string
        println!("DEBUG CONCAT: Created concatenated string: '{}'", result);
        let string_handle = tx.create_string(&result)?;
        
        // Store result in target register
        tx.set_register(self.current_thread, base + a, Value::String(string_handle))?;
        println!("DEBUG CONCAT: Result stored in register {}", base + a);
    }
    
    StepResult::Continue
},
            
            OpCode::Close => {
                // Close all upvalues >= R(A)
                let base = frame.base_register as usize;
                let close_threshold = base + a;
                
                // Close all upvalues at or above the threshold
                tx.close_thread_upvalues(self.current_thread, close_threshold)?;
                
                StepResult::Continue
            },

OpCode::Closure => {
    // R(A) := closure(KPROTO[Bx])
    let base = frame.base_register as usize;
    let bx = instruction.bx() as usize;
    
    // Phase 1: Extract needed information without holding a long-lived borrow
    let proto_result = {
        // Get the closure
        let closure_obj = tx.get_closure(frame.closure)?;
        
        // Check constant index bounds
        if bx >= closure_obj.proto.constants.len() {
            return Err(LuaError::RuntimeError(format!(
                "Prototype index {} out of bounds (constants: {})",
                bx, closure_obj.proto.constants.len()
            )));
        }
        
        // Get the function prototype from constants
        let proto_value = &closure_obj.proto.constants[bx];
        
        // Match the prototype and extract what we need
        match proto_value {
            Value::FunctionProto(proto_handle) => {
                // We have a function prototype
                Ok(*proto_handle)
            },
            _ => {
                // Not a function prototype
                Err(LuaError::TypeError {
                    expected: "function prototype".to_string(),
                    got: proto_value.type_name().to_string(),
                })
            }
        }
    };
    
    // Handle error cases early
    let proto_handle = proto_result?;
    
    // Phase 2: Validate and extract the function prototype (separate transaction)
    let proto = tx.get_function_proto_copy(proto_handle)?;
    
    // Phase 3: Process upvalue instructions
    let num_upvalues = proto.upvalues.len();
    let mut upvalues = Vec::with_capacity(num_upvalues);
    
    for i in 0..num_upvalues {
        // Create a separate scope for each upvalue to avoid borrow conflicts
        let upvalue_handle = {
            // Read the instruction that follows the CLOSURE instruction
            let upval_pc = frame.pc + 1 + i;
            
            // Validate PC bounds
            let bytecode_len = {
                let closure_obj = tx.get_closure(frame.closure)?;
                closure_obj.proto.bytecode.len()
            };
            
            if upval_pc >= bytecode_len {
                tx.commit()?;
                return Err(LuaError::RuntimeError(format!(
                    "Upvalue instruction PC {} out of bounds",
                    upval_pc
                )));
            }
            
            // Process based on upvalue info
            let upvalue_info = proto.upvalues[i];
            
            if upvalue_info.in_stack {
                // Creating an upvalue that points to the stack
                let stack_idx = base + upvalue_info.index as usize;
                tx.find_or_create_upvalue(self.current_thread, stack_idx)?
            } else {
                // Getting an upvalue from parent closure
                let idx = upvalue_info.index as usize;
                
                // Get parent's upvalues
                let parent_upvalues = {
                    let parent_closure = tx.get_closure(frame.closure)?;
                    
                    if idx >= parent_closure.upvalues.len() {
                        tx.commit()?;
                        return Err(LuaError::RuntimeError(format!(
                            "Parent upvalue index {} out of bounds",
                            idx
                        )));
                    }
                    
                    // Clone the parent's upvalues to avoid borrow issues
                    parent_closure.upvalues.clone()
                };
                
                // Validate the upvalue handle outside of closure borrow
                let upval_handle = parent_upvalues[idx];
                tx.validate_handle(&upval_handle)?;
                
                upval_handle
            }
        };
        
        upvalues.push(upvalue_handle);
    }
    
    // Phase 4: Create the closure with upvalues
    let new_closure = Closure {
        proto,
        upvalues,
    };
    
    let new_closure_handle = tx.create_closure(new_closure)?;
    tx.set_register(self.current_thread, base + a, Value::Closure(new_closure_handle))?;
    
    // Skip the upvalue initialization instructions
    for _ in 0..num_upvalues {
        tx.increment_pc(self.current_thread)?;
    }
    
    StepResult::Continue
},

            OpCode::Self_ => {
                // R(A+1) := R(B); R(A) := R(B)[RK(C)]
                let base = frame.base_register as usize;
                
                // Get the table
                let table_val = tx.read_register(self.current_thread, base + b)?;
                
                // Set R(A+1) to the table (self)
                tx.set_register(self.current_thread, base + a + 1, table_val.clone())?;
                
                // Get key from RK(C)
                let key_val = if c & 0x100 != 0 {
                    // RK(C) is constant
                    let const_idx = c & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    if const_idx >= closure_obj.proto.constants.len() {
                        tx.commit()?;
                        return Err(LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )));
                    }
                    closure_obj.proto.constants[const_idx].clone()
                } else {
                    // RK(C) is register
                    tx.read_register(self.current_thread, base + c)?
                };
                
                // Get method from table and store in R(A)
                match table_val {
                    Value::Table(table) => {
                        // Try direct table lookup
                        let method = tx.read_table_field(table, &key_val)?;
                        
                        // Check for __index metamethod if the method is nil
                        if method.is_nil() {
                            if let Some(mm) = crate::lua::metamethod::resolve_metamethod(
                                &mut tx, &Value::Table(table), crate::lua::metamethod::MetamethodType::Index
                            )? {
                                match mm {
                                    Value::Table(index_table) => {
                                        // Metatable __index is a table, look up the method there
                                        let mm_method = tx.read_table_field(index_table, &key_val)?;
                                        tx.set_register(self.current_thread, base + a, mm_method)?;
                                    }
                                    Value::Closure(_) | Value::CFunction(_) => {
                                        // Metatable __index is a function, queue metamethod call
                                        let method_name = tx.create_string("__index")?;
                                        tx.queue_operation(PendingOperation::MetamethodCall {
                                            method: method_name,
                                            target: Value::Table(table),
                                            args: vec![Value::Table(table), key_val],
                                            context: ReturnContext::Register { 
                                                base: frame.base_register, 
                                                offset: a 
                                            },
                                        })?;
                                    }
                                    _ => {
                                        // No valid metamethod, result is nil
                                        tx.set_register(self.current_thread, base + a, Value::Nil)?;
                                    }
                                }
                            } else {
                                // No __index metamethod, result is nil
                                tx.set_register(self.current_thread, base + a, Value::Nil)?;
                            }
                        } else {
                            // Method found directly in the table
                            tx.set_register(self.current_thread, base + a, method)?;
                        }
                        
                        StepResult::Continue
                    },
                    _ => {
                        // Only tables can have methods
                        tx.commit()?;
                        return Err(LuaError::TypeError {
                            expected: "table".to_string(),
                            got: table_val.type_name().to_string(),
                        });
                    }
                }
            },

            OpCode::SetList => {
                // R(A)[(C-1)*FPF+i] := R(A+i), 1 <= i <= B
                // FPF (Fields Per Flush) is typically 50 in Lua
                const FPF: usize = 50;
                
                let base = frame.base_register as usize;
                let table_val = tx.read_register(self.current_thread, base + a)?;
                
                match table_val {
                    Value::Table(table) => {
                        // Calculate the starting index for assignment
                        let start_index = if c > 0 {
                            (c - 1) * FPF
                        } else {
                            // If C == 0, the actual C value is in the next instruction
                            // Get the next instruction
                            let next_pc = frame.pc + 1;
                            
                            // Validate that the next instruction exists
                            let closure_obj = tx.get_closure(frame.closure)?;
                            if next_pc >= closure_obj.proto.bytecode.len() {
                                tx.commit()?;
                                return Err(LuaError::RuntimeError(format!(
                                    "Missing next instruction for SetList C=0 case at PC {}",
                                    frame.pc
                                )));
                            }
                            
                            // Read the next instruction as a raw value
                            // In Lua 5.1, the next instruction is the actual C value, not an encoded instruction
                            let extra_c = closure_obj.proto.bytecode[next_pc] as usize;
                            
                            // Increment PC to skip the extra instruction
                            tx.increment_pc(self.current_thread)?;
                            
                            // Use the extra C value directly
                            (extra_c - 1) * FPF
                        };
                        
                        // B is the number of elements to set
                        let num_elements = if b == 0 {
                            // If B == 0, set all elements on the stack from R(A+1) to top
                            let top = tx.get_stack_top(self.current_thread)?;
                            top - base - a - 1
                        } else {
                            b
                        };
                        
                        // Set each element
                        for i in 0..num_elements {
                            let value = tx.read_register(self.current_thread, base + a + 1 + i)?;
                            let index = start_index + i + 1; // Lua arrays are 1-indexed
                            
                            // Convert index to Value::Number for table indexing
                            let index_val = Value::Number(index as f64);
                            tx.set_table_field(table, index_val, value)?;
                        }
                        
                        StepResult::Continue
                    }
                    _ => {
                        tx.commit()?;
                        return Err(LuaError::TypeError { 
                            expected: "table".to_string(), 
                            got: table_val.type_name().to_string(),
                        });
                    }
                }
            }
            
            OpCode::Eq => {
                // if ((RK(B) == RK(C)) ~= A) then pc++
                // Note: A=0 means "expected false", A=1 means "expected true"
                let base = frame.base_register as usize;
                let expected = a != 0;
                
                // Get operands
                let left = if b & 0x100 != 0 {
                    // RK(B) is constant
                    let const_idx = b & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    closure_obj.proto.constants.get(const_idx)
                        .cloned()
                        .ok_or_else(|| LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )))?
                } else {
                    // RK(B) is register
                    tx.read_register(self.current_thread, base + b)?
                };
                
                let right = if c & 0x100 != 0 {
                    // RK(C) is constant
                    let const_idx = c & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    closure_obj.proto.constants.get(const_idx)
                        .cloned()
                        .ok_or_else(|| LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )))?
                } else {
                    // RK(C) is register
                    tx.read_register(self.current_thread, base + c)?
                };
                
                // Try direct comparison first
                let need_metamethod = match (&left, &right) {
                    // Different types are never equal without metamethods
                    (l, r) if std::mem::discriminant(l) != std::mem::discriminant(r) => false,

                    // For tables, userdata - always use metamethods if available
                    (Value::Table(_), Value::Table(_)) | 
                    (Value::UserData(_), Value::UserData(_)) => true,

                    // For other types, direct comparison is sufficient
                    _ => false,
                };

                // Perform direct comparison if we don't need metamethods
                if !need_metamethod {
                    let direct_result = match (&left, &right) {
                        // Compare primitive types directly
                        (Value::Nil, Value::Nil) => true,
                        (Value::Boolean(l), Value::Boolean(r)) => l == r,
                        (Value::Number(l), Value::Number(r)) => {
                            // NaN != NaN in Lua
                            if l.is_nan() || r.is_nan() {
                                false
                            } else {
                                l == r
                            }
                        },
                        (Value::String(l), Value::String(r)) => {
                            if l == r {
                                // Same handle means same string
                                true
                            } else {
                                // Different handles, compare content
                                let l_str = tx.get_string_value(*l)?;
                                let r_str = tx.get_string_value(*r)?;
                                l_str == r_str
                            }
                        },
                        // Other types compared by identity
                        (Value::Closure(l), Value::Closure(r)) => l == r,
                        (Value::Thread(l), Value::Thread(r)) => l == r,
                        (Value::CFunction(l), Value::CFunction(r)) => {
                            // Function pointers are compared by identity
                            std::ptr::eq((*l) as *const (), (*r) as *const ())
                        },
                        // Different types are never equal
                        _ => false,
                    };
                    
                    // Skip next instruction if comparison result doesn't match expected
                    if direct_result != expected {
                        tx.increment_pc(self.current_thread)?;
                    }
                    
                    StepResult::Continue
                } else {
                    // Try metamethod for table/userdata types
                    // Note: For Eq, both operands must have the same metamethod (unlike Add, etc.)
                    // Check for the __eq metamethod
                    if let Some((_, _)) = crate::lua::metamethod::resolve_binary_metamethod(
                        &mut tx, &left, &right, crate::lua::metamethod::MetamethodType::Eq
                    )? {
                        // Create metamethod name and context
                        let method_name = tx.create_string("__eq")?;
                        
                        // Queue metamethod call
                        tx.queue_operation(PendingOperation::MetamethodCall {
                            method: method_name,
                            target: left.clone(), 
                            args: vec![left, right],
                            context: ReturnContext::Metamethod { 
                                context: crate::lua::metamethod::MetamethodContext {
                                    mm_type: crate::lua::metamethod::MetamethodType::Eq,
                                    continuation: crate::lua::metamethod::MetamethodContinuation::ComparisonSkip {
                                        thread: self.current_thread,
                                        expected,
                                    },
                                },
                            },
                        })?;
                        
                        // Don't increment PC - will be handled by metamethod result
                        should_increment_pc = false;
                    } else {
                        // No metamethod, fall back to regular equality rules for tables/userdata
                        // In Lua, objects of same type but no metamethod are equal only if they're the same object
                        let direct_result = match (&left, &right) {
                            (Value::Table(l), Value::Table(r)) => l == r,
                            (Value::UserData(l), Value::UserData(r)) => l == r,
                            _ => false, // Shouldn't happen due to earlier discriminant check
                        };
                        
                        if direct_result != expected {
                            tx.increment_pc(self.current_thread)?;
                        }
                    }
                    
                    StepResult::Continue
                }
            }
            
            OpCode::Lt => {
                // if ((RK(B) < RK(C)) ~= A) then pc++
                let base = frame.base_register as usize;
                let expected = a != 0;
                
                // Get operands
                let left = if b & 0x100 != 0 {
                    // RK(B) is constant
                    let const_idx = b & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    closure_obj.proto.constants.get(const_idx)
                        .cloned()
                        .ok_or_else(|| LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )))?
                } else {
                    // RK(B) is register
                    tx.read_register(self.current_thread, base + b)?
                };
                
                let right = if c & 0x100 != 0 {
                    // RK(C) is constant
                    let const_idx = c & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    closure_obj.proto.constants.get(const_idx)
                        .cloned()
                        .ok_or_else(|| LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )))?
                } else {
                    // RK(C) is register
                    tx.read_register(self.current_thread, base + c)?
                };
                
                // Try direct comparison for numbers and strings
                let direct_result_opt = match (&left, &right) {
                    // Only number-number and string-string comparisons can be done directly
                    (Value::Number(l), Value::Number(r)) => {
                        // Direct numeric comparison
                        // If either is NaN, the result is always false
                        if l.is_nan() || r.is_nan() {
                            Some(false)
                        } else {
                            Some(l < r)
                        }
                    },
                    (Value::String(l_handle), Value::String(r_handle)) => {
                        // For strings, we always compare lexicographically
                        let l_str = tx.get_string_value(*l_handle)?;
                        let r_str = tx.get_string_value(*r_handle)?;
                        Some(l_str < r_str)
                    },
                    // Allow string-number or number-string coercion if the string is a valid number
                    (Value::Number(n), Value::String(s_handle)) => {
                        if let Ok(Some(s_num)) = crate::lua::metamethod::coerce_to_number(&mut tx, &Value::String(*s_handle)) {
                            Some(*n < s_num)
                        } else {
                            None // String can't be coerced to number, use metamethod
                        }
                    },
                    (Value::String(s_handle), Value::Number(n)) => {
                        if let Ok(Some(s_num)) = crate::lua::metamethod::coerce_to_number(&mut tx, &Value::String(*s_handle)) {
                            Some(s_num < *n)
                        } else {
                            None // String can't be coerced to number, use metamethod
                        }
                    },
                    // All other type combinations require metamethods
                    _ => None,
                };
                
                if let Some(result) = direct_result_opt {
                    // Direct comparison succeeded - skip if result doesn't match expected
                    if result != expected {
                        tx.increment_pc(self.current_thread)?;
                    }
                    StepResult::Continue
                } else {
                    // Try metamethod
                    if let Some((_, _)) = crate::lua::metamethod::resolve_binary_metamethod(
                        &mut tx, &left, &right, crate::lua::metamethod::MetamethodType::Lt
                    )? {
                        // Create metamethod name and context
                        let method_name = tx.create_string("__lt")?;
                        
                        // Queue metamethod call
                        tx.queue_operation(PendingOperation::MetamethodCall {
                            method: method_name,
                            target: left.clone(),
                            args: vec![left, right],
                            context: ReturnContext::Metamethod {
                                context: crate::lua::metamethod::MetamethodContext {
                                    mm_type: crate::lua::metamethod::MetamethodType::Lt,
                                    continuation: crate::lua::metamethod::MetamethodContinuation::ComparisonSkip {
                                        thread: self.current_thread,
                                        expected,
                                    },
                                },
                            },
                        })?;
                        
                        // Don't increment PC - will be handled by metamethod result
                        should_increment_pc = false;
                        StepResult::Continue
                    } else {
                        // No metamethod and types don't support direct comparison - error
                        tx.commit()?;
                        return Err(LuaError::TypeError {
                            expected: "values comparable with the < operator".to_string(),
                            got: format!("{} and {}", left.type_name(), right.type_name()),
                        });
                    }
                }
            }
            
            OpCode::Le => {
                // if ((RK(B) <= RK(C)) ~= A) then pc++
                let base = frame.base_register as usize;
                let expected = a != 0;
                
                // Get operands
                let left = if b & 0x100 != 0 {
                    // RK(B) is constant
                    let const_idx = b & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    closure_obj.proto.constants.get(const_idx)
                        .cloned()
                        .ok_or_else(|| LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )))?
                } else {
                    // RK(B) is register
                    tx.read_register(self.current_thread, base + b)?
                };
                
                let right = if c & 0x100 != 0 {
                    // RK(C) is constant
                    let const_idx = c & 0xFF;
                    let closure_obj = tx.get_closure(frame.closure)?;
                    closure_obj.proto.constants.get(const_idx)
                        .cloned()
                        .ok_or_else(|| LuaError::RuntimeError(format!(
                            "Constant index {} out of bounds", const_idx
                        )))?
                } else {
                    // RK(C) is register
                    tx.read_register(self.current_thread, base + c)?
                };
                
                // Try direct comparison for numbers and strings
                let direct_result_opt = match (&left, &right) {
                    // Only number-number and string-string comparisons can be done directly
                    (Value::Number(l), Value::Number(r)) => {
                        // Direct numeric comparison
                        // If either is NaN, the result is always false
                        if l.is_nan() || r.is_nan() {
                            Some(false)
                        } else {
                            Some(l <= r)
                        }
                    },
                    (Value::String(l_handle), Value::String(r_handle)) => {
                        // For strings, we always compare lexicographically
                        let l_str = tx.get_string_value(*l_handle)?;
                        let r_str = tx.get_string_value(*r_handle)?;
                        Some(l_str <= r_str)
                    },
                    // Allow string-number or number-string coercion if the string is a valid number
                    (Value::Number(n), Value::String(s_handle)) => {
                        if let Ok(Some(s_num)) = crate::lua::metamethod::coerce_to_number(&mut tx, &Value::String(*s_handle)) {
                            Some(*n <= s_num)
                        } else {
                            None // String can't be coerced to number, use metamethod
                        }
                    },
                    (Value::String(s_handle), Value::Number(n)) => {
                        if let Ok(Some(s_num)) = crate::lua::metamethod::coerce_to_number(&mut tx, &Value::String(*s_handle)) {
                            Some(s_num <= *n)
                        } else {
                            None // String can't be coerced to number, use metamethod
                        }
                    },
                    // All other type combinations require metamethods
                    _ => None,
                };
                
                if let Some(result) = direct_result_opt {
                    // Direct comparison succeeded - skip if result doesn't match expected
                    if result != expected {
                        tx.increment_pc(self.current_thread)?;
                    }
                    StepResult::Continue
                } else {
                    // Try metamethod - complex logic for Le
                    // First try __le metamethod
                    let found_le_metamethod = crate::lua::metamethod::resolve_binary_metamethod(
                        &mut tx, &left, &right, crate::lua::metamethod::MetamethodType::Le
                    )?.is_some();
                    
                    if found_le_metamethod {
                        // Use __le metamethod
                        let method_name = tx.create_string("__le")?;
                        
                        // Queue metamethod call
                        tx.queue_operation(PendingOperation::MetamethodCall {
                            method: method_name,
                            target: left.clone(),
                            args: vec![left, right],
                            context: ReturnContext::Metamethod {
                                context: crate::lua::metamethod::MetamethodContext {
                                    mm_type: crate::lua::metamethod::MetamethodType::Le,
                                    continuation: crate::lua::metamethod::MetamethodContinuation::ComparisonSkip {
                                        thread: self.current_thread,
                                        expected,
                                    },
                                },
                            },
                        })?;
                        
                        // Don't increment PC - will be handled by metamethod result
                        should_increment_pc = false;
                        StepResult::Continue
                    } else {
                        // Try __lt metamethod with reversed arguments
                        // In Lua 5.1: a <= b is equivalent to not (b < a)
                        let found_lt_metamethod = crate::lua::metamethod::resolve_binary_metamethod(
                            &mut tx, &right, &left, crate::lua::metamethod::MetamethodType::Lt
                        )?.is_some();
                        
                        if found_lt_metamethod {
                            // Use __lt metamethod with reversed arguments and negated result
                            let method_name = tx.create_string("__lt")?;
                            
                            // Queue metamethod call with swapped arguments - note the NOT in the expected value
                            // Now the logic is: "if ((right < left) == expected) then pc++"
                            // Since a <= b is equivalent to not (b < a), and we want to increment PC if result != expected,
                            // we need to invert the expected value
                            tx.queue_operation(PendingOperation::MetamethodCall {
                                method: method_name,
                                target: right.clone(),
                                args: vec![right, left], // Arguments reversed!
                                context: ReturnContext::Metamethod {
                                    context: crate::lua::metamethod::MetamethodContext {
                                        mm_type: crate::lua::metamethod::MetamethodType::Lt,
                                        continuation: crate::lua::metamethod::MetamethodContinuation::ComparisonSkip {
                                            thread: self.current_thread,
                                            expected: !expected, // Note the inversion here!
                                        },
                                    },
                                },
                            })?;
                            
                            // Don't increment PC - will be handled by metamethod result
                            should_increment_pc = false;
                            StepResult::Continue
                        } else {
                            // No metamethod and types don't support direct comparison - error
                            tx.commit()?;
                            return Err(LuaError::TypeError {
                                expected: "values comparable with the <= operator".to_string(),
                                got: format!("{} and {}", left.type_name(), right.type_name()),
                            });
                        }
                    }
                }
            }
            
            OpCode::Jmp => {
                // pc += sBx (signed immediate offset)
                let sbx = instruction.sbx();
                
                // Get current PC and add the signed offset
                let current_pc = frame.pc;
                let new_pc = (current_pc as i32 + sbx) as usize;
                
                // Set PC directly instead of incrementing
                tx.set_pc(self.current_thread, new_pc)?;
                
                // Don't increment PC - we've already set it directly
                should_increment_pc = false;
                
                StepResult::Continue
            }
            
            OpCode::Test => {
                // if not (R(A) <=> C) then pc++
                // In Lua, C=0 means "test for false", C=1 means "test for true"
                
                let base = frame.base_register as usize;
                let test_value = tx.read_register(self.current_thread, base + a)?;
                let expected_true = c != 0;
                
                // Test truthiness against expected (using is_falsey method)
                let is_truthy = !test_value.is_falsey();
                
                // Skip next instruction if not matching expected truthiness
                if is_truthy != expected_true {
                    tx.increment_pc(self.current_thread)?;
                    should_increment_pc = false; // Don't increment PC again since we already did
                }
                
                StepResult::Continue
            }
            
            OpCode::TestSet => {
                // if (R(B) <=> C) then R(A) := R(B) else pc++
                // In Lua, C=0 means "test for false", C=1 means "test for true"
                
                let base = frame.base_register as usize;
                let test_value = tx.read_register(self.current_thread, base + b)?;
                let expected_true = c != 0;
                
                // Test truthiness against expected (using is_falsey method)
                let is_truthy = !test_value.is_falsey();
                
                if is_truthy == expected_true {
                    // Condition matches, set R(A) = R(B)
                    tx.set_register(self.current_thread, base + a, test_value)?;
                } else {
                    // Condition doesn't match, skip next instruction
                    tx.increment_pc(self.current_thread)?;
                    should_increment_pc = false; // Don't increment PC again since we already did
                }
                
                StepResult::Continue
            }
            
            OpCode::ForPrep => {
                // R(A) -= R(A+2); pc += sBx
                // This initializes a numeric for loop
                // R(A) = index, R(A+1) = limit, R(A+2) = step
                
                let base = frame.base_register as usize;
                
                // Get values from registers
                let index_val = tx.read_register(self.current_thread, base + a)?;
                let step_val = tx.read_register(self.current_thread, base + a + 2)?;
                
                // Implement numeric for loop initialization
                // First try to coerce both to numbers
                let (index_num, step_num) = match (index_val, step_val) {
                    (Value::Number(i), Value::Number(s)) => {
                        // Direct numeric calculation
                        (i, s)
                    },
                    (index_v, step_v) => {
                        // Try to convert both to numbers (Lua coercion rules)
                        let index_opt = match &index_v {
                            Value::Number(n) => Some(*n),
                            Value::String(handle) => {
                                // Try to parse the string as a number
                                let s = tx.get_string_value(*handle)?;
                                s.parse::<f64>().ok()
                            },
                            _ => None,
                        };
                        
                        let step_opt = match &step_v {
                            Value::Number(n) => Some(*n),
                            Value::String(handle) => {
                                // Try to parse the string as a number
                                let s = tx.get_string_value(*handle)?;
                                s.parse::<f64>().ok()
                            },
                            _ => None,
                        };
                        
                        match (index_opt, step_opt) {
                            (Some(i), Some(s)) => (i, s),
                            _ => {
                                tx.commit()?;
                                return Err(LuaError::RuntimeError("'for' initial value and step must be numbers".to_string()));
                            }
                        }
                    }
                };
                
                // Subtract step from index (as initial step for the loop)
                let new_index = index_num - step_num;
                println!("ForPrep: setting the initial value from {} to {}", index_num, new_index);
                
                // Store the results back to R(A)
                tx.set_register(self.current_thread, base + a, Value::Number(new_index))?;
                
                // Get sbx value for PC jump
                let sbx = instruction.sbx();
                
                // Jump forward to the end of the loop body (ForLoop will jump back here)
                println!("ForPrep: sbx offset = {}", sbx);

                

                let current_pc = frame.pc;
                let new_pc = (current_pc as i32 + sbx) as usize;
                
                println!("ForPrep: jumping from PC {} to PC {}", current_pc, new_pc);

                

                tx.set_pc(self.current_thread, new_pc)?;
                
                // Don't increment PC - we've already set it directly
                should_increment_pc = false;
                
                StepResult::Continue
            }
            
            OpCode::ForLoop => {
                // R(A) += R(A+2); if R(A) <?= R(A+1) then { pc+=sBx; R(A+3) = R(A) }
                // Numeric for loop iteration
                // R(A) = index, R(A+1) = limit, R(A+2) = step, R(A+3) = loop variable
                
                let base = frame.base_register as usize;
                
                // Get values from registers
                let index_val = tx.read_register(self.current_thread, base + a)?;
                let limit_val = tx.read_register(self.current_thread, base + a + 1)?;
                let step_val = tx.read_register(self.current_thread, base + a + 2)?;
                
                // Convert to numbers (they should already be numbers from ForPrep)
                let index_num = match index_val {
                    Value::Number(n) => n,
                    _ => {
                        tx.commit()?;
                        return Err(LuaError::RuntimeError("Invalid 'for' index (not a number)".to_string()));
                    }
                };
                
                let limit_num = match limit_val {
                    Value::Number(n) => n,
                    _ => {
                        tx.commit()?;
                        return Err(LuaError::RuntimeError("Invalid 'for' limit (not a number)".to_string()));
                    }
                };
                
                let step_num = match step_val {
                    Value::Number(n) => n,
                    _ => {
                        tx.commit()?;
                        return Err(LuaError::RuntimeError("Invalid 'for' step (not a number)".to_string()));
                    }
                };
                
                // Increment the index by step
                let new_index = index_num + step_num;
                
                // Store updated index immediately
                tx.set_register(self.current_thread, base + a, Value::Number(new_index))?;
                
                // Check if the loop should continue
                let should_continue = if step_num > 0.0 {
                    // Positive step: continue if index <= limit
                    new_index <= limit_num
                } else {
                    // Negative step: continue if index >= limit
                    new_index >= limit_num
                };
                
                if should_continue {
                    // Loop continues - set loop variable and jump back to loop body
                    // Set Lua variable to the new index value (display value)
                    tx.set_register(self.current_thread, base + a + 3, Value::Number(new_index))?;
                    
                    // Get sbx value for PC jump (jump back to loop body)
                    let sbx = instruction.sbx();
                    
                    // Jump based on the sbx offset (which should be negative)
                    let current_pc = frame.pc;
                    let new_pc = (current_pc as i32 + sbx) as usize;
                    println!("ForLoop: jumping from PC {} to PC {}", current_pc, new_pc);
                    
                    tx.set_pc(self.current_thread, new_pc)?;
                    
                    // Don't increment PC - we've already set it directly
                    should_increment_pc = false;
                } 
                // else: Loop is done - just fall through to the next instruction
                
                StepResult::Continue
            }
            
            OpCode::TForLoop => {
                // R(A+3), ... , R(A+3+C) := R(A)(R(A+1), R(A+2)); 
                // if R(A+3) ~= nil then R(A+2)=R(A+3) else pc++
                // 
                // In Lua 5.1, TForLoop just calls the iterator and sets up variables.
                // A separate JMP instruction must follow to handle the loop back logic.
                // If loop is done (iterator returns nil), it skips the JMP instruction.
                
                let base = frame.base_register as usize;
                let num_returns = c as usize; // Number of expected return values
                
                // Get the iterator function, state, and control variable
                let iterator = tx.read_register(self.current_thread, base + a)?;
                let state = tx.read_register(self.current_thread, base + a + 1)?;
                let control = tx.read_register(self.current_thread, base + a + 2)?;
                
                // Save current PC for context
                let current_pc = frame.pc;
                
                // Process based on iterator function type
                match iterator {
                    Value::Closure(closure) => {
                        // First increment the PC to the next instruction
                        tx.increment_pc(self.current_thread)?;
                        
                        // Queue the iterator call as a function call
                        tx.queue_operation(PendingOperation::FunctionCall {
                            closure,
                            args: vec![state, control],
                            context: ReturnContext::ForLoop { 
                                base: frame.base_register, 
                                a, 
                                c: num_returns,
                                pc: current_pc,
                                sbx: 0,
                            },
                        })?;
                        
                        // Since we already incremented PC, don't do it again
                        should_increment_pc = false;
                        
                        StepResult::Continue
                    },
                    Value::CFunction(cfunc) => {
                        // For C functions, we need special handling
                        // First increment the PC
                        tx.increment_pc(self.current_thread)?;
                        
                        // Commit the transaction
                        tx.commit()?;
                        
                        // Extract arguments we need to avoid self-borrowing conflicts
                        let func_copy = cfunc; // CFunction implements Copy
                        let args = vec![state, control];
                        let base_register = frame.base_register;
                        let thread_handle = self.current_thread;
                        
                        // Call the special handler with clean borrows
                        should_increment_pc = false; // We already incremented PC above
                        
                        return self.handle_tforloop_c_function(
                            func_copy,
                            args,
                            base_register,
                            a,
                            num_returns,
                            current_pc,
                            0, // sbx is not used in Lua 5.1 TForLoop
                            thread_handle
                        );
                    },
                    _ => {
                        // Not a function - return error after committing
                        tx.commit()?;
                        return Err(LuaError::TypeError { 
                            expected: "function".to_string(), 
                            got: iterator.type_name().to_string(),
                        });
                    },
                }
            }
            
            OpCode::VarArg => {
                // R(A), R(A+1), ..., R(A+B-2) = vararg
                let base = frame.base_register as usize;
                
                // Phase 1: Collect needed information
                let (vararg_values, expected_results) = {
                    // Get current call frame
                    let frame = tx.get_current_frame(self.current_thread)?;
                    
                    // Get varargs from the frame
                    let varargs = match &frame.varargs {
                        Some(vars) => vars.clone(),
                        None => {
                            // No varargs available
                            tx.commit()?;
                            return Err(LuaError::RuntimeError("Function has no variable arguments".to_string()));
                        }
                    };
                    
                    // Determine number of results to return
                    let expected = if b == 0 {
                        // Use all available varargs
                        varargs.len()
                    } else {
                        // Fixed number: B-1
                        b - 1
                    };
                    
                    (varargs, expected)
                };
                
                // Phase 2: Store vararg values in registers
                for i in 0..expected_results {
                    let value = if i < vararg_values.len() {
                        vararg_values[i].clone()
                    } else {
                        Value::Nil
                    };
                    
                    tx.set_register(self.current_thread, base + a + i, value)?;
                }
                
                StepResult::Continue
            },
            
            OpCode::ExtraArg => {
                // ExtraArg is only used as data for previous instructions
                // It is never executed directly - it contains extra bits for the argument
                // of the previous instruction.
                // Typically used to extend the argument range of certain opcodes (e.g., C in SetList)
                
                // In Lua 5.1, the ExtraArg instruction is simply skipped during normal execution
                // We'll just increment the PC and continue
                
                StepResult::Continue
            },
            
            _ => {
                tx.commit()?;
                return Err(LuaError::NotImplemented(format!("Opcode {:?}", opcode)));
            }
        };
        
        // Increment PC if needed
        if should_increment_pc {
            tx.increment_pc(self.current_thread)?;
        }
        
        // Commit transaction and get pending operations
        let pending_ops = tx.commit()?;
        
        // Queue any new pending operations
        for op in pending_ops {
            self.pending_operations.push_back(op);
        }
        
        Ok(result)
    }
}

/// Helper function for handling metamethod continuations
/// This is a free function to avoid borrowing self, making it usable from within other methods
fn handle_metamethod_result(
    tx: &mut HeapTransaction,
    current_thread: ThreadHandle,
    result_value: Value,
    context: crate::lua::metamethod::MetamethodContext,
) -> LuaResult<()> {
    use crate::lua::metamethod::MetamethodContinuation;
    
    // Process the continuation using the provided transaction
    match context.continuation {
        MetamethodContinuation::StoreInRegister { base, offset } => {
            tx.set_register(current_thread, base as usize + offset, result_value)?;
        },
        MetamethodContinuation::TableAssignment { table, key } => {
            tx.set_table_field(table, key, result_value)?;
        },
        MetamethodContinuation::ComparisonResult { base, a, invert } => {
            // Convert result to boolean
            let bool_result = result_value.to_boolean();
            let final_result = if invert { !bool_result } else { bool_result };
            
            tx.set_register(current_thread, base as usize + a, Value::Boolean(final_result))?;
        },
        MetamethodContinuation::ComparisonSkip { thread, expected } => {
            // For comparison operations, we need to conditionally skip the next instruction
            let result = result_value.to_boolean();
            
            // Skip next instruction if (result != expected)
            if result != expected {
                tx.increment_pc(thread)?;
            }
        },
        MetamethodContinuation::ChainOperation { next_op } => {
            // Queue the next operation
            tx.queue_operation(*next_op)?;
        },
    }
    
    Ok(())
}

impl LuaVM {
    // Handle C function call without borrowing self more than once
    fn handle_c_function_call(
        &mut self,
        func: CFunction,
        args: Vec<Value>,
        base_register: u16, 
        register_a: usize,
        thread_handle: ThreadHandle,
    ) -> LuaResult<StepResult> {
        // Setup C execution context
        let stack_base = base_register as usize + register_a;
        
        println!("DEBUG CFUNC: Calling C function at {:?} with {} args", func as *const (), args.len());
        
        // First, set up the stack with arguments
        {
            let mut tx = HeapTransaction::new(&mut self.heap);
            
            // Copy arguments to registers where the C function expects them
            for (i, arg) in args.iter().enumerate() {
                println!("DEBUG CFUNC: Setting arg {} to {:?}", i, arg);
                tx.set_register(thread_handle, stack_base + i, arg.clone())?;
            }
            
            tx.commit()?;
        }
        
        // Call C function with isolated context - this is a new borrow
        let (result_count, results_pushed) = {
            let mut ctx = ExecutionContext::new(self, stack_base, args.len(), thread_handle);
            let count = match func(&mut ctx) {
                Ok(count) => {
                    println!("DEBUG CFUNC: Function returned success with {} values", count);
                    count as usize
                },
                Err(e) => {
                    println!("DEBUG CFUNC: Function returned error: {:?}", e);
                    return Ok(StepResult::Error(e));
                },
            };
            (count, ctx.results_pushed)
        };
        
        // Verify result count matches what the context pushed
        if result_count != results_pushed {
            println!("DEBUG CFUNC: Warning: returned {} but pushed {} values", result_count, results_pushed);
        }
        
        // Collect results after function returns - new borrow of self
        let mut results = Vec::with_capacity(result_count);
        
        {
            // Create a new transaction to read the results
            let mut tx = HeapTransaction::new(&mut self.heap);
            println!("DEBUG CFUNC: Collecting {} results from stack", result_count);
            
            for i in 0..result_count {
                match tx.read_register(thread_handle, stack_base + i) {
                    Ok(value) => {
                        println!("DEBUG CFUNC: Result {}: {:?} ({})", i, value, value.type_name());
                        results.push(value);
                    },
                    Err(e) => {
                        println!("DEBUG CFUNC: Error reading result {}: {:?}", i, e);
                        break;
                    }
                }
            }
            
            // Queue results as a CFunctionReturn operation
            println!("DEBUG CFUNC: Queueing CFunctionReturn with {} values", results.len());
            tx.queue_operation(PendingOperation::CFunctionReturn {
                values: results,
                context: ReturnContext::Register {
                    base: base_register,
                    offset: register_a,
                },
            })?;
            
            tx.commit()?;
        }
        
        Ok(StepResult::Continue)
    }

    // Update process_c_function_return to use the static helper
    fn process_c_function_return(
        &mut self, 
        values: Vec<Value>, 
        context: ReturnContext
    ) -> LuaResult<StepResult> {
        println!("DEBUG CFUNC_RETURN: Processing {} return values", values.len());
        
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        match context {
            ReturnContext::Register { base, offset } => {
                println!("DEBUG CFUNC_RETURN: Storing to registers starting at base={}, offset={}", base, offset);
                
                // Store the return values
                if !values.is_empty() {
                    // Store each return value in its target register
                    for (i, value) in values.iter().enumerate() {
                        let reg_idx = base as usize + offset + i;
                        tx.set_register(self.current_thread, reg_idx, value.clone())?;
                    }
                } else {
                    // No return values - set the register to nil
                    tx.set_register(self.current_thread, base as usize + offset, Value::Nil)?;
                }
            },
            // Handle other context types
            ReturnContext::FinalResult => {
                println!("DEBUG CFUNC_RETURN: Final result context - will be handled by execute_function");
                // Final result will be handled by execute_function
            },
            ReturnContext::TableField { table, key } => {
                println!("DEBUG CFUNC_RETURN: Storing to table field, table={:?}, key={:?}", table, key);
                // Store in table
                if !values.is_empty() {
                    tx.set_table_field(table, key, values[0].clone())?;
                } else {
                    tx.set_table_field(table, key, Value::Nil)?;
                }
            },
            ReturnContext::Stack => {
                println!("DEBUG CFUNC_RETURN: Pushing to stack, {} values", values.len());
                // Push to stack
                for value in values {
                    tx.push_stack(self.current_thread, value)?;
                }
            },
            ReturnContext::Metamethod { context: mm_context } => {
                println!("DEBUG CFUNC_RETURN: Processing metamethod continuation");
                let result_value = values.get(0).cloned().unwrap_or(Value::Nil);
                
                // Use the free function to avoid borrowing self
                handle_metamethod_result(&mut tx, self.current_thread, result_value, mm_context)?;
            },
            ReturnContext::ForLoop { base, a, c, pc, sbx } => {
                println!("DEBUG CFUNC_RETURN: Processing for-loop results");
                let first_value = values.get(0).cloned().unwrap_or(Value::Nil);
                
                if !first_value.is_nil() {
                    // Loop continues
                    // Update control variable R(A+2)
                    tx.set_register(self.current_thread, base as usize + a + 2, first_value.clone())?;
                    
                    // Store loop variables in R(A+3) ... R(A+3+C-1)
                    for (i, value) in values.iter().enumerate() {
                        if i < c {
                            tx.set_register(self.current_thread, base as usize + a + 3 + i, value.clone())?;
                        }
                    }
                    
                    // Jump to loop body
                    let jump_pc = (pc as i32 + sbx) as usize;
                    tx.set_pc(self.current_thread, jump_pc)?;
                }
                // else: Loop is done, PC already incremented in TForLoop handler
            },
        }
        
        tx.commit()?;
        Ok(StepResult::Continue)
    }

    /// Process a metamethod call
    fn process_metamethod_call(
        &mut self,
        method: StringHandle,
        target: Value,
        args: Vec<Value>,
        context: ReturnContext,
    ) -> LuaResult<StepResult> {
        // Get the metamethod name to look up the actual method
        let method_name = {
            let mut tx = HeapTransaction::new(&mut self.heap);
            let name = tx.get_string_value(method)?;
            tx.commit()?;
            name
        };
        
        // Resolve the metamethod
        let mm_value = {
            let mut tx = HeapTransaction::new(&mut self.heap);
            
            // Use the metamethod type from the name
            let mm_type = match method_name.as_str() {
                "__index" => crate::lua::metamethod::MetamethodType::Index,
                "__newindex" => crate::lua::metamethod::MetamethodType::NewIndex,
                "__add" => crate::lua::metamethod::MetamethodType::Add,
                "__sub" => crate::lua::metamethod::MetamethodType::Sub,
                "__mul" => crate::lua::metamethod::MetamethodType::Mul,
                "__div" => crate::lua::metamethod::MetamethodType::Div,
                "__mod" => crate::lua::metamethod::MetamethodType::Mod,
                "__pow" => crate::lua::metamethod::MetamethodType::Pow,
                "__unm" => crate::lua::metamethod::MetamethodType::Unm,
                "__concat" => crate::lua::metamethod::MetamethodType::Concat,
                "__eq" => crate::lua::metamethod::MetamethodType::Eq,
                "__lt" => crate::lua::metamethod::MetamethodType::Lt,
                "__le" => crate::lua::metamethod::MetamethodType::Le,
                "__len" => crate::lua::metamethod::MetamethodType::Len,
                "__call" => crate::lua::metamethod::MetamethodType::Call,
                _ => {
                    return Err(LuaError::RuntimeError(format!("Unknown metamethod '{}'", method_name)));
                }
            };
            
            // Resolve the metamethod on the target
            crate::lua::metamethod::resolve_metamethod(&mut tx, &target, mm_type)?
        };
        
        // If we found a metamethod, execute it
        if let Some(mm_func) = mm_value {
            match mm_func {
                Value::Closure(closure) => {
                    // Queue function call with the metamethod as the function
                    // Note that Lua metamethod calls pass the original arguments
                    self.process_function_call(closure, args, context)
                },
                Value::CFunction(cfunc) => {
                    // For C functions, we need to set up and call directly
                    let mut tx = HeapTransaction::new(&mut self.heap);
                    
                    // Get current frame for register base
                    let frame = tx.get_current_frame(self.current_thread)?;
                    let base = frame.base_register as usize;
                    
                    // Calculate where to place arguments
                    let stack_top = tx.get_stack_size(self.current_thread)?;
                    
                    // Push arguments onto stack
                    for arg in &args {
                        tx.push_stack(self.current_thread, arg.clone())?;
                    }
                    
                    tx.commit()?;
                    
                    // Extract what we need for clean borrows
                    let func_copy = cfunc;
                    let args_copy = args.clone();
                    let args_len = args.len();
                    let thread_handle = self.current_thread;
                    
                    // Call C function
                    let mut ctx = ExecutionContext::new(self, stack_top, args_copy.len(), thread_handle);
                    
                    let result_count = match func_copy(&mut ctx) {
                        Ok(count) => count as usize,
                        Err(e) => {
                            // Clean up stack
                            let mut tx = HeapTransaction::new(&mut self.heap);
                            tx.pop_stack(thread_handle, args_len)?;
                            tx.commit()?;
                            return Err(e);
                        }
                    };
                    
                    // Collect results
                    let mut results = Vec::with_capacity(result_count);
                    let mut tx = HeapTransaction::new(&mut self.heap);
                    
                    for i in 0..result_count {
                        if let Ok(value) = tx.read_register(thread_handle, stack_top + i) {
                            results.push(value);
                        }
                    }
                    
                    // Clean up stack - remove args and results
                    tx.pop_stack(thread_handle, args_len + result_count)?;
                    
                    // Process results based on context
                    match context {
                        ReturnContext::Register { base, offset } => {
                            if !results.is_empty() {
                                tx.set_register(thread_handle, base as usize + offset, results[0].clone())?;
                            } else {
                                tx.set_register(thread_handle, base as usize + offset, Value::Nil)?;
                            }
                        },
                        ReturnContext::TableField { table, key } => {
                            let value = results.get(0).cloned().unwrap_or(Value::Nil);
                            tx.set_table_field(table, key, value)?;
                        },
                        ReturnContext::Stack => {
                            for result in results {
                                tx.push_stack(thread_handle, result)?;
                            }
                        },
                        ReturnContext::FinalResult => {
                            // Final result handled by execute_function
                        },
                        ReturnContext::Metamethod { context: mm_context } => {
                            let result_value = results.get(0).cloned().unwrap_or(Value::Nil);
                            
                            // Use the free function to avoid borrowing self
                            handle_metamethod_result(&mut tx, self.current_thread, result_value, mm_context)?;
                        },
                        ReturnContext::ForLoop { base, a, c, pc, sbx } => {
                            // Get first return value - if nil, the iteration is done
                            let first_result = results.get(0).cloned().unwrap_or(Value::Nil);
                            if !first_result.is_nil() {
                                // Iteration continues
                                
                                // Update control variable: R(A+2) = first result
                                tx.set_register(self.current_thread, base as usize + a + 2, first_result.clone())?;
                                
                                // Store all returned values as loop variables
                                for (i, value) in results.iter().enumerate() {
                                    if i < c {
                                        tx.set_register(self.current_thread, base as usize + a + 3 + i, value.clone())?;
                                    }
                                }
                                
                                // Pad with nil values if fewer than c values were returned
                                for i in results.len()..c {
                                    tx.set_register(self.current_thread, base as usize + a + 3 + i, Value::Nil)?;
                                }
                                
                                // Jump to loop body
                                let jump_pc = (pc as i32 + sbx) as usize;
                                tx.set_pc(self.current_thread, jump_pc)?;
                            }
                            // else: loop is done, PC already incremented in TForLoop handler
                        },
                    }
                    
                    tx.commit()?;
                    
                    Ok(StepResult::Continue)
                },
                _ => {
                    // Invalid metamethod type
                    Err(LuaError::TypeError {
                        expected: "function".to_string(),
                        got: mm_func.type_name().to_string(),
                    })
                }
            }
        } else {
            // No metamethod found, return error
            Err(LuaError::RuntimeError(format!("Metamethod '{}' not found", method_name)))
        }
    }


    
    fn process_pending_operation(&mut self, op: PendingOperation) -> LuaResult<StepResult> {
        match op {
            PendingOperation::FunctionCall { closure, args, context } => {
                println!("DEBUG PENDING_OP: Processing FunctionCall operation");
                self.process_function_call(closure, args, context)
            },
            PendingOperation::CFunctionReturn { values, context } => {
                println!("DEBUG PENDING_OP: Processing CFunctionReturn operation with {} values", values.len());
                for (i, value) in values.iter().enumerate() {
                    println!("DEBUG PENDING_OP:   CFunctionReturn value {}: {:?} ({})", 
                             i, value, value.type_name());
                }
                println!("DEBUG PENDING_OP: Calling process_c_function_return...");
                let result = self.process_c_function_return(values, context);
                println!("DEBUG PENDING_OP: process_c_function_return completed with result: {:?}", 
                         result.is_ok());
                result
            },
            PendingOperation::MetamethodCall { method, target, args, context } => {
                println!("DEBUG PENDING_OP: Processing MetamethodCall operation");
                self.process_metamethod_call(method, target, args, context)
            },
            PendingOperation::Concatenation { values, current_index, dest_register, mut accumulated } => {
                println!("DEBUG PENDING_OP: Processing Concatenation operation at index {}/{}", 
                         current_index, values.len());
                
                // Process concatenation with proper coercion and metamethod handling
                let mut tx = HeapTransaction::new(&mut self.heap);
                
                if current_index >= values.len() {
                    // All values processed, create final string
                    let result_string = accumulated.join("");
                    println!("DEBUG PENDING_OP: Concatenation complete, final result: '{}'", result_string);
                    let string_handle = tx.create_string(&result_string)?;
                    tx.set_register(self.current_thread, dest_register as usize, Value::String(string_handle))?;
                    tx.commit()?;
                    Ok(StepResult::Continue)
                } else {
                    // Process current value
                    let current_value = &values[current_index];
                    println!("DEBUG PENDING_OP: Concatenating value: {:?}", current_value);
                    
                    match current_value {
                        Value::String(handle) => {
                            // Direct string concatenation
                            let s = tx.get_string_value(*handle)?;
                            println!("DEBUG PENDING_OP: Adding string: '{}'", s);
                            accumulated.push(s);
                            
                            // Queue next concatenation step
                            tx.queue_operation(PendingOperation::Concatenation {
                                values: values.clone(),
                                current_index: current_index + 1,
                                dest_register,
                                accumulated,
                            })?;
                            
                            tx.commit()?;
                            Ok(StepResult::Continue)
                        },
                        Value::Number(n) => {
                            // Convert number to string
                            println!("DEBUG PENDING_OP: Converting number to string: {}", n);
                            accumulated.push(n.to_string());
                            
                            // Queue next concatenation step
                            tx.queue_operation(PendingOperation::Concatenation {
                                values: values.clone(),
                                current_index: current_index + 1,
                                dest_register,
                                accumulated,
                            })?;
                            
                            tx.commit()?;
                            Ok(StepResult::Continue)
                        },
                        Value::Boolean(b) => {
                            // Convert boolean to string
                            println!("DEBUG PENDING_OP: Converting boolean to string: {}", b);
                            accumulated.push(b.to_string());
                            
                            // Queue next concatenation step
                            tx.queue_operation(PendingOperation::Concatenation {
                                values: values.clone(),
                                current_index: current_index + 1,
                                dest_register,
                                accumulated,
                            })?;
                            
                            tx.commit()?;
                            Ok(StepResult::Continue)
                        },
                        Value::Nil => {
                            // Convert nil to string
                            println!("DEBUG PENDING_OP: Converting nil to string: nil");
                            accumulated.push("nil".to_string());
                            
                            // Queue next concatenation step
                            tx.queue_operation(PendingOperation::Concatenation {
                                values: values.clone(),
                                current_index: current_index + 1,
                                dest_register,
                                accumulated,
                            })?;
                            
                            tx.commit()?;
                            Ok(StepResult::Continue)
                        },
                        _ => {
                            println!("DEBUG PENDING_OP: Complex value, checking metamethods");
                            
                            // First check for __tostring metamethod
                            if let Some(_) = crate::lua::metamethod::resolve_metamethod(
                                &mut tx, current_value, crate::lua::metamethod::MetamethodType::ToString
                            )? {
                                println!("DEBUG PENDING_OP: Found __tostring metamethod");
                                
                                // Use __tostring metamethod
                                let method_name = tx.create_string("__tostring")?;
                                let target_value = current_value.clone();
                                
                                // Create a temporary register for the result
                                let temp_register = tx.get_stack_size(self.current_thread)?;
                                tx.push_stack(self.current_thread, Value::Nil)?;
                                
                                // Clone accumulated for the new continuation
                                let accumulated_clone = accumulated.clone();
                                
                                // Queue metamethod call
                                tx.queue_operation(PendingOperation::MetamethodCall {
                                    method: method_name,
                                    target: target_value.clone(),
                                    args: vec![target_value],
                                    context: ReturnContext::Register {
                                        base: 0,
                                        offset: temp_register,
                                    },
                                })?;
                                
                                // Then queue operation to read result and continue concat
                                tx.queue_operation(PendingOperation::ConcatAfterMetamethod {
                                    values,
                                    current_index,
                                    dest_register,
                                    accumulated: accumulated_clone,
                                    result_register: temp_register,
                                })?;
                                
                                tx.commit()?;
                                Ok(StepResult::Continue)
                            } else {
                                println!("DEBUG PENDING_OP: No __tostring metamethod, trying __concat");
                                
                                // Create a string from accumulated fragments
                                let result_string = accumulated.join("");
                                let mut tx2 = HeapTransaction::new(&mut self.heap); 
                                let left_str_handle = tx2.create_string(&result_string)?;
                                tx2.commit()?;
                                
                                // Start a new transaction
                                let mut tx = HeapTransaction::new(&mut self.heap);
                                let left_value = Value::String(left_str_handle);
                                
                                if let Some(_) = crate::lua::metamethod::resolve_metamethod(
                                    &mut tx, &left_value, crate::lua::metamethod::MetamethodType::Concat
                                )? {
                                    println!("DEBUG PENDING_OP: Found __concat metamethod on left operand");
                                    
                                    // Use left operand's __concat metamethod
                                    let method_name = tx.create_string("__concat")?;
                                    let right_value = current_value.clone();
                                    
                                    // Get remaining values past current
                                    let remaining_values = if current_index + 1 < values.len() {
                                        values[current_index + 1..].to_vec()
                                    } else {
                                        Vec::new()
                                    };
                                    
                                    // Queue metamethod call
                                    tx.queue_operation(PendingOperation::MetamethodCall {
                                        method: method_name,
                                        target: left_value.clone(),
                                        args: vec![left_value, right_value],
                                        context: ReturnContext::Register {
                                            base: 0, 
                                            offset: dest_register as usize,
                                        },
                                    })?;
                                    
                                    // If there are more values to concatenate, queue another CONCAT op
                                    if !remaining_values.is_empty() {
                                        tx.queue_operation(PendingOperation::ConcatContinuation {
                                            values: remaining_values,
                                            dest_register,
                                        })?;
                                    }
                                    
                                    tx.commit()?;
                                    Ok(StepResult::Continue)
                                } else if let Some(_) = crate::lua::metamethod::resolve_metamethod(
                                    &mut tx, current_value, crate::lua::metamethod::MetamethodType::Concat
                                )? {
                                    println!("DEBUG PENDING_OP: Found __concat metamethod on right operand");
                                    
                                    // Use right operand's __concat metamethod
                                    let method_name = tx.create_string("__concat")?;
                                    let right_value = current_value.clone();
                                    
                                    // Get remaining values past current
                                    let remaining_values = if current_index + 1 < values.len() {
                                        values[current_index + 1..].to_vec()
                                    } else {
                                        Vec::new()
                                    };
                                    
                                    tx.queue_operation(PendingOperation::MetamethodCall {
                                        method: method_name,
                                        target: right_value.clone(),
                                        args: vec![left_value, right_value],
                                        context: ReturnContext::Register {
                                            base: 0,
                                            offset: dest_register as usize,
                                        },
                                    })?;
                                    
                                    // If there are more values to concatenate, queue another CONCAT op
                                    if !remaining_values.is_empty() {
                                        tx.queue_operation(PendingOperation::ConcatContinuation {
                                            values: remaining_values,
                                            dest_register,
                                        })?;
                                    }
                                    
                                    tx.commit()?;
                                    Ok(StepResult::Continue)
                                } else {
                                    println!("DEBUG PENDING_OP: No __concat or __tostring metamethod found, failing");
                                    // No __concat or __tostring metamethod
                                    tx.commit()?;
                                    Err(LuaError::TypeError {
                                        expected: "string or number".to_string(),
                                        got: current_value.type_name().to_string(),
                                    })
                                }
                            }
                        }
                    }
                }
            },

            PendingOperation::ConcatAfterMetamethod { values, current_index, dest_register, accumulated, result_register } => {
                println!("DEBUG PENDING_OP: Processing ConcatAfterMetamethod - continuing concatenation");
                let mut tx = HeapTransaction::new(&mut self.heap);
                
                // Read the metamethod result
                let result = tx.read_register(self.current_thread, result_register)?;
                
                // Convert to string as needed
                let result_str = match result {
                    Value::String(handle) => {
                        tx.get_string_value(handle)?
                    },
                    Value::Number(n) => {
                        n.to_string()
                    },
                    Value::Boolean(b) => {
                        b.to_string()
                    },
                    Value::Nil => {
                        "nil".to_string()
                    },
                    _ => {
                        // Unable to convert
                        println!("DEBUG PENDING_OP: Metamethod didn't return a string, got: {:?}", result);
                        tx.commit()?;
                        return Err(LuaError::TypeError {
                            expected: "string from __tostring metamethod".to_string(),
                            got: result.type_name().to_string(),
                        });
                    }
                };
                
                // Add the string to accumulated
                let mut new_accumulated = accumulated;
                new_accumulated.push(result_str);
                println!("DEBUG PENDING_OP: Added string from metamethod, continuing with index {}+1", current_index);
                
                // Continue concatenation with the next value
                tx.queue_operation(PendingOperation::Concatenation {
                    values, 
                    current_index: current_index + 1,
                    dest_register,
                    accumulated: new_accumulated,
                })?;
                
                // Clean up temporary register
                tx.pop_stack(self.current_thread, 1)?;
                
                tx.commit()?;
                Ok(StepResult::Continue)
            },

            PendingOperation::ConcatContinuation { values, dest_register } => {
                println!("DEBUG PENDING_OP: Processing ConcatContinuation with {} values", values.len());
                
                let mut tx = HeapTransaction::new(&mut self.heap);
                
                // Read the current intermediate result for base concatenation
                let current_result = tx.read_register(self.current_thread, dest_register as usize)?;
                
                // Convert it to string
                let base_str = match current_result {
                    Value::String(handle) => {
                        tx.get_string_value(handle)?
                    },
                    _ => {
                        // This should not happen, as the metamethod should have put a string here
                        println!("DEBUG PENDING_OP: Expected string as concatenation base, got: {:?}", current_result);
                        tx.commit()?;
                        return Err(LuaError::TypeError {
                            expected: "string as concatenation base".to_string(),
                            got: current_result.type_name().to_string(),
                        });
                    }
                };
                
                // Initialize accumulated values with the base string
                let accumulated = vec![base_str];
                
                // Queue a new concatenation operation
                tx.queue_operation(PendingOperation::Concatenation {
                    values,
                    current_index: 0,
                    dest_register,
                    accumulated,
                })?;
                
                tx.commit()?;
                Ok(StepResult::Continue)
            },

            PendingOperation::PostCallRegisterFix { dest_register, source_register, next_pc } => {
                println!("DEBUG POST_CALL_FIX: Fixing registers after CALL");
                let mut tx = HeapTransaction::new(&mut self.heap);
                
                // Get current PC
                let thread = self.current_thread;
                let frame = tx.get_current_frame(thread)?;
                let current_pc = frame.pc;
                
                println!("DEBUG POST_CALL_FIX: Current PC: {}, Expected next PC: {}", current_pc, next_pc);
                
                // Only execute the fix if we're at the expected PC (immediately after the CALL)
                if current_pc == next_pc {
                    println!("DEBUG POST_CALL_FIX: Moving value from R({}) to R({})", source_register, dest_register);
                    
                    // Get the value from the source register
                    let value = tx.read_register(thread, source_register as usize)?;
                    println!("DEBUG POST_CALL_FIX: Moving value: {:?}", value);
                    
                    // Store it in the destination register
                    tx.set_register(thread, dest_register as usize, value)?;
                    
                    // Clean up - remove the temporary register if it was after the stack top
                    // (Only do this if it's not part of regular registers)
                    let base = frame.base_register as usize;
                    if source_register as usize >= base + 10 { // Heuristic for stack-allocated temporaries
                        println!("DEBUG POST_CALL_FIX: Cleaning up temporary register");
                        tx.pop_stack(thread, 1)?;
                    }
                } else {
                    println!("DEBUG POST_CALL_FIX: Skipping fix (current PC {} != expected {})", current_pc, next_pc);
                    // Skip the fix - we're not at the right PC
                    // Requeue it for later
                    tx.queue_operation(PendingOperation::PostCallRegisterFix {
                        dest_register,
                        source_register,
                        next_pc,
                    })?;
                }
                
                tx.commit()?;
                Ok(StepResult::Continue)
            },

            PendingOperation::MoveAfterInstruction { from_register, to_register, execution_pc } => {
                println!("DEBUG MOVE_AFTER: Checking whether to move from {} to {}, current PC: {}",
                         from_register, to_register, execution_pc);
                         
                let mut tx = HeapTransaction::new(&mut self.heap);
                
                // Get current PC
                let frame = tx.get_current_frame(self.current_thread)?;
                let current_pc = frame.pc;
                
                if current_pc == execution_pc {
                    println!("DEBUG MOVE_AFTER: Moving value from register {} to {}", from_register, to_register);
                    
                    // Copy the value from source to destination register
                    let value = tx.read_register(self.current_thread, from_register)?;
                    tx.set_register(self.current_thread, to_register, value)?;
                    
                    // Free the temporary register
                    tx.pop_stack(self.current_thread, 1)?;
                    
                    tx.commit()?;
                    Ok(StepResult::Continue)
                } else {
                    println!("DEBUG MOVE_AFTER: Not ready to move (PC: {} != {}), requeuing", current_pc, execution_pc);
                    
                    // Requeue the operation for later
                    tx.queue_operation(PendingOperation::MoveAfterInstruction {
                        from_register,
                        to_register,
                        execution_pc,
                    })?;
                    
                    tx.commit()?;
                    Ok(StepResult::Continue)
                }
            },


            _ => {
                println!("DEBUG PENDING_OP: Unimplemented operation type: {:?}", 
                         std::mem::discriminant(&op));
                Err(LuaError::NotImplemented("Pending operation type".to_string()))
            },
        }
    }
    
    /// Process a function call
    fn process_function_call(
        &mut self,
        closure: ClosureHandle,
        args: Vec<Value>,
        context: ReturnContext,
    ) -> LuaResult<StepResult> {
        // First transaction for function call setup
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Get closure info
        let closure_obj = tx.get_closure(closure)?;
        let num_params = closure_obj.proto.num_params as usize;
        let is_vararg = closure_obj.proto.is_vararg;
        let max_stack = closure_obj.proto.max_stack_size as usize;
        
        // Determine stack base for new frame
        let current_top = tx.get_stack_size(self.current_thread)?;
        let new_base = current_top;
        
        // Collect varargs if needed
        let varargs = if is_vararg && args.len() > num_params {
            // Collect excess arguments as varargs
            Some(args[num_params..].to_vec())
        } else {
            None
        };
        
        // CRITICAL FIX: We need to ensure we have enough stack space before proceeding
        // The total stack space needed is:
        // 1. Base register (new_base)
        // 2. All registers the function will use (max_stack)
        // 3. Additional safety margin of 1 to prevent off-by-one errors
        let total_stack_needed = new_base + max_stack + 1; // +1 safety margin
        
        // Get current stack size
        let current_stack_size = tx.get_stack_size(self.current_thread)?;
        
        // Calculate additional space needed (if any)
        if current_stack_size < total_stack_needed {
            let additional_needed = total_stack_needed - current_stack_size;
            
            // Add Nil values to extend the stack
            for _ in 0..additional_needed {
                tx.push_stack(self.current_thread, Value::Nil)?;
            }
        }
        
        // First, push parameters
        for i in 0..num_params {
            let value = if i < args.len() {
                args[i].clone()
            } else {
                Value::Nil
            };
            tx.set_register(self.current_thread, new_base + i, value)?;
        }
        
        // Initialize remaining registers for local variables and temporaries
        for i in num_params..max_stack {
            tx.set_register(self.current_thread, new_base + i, Value::Nil)?;
        }
        
        // Create new call frame
        let new_frame = CallFrame {
            closure,
            pc: 0,
            base_register: new_base as u16,
            expected_results: match &context {
                ReturnContext::Register { .. } => Some(1),
                _ => None,
            },
            varargs,
        };
        
        // Push call frame
        tx.push_call_frame(self.current_thread, new_frame)?;
        
        // Commit transaction
        tx.commit()?;
        
        // Store return context in a separate scope to avoid borrow issues
        let call_depth = {
            // Second transaction just to get call depth
            let mut tx2 = HeapTransaction::new(&mut self.heap);
            
            // Get current call depth (after we pushed the frame)
            let frame_count = tx2.get_thread_call_depth(self.current_thread)?;
            tx2.commit()?;
            frame_count
        };
        
        // Store return context
        self.return_contexts.insert(call_depth, context);
        
        Ok(StepResult::Continue)
    }
    
    /// Get a constant from a closure
    fn get_constant(&self, tx: &mut HeapTransaction, closure: ClosureHandle, index: usize) -> LuaResult<Value> {
        let closure_obj = tx.get_closure(closure)?;
        
        closure_obj.proto.constants.get(index)
            .cloned()
            .ok_or(LuaError::RuntimeError(format!(
                "Constant index {} out of bounds",
                index
            )))
    }
    
    /// Get call depth by counting frames
    fn get_call_depth(&mut self) -> LuaResult<usize> {
        let mut tx = HeapTransaction::new(&mut self.heap);
        let depth = tx.get_thread_call_depth(self.current_thread)?;
        tx.commit()?;
        Ok(depth)
    }
    
    /// Check if we should kill the script
    fn should_kill(&self) -> bool {
        // TODO: Check kill flag
        false
    }
    
    /// Check if we've exceeded the timeout
    fn check_timeout(&self) -> bool {
        if let (Some(start), Some(timeout)) = (self.start_time, self.timeout) {
            start.elapsed() > timeout
        } else {
            false
        }
    }
    
    /// Initialize the standard library
    pub fn init_stdlib(&mut self) -> LuaResult<()> {
        crate::lua::stdlib::init_all(self)
    }
    
    /// Execute a compiled module
    pub fn execute_module(&mut self, module: &crate::lua::compiler::CompiledModule, args: &[Value]) -> LuaResult<Value> {
        println!("DEBUG: execute_module - Starting execution");
        
        // Load the module into the heap using a transaction
        let mut tx = HeapTransaction::new(&mut self.heap);
        
        // Use the loader module to load the compiled code
        let proto_handle = crate::lua::compiler::loader::load_module(&mut tx, module)?;
        
        // Get the prototype to check max stack size
        let proto = tx.get_function_proto_copy(proto_handle)?;
        println!("DEBUG: Main function - max_stack_size: {}", proto.max_stack_size);
        
        // CRITICAL FIX: Ensure the stack has sufficient space for all registers
        // the main function might access
        let thread_handle = self.current_thread;
        
        // Get current stack size
        let current_stack_size = {
            let thread_obj = tx.get_thread(thread_handle)?;
            thread_obj.stack.len()
        };
        println!("DEBUG: Current stack size: {}", current_stack_size);
        
        // Calculate needed size: need to accommodate proto.max_stack_size registers
        // Add extra safety margin
        let needed_stack_size = proto.max_stack_size as usize + 10; // +10 safety margin
        println!("DEBUG: Needed stack size: {}", needed_stack_size);
        
        // Ensure stack has at least needed_stack_size elements
        if current_stack_size < needed_stack_size {
            println!("DEBUG: Growing stack to {}", needed_stack_size);
            // Add Nil values to extend the stack to exactly needed_stack_size
            for i in current_stack_size..needed_stack_size {
                println!("DEBUG: Pushing Nil to stack at position {}", i);
                tx.push_stack(thread_handle, Value::Nil)?;
            }
        }
        
        // Double-check that we have sufficient stack space
        let final_stack_size = {
            let thread_obj = tx.get_thread(thread_handle)?;
            thread_obj.stack.len()
        };
        println!("DEBUG: Final stack size: {}", final_stack_size);
        
        if final_stack_size < needed_stack_size {
            return Err(LuaError::InternalError(format!(
                "Failed to initialize stack to required size: {} (current: {})", 
                needed_stack_size, final_stack_size
            )));
        }
        
        // Create the main closure
        let closure = Closure {
            proto,
            upvalues: Vec::new(), // Main chunk has no upvalues
        };
        
        let closure_handle = tx.create_closure(closure)?;
        println!("DEBUG: Created main closure");
        
        // Commit the transaction
        tx.commit()?;
        println!("DEBUG: Transaction committed");
        
        // Execute the main closure
        println!("DEBUG: Calling execute_function");
        self.execute_function(closure_handle, args)
    }
    
    /// Get the heap for direct access (used in test helpers)
    pub fn heap(&self) -> &LuaHeap {
        &self.heap
    }
    
    /// Create a table
    pub fn create_table(&mut self) -> LuaResult<TableHandle> {
        let mut tx = HeapTransaction::new(&mut self.heap);
        let table = tx.create_table()?;
        tx.commit()?;
        Ok(table)
    }
    
    /// Create a string
    pub fn create_string(&mut self, s: &str) -> LuaResult<StringHandle> {
        let mut tx = HeapTransaction::new(&mut self.heap);
        let handle = tx.create_string(s)?;
        tx.commit()?;
        Ok(handle)
    }
    
    /// Set a table field by numeric index
    pub fn set_table_index(&mut self, table: TableHandle, index: usize, value: Value) -> LuaResult<()> {
        let mut tx = HeapTransaction::new(&mut self.heap);
        tx.set_table_field(table, Value::Number(index as f64), value)?;
        tx.commit()?;
        Ok(())
    }
    
    /// Get the global table
    pub fn globals(&mut self) -> LuaResult<TableHandle> {
        self.heap.globals()
    }
    
    /// Set a table field 
    pub fn set_table(&mut self, table: TableHandle, key: Value, value: Value) -> LuaResult<()> {
        let mut tx = HeapTransaction::new(&mut self.heap);
        tx.set_table_field(table, key, value)?;
        tx.commit()?;
        Ok(())
    }
    
    /// Capture the current execution traceback for error reporting
    pub fn capture_traceback(&mut self) -> LuaResult<crate::lua::error::LuaTraceback> {
        let mut frames = Vec::new();
        
        // Two-phase borrowing to avoid borrow issues:
        // First, collect all call frames
        let call_frames = {
            let mut tx = HeapTransaction::new(&mut self.heap);
            let thread = tx.get_thread(self.current_thread)?;
            
            // Clone the call frames to avoid borrow issues
            thread.call_frames.clone()
        };
        
        // Process each call frame to build the traceback
        // We walk backwards through the frames (most recent first)
        for frame in call_frames.iter().rev() {
            // Create a call info entry for this frame
            let call_info = crate::lua::error::CallInfo {
                function_name: Some("function".to_string()), // Default name
                source_file: None,  // Would come from debug info in real implementation
                line_number: None,  // Would come from debug info in real implementation
                pc: frame.pc,
            };
            
            frames.push(call_info);
        }
        
        // Return the traceback
        Ok(crate::lua::error::LuaTraceback { frames })
    }
    
    /// Create a runtime error with traceback
    pub fn create_runtime_error_with_trace(&mut self, message: String) -> LuaError {
        use crate::lua::error::{LuaError, LuaTraceback, CallInfo};
        
        // Two-phase approach to avoid borrow checker issues
        let call_frames = {
            let mut tx = HeapTransaction::new(&mut self.heap);
            
            // Get the current thread and clone its call frames
            let thread = match tx.get_thread(self.current_thread) {
                Ok(t) => t,
                Err(_) => return LuaError::RuntimeError(message), // Fallback
            };
            
            thread.call_frames.clone()
        };
        
        // Create the traceback from cloned call frames
        let frames = call_frames.iter().rev().map(|frame| {
            CallInfo {
                function_name: Some("function".to_string()), // Default name
                source_file: None,  // Would come from debug info
                line_number: None,  // Would come from debug info
                pc: frame.pc,
            }
        }).collect();
        
        let traceback = LuaTraceback { frames };
        LuaError::RuntimeErrorWithTrace { message, traceback }
    }
    
    /// Create a type error with traceback
    pub fn create_type_error_with_trace(&mut self, expected: String, got: String) -> LuaError {
        use crate::lua::error::{LuaError, LuaTraceback, CallInfo};
        
        // Two-phase approach to avoid borrow checker issues
        let call_frames = {
            let mut tx = HeapTransaction::new(&mut self.heap);
            
            // Get the current thread and clone its call frames
            let thread = match tx.get_thread(self.current_thread) {
                Ok(t) => t,
                Err(_) => return LuaError::TypeError { expected, got }, // Fallback
            };
            
            thread.call_frames.clone()
        };
        
        // Create the traceback from cloned call frames
        let frames = call_frames.iter().rev().map(|frame| {
            CallInfo {
                function_name: Some("function".to_string()), // Default name
                source_file: None,  // Would come from debug info
                line_number: None,  // Would come from debug info
                pc: frame.pc,
            }
        }).collect();
        
        let traceback = LuaTraceback { frames };
        LuaError::TypeErrorWithTrace { expected, got, traceback }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lua::value;
    
    #[test]
    fn test_vm_creation() {
        let vm = LuaVM::new().unwrap();
        assert_eq!(vm.execution_state, ExecutionState::Ready);
        assert_eq!(vm.instruction_count, 0);
    }
    
    #[test]
    fn test_instruction_parsing() {
        // Test MOVE instruction: opcode=0, A=1, B=2
        let instr = Instruction(0x00004040);
        assert_eq!(instr.opcode(), OpCode::Move);
        assert_eq!(instr.a(), 1);
        assert_eq!(instr.b(), 0);
        
        // Test LOADK instruction: opcode=1, A=0, Bx=1
        let instr = Instruction(0x00004001);
        assert_eq!(instr.opcode(), OpCode::LoadK);
        assert_eq!(instr.a(), 0);
        assert_eq!(instr.bx(), 1);
    }
    
    #[test]
    fn test_table_metamethod_operations() {
        let mut vm = LuaVM::new().unwrap();
        
        // Create a table with a metatable
        let (table, metatable) = {
            let mut tx = HeapTransaction::new(&mut vm.heap);
            let t = tx.create_table().unwrap();
            let mt = tx.create_table().unwrap();
            
            // Set the metatable
            tx.set_table_metatable(t, Some(mt)).unwrap();
            
            // Add an __index method that's a table
            let index_str = tx.create_string("__index").unwrap();
            let index_table = tx.create_table().unwrap();
            
            // Put a value in the __index table
            let key = tx.create_string("test_key").unwrap();
            let value = tx.create_string("test_value").unwrap();
            tx.set_table_field(index_table, Value::String(key), Value::String(value)).unwrap();
            
            // Set __index to the table
            tx.set_table_field(mt, Value::String(index_str), Value::Table(index_table)).unwrap();
            
            tx.commit().unwrap();
            
            (t, mt)
        };
        
        // Test that we can get a value through the __index metamethod
        {
            let mut tx = HeapTransaction::new(&mut vm.heap);
            let key = tx.create_string("test_key").unwrap();
            
            // This should trigger the __index metamethod lookup
            let mm = crate::lua::metamethod::resolve_metamethod(
                &mut tx, &Value::Table(table), crate::lua::metamethod::MetamethodType::Index
            ).unwrap();
            
            assert!(mm.is_some(), "Should find __index metamethod");
            
            // The metamethod is a table, so we can look up the key in it
            match mm {
                Some(Value::Table(index_table)) => {
                    let result = tx.read_table_field(index_table, &Value::String(key)).unwrap();
                    match result {
                        Value::String(s) => {
                            let str_val = tx.get_string_value(s).unwrap();
                            assert_eq!(str_val, "test_value");
                        },
                        _ => panic!("Expected string value"),
                    }
                },
                _ => panic!("Expected table __index"),
            }
        }
    }
    
    #[test]
    fn test_comparison_metamethods() {
        let mut vm = LuaVM::new().unwrap();
        
        // Create two tables with metamethods
        let (table1, table2, metatable) = {
            let mut tx = HeapTransaction::new(&mut vm.heap);
            
            // Create tables and metatable
            let t1 = tx.create_table().unwrap();
            let t2 = tx.create_table().unwrap();
            let mt = tx.create_table().unwrap();
            
            // Create string keys first to avoid borrow issues
            let value_key = tx.create_string("value").unwrap();
            
            // Set data in tables
            tx.set_table_field(t1, Value::String(value_key.clone()), Value::Number(10.0)).unwrap();
            tx.set_table_field(t2, Value::String(value_key), Value::Number(20.0)).unwrap();
            
            // Set both tables to use the same metatable
            tx.set_table_metatable(t1, Some(mt)).unwrap();
            tx.set_table_metatable(t2, Some(mt)).unwrap();
            
            // Create string keys for metamethods
            let eq_name = tx.create_string("__eq").unwrap();
            let lt_name = tx.create_string("__lt").unwrap();
            
            // Simple CFunction that always returns true for testing
            let eq_func: CFunction = |ctx| {
                ctx.push_result(Value::Boolean(true)).unwrap();
                Ok(1)
            };
            
            tx.set_table_field(mt, Value::String(eq_name), Value::CFunction(eq_func)).unwrap();
            
            // CFunction that compares the "value" field
            let lt_func: CFunction = |ctx| {
                // In a real implementation, this would properly access the values
                // For this test, we'll just return a fixed result
                ctx.push_result(Value::Boolean(true)).unwrap();
                Ok(1)
            };
            
            tx.set_table_field(mt, Value::String(lt_name), Value::CFunction(lt_func)).unwrap();
            
            tx.commit().unwrap();
            
            (t1, t2, mt)
        };
        
        // Test that we can successfully call the __eq metamethod
        {
            let mut tx = HeapTransaction::new(&mut vm.heap);
            
            // Check for __eq metamethod
            let eq_mm_opt = crate::lua::metamethod::resolve_metamethod(
                &mut tx, &Value::Table(table1), crate::lua::metamethod::MetamethodType::Eq
            ).unwrap();
            
            assert!(eq_mm_opt.is_some(), "Should find __eq metamethod");
            assert!(matches!(eq_mm_opt.unwrap(), Value::CFunction(_)), "Expected CFunction for __eq");
            
            // Check for __lt metamethod
            let lt_mm_opt = crate::lua::metamethod::resolve_metamethod(
                &mut tx, &Value::Table(table1), crate::lua::metamethod::MetamethodType::Lt
            ).unwrap();
            
            assert!(lt_mm_opt.is_some(), "Should find __lt metamethod");
            assert!(matches!(lt_mm_opt.unwrap(), Value::CFunction(_)), "Expected CFunction for __lt");
            
            // Test that tables with different values compare as expected
            let found_binary_mm = crate::lua::metamethod::resolve_binary_metamethod(
                &mut tx, &Value::Table(table1), &Value::Table(table2), 
                crate::lua::metamethod::MetamethodType::Lt
            ).unwrap();
            
            assert!(found_binary_mm.is_some(), "Should find binary metamethod");
        }
    }
    
    #[test]
    fn test_flow_control_operations() {
        // Create a VM instance
        let mut vm = LuaVM::new().unwrap();
        
        // Create properly encoded bytecode instructions
        // Format: lowest 6 bits = opcode, bits 6-13 = A, bits 14-22 = C, bits 23-31 = B
        // For Jmp: sbx = bits 14-31, PC += sbx (sbx is signed, biased by 131071)
        let move_instr = 0 | (1 << 6) | (0 << 23);    // MOVE R(1), R(0) - opcode=0, A=1, B=0
        let loadbool_instr = 2 | (0 << 6) | (1 << 23); // LOADBOOL R(0), true, false - opcode=2, A=0, B=1, C=0
        let jmp_instr = 22 | (0 << 6) | ((3+131071) << 14); // JMP +3 - opcode=22, sBx=3 (biased by 131071)
        let test_instr_fail = 26 | (3 << 6) | (0 << 14); // TEST R(3), false - opcode=26, A=3, C=0
        let test_instr_skip = 26 | (2 << 6) | (1 << 14); // TEST R(2), true - opcode=26, A=2, C=1
        let testset_instr = 27 | (3 << 6) | (2 << 23) | (1 << 14); // TESTSET R(3), R(2), true - opcode=27, A=3, B=2, C=1
        
        let test_bytecode = vec![
            move_instr,         // 0: MOVE R(1), R(0)         ; Initialize R(1) = R(0) (nil)
            loadbool_instr,     // 1: LOADBOOL R(0), true, 0  ; R(0) = true
            jmp_instr,          // 2: JMP +3                  ; Jump to instruction 5
            0,                  // 3: (Skipped by JMP)
            0,                  // 4: (Skipped by JMP)
            test_instr_fail,    // 5: TEST R(3), 0            ; if !R(3).is_falsey() == false, NO skip (match)
            0,                  // 6: (Should be executed as R(3) is nil/falsey)
            test_instr_skip,    // 7: TEST R(2), 1            ; if !R(2).is_falsey() == true, should skip (no match)
            0,                  // 8: (Should be skipped as R(2) is nil/falsey)
            testset_instr,      // 9: TESTSET R(3), R(2), 1   ; if !R(2).is_falsey() == true, R(3)=R(2), else skip
        ];
        
        // Create a function prototype
        let proto = FunctionProto {
            bytecode: test_bytecode,
            constants: vec![],
            num_params: 0,
            is_vararg: false,
            max_stack_size: 10,
            upvalues: vec![],
        };
        
        // Create a closure
        let closure = Closure {
            proto,
            upvalues: vec![],
        };
        
        // Create the initial state
        let thread_handle = vm.current_thread;
        let mut tx = HeapTransaction::new(&mut vm.heap);
        
        // Create a closure handle
        let closure_handle = tx.create_closure(closure).unwrap();
        
        // Create a call frame
        let frame = CallFrame {
            closure: closure_handle,
            pc: 0,
            base_register: 0,
            expected_results: None,
            varargs: None,
        };
        
        // Push the frame to the thread
        tx.push_call_frame(thread_handle, frame).unwrap();
        
        // Initialize stack (registers)
        for _i in 0..10 {
            tx.push_stack(thread_handle, Value::Nil).unwrap();
        }
        
        tx.commit().unwrap();
        
        // Helper function to get the PC (accessing frame directly)
        let get_pc = |vm: &mut LuaVM| -> usize {
            let mut tx = HeapTransaction::new(&mut vm.heap);
            let frame = tx.get_current_frame(vm.current_thread).unwrap();
            let pc = frame.pc;
            tx.commit().unwrap();
            pc
        };
        
        // Helper to read register value
        let read_register = |vm: &mut LuaVM, reg: usize| -> Value {
            let mut tx = HeapTransaction::new(&mut vm.heap);
            let value = tx.read_register(vm.current_thread, reg).unwrap();
            tx.commit().unwrap();
            value
        };
        
        println!("Starting flow control test");
        
        // Now execute each instruction and check its effect
        
        // Initial state: PC should be 0
        assert_eq!(get_pc(&mut vm), 0, "Initial PC should be 0");
        
        // Step 1: MOVE instruction - should set R(1) to nil (R(0))
        vm.step().unwrap();
        assert_eq!(get_pc(&mut vm), 1, "PC should be 1 after first step");
        assert!(read_register(&mut vm, 1).is_nil(), "R(1) should be nil after MOVE");
        
        // Step 2: LOADBOOL - should set R(0) to true
        vm.step().unwrap();
        assert_eq!(get_pc(&mut vm), 2, "PC should be 2 after LOADBOOL");
        assert_eq!(read_register(&mut vm, 0), Value::Boolean(true), "R(0) should be true after LOADBOOL");
        
        // Step 3: JMP +3 - should jump to instruction 5
        vm.step().unwrap();
        assert_eq!(get_pc(&mut vm), 5, "PC should be 5 after JMP +3");
        
        // Step 4: TEST R(3), 0 - R(3) is nil (falsey) which matches expected=false, so NO skip
        vm.step().unwrap();
        assert_eq!(get_pc(&mut vm), 6, "PC should be 6 after TEST (no skip)");
        
        // Step 5: Execute instruction 6 (which was not skipped)
        vm.step().unwrap();
        assert_eq!(get_pc(&mut vm), 7, "PC should be 7 after dummy instruction");
        
        // Step 6: TEST R(2), 1 - R(2) is nil (falsey), but expected=true, so they don't match - SHOULD skip
        println!("Before TEST instruction: PC = {}", get_pc(&mut vm));
        vm.step().unwrap();
        println!("After TEST instruction: PC = {}", get_pc(&mut vm));
        
        // With the fixed TEST opcode, PC should increment only once
        assert_eq!(get_pc(&mut vm), 8, "PC should be 8 after TEST (with single skip)");
        
        // Step 7: Execute instruction at PC 8 (the one after the skipped instruction)
        vm.step().unwrap();
        assert_eq!(get_pc(&mut vm), 9, "PC should be 9 after executing instruction 8");
        
        // Step 8: TESTSET R(3), R(2), 1 - R(2) is falsey, expected=true (C=1), so they don't match
        // When they don't match: don't set R(3) and skip next instruction
        let original_r3 = read_register(&mut vm, 3);
        println!("Before TESTSET: PC = {}, R(3) = {:?}", get_pc(&mut vm), original_r3);
        vm.step().unwrap();
        println!("After TESTSET: PC = {}", get_pc(&mut vm));
        
        // With the fixed TESTSET opcode, PC should increment only once
        assert_eq!(get_pc(&mut vm), 10, "PC should be 10 after TESTSET (with single skip)");
        
        // R(3) should not have been changed, as the test condition failed
        let final_r3 = read_register(&mut vm, 3);
        assert_eq!(original_r3, final_r3, "R(3) should not have been changed by TESTSET");
        
        println!("Flow control test completed successfully");
    }

    #[test]
    fn test_for_loop_operations() {
        // Create a VM instance
        let mut vm = LuaVM::new().unwrap();
        
        // This test sets up a correct numeric for loop bytecode sequence
        // Following Lua 5.1 semantics, we'll create a for i=1,3,1 do ... end loop
        
        // Register layout:
        // R(0) = initial index (1)
        // R(1) = limit (3)
        // R(2) = step (1)
        // R(3) = loop var (i)
        
        // Create a constant for index (1)
        let loadk_index = 1 | (0 << 6) | (0 << 14); // LOADK R(0), K(0)
        
        // Create a constant for limit (3)
        let loadk_limit = 1 | (1 << 6) | (1 << 14); // LOADK R(1), K(1)
        
        // Create a constant for step (1)
        let loadk_step = 1 | (2 << 6) | (2 << 14); // LOADK R(2), K(2)
        
        // ForPrep - initialize loop and jump to loop head
        // Jump to instruction 6 (past the loop body)
        let forprep_instr = 31 | (0 << 6) | ((2+131071) << 14); // ForPrep R(0), +2
        
        // Dummy instruction for loop body
        let body_instr = 0; // NOP (MOVE R(0), R(0))
        
        // ForLoop - increment loop counter and conditionally loop back
        // Jump back to instruction 4 (loop body) if not done
        let forloop_instr = 32 | (0 << 6) | ((131071-1) << 14); // ForLoop R(0), -1
        
        let test_bytecode = vec![
            loadk_index,    // 0: LOADK R(0), K(0)  ; Initialize index to 1
            loadk_limit,    // 1: LOADK R(1), K(1)  ; Set limit to 3
            loadk_step,     // 2: LOADK R(2), K(2)  ; Set step to 1
            forprep_instr,  // 3: FORPREP R(0), +2  ; Initialize loop and jump to 5
            body_instr,     // 4: Loop body (MOVE instruction acting as NOP)
            forloop_instr,  // 5: FORLOOP R(0), -1  ; Loop back to 4 if not done
            0,              // 6: End of loop
        ];
        
        // Create constants for the loop
        let constants = vec![
            Value::Number(1.0), // Initial value
            Value::Number(3.0), // Limit
            Value::Number(1.0), // Step
        ];
        
        // Create a function prototype
        let proto = FunctionProto {
            bytecode: test_bytecode,
            constants,
            num_params: 0,
            is_vararg: false,
            max_stack_size: 10,
            upvalues: vec![],
        };
        
        // Create a closure
        let closure = Closure {
            proto,
            upvalues: vec![],
        };
        
        // Create the initial state
        let thread_handle = vm.current_thread;
        let mut tx = HeapTransaction::new(&mut vm.heap);
        
        // Create a closure handle
        let closure_handle = tx.create_closure(closure).unwrap();
        
        // Create a call frame
        let frame = CallFrame {
            closure: closure_handle,
            pc: 0,
            base_register: 0,
            expected_results: None,
            varargs: None,
        };
        
        // Push the frame to the thread
        tx.push_call_frame(thread_handle, frame).unwrap();
        
        // Initialize stack (registers)
        for _i in 0..10 {
            tx.push_stack(thread_handle, Value::Nil).unwrap();
        }
        
        tx.commit().unwrap();
        
        // Helper function to get the PC
        let get_pc = |vm: &mut LuaVM| -> usize {
            let mut tx = HeapTransaction::new(&mut vm.heap);
            let frame = tx.get_current_frame(vm.current_thread).unwrap();
            let pc = frame.pc;
            tx.commit().unwrap();
            pc
        };
        
        // Helper to read register value
        let read_register = |vm: &mut LuaVM, reg: usize| -> Value {
            let mut tx = HeapTransaction::new(&mut vm.heap);
            let value = tx.read_register(vm.current_thread, reg).unwrap();
            tx.commit().unwrap();
            value
        };
        
        println!("Starting loop test with correct bytecode structure");
        
        // Initial state
        assert_eq!(get_pc(&mut vm), 0, "Initial PC should be 0");
        
        // Step 1: LoadK R(0), K(0) - load index initial value (1)
        vm.step().unwrap();
        assert_eq!(get_pc(&mut vm), 1);
        assert_eq!(read_register(&mut vm, 0), Value::Number(1.0));
        
        // Step 2: LoadK R(1), K(1) - load limit value (3)
        vm.step().unwrap();
        assert_eq!(get_pc(&mut vm), 2);
        assert_eq!(read_register(&mut vm, 1), Value::Number(3.0));
        
        // Step 3: LoadK R(2), K(2) - load step value (1)
        vm.step().unwrap();
        assert_eq!(get_pc(&mut vm), 3);
        assert_eq!(read_register(&mut vm, 2), Value::Number(1.0));
        
        // Step 4: ForPrep R(0), +2 - initialize loop (index = 0) and jump to 5
        println!("Executing ForPrep at PC {}", get_pc(&mut vm));
        vm.step().unwrap();
        assert_eq!(get_pc(&mut vm), 5, "ForPrep should jump to in instruction 5");
        assert_eq!(read_register(&mut vm, 0), Value::Number(0.0), "Index after ForPrep should be 0.0");
        
        // FIRST ITERATION
        
        // Step 5: ForLoop R(0), -1 - increment index and loop back if not done
        println!("Executing ForLoop at PC {}", get_pc(&mut vm));
        vm.step().unwrap();
        assert_eq!(get_pc(&mut vm), 4, "ForLoop should jump back to loop body at PC 4");
        assert_eq!(read_register(&mut vm, 0), Value::Number(1.0), "Index after first ForLoop should be 1.0");
        assert_eq!(read_register(&mut vm, 3), Value::Number(1.0), "Loop variable should be 1.0");
        
        // Step 6: Loop body instruction
        vm.step().unwrap();
        assert_eq!(get_pc(&mut vm), 5, "PC should be at ForLoop instruction");
        
        // SECOND ITERATION
        
        // Step 7: ForLoop R(0), -1 - second iteration
        println!("Executing ForLoop at PC {}", get_pc(&mut vm));
        vm.step().unwrap();
        assert_eq!(get_pc(&mut vm), 4, "ForLoop should jump back to loop body at PC 4");
        assert_eq!(read_register(&mut vm, 0), Value::Number(2.0), "Index after second ForLoop should be 2.0");
        assert_eq!(read_register(&mut vm, 3), Value::Number(2.0), "Loop variable should be 2.0");
        
        // Step 8: Loop body instruction
        vm.step().unwrap();
        assert_eq!(get_pc(&mut vm), 5, "PC should be at ForLoop instruction");
        
        // THIRD ITERATION
        
        // Step 9: ForLoop R(0), -1 - third iteration
        println!("Executing ForLoop at PC {}", get_pc(&mut vm));
        vm.step().unwrap();
        assert_eq!(get_pc(&mut vm), 4, "ForLoop should jump back to loop body at PC 4");
        assert_eq!(read_register(&mut vm, 0), Value::Number(3.0), "Index after third ForLoop should be 3.0");
        assert_eq!(read_register(&mut vm, 3), Value::Number(3.0), "Loop variable should be 3.0");
        
        // Step 10: Loop body instruction
        vm.step().unwrap();
        assert_eq!(get_pc(&mut vm), 5, "PC should be at ForLoop instruction");
        
        // FOURTH ITERATION (LOOP EXIT)
        
        // Step 11: ForLoop R(0), -1 - fourth iteration (should exit)
        println!("Executing ForLoop at PC {} (should exit loop)", get_pc(&mut vm));
        vm.step().unwrap();
        assert_eq!(get_pc(&mut vm), 6, "ForLoop should exit to instruction 6");
        assert_eq!(read_register(&mut vm, 0), Value::Number(4.0), "Index after fourth ForLoop should be 4.0");
        
        println!("Loop test completed successfully");
    }

    #[test]
    fn test_tforloop_operations() {
        // Create a VM instance
        let mut vm = LuaVM::new().unwrap();
        
        // This test creates a simple C-function iterator that:
        // 1. Takes a control value (initially nil)
        // 2. Returns a sequence of numbers 1, 2, 3 on subsequent calls
        // 3. Returns nil when the sequence is finished
        
        // First, create the C function iterator
        let iterator: CFunction = |ctx| {
            // Get the control variable (second argument)
            // The arguments in our iterator are state (0) and control (1)
            let control = ctx.get_arg(1).unwrap_or(Value::Nil);
            println!("C function got control value: {:?}", control);
            
            // Determine the next value based on control variable
            let next_value = match control {
                Value::Nil => 1.0, // First iteration returns 1
                Value::Number(n) => {
                    if n < 3.0 {
                        n + 1.0 // Next iteration
                    } else {
                        // End of sequence, return nil
                        ctx.push_result(Value::Nil).unwrap();
                        return Ok(1);
                    }
                }
                _ => {
                    // End iteration for any other control value
                    ctx.push_result(Value::Nil).unwrap();
                    return Ok(1);
                }
            };
            
            // Push the next value as the first result
            // In TForLoop, this becomes both the control variable and first loop variable
            println!("C function returning: {:?}", Value::Number(next_value));
            ctx.push_result(Value::Number(next_value)).unwrap();
            
            // Return 1 value
            Ok(1)
        };
        
        // Register layout:
        // R(0) = iterator function
        // R(1) = state (nil in this case, could be a table)
        // R(2) = control variable (updated by the iterator)
        // R(3), R(4), ... = loop variables
        
        // TForLoop encoding:
        // Bits 0-5: opcode (33 = 0x21)
        // Bits 6-13: A (0) - The base register for the iterator
        // Bits 14-22: C (1) - Number of variables to return
        
        let tforloop_instr = 33 | (0 << 6) | (1 << 14);  // TForLoop R(0), 1
        
        // Create a JMP instruction to follow TForLoop for the loop back
        // This is the standard pattern in Lua 5.1 bytecode
        let jmp_back_instr = 22 | (0 << 6) | ((131071-2) << 14); // JMP -2
        
        let bytecode = vec![
            0,                  // 0: NOP (placeholder)
            tforloop_instr,     // 1: TForLoop R(0), C=1 (number of values)
            jmp_back_instr,     // 2: JMP -2 (back to TForLoop)
            0,                  // 3: NOP (end of loop)
        ];
        
        // Create a function prototype with the test bytecode
        let proto = FunctionProto {
            bytecode,
            constants: vec![],
            num_params: 0,
            is_vararg: false,
            max_stack_size: 10,
            upvalues: vec![],
        };
        
        // Create a closure
        let closure = Closure {
            proto,
            upvalues: vec![],
        };
        
        // Setup the VM
        let thread_handle = vm.current_thread;
        let mut tx = HeapTransaction::new(&mut vm.heap);
        
        // Create closure handle
        let closure_handle = tx.create_closure(closure).unwrap();
        
        // Create call frame
        let frame = CallFrame {
            closure: closure_handle,
            pc: 0,
            base_register: 0,
            expected_results: None,
            varargs: None,
        };
        
        // Push call frame
        tx.push_call_frame(thread_handle, frame).unwrap();
        
        // Initialize registers
        for _i in 0..10 {
            tx.push_stack(thread_handle, Value::Nil).unwrap();
        }
        
        // Set up the iterator, state, and control variables
        tx.set_register(thread_handle, 0, Value::CFunction(iterator)).unwrap();
        tx.set_register(thread_handle, 1, Value::Nil).unwrap();  // State (empty)
        tx.set_register(thread_handle, 2, Value::Nil).unwrap();  // Control (will be updated)
        
        // Commit setup
        tx.commit().unwrap();
        
        // Helper functions
        let get_pc = |vm: &mut LuaVM| -> usize {
            let mut tx = HeapTransaction::new(&mut vm.heap);
            let frame = tx.get_current_frame(vm.current_thread).unwrap();
            let pc = frame.pc;
            tx.commit().unwrap();
            pc
        };
        
        let get_register = |vm: &mut LuaVM, reg: usize| -> Value {
            let mut tx = HeapTransaction::new(&mut vm.heap);
            let value = tx.read_register(vm.current_thread, reg).unwrap();
            tx.commit().unwrap();
            value
        };
        
        println!("Starting TForLoop test");
        
        // Execute the first NOP instruction
        assert_eq!(get_pc(&mut vm), 0, "Initial PC should be 0");
        vm.step().unwrap();
        assert_eq!(get_pc(&mut vm), 1, "PC should be 1 after NOP");
        
        // Check initial register values
        println!("Initial register values: R(0)={:?}, R(1)={:?}, R(2)={:?}", 
            get_register(&mut vm, 0),
            get_register(&mut vm, 1),
            get_register(&mut vm, 2));
        
        // First iteration: Execute TForLoop - should call our iterator function
        println!("Executing TForLoop for first iteration");
        vm.step().unwrap();
        
        // Check control variable and loop variables are set correctly
        println!("After first iteration: R(2)={:?}, R(3)={:?}, R(4)={:?}",
            get_register(&mut vm, 2),
            get_register(&mut vm, 3),
            get_register(&mut vm, 4));
        
        // In TForLoop, R(A+2) and R(A+3) should both be set to the first return value
        assert_eq!(get_register(&mut vm, 2), Value::Number(1.0), "Control variable should be 1.0");
        assert_eq!(get_register(&mut vm, 3), Value::Number(1.0), "First loop variable should be 1.0");
        
        // PC should now be at the JMP instruction
        let current_pc = get_pc(&mut vm);
        println!("Current PC after first iteration: {}", current_pc);
        assert_eq!(current_pc, 2, "PC should be at JMP instruction after TForLoop");
        
        // Execute the JMP instruction to go back to TForLoop
        vm.step().unwrap();
        let pc_after_jmp = get_pc(&mut vm);
        assert_eq!(pc_after_jmp, 0, "PC should be 0 after JMP -2");
    }

    #[test]
    fn test_upvalue_operations() {
        let mut vm = LuaVM::new().unwrap();
        
        // Create a transaction to set up the test
        let (_closure_handle, _upvalue_handle) = {
            let mut tx = HeapTransaction::new(&mut vm.heap);
            
            // Create an upvalue
            let upvalue = value::Upvalue {
                stack_index: None,
                value: Some(Value::Number(42.0)),
            };
            let upvalue_handle = tx.create_upvalue(upvalue).unwrap();
            
            // Create a function proto with GetUpval and SetUpval instructions
            
            // Encoding for GetUpval:
            // opcode = 4 (GetUpval)
            // A = 0 (store in R(0))
            // B = 0 (first upvalue)
            // Format: opcode | A << 6 | B << 23
            let get_upval_instr = 4 | (0 << 6) | (0 << 23);
            
            // Encoding for SetUpval:
            // opcode = 7 (SetUpval)
            // A = 1 (load from R(1))
            // B = 0 (first upvalue)
            // Format: opcode | A << 6 | B << 23
            let set_upval_instr = 7 | (1 << 6) | (0 << 23);
            
            // Create a return instruction
            // opcode = 30 (Return)
            // A = 0 (return from R(0))
            // B = 2 (return 1 value)
            let return_instr = 30 | (0 << 6) | (2 << 23);
            
            // Create function proto
            let proto = value::FunctionProto {
                bytecode: vec![get_upval_instr, set_upval_instr, return_instr],
                constants: vec![],
                num_params: 0,
                is_vararg: false,
                max_stack_size: 10,
                upvalues: vec![value::UpvalueInfo {
                    in_stack: false,
                    index: 0,
                }],
            };
            
            // Create closure with the upvalue
            let closure = value::Closure {
                proto,
                upvalues: vec![upvalue_handle],
            };
            
            // Create closure handle
            let closure_handle = tx.create_closure(closure).unwrap();
            
            tx.commit().unwrap();
            
            (closure_handle, upvalue_handle)
        };
        
        // This test will verify our upvalue implementation
        // but we won't actually execute it for now, as we need to complete
        // the VM implementation for function execution
        
        println!("Created closure with upvalue for verification");
    }
}