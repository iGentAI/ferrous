//! Replication backlog for partial resynchronization

use std::sync::Mutex;
use std::collections::VecDeque;
use crate::error::{FerrousError, Result};
use crate::protocol::{RespFrame, serialize_resp_frame};

/// Circular buffer for replication backlog
pub struct ReplicationBacklog {
    /// Maximum size of the backlog
    max_size: usize,
    
    /// Backlog buffer
    buffer: Mutex<VecDeque<u8>>,
    
    /// Start offset of the backlog
    start_offset: Mutex<u64>,
    
    /// Current size
    current_size: Mutex<usize>,
}

impl ReplicationBacklog {
    /// Create a new replication backlog
    pub fn new(max_size: usize) -> Self {
        ReplicationBacklog {
            max_size,
            buffer: Mutex::new(VecDeque::with_capacity(max_size)),
            start_offset: Mutex::new(0),
            current_size: Mutex::new(0),
        }
    }
    
    /// Append a command to the backlog
    pub fn append_command(&self, cmd: &RespFrame) -> Result<()> {
        let mut serialized = Vec::new();
        serialize_resp_frame(cmd, &mut serialized)?;
        
        let mut buffer = self.buffer.lock().unwrap();
        let mut current_size = self.current_size.lock().unwrap();
        let mut start_offset = self.start_offset.lock().unwrap();
        
        // Add to buffer
        for byte in serialized {
            if *current_size >= self.max_size {
                // Remove oldest byte
                if buffer.pop_front().is_some() {
                    *start_offset += 1;
                }
            } else {
                *current_size += 1;
            }
            buffer.push_back(byte);
        }
        
        Ok(())
    }
    
    /// Get data from a specific offset
    pub fn get_data_from_offset(&self, offset: u64) -> Result<Vec<u8>> {
        let buffer = self.buffer.lock().unwrap();
        let start_offset = self.start_offset.lock().unwrap();
        let current_size = self.current_size.lock().unwrap();
        
        // Check if offset is within our range
        let end_offset = *start_offset + *current_size as u64;
        
        if offset < *start_offset || offset >= end_offset {
            return Err(FerrousError::Command(
                crate::error::CommandError::Generic("offset out of range".into())
            ));
        }
        
        // Calculate position in buffer
        let pos = (offset - *start_offset) as usize;
        
        // Copy data from position to end
        let data: Vec<u8> = buffer.iter()
            .skip(pos)
            .cloned()
            .collect();
        
        Ok(data)
    }
    
    /// Clear the backlog
    pub fn clear(&self) {
        let mut buffer = self.buffer.lock().unwrap();
        let mut current_size = self.current_size.lock().unwrap();
        
        buffer.clear();
        *current_size = 0;
    }
    
    /// Get current backlog size
    pub fn size(&self) -> usize {
        *self.current_size.lock().unwrap()
    }
    
    /// Get start offset
    pub fn start_offset(&self) -> u64 {
        *self.start_offset.lock().unwrap()
    }
    
    /// Get end offset
    pub fn end_offset(&self) -> u64 {
        let start = self.start_offset.lock().unwrap();
        let size = self.current_size.lock().unwrap();
        *start + *size as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_backlog_basic() {
        let backlog = ReplicationBacklog::new(100);
        assert_eq!(backlog.size(), 0);
        
        // Add a command
        let cmd = RespFrame::array(vec![
            RespFrame::bulk_string(b"SET"),
            RespFrame::bulk_string(b"key"),
            RespFrame::bulk_string(b"value"),
        ]);
        
        backlog.append_command(&cmd).unwrap();
        assert!(backlog.size() > 0);
    }
    
    #[test]
    fn test_backlog_circular() {
        let backlog = ReplicationBacklog::new(10);
        
        // Fill backlog beyond capacity
        for i in 0..20 {
            let cmd = RespFrame::bulk_string(format!("{}", i).as_bytes());
            backlog.append_command(&cmd).unwrap();
        }
        
        // Size should be capped at max_size
        assert!(backlog.size() <= 10);
    }
}