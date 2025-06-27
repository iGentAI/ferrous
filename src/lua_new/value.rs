//! Lua value representation using generational arena architecture

use crate::lua_new::arena::{Handle, TypedHandle};
use std::hash::{Hash, Hasher};
use std::fmt;

/// Type-safe handle for strings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StringHandle(pub Handle);

/// Type-safe handle for tables
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TableHandle(pub Handle);

/// Type-safe handle for closures
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClosureHandle(pub Handle);

/// Type-safe handle for threads
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThreadHandle(pub Handle);

/// Core Lua value type - 16 bytes total
#[derive(Debug, Clone, Copy)]
pub enum Value {
    /// Nil value
    Nil,
    
    /// Boolean value
    Boolean(bool),
    
    /// Number value (Lua uses f64 for all numbers)
    Number(f64),
    
    /// String handle (points to interned string in heap)
    String(StringHandle),
    
    /// Table handle (points to table in heap)
    Table(TableHandle),
    
    /// Closure handle (points to closure in heap)
    Closure(ClosureHandle),
    
    /// Thread handle (points to thread in heap)
    Thread(ThreadHandle),
    
    /// Built-in function pointer
    CFunction(CFunction),
}

/// Built-in function type
pub type CFunction = fn(&mut crate::lua_new::vm::ExecutionContext) -> crate::lua_new::error::Result<i32>;

impl Value {
    /// Get the type name of this value
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Boolean(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Table(_) => "table",
            Value::Closure(_) => "function",
            Value::Thread(_) => "thread",
            Value::CFunction(_) => "function",
        }
    }
    
    /// Convert to boolean (Lua's truthiness rules)
    pub fn to_bool(&self) -> bool {
        !matches!(self, Value::Nil | Value::Boolean(false))
    }
    
    /// Check if nil
    pub fn is_nil(&self) -> bool {
        matches!(self, Value::Nil)
    }
    
    /// Check if number
    pub fn is_number(&self) -> bool {
        matches!(self, Value::Number(_))
    }
    
    /// Check if string
    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }
    
    /// Check if table
    pub fn is_table(&self) -> bool {
        matches!(self, Value::Table(_))
    }
    
    /// Check if function (closure or C function)
    pub fn is_function(&self) -> bool {
        matches!(self, Value::Closure(_) | Value::CFunction(_))
    }
    
    /// Check if same type
    pub fn same_type(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Nil, Value::Nil) => true,
            (Value::Boolean(_), Value::Boolean(_)) => true,
            (Value::Number(_), Value::Number(_)) => true,
            (Value::String(_), Value::String(_)) => true,
            (Value::Table(_), Value::Table(_)) => true,
            (Value::Closure(_), Value::Closure(_)) => true,
            (Value::Thread(_), Value::Thread(_)) => true,
            (Value::CFunction(_), Value::CFunction(_)) => true,
            (Value::Closure(_), Value::CFunction(_)) => true,
            (Value::CFunction(_), Value::Closure(_)) => true,
            _ => false,
        }
    }
    
    /// Try to convert to a number
    pub fn to_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            Value::Boolean(b) => Some(if *b { 1.0 } else { 0.0 }),
            _ => None,
        }
    }
}

/// Implement equality for Value
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Nil, Value::Nil) => true,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => {
                // Handle NaN specially as per Lua semantics
                if a.is_nan() && b.is_nan() {
                    false  // NaN != NaN in Lua
                } else {
                    a == b
                }
            }
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Table(a), Value::Table(b)) => a == b,
            (Value::Closure(a), Value::Closure(b)) => a == b,
            (Value::Thread(a), Value::Thread(b)) => a == b,
            // C functions are compared by pointer
            (Value::CFunction(a), Value::CFunction(b)) => {
                a as *const CFunction == b as *const CFunction
            }
            _ => false,
        }
    }
}

impl Eq for Value {}

/// Implement hashing for Value
impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash discriminant
        match self {
            Value::Nil => 0u8.hash(state),
            Value::Boolean(b) => {
                1u8.hash(state);
                b.hash(state);
            }
            Value::Number(n) => {
                2u8.hash(state);
                // Hash the bit pattern for consistency
                n.to_bits().hash(state);
            }
            Value::String(s) => {
                3u8.hash(state);
                s.hash(state);
            }
            Value::Table(t) => {
                4u8.hash(state);
                t.hash(state);
            }
            Value::Closure(c) => {
                5u8.hash(state);
                c.hash(state);
            }
            Value::Thread(t) => {
                6u8.hash(state);
                t.hash(state);
            }
            Value::CFunction(f) => {
                7u8.hash(state);
                // Hash function pointer
                (*f as usize).hash(state);
            }
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Nil => write!(f, "nil"),
            Value::Boolean(b) => write!(f, "{}", b),
            Value::Number(n) => write!(f, "{}", n),
            Value::String(_) => write!(f, "<string>"),
            Value::Table(_) => write!(f, "<table>"),
            Value::Closure(_) => write!(f, "<function>"),
            Value::Thread(_) => write!(f, "<thread>"), 
            Value::CFunction(_) => write!(f, "<cfunction>"),
        }
    }
}

/// Upvalue reference for closures
#[derive(Debug, Clone)]
pub enum UpvalueRef {
    /// Reference to a register in a parent stack frame
    Open {
        /// Index in the thread's value stack
        register_idx: u16,
    },
    
    /// Closed upvalue with captured value
    Closed {
        /// The captured value
        value: Value,
    },
}

/// Bytecode instruction (32-bit format compatible with Lua 5.1)
#[derive(Debug, Copy, Clone)]
pub struct Instruction(pub u32);

impl Instruction {
    /// Create a new instruction
    pub fn new(value: u32) -> Self {
        Instruction(value)
    }
    
    /// Get the raw value
    pub fn raw(&self) -> u32 {
        self.0
    }
    
    /// Extract opcode (6 bits)
    pub fn opcode(&self) -> u8 {
        (self.0 & 0x3F) as u8
    }
    
    /// Extract A field (8 bits)
    pub fn a(&self) -> u8 {
        ((self.0 >> 6) & 0xFF) as u8
    }
    
    /// Extract B field (9 bits)
    pub fn b(&self) -> u16 {
        ((self.0 >> 14) & 0x1FF) as u16
    }
    
    /// Extract C field (9 bits)
    pub fn c(&self) -> u16 {
        ((self.0 >> 23) & 0x1FF) as u16
    }
    
    /// Extract Bx field (18 bits unsigned)
    pub fn bx(&self) -> u32 {
        (self.0 >> 14) & 0x3FFFF
    }
    
    /// Extract sBx field (18 bits signed)
    pub fn sbx(&self) -> i32 {
        (self.bx() as i32) - 131071
    }
}

/// Function prototype (compiled bytecode)
#[derive(Debug, Clone)]
pub struct FunctionProto {
    /// Compiled bytecode
    pub code: Vec<Instruction>,
    
    /// Constant values used by this function
    pub constants: Vec<Value>,
    
    /// Number of parameters
    pub param_count: u8,
    
    /// Is variadic (...)
    pub is_vararg: bool,
    
    /// Maximum stack size needed
    pub max_stack_size: u8,
    
    /// Number of upvalues
    pub upvalue_count: u8,
    
    /// Source file name (if available)
    pub source: Option<StringHandle>,
    
    /// Line number information (if available)
    pub line_info: Option<Vec<u32>>,
    
    /// Nested function prototypes
    pub nested_protos: Vec<FunctionProto>,
}

impl Default for FunctionProto {
    fn default() -> Self {
        FunctionProto {
            code: Vec::new(),
            constants: Vec::new(),
            param_count: 0,
            is_vararg: false,
            max_stack_size: 2,
            upvalue_count: 0,
            source: None,
            line_info: None,
            nested_protos: Vec::new(),
        }
    }
}

/// Lua opcodes (compatible with Lua 5.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OpCode {
    Move = 0,
    LoadK = 1,
    LoadBool = 2,
    LoadNil = 3,
    GetUpval = 4,
    GetGlobal = 5,
    GetTable = 6,
    SetGlobal = 7,
    SetUpval = 8,
    SetTable = 9,
    NewTable = 10,
    Self_ = 11,
    Add = 12,
    Sub = 13,
    Mul = 14,
    Div = 15,
    Mod = 16,
    Pow = 17,
    Unm = 18,
    Not = 19,
    Len = 20,
    Concat = 21,
    Jmp = 22,
    Eq = 23,
    Lt = 24,
    Le = 25,
    Test = 26,
    TestSet = 27,
    Call = 28,
    TailCall = 29,
    Return = 30,
    ForLoop = 31,
    ForPrep = 32,
    TForLoop = 33,
    SetList = 34,
    Close = 35,
    Closure = 36,
    Vararg = 37,
}

impl From<u8> for OpCode {
    fn from(value: u8) -> Self {
        match value {
            0 => OpCode::Move,
            1 => OpCode::LoadK,
            2 => OpCode::LoadBool,
            3 => OpCode::LoadNil,
            4 => OpCode::GetUpval,
            5 => OpCode::GetGlobal,
            6 => OpCode::GetTable,
            7 => OpCode::SetGlobal,
            8 => OpCode::SetUpval,
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
            31 => OpCode::ForLoop,
            32 => OpCode::ForPrep,
            33 => OpCode::TForLoop,
            34 => OpCode::SetList,
            35 => OpCode::Close,
            36 => OpCode::Closure,
            37 => OpCode::Vararg,
            _ => OpCode::Move, // Default fallback
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_value_size() {
        // Ensure Value enum is 16 bytes
        assert_eq!(std::mem::size_of::<Value>(), 16);
    }
    
    #[test]
    fn test_instruction_decoding() {
        // Test instruction encoding/decoding
        let instr = Instruction(0x00004001); // LOADK A=0, Bx=1
        assert_eq!(instr.opcode(), 1); // LoadK
        assert_eq!(instr.a(), 0);
        assert_eq!(instr.bx(), 1);
    }
}