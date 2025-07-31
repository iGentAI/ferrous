//! Connection management for individual clients
//! 
//! Handles the lifecycle of a client connection including reading, writing,
//! and protocol parsing.

use std::net::{TcpStream, SocketAddr};
use std::io::{Read, Write, ErrorKind};
use std::time::Instant;
use std::collections::HashMap;
use crate::error::{FerrousError, Result};
use crate::protocol::{RespParser, RespFrame, serialize_resp_frame};
use crate::storage::commands::transactions::TransactionState;

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Connected but not authenticated (if auth is required)
    Connected,
    
    /// Authenticated and ready for commands
    Authenticated,
    
    /// Blocked on a blocking operation
    Blocked,
    
    /// Connection is closing
    Closing,
}

/// Represents a client connection
pub struct Connection {
    /// Unique connection ID
    pub id: u64,
    
    /// TCP stream
    stream: TcpStream,
    
    /// Client address
    pub addr: SocketAddr,
    
    /// Connection state
    pub state: ConnectionState,
    
    /// RESP protocol parser
    parser: RespParser,
    
    /// Write buffer
    write_buffer: Vec<u8>,
    
    /// Write buffer offset (for partial writes)
    write_offset: usize,
    
    /// Last activity timestamp
    pub last_activity: Instant,
    
    /// Creation time
    pub created_at: Instant,
    
    /// Selected database (default 0)
    pub db_index: usize,
    
    /// Transaction state
    pub transaction_state: TransactionState,
    
    /// Whether this connection is subscribed to MONITOR
    pub is_monitoring: bool,
    
    /// Client name (set via CLIENT SETNAME)
    pub name: Option<String>,
    
    /// Per-connection Lua script cache (SHA1 -> script content)
    pub script_cache: HashMap<String, String>,
}

impl Connection {
    /// Create a new connection
    pub fn new(id: u64, stream: TcpStream, addr: SocketAddr) -> Result<Self> {
        // Set non-blocking mode
        stream.set_nonblocking(true)?;
        
        // Set TCP nodelay for low latency
        stream.set_nodelay(true)?;
        
        let now = Instant::now();
        
        Ok(Connection {
            id,
            stream,
            addr,
            state: ConnectionState::Connected,
            parser: RespParser::new(),
            write_buffer: Vec::with_capacity(16384), // Larger initial capacity for better pipelining
            write_offset: 0,
            last_activity: now,
            created_at: now,
            db_index: 0,
            transaction_state: TransactionState::default(),
            is_monitoring: false,
            name: None,
            script_cache: HashMap::new(),
        })
    }
    
    /// Read data from the connection
    /// Returns true if data was read, false if would block
    pub fn read(&mut self) -> Result<bool> {
        let mut buf = [0u8; 8192]; // Larger read buffer for better pipelining
        
        match self.stream.read(&mut buf) {
            Ok(0) => {
                // Connection closed by peer
                self.state = ConnectionState::Closing;
                Err(FerrousError::Connection("Connection closed by peer".into()))
            }
            Ok(n) => {
                self.last_activity = Instant::now();
                self.parser.feed(&buf[..n]);
                Ok(true)
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                // No data available
                Ok(false)
            }
            Err(e) => Err(e.into()),
        }
    }
    
    /// Try to parse a frame from the read buffer
    pub fn parse_frame(&mut self) -> Result<Option<RespFrame>> {
        self.parser.parse()
    }
    
    /// Send a frame to the client
    pub fn send_frame(&mut self, frame: &RespFrame) -> Result<()> {
        // Serialize directly to the write buffer without clearing it
        serialize_resp_frame(frame, &mut self.write_buffer)?;
        // Don't flush here - let the caller decide when to flush
        Ok(())
    }
    
    /// Send raw bytes to the client
    pub fn send_raw(&mut self, data: &[u8]) -> Result<()> {
        self.write_buffer.extend_from_slice(data);
        Ok(())
    }
    
    /// Flush the write buffer
    pub fn flush(&mut self) -> Result<()> {
        if self.write_offset >= self.write_buffer.len() {
            // Nothing to write
            self.write_buffer.clear();
            self.write_offset = 0;
            return Ok(());
        }
        
        let mut attempts = 0;
        const MAX_ATTEMPTS: usize = 3;
        
        while self.write_offset < self.write_buffer.len() && attempts < MAX_ATTEMPTS {
            match self.stream.write(&self.write_buffer[self.write_offset..]) {
                Ok(0) => {
                    // Can't write, connection might be closed
                    return Err(FerrousError::Connection("Cannot write to connection".into()));
                }
                Ok(n) => {
                    self.write_offset += n;
                    self.last_activity = Instant::now();
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    // Can't write more right now, maintain offset
                    attempts += 1;
                    // A short yield to give the socket a chance to become writable
                    std::thread::yield_now();
                    continue;
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => {
                    // Retry
                    attempts += 1;
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
        
        // Clear buffer if everything was written
        if self.write_offset >= self.write_buffer.len() {
            self.write_buffer.clear();
            self.write_offset = 0;
        }
        
        Ok(())
    }
    
    /// Check if the connection has data to write
    pub fn has_pending_writes(&self) -> bool {
        self.write_offset < self.write_buffer.len()
    }
    
    /// Close the connection
    pub fn close(&mut self) -> Result<()> {
        self.state = ConnectionState::Closing;
        // Try to flush remaining data before closing
        let _ = self.flush();
        self.stream.shutdown(std::net::Shutdown::Both)?;
        Ok(())
    }
    
    /// Check if the connection is closing
    pub fn is_closing(&self) -> bool {
        self.state == ConnectionState::Closing
    }
    
    /// Get time since last activity
    pub fn idle_time(&self) -> std::time::Duration {
        self.last_activity.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_connection_state() {
        let state = ConnectionState::Connected;
        assert_eq!(state, ConnectionState::Connected);
        assert_ne!(state, ConnectionState::Authenticated);
    }
}