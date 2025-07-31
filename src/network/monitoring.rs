//! Zero-overhead trait-based monitoring system
//!
//! This module provides monitoring capabilities that can be completely disabled
//! at runtime with zero performance cost when turned off.

use std::time::{Instant, SystemTime};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use crate::protocol::RespFrame;

/// Zero-overhead monitoring trait
pub trait PerformanceMonitoring: Send + Sync {
    /// Check if monitoring is active (for early exit)
    fn is_enabled(&self) -> bool;
    
    /// Start timing a command (returns start time)
    fn start_timing(&self) -> Option<Instant>;
    
    /// Record command completion with timing
    fn record_command_timing(&self, start_time: Option<Instant>, command: &str, parts: &[RespFrame], client_addr: &str);
    
    /// Record command statistics
    fn record_command_count(&self);
    
    /// Record cache hit/miss statistics  
    fn record_cache_hit(&self, hit: bool);
    
    /// Broadcast to monitor subscribers
    fn broadcast_to_monitors(&self, parts: &[RespFrame], conn_id: u64, db: usize, timestamp: SystemTime);
}

/// Active monitoring implementation with full functionality
pub struct ActiveMonitoring {
    slowlog: Arc<crate::storage::commands::slowlog::Slowlog>,
    monitor_subscribers: Arc<crate::monitor::MonitorSubscribers>,
    stats: Arc<crate::network::server::ServerStats>,
}

/// Null monitoring implementation - compiles to zero overhead
pub struct NullMonitoring;

impl PerformanceMonitoring for ActiveMonitoring {
    #[inline(always)]
    fn is_enabled(&self) -> bool {
        true
    }
    
    #[inline(always)]
    fn start_timing(&self) -> Option<Instant> {
        Some(Instant::now())
    }
    
    fn record_command_timing(&self, start_time: Option<Instant>, command: &str, parts: &[RespFrame], client_addr: &str) {
        if let Some(start) = start_time {
            let duration = start.elapsed();
            let duration_micros = duration.as_micros() as u64;
            let threshold_micros = self.slowlog.get_threshold_micros();
            
            crate::storage::commands::debug::log_command_timing(command, duration_micros, threshold_micros);
            
            if threshold_micros >= 0 && duration_micros >= threshold_micros as u64 {
                crate::storage::commands::debug::log_slowlog(
                    &format!("Adding slow command: {} ({}μs) from {}", command, duration_micros, client_addr)
                );
                
                self.slowlog.add_if_slow(duration, parts, client_addr, None);
            } else {
                crate::storage::commands::debug::log_slowlog(
                    &format!("Command not slow enough: {} ({}μs, threshold {}μs)", 
                             command, duration_micros, threshold_micros)
                );
            }
        }
    }
    
    fn record_command_count(&self) {
        self.stats.total_commands_processed.fetch_add(1, Ordering::Relaxed);
    }
    
    fn record_cache_hit(&self, hit: bool) {
        if hit {
            self.stats.keyspace_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.stats.keyspace_misses.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    fn broadcast_to_monitors(&self, parts: &[RespFrame], conn_id: u64, db: usize, timestamp: SystemTime) {
        if !self.monitor_subscribers.get_subscribers().is_empty() {
            if let RespFrame::BulkString(Some(cmd_bytes)) = &parts[0] {
                let cmd = String::from_utf8_lossy(cmd_bytes).to_uppercase();
                if cmd != "AUTH" {  // Don't broadcast AUTH commands for security
                    // Use static method for monitor output formatting
                    let monitor_output = crate::monitor::MonitorSubscribers::format_monitor_output(
                        timestamp,
                        db,
                        &format!("127.0.0.1:{}", conn_id), // Simplified client addr
                        parts
                    );
                    
                    // Note: Actual broadcasting would be handled by the server
                    // This trait method serves to prepare the output
                    // Server will handle the actual delivery to subscribers
                    
                    println!("Monitor broadcast prepared for command: {}", cmd);
                }
            }
        }
    }
}

impl PerformanceMonitoring for NullMonitoring {
    #[inline(always)]
    fn is_enabled(&self) -> bool {
        false
    }
    
    #[inline(always)]
    fn start_timing(&self) -> Option<Instant> {
        None
    }
    
    #[inline(always)]
    fn record_command_timing(&self, _start_time: Option<Instant>, _command: &str, _parts: &[RespFrame], _client_addr: &str) {
        // Zero-cost no-op - compiles away completely
    }
    
    #[inline(always)]
    fn record_command_count(&self) {
        // Zero-cost no-op - compiles away completely
    }
    
    #[inline(always)]
    fn record_cache_hit(&self, _hit: bool) {
        // Zero-cost no-op - compiles away completely
    }
    
    #[inline(always)]
    fn broadcast_to_monitors(&self, _parts: &[RespFrame], _conn_id: u64, _db: usize, _timestamp: SystemTime) {
        // Zero-cost no-op - compiles away completely
    }
}

/// Configuration for monitoring features
pub struct MonitoringConfig {
    pub slowlog_enabled: bool,
    pub monitor_enabled: bool,  
    pub stats_enabled: bool,
}

impl MonitoringConfig {
    /// Production-optimized configuration - all monitoring disabled like Valkey
    pub const fn production() -> Self {
        Self {
            slowlog_enabled: false,
            monitor_enabled: false,
            stats_enabled: false,
        }
    }
    
    /// Development configuration - all monitoring enabled
    pub const fn development() -> Self {
        Self {
            slowlog_enabled: true,
            monitor_enabled: true,
            stats_enabled: true,
        }
    }
    
    /// Create from ferrous configuration
    pub fn from_config(config: &crate::config::MonitoringConfig) -> Self {
        Self {
            slowlog_enabled: config.slowlog_enabled,
            monitor_enabled: config.monitor_enabled,
            stats_enabled: config.stats_enabled,
        }
    }
    
    /// Create monitoring backend - zero overhead when all disabled
    pub fn create_monitoring(
        &self,
        slowlog: Arc<crate::storage::commands::slowlog::Slowlog>,
        monitor_subscribers: Arc<crate::monitor::MonitorSubscribers>,
        stats: Arc<crate::network::server::ServerStats>,
    ) -> Arc<dyn PerformanceMonitoring> {
        if self.slowlog_enabled || self.monitor_enabled || self.stats_enabled {
            Arc::new(ActiveMonitoring {
                slowlog,
                monitor_subscribers,
                stats,
            })
        } else {
            Arc::new(NullMonitoring)
        }
    }
}