//! RESP protocol parser implementation
//! 
//! Provides efficient parsing of RESP2 and RESP3 protocol frames with zero-copy
//! optimizations where possible.

use std::sync::Arc;
use crate::error::{FerrousError, Result};
use super::resp::RespFrame;

/// Parser state for incremental RESP parsing
pub struct RespParser {
    buffer: Vec<u8>,
    position: usize,
}

impl RespParser {
    /// Create a new parser
    pub fn new() -> Self {
        RespParser {
            buffer: Vec::with_capacity(4096),
            position: 0,
        }
    }
    
    /// Feed data into the parser
    pub fn feed(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }
    
    /// Try to parse a complete frame from the buffer
    pub fn parse(&mut self) -> Result<Option<RespFrame>> {
        if self.position >= self.buffer.len() {
            return Ok(None);
        }
        
        // Special handling for raw protocol (e.g., redis-benchmark sometimes sends raw "PING")
        if self.position + 4 <= self.buffer.len() && 
           &self.buffer[self.position..self.position+4] == b"PING" {
            // Found a raw PING command, consume it
            self.position += 4;
            
            // Skip trailing whitespace or newlines if present
            while self.position < self.buffer.len() && 
                  (self.buffer[self.position] == b' ' || 
                   self.buffer[self.position] == b'\r' || 
                   self.buffer[self.position] == b'\n') {
                self.position += 1;
            }
            
            // Return a properly formatted PING command
            return Ok(Some(RespFrame::Array(Some(vec![
                RespFrame::BulkString(Some(Arc::new(b"PING".to_vec())))
            ]))));
        }
        
        // Handle normal RESP protocol
        match parse_frame(&self.buffer[self.position..])? {
            Some((frame, consumed)) => {
                self.position += consumed;
                // If we've consumed more than half the buffer, compact it
                if self.position > self.buffer.len() / 2 {
                    self.buffer.drain(..self.position);
                    self.position = 0;
                }
                Ok(Some(frame))
            }
            None => Ok(None),
        }
    }
    
    /// Clear the parser buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.position = 0;
    }
}

/// Parse a RESP frame from a byte slice
/// Returns Some((frame, bytes_consumed)) if a complete frame is found
pub fn parse_resp_frame(data: &[u8]) -> Result<Option<(RespFrame, usize)>> {
    parse_frame(data)
}

/// Internal frame parser
fn parse_frame(data: &[u8]) -> Result<Option<(RespFrame, usize)>> {
    if data.is_empty() {
        return Ok(None);
    }
    
    match data[0] {
        b'+' => parse_simple_string(data),
        b'-' => parse_error(data),
        b':' => parse_integer(data),
        b'$' => parse_bulk_string(data),
        b'*' => parse_array(data),
        b'_' => parse_null(data),
        b'#' => parse_boolean(data),
        b',' => parse_double(data),
        b'%' => parse_map(data),
        b'~' => parse_set(data),
        _ => Err(FerrousError::Protocol(format!(
            "Invalid RESP type byte: {}", data[0] as char
        ))),
    }
}

/// Parse a simple string: +OK\r\n
fn parse_simple_string(data: &[u8]) -> Result<Option<(RespFrame, usize)>> {
    parse_line(data, 1).map(|opt| {
        opt.map(|(line, consumed)| {
            (RespFrame::SimpleString(Arc::new(line.to_vec())), consumed)
        })
    })
}

/// Parse an error: -Error message\r\n
fn parse_error(data: &[u8]) -> Result<Option<(RespFrame, usize)>> {
    parse_line(data, 1).map(|opt| {
        opt.map(|(line, consumed)| {
            (RespFrame::Error(Arc::new(line.to_vec())), consumed)
        })
    })
}

/// Parse an integer: :1000\r\n
fn parse_integer(data: &[u8]) -> Result<Option<(RespFrame, usize)>> {
    parse_line(data, 1).and_then(|opt| {
        opt.map(|(line, consumed)| {
            let s = std::str::from_utf8(line)
                .map_err(|_| FerrousError::Protocol("Invalid UTF-8 in integer".into()))?;
            let n = s.parse::<i64>()
                .map_err(|_| FerrousError::Protocol("Invalid integer format".into()))?;
            Ok((RespFrame::Integer(n), consumed))
        }).transpose()
    })
}

/// Parse a bulk string: $6\r\nfoobar\r\n or $-1\r\n (null)
fn parse_bulk_string(data: &[u8]) -> Result<Option<(RespFrame, usize)>> {
    let (len_line, header_consumed) = match parse_line(data, 1)? {
        Some(v) => v,
        None => return Ok(None),
    };
    
    let len_str = std::str::from_utf8(len_line)
        .map_err(|_| FerrousError::Protocol("Invalid UTF-8 in bulk length".into()))?;
    let len = len_str.parse::<i64>()
        .map_err(|_| FerrousError::Protocol("Invalid bulk string length".into()))?;
    
    if len == -1 {
        return Ok(Some((RespFrame::BulkString(None), header_consumed)));
    }
    
    if len < 0 {
        return Err(FerrousError::Protocol("Invalid negative bulk string length".into()));
    }
    
    let len = len as usize;
    let total_needed = header_consumed + len + 2; // +2 for \r\n
    
    if data.len() < total_needed {
        return Ok(None); // Need more data
    }
    
    // Verify trailing \r\n
    if data[header_consumed + len] != b'\r' || data[header_consumed + len + 1] != b'\n' {
        return Err(FerrousError::Protocol("Missing CRLF after bulk string".into()));
    }
    
    let content = data[header_consumed..header_consumed + len].to_vec();
    Ok(Some((RespFrame::BulkString(Some(Arc::new(content))), total_needed)))
}

/// Parse an array: *2\r\n$3\r\nfoo\r\n$3\r\nbar\r\n
fn parse_array(data: &[u8]) -> Result<Option<(RespFrame, usize)>> {
    let (len_line, header_consumed) = match parse_line(data, 1)? {
        Some(v) => v,
        None => return Ok(None),
    };
    
    let len_str = std::str::from_utf8(len_line)
        .map_err(|_| FerrousError::Protocol("Invalid UTF-8 in array length".into()))?;
    let len = len_str.parse::<i64>()
        .map_err(|_| FerrousError::Protocol("Invalid array length".into()))?;
    
    if len == -1 {
        return Ok(Some((RespFrame::Array(None), header_consumed)));
    }
    
    if len < 0 {
        return Err(FerrousError::Protocol("Invalid negative array length".into()));
    }
    
    let len = len as usize;
    let mut elements = Vec::with_capacity(len);
    let mut total_consumed = header_consumed;
    
    for _ in 0..len {
        match parse_frame(&data[total_consumed..])? {
            Some((frame, consumed)) => {
                elements.push(frame);
                total_consumed += consumed;
            }
            None => return Ok(None), // Need more data
        }
    }
    
    Ok(Some((RespFrame::Array(Some(elements)), total_consumed)))
}

/// Parse null (RESP3): _\r\n
fn parse_null(data: &[u8]) -> Result<Option<(RespFrame, usize)>> {
    if data.len() < 3 {
        return Ok(None);
    }
    if data[1] == b'\r' && data[2] == b'\n' {
        Ok(Some((RespFrame::Null, 3)))
    } else {
        Err(FerrousError::Protocol("Invalid null format".into()))
    }
}

/// Parse boolean (RESP3): #t\r\n or #f\r\n
fn parse_boolean(data: &[u8]) -> Result<Option<(RespFrame, usize)>> {
    if data.len() < 4 {
        return Ok(None);
    }
    match (data[1], data[2], data[3]) {
        (b't', b'\r', b'\n') => Ok(Some((RespFrame::Boolean(true), 4))),
        (b'f', b'\r', b'\n') => Ok(Some((RespFrame::Boolean(false), 4))),
        _ => Err(FerrousError::Protocol("Invalid boolean format".into())),
    }
}

/// Parse double (RESP3): ,1.23\r\n
fn parse_double(data: &[u8]) -> Result<Option<(RespFrame, usize)>> {
    parse_line(data, 1).and_then(|opt| {
        opt.map(|(line, consumed)| {
            let s = std::str::from_utf8(line)
                .map_err(|_| FerrousError::Protocol("Invalid UTF-8 in double".into()))?;
            let n = s.parse::<f64>()
                .map_err(|_| FerrousError::Protocol("Invalid double format".into()))?;
            Ok((RespFrame::Double(n), consumed))
        }).transpose()
    })
}

/// Parse map (RESP3): %2\r\n+key1\r\n:1\r\n+key2\r\n:2\r\n
fn parse_map(data: &[u8]) -> Result<Option<(RespFrame, usize)>> {
    let (len_line, header_consumed) = match parse_line(data, 1)? {
        Some(v) => v,
        None => return Ok(None),
    };
    
    let len_str = std::str::from_utf8(len_line)
        .map_err(|_| FerrousError::Protocol("Invalid UTF-8 in map length".into()))?;
    let len = len_str.parse::<usize>()
        .map_err(|_| FerrousError::Protocol("Invalid map length".into()))?;
    
    let mut pairs = Vec::with_capacity(len);
    let mut total_consumed = header_consumed;
    
    for _ in 0..len {
        // Parse key
        let key = match parse_frame(&data[total_consumed..])? {
            Some((frame, consumed)) => {
                total_consumed += consumed;
                frame
            }
            None => return Ok(None),
        };
        
        // Parse value
        let value = match parse_frame(&data[total_consumed..])? {
            Some((frame, consumed)) => {
                total_consumed += consumed;
                frame
            }
            None => return Ok(None),
        };
        
        pairs.push((key, value));
    }
    
    Ok(Some((RespFrame::Map(pairs), total_consumed)))
}

/// Parse set (RESP3): ~2\r\n+elem1\r\n+elem2\r\n
fn parse_set(data: &[u8]) -> Result<Option<(RespFrame, usize)>> {
    let (len_line, header_consumed) = match parse_line(data, 1)? {
        Some(v) => v,
        None => return Ok(None),
    };
    
    let len_str = std::str::from_utf8(len_line)
        .map_err(|_| FerrousError::Protocol("Invalid UTF-8 in set length".into()))?;
    let len = len_str.parse::<usize>()
        .map_err(|_| FerrousError::Protocol("Invalid set length".into()))?;
    
    let mut elements = Vec::with_capacity(len);
    let mut total_consumed = header_consumed;
    
    for _ in 0..len {
        match parse_frame(&data[total_consumed..])? {
            Some((frame, consumed)) => {
                elements.push(frame);
                total_consumed += consumed;
            }
            None => return Ok(None),
        }
    }
    
    Ok(Some((RespFrame::Set(elements), total_consumed)))
}

/// Parse a line ending with \r\n
fn parse_line(data: &[u8], skip_prefix: usize) -> Result<Option<(&[u8], usize)>> {
    if data.len() < skip_prefix + 2 {
        return Ok(None);
    }
    
    for i in skip_prefix..data.len() - 1 {
        if data[i] == b'\r' && data[i + 1] == b'\n' {
            return Ok(Some((&data[skip_prefix..i], i + 2)));
        }
    }
    
    Ok(None) // Need more data
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_simple_string() {
        let data = b"+OK\r\n";
        let result = parse_resp_frame(data).unwrap();
        assert!(matches!(result, Some((RespFrame::SimpleString(_), 5))));
    }
    
    #[test]
    fn test_parse_error() {
        let data = b"-Error message\r\n";
        let result = parse_resp_frame(data).unwrap();
        assert!(matches!(result, Some((RespFrame::Error(_), 16))));
    }
    
    #[test]
    fn test_parse_integer() {
        let data = b":1000\r\n";
        let result = parse_resp_frame(data).unwrap();
        assert!(matches!(result, Some((RespFrame::Integer(1000), 7))));
        
        let data = b":-42\r\n";
        let result = parse_resp_frame(data).unwrap();
        assert!(matches!(result, Some((RespFrame::Integer(-42), 6))));
    }
    
    #[test]
    fn test_parse_bulk_string() {
        let data = b"$6\r\nfoobar\r\n";
        let result = parse_resp_frame(data).unwrap();
        assert!(matches!(result, Some((RespFrame::BulkString(Some(_)), 13))));
        
        let data = b"$-1\r\n";
        let result = parse_resp_frame(data).unwrap();
        assert!(matches!(result, Some((RespFrame::BulkString(None), 5))));
    }
    
    #[test]
    fn test_parse_array() {
        let data = b"*2\r\n$3\r\nfoo\r\n$3\r\nbar\r\n";
        let result = parse_resp_frame(data).unwrap();
        assert!(matches!(result, Some((RespFrame::Array(Some(arr)), 23)) if arr.len() == 2));
        
        let data = b"*-1\r\n";
        let result = parse_resp_frame(data).unwrap();
        assert!(matches!(result, Some((RespFrame::Array(None), 5))));
    }
    
    #[test]
    fn test_parse_null() {
        let data = b"_\r\n";
        let result = parse_resp_frame(data).unwrap();
        assert!(matches!(result, Some((RespFrame::Null, 3))));
    }
    
    #[test]
    fn test_parse_boolean() {
        let data = b"#t\r\n";
        let result = parse_resp_frame(data).unwrap();
        assert!(matches!(result, Some((RespFrame::Boolean(true), 4))));
        
        let data = b"#f\r\n";
        let result = parse_resp_frame(data).unwrap();
        assert!(matches!(result, Some((RespFrame::Boolean(false), 4))));
    }
    
    #[test]
    fn test_incremental_parsing() {
        let mut parser = RespParser::new();
        
        // Feed partial data
        parser.feed(b"*2\r\n$3\r\n");
        assert!(parser.parse().unwrap().is_none());
        
        // Feed more data
        parser.feed(b"foo\r\n$3\r\nbar\r\n");
        let frame = parser.parse().unwrap().unwrap();
        assert!(matches!(frame, RespFrame::Array(Some(arr)) if arr.len() == 2));
    }
}