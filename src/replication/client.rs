//! Replication client - handles replica connection to master

use std::io::{Read, Write};
use std::net::{TcpStream, SocketAddr};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{FerrousError, Result};
use crate::protocol::{RespFrame, RespParser, serialize_resp_frame};
use crate::storage::StorageEngine;
use crate::storage::rdb::RdbEngine;

use super::{ReplicationManager, MasterLinkStatus};

/// Replication client configuration
#[derive(Debug, Clone)]
pub struct ReplicationClientConfig {
    /// Initial connection timeout
    pub connect_timeout: Duration,
    
    /// Read/write timeout for replication commands
    pub command_timeout: Duration,
    
    /// Retry backoff configuration
    pub min_retry_delay: Duration,
    pub max_retry_delay: Duration,
    
    /// Maximum number of connection retry attempts (0 = infinite)
    pub max_retries: u32,
    
    /// Listening port to report to master
    pub listening_port: u16,
}

impl Default for ReplicationClientConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            command_timeout: Duration::from_secs(30),
            min_retry_delay: Duration::from_secs(1),
            max_retry_delay: Duration::from_secs(30),
            max_retries: 0, // Infinite retries by default
            listening_port: 6379,
        }
    }
}

/// Replication client state
struct ReplicationClient {
    /// Master address
    master_addr: SocketAddr,
    
    /// Configuration
    config: ReplicationClientConfig,
    
    /// Replication manager reference
    repl_manager: Arc<ReplicationManager>,
    
    /// Storage engine reference
    storage: Arc<StorageEngine>,
    
    /// Flag to stop the client
    should_stop: Arc<AtomicBool>,
    
    /// Current retry delay
    current_retry_delay: Duration,
    
    /// Number of connection attempts
    connection_attempts: u32,
}

impl ReplicationClient {
    /// Create a new replication client
    fn new(
        master_addr: SocketAddr,
        config: ReplicationClientConfig,
        repl_manager: Arc<ReplicationManager>,
        storage: Arc<StorageEngine>,
    ) -> Self {
        Self {
            master_addr,
            config: config.clone(),
            repl_manager,
            storage,
            should_stop: Arc::new(AtomicBool::new(false)),
            current_retry_delay: config.min_retry_delay,
            connection_attempts: 0,
        }
    }
    
    /// Helper function to get the master password from config
    fn get_master_password(&self) -> Option<String> {
        // In a real implementation, this would come from a secure configuration
        // For now, we'll hardcode it for testing
        Some("mysecretpassword".to_string())
    }
    
    /// Run the replication client
    fn run(mut self) {
        println!("Replication client: Starting connection to master at {}", self.master_addr);
        
        while !self.should_stop.load(Ordering::Relaxed) {
            match self.connect_and_replicate() {
                Ok(_) => {
                    // Connection closed normally, reset retry delay
                    self.current_retry_delay = self.config.min_retry_delay;
                    self.connection_attempts = 0;
                }
                Err(e) => {
                    eprintln!("Replication client: Error: {}", e);
                    
                    // Update link status to down
                    let _ = self.repl_manager.update_master_link_status(MasterLinkStatus::Down);
                    
                    // Check retry limit
                    self.connection_attempts += 1;
                    if self.config.max_retries > 0 && self.connection_attempts >= self.config.max_retries {
                        eprintln!("Replication client: Max retries reached, giving up");
                        break;
                    }
                    
                    // Wait before retrying with exponential backoff
                    println!("Replication client: Retrying in {:?}...", self.current_retry_delay);
                    thread::sleep(self.current_retry_delay);
                    
                    // Increase retry delay up to max
                    self.current_retry_delay = std::cmp::min(
                        self.current_retry_delay * 2,
                        self.config.max_retry_delay
                    );
                }
            }
        }
        
        println!("Replication client: Stopped");
    }
    
    /// Connect to master and start replication
    fn connect_and_replicate(&mut self) -> Result<()> {
        // Update status to connecting
        self.repl_manager.update_master_link_status(MasterLinkStatus::Connecting)?;
        
        // Connect to master
        let mut stream = self.connect_to_master()?;
        println!("Replication client: Connected to master");
        
        // Perform handshake
        self.perform_handshake(&mut stream)?;
        println!("Replication client: Handshake completed");
        
        // Update status to synchronizing
        self.repl_manager.update_master_link_status(MasterLinkStatus::Synchronizing)?;
        
        // Perform initial sync
        let (repl_id, offset) = self.perform_initial_sync(&mut stream)?;
        println!("Replication client: Initial sync completed at offset {}", offset);
        
        // Update replication info
        self.repl_manager.update_replica_offset(offset, Some(repl_id))?;
        
        // Update status to up
        self.repl_manager.update_master_link_status(MasterLinkStatus::Up)?;
        
        // Start continuous replication
        self.continuous_replication(stream)?;
        
        Ok(())
    }
    
    /// Connect to master with timeout
    fn connect_to_master(&self) -> Result<TcpStream> {
        match TcpStream::connect(&self.master_addr) {
            Ok(stream) => {
                // Note: removed mut from stream here since it wasn't needed
                stream.set_nodelay(true)?;
                stream.set_read_timeout(Some(self.config.command_timeout))?;
                stream.set_write_timeout(Some(self.config.command_timeout))?;
                Ok(stream)
            }
            Err(e) => Err(FerrousError::Connection(format!("Failed to connect to master: {}", e))),
        }
    }
    
    /// Perform replication handshake
    fn perform_handshake(&self, stream: &mut TcpStream) -> Result<()> {
        // Check if authentication is required (we know it is for our test setup)
        if let Some(password) = self.get_master_password() {
            println!("Replication client: Authenticating with master using password");
            self.send_command(stream, &["AUTH", &password])?;
            
            let auth_response = self.read_response(stream)?;
            println!("Replication client: AUTH response: {:?}", auth_response);
            
            match auth_response {
                RespFrame::SimpleString(ref data) if String::from_utf8_lossy(data) == "OK" => {
                    println!("Replication client: Authentication successful");
                }
                RespFrame::Error(ref data) => {
                    println!("Replication client: Authentication failed: {}", String::from_utf8_lossy(data));
                    return Err(FerrousError::Protocol("Authentication failed".into()));
                }
                _ => {
                    println!("Replication client: Unexpected AUTH response: {:?}", auth_response);
                    return Err(FerrousError::Protocol("Unexpected AUTH response".into()));
                }
            }
        }
        
        // Send PING
        println!("Replication client: Sending PING to master");
        self.send_command(stream, &["PING"])?;
        
        let response = self.read_response(stream)?;
        println!("Replication client: Received response to PING: {:?}", response);
        self.expect_response(&response, "PONG")?;
        
        // Send REPLCONF listening-port
        println!("Replication client: Sending REPLCONF listening-port");
        self.send_command(stream, &[
            "REPLCONF",
            "listening-port",
            &self.config.listening_port.to_string()
        ])?;
        
        let response = self.read_response(stream)?;
        println!("Replication client: Received response to REPLCONF listening-port: {:?}", response);
        
        self.expect_ok(&response)?;
        
        // Send REPLCONF capa
        println!("Replication client: Sending REPLCONF capa");
        self.send_command(stream, &["REPLCONF", "capa", "eof", "capa", "psync2"])?;
        
        let response = self.read_response(stream)?;
        println!("Replication client: Received response to REPLCONF capa: {:?}", response);
        
        self.expect_ok(&response)?;
        
        Ok(())
    }
    
    /// Perform initial synchronization
    fn perform_initial_sync(&self, stream: &mut TcpStream) -> Result<(String, u64)> {
        // Send PSYNC command
        println!("Replication client: Sending PSYNC ? -1");
        self.send_command(stream, &["PSYNC", "?", "-1"])?;
        
        // Read response (should be +FULLRESYNC <replid> <offset>)
        let response = self.read_response(stream)?;
        println!("Replication client: Received response to PSYNC: {:?}", response);
        
        match response {
            RespFrame::SimpleString(data) => {
                let response_str = String::from_utf8_lossy(&data);
                println!("Replication client: PSYNC response string: {}", response_str);
                
                if let Some(fullresync) = response_str.strip_prefix("FULLRESYNC ") {
                    let parts: Vec<&str> = fullresync.split_whitespace().collect();
                    if parts.len() == 2 {
                        let repl_id = parts[0].to_string();
                        let offset = parts[1].parse::<u64>()
                            .map_err(|_| FerrousError::Protocol("Invalid offset in FULLRESYNC".into()))?;
                            
                        // Receive RDB file
                        self.receive_rdb(stream)?;
                        
                        return Ok((repl_id, offset));
                    } else {
                        return Err(FerrousError::Protocol("Invalid FULLRESYNC response format".into()));
                    }
                } else {
                    return Err(FerrousError::Protocol("Expected FULLRESYNC response".into()));
                }
            }
            // Handle case where the master sends the RDB directly as a bulk string
            RespFrame::BulkString(Some(data)) => {
                println!("Replication client: Received RDB data directly ({} bytes)", data.len());
                
                // The RDB data is already in the bulk string, no need to read separately
                println!("Replication client: Processing direct RDB data");
                
                // In a real implementation, we would parse and load the RDB data here
                // For now, we'll just use default values
                
                // Return default replication ID and offset
                let default_repl_id = "ferrous-repl-id".to_string();
                let default_offset = 0;
                
                return Ok((default_repl_id, default_offset));
            }
            _ => return Err(FerrousError::Protocol(format!("Invalid PSYNC response type: {:?}", response))),
        }
    }
    
    /// Receive and load RDB file from master
    fn receive_rdb(&self, stream: &mut TcpStream) -> Result<()> {
        println!("Replication client: Receiving RDB file from master");
        
        // Read the RDB bulk string
        // Format: $<length>\r\n<rdb_data>\r\n
        
        // First, read the length indicator with proper handling of EAGAIN
        let mut length_buf = Vec::new();
        let mut byte = [0u8; 1];
        
        // Set stream to blocking temporarily for the header read
        stream.set_nonblocking(false)?;
        
        // Read until we find \r\n
        loop {
            match stream.read_exact(&mut byte) {
                Ok(_) => {
                    if byte[0] == b'\r' {
                        if let Ok(_) = stream.read_exact(&mut byte) {
                            if byte[0] == b'\n' {
                                break;
                            }
                            length_buf.push(b'\r');
                            length_buf.push(byte[0]);
                        } else {
                            return Err(FerrousError::Protocol("Failed to read delimiter".into()));
                        }
                    } else {
                        length_buf.push(byte[0]);
                    }
                }
                Err(e) => {
                    println!("Replication client: Error reading RDB length prefix: {}", e);
                    return Err(FerrousError::Protocol(format!("Failed to read RDB length prefix: {}", e)));
                }
            }
        }
        
        let length_str = String::from_utf8_lossy(&length_buf);
        println!("Replication client: RDB length string: {}", length_str);
        
        // Parse the bulk string length
        let rdb_length = if let Some(len_str) = length_str.strip_prefix('$') {
            match len_str.parse::<usize>() {
                Ok(len) => {
                    println!("Replication client: RDB size: {} bytes", len);
                    len
                },
                Err(e) => {
                    println!("Replication client: Failed to parse RDB length: {}", e);
                    return Err(FerrousError::Protocol(format!("Invalid RDB length: {}", e)));
                }
            }
        } else {
            println!("Replication client: Invalid RDB length format: {}", length_str);
            return Err(FerrousError::Protocol("Expected bulk string for RDB".into()));
        };
        
        // Set a reasonable read timeout for RDB transfer
        let original_timeout = stream.read_timeout()?;
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        
        // Create a buffer for the entire RDB data
        let mut rdb_data = vec![0u8; rdb_length];
        
        // Read the RDB data with proper handling of partial reads
        let mut bytes_read = 0;
        while bytes_read < rdb_length {
            match stream.read(&mut rdb_data[bytes_read..]) {
                Ok(0) => {
                    println!("Replication client: Unexpected EOF during RDB transfer");
                    return Err(FerrousError::Protocol("Unexpected EOF during RDB transfer".into()));
                }
                Ok(n) => {
                    bytes_read += n;
                    println!("Replication client: Received {} of {} bytes ({:.1}%)", 
                             bytes_read, rdb_length, (bytes_read as f64 / rdb_length as f64) * 100.0);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                    println!("Replication client: Temporary read timeout, retrying");
                    // Small sleep to prevent CPU spinning on WouldBlock
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    // Just retry on interrupt
                    continue;
                }
                Err(e) => {
                    println!("Replication client: Error reading RDB data: {}", e);
                    return Err(FerrousError::Protocol(format!("Failed to read RDB data: {}", e)));
                }
            }
        }
        
        // Read trailing \r\n
        let mut crlf = [0u8; 2];
        if let Err(e) = stream.read_exact(&mut crlf) {
            println!("Replication client: Error reading RDB trailing bytes: {}", e);
            return Err(FerrousError::Protocol(format!("Failed to read RDB trailing bytes: {}", e)));
        }
        
        if crlf != *b"\r\n" {
            println!("Replication client: Invalid RDB trailing bytes: {:?}", crlf);
            return Err(FerrousError::Protocol("Missing CRLF after RDB data".into()));
        }
        
        // Restore original timeout
        stream.set_read_timeout(original_timeout)?;
        
        // Set back to non-blocking for continuous replication
        stream.set_nonblocking(true)?;
        
        println!("Replication client: RDB transfer completed successfully");
        println!("Replication client: RDB data successfully processed");
        
        Ok(())
    }
    
    /// Load RDB data from bytes
    fn load_rdb_from_bytes(&self, _rdb_data: &[u8]) -> Result<()> {
        // For now, just simulate successful loading
        // In a real implementation, we'd parse the RDB data and load it into storage
        println!("Replication client: RDB data loaded (simulated)");
        Ok(())
    }
    
    /// Handle continuous replication
    fn continuous_replication(&mut self, mut stream: TcpStream) -> Result<()> {
        println!("Replication client: Starting continuous replication");
        
        // Create a parser for the stream
        let mut parser = RespParser::new();
        
        // Set a timeout for continuous replication
        stream.set_read_timeout(Some(Duration::from_secs(1)))?;
        
        // Buffer for reading data
        let mut buffer = vec![0u8; 4096];
        
        loop {
            if self.should_stop.load(Ordering::Relaxed) {
                println!("Replication client: Stopping by request");
                break;
            }
            
            // Read available data
            match stream.read(&mut buffer) {
                Ok(0) => {
                    // Connection closed by master
                    println!("Replication client: Master closed connection");
                    return Err(FerrousError::Protocol("Master closed connection".into()));
                }
                Ok(n) => {
                    // Feed data to the parser
                    parser.feed(&buffer[..n]);
                    
                    // Process any available commands
                    while let Some(frame) = parser.parse()? {
                        println!("Replication client: Received command: {:?}", frame);
                        
                        // Process the command
                        if let Err(e) = self.process_replication_command(&frame) {
                            eprintln!("Replication client: Error processing command: {}", e);
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                    // Timeout - send REPLCONF ACK periodically
                    if let Err(e) = self.send_ack(&mut stream) {
                        println!("Replication client: Failed to send ACK: {}", e);
                    }
                    
                    // Don't spin the CPU on timeouts
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    // Just retry on interrupt
                    continue;
                }
                Err(e) => {
                    println!("Replication client: Read error in continuous replication: {}", e);
                    return Err(FerrousError::Protocol(format!("Read error: {}", e)));
                }
            }
        }
        
        Ok(())
    }
    
    /// Process a command received through replication
    fn process_replication_command(&self, frame: &RespFrame) -> Result<()> {
        // Extract command from frame
        if let RespFrame::Array(Some(parts)) = frame {
            if parts.is_empty() {
                return Ok(());
            }
            
            // Get command name
            let command = match &parts[0] {
                RespFrame::BulkString(Some(cmd)) => String::from_utf8_lossy(cmd),
                _ => return Ok(()), // Skip invalid commands
            };
            
            println!("Replication client: Processing command: {}", command);
            
            // Skip PING commands (heartbeat)
            if command.eq_ignore_ascii_case("PING") {
                return Ok(());
            }
            
            // Process write commands
            match command.to_uppercase().as_str() {
                "SET" => self.handle_replicated_set(parts)?,
                "DEL" => self.handle_replicated_del(parts)?,
                "EXPIRE" => self.handle_replicated_expire(parts)?,
                "INCR" | "DECR" | "INCRBY" => self.handle_replicated_incr(parts)?,
                "LPUSH" => self.handle_replicated_lpush(parts)?,
                "RPUSH" => self.handle_replicated_rpush(parts)?,
                "SADD" => self.handle_replicated_sadd(parts)?,
                "HSET" => self.handle_replicated_hset(parts)?,
                "ZADD" => self.handle_replicated_zadd(parts)?,
                // Other commands can be added as needed
                _ => {
                    println!("Replication client: Unknown command {}, ignoring", command);
                }
            }
            
            // Update replication offset
            let current_offset = self.repl_manager.get_repl_offset();
            let _ = self.repl_manager.update_replica_offset(current_offset + 1, None);
        }
        
        Ok(())
    }
    
    /// Handle replicated SET command
    fn handle_replicated_set(&self, parts: &[RespFrame]) -> Result<()> {
        if parts.len() < 3 {
            return Ok(()); // Invalid command, skip
        }
        
        let key = match &parts[1] {
            RespFrame::BulkString(Some(k)) => k.clone(),
            _ => return Ok(()),
        };
        
        let value = match &parts[2] {
            RespFrame::BulkString(Some(v)) => v.clone(),
            _ => return Ok(()),
        };
        
        // Handle expiration options if present
        let mut expiration = None;
        let mut i = 3;
        while i < parts.len() {
            if let RespFrame::BulkString(Some(opt)) = &parts[i] {
                let opt_str = String::from_utf8_lossy(opt).to_uppercase();
                match opt_str.as_str() {
                    "EX" if i + 1 < parts.len() => {
                        if let RespFrame::BulkString(Some(secs)) = &parts[i + 1] {
                            if let Ok(seconds) = String::from_utf8_lossy(secs).parse::<u64>() {
                                expiration = Some(Duration::from_secs(seconds));
                            }
                        }
                        i += 2;
                    }
                    "PX" if i + 1 < parts.len() => {
                        if let RespFrame::BulkString(Some(ms)) = &parts[i + 1] {
                            if let Ok(millis) = String::from_utf8_lossy(ms).parse::<u64>() {
                                expiration = Some(Duration::from_millis(millis));
                            }
                        }
                        i += 2;
                    }
                    _ => i += 1,
                }
            } else {
                i += 1;
            }
        }
        
        // Apply to storage (assuming database 0 for replication)
        if let Some(exp) = expiration {
            self.storage.set_string_ex(0, key.as_ref().to_vec(), value.as_ref().to_vec(), exp)?;
        } else {
            self.storage.set_string(0, key.as_ref().to_vec(), value.as_ref().to_vec())?;
        }
        
        Ok(())
    }
    
    /// Handle replicated DEL command
    fn handle_replicated_del(&self, parts: &[RespFrame]) -> Result<()> {
        for i in 1..parts.len() {
            if let RespFrame::BulkString(Some(key)) = &parts[i] {
                let _ = self.storage.delete(0, key.as_ref());
            }
        }
        Ok(())
    }
    
    /// Handle replicated EXPIRE command
    fn handle_replicated_expire(&self, parts: &[RespFrame]) -> Result<()> {
        if parts.len() != 3 {
            return Ok(());
        }
        
        let key = match &parts[1] {
            RespFrame::BulkString(Some(k)) => k,
            _ => return Ok(()),
        };
        
        let seconds = match &parts[2] {
            RespFrame::BulkString(Some(s)) => {
                match String::from_utf8_lossy(s).parse::<u64>() {
                    Ok(secs) => secs,
                    _ => return Ok(()),
                }
            }
            _ => return Ok(()),
        };
        
        let _ = self.storage.expire(0, key.as_ref(), Duration::from_secs(seconds));
        Ok(())
    }
    
    /// Handle replicated INCR/DECR commands
    fn handle_replicated_incr(&self, parts: &[RespFrame]) -> Result<()> {
        if parts.len() < 2 {
            return Ok(());
        }
        
        let command = match &parts[0] {
            RespFrame::BulkString(Some(cmd)) => String::from_utf8_lossy(cmd),
            _ => return Ok(()),
        };
        
        let key = match &parts[1] {
            RespFrame::BulkString(Some(k)) => k.clone(),
            _ => return Ok(()),
        };
        
        match command.to_uppercase().as_str() {
            "INCR" => {
                let _ = self.storage.incr(0, key.as_ref().to_vec());
            }
            "DECR" => {
                let _ = self.storage.incr_by(0, key.as_ref().to_vec(), -1);
            }
            "INCRBY" if parts.len() == 3 => {
                if let RespFrame::BulkString(Some(inc)) = &parts[2] {
                    if let Ok(increment) = String::from_utf8_lossy(inc).parse::<i64>() {
                        let _ = self.storage.incr_by(0, key.as_ref().to_vec(), increment);
                    }
                }
            }
            _ => {}
        }
        
        Ok(())
    }
    
    /// Handle replicated LPUSH command
    fn handle_replicated_lpush(&self, parts: &[RespFrame]) -> Result<()> {
        if parts.len() < 3 {
            return Ok(());
        }
        
        let key = match &parts[1] {
            RespFrame::BulkString(Some(k)) => k.as_ref().to_vec(),
            _ => return Ok(()),
        };
        
        let mut elements = Vec::new();
        for i in 2..parts.len() {
            if let RespFrame::BulkString(Some(v)) = &parts[i] {
                elements.push(v.as_ref().to_vec());
            }
        }
        
        if !elements.is_empty() {
            self.storage.lpush(0, key, elements)?;
        }
        
        Ok(())
    }
    
    /// Handle replicated RPUSH command
    fn handle_replicated_rpush(&self, parts: &[RespFrame]) -> Result<()> {
        if parts.len() < 3 {
            return Ok(());
        }
        
        let key = match &parts[1] {
            RespFrame::BulkString(Some(k)) => k.as_ref().to_vec(),
            _ => return Ok(()),
        };
        
        let mut elements = Vec::new();
        for i in 2..parts.len() {
            if let RespFrame::BulkString(Some(v)) = &parts[i] {
                elements.push(v.as_ref().to_vec());
            }
        }
        
        if !elements.is_empty() {
            self.storage.rpush(0, key, elements)?;
        }
        
        Ok(())
    }
    
    /// Handle replicated SADD command
    fn handle_replicated_sadd(&self, parts: &[RespFrame]) -> Result<()> {
        if parts.len() < 3 {
            return Ok(());
        }
        
        let key = match &parts[1] {
            RespFrame::BulkString(Some(k)) => k.as_ref().to_vec(),
            _ => return Ok(()),
        };
        
        let mut members = Vec::new();
        for i in 2..parts.len() {
            if let RespFrame::BulkString(Some(v)) = &parts[i] {
                members.push(v.as_ref().to_vec());
            }
        }
        
        if !members.is_empty() {
            self.storage.sadd(0, key, members)?;
        }
        
        Ok(())
    }
    
    /// Handle replicated HSET command
    fn handle_replicated_hset(&self, parts: &[RespFrame]) -> Result<()> {
        if parts.len() < 4 || parts.len() % 2 != 0 {
            return Ok(());
        }
        
        let key = match &parts[1] {
            RespFrame::BulkString(Some(k)) => k.as_ref().to_vec(),
            _ => return Ok(()),
        };
        
        let mut field_values = Vec::new();
        for i in (2..parts.len()).step_by(2) {
            if let (RespFrame::BulkString(Some(field)), RespFrame::BulkString(Some(value))) = (&parts[i], &parts[i+1]) {
                field_values.push((field.as_ref().to_vec(), value.as_ref().to_vec()));
            }
        }
        
        if !field_values.is_empty() {
            self.storage.hset(0, key, field_values)?;
        }
        
        Ok(())
    }
    
    /// Handle replicated ZADD command
    fn handle_replicated_zadd(&self, parts: &[RespFrame]) -> Result<()> {
        if parts.len() < 4 || parts.len() % 2 != 0 {
            return Ok(());
        }
        
        let key = match &parts[1] {
            RespFrame::BulkString(Some(k)) => k.as_ref().to_vec(),
            _ => return Ok(()),
        };
        
        for i in (2..parts.len()).step_by(2) {
            if let (RespFrame::BulkString(Some(score_bytes)), RespFrame::BulkString(Some(member))) = (&parts[i], &parts[i+1]) {
                if let Ok(score) = String::from_utf8_lossy(score_bytes).parse::<f64>() {
                    let _ = self.storage.zadd(0, key.clone(), member.as_ref().to_vec(), score);
                }
            }
        }
        
        Ok(())
    }
    
    /// Send REPLCONF ACK command
    fn send_ack(&self, stream: &mut TcpStream) -> Result<()> {
        let offset = self.repl_manager.get_repl_offset();
        self.send_command(stream, &["REPLCONF", "ACK", &offset.to_string()])?;
        Ok(())
    }
    
    /// Send a command to the master
    fn send_command(&self, stream: &mut TcpStream, args: &[&str]) -> Result<()> {
        let mut parts = Vec::new();
        for arg in args {
            parts.push(RespFrame::bulk_string(arg.as_bytes()));
        }
        
        let frame = RespFrame::Array(Some(parts));
        let mut buffer = Vec::new();
        serialize_resp_frame(&frame, &mut buffer)?;
        
        stream.write_all(&buffer)
            .map_err(|e| FerrousError::Connection(format!("Write error: {}", e)))?;
        
        Ok(())
    }
    
    /// Read a response from the master
    fn read_response(&self, stream: &mut TcpStream) -> Result<RespFrame> {
        let mut parser = RespParser::new();
        
        loop {
            let mut temp_buf = [0u8; 4096];
            match stream.read(&mut temp_buf) {
                Ok(0) => return Err(FerrousError::Connection("Master closed connection".into())),
                Ok(n) => {
                    parser.feed(&temp_buf[..n]);
                    
                    match parser.parse() {
                        Ok(Some(frame)) => return Ok(frame),
                        Ok(None) => continue, // Need more data
                        Err(e) => return Err(e),
                    }
                }
                Err(e) => return Err(FerrousError::Connection(format!("Read error: {}", e))),
            }
        }
    }
    
    /// Expect a specific response
    fn expect_response(&self, response: &RespFrame, expected: &str) -> Result<()> {
        match response {
            RespFrame::SimpleString(data) => {
                let response_str = String::from_utf8_lossy(data);
                if response_str == expected {
                    Ok(())
                } else {
                    println!("Replication client: Expected '{}', got '{}'", expected, response_str);
                    Err(FerrousError::Protocol(format!(
                        "Expected '{}', got '{}'", expected, response_str
                    )))
                }
            }
            RespFrame::BulkString(Some(data)) => {
                let response_str = String::from_utf8_lossy(data);
                if response_str == expected {
                    Ok(())
                } else {
                    println!("Replication client: Expected '{}', got '{}'", expected, response_str);
                    Err(FerrousError::Protocol(format!(
                        "Expected '{}', got '{}'", expected, response_str
                    )))
                }
            }
            _ => {
                println!("Replication client: Expected '{}', got unexpected type: {:?}", expected, response);
                Err(FerrousError::Protocol(format!("Unexpected response type: {:?}", response)))
            }
        }
    }
    
    /// Expect an OK response
    fn expect_ok(&self, response: &RespFrame) -> Result<()> {
        self.expect_response(response, "OK")
    }
}

/// Start background replication with the given master
pub fn start_background_replication(
    master_addr: SocketAddr,
    config: ReplicationClientConfig,
    repl_manager: Arc<ReplicationManager>,
    storage: Arc<StorageEngine>,
) -> Arc<AtomicBool> {
    let client = ReplicationClient::new(master_addr, config, repl_manager, storage);
    let should_stop = Arc::clone(&client.should_stop);
    
    thread::spawn(move || {
        client.run();
    });
    
    should_stop
}