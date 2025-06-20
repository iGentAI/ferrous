//! MONITOR command implementation
//! 
//! The MONITOR command allows clients to see all commands processed by the server
//! in real-time. This is useful for debugging and understanding what commands
//! are being executed.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::error::Result;
use crate::protocol::RespFrame;

/// Manages connections that are monitoring command execution
#[derive(Debug)]
pub struct MonitorSubscribers {
    /// Set of connection IDs that are monitoring
    subscribers: Arc<Mutex<HashSet<u64>>>,
}

impl MonitorSubscribers {
    /// Create a new MonitorSubscribers instance
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(HashSet::new())),
        }
    }
    
    /// Add a connection to the monitor subscribers
    pub fn subscribe(&self, conn_id: u64) -> Result<()> {
        let mut subs = self.subscribers.lock().unwrap();
        subs.insert(conn_id);
        Ok(())
    }
    
    /// Remove a connection from the monitor subscribers
    pub fn unsubscribe(&self, conn_id: u64) -> Result<()> {
        let mut subs = self.subscribers.lock().unwrap();
        subs.remove(&conn_id);
        Ok(())
    }
    
    /// Check if a connection is monitoring
    pub fn is_monitoring(&self, conn_id: u64) -> bool {
        let subs = self.subscribers.lock().unwrap();
        subs.contains(&conn_id)
    }
    
    /// Get all monitoring connection IDs
    pub fn get_subscribers(&self) -> Vec<u64> {
        let subs = self.subscribers.lock().unwrap();
        subs.iter().cloned().collect()
    }
    
    /// Format a command for MONITOR output
    /// Format: timestamp [db clientaddr:port] "COMMAND" "arg1" "arg2" ...
    pub fn format_monitor_output(
        timestamp: SystemTime,
        db: usize,
        client_addr: &str,
        command_parts: &[RespFrame]
    ) -> RespFrame {
        // Get Unix timestamp with microseconds
        let since_epoch = timestamp.duration_since(UNIX_EPOCH).unwrap();
        let timestamp_str = format!(
            "{}.{}",
            since_epoch.as_secs(),
            since_epoch.subsec_micros()
        );
        
        // Build the output string
        let mut output = format!("{} [{}] {}", timestamp_str, db, client_addr);
        
        // Add each command part as a quoted string
        for part in command_parts {
            match part {
                RespFrame::BulkString(Some(bytes)) => {
                    // Escape the string for display
                    let escaped = escape_string(bytes);
                    output.push_str(&format!(" \"{}\"", escaped));
                }
                RespFrame::SimpleString(bytes) => {
                    let escaped = escape_string(bytes);
                    output.push_str(&format!(" \"{}\"", escaped));
                }
                RespFrame::Integer(n) => {
                    output.push_str(&format!(" \"{}\"", n));
                }
                _ => {
                    // For other types, use a placeholder
                    output.push_str(" \"<complex-value>\"");
                }
            }
        }
        
        // Return as a simple string (not bulk string) for MONITOR compatibility
        RespFrame::SimpleString(Arc::new(output.into_bytes()))
    }
}

/// Escape a byte string for display in MONITOR output
fn escape_string(bytes: &[u8]) -> String {
    let mut result = String::new();
    
    for &byte in bytes {
        match byte {
            b'\\' => result.push_str("\\\\"),
            b'"' => result.push_str("\\\""),
            b'\n' => result.push_str("\\n"),
            b'\r' => result.push_str("\\r"),
            b'\t' => result.push_str("\\t"),
            0x00..=0x1F | 0x7F => {
                // Control characters
                result.push_str(&format!("\\x{:02x}", byte));
            }
            _ => {
                // Try to interpret as UTF-8, fall back to hex if invalid
                if let Ok(s) = std::str::from_utf8(&[byte]) {
                    result.push_str(s);
                } else {
                    result.push_str(&format!("\\x{:02x}", byte));
                }
            }
        }
    }
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_monitor_subscribers() {
        let monitor = MonitorSubscribers::new();
        
        // Add subscribers
        monitor.subscribe(1).unwrap();
        monitor.subscribe(2).unwrap();
        
        // Check if monitoring
        assert!(monitor.is_monitoring(1));
        assert!(monitor.is_monitoring(2));
        assert!(!monitor.is_monitoring(3));
        
        // Get all subscribers
        let subs = monitor.get_subscribers();
        assert_eq!(subs.len(), 2);
        assert!(subs.contains(&1));
        assert!(subs.contains(&2));
        
        // Unsubscribe
        monitor.unsubscribe(1).unwrap();
        assert!(!monitor.is_monitoring(1));
        assert!(monitor.is_monitoring(2));
    }
    
    #[test]
    fn test_escape_string() {
        assert_eq!(escape_string(b"hello"), "hello");
        assert_eq!(escape_string(b"hello\nworld"), "hello\\nworld");
        assert_eq!(escape_string(b"quote\"test"), "quote\\\"test");
        assert_eq!(escape_string(b"back\\slash"), "back\\\\slash");
        assert_eq!(escape_string(b"\x00\x01\x02"), "\\x00\\x01\\x02");
    }
    
    #[test]
    fn test_format_monitor_output() {
        let timestamp = SystemTime::now();
        let db = 0;
        let client_addr = "127.0.0.1:12345";
        let command_parts = vec![
            RespFrame::BulkString(Some(Arc::new(b"SET".to_vec()))),
            RespFrame::BulkString(Some(Arc::new(b"key".to_vec()))),
            RespFrame::BulkString(Some(Arc::new(b"value".to_vec()))),
        ];
        
        let output = MonitorSubscribers::format_monitor_output(
            timestamp,
            db,
            client_addr,
            &command_parts
        );
        
        match output {
            RespFrame::SimpleString(bytes) => {
                let s = String::from_utf8_lossy(&bytes);
                // Check format: timestamp [db] addr "SET" "key" "value"
                assert!(s.contains("[0]"));
                assert!(s.contains("127.0.0.1:12345"));
                assert!(s.contains("\"SET\""));
                assert!(s.contains("\"key\""));
                assert!(s.contains("\"value\""));
            }
            _ => panic!("Expected SimpleString"),
        }
    }
}