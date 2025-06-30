//! Lua Value Types and Object Definitions
//! 
//! This module defines the core value types and objects used in the Lua VM.
//! All heap-allocated objects are referenced through typed handles for safety.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::fmt;

use super::arena::{Handle, TypedHandle};
use super::error::{LuaError, Result};

// Type aliases for handles
pub type StringHandle = TypedHandle<LuaString>;
pub type TableHandle = TypedHandle<Table>;
pub type ClosureHandle = TypedHandle<Closure>;
pub type ThreadHandle = TypedHandle<Thread>;
pub type UpvalueHandle = TypedHandle<Upvalue>;
pub type UserDataHandle = TypedHandle<UserData>;

// Re-export handle type for convenience
pub use super::arena::Handle as HandleType;

/// A Lua value
#[derive(Clone, Debug)]
pub enum Value {
    /// nil
    Nil,
    /// boolean
    Boolean(bool),
    /// number (Lua uses doubles for all numbers)
    Number(f64),
    /// string (interned)
    String(StringHandle),
    /// table
    Table(TableHandle),
    /// closure (Lua function)
    Closure(ClosureHandle),
    /// thread (coroutine)
    Thread(ThreadHandle),
    /// C function
    CFunction(CFunction),
    /// userdata
    UserData(UserDataHandle),
}

/// A C function callable from Lua
pub type CFunction = fn(&mut super::vm::ExecutionContext) -> Result<i32>;

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
            Value::UserData(_) => "userdata",
        }
    }
    
    /// Check if value is truthy (not nil or false)
    pub fn is_truthy(&self) -> bool {
        !matches!(self, Value::Nil | Value::Boolean(false))
    }
    
    /// Check if the value is nil
    pub fn is_nil(&self) -> bool {
        matches!(self, Value::Nil)
    }
    
    /// Check if the value is a boolean
    pub fn is_boolean(&self) -> bool {
        matches!(self, Value::Boolean(_))
    }
    
    /// Check if the value is a number
    pub fn is_number(&self) -> bool {
        matches!(self, Value::Number(_))
    }
    
    /// Check if the value is a string
    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }
    
    /// Check if the value is a table
    pub fn is_table(&self) -> bool {
        matches!(self, Value::Table(_))
    }
    
    /// Check if the value is a function
    pub fn is_function(&self) -> bool {
        matches!(self, Value::Closure(_) | Value::CFunction(_))
    }
    
    /// Check if the value is a thread
    pub fn is_thread(&self) -> bool {
        matches!(self, Value::Thread(_))
    }
    
    /// Check if the value is userdata
    pub fn is_userdata(&self) -> bool {
        matches!(self, Value::UserData(_))
    }
    
    /// Get the boolean value
    pub fn as_boolean(&self) -> Result<bool> {
        match self {
            Value::Boolean(b) => Ok(*b),
            _ => Err(LuaError::TypeError(
                format!("expected boolean, got {}", self.type_name())
            )),
        }
    }
    
    /// Get the number value
    pub fn as_number(&self) -> Result<f64> {
        match self {
            Value::Number(n) => Ok(*n),
            _ => Err(LuaError::TypeError(
                format!("expected number, got {}", self.type_name())
            )),
        }
    }
    
    /// Get the string handle
    pub fn as_string(&self) -> Result<StringHandle> {
        match self {
            Value::String(h) => Ok(h.clone()),
            _ => Err(LuaError::TypeError(format!("expected string, got {}", self.type_name()))),
        }
    }
    
    /// Get the table handle
    pub fn as_table(&self) -> Result<TableHandle> {
        match self {
            Value::Table(h) => Ok(h.clone()),
            _ => Err(LuaError::TypeError(format!("expected table, got {}", self.type_name()))),
        }
    }
    
    /// Get the closure handle
    pub fn as_closure(&self) -> Result<ClosureHandle> {
        match self {
            Value::Closure(h) => Ok(h.clone()),
            _ => Err(LuaError::TypeError(format!("expected Lua function, got {}", self.type_name()))),
        }
    }
    
    /// Get the C function
    pub fn as_cfunction(&self) -> Result<CFunction> {
        match self {
            Value::CFunction(f) => Ok(*f),
            _ => Err(LuaError::TypeError(
                format!("expected C function, got {}", self.type_name())
            )),
        }
    }
    
    /// Get the thread handle
    pub fn as_thread(&self) -> Result<ThreadHandle> {
        match self {
            Value::Thread(h) => Ok(h.clone()),
            _ => Err(LuaError::TypeError(format!("expected thread, got {}", self.type_name()))),
        }
    }
    
    /// Get the userdata handle
    pub fn as_userdata(&self) -> Result<UserDataHandle> {
        match self {
            Value::UserData(h) => Ok(h.clone()),
            _ => Err(LuaError::TypeError(format!("expected userdata, got {}", self.type_name()))),
        }
    }
    
    /// Try to convert to a number
    pub fn to_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            Value::String(handle) => {
                // Would need heap access to convert string to number
                // This is handled at a higher level
                None
            }
            _ => None,
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Nil, Value::Nil) => true,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::String(a), Value::String(b)) => a.0 == b.0,
            (Value::Table(a), Value::Table(b)) => a.0 == b.0,
            (Value::Closure(a), Value::Closure(b)) => a.0 == b.0,
            (Value::Thread(a), Value::Thread(b)) => a.0 == b.0,
            (Value::UserData(a), Value::UserData(b)) => a.0 == b.0,
            // C functions can't be compared
            _ => false,
        }
    }
}

impl Eq for Value {}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Value::Nil => {}
            Value::Boolean(b) => b.hash(state),
            Value::Number(n) => n.to_bits().hash(state),
            Value::String(h) => h.0.hash(state),
            Value::Table(h) => h.0.hash(state),
            Value::Closure(h) => h.0.hash(state),
            Value::Thread(h) => h.0.hash(state),
            Value::CFunction(f) => (*f as usize).hash(state),
            Value::UserData(h) => h.0.hash(state),
        }
    }
}

/// A Lua string (immutable byte array)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LuaString {
    pub bytes: Vec<u8>,
}

impl LuaString {
    /// Create a new Lua string  
    pub fn new(s: &str) -> Self {
        LuaString {
            bytes: s.as_bytes().to_vec(),
        }
    }
    
    /// Get as UTF-8 string if valid
    pub fn to_str(&self) -> Result<&str> {
        std::str::from_utf8(&self.bytes)
            .map_err(|_| LuaError::InvalidEncoding)
    }
}

/// A Lua table
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    /// Array part (1-indexed in Lua)
    pub array: Vec<Value>,
    /// Hash part
    pub hash_map: HashMap<Value, Value>,
    /// Metatable if any
    pub metatable: Option<TableHandle>,
}

impl Table {
    /// Create a new empty table
    pub fn new() -> Self {
        Table {
            array: Vec::new(),
            hash_map: HashMap::new(),
            metatable: None,
        }
    }
    
    /// Get a value by key
    pub fn get(&self, key: &Value) -> Option<&Value> {
        // Check array part for positive integer keys
        if let Value::Number(n) = key {
            if n.fract() == 0.0 && *n > 0.0 && *n <= self.array.len() as f64 {
                let idx = *n as usize - 1;
                return Some(&self.array[idx]);
            }
        }
        
        // Check hash part
        self.hash_map.get(key)
    }
    
    /// Set a value by key
    pub fn set(&mut self, key: Value, value: Value) {
        // Handle array part for positive integer keys
        if let Value::Number(n) = key {
            if n.fract() == 0.0 && n > 0.0 {
                let idx = n as usize - 1;
                
                // Extend array if needed
                if idx < self.array.len() {
                    self.array[idx] = value;
                    return;
                } else if idx == self.array.len() {
                    self.array.push(value);
                    return;
                }
            }
        }
        
        // Use hash part
        if matches!(value, Value::Nil) {
            // Remove nil values
            self.hash_map.remove(&key);
        } else {
            self.hash_map.insert(key, value);
        }
    }
    
    /// Get the length of the array part
    pub fn len(&self) -> usize {
        self.array.len()
    }
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

impl std::hash::Hash for Table {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // We'll hash based on object identity rather than contents
        // This is acceptable for Lua metatables which are identity-based
        std::ptr::addr_of!(*self).hash(state);
    }
}

impl Eq for Table {}

/// A Lua closure
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Closure {
    /// Function prototype
    pub proto: FunctionProto,
    /// Captured upvalues
    pub upvalues: Vec<UpvalueHandle>,
}

/// Function prototype (bytecode and metadata)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FunctionProto {
    /// Bytecode instructions
    pub bytecode: Vec<u32>,
    /// Constants used by this function
    pub constants: Vec<Value>,
    /// Upvalue descriptors
    pub upvalues: Vec<UpvalueDesc>,
    /// Number of parameters
    pub param_count: u8,
    /// Is variadic
    pub is_vararg: bool,
    /// Source file name
    pub source: Option<String>,
    /// Line where function starts
    pub line_defined: u32,
    /// Line where function ends
    pub last_line_defined: u32,
    /// Debug line info
    pub line_info: Vec<u32>,
    /// Local variable info for debugging
    pub locals: Vec<LocalVar>,
}

/// Upvalue descriptor
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UpvalueDesc {
    /// Name for debugging
    pub name: Option<String>,
    /// Is it in the stack of the enclosing function?
    pub in_stack: bool,
    /// Index in stack or upvalue list
    pub index: u8,
}

/// Local variable info
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LocalVar {
    /// Variable name
    pub name: String,
    /// First instruction where variable is active
    pub start_pc: u32,
    /// First instruction where variable is dead
    pub end_pc: u32,
}

/// An upvalue (captured variable)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Upvalue {
    /// Open upvalue (still on stack)
    Open {
        thread: ThreadHandle,
        stack_index: usize,
    },
    /// Closed upvalue (value captured)
    Closed(Value),
}

/// A Lua thread (coroutine)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Thread {
    /// Call stack
    pub call_frames: Vec<CallFrame>,
    /// Value stack
    pub stack: Vec<Value>,
    /// Thread status
    pub status: ThreadStatus,
}

impl Thread {
    /// Create a new thread
    pub fn new() -> Self {
        Thread {
            call_frames: Vec::new(),
            stack: Vec::new(),
            status: ThreadStatus::Ready,
        }
    }
}

impl Default for Thread {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread status
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThreadStatus {
    /// Ready to run
    Ready,
    /// Currently running
    Running,
    /// Suspended (yielded)
    Suspended,
    /// Finished normally
    Dead,
    /// Errored
    Error,
}

/// A call frame
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CallFrame {
    /// Closure being executed
    pub closure: ClosureHandle,
    /// Program counter
    pub pc: usize,
    /// Base register for this frame
    pub base_register: u16,
    /// Expected number of return values
    pub return_count: u8,
    /// Frame type
    pub frame_type: CallFrameType,
}

/// Type of call frame
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CallFrameType {
    /// Normal function call
    Normal,
    /// Tail call
    TailCall,
    /// Protected call (pcall)
    Protected,
}

/// User data
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UserData {
    /// Type name for debugging
    pub data_type: String,
    /// Metatable if any
    pub metatable: Option<TableHandle>,
}

/// Metamethod names
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MetamethodType {
    /// __index
    Index,
    /// __newindex
    NewIndex,
    /// __gc
    Gc,
    /// __mode
    Mode,
    /// __len
    Len,
    /// __eq
    Eq,
    /// __lt
    Lt,
    /// __le
    Le,
    /// __add
    Add,
    /// __sub
    Sub,
    /// __mul
    Mul,
    /// __div
    Div,
    /// __mod
    Mod,
    /// __pow
    Pow,
    /// __unm
    Unm,
    /// __concat
    Concat,
    /// __call
    Call,
    /// __tostring
    ToString,
}

impl MetamethodType {
    /// Get the metamethod name as a string
    pub fn name(&self) -> &'static str {
        match self {
            MetamethodType::Index => "__index",
            MetamethodType::NewIndex => "__newindex",
            MetamethodType::Gc => "__gc",
            MetamethodType::Mode => "__mode",
            MetamethodType::Len => "__len",
            MetamethodType::Eq => "__eq",
            MetamethodType::Lt => "__lt",
            MetamethodType::Le => "__le",
            MetamethodType::Add => "__add",
            MetamethodType::Sub => "__sub",
            MetamethodType::Mul => "__mul",
            MetamethodType::Div => "__div",
            MetamethodType::Mod => "__mod",
            MetamethodType::Pow => "__pow",
            MetamethodType::Unm => "__unm",
            MetamethodType::Concat => "__concat",
            MetamethodType::Call => "__call",
            MetamethodType::ToString => "__tostring",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_type_name() {
        assert_eq!(Value::Nil.type_name(), "nil");
        assert_eq!(Value::Boolean(true).type_name(), "boolean");
        assert_eq!(Value::Number(42.0).type_name(), "number");
    }
    
    #[test]
    fn test_value_truthy() {
        assert!(!Value::Nil.is_truthy());
        assert!(!Value::Boolean(false).is_truthy());
        assert!(Value::Boolean(true).is_truthy());
        assert!(Value::Number(0.0).is_truthy());
    }
    
    #[test]
    fn test_value_conversions() {
        assert_eq!(Value::Boolean(true).as_boolean().unwrap(), true);
        assert_eq!(Value::Number(42.0).as_number().unwrap(), 42.0);
        assert!(Value::Nil.as_boolean().is_err());
        assert!(Value::Nil.as_number().is_err());
    }
}