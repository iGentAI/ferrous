//! Ferrous library
//! 
//! This file exposes the public API of Ferrous for use as a library.

pub mod error;
pub mod network;
pub mod protocol;
pub mod storage;
pub mod monitor;
pub mod pubsub;
pub mod replication;
pub mod config;

// Note: Lua module has been removed for reimplementation
// Uncomment when new implementation is ready:
// pub mod lua;

// Re-export commonly used types
pub use error::FerrousError;
pub use storage::engine::StorageEngine;
pub use network::server::Server;
pub use protocol::resp::RespFrame;
pub use config::Config;

// Lua exports will be readded once reimplemented