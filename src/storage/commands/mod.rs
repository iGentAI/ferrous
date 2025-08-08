//! Command handler modules
//! 
//! This module organizes all Redis command implementations by data type.

pub mod lists;
pub mod sets;
pub mod hashes;
pub mod strings;
pub mod transactions;
pub mod aof;
pub mod monitor;
pub mod config;
pub mod scan;
pub mod slowlog;
pub mod debug;
pub mod monitor_cmd;
pub mod client;
pub mod memory;
pub mod lua;          // MLua-based Lua 5.1 scripting
pub mod streams;
pub mod consumer_groups;
pub mod executor;

// Re-export all handlers for easy access
       // Export new MLua-based Lua commands
      // Export stream commands
 // Export consumer group commands

// Export unified command processing
