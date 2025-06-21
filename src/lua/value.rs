//! Lua value representation
//! 
//! This module defines how Lua values are represented in memory,
//! optimized for Redis use cases.

use std::sync::Arc;
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::cmp::Ordering;

/// Core Lua value type
#[derive(Debug, Clone)]
pub enum LuaValue {
    /// Nil value
    Nil,
    
    /// Boolean value
    Boolean(bool),
    
    /// Number (Lua uses doubles for all numbers)
    Number(f64),
    
    /// String (interned for efficiency)
    String(LuaString),
    
    /// Table (hash table with array optimization)
    Table(Rc<RefCell<LuaTable>>),
    
    /// Function (Lua or Rust)
    Function(LuaFunction),
    
    /// Thread (coroutine) - not used in Redis
    Thread(LuaThread),
    
    /// Light userdata (pointer) - not used in Redis
    LightUserData(usize),
}

/// Interned string representation
#[derive(Clone)]
pub struct LuaString {
    /// Shared byte data
    bytes: Arc<Vec<u8>>,
    
    /// Pre-computed hash for fast comparison
    hash: u64,
}

impl LuaString {
    /// Create a new Lua string from bytes
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        let hash = {
            use std::collections::hash_map::DefaultHasher;
            let mut hasher = DefaultHasher::new();
            bytes.hash(&mut hasher);
            hasher.finish()
        };
        
        LuaString {
            bytes: Arc::new(bytes),
            hash,
        }
    }
    
    /// Create from a Rust string
    pub fn from_string(s: String) -> Self {
        Self::from_bytes(s.into_bytes())
    }
    
    /// Create from a string slice
    pub fn from_str(s: &str) -> Self {
        Self::from_bytes(s.as_bytes().to_vec())
    }
    
    /// Get the bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
    
    /// Try to get as UTF-8 string
    pub fn to_str(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.bytes)
    }
}

impl PartialEq for LuaString {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.bytes == other.bytes
    }
}

impl Eq for LuaString {}

impl Hash for LuaString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
    }
}

impl fmt::Debug for LuaString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_str() {
            Ok(s) => write!(f, "LuaString({:?})", s),
            Err(_) => write!(f, "LuaString({:?})", self.bytes),
        }
    }
}

/// Lua table (the only complex data structure in Lua)
#[derive(Debug)]
pub struct LuaTable {
    /// Array part (1-indexed as per Lua convention)
    array: Vec<LuaValue>,
    
    /// Hash part for non-integer or sparse keys
    hash: HashMap<LuaValue, LuaValue>,
    
    /// Metatable (not used in Redis Lua)
    metatable: Option<Rc<RefCell<LuaTable>>>,
}

impl LuaTable {
    /// Create a new empty table
    pub fn new() -> Self {
        LuaTable {
            array: Vec::new(),
            hash: HashMap::new(),
            metatable: None,
        }
    }
    
    /// Get a value by key
    pub fn get(&self, key: &LuaValue) -> Option<&LuaValue> {
        match key {
            LuaValue::Number(n) if n.fract() == 0.0 && *n >= 1.0 => {
                let index = (*n as usize) - 1;
                if index < self.array.len() {
                    Some(&self.array[index])
                } else {
                    self.hash.get(key)
                }
            }
            _ => self.hash.get(key),
        }
    }
    
    /// Set a value by key
    pub fn set(&mut self, key: LuaValue, value: LuaValue) {
        match &key {
            LuaValue::Number(n) if n.fract() == 0.0 && *n >= 1.0 => {
                let index = (*n as usize) - 1;
                
                // Extend array if setting consecutive indices
                if index == self.array.len() {
                    self.array.push(value);
                } else if index < self.array.len() {
                    self.array[index] = value;
                } else {
                    // Sparse array, use hash part
                    self.hash.insert(key, value);
                }
            }
            _ => {
                self.hash.insert(key, value);
            }
        }
    }
    
    /// Get the length of the table (Lua's # operator)
    pub fn len(&self) -> usize {
        self.array.len()
    }
    
    /// Check if table is array-like (consecutive integer keys starting from 1)
    pub fn is_array(&self) -> bool {
        self.hash.is_empty() || 
        self.hash.keys().all(|k| {
            matches!(k, LuaValue::Number(n) if n.fract() == 0.0 && *n > 0.0)
        })
    }
}

impl Default for LuaTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Lua function type
#[derive(Debug, Clone)]
pub enum LuaFunction {
    /// Lua function (closure with bytecode)
    Lua(Rc<LuaClosure>),
    
    /// Rust function callable from Lua
    Rust(LuaRustFunction),
}

/// Lua closure (function with captured variables)
#[derive(Debug)]
pub struct LuaClosure {
    /// Function prototype with bytecode
    pub proto: Rc<FunctionProto>,
    
    /// Captured upvalues
    pub upvalues: Vec<UpvalueRef>,
}

/// Function prototype (shared between closures)
#[derive(Debug, Clone)]
pub struct FunctionProto {
    /// Compiled bytecode
    pub code: Vec<Instruction>,
    
    /// Constant values used by this function
    pub constants: Vec<LuaValue>,
    
    /// Number of parameters
    pub num_params: u8,
    
    /// Is variadic (...)
    pub is_vararg: bool,
    
    /// Maximum stack size needed
    pub max_stack_size: u8,
}

/// Bytecode instruction (32-bit format compatible with Lua 5.1)
#[derive(Debug, Copy, Clone)]
pub struct Instruction(pub u32);

/// Upvalue reference (for closures)
#[derive(Debug, Clone)]
pub struct UpvalueRef {
    // Simplified for initial implementation
    pub index: usize,
}

/// Rust function callable from Lua
pub type LuaRustFunction = fn(vm: &mut super::vm::LuaVm, args: &[LuaValue]) -> Result<LuaValue, super::error::LuaError>;

/// Lua thread (coroutine) - minimal support for Redis
#[derive(Debug, Clone)]
pub struct LuaThread {
    // Placeholder for coroutine state
}

/// Implement equality for LuaValue (needed for table keys)
impl PartialEq for LuaValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (LuaValue::Nil, LuaValue::Nil) => true,
            (LuaValue::Boolean(a), LuaValue::Boolean(b)) => a == b,
            (LuaValue::Number(a), LuaValue::Number(b)) => {
                // Handle NaN specially as per Lua semantics
                if a.is_nan() && b.is_nan() {
                    false  // NaN != NaN in Lua
                } else {
                    a == b
                }
            }
            (LuaValue::String(a), LuaValue::String(b)) => a == b,
            _ => false,  // Different types or reference types
        }
    }
}

impl Eq for LuaValue {}

/// Implement hashing for LuaValue (needed for table keys)
impl Hash for LuaValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            LuaValue::Nil => 0u8.hash(state),
            LuaValue::Boolean(b) => {
                1u8.hash(state);
                b.hash(state);
            }
            LuaValue::Number(n) => {
                2u8.hash(state);
                // Hash the bit pattern for consistency
                n.to_bits().hash(state);
            }
            LuaValue::String(s) => {
                3u8.hash(state);
                s.hash(state);
            }
            LuaValue::Table(t) => {
                // Tables hash by identity (pointer address)
                4u8.hash(state);
                let ptr = Rc::as_ptr(t) as usize;
                ptr.hash(state);
            }
            LuaValue::Function(_) => {
                // Functions hash by identity
                5u8.hash(state);
                let ptr = self as *const _ as usize;
                ptr.hash(state);
            }
            LuaValue::Thread(_) => {
                // Threads hash by identity
                6u8.hash(state);
                let ptr = self as *const _ as usize;
                ptr.hash(state);
            }
            LuaValue::LightUserData(ptr) => {
                7u8.hash(state);
                ptr.hash(state);
            }
        }
    }
}

/// Type checking helpers
impl LuaValue {
    pub fn type_name(&self) -> &'static str {
        match self {
            LuaValue::Nil => "nil",
            LuaValue::Boolean(_) => "boolean",
            LuaValue::Number(_) => "number",
            LuaValue::String(_) => "string",
            LuaValue::Table(_) => "table",
            LuaValue::Function(_) => "function",
            LuaValue::Thread(_) => "thread",
            LuaValue::LightUserData(_) => "userdata",
        }
    }
    
    pub fn is_nil(&self) -> bool {
        matches!(self, LuaValue::Nil)
    }
    
    pub fn is_number(&self) -> bool {
        matches!(self, LuaValue::Number(_))
    }
    
    pub fn is_string(&self) -> bool {
        matches!(self, LuaValue::String(_))
    }
    
    pub fn is_table(&self) -> bool {
        matches!(self, LuaValue::Table(_))
    }
    
    pub fn is_function(&self) -> bool {
        matches!(self, LuaValue::Function(_))
    }
    
    /// Convert to boolean (Lua's truthiness rules)
    pub fn to_bool(&self) -> bool {
        !matches!(self, LuaValue::Nil | LuaValue::Boolean(false))
    }
}

use std::fmt;