//! Connection management for individual clients
//! 
//! Handles the lifecycle of a client connection including reading, writing,
//! and protocol parsing.

use std::net::{TcpStream, SocketAddr};
use std::io::{Read, Write, ErrorKind};
use std::time::Instant;
use crate::error::{FerrousError, Result};
use crate::protocol::{RespParser, RespFrame, serialize_resp_frame};

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
    
    /// Last activity timestamp
    pub last_activity: Instant,
    
    /// Selected database (default 0)
    pub db_index: usize,
}

impl Connection {
    /// Create a new connection
    pub fn new(id: u64, stream: TcpStream, addr: SocketAddr) -> Result<Self> {
        // Set non-blocking mode
        stream.set_nonblocking(true)?;
        
        // Set TCP nodelay
        stream.set_nodelay(true)?;
        
        Ok(Connection {
            id,
            stream,
            addr,
            state: ConnectionState::Connected,
            parser: RespParser::new(),
            write_buffer: Vec::with_capacity(4096),
            last_activity: Instant::now(),
            db_index: 0,
        })
    }
    
    /// Read data from the connection
    /// Returns true if data was read, false if would block
    pub fn read(&mut self) -> Result<bool> {
        let mut buf = [0u8; 4096];
        
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
        self.write_buffer.clear();
        serialize_resp_frame(frame, &mut self.write_buffer)?;
        self.flush()
    }
    
    /// Send raw bytes to the client
    pub fn send_raw(&mut self, data: &[u8]) -> Result<()> {
        self.write_buffer.extend_from_slice(data);
        self.flush()
    }
    
    /// Flush the write buffer
    pub fn flush(&mut self) -> Result<()> {
        if self.write_buffer.is_empty() {
            return Ok(());
        }
        
        let mut written = 0;
        while written < self.write_buffer.len() {
            match self.stream.write(&self.write_buffer[written..]) {
                Ok(n) => {
                    written += n;
                    self.last_activity = Instant::now();
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    // Can't write more right now
                    break;
                }
                Err(e) => return Err(e.into()),
            }
        }
        
        // Remove written data
        self.write_buffer.drain(..written);
        
        Ok(())
    }
    
    /// Check if the connection has data to write
    pub fn has_pending_writes(&self) -> bool {
        !self.write_buffer.is_empty()
    }
    
    /// Close the connection
    pub fn close(&mut self) -> Result<()> {
        self.state = ConnectionState::Closing;
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