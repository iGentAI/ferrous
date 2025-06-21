//! RESP (REdis Serialization Protocol) implementation
//! 
//! This module provides parsing and serialization for RESP2 and RESP3 protocols,
//! ensuring 100% compatibility with Redis clients.

pub mod resp;
pub mod parser;
pub mod serializer;

pub use resp::{RespFrame, RespValue};
pub use parser::RespParser;
pub use serializer::RespSerializer;

// Re-export commonly used items
pub use parser::parse_resp_frame;
pub use serializer::serialize_resp_frame;

/// Extract a string from a RESP frame
pub fn extract_string(frame: &RespFrame) -> crate::error::Result<String> {
    match frame {
        RespFrame::SimpleString(data) => {
            String::from_utf8(data.to_vec())
                .map_err(|_| crate::error::FerrousError::Protocol("Invalid UTF-8 in simple string".to_string()))
        },
        RespFrame::BulkString(Some(data)) => {
            String::from_utf8(data.to_vec())
                .map_err(|_| crate::error::FerrousError::Protocol("Invalid UTF-8 in bulk string".to_string()))
        },
        _ => Err(crate::error::FerrousError::Protocol("Expected string".to_string())),
    }
}

/// Extract raw bytes from a RESP frame
pub fn extract_bytes(frame: &RespFrame) -> crate::error::Result<Vec<u8>> {
    match frame {
        RespFrame::SimpleString(data) => Ok(data.to_vec()),
        RespFrame::BulkString(Some(data)) => Ok(data.to_vec()),
        _ => Err(crate::error::FerrousError::Protocol("Expected string".to_string())),
    }
}

/// Extract an integer from a RESP frame
pub fn extract_integer(frame: &RespFrame) -> crate::error::Result<i64> {
    match frame {
        RespFrame::Integer(value) => Ok(*value),
        RespFrame::BulkString(Some(data)) => {
            let s = String::from_utf8(data.to_vec())
                .map_err(|_| crate::error::FerrousError::Protocol("Invalid UTF-8 in bulk string".to_string()))?;
            
            s.parse::<i64>()
                .map_err(|_| crate::error::FerrousError::Protocol("Expected integer".to_string()))
        },
        _ => Err(crate::error::FerrousError::Protocol("Expected integer".to_string())),
    }
}