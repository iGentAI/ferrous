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

// Re-export all handlers for easy access
pub use lists::*;
pub use sets::*;
pub use hashes::*;
pub use strings::*;
pub use transactions::*;
pub use aof::*;
pub use monitor::*;
pub use config::*;
pub use scan::*;
pub use slowlog::*;
pub use debug::*;
pub use monitor_cmd::*;
pub use client::*;
pub use memory::*;
pub use lua::*;       // Export new MLua-based Lua commands