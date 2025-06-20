//! Monitoring command implementations
//! 
//! Provides Redis-compatible monitoring commands including INFO, MONITOR, and SLOWLOG.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use std::process;
use std::fmt::Write;
use crate::error::Result;
use crate::protocol::RespFrame;
use crate::storage::StorageEngine;
use crate::network::server::ServerStats;
use crate::replication::ReplicationManager;

/// Handle INFO command
pub fn handle_info(
    storage: &Arc<StorageEngine>, 
    stats: &Arc<ServerStats>,
    start_time: SystemTime,
    connected_clients: usize,
    max_clients: usize,
    replication: &Arc<ReplicationManager>,
    parts: &[RespFrame]
) -> Result<RespFrame> {
    // Parse section filter if provided
    let section = if parts.len() > 1 {
        match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => {
                Some(String::from_utf8_lossy(bytes).to_lowercase())
            }
            _ => None,
        }
    } else {
        None
    };
    
    let mut info_output = String::new();
    let show_all = section.is_none();
    
    // Server section
    if show_all || section.as_deref() == Some("server") {
        append_server_info(&mut info_output, start_time);
    }
    
    // Clients section
    if show_all || section.as_deref() == Some("clients") {
        append_clients_info(&mut info_output, connected_clients, max_clients, stats);
    }
    
    // Memory section
    if show_all || section.as_deref() == Some("memory") {
        append_memory_info(&mut info_output, storage, stats);
    }
    
    // Stats section
    if show_all || section.as_deref() == Some("stats") {
        append_stats_info(&mut info_output, stats, start_time);
    }
    
    // Replication section
    if show_all || section.as_deref() == Some("replication") {
        append_replication_info(&mut info_output, replication);
    }
    
    // CPU section
    if show_all || section.as_deref() == Some("cpu") {
        append_cpu_info(&mut info_output);
    }
    
    // Keyspace section
    if show_all || section.as_deref() == Some("keyspace") {
        append_keyspace_info(&mut info_output, storage);
    }
    
    Ok(RespFrame::from_string(info_output))
}

fn append_server_info(output: &mut String, start_time: SystemTime) {
    writeln!(output, "# Server").unwrap();
    writeln!(output, "redis_version:7.0.0-ferrous").unwrap();
    writeln!(output, "ferrous_version:{}", env!("CARGO_PKG_VERSION")).unwrap();
    writeln!(output, "redis_mode:standalone").unwrap();
    writeln!(output, "process_id:{}", process::id()).unwrap();
    
    // Calculate uptime
    let uptime = start_time.elapsed().unwrap_or_default().as_secs();
    writeln!(output, "uptime_in_seconds:{}", uptime).unwrap();
    writeln!(output, "uptime_in_days:{}", uptime / 86400).unwrap();
    writeln!(output, "rust_version:{}", rustc_version()).unwrap();
    writeln!(output, "").unwrap();
}

fn append_clients_info(output: &mut String, connected_clients: usize, max_clients: usize, stats: &Arc<ServerStats>) {
    writeln!(output, "# Clients").unwrap();
    writeln!(output, "connected_clients:{}", connected_clients).unwrap();
    writeln!(output, "blocked_clients:{}", stats.blocked_clients.load(Ordering::Relaxed)).unwrap();
    writeln!(output, "maxclients:{}", max_clients).unwrap();
    writeln!(output, "").unwrap();
}

fn append_memory_info(output: &mut String, storage: &Arc<StorageEngine>, stats: &Arc<ServerStats>) {
    writeln!(output, "# Memory").unwrap();
    
    // Get memory stats from storage engine
    let used_memory = storage.memory_usage();
    let peak_memory = stats.peak_memory.load(Ordering::Relaxed);
    
    // Update peak memory if current is higher
    if used_memory > peak_memory {
        stats.peak_memory.store(used_memory, Ordering::Relaxed);
    }
    
    // Memory usage metrics
    writeln!(output, "used_memory:{}", used_memory).unwrap();
    writeln!(output, "used_memory_human:{}", format_bytes(used_memory)).unwrap();
    writeln!(output, "used_memory_peak:{}", peak_memory).unwrap();
    writeln!(output, "used_memory_peak_human:{}", format_bytes(peak_memory)).unwrap();
    
    // Memory fragmentation ratio (simplified - actual RSS not available)
    writeln!(output, "mem_fragmentation_ratio:1.0").unwrap();
    writeln!(output, "").unwrap();
}

fn append_stats_info(output: &mut String, stats: &Arc<ServerStats>, start_time: SystemTime) {
    writeln!(output, "# Stats").unwrap();
    
    writeln!(
        output,
        "total_connections_received:{}",
        stats.total_connections_received.load(Ordering::Relaxed)
    ).unwrap();
    writeln!(
        output,
        "total_commands_processed:{}",
        stats.total_commands_processed.load(Ordering::Relaxed)
    ).unwrap();
    
    // Calculate operations per second
    let uptime_secs = start_time.elapsed().unwrap_or_default().as_secs();
    let ops_per_sec = if uptime_secs > 0 {
        stats.total_commands_processed.load(Ordering::Relaxed) / uptime_secs
    } else {
        0
    };
    writeln!(output, "instantaneous_ops_per_sec:{}", ops_per_sec).unwrap();
    
    writeln!(
        output,
        "keyspace_hits:{}",
        stats.keyspace_hits.load(Ordering::Relaxed)
    ).unwrap();
    writeln!(
        output,
        "keyspace_misses:{}",
        stats.keyspace_misses.load(Ordering::Relaxed)
    ).unwrap();
    
    // Hit rate calculation
    let hits = stats.keyspace_hits.load(Ordering::Relaxed);
    let misses = stats.keyspace_misses.load(Ordering::Relaxed);
    let total_access = hits + misses;
    let hit_rate = if total_access > 0 {
        (hits as f64 / total_access as f64) * 100.0
    } else {
        0.0
    };
    writeln!(output, "keyspace_hit_rate:{:.2}", hit_rate).unwrap();
    
    // Authentication statistics
    writeln!(
        output,
        "auth_successes:{}",
        stats.auth_successes.load(Ordering::Relaxed)
    ).unwrap();
    writeln!(
        output,
        "auth_failures:{}",
        stats.auth_failures.load(Ordering::Relaxed)
    ).unwrap();
    
    writeln!(output, "").unwrap();
}

fn append_replication_info(output: &mut String, replication: &Arc<ReplicationManager>) {
    writeln!(output, "# Replication").unwrap();
    let repl_info = replication.get_info();
    for (key, value) in repl_info {
        writeln!(output, "{}:{}", key, value).unwrap();
    }
    writeln!(output, "").unwrap();
}

fn append_cpu_info(output: &mut String) {
    writeln!(output, "# CPU").unwrap();
    
    // Note: Detailed CPU usage would require platform-specific code
    // For simplicity, we just report placeholder values
    writeln!(output, "used_cpu_sys:0.00").unwrap();
    writeln!(output, "used_cpu_user:0.00").unwrap();
    writeln!(output, "used_cpu_sys_children:0.00").unwrap();
    writeln!(output, "used_cpu_user_children:0.00").unwrap();
    
    writeln!(output, "").unwrap();
}

fn append_keyspace_info(output: &mut String, storage: &Arc<StorageEngine>) {
    writeln!(output, "# Keyspace").unwrap();
    
    // Get statistics for each database
    for db in 0..storage.database_count() {
        if let Ok(keys) = storage.get_all_keys(db) {
            if !keys.is_empty() {
                let total_keys = keys.len();
                let mut expiring_keys = 0;
                
                // Count keys with expiration
                for key in &keys {
                    if let Ok(Some(_)) = storage.ttl(db, key) {
                        expiring_keys += 1;
                    }
                }
                
                writeln!(
                    output,
                    "db{}:keys={},expires={}",
                    db, total_keys, expiring_keys
                ).unwrap();
            }
        }
    }
}

/// Format bytes in human-readable form
fn format_bytes(bytes: usize) -> String {
    const UNITS: &[&str] = &["B", "K", "M", "G", "T"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    
    if unit_idx == 0 {
        format!("{}{}", bytes, UNITS[unit_idx])
    } else {
        format!("{:.2}{}", size, UNITS[unit_idx])
    }
}

/// Get the Rust version used to compile this binary
fn rustc_version() -> &'static str {
    // Get Rust version from environment variables set during compilation
    // This is a simplification - in practice you might want to use the `rustc_version` crate
    // or capture this information during build
    env!("CARGO_PKG_RUST_VERSION", "unknown")
}