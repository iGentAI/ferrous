//! Network layer for Ferrous
//! 
//! Handles TCP connections, client management, and network I/O.

pub mod server;
pub mod connection;
pub mod listener;

pub use server::Server;
pub use connection::{Connection, ConnectionState};
pub use listener::Listener;

/// Network configuration
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// TCP bind address
    pub bind_addr: String,
    
    /// TCP port
    pub port: u16,
    
    /// Maximum number of client connections
    pub max_clients: usize,
    
    /// TCP backlog size
    pub tcp_backlog: u32,
    
    /// Connection timeout in seconds
    pub timeout: u64,
    
    /// TCP keepalive interval in seconds
    pub tcp_keepalive: Option<u64>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        NetworkConfig {
            bind_addr: "127.0.0.1".to_string(),
            port: 6379,
            max_clients: 10000,
            tcp_backlog: 511,
            timeout: 0, // 0 means no timeout
            tcp_keepalive: Some(300), // 5 minutes
        }
    }
}