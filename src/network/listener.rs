//! TCP listener for accepting client connections

use std::net::{TcpListener, TcpStream, SocketAddr};
use std::io;
use crate::error::{FerrousError, Result};
use super::NetworkConfig;

/// TCP listener wrapper
pub struct Listener {
    listener: TcpListener,
    config: NetworkConfig,
}

impl Listener {
    /// Create a new listener bound to the configured address
    pub fn bind(config: NetworkConfig) -> Result<Self> {
        let addr = format!("{}:{}", config.bind_addr, config.port);
        let listener = TcpListener::bind(&addr)
            .map_err(|e| FerrousError::Io(
                format!("Failed to bind to {}: {}", addr, e)
            ))?;
        
        // Set non-blocking mode
        listener.set_nonblocking(true)?;
        
        println!("Ferrous listening on {}", addr);
        
        Ok(Listener { listener, config })
    }
    
    /// Accept a new connection
    /// Returns None if would block
    pub fn accept(&self) -> Result<Option<(TcpStream, SocketAddr)>> {
        match self.listener.accept() {
            Ok((stream, addr)) => {
                // Note: TCP keepalive would require platform-specific code
                // For now, we'll handle timeouts at the application level
                
                Ok(Some((stream, addr)))
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                Ok(None)
            }
            Err(e) => Err(e.into()),
        }
    }
    
    /// Get the local address the listener is bound to
    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.listener.local_addr().map_err(Into::into)
    }
}