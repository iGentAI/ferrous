//! Ferrous - A Redis-compatible in-memory database server written in pure Rust
//! 
//! This is a customized entry point for running Ferrous as a secondary instance (replica).

mod error;
mod protocol;
mod network;
mod storage;
mod pubsub;
mod replication;

use std::process;
use error::Result;
use network::{Server, NetworkConfig};
use replication::ReplicationConfig;

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

fn run() -> Result<()> {
    println!("Starting Ferrous Replica - Redis-compatible server in Rust");
    println!("Version: {}", env!("CARGO_PKG_VERSION"));
    
    // Create server config with a different port
    let mut network_config = NetworkConfig::default();
    network_config.port = 6380; // Use different port for the replica
    
    // For testing authentication, enable password protection
    network_config.password = Some("mysecretpassword".to_string());
    
    println!("Authentication enabled");
    println!("Listening on port: {}", network_config.port);
    
    // Create replication configuration
    let replication_config = ReplicationConfig::default();
    
    // Create and run server
    let mut server = Server::with_configs(network_config, replication_config)?;
    
    // Run the server
    server.run()
}