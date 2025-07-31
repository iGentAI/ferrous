//! Rc<RefCell> Handle Types for Lua VM
//!
//! This module defines handle types that use Rc<RefCell> for fine-grained
//! interior mutability instead of the arena-based approach with global RefCell.
//! This allows for better borrowing patterns and proper shared upvalue semantics.

use std::rc::Rc;
use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::fmt;

// Forward declarations for types that will be defined
use super::value::{
    Value, LuaString, Table, Closure, Thread, Upvalue, 
    UserData, FunctionProto, OrderedFloat, HashableValue
};

/// Basic handle types wrapping Rc<RefCell<T>>

/// String handle type
pub type StringHandle = Rc<RefCell<LuaString>>;

/// Table handle type
pub type TableHandle = Rc<RefCell<Table>>;

/// Closure handle type
pub type ClosureHandle = Rc<RefCell<Closure>>;

/// Thread handle type
pub type ThreadHandle = Rc<RefCell<Thread>>;

/// Upvalue handle type
pub type UpvalueHandle = Rc<RefCell<UpvalueState>>;

/// Userdata handle type
pub type UserDataHandle = Rc<RefCell<UserData>>;

/// Function prototype handle type (immutable, so just Rc)
pub type FunctionProtoHandle = Rc<FunctionProto>;

/// Upvalue state enum
#[derive(Debug, Clone)]
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

/// Implementation for UpvalueState
impl UpvalueState {
    /// Create a new open upvalue
    pub fn new_open(thread: ThreadHandle, stack_index: usize) -> Self {
        UpvalueState::Open {
            thread,
            stack_index,
        }
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
    
    /// Get the value of this upvalue
    pub fn get_value(&self) -> Value {
        match self {
            UpvalueState::Open { thread, stack_index } => {
                // Get value from thread stack
                let thread_ref = thread.borrow();
                if *stack_index < thread_ref.stack.len() {
                    thread_ref.stack[*stack_index].clone()
                } else {
                    Value::Nil
                }
            },
            UpvalueState::Closed { value } => {
                value.clone()
            }
        }
    }
    
    /// Set the value of this upvalue
    pub fn set_value(&mut self, value: Value) {
        match self {
            UpvalueState::Open { thread, stack_index } => {
                // Set value in thread stack
                let mut thread_ref = thread.borrow_mut();
                if *stack_index < thread_ref.stack.len() {
                    thread_ref.stack[*stack_index] = value;
                }
                // If stack index is out of bounds, do nothing
            },
            UpvalueState::Closed { value: ref mut v } => {
                *v = value;
            }
        }
    }
}

// Implement PartialEq, Eq, Hash for UpvalueState by handle identity
impl PartialEq for UpvalueState {
    fn eq(&self, _other: &Self) -> bool {
        // Identity comparison not meaningful for upvalues
        // This implementation exists just for trait bounds
        false
    }
}

impl Eq for UpvalueState {}

impl Hash for UpvalueState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Identity-based hash
        // Just hash a distinguishing value to satisfy the trait bound
        match self {
            UpvalueState::Open { .. } => 1u8.hash(state),
            UpvalueState::Closed { .. } => 2u8.hash(state),
        }
    }
}

impl fmt::Display for UpvalueState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UpvalueState::Open { stack_index, .. } => {
                write!(f, "UpvalueOpen[{}]", stack_index)
            },
            UpvalueState::Closed { value } => {
                write!(f, "UpvalueClosed[{}]", value)
            }
        }
    }
}

/// New version of Value updated for Rc<RefCell> handle types
#[derive(Debug, Clone)]
pub enum RcValue {
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
    CFunction(super::value::CFunction),
    
    /// Userdata (handle to heap-allocated userdata)
    UserData(UserDataHandle),
    
    /// Function prototype (handle to heap-allocated function prototype)
    FunctionProto(FunctionProtoHandle),
}

/// Table structure for Rc<RefCell> implementation
#[derive(Debug, Clone)]
pub struct RcTable {
    /// Array part of the table
    pub array: Vec<RcValue>,
    
    /// Hash part of the table
    pub map: std::collections::HashMap<RcHashableValue, RcValue>,
    
    /// Optional metatable
    pub metatable: Option<TableHandle>,
}

/// Hashable value for table keys in the Rc<RefCell> implementation
#[derive(Debug, Clone)]
pub enum RcHashableValue {
    /// Nil value
    Nil,
    
    /// Boolean value
    Boolean(bool),
    
    /// Number value with total ordering
    Number(OrderedFloat),
    
    /// String value with cached content hash
    String(StringHandle, u64),
}

// Implement PartialEq, Eq, Hash for RcHashableValue
impl PartialEq for RcHashableValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (RcHashableValue::Nil, RcHashableValue::Nil) => true,
            (RcHashableValue::Boolean(a), RcHashableValue::Boolean(b)) => a == b,
            (RcHashableValue::Number(a), RcHashableValue::Number(b)) => a == b,
            (RcHashableValue::String(handle_a, hash_a), RcHashableValue::String(handle_b, hash_b)) => {
                // Fast path: same handle means same string
                if Rc::ptr_eq(handle_a, handle_b) {
                    return true;
                }
                
                // Content-based comparison
                if hash_a == hash_b {
                    // If content hashes match, do a deeper comparison
                    let string_a = handle_a.borrow();
                    let string_b = handle_b.borrow();
                    string_a.bytes == string_b.bytes
                } else {
                    false
                }
            },
            _ => false,
        }
    }
}

impl Eq for RcHashableValue {}

impl Hash for RcHashableValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash discriminant first
        std::mem::discriminant(self).hash(state);
        
        match self {
            RcHashableValue::Nil => {},
            RcHashableValue::Boolean(b) => b.hash(state),
            RcHashableValue::Number(n) => n.hash(state),
            RcHashableValue::String(_, content_hash) => {
                // Use cached content hash for consistent hashing
                content_hash.hash(state);
            },
        }
    }
}

// Implementation for converting between old and new types
impl From<Value> for RcValue {
    fn from(_value: Value) -> Self {
        // This will be properly implemented when migrating
        // For now, just a placeholder
        RcValue::Nil
    }
}

// For testing
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_upvalue_state() {
        // Create a thread
        let thread = Rc::new(RefCell::new(Thread::new()));
        
        // Add a value to the thread stack
        {
            let mut thread_ref = thread.borrow_mut();
            thread_ref.stack.push(Value::Number(42.0));
        }
        
        // Create an open upvalue
        let upvalue_state = UpvalueState::new_open(Rc::clone(&thread), 0);
        
        // Check if it correctly reads the value
        match &upvalue_state {
            UpvalueState::Open { thread: t, stack_index } => {
                assert!(Rc::ptr_eq(t, &thread));
                assert_eq!(*stack_index, 0);
            },
            _ => panic!("Expected Open upvalue"),
        }
        
        // Create a closed upvalue
        let closed_state = UpvalueState::new_closed(Value::String(StringHandle::from(
            super::super::arena::Handle::new(1, 1))));
        
        match &closed_state {
            UpvalueState::Closed { value } => {
                assert!(matches!(value, Value::String(_)));
            },
            _ => panic!("Expected Closed upvalue"),
        }
    }
    
    #[test]
    fn test_rc_hashable_value() {
        // Create string handles with same content but different instances
        let string1 = LuaString::new("test");
        let string2 = LuaString::new("test");
        
        let handle1 = Rc::new(RefCell::new(string1));
        let handle2 = Rc::new(RefCell::new(string2));
        
        // Create hashable values with same content hash
        let hash = 12345; // Dummy hash
        let hashable1 = RcHashableValue::String(Rc::clone(&handle1), hash);
        let hashable2 = RcHashableValue::String(Rc::clone(&handle2), hash);
        
        // They should be equal because they have the same content
        assert_eq!(hashable1, hashable2);
        
        // Test other types
        let bool1 = RcHashableValue::Boolean(true);
        let bool2 = RcHashableValue::Boolean(true);
        assert_eq!(bool1, bool2);
        
        let num1 = RcHashableValue::Number(OrderedFloat(3.14));
        let num2 = RcHashableValue::Number(OrderedFloat(3.14));
        assert_eq!(num1, num2);
    }
}