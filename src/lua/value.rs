//! Lua Value Types
//! 
//! This module defines all Lua value types and their representations
//! in the Ferrous Lua VM.

use super::handle::{StringHandle, TableHandle, ClosureHandle, ThreadHandle, 
                   UpvalueHandle, UserDataHandle, FunctionProtoHandle};
use super::error::{LuaError, LuaResult};
use std::collections::HashMap;
use std::fmt;
use std::hash::Hasher;

/// Type representing a C function callable from Lua
pub type CFunction = fn(&mut crate::lua::vm::ExecutionContext) -> LuaResult<i32>;

/// Main Lua value type
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Nil value
    Nil,
    
    /// Boolean value
    Boolean(bool),
    
    /// Number value (Lua uses doubles for all numbers)
    Number(f64),
    
    /// String value (handle to heap-allocated string)
    String(StringHandle),
    
    /// Table value (handle to heap-allocated table)
    Table(TableHandle),
    
    /// Function closure (handle to heap-allocated closure)
    Closure(ClosureHandle),
    
    /// Thread/coroutine (handle to heap-allocated thread)
    Thread(ThreadHandle),
    
    /// C function
    CFunction(CFunction),
    
    /// Userdata (handle to heap-allocated userdata)
    UserData(UserDataHandle),
    
    /// Function prototype (handle to heap-allocated function prototype)
    FunctionProto(FunctionProtoHandle),
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
        }
    }
}

// Make sure Value also implements the needed traits
impl Eq for Value {}

impl std::hash::Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Value::Nil => {}
            Value::Boolean(b) => b.hash(state),
            Value::Number(n) => OrderedFloat(*n).hash(state),
            Value::String(s) => s.hash(state),
            Value::Table(t) => t.hash(state),
            Value::Closure(c) => c.hash(state),
            Value::Thread(t) => t.hash(state),
            // Function pointers are not hashable in a stable way
            Value::CFunction(_) => 0.hash(state), // Use a constant hash value
            Value::UserData(u) => u.hash(state),
            Value::FunctionProto(p) => p.hash(state),
        }
    }
}

/// Lua string representation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaString {
    /// UTF-8 bytes of the string
    pub bytes: Vec<u8>,
    /// Cached hash of the content for efficient comparison
    pub content_hash: u64,
}

impl LuaString {
    /// Create a new Lua string from a Rust string slice.
    pub fn from_str(s: &str) -> Result<Self, LuaError> {
        let bytes = s.as_bytes().to_vec();
        
        // Calculate content hash for string interning
        let content_hash = Self::calculate_content_hash(&bytes);
        
        Ok(LuaString {
            bytes,
            content_hash,
        })
    }

    /// Create a new Lua string from a Rust string slice with pre-calculated hash.
    pub fn from_str_with_hash(s: &str, content_hash: u64) -> Result<Self, LuaError> {
        let bytes = s.as_bytes().to_vec();
        
        Ok(LuaString {
            bytes,
            content_hash,
        })
    }

    /// Create a new Lua string from a byte vector.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        // Calculate content hash for string interning
        let content_hash = Self::calculate_content_hash(&bytes);
        
        LuaString {
            bytes,
            content_hash,
        }
    }

    /// Calculate content hash for string interning
    pub fn calculate_content_hash(bytes: &[u8]) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        hasher.finish()
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

impl From<&str> for LuaString {
    fn from(s: &str) -> Self {
        LuaString::new(s)
    }
}

impl From<String> for LuaString {
    fn from(s: String) -> Self {
        LuaString::new(s)
    }
}

/// Lua table representation
#[derive(Debug, Clone, PartialEq, Eq)]
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
        self.metatable
    }
    
    /// Set the metatable
    pub fn set_metatable(&mut self, metatable: Option<TableHandle>) {
        self.metatable = metatable;
    }
    
    /// Get the length of the array part
    pub fn array_len(&self) -> usize {
        self.array.len()
    }
    
    /// Get from array part by index (1-based)
    pub fn get_array(&self, index: usize) -> Option<&Value> {
        if index > 0 && index <= self.array.len() {
            Some(&self.array[index - 1])
        } else {
            None
        }
    }
    
    /// Set in array part by index (1-based)
    pub fn set_array(&mut self, index: usize, value: Value) {
        if index > 0 && index <= self.array.len() {
            self.array[index - 1] = value;
        } else if index == self.array.len() + 1 {
            self.array.push(value);
        }
        // Otherwise, fall back to hash part
    }
    
    /// Get a value from the table by hashable key (requires pre-computed hash for strings)
    pub fn get_by_hashable(&self, key: &HashableValue) -> Option<&Value> {
        self.map.get(key)
    }
    
    /// Set a value in the table by hashable key (requires pre-computed hash for strings)
    pub fn set_by_hashable(&mut self, key: HashableValue, value: Value) {
        if value.is_nil() {
            self.map.remove(&key);
        } else {
            self.map.insert(key, value);
        }
    }
    
    /// Get map size for debugging
    pub fn map_len(&self) -> usize {
        self.map.len()
    }
    
    /// Check if map contains a hashable key
    pub fn contains_key(&self, key: &HashableValue) -> bool {
        self.map.contains_key(key)
    }
    
    /// Get a value from the table by any key type
    pub fn get(&self, key: &Value) -> Value {
        // Try array optimization for integer keys
        if let Value::Number(n) = key {
            if n.fract() == 0.0 && *n > 0.0 && *n <= self.array.len() as f64 {
                let idx = *n as usize;
                return self.array[idx - 1].clone();
            }
        }
        
        // For other keys, try to convert to hashable
        // Since we don't have string content hash here, we can't look up string keys
        match HashableValue::from_value_with_context(key, "Table::get") {
            Ok(hashable) => {
                self.map.get(&hashable).cloned().unwrap_or(Value::Nil)
            },
            Err(_) => {
                // Key is not hashable (e.g., a table or function)
                Value::Nil
            }
        }
    }
    
    /// Set a value in the table by any key type
    pub fn set(&mut self, key: Value, value: Value) -> LuaResult<()> {
        // Try array optimization for integer keys
        if let Value::Number(n) = &key {
            if n.fract() == 0.0 && *n > 0.0 && *n <= (self.array.len() + 1) as f64 {
                self.set_array(*n as usize, value);
                return Ok(());
            }
        }
        
        // For string keys, we cannot create a HashableValue here without content hash access
        // The caller must use heap methods that have access to string content
        if matches!(&key, Value::String(_)) {
            return Err(LuaError::InternalError(
                "Table::set cannot be used directly with string keys. Use heap.write_table_field() instead.".to_string()
            ));
        }
        
        // Otherwise use hash map (for non-string keys)
        let hashable = HashableValue::from_value_with_context(&key, "Table::set")?;
        if value.is_nil() {
            self.map.remove(&hashable);
        } else {
            self.map.insert(hashable, value);
        }
        
        Ok(())
    }
    
    /// Set a field in the table
    pub fn set_field(&mut self, key: Value, value: Value) -> LuaResult<()> {
        // Try array optimization for integer keys
        if let Value::Number(n) = &key {
            if n.fract() == 0.0 && *n > 0.0 && *n <= (self.array.len() + 1) as f64 {
                self.set_array(*n as usize, value);
                return Ok(());
            }
        }
        
        // For string keys, we cannot create a HashableValue here without content hash access
        // The caller must use heap methods that have access to string content
        if matches!(&key, Value::String(_)) {
            return Err(LuaError::InternalError(
                "Table::set_field cannot be used directly with string keys. Use heap.write_table_field() instead.".to_string()
            ));
        }
        
        // Otherwise use hash map (for non-string keys)
        let hashable = HashableValue::from_value_with_context(&key, "Table::set_field")?;
        if value.is_nil() {
            self.map.remove(&hashable);
        } else {
            self.map.insert(hashable, value);
        }
        
        Ok(())
    }
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

// Manual implementation of Hash for Table
impl std::hash::Hash for Table {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash array part
        self.array.hash(state);
        
        // Hash metatable
        self.metatable.hash(state);
        
        // For the HashMap, use a commutative hash combining technique
        // Hash the length first
        let size = self.map.len();
        size.hash(state);
        
        // Then combine hashes of each key-value pair using XOR (commutative operation)
        // This ensures order independence
        let mut combined_hash: u64 = 0;
        
        for (k, v) in &self.map {
            // Create a separate hasher for each key-value pair
            let mut pair_hasher = std::collections::hash_map::DefaultHasher::new();
            k.hash(&mut pair_hasher);
            v.hash(&mut pair_hasher);
            let pair_hash = pair_hasher.finish();
            
            // XOR the pair hash with the combined hash (order independent)
            combined_hash ^= pair_hash;
        }
        
        // Hash the final combined value
        combined_hash.hash(state);
    }
}

/// Wrapper for hashable values (used as table keys)
#[derive(Debug, Clone)]
pub enum HashableValue {
    Nil,
    Boolean(bool),
    Number(OrderedFloat),
    String(StringHandle, u64), // Handle and content hash for content-based comparison
}

impl HashableValue {
    /// Check if a value is hashable without creating the HashableValue
    /// This can be used for validation without generating errors.
    pub fn is_hashable(value: &Value) -> bool {
        match value {
            Value::Nil | Value::Boolean(_) | Value::Number(_) | Value::String(_) => true,
            _ => false
        }
    }
    
    /// Try to create a hashable value from a Lua value with context for better error messages
    pub fn from_value_with_context(value: &Value, context: &str) -> LuaResult<Self> {
        match value {
            Value::Nil => Ok(HashableValue::Nil),
            Value::Boolean(b) => Ok(HashableValue::Boolean(*b)),
            Value::Number(n) => Ok(HashableValue::Number(OrderedFloat(*n))),
            Value::String(_) => {
                // String values require content hash which must be provided by the caller
                // This method cannot be used for string values - callers must use
                // methods that have access to the heap for content hash lookup
                Err(LuaError::InternalError(
                    format!("Cannot create HashableValue from string without content hash access. Use heap.create_hashable_value() instead. Context: {}", context)
                ))
            },
            Value::Table(_) => {
                Err(LuaError::TypeError {
                    expected: format!("nil, boolean, number, or string (in {})", context),
                    got: "table (tables cannot be used as keys)".to_string(),
                })
            },
            Value::Closure(_) | Value::CFunction(_) | Value::FunctionProto(_) => {
                Err(LuaError::TypeError {
                    expected: format!("nil, boolean, number, or string (in {})", context),
                    got: "function (functions cannot be used as keys)".to_string(),
                })
            },
            other => {
                Err(LuaError::TypeError {
                    expected: format!("nil, boolean, number, or string (in {})", context),
                    got: format!("{} (cannot be used as a key)", other.type_name()),
                })
            }
        }
    }
    
    /// Create a HashableValue::String from a raw Handle<LuaString> with content hash
    pub fn from_string_handle_with_hash(handle: super::arena::Handle<LuaString>, content_hash: u64) -> Self {
        // Convert Handle<LuaString> to StringHandle first
        let string_handle = StringHandle::from(handle);
        HashableValue::String(string_handle, content_hash)
    }

    /// Try to create a hashable value from a reference to a Value
    /// This is used for table lookups where we need the content hash
    pub fn from_value_ref(value: &Value, provider: &impl StringContentProvider) -> Option<Self> {
        match value {
            Value::Nil => Some(HashableValue::Nil),
            Value::Boolean(b) => Some(HashableValue::Boolean(*b)),
            Value::Number(n) => Some(HashableValue::Number(OrderedFloat(*n))),
            Value::String(handle) => {
                // Get content hash from provider
                provider.get_string_content_hash(*handle)
                    .map(|hash| HashableValue::String(*handle, hash))
            },
            _ => None,
        }
    }

    /// Try to create a hashable value from a Lua value with string content hash
    /// This version requires the caller to provide the string content hash when needed
    pub fn from_value_with_hash(value: &Value, string_hash: Option<u64>) -> LuaResult<Self> {
        match value {
            Value::Nil => Ok(HashableValue::Nil),
            Value::Boolean(b) => Ok(HashableValue::Boolean(*b)),
            Value::Number(n) => Ok(HashableValue::Number(OrderedFloat(*n))),
            Value::String(s) => {
                match string_hash {
                    Some(hash) => Ok(HashableValue::String(*s, hash)),
                    None => Err(LuaError::RuntimeError(
                        "String content hash required for HashableValue::String".to_string()
                    ))
                }
            },
            _ => Err(LuaError::TypeError {
                expected: "nil, boolean, number, or string".to_string(),
                got: format!("{} (cannot be used as a key)", value.type_name()),
            })
        }
    }

    
    /// Convert back to a Lua Value
    pub fn to_value(&self) -> Value {
        match self {
            HashableValue::Nil => Value::Nil,
            HashableValue::Boolean(b) => Value::Boolean(*b),
            HashableValue::Number(n) => Value::Number(n.0),
            HashableValue::String(s, _) => Value::String(*s),
        }
    }
    
    /// Debug representation showing the actual hash used
    pub fn debug_hash_info(&self) -> String {
        match self {
            HashableValue::Nil => "HashableValue::Nil".to_string(),
            HashableValue::Boolean(b) => format!("HashableValue::Boolean({})", b),
            HashableValue::Number(n) => format!("HashableValue::Number({})", n.0),
            HashableValue::String(handle, hash) => {
                format!("HashableValue::String(handle: {:?}, content_hash: {:#x})", handle, hash)
            },
        }
    }
}

impl PartialEq for HashableValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (HashableValue::Nil, HashableValue::Nil) => true,
            (HashableValue::Boolean(a), HashableValue::Boolean(b)) => a == b,
            (HashableValue::Number(a), HashableValue::Number(b)) => a == b,
            (HashableValue::String(handle_a, hash_a), HashableValue::String(handle_b, hash_b)) => {
                // Fast path: same handle means same string
                if handle_a == handle_b {
                    return true;
                }
                // Content-based comparison using cached hashes
                hash_a == hash_b
            },
            _ => false,
        }
    }
}

impl Eq for HashableValue {}

impl std::hash::Hash for HashableValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash the discriminant first
        std::mem::discriminant(self).hash(state);
        
        match self {
            HashableValue::Nil => {},
            HashableValue::Boolean(b) => b.hash(state),
            HashableValue::Number(n) => n.hash(state),
            HashableValue::String(_, content_hash) => {
                // Use the cached content hash for consistent hashing
                content_hash.hash(state);
            },
        }
    }
}

/// Trait for accessing string content to compute hashes
pub trait StringContentProvider {
    /// Get the content bytes of a string by handle
    fn get_string_content(&self, handle: StringHandle) -> Option<&[u8]>;
    
    /// Get the content hash of a string by handle
    fn get_string_content_hash(&self, handle: StringHandle) -> Option<u64>;
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

impl std::hash::Hash for OrderedFloat {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

/// Function prototype (compiled function code)
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Closure {
    /// Function prototype
    pub proto: FunctionProto,
    
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

/// Upvalue representation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Upvalue {
    /// Location in thread stack (if open)
    pub stack_index: Option<usize>,
    
    /// Captured value (if closed)
    pub value: Option<Value>,
}

/// Userdata type
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    fn test_value_type_names() {
        assert_eq!(Value::Nil.type_name(), "nil");
        assert_eq!(Value::Boolean(true).type_name(), "boolean");
        assert_eq!(Value::Number(42.0).type_name(), "number");
    }
    
    #[test]
    fn test_value_truthiness() {
        assert!(Value::Nil.is_falsey());
        assert!(Value::Boolean(false).is_falsey());
        assert!(!Value::Boolean(true).is_falsey());
        assert!(!Value::Number(0.0).is_falsey());
    }
    
    #[test]
    fn test_table_array_access() {
        let mut table = Table::new();
        
        table.set_array(1, Value::Number(10.0));
        table.set_array(2, Value::Number(20.0));
        
        assert_eq!(table.get_array(1), Some(&Value::Number(10.0)));
        assert_eq!(table.get_array(2), Some(&Value::Number(20.0)));
        assert_eq!(table.get_array(3), None);
    }
    
    #[test]
    fn test_table_hashable_access() {
        let mut table = Table::new();
        
        // Test with hashable key types
        let num_key = HashableValue::Number(OrderedFloat(1.0));
        let bool_key = HashableValue::Boolean(true);
        
        table.set_by_hashable(num_key.clone(), Value::Number(10.0));
        table.set_by_hashable(bool_key.clone(), Value::Number(20.0));
        
        assert_eq!(
            table.get_by_hashable(&num_key),
            Some(&Value::Number(10.0))
        );
        assert_eq!(
            table.get_by_hashable(&bool_key),
            Some(&Value::Number(20.0))
        );
    }
}