//! Ferrous - A Redis-compatible in-memory database server written in pure Rust
//! 
//! This is the main entry point for the Ferrous server.

mod config;
mod error;
mod protocol;
mod network;
mod storage;
mod pubsub;
mod replication;
mod monitor;
mod lua;

use std::process;
use error::Result;
use network::Server;
use config::{Config};

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

fn run() -> Result<()> {
    println!("Starting Ferrous - Redis-compatible server in Rust");
    println!("Version: {}", env!("CARGO_PKG_VERSION"));
    
    // Parse command-line arguments
    let cli_args = config::parse_cli_args();
    
    // Load configuration
    let mut config = if let Some(ref config_path) = cli_args.config {
        println!("Loading configuration from: {}", config_path.display());
        match config::Config::from_file(config_path) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("Error loading configuration: {}", e);
                process::exit(1);
            }
        }
    } else {
        config::Config::default()
    };
    
    // Apply command-line overrides
    config.apply_cli_args(cli_args);
    
    // Check for password
    if let Some(ref password) = config.network.password {
        println!("Authentication enabled with password: '{}'", password);
    }
    
    println!("Ferrous listening on {}:{}", config.network.bind_addr, config.network.port);
    
    // Create and run server
    let mut server = Server::from_config(config)?;
    
    // Run the server
    server.run()
}