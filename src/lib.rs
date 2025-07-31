//! Ferrous library
//! 
//! A Redis-compatible in-memory database server written in Rust with MLua-based Lua 5.1 scripting support.

pub mod error;
pub mod network;
pub mod protocol;
pub mod storage;
pub mod monitor;
pub mod pubsub;
pub mod replication;
pub mod config;

// Re-export commonly used types
pub use error::FerrousError;
pub use storage::engine::StorageEngine;
pub use network::server::Server;
pub use protocol::resp::RespFrame;
pub use config::Config;