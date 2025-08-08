//! SLOWLOG command implementation
//! 
//! The slowlog is used to track commands that exceed a configurable execution time threshold.
//! It helps identify performance bottlenecks in production systems.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::sync::atomic::{AtomicU64, AtomicI64, Ordering};
use crate::error::Result;
use crate::protocol::RespFrame;

/// Maximum length of the slowlog (can be configured)
const DEFAULT_SLOWLOG_MAX_LEN: usize = 128;

/// Default threshold in microseconds (10ms)
const DEFAULT_SLOWLOG_THRESHOLD_MICROS: i64 = 10_000;

/// A single slowlog entry
#[derive(Debug, Clone)]
pub struct SlowlogEntry {
    /// Unique ID for this entry
    pub id: u64,
    
    /// Unix timestamp when the command started
    pub timestamp: u64,
    
    /// Execution time in microseconds
    pub duration_micros: u64,
    
    /// Command and arguments (limited for memory)
    pub command: Vec<Vec<u8>>,
    
    /// Client IP and port
    pub client_addr: String,
    
    /// Client name if set
    pub client_name: Option<String>,
}

/// The slowlog system
pub struct Slowlog {
    /// Entries stored in order (newest first)
    entries: Arc<Mutex<VecDeque<SlowlogEntry>>>,
    
    /// Maximum number of entries to keep
    max_len: AtomicU64,
    
    /// Threshold in microseconds
    threshold_micros: AtomicI64,
    
    /// ID generator for entries
    next_id: AtomicU64,
}

impl Default for Slowlog {
    fn default() -> Self {
        Self::new()
    }
}

impl Slowlog {
    /// Create a new slowlog
    pub fn new() -> Self {
        Slowlog {
            entries: Arc::new(Mutex::new(VecDeque::new())),
            max_len: AtomicU64::new(DEFAULT_SLOWLOG_MAX_LEN as u64),
            threshold_micros: AtomicI64::new(DEFAULT_SLOWLOG_THRESHOLD_MICROS),
            next_id: AtomicU64::new(1),
        }
    }
    
    /// Add an entry to the slowlog if it exceeds the threshold
    pub fn add_if_slow(&self, duration: Duration, command_parts: &[RespFrame], client_addr: &str, client_name: Option<&str>) {
        let duration_micros = duration.as_micros() as u64;
        let threshold = self.threshold_micros.load(Ordering::Relaxed);
        
        // Check if command is slow enough to log
        if threshold >= 0 && duration_micros >= threshold as u64 {
            self.add_entry(duration_micros, command_parts, client_addr, client_name);
        }
    }
    
    /// Add an entry to the slowlog
    fn add_entry(&self, duration_micros: u64, command_parts: &[RespFrame], client_addr: &str, client_name: Option<&str>) {
        // Convert command parts to bytes
        let mut command = Vec::new();
        for part in command_parts {
            match part {
                RespFrame::BulkString(Some(bytes)) => {
                    // Limit argument size to prevent memory bloat
                    let arg = if bytes.len() > 64 {
                        let mut truncated = bytes[..64].to_vec();
                        truncated.extend_from_slice(b"... (truncated)");
                        truncated
                    } else {
                        bytes.to_vec()
                    };
                    command.push(arg);
                }
                _ => {} // Skip non-bulk-string arguments
            }
            
            // Limit number of arguments stored
            if command.len() >= 10 {
                command.push(b"... (more arguments)".to_vec());
                break;
            }
        }
        
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let entry = SlowlogEntry {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            timestamp,
            duration_micros,
            command,
            client_addr: client_addr.to_string(),
            client_name: client_name.map(|s| s.to_string()),
        };
        
        let mut entries = self.entries.lock().unwrap();
        
        // Add to front (newest first)
        entries.push_front(entry);
        
        // Trim to max length
        let max_len = self.max_len.load(Ordering::Relaxed) as usize;
        while entries.len() > max_len {
            entries.pop_back();
        }
    }
    
    /// Get entries from the slowlog
    pub fn get_entries(&self, count: Option<usize>) -> Vec<SlowlogEntry> {
        let entries = self.entries.lock().unwrap();
        let limit = count.unwrap_or(entries.len());
        
        entries.iter()
            .take(limit)
            .cloned()
            .collect()
    }
    
    /// Get the number of entries in the slowlog
    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }
    
    /// Clear all entries from the slowlog
    pub fn reset(&self) {
        self.entries.lock().unwrap().clear();
    }
    
    /// Get current slowlog max length
    pub fn get_max_len(&self) -> u64 {
        self.max_len.load(Ordering::Relaxed)
    }
    
    /// Set slowlog max length
    pub fn set_max_len(&self, max_len: u64) {
        self.max_len.store(max_len, Ordering::Relaxed);
        
        // Trim existing entries if needed
        let mut entries = self.entries.lock().unwrap();
        while entries.len() > max_len as usize {
            entries.pop_back();
        }
    }
    
    /// Get current slowlog threshold in microseconds
    pub fn get_threshold_micros(&self) -> i64 {
        self.threshold_micros.load(Ordering::Relaxed)
    }
    
    /// Set slowlog threshold in microseconds
    pub fn set_threshold_micros(&self, threshold: i64) {
        self.threshold_micros.store(threshold, Ordering::Relaxed);
    }
}

/// Handle SLOWLOG GET command
pub fn handle_slowlog_get(slowlog: &Slowlog, parts: &[RespFrame]) -> Result<RespFrame> {
    // SLOWLOG GET [count]
    let count = if parts.len() >= 3 {
        match &parts[2] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<usize>() {
                    Ok(n) => Some(n),
                    Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid count format")),
        }
    } else {
        None
    };
    
    let entries = slowlog.get_entries(count);
    
    // Convert entries to RESP format
    let resp_entries: Vec<RespFrame> = entries.into_iter().map(|entry| {
        // Each entry is an array of:
        // 1. ID (integer)
        // 2. Timestamp (integer)
        // 3. Duration in microseconds (integer)
        // 4. Command array
        // 5. Client info (IP:port)
        // 6. Client name (if set)
        
        let mut entry_parts = vec![
            RespFrame::Integer(entry.id as i64),
            RespFrame::Integer(entry.timestamp as i64),
            RespFrame::Integer(entry.duration_micros as i64),
        ];
        
        // Command array
        let command_array = entry.command.into_iter()
            .map(|arg| RespFrame::BulkString(Some(Arc::new(arg))))
            .collect();
        entry_parts.push(RespFrame::Array(Some(command_array)));
        
        // Client address
        entry_parts.push(RespFrame::BulkString(Some(Arc::new(entry.client_addr.into_bytes()))));
        
        // Client name (empty string if not set)
        let client_name = entry.client_name.unwrap_or_default();
        entry_parts.push(RespFrame::BulkString(Some(Arc::new(client_name.into_bytes()))));
        
        RespFrame::Array(Some(entry_parts))
    }).collect();
    
    Ok(RespFrame::Array(Some(resp_entries)))
}

/// Handle SLOWLOG LEN command
pub fn handle_slowlog_len(slowlog: &Slowlog) -> Result<RespFrame> {
    Ok(RespFrame::Integer(slowlog.len() as i64))
}

/// Handle SLOWLOG RESET command
pub fn handle_slowlog_reset(slowlog: &Slowlog) -> Result<RespFrame> {
    slowlog.reset();
    Ok(RespFrame::ok())
}

/// Handle SLOWLOG command
pub fn handle_slowlog(slowlog: &Slowlog, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'slowlog' command"));
    }
    
    // Extract subcommand
    let subcommand = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_uppercase(),
        _ => return Ok(RespFrame::error("ERR invalid subcommand format")),
    };
    
    match subcommand.as_str() {
        "GET" => handle_slowlog_get(slowlog, parts),
        "LEN" => handle_slowlog_len(slowlog),
        "RESET" => handle_slowlog_reset(slowlog),
        _ => Ok(RespFrame::error("ERR Unknown subcommand or wrong number of arguments for SLOWLOG")),
    }
}