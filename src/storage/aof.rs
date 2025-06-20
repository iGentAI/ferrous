//! AOF (Append Only File) persistence implementation
//! 
//! Provides append-only file logging for command persistence and replay.

use std::fs::{File, OpenOptions};
use std::io::{self, Write, BufWriter, BufReader, BufRead};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use std::thread;

use crate::error::{FerrousError, Result};
use crate::protocol::{RespFrame, serialize_resp_frame, RespParser};
use crate::storage::StorageEngine;

/// AOF persistence engine
pub struct AofEngine {
    /// Path to AOF file
    file_path: PathBuf,
    
    /// Active AOF file writer
    writer: Arc<Mutex<Option<BufWriter<File>>>>,
    
    /// Configuration
    config: AofConfig,
    
    /// Last fsync time for everysec mode
    last_fsync: Arc<Mutex<Instant>>,
    
    /// Is background rewrite in progress?
    rewrite_in_progress: Arc<Mutex<bool>>,
}

/// AOF configuration
#[derive(Debug, Clone)]
pub struct AofConfig {
    /// Enable AOF
    pub enabled: bool,
    
    /// Fsync policy
    pub fsync_policy: FsyncPolicy,
    
    /// AOF filename
    pub filename: String,
    
    /// Working directory
    pub dir: String,
    
    /// Rewrite trigger percentage
    pub auto_rewrite_percentage: u64,
    
    /// Rewrite trigger minimum size
    pub auto_rewrite_min_size: u64,
}

/// Fsync policies
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FsyncPolicy {
    /// Fsync after every write (safest, slowest)
    Always,
    /// Fsync every second (good compromise)
    EverySecond,
    /// Never fsync, let OS handle it (fastest, least safe)
    No,
}

impl Default for AofConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            fsync_policy: FsyncPolicy::EverySecond,
            filename: "appendonly.aof".to_string(),
            dir: "./".to_string(),
            auto_rewrite_percentage: 100,
            auto_rewrite_min_size: 64 * 1024 * 1024, // 64MB
        }
    }
}

impl AofEngine {
    /// Create a new AOF engine
    pub fn new(config: AofConfig) -> Self {
        let mut file_path = PathBuf::from(&config.dir);
        file_path.push(&config.filename);
        
        Self {
            file_path,
            writer: Arc::new(Mutex::new(None)),
            config,
            last_fsync: Arc::new(Mutex::new(Instant::now())),
            rewrite_in_progress: Arc::new(Mutex::new(false)),
        }
    }
    
    /// Initialize AOF (open file for appending)
    pub fn init(&self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }
        
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)?;
        
        let mut writer = self.writer.lock().unwrap();
        *writer = Some(BufWriter::new(file));
        
        Ok(())
    }
    
    /// Load commands from AOF file
    pub fn load(&self, storage: &Arc<StorageEngine>) -> Result<()> {
        if !self.file_path.exists() {
            return Ok(());
        }
        
        let file = File::open(&self.file_path)?;
        let reader = BufReader::new(file);
        let mut parser = RespParser::new();
        
        // Read and replay all commands
        for line in reader.lines() {
            let line = line?;
            parser.feed(line.as_bytes());
            parser.feed(b"\n");
            
            while let Some(frame) = parser.parse()? {
                // Execute command against storage
                // This is simplified - in reality we'd need the full command processor
                match frame {
                    RespFrame::Array(Some(parts)) if !parts.is_empty() => {
                        // Process command
                        self.replay_command(storage, &parts)?;
                    }
                    _ => continue,
                }
            }
        }
        
        Ok(())
    }
    
    /// Append a command to the AOF
    pub fn append_command(&self, command: &[RespFrame]) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }
        
        let mut writer_guard = self.writer.lock().unwrap();
        if let Some(writer) = writer_guard.as_mut() {
            // Serialize command as RESP array
            let frame = RespFrame::Array(Some(command.to_vec()));
            serialize_resp_frame(&frame, writer)?;
            
            // Handle fsync based on policy
            match self.config.fsync_policy {
                FsyncPolicy::Always => {
                    writer.flush()?;
                    writer.get_ref().sync_all()?;
                }
                FsyncPolicy::EverySecond => {
                    writer.flush()?;
                    
                    // Check if we should fsync
                    let mut last_fsync = self.last_fsync.lock().unwrap();
                    if last_fsync.elapsed() >= Duration::from_secs(1) {
                        writer.get_ref().sync_all()?;
                        *last_fsync = Instant::now();
                    }
                }
                FsyncPolicy::No => {
                    // Just flush to OS buffers
                    writer.flush()?;
                }
            }
        }
        
        Ok(())
    }
    
    /// Perform background rewrite
    pub fn bgrewrite(&self) -> Result<()> {
        {
            let mut rewrite_in_progress = self.rewrite_in_progress.lock().unwrap();
            if *rewrite_in_progress {
                return Err(FerrousError::Internal("Background AOF rewrite already in progress".into()));
            }
            *rewrite_in_progress = true;
        }
        
        // In a real implementation, this would fork or use a thread
        // For simplicity, we'll use a thread here
        let engine = self.clone();
        
        thread::spawn(move || {
            if let Err(e) = engine.do_rewrite() {
                eprintln!("AOF rewrite failed: {}", e);
            }
            
            let mut rewrite_in_progress = engine.rewrite_in_progress.lock().unwrap();
            *rewrite_in_progress = false;
        });
        
        Ok(())
    }
    
    /// Perform the actual rewrite
    fn do_rewrite(&self) -> Result<()> {
        // This is a simplified rewrite
        // In reality, we'd need to:
        // 1. Fork the process or carefully synchronize
        // 2. Write all current data to a temp file
        // 3. Buffer any new commands during rewrite
        // 4. Atomically replace the old AOF
        
        println!("AOF rewrite started");
        
        // For now, just compact the existing AOF
        let temp_path = self.file_path.with_extension("aof.rewrite");
        
        // Write compacted data to temp file
        // ... implementation details ...
        
        // Rename temp file to replace original
        std::fs::rename(&temp_path, &self.file_path)?;
        
        println!("AOF rewrite completed");
        Ok(())
    }
    
    /// Replay a command during AOF loading
    fn replay_command(&self, storage: &Arc<StorageEngine>, parts: &[RespFrame]) -> Result<()> {
        // Extract command name
        let cmd_frame = &parts[0];
        let command = match cmd_frame {
            RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_uppercase(),
            _ => return Ok(()), // Skip invalid commands
        };
        
        // Only replay write commands
        match command.as_str() {
            "SET" | "DEL" | "EXPIRE" | "LPUSH" | "RPUSH" | "SADD" | "ZADD" | "HSET" => {
                // Simplified replay - in reality we'd call the actual command handlers
                // This is just to demonstrate the concept
                Ok(())
            }
            _ => Ok(()), // Skip read-only commands
        }
    }
}

impl Clone for AofEngine {
    fn clone(&self) -> Self {
        Self {
            file_path: self.file_path.clone(),
            writer: Arc::clone(&self.writer),
            config: self.config.clone(),
            last_fsync: Arc::clone(&self.last_fsync),
            rewrite_in_progress: Arc::clone(&self.rewrite_in_progress),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_aof_config_default() {
        let config = AofConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.fsync_policy, FsyncPolicy::EverySecond);
    }
}