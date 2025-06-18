//! Storage engine for Ferrous
//! 
//! This module provides the core data structures and storage functionality
//! for Redis-compatible data types.

pub mod engine;
pub mod value;
pub mod memory;
pub mod skiplist;
pub mod rdb;
pub mod monitor;

pub use engine::{StorageEngine, GetResult};
pub use value::{Value, ValueType, StringEncoding, StoredValue, ValueMetadata};
pub use memory::{MemoryManager, EvictionPolicy};
pub use skiplist::SkipList;
pub use rdb::{RdbEngine, RdbConfig};
pub use monitor::StorageMonitor;

/// Database index type
pub type DatabaseIndex = usize;

/// Key type for storage
pub type Key = Vec<u8>;