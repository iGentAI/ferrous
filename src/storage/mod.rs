//! Storage engine for Ferrous
//! 
//! This module provides the core data structures and storage functionality
//! for Redis-compatible data types.

pub mod engine;
pub mod value;
pub mod memory;
pub mod skiplist;
pub mod stream;
pub mod consumer_groups;
pub mod rdb;
pub mod monitor;
pub mod aof;
pub mod commands;
pub mod lua_cache;

#[cfg(test)]
mod stream_integration_tests;

pub use engine::{StorageEngine, GetResult};
pub use value::{Value, ValueType, StringEncoding, StoredValue, ValueMetadata};
pub use memory::{MemoryManager, EvictionPolicy};
pub use skiplist::SkipList;
pub use rdb::{RdbEngine, RdbConfig};
pub use monitor::StorageMonitor;
pub use aof::{AofEngine, AofConfig};
pub use stream::{Stream, StreamId, StreamEntry};
// Consumer groups will be exported once fully implemented:
// pub use consumer_groups::{ConsumerGroup, Consumer, ConsumerGroupManager};

/// Database index type
pub type DatabaseIndex = usize;

/// Key type for storage
pub type Key = Vec<u8>;