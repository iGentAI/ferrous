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