//! Rc<RefCell> Based Lua Value Types
//!
//! This module defines Lua value types using Rc<RefCell> for heap-allocated objects,
//! which provides fine-grained interior mutability and proper shared mutable state semantics.

use std::rc::Rc;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

use super::error::{LuaError, LuaResult};

thread_local! {
    /// A set of raw pointers to heap-allocated values (tables, closures, etc.)
    /// that are currently being visited by a Debug::fmt call to prevent recursion.
    static DEBUG_SEEN: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
}

/// An RAII guard to manage adding/removing pointers from the DEBUG_SEEN set.
struct SeenGuard {
    ptr: usize,
    is_new: bool,
}

impl SeenGuard {
    /// Creates a new guard. Inserts the pointer into the seen set.
    /// `is_new` will be true if the pointer was not already in the set.
    fn new(ptr: usize) -> Self {
        let is_new = DEBUG_SEEN.with(|seen| seen.borrow_mut().insert(ptr));
        SeenGuard { ptr, is_new }
    }
}

impl Drop for SeenGuard {
    fn drop(&mut self) {
        // Only remove the pointer if this guard was the one that added it.
        if self.is_new {
            DEBUG_SEEN.with(|seen| {
                seen.borrow_mut().remove(&self.ptr);
            });
        }
    }
}

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
#[derive(Clone)]
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

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Nil => write!(f, "Nil"),
            Value::Boolean(b) => f.debug_tuple("Boolean").field(b).finish(),
            Value::Number(n) => f.debug_tuple("Number").field(n).finish(),
            Value::String(s) => {
                if let Ok(string_ref) = s.try_borrow() {
                    write!(f, "String({:?})", &*string_ref)
                } else {
                    write!(f, "String(<borrowed>)")
                }
            },
            Value::Table(t) => {
                let ptr = Rc::as_ptr(t) as usize;
                let guard = SeenGuard::new(ptr);
                if guard.is_new {
                    if let Ok(table_ref) = t.try_borrow() {
                        f.debug_tuple("Table").field(&*table_ref).finish()
                    } else {
                        write!(f, "Table(<borrowed>)")
                    }
                } else {
                    write!(f, "Table(<recursive>)")
                }
            },
            Value::Closure(c) => {
                let ptr = Rc::as_ptr(c) as usize;
                let guard = SeenGuard::new(ptr);
                if guard.is_new {
                    if let Ok(closure_ref) = c.try_borrow() {
                        f.debug_tuple("Closure").field(&*closure_ref).finish()
                    } else {
                        write!(f, "Closure(<borrowed>)")
                    }
                } else {
                    write!(f, "Closure(<recursive>)")
                }
            },
            Value::Thread(t) => {
                f.debug_tuple("Thread").field(t).finish()
            },
            Value::CFunction(c) => {
                f.debug_tuple("CFunction").field(&(*c as *const ())).finish()
            },
            Value::UserData(u) => {
                let ptr = Rc::as_ptr(u) as usize;
                let guard = SeenGuard::new(ptr);
                if guard.is_new {
                    if let Ok(userdata_ref) = u.try_borrow() {
                        f.debug_tuple("UserData").field(&*userdata_ref).finish()
                    } else {
                        write!(f, "UserData(<borrowed>)")
                    }
                } else {
                    write!(f, "UserData(<recursive>)")
                }
            },
            Value::FunctionProto(p) => {
                let ptr = Rc::as_ptr(p) as usize;
                let guard = SeenGuard::new(ptr);
                if guard.is_new {
                    f.debug_tuple("FunctionProto").field(&**p).finish()
                } else {
                    write!(f, "FunctionProto(<recursive>)")
                }
            },
            Value::PendingMetamethod(val) => f.debug_tuple("PendingMetamethod").field(val).finish(),
        }
    }
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
            Value::String(s) => {
                let s_ref = s.borrow();
                match s_ref.to_str() {
                    Ok(str_slice) => write!(f, "{}", str_slice),
                    Err(_) => write!(f, "<binary string>"),
                }
            },
            Value::Table(t) => write!(f, "table: {:p}", Rc::as_ptr(t)),
            Value::Closure(c) => write!(f, "function: {:p}", Rc::as_ptr(c)),
            Value::CFunction(c) => write!(f, "function: {:p}", *c),
            Value::Thread(t) => write!(f, "thread: {:p}", Rc::as_ptr(t)),
            Value::UserData(u) => write!(f, "userdata: {:p}", Rc::as_ptr(u)),
            Value::FunctionProto(p) => write!(f, "function: {:p}", Rc::as_ptr(p)),
            Value::PendingMetamethod(val) => write!(f, "pending metamethod for {}", val.type_name()),
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
                // Use bytes directly to ensure consistency with PartialEq
                let string_ref = s.borrow();
                string_ref.bytes.hash(state);
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
        
        eprintln!("DEBUG LuaString::from_str: Creating string '{}' with hash {}", s, content_hash);
        
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
        
        eprintln!("DEBUG LuaString::new: Creating string with {} bytes, hash {}", bytes.len(), content_hash);
        
        LuaString {
            bytes,
            content_hash,
        }
    }
    
    /// Create a new Lua string from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let content_hash = Self::calculate_content_hash(bytes);
        
        eprintln!("DEBUG LuaString::from_bytes: Creating string from {} bytes, hash {}", bytes.len(), content_hash);
        
        LuaString {
            bytes: bytes.to_vec(),
            content_hash,
        }
    }
    
    /// Calculate content hash for string interning
    pub fn calculate_content_hash(bytes: &[u8]) -> u64 {
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        let hash = hasher.finish();
        eprintln!("DEBUG LuaString::calculate_content_hash: {} bytes -> hash {}", bytes.len(), hash);
        hash
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
#[derive(Clone)]
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

impl fmt::Debug for HashableValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HashableValue::Nil => write!(f, "Nil"),
            HashableValue::Boolean(b) => f.debug_tuple("Boolean").field(b).finish(),
            HashableValue::Number(n) => f.debug_tuple("Number").field(&n.0).finish(),
            HashableValue::String(s) => {
                if let Ok(string_ref) = s.try_borrow() {
                    write!(f, "String({:?})", &*string_ref)
                } else {
                    write!(f, "String(<borrowed>)")
                }
            },
        }
    }
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
                
                // Use try_borrow to handle concurrent access gracefully
                let a_borrowed = match a.try_borrow() {
                    Ok(ref_a) => ref_a,
                    Err(_) => return false, // Can't compare if one is mutably borrowed
                };
                
                let b_borrowed = match b.try_borrow() {
                    Ok(ref_b) => ref_b,
                    Err(_) => return false, // Can't compare if one is mutably borrowed
                };
                
                // Check content hash first for efficiency
                if a_borrowed.content_hash != b_borrowed.content_hash {
                    return false;
                }
                
                // If hashes match, verify actual content
                a_borrowed.bytes == b_borrowed.bytes
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
                // Use bytes directly to ensure consistency with PartialEq
                let string_ref = s.borrow();
                string_ref.bytes.hash(state);
            },
        }
    }
}

/// Lua table representation
#[derive(Clone)]
pub struct Table {
    /// Array part of the table
    pub array: Vec<Value>,
    
    /// HashMap part of the table
    pub map: HashMap<HashableValue, Value>,
    
    /// Optional metatable
    pub metatable: Option<TableHandle>,
}

impl fmt::Debug for Table {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Table")
            .field("array", &self.array)
            .field("map", &self.map)
            .field("metatable", &self.metatable)
            .finish()
    }
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
    
    /// Get the length of the array part following Lua 5.1 semantics
    /// In Lua 5.1, #t is the highest integer index i such that t[i] != nil and t[i+1] == nil
    pub fn array_len(&self) -> usize {
        if self.array.is_empty() {
            return 0;
        }
        if self.array.last().map_or(true, |v| !v.is_nil()) {
            return self.array.len();
        }
        // Binary search for the last non-nil element (the "border")
        let mut high = self.array.len();
        let mut low = 0;
        while low < high {
            let mid = high - (high - low) / 2;
            if self.array[mid - 1].is_nil() {
                high = mid - 1;
            } else {
                low = mid;
            }
        }
        low
    }
    
    /// Get a field from the table (raw get, no metamethods).
    /// Distinguishes between a key not being present (`None`) and a key being present with a `nil` value (`Some(Value::Nil)`).
    pub fn get_field(&self, key: &Value) -> Option<Value> {
        eprintln!("DEBUG Table::get_field: Called with key={:?}", key);
        eprintln!("DEBUG Table::get_field: Table has {} array elements, {} map entries", 
                 self.array.len(), self.map.len());
        
        // Handle numeric keys which might access the array part.
        if let Value::Number(n) = key {
            eprintln!("DEBUG Table::get_field: Numeric key n={}", n);
            // Check for integer value suitable for 1-based indexing.
            if n.fract() == 0.0 && *n >= 1.0 {
                let index = *n as usize;
                // If the index is within the bounds of our array part, the array is authoritative.
                if index > 0 && index <= self.array.len() {
                    let value = self.array[index - 1].clone();
                    eprintln!("DEBUG Table::get_field: Found in array[{}]: {:?}", index - 1, value);
                    // The key is in the array part. Return its value, which might be Some(Nil).
                    return Some(value);
                } else {
                    eprintln!("DEBUG Table::get_field: Numeric key {} outside array bounds (len={})", 
                             index, self.array.len());
                }
            } else {
                eprintln!("DEBUG Table::get_field: Non-integer numeric key or < 1: {}", n);
            }
        }

        // For non-integer numbers, strings, or integers outside the array part, check the map.
        match HashableValue::from_value(key) {
            Ok(hashable) => {
                eprintln!("DEBUG Table::get_field: Converted to hashable: {:?}", hashable);
                let result = self.map.get(&hashable).cloned();
                eprintln!("DEBUG Table::get_field: Map lookup result: {:?}", result.as_ref().map(|v| format!("{:?}", v)));
                result
            },
            Err(e) => {
                eprintln!("DEBUG Table::get_field: Key is not hashable: {:?}", e);
                None // Key is not a hashable type.
            }
        }
    }
    
    /// Gets a mutable reference to a value in the table by key.
    /// This is crucial for the two-phase commit pattern for circular references.
    pub fn get_field_mut(&mut self, key: &Value) -> Option<&mut Value> {
        // Attempt to find in the array part first for integer keys
        if let Value::Number(n) = key {
            if n.fract() == 0.0 && *n >= 1.0 {
                let index = *n as usize;
                if index > 0 && index <= self.array.len() {
                    // Lua indices are 1-based
                    return self.array.get_mut(index - 1);
                }
            }
        }
        
        // Fall back to the map part
        match HashableValue::from_value(key) {
            Ok(hashable) => self.map.get_mut(&hashable),
            Err(_) => None,
        }
    }
    
    /// Set a field in the table
    pub fn set_field(&mut self, key: Value, value: Value) -> LuaResult<()> {
        eprintln!("DEBUG Table::set_field: Called with key={:?}, value={:?}", key, value);
        eprintln!("DEBUG Table::set_field: Current state - array.len()={}, map.len()={}", 
                 self.array.len(), self.map.len());
        
        // Try array optimization for integer keys
        if let Value::Number(n) = &key {
            eprintln!("DEBUG Table::set_field: Key is Number({})", n);
            if n.fract() == 0.0 && *n > 0.0 {
                let idx = *n as usize;
                eprintln!("DEBUG Table::set_field: Integer key, index={}, array.len()={}", idx, self.array.len());
                
                if idx > 0 && idx <= self.array.len() {
                    eprintln!("DEBUG Table::set_field: Setting array[{}] = {:?}", idx - 1, value);
                    self.array[idx - 1] = value;
                    eprintln!("DEBUG Table::set_field: Array assignment complete");
                    return Ok(());
                } else if idx == self.array.len() + 1 {
                    eprintln!("DEBUG Table::set_field: Appending to array at index {}", idx - 1);
                    self.array.push(value);
                    eprintln!("DEBUG Table::set_field: Array append complete");
                    return Ok(());
                } else if idx > 0 && idx <= self.array.capacity() && idx < 10000 { // Heuristic to avoid huge allocations
                    eprintln!("DEBUG Table::set_field: Resizing array from {} to {}", self.array.len(), idx);
                    // Fill gaps with nil
                    self.array.resize(idx, Value::Nil);
                    self.array[idx - 1] = value;
                    eprintln!("DEBUG Table::set_field: Array resize and assignment complete");
                    return Ok(());
                }
                eprintln!("DEBUG Table::set_field: Array path not taken, falling through to map");
                // For very sparse arrays, fall through to use the hash part
            } else {
                eprintln!("DEBUG Table::set_field: Non-integer number key, using map");
            }
        } else {
            eprintln!("DEBUG Table::set_field: Non-number key, using map");
        }
        
        // For other keys, convert to hashable
        eprintln!("DEBUG Table::set_field: Converting key to HashableValue");
        let hashable = HashableValue::from_value(&key)?;
        eprintln!("DEBUG Table::set_field: HashableValue conversion successful: {:?}", hashable);
        
        if value.is_nil() {
            eprintln!("DEBUG Table::set_field: Value is nil, removing from map");
            let removed = self.map.remove(&hashable);
            eprintln!("DEBUG Table::set_field: Remove result: {:?}", removed.is_some());
        } else {
            eprintln!("DEBUG Table::set_field: Inserting into map: {:?} => {:?}", hashable, value);
            let previous = self.map.insert(hashable.clone(), value.clone());
            eprintln!("DEBUG Table::set_field: Map insert complete, previous value: {:?}", previous.is_some());
            
            // VERIFICATION: Immediately check if we can retrieve the value
            if let Some(verification_value) = self.map.get(&hashable) {
                eprintln!("DEBUG Table::set_field: VERIFICATION SUCCESS - retrieved: {:?}", verification_value);
            } else {
                eprintln!("DEBUG Table::set_field: VERIFICATION FAILED - key not found in map immediately after insert!");
            }
        }
        
        eprintln!("DEBUG Table::set_field: Final state - array.len()={}, map.len()={}", 
                 self.array.len(), self.map.len());
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
#[derive(Clone)]
pub struct Closure {
    /// Function prototype
    pub proto: FunctionProtoHandle,
    
    /// Captured upvalues
    pub upvalues: Vec<UpvalueHandle>,
    
    /// Function environment (Lua 5.1 cl->env field) 
    pub env: TableHandle,
}

impl fmt::Debug for Closure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Closure")
            .field("proto", &self.proto)
            .field("upvalues", &self.upvalues)
            .field("env", &self.env)
            .finish()
    }
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
    /// Execution frames (normal calls and, in the future, continuations)
    pub call_frames: Vec<Frame>,
    
    /// Value stack
    pub stack: Vec<Value>,
    
    /// Thread status
    pub status: ThreadStatus,
    
    /// Open upvalues list (sorted by stack index, highest first)
    pub open_upvalues: Vec<UpvalueHandle>,

    /// Logical top of the stack (Lua's `L->top`).
    ///
    /// This decouples *visible* register space from the physical backing
    /// vector length, allowing the VM to adjust `top` on function
    /// boundaries without destroying values that reside above it.
    pub top: usize,
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
            top: 0,
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
    /// Is this call protected (pcall/xpcall)?
    pub is_protected: bool,
    /// Error handler for xpcall
    pub xpcall_handler: Option<Value>,
    /// Result base for protected calls
    pub result_base: usize,
    /// Fixed frame top limit (base + maxstacksize)
    pub frame_top: usize,
}

/// ---------------------------------------------------------------------------
/// Unified execution-frame abstraction
///
/// The Frame enum provides a unified execution model that completely eliminates
/// the temporal state separation issues. All operations now execute directly
/// using the Frame-based architecture with no queue dependencies.
/// ---------------------------------------------------------------------------

/// A single entry on the VM's execution stack.
#[derive(Debug, Clone)]
pub enum Frame {
    /// A normal Lua call frame (wraps the original `CallFrame` struct).
    Call(CallFrame),

    /// A continuation frame used to resume a previously suspended VM
    /// operation (for example after a metamethod call or cooperative
    /// yield).  During the transition period this is intentionally very
    /// lightweight – additional state can be threaded through the
    /// opaque `state` field without touching the public surface.
    Continuation(ContinuationFrame),
}

/// Placeholder structure for continuation frames.
///
/// This supports the unified Frame architecture that eliminated all queue infrastructure.
/// The Frame-based execution model provides direct execution without temporal state separation.
/// This structure is available for future use but current implementation uses direct execution.
#[derive(Debug, Clone)]
pub struct ContinuationFrame {
    /// Program-counter to jump to when the continuation is resumed.
    pub resume_pc: usize,

    /// State payload for suspended operations. With the elimination of the queue system,
    /// this provides a foundation for future features requiring state preservation.
    pub state: ContinuationState,
}

/// Continuation state for the queue-free execution model.
///
/// With the complete elimination of queue infrastructure, this supports the direct
/// execution model while providing flexibility for future enhancements.
#[derive(Debug, Clone)]
pub enum ContinuationState {
    /// The continuation has been created but not yet executed.
    Pending,
    /// The continuation finished and produced return values for direct processing.
    /// This replaces the old queue-based return value handling with immediate processing.
    Completed(Vec<Value>),
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