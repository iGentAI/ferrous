//! RESP data types and frame definitions
//! 
//! Supports both RESP2 and RESP3 protocols for full Redis compatibility.

use std::sync::Arc;

/// Type alias for byte strings used throughout the protocol
pub type Bytes = Arc<Vec<u8>>;

/// RESP protocol frame types
#[derive(Debug, Clone, PartialEq)]
pub enum RespFrame {
    /// Simple string: +OK\r\n
    SimpleString(Bytes),
    
    /// Error: -Error message\r\n
    Error(Bytes),
    
    /// Integer: :1000\r\n
    Integer(i64),
    
    /// Bulk string: $6\r\nfoobar\r\n or $-1\r\n (null)
    BulkString(Option<Bytes>),
    
    /// Array: *2\r\n$3\r\nfoo\r\n$3\r\nbar\r\n or *-1\r\n (null)
    Array(Option<Vec<RespFrame>>),
    
    // RESP3 additions
    /// Null value: _\r\n
    Null,
    
    /// Boolean: #t\r\n or #f\r\n
    Boolean(bool),
    
    /// Double: ,1.23\r\n or ,inf\r\n
    Double(f64),
    
    /// Map: %2\r\n+first\r\n:1\r\n+second\r\n:2\r\n
    Map(Vec<(RespFrame, RespFrame)>),
    
    /// Set: ~2\r\n+first\r\n+second\r\n
    Set(Vec<RespFrame>),
}

/// Simplified value type for internal use
#[derive(Debug, Clone, PartialEq)]
pub enum RespValue {
    /// String value (simple or bulk)
    String(String),
    
    /// Integer value
    Integer(i64),
    
    /// Error message
    Error(String),
    
    /// Array of values
    Array(Vec<RespValue>),
    
    /// Null/nil value
    Null,
    
    /// Boolean (RESP3)
    Boolean(bool),
    
    /// Floating point (RESP3)
    Double(f64),
}

impl RespFrame {
    /// Create a simple string response
    pub fn ok() -> Self {
        RespFrame::SimpleString(Arc::new(b"OK".to_vec()))
    }
    
    /// Create a simple string response
    pub fn simple_string(s: impl Into<Vec<u8>>) -> Self {
        RespFrame::SimpleString(Arc::new(s.into()))
    }
    
    /// Check if this frame is an error
    pub fn is_error(&self) -> bool {
        matches!(self, RespFrame::Error(_))
    }
    
    /// Create an error response
    pub fn error(msg: impl Into<Vec<u8>>) -> Self {
        RespFrame::Error(Arc::new(msg.into()))
    }
    
    /// Create a null bulk string
    pub fn null_bulk() -> Self {
        RespFrame::BulkString(None)
    }
    
    /// Create a null array
    pub fn null_array() -> Self {
        RespFrame::Array(None)
    }
    
    /// Convert bytes to a frame
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        RespFrame::BulkString(Some(Arc::new(bytes)))
    }
    
    /// Convert a string to a bulk string frame
    pub fn from_string(s: impl Into<String>) -> Self {
        let s = s.into();
        RespFrame::BulkString(Some(Arc::new(s.into_bytes())))
    }
    
    /// Create a bulk string from bytes
    pub fn bulk_string(bytes: impl AsRef<[u8]>) -> Self {
        RespFrame::BulkString(Some(Arc::new(bytes.as_ref().to_vec())))
    }
    
    /// Create an array of frames
    pub fn array(frames: Vec<RespFrame>) -> Self {
        RespFrame::Array(Some(frames))
    }
    
    /// Check if this frame represents a null/nil value
    pub fn is_null(&self) -> bool {
        matches!(self, 
            RespFrame::Null | 
            RespFrame::BulkString(None) | 
            RespFrame::Array(None)
        )
    }
}

impl From<String> for RespFrame {
    fn from(s: String) -> Self {
        RespFrame::from_string(s)
    }
}

impl From<&str> for RespFrame {
    fn from(s: &str) -> Self {
        RespFrame::from_string(s)
    }
}

impl From<i64> for RespFrame {
    fn from(n: i64) -> Self {
        RespFrame::Integer(n)
    }
}

impl From<Vec<RespFrame>> for RespFrame {
    fn from(frames: Vec<RespFrame>) -> Self {
        RespFrame::Array(Some(frames))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_resp_frame_creation() {
        let ok = RespFrame::ok();
        assert!(matches!(ok, RespFrame::SimpleString(_)));
        
        let err = RespFrame::error("ERR test");
        assert!(matches!(err, RespFrame::Error(_)));
        
        let null = RespFrame::null_bulk();
        assert!(null.is_null());
    }
    
    #[test]
    fn test_resp_frame_conversions() {
        let frame: RespFrame = "hello".into();
        assert!(matches!(frame, RespFrame::BulkString(Some(_))));
        
        let frame: RespFrame = 42i64.into();
        assert!(matches!(frame, RespFrame::Integer(42)));
    }
}