//! Debug utilities for development and troubleshooting
//! 
//! This module provides utilities for debugging and logging during development.
//! It can be compiled out in production builds.

// Feature flag for enabling/disabling debug output
// #[cfg(feature = "debug")]
pub const DEBUG_ENABLED: bool = true;
// #[cfg(not(feature = "debug"))]
// pub const DEBUG_ENABLED: bool = false;

/// Print debug message with module name and line number
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        if $crate::storage::commands::debug::DEBUG_ENABLED {
            println!("DEBUG [{}:{}]: {}", file!(), line!(), format!($($arg)*));
        }
    }
}

/// Print debug message specifically for the SLOWLOG feature
pub fn log_slowlog(message: &str) {
    if DEBUG_ENABLED {
        println!("SLOWLOG DEBUG: {}", message);
    }
}

/// Log command timing information for debugging
pub fn log_command_timing(command: &str, duration_micros: u64, threshold_micros: i64) {
    if DEBUG_ENABLED {
        println!("TIMING: Command '{}' took {}μs (threshold: {}μs)", 
                 command, duration_micros, threshold_micros);
    }
}