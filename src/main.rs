//! Ferrous - A Redis-compatible in-memory database server written in pure Rust
//! 
//! This is the main entry point for the Ferrous server.

mod error;
mod protocol;
mod network;
mod storage;
mod pubsub;

use std::process;
use error::Result;
use network::{Server, NetworkConfig};

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

fn run() -> Result<()> {
    println!("Starting Ferrous - Redis-compatible server in Rust");
    println!("Version: {}", env!("CARGO_PKG_VERSION"));
    
    // Create server with config that includes password
    let mut config = NetworkConfig::default();
    
    // For testing authentication, enable password protection
    config.password = Some("mysecretpassword".to_string());
    
    println!("Authentication enabled with password: 'mysecretpassword'");
    
    let mut server = Server::with_config(config)?;
    
    // Run the server
    server.run()
}