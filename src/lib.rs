//! Ferrous library
//! 
//! This file exposes the public API of Ferrous for use as a library.

pub mod error;
pub mod protocol;
pub mod network;
pub mod storage;
pub mod pubsub;
pub mod replication;
pub mod config;
pub mod monitor;

// Re-export commonly used types
pub use error::{FerrousError, Result};
pub use protocol::{RespFrame, RespParser};
pub use network::{Server, Connection, NetworkConfig};
pub use storage::{StorageEngine, Value, ValueType};
pub use pubsub::PubSubManager;
pub use replication::{ReplicationManager, ReplicationConfig};
pub use config::Config;
pub use monitor::MonitorSubscribers;