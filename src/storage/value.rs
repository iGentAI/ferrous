//! Value types for storage engine
//! 
//! Defines all Redis-compatible data types and their operations.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Instant, Duration};
use std::sync::Arc;
use crate::storage::skiplist::SkipList;
use crate::storage::stream::Stream;

/// All possible Redis value types
#[derive(Debug, Clone)]
pub enum Value {
    /// String value (bytes)
    String(Vec<u8>),
    
    /// List value (ordered collection)
    List(VecDeque<Vec<u8>>),
    
    /// Set value (unordered unique collection)
    Set(HashSet<Vec<u8>>),
    
    /// Hash value (field-value pairs)
    Hash(HashMap<Vec<u8>, Vec<u8>>),
    
    /// Sorted set value using skip list implementation
    SortedSet(Arc<SkipList<Vec<u8>, f64>>),
    
    /// Stream value for time-series data - direct storage for integrated architecture
    Stream(Stream),
}

/// Value type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    String,
    List,
    Set,
    Hash,
    SortedSet,
    Stream,
}

/// String encoding optimization
#[derive(Debug, Clone, Copy)]
pub enum StringEncoding {
    /// Raw bytes
    Raw,
    /// Integer value stored as string
    Int,
    /// Small string optimization (future)
    Embstr,
}

/// Metadata for stored values
#[derive(Debug, Clone)]
pub struct ValueMetadata {
    /// When the value expires (if any)
    pub expires_at: Option<Instant>,
    
    /// When the value was created
    pub created_at: Instant,
    
    /// Last access time for LRU
    pub last_accessed: Instant,
    
    /// String encoding type
    pub encoding: StringEncoding,
}

/// A stored entry with value and metadata
#[derive(Debug, Clone)]
pub struct StoredValue {
    /// The actual value
    pub value: Value,
    
    /// Metadata for the value
    pub metadata: ValueMetadata,
}

impl Value {
    /// Get the type of this value
    pub fn value_type(&self) -> ValueType {
        match self {
            Value::String(_) => ValueType::String,
            Value::List(_) => ValueType::List,
            Value::Set(_) => ValueType::Set,
            Value::Hash(_) => ValueType::Hash,
            Value::SortedSet(_) => ValueType::SortedSet,
            Value::Stream(_) => ValueType::Stream,
        }
    }
    
    /// Create a string value from bytes
    pub fn string<T: Into<Vec<u8>>>(data: T) -> Self {
        Value::String(data.into())
    }
    
    /// Create an integer string value
    pub fn integer(n: i64) -> Self {
        Value::String(n.to_string().into_bytes())
    }
    
    /// Try to parse string value as integer
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Value::String(bytes) => {
                std::str::from_utf8(bytes)
                    .ok()?
                    .parse::<i64>()
                    .ok()
            }
            _ => None,
        }
    }
    
    /// Get string bytes if this is a string value
    pub fn as_string(&self) -> Option<&[u8]> {
        match self {
            Value::String(bytes) => Some(bytes),
            _ => None,
        }
    }
    
    /// Create an empty list
    pub fn empty_list() -> Self {
        Value::List(VecDeque::new())
    }
    
    /// Create an empty set
    pub fn empty_set() -> Self {
        Value::Set(HashSet::new())
    }
    
    /// Create an empty hash
    pub fn empty_hash() -> Self {
        Value::Hash(HashMap::new())
    }
    
    /// Create an empty sorted set
    pub fn empty_sorted_set() -> Self {
        Value::SortedSet(Arc::new(SkipList::new()))
    }
    
    /// Create an empty stream value for size calculation
    pub fn empty_stream() -> Self {
        Value::Stream(Stream::new())
    }
}

impl ValueMetadata {
    /// Create new metadata for a value
    pub fn new() -> Self {
        let now = Instant::now();
        ValueMetadata {
            expires_at: None,
            created_at: now,
            last_accessed: now,
            encoding: StringEncoding::Raw,
        }
    }
    
    /// Create metadata with expiration time
    pub fn with_expiration(expires_in: Duration) -> Self {
        let now = Instant::now();
        ValueMetadata {
            expires_at: Some(now + expires_in),
            created_at: now,
            last_accessed: now,
            encoding: StringEncoding::Raw,
        }
    }
    
    /// Check if this value has expired
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .map(|expires_at| Instant::now() > expires_at)
            .unwrap_or(false)
    }
    
    /// Update last access time
    pub fn touch(&mut self) {
        self.last_accessed = Instant::now();
    }
    
    /// Set expiration time
    pub fn set_expiration(&mut self, expires_in: Duration) {
        self.expires_at = Some(Instant::now() + expires_in);
    }
    
    /// Clear expiration
    pub fn clear_expiration(&mut self) {
        self.expires_at = None;
    }
}

impl StoredValue {
    /// Create a new stored value
    pub fn new(value: Value) -> Self {
        StoredValue {
            value,
            metadata: ValueMetadata::new(),
        }
    }
    
    /// Create a stored value with expiration
    pub fn with_expiration(value: Value, expires_in: Duration) -> Self {
        StoredValue {
            value,
            metadata: ValueMetadata::with_expiration(expires_in),
        }
    }
    
    /// Check if this stored value has expired
    pub fn is_expired(&self) -> bool {
        self.metadata.is_expired()
    }
    
    /// Touch this value (update access time)
    pub fn touch(&mut self) {
        self.metadata.touch();
    }
}

impl Default for ValueMetadata {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for Value {
    fn default() -> Self {
        Value::SortedSet(Arc::new(SkipList::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_value_types() {
        let string_val = Value::string("hello");
        assert_eq!(string_val.value_type(), ValueType::String);
        
        let int_val = Value::integer(42);
        assert_eq!(int_val.as_integer(), Some(42));
    }
    
    #[test]
    fn test_expiration() {
        let mut stored = StoredValue::with_expiration(
            Value::string("test"), 
            Duration::from_millis(1)
        );
        
        assert!(!stored.is_expired());
        
        std::thread::sleep(Duration::from_millis(5));
        assert!(stored.is_expired());
    }
    
    #[test]
    fn test_touch() {
        let mut stored = StoredValue::new(Value::string("test"));
        let initial_access = stored.metadata.last_accessed;
        
        std::thread::sleep(Duration::from_millis(1));
        stored.touch();
        
        assert!(stored.metadata.last_accessed > initial_access);
    }
}