//! Rc<RefCell> Based Lua Value Types
//!
//! This module defines Lua value types using Rc<RefCell> for heap-allocated objects,
//! which provides fine-grained interior mutability and proper shared mutable state semantics.

use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

use super::error::{LuaError, LuaResult};

/// Type representing a C function callable from Lua
pub type CFunction = fn(&mut dyn super::rc_vm::ExecutionContext) -> LuaResult<i32>;

/// Forward declarations
pub type StringHandle = Rc<RefCell<LuaString>>;
pub type TableHandle = Rc<RefCell<Table>>;
pub type ClosureHandle = Rc<RefCell<Closure>>;
pub type ThreadHandle = Rc<RefCell<Thread>>;
pub type UpvalueHandle = Rc<RefCell<UpvalueState>>;
pub type UserDataHandle = Rc<RefCell<UserData>>;
pub type FunctionProtoHandle = Rc<FunctionProto>;

/// Main Lua value type
#[derive(Debug, Clone)]
pub enum Value {
    /// Nil value
    Nil,
    
    /// Boolean value
    Boolean(bool),
    
    /// Number value (Lua uses doubles for all numbers)
    Number(f64),
    
    /// String value (Rc<RefCell<LuaString>>)
    String(StringHandle),
    
    /// Table value (Rc<RefCell<Table>>)
    Table(TableHandle),
    
    /// Function closure (Rc<RefCell<Closure>>)
    Closure(ClosureHandle),
    
    /// Thread/coroutine (Rc<RefCell<Thread>>)
    Thread(ThreadHandle),
    
    /// C function
    CFunction(CFunction),
    
    /// Userdata (Rc<RefCell<UserData>>)
    UserData(UserDataHandle),
    
    /// Function prototype (Rc<FunctionProto>)
    FunctionProto(FunctionProtoHandle),
    
    /// Pending metamethod call (for non-recursive metamethod resolution)
    PendingMetamethod(Box<Value>),
}

impl Value {
    /// Get the type name of this value
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Boolean(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Table(_) => "table",
            Value::Closure(_) | Value::CFunction(_) | Value::FunctionProto(_) => "function",
            Value::Thread(_) => "thread",
            Value::UserData(_) => "userdata",
            Value::PendingMetamethod(_) => "pending_metamethod",
        }
    }
    
    /// Check if this value is nil
    pub fn is_nil(&self) -> bool {
        matches!(self, Value::Nil)
    }
    
    /// Check if this value is falsey (nil or false)
    pub fn is_falsey(&self) -> bool {
        match self {
            Value::Nil => true,
            Value::Boolean(false) => true,
            _ => false,
        }
    }
    
    /// Try to convert to a number
    pub fn to_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }
    
    /// Try to convert to a boolean (Lua truthiness)
    pub fn to_boolean(&self) -> bool {
        !self.is_falsey()
    }
    
    /// Check if this value is a number
    pub fn is_number(&self) -> bool {
        matches!(self, Value::Number(_))
    }
    
    /// Check if this value is a string
    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }
    
    /// Check if this value is a table
    pub fn is_table(&self) -> bool {
        matches!(self, Value::Table(_))
    }
    
    /// Check if value is a function (closure or CFunction)
    pub fn is_function(&self) -> bool {
        matches!(self, Value::Closure(_) | Value::CFunction(_))
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Nil => write!(f, "nil"),
            Value::Boolean(b) => write!(f, "{}", b),
            Value::Number(n) => {
                // Format numbers like Lua does
                if n.fract() == 0.0 && n.abs() < 1e14 {
                    write!(f, "{:.0}", n)
                } else {
                    write!(f, "{}", n)
                }
            }
            Value::String(_) => write!(f, "<string>"),
            Value::Table(_) => write!(f, "<table>"),
            Value::Closure(_) => write!(f, "<function>"),
            Value::CFunction(_) => write!(f, "<C function>"),
            Value::Thread(_) => write!(f, "<thread>"),
            Value::UserData(_) => write!(f, "<userdata>"),
            Value::FunctionProto(_) => write!(f, "<function prototype>"),
            Value::PendingMetamethod(_) => write!(f, "<pending metamethod>"),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Nil, Value::Nil) => true,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::String(a), Value::String(b)) => {
                // Fast path: same handle
                if Rc::ptr_eq(a, b) {
                    return true;
                }
                
                // Content comparison
                let a_ref = a.borrow();
                let b_ref = b.borrow();
                a_ref.bytes == b_ref.bytes
            },
            (Value::Table(a), Value::Table(b)) => {
                // Identity comparison for tables
                Rc::ptr_eq(a, b)
            },
            (Value::Closure(a), Value::Closure(b)) => {
                // Identity comparison for closures
                Rc::ptr_eq(a, b)
            },
            (Value::Thread(a), Value::Thread(b)) => {
                // Identity comparison for threads
                Rc::ptr_eq(a, b)
            },
            (Value::CFunction(a), Value::CFunction(b)) => {
                // Function pointer comparison
                std::ptr::eq(*a as *const (), *b as *const ())
            },
            (Value::UserData(a), Value::UserData(b)) => {
                // Identity comparison for userdata
                Rc::ptr_eq(a, b)
            },
            (Value::FunctionProto(a), Value::FunctionProto(b)) => {
                // Identity comparison for function prototypes
                Rc::ptr_eq(a, b)
            },
            _ => false,
        }
    }
}

impl Eq for Value {}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Value::Nil => {},
            Value::Boolean(b) => b.hash(state),
            Value::Number(n) => OrderedFloat(*n).hash(state),
            Value::String(s) => {
                // Use the content hash for consistent behavior
                let string_ref = s.borrow();
                string_ref.content_hash.hash(state);
            },
            Value::Table(t) => {
                // Use pointer identity
                (Rc::as_ptr(t) as usize).hash(state);
            },
            Value::Closure(c) => {
                // Use pointer identity
                (Rc::as_ptr(c) as usize).hash(state);
            },
            Value::Thread(t) => {
                // Use pointer identity
                (Rc::as_ptr(t) as usize).hash(state);
            },
            Value::CFunction(c) => {
                // Use function pointer identity
                (*c as usize).hash(state);
            },
            Value::UserData(u) => {
                // Use pointer identity
                (Rc::as_ptr(u) as usize).hash(state);
            },
            Value::FunctionProto(p) => {
                // Use pointer identity
                (Rc::as_ptr(p) as usize).hash(state);
            },
            Value::PendingMetamethod(_) => {
                // Not hashable
                0.hash(state);
            }
        }
    }
}

/// Lua string representation
#[derive(Debug, Clone)]
pub struct LuaString {
    /// UTF-8 bytes of the string
    pub bytes: Vec<u8>,
    
    /// Cached hash of the content for efficient comparison
    pub content_hash: u64,
}

impl LuaString {
    /// Create a new Lua string from a Rust string slice.
    pub fn from_str(s: &str) -> LuaResult<Self> {
        let bytes = s.as_bytes().to_vec();
        
        // Calculate content hash for string interning
        let content_hash = Self::calculate_content_hash(&bytes);
        
        Ok(LuaString {
            bytes,
            content_hash,
        })
    }
    
    /// Create a new Lua string from a Rust string
    pub fn new(s: impl Into<String>) -> Self {
        let s = s.into();
        let bytes = s.into_bytes();
        let content_hash = Self::calculate_content_hash(&bytes);
        LuaString {
            bytes,
            content_hash,
        }
    }
    
    /// Create a new Lua string from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let content_hash = Self::calculate_content_hash(bytes);
        
        LuaString {
            bytes: bytes.to_vec(),
            content_hash,
        }
    }
    
    /// Calculate content hash for string interning
    pub fn calculate_content_hash(bytes: &[u8]) -> u64 {
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        hasher.finish()
    }
    
    /// Get the bytes of this string
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
    
    /// Try to convert to a UTF-8 string
    pub fn to_str(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.bytes)
    }
    
    /// Get the length in bytes
    pub fn len(&self) -> usize {
        self.bytes.len()
    }
    
    /// Check if this string is empty
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

impl PartialEq for LuaString {
    fn eq(&self, other: &Self) -> bool {
        // First check hash for quick comparison
        if self.content_hash != other.content_hash {
            return false;
        }
        
        // If hashes match, compare the actual content
        self.bytes == other.bytes
    }
}

impl Eq for LuaString {}

impl Hash for LuaString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Use the cached content hash
        self.content_hash.hash(state);
    }
}

/// Wrapper for f64 that implements Eq and Hash
#[derive(Debug, Clone, Copy)]
pub struct OrderedFloat(pub f64);

impl PartialEq for OrderedFloat {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for OrderedFloat {}

impl Hash for OrderedFloat {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

/// Hashable value for table keys
#[derive(Debug, Clone)]
pub enum HashableValue {
    /// Nil value
    Nil,
    
    /// Boolean value
    Boolean(bool),
    
    /// Number value with total ordering
    Number(OrderedFloat),
    
    /// String value with cached content hash
    String(StringHandle),
}

impl HashableValue {
    /// Create a hashable value from a Lua value
    pub fn from_value(value: &Value) -> LuaResult<Self> {
        match value {
            Value::Nil => Ok(HashableValue::Nil),
            Value::Boolean(b) => Ok(HashableValue::Boolean(*b)),
            Value::Number(n) => Ok(HashableValue::Number(OrderedFloat(*n))),
            Value::String(s) => Ok(HashableValue::String(Rc::clone(s))),
            _ => Err(LuaError::TypeError {
                expected: "nil, boolean, number, or string".to_string(),
                got: format!("{} (cannot be used as a key)", value.type_name()),
            }),
        }
    }
    
    /// Convert back to a Lua Value
    pub fn to_value(&self) -> Value {
        match self {
            HashableValue::Nil => Value::Nil,
            HashableValue::Boolean(b) => Value::Boolean(*b),
            HashableValue::Number(n) => Value::Number(n.0),
            HashableValue::String(s) => Value::String(Rc::clone(s)),
        }
    }
}

impl PartialEq for HashableValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (HashableValue::Nil, HashableValue::Nil) => true,
            (HashableValue::Boolean(a), HashableValue::Boolean(b)) => a == b,
            (HashableValue::Number(a), HashableValue::Number(b)) => a == b,
            (HashableValue::String(a), HashableValue::String(b)) => {
                // Fast path: if it's the same Rc pointer, they're equal
                if Rc::ptr_eq(a, b) {
                    return true;
                }
                
                // Otherwise, compare content
                let a_ref = a.borrow();
                let b_ref = b.borrow();
                a_ref.bytes == b_ref.bytes
            },
            _ => false,
        }
    }
}

impl Eq for HashableValue {}

impl Hash for HashableValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash the discriminant first (nil, bool, number, string)
        std::mem::discriminant(self).hash(state);
        
        match self {
            HashableValue::Nil => {},
            HashableValue::Boolean(b) => b.hash(state),
            HashableValue::Number(n) => n.hash(state),
            HashableValue::String(s) => {
                // String hashing uses content hash
                let string_ref = s.borrow();
                string_ref.content_hash.hash(state);
            },
        }
    }
}

/// Lua table representation
#[derive(Debug, Clone)]
pub struct Table {
    /// Array part of the table
    pub array: Vec<Value>,
    
    /// HashMap part of the table
    pub map: HashMap<HashableValue, Value>,
    
    /// Optional metatable
    pub metatable: Option<TableHandle>,
}

impl Table {
    /// Create a new empty table
    pub fn new() -> Self {
        Table {
            array: Vec::new(),
            map: HashMap::new(),
            metatable: None,
        }
    }
    
    /// Create a new table with capacity
    pub fn with_capacity(array_cap: usize, map_cap: usize) -> Self {
        Table {
            array: Vec::with_capacity(array_cap),
            map: HashMap::with_capacity(map_cap),
            metatable: None,
        }
    }
    
    /// Get the metatable
    pub fn metatable(&self) -> Option<TableHandle> {
        self.metatable.clone()
    }
    
    /// Set the metatable
    pub fn set_metatable(&mut self, metatable: Option<TableHandle>) {
        self.metatable = metatable;
    }
    
    /// Get the length of the array part
    pub fn array_len(&self) -> usize {
        self.array.len()
    }
    
    /// Get a field from the table
    pub fn get_field(&self, key: &Value) -> Option<Value> {
        // Try array optimization for integer keys
        if let Value::Number(n) = key {
            if n.fract() == 0.0 && *n > 0.0 && *n <= self.array.len() as f64 {
                let idx = *n as usize;
                return Some(self.array[idx - 1].clone());
            }
        }
        
        // For other keys, try to convert to hashable
        match HashableValue::from_value(key) {
            Ok(hashable) => self.map.get(&hashable).cloned(),
            Err(_) => None,
        }
    }
    
    /// Set a field in the table
    pub fn set_field(&mut self, key: Value, value: Value) -> LuaResult<()> {
        // Try array optimization for integer keys
        if let Value::Number(n) = &key {
            if n.fract() == 0.0 && *n > 0.0 {
                let idx = *n as usize;
                if idx <= self.array.len() {
                    self.array[idx - 1] = value;
                    return Ok(());
                } else if idx == self.array.len() + 1 {
                    self.array.push(value);
                    return Ok(());
                } else if idx <= self.array.len() * 2 {
                    // Fill gaps with nil
                    while self.array.len() < idx - 1 {
                        self.array.push(Value::Nil);
                    }
                    self.array.push(value);
                    return Ok(());
                }
                // For very sparse arrays, use the hash part
            }
        }
        
        // For other keys, convert to hashable
        let hashable = HashableValue::from_value(&key)?;
        if value.is_nil() {
            self.map.remove(&hashable);
        } else {
            self.map.insert(hashable, value);
        }
        
        Ok(())
    }
}

/// Upvalue state enum
#[derive(Clone)]
pub enum UpvalueState {
    /// Open upvalue (references a value on the stack)
    Open {
        /// Thread containing the value
        thread: ThreadHandle,
        
        /// Stack index of the value
        stack_index: usize,
    },
    
    /// Closed upvalue (contains the value directly)
    Closed {
        /// The captured value
        value: Value,
    },
}

// Custom Debug implementation to break circular reference cycles
impl fmt::Debug for UpvalueState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UpvalueState::Open { stack_index, .. } => {
                write!(f, "UpvalueOpen {{ stack_index: {} }}", stack_index)
            },
            UpvalueState::Closed { value } => {
                write!(f, "UpvalueClosed {{ value: {:?} }}", value)
            }
        }
    }
}

impl UpvalueState {
    /// Create a new open upvalue - with safety validations
    pub fn new_open(thread: ThreadHandle, stack_index: usize) -> Result<Self, LuaError> {
        // Add safety checks to protect against invalid indices
        const MAX_STACK_INDEX: usize = 65535; // Reasonable limit for Lua 5.1
        
        if stack_index > MAX_STACK_INDEX {
            return Err(LuaError::StackOverflow);
        }
        
        // Check that the stack position is valid
        {
            let thread_ref = thread.borrow();
            if stack_index >= thread_ref.stack.len() {
                return Err(LuaError::RuntimeError(format!(
                    "Cannot create upvalue for stack index {} (stack size: {})",
                    stack_index, thread_ref.stack.len()
                )));
            }
        }
        
        Ok(UpvalueState::Open {
            thread,
            stack_index,
        })
    }
    
    /// Create a new closed upvalue
    pub fn new_closed(value: Value) -> Self {
        UpvalueState::Closed {
            value,
        }
    }
    
    /// Check if this upvalue is open
    pub fn is_open(&self) -> bool {
        matches!(self, UpvalueState::Open { .. })
    }
    
    /// Close this upvalue, capturing the current value
    pub fn close(&mut self, value: Value) {
        *self = UpvalueState::Closed { value };
    }
    
    /// Get the upvalue's stack index if it's open
    pub fn get_stack_index(&self) -> Option<usize> {
        match self {
            UpvalueState::Open { stack_index, .. } => Some(*stack_index),
            _ => None,
        }
    }
    
    /// Get thread handle if this is an open upvalue
    pub fn get_thread(&self) -> Option<ThreadHandle> {
        match self {
            UpvalueState::Open { thread, .. } => Some(Rc::clone(thread)),
            _ => None,
        }
    }
    
    /// Get closed value if this is a closed upvalue
    pub fn get_closed_value(&self) -> Option<&Value> {
        match self {
            UpvalueState::Closed { value } => Some(value),
            _ => None,
        }
    }
    
    /// Try to get the value without accessing the stack directly
    pub fn try_get_value(&self, max_depth: usize) -> Option<Value> {
        if max_depth == 0 {
            // Prevent excessive recursion
            return None;
        }
        
        match self {
            UpvalueState::Closed { value } => Some(value.clone()),
            UpvalueState::Open { thread, stack_index } => {
                let thread_ref = thread.borrow();
                if *stack_index < thread_ref.stack.len() {
                    Some(thread_ref.stack[*stack_index].clone())
                } else {
                    None
                }
            }
        }
    }
}

/// Custom Display implementation to avoid stack overflow in Debug
impl fmt::Display for UpvalueState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UpvalueState::Open { stack_index, .. } => {
                write!(f, "UpvalueOpen[{}]", stack_index)
            },
            UpvalueState::Closed { value } => {
                // Don't print full value, just its type
                write!(f, "UpvalueClosed[{}]", value.type_name())
            }
        }
    }
}

/// Function prototype (compiled function code)
#[derive(Debug, Clone)]
pub struct FunctionProto {
    /// Bytecode instructions
    pub bytecode: Vec<u32>,
    
    /// Constant values used by the function
    pub constants: Vec<Value>,
    
    /// Number of parameters
    pub num_params: u8,
    
    /// Whether the function is variadic
    pub is_vararg: bool,
    
    /// Maximum stack size needed
    pub max_stack_size: u8,
    
    /// Upvalue information
    pub upvalues: Vec<UpvalueInfo>,
}

/// Information about an upvalue
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UpvalueInfo {
    /// Is the upvalue in the stack?
    pub in_stack: bool,
    
    /// Index in stack or outer upvalues
    pub index: u8,
}

/// Function closure
#[derive(Debug, Clone)]
pub struct Closure {
    /// Function prototype
    pub proto: FunctionProtoHandle,
    
    /// Captured upvalues
    pub upvalues: Vec<UpvalueHandle>,
}

/// Thread status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThreadStatus {
    /// Thread is running
    Running,
    
    /// Thread is suspended (yielded)
    Suspended,
    
    /// Thread completed normally
    Normal,
    
    /// Thread errored
    Error,
}

/// Lua thread (coroutine)
#[derive(Clone)]
pub struct Thread {
    /// Call frames
    pub call_frames: Vec<CallFrame>,
    
    /// Value stack
    pub stack: Vec<Value>,
    
    /// Thread status
    pub status: ThreadStatus,
    
    /// Open upvalues list (sorted by stack index, highest first)
    pub open_upvalues: Vec<UpvalueHandle>,
}

// Custom Debug implementation for Thread to break circular references
impl fmt::Debug for Thread {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Thread")
            .field("call_frames", &self.call_frames.len())
            .field("stack_size", &self.stack.len())
            .field("status", &self.status)
            .field("open_upvalues_count", &self.open_upvalues.len())
            .finish()
    }
}

impl Thread {
    /// Create a new thread
    pub fn new() -> Self {
        Thread {
            call_frames: Vec::new(),
            stack: Vec::new(),
            status: ThreadStatus::Normal,
            open_upvalues: Vec::new(),
        }
    }
}

/// Call frame (activation record)
#[derive(Debug, Clone)]
pub struct CallFrame {
    /// Closure being executed
    pub closure: ClosureHandle,
    
    /// Program counter
    pub pc: usize,
    
    /// Base register in stack
    pub base_register: u16,
    
    /// Number of expected results
    pub expected_results: Option<usize>,
    
    /// Variable arguments for this frame (if the function is vararg)
    pub varargs: Option<Vec<Value>>,
}

/// Userdata type
#[derive(Debug, Clone)]
pub struct UserData {
    /// Type ID for type safety
    pub type_id: std::any::TypeId,
    
    /// Opaque data pointer
    pub data: Vec<u8>,
    
    /// Optional metatable
    pub metatable: Option<TableHandle>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_value_basics() {
        assert_eq!(Value::Nil.type_name(), "nil");
        assert_eq!(Value::Boolean(true).type_name(), "boolean");
        assert_eq!(Value::Number(42.0).type_name(), "number");
        
        assert!(Value::Nil.is_falsey());
        assert!(Value::Boolean(false).is_falsey());
        assert!(!Value::Boolean(true).is_falsey());
        assert!(!Value::Number(0.0).is_falsey());
    }
    
    #[test]
    fn test_string_equality() {
        // Create two different strings with same content
        let s1 = LuaString::new("hello");
        let s2 = LuaString::new("hello");
        
        // Create handles
        let handle1 = Rc::new(RefCell::new(s1));
        let handle2 = Rc::new(RefCell::new(s2));
        
        // Make values
        let val1 = Value::String(handle1);
        let val2 = Value::String(handle2);
        
        // They should be equal
        assert_eq!(val1, val2);
    }
    
    #[test]
    fn test_table_basics() {
        let mut table = Table::new();
        
        // Set array elements
        let key1 = Value::Number(1.0);
        let value1 = Value::Boolean(true);
        table.set_field(key1.clone(), value1.clone()).unwrap();
        
        // Check array access
        assert_eq!(table.get_field(&key1), Some(value1));
    }
    
    #[test]
    fn test_hashable_values() {
        // Create a string
        let string = LuaString::new("test");
        let handle = Rc::new(RefCell::new(string));
        let value = Value::String(Rc::clone(&handle));
        
        // Create a hashable value
        let hashable = HashableValue::from_value(&value).unwrap();
        
        // Convert back
        let value2 = hashable.to_value();
        
        // Should be equal
        assert_eq!(value, value2);
    }
}