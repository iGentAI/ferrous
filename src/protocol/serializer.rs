//! RESP protocol serializer implementation
//! 
//! Provides efficient serialization of RESP frames to byte buffers for network
//! transmission.

use std::io::Write;
use crate::error::Result;
use super::resp::RespFrame;

/// Serialize a RESP frame to a writer
pub fn serialize_resp_frame<W: Write>(frame: &RespFrame, writer: &mut W) -> Result<()> {
    match frame {
        RespFrame::SimpleString(bytes) => {
            writer.write_all(b"+")?;
            writer.write_all(bytes)?;
            writer.write_all(b"\r\n")?;
        }
        
        RespFrame::Error(bytes) => {
            writer.write_all(b"-")?;
            writer.write_all(bytes)?;
            writer.write_all(b"\r\n")?;
        }
        
        RespFrame::Integer(n) => {
            writer.write_all(b":")?;
            writer.write_all(n.to_string().as_bytes())?;
            writer.write_all(b"\r\n")?;
        }
        
        RespFrame::BulkString(opt) => {
            match opt {
                Some(bytes) => {
                    writer.write_all(b"$")?;
                    writer.write_all(bytes.len().to_string().as_bytes())?;
                    writer.write_all(b"\r\n")?;
                    writer.write_all(bytes)?;
                    writer.write_all(b"\r\n")?;
                }
                None => {
                    writer.write_all(b"$-1\r\n")?;
                }
            }
        }
        
        RespFrame::Array(opt) => {
            match opt {
                Some(frames) => {
                    writer.write_all(b"*")?;
                    writer.write_all(frames.len().to_string().as_bytes())?;
                    writer.write_all(b"\r\n")?;
                    for frame in frames {
                        serialize_resp_frame(frame, writer)?;
                    }
                }
                None => {
                    writer.write_all(b"*-1\r\n")?;
                }
            }
        }
        
        RespFrame::Null => {
            writer.write_all(b"_\r\n")?;
        }
        
        RespFrame::Boolean(b) => {
            if *b {
                writer.write_all(b"#t\r\n")?;
            } else {
                writer.write_all(b"#f\r\n")?;
            }
        }
        
        RespFrame::Double(f) => {
            writer.write_all(b",")?;
            writer.write_all(f.to_string().as_bytes())?;
            writer.write_all(b"\r\n")?;
        }
        
        RespFrame::Map(pairs) => {
            writer.write_all(b"%")?;
            writer.write_all(pairs.len().to_string().as_bytes())?;
            writer.write_all(b"\r\n")?;
            for (key, value) in pairs {
                serialize_resp_frame(key, writer)?;
                serialize_resp_frame(value, writer)?;
            }
        }
        
        RespFrame::Set(elements) => {
            writer.write_all(b"~")?;
            writer.write_all(elements.len().to_string().as_bytes())?;
            writer.write_all(b"\r\n")?;
            for element in elements {
                serialize_resp_frame(element, writer)?;
            }
        }
    }
    
    Ok(())
}

/// Serialize a RESP frame to a byte vector
pub fn serialize_to_vec(frame: &RespFrame) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    serialize_resp_frame(frame, &mut buf)?;
    Ok(buf)
}

/// Efficient serializer for multiple frames
pub struct RespSerializer {
    buffer: Vec<u8>,
}

impl RespSerializer {
    /// Create a new serializer
    pub fn new() -> Self {
        RespSerializer {
            buffer: Vec::with_capacity(4096),
        }
    }
    
    /// Add a frame to the buffer
    pub fn add(&mut self, frame: &RespFrame) -> Result<()> {
        serialize_resp_frame(frame, &mut self.buffer)
    }
    
    /// Take the buffer and reset
    pub fn take(&mut self) -> Vec<u8> {
        std::mem::replace(&mut self.buffer, Vec::with_capacity(4096))
    }
    
    /// Get a reference to the buffer
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }
    
    /// Clear the buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

/// Helper to create common responses
pub struct ResponseBuilder;

impl ResponseBuilder {
    /// Create an OK response
    pub fn ok() -> RespFrame {
        RespFrame::ok()
    }
    
    /// Create an error response
    pub fn error(msg: impl Into<String>) -> RespFrame {
        let msg = msg.into();
        RespFrame::Error(std::sync::Arc::new(msg.into_bytes()))
    }
    
    /// Create a null response
    pub fn null() -> RespFrame {
        RespFrame::null_bulk()
    }
    
    /// Create an integer response
    pub fn integer(n: i64) -> RespFrame {
        RespFrame::Integer(n)
    }
    
    /// Create a bulk string response
    pub fn bulk_string(s: impl Into<Vec<u8>>) -> RespFrame {
        RespFrame::BulkString(Some(std::sync::Arc::new(s.into())))
    }
    
    /// Create an array response
    pub fn array(frames: Vec<RespFrame>) -> RespFrame {
        RespFrame::Array(Some(frames))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_serialize_simple_string() {
        let frame = RespFrame::ok();
        let result = serialize_to_vec(&frame).unwrap();
        assert_eq!(result, b"+OK\r\n");
    }
    
    #[test]
    fn test_serialize_error() {
        let frame = RespFrame::error("ERR test");
        let result = serialize_to_vec(&frame).unwrap();
        assert_eq!(result, b"-ERR test\r\n");
    }
    
    #[test]
    fn test_serialize_integer() {
        let frame = RespFrame::Integer(42);
        let result = serialize_to_vec(&frame).unwrap();
        assert_eq!(result, b":42\r\n");
        
        let frame = RespFrame::Integer(-100);
        let result = serialize_to_vec(&frame).unwrap();
        assert_eq!(result, b":-100\r\n");
    }
    
    #[test]
    fn test_serialize_bulk_string() {
        let frame = RespFrame::from_string("hello");
        let result = serialize_to_vec(&frame).unwrap();
        assert_eq!(result, b"$5\r\nhello\r\n");
        
        let frame = RespFrame::null_bulk();
        let result = serialize_to_vec(&frame).unwrap();
        assert_eq!(result, b"$-1\r\n");
    }
    
    #[test]
    fn test_serialize_array() {
        let frame = RespFrame::Array(Some(vec![
            RespFrame::from_string("foo"),
            RespFrame::from_string("bar"),
        ]));
        let result = serialize_to_vec(&frame).unwrap();
        assert_eq!(result, b"*2\r\n$3\r\nfoo\r\n$3\r\nbar\r\n");
    }
    
    #[test]
    fn test_resp_serializer() {
        let mut serializer = RespSerializer::new();
        serializer.add(&RespFrame::ok()).unwrap();
        serializer.add(&RespFrame::Integer(42)).unwrap();
        
        let buffer = serializer.take();
        assert_eq!(buffer, b"+OK\r\n:42\r\n");
    }
}