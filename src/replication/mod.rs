//! Replication module for ferrous
//! 
//! Implements Redis-compatible master-slave replication including:
//! - REPLICAOF command support
//! - Full synchronization via RDB
//! - Incremental sync (PSYNC)
//! - Command propagation
//! - Replication backlog

mod manager;
pub mod sync; // Make the sync module public
pub mod commands;
mod backlog;
mod client;

pub use manager::{ReplicationManager, ReplicationRole, ReplicationState, MasterLinkStatus};
pub use sync::{SyncProtocol, PsyncResult}; // Re-export SyncProtocol
pub use commands::{handle_replicaof, handle_replconf, handle_slaveof, handle_sync, handle_psync};
pub use backlog::ReplicationBacklog;
pub use client::{start_background_replication, ReplicationClientConfig};

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use std::net::SocketAddr;

/// Replication configuration
#[derive(Debug, Clone)]
pub struct ReplicationConfig {
    /// Maximum size of the replication backlog
    pub backlog_size: usize,
    
    /// Replication timeout (in seconds)
    pub timeout: u64,
    
    /// Ping replica period (in seconds)
    pub ping_replica_period: u64,
    
    /// Enable diskless replication
    pub diskless_sync: bool,
    
    /// Master host to replicate from (if this is a replica)
    pub master_host: Option<String>,
    
    /// Master port to replicate from (if this is a replica)
    pub master_port: Option<u16>,
}

impl Default for ReplicationConfig {
    fn default() -> Self {
        ReplicationConfig {
            backlog_size: 1_048_576, // 1MB default
            timeout: 60,
            ping_replica_period: 10,
            diskless_sync: false,
            master_host: None,
            master_port: None,
        }
    }
}

/// Information about a connected replica
#[derive(Debug)]
pub struct ReplicaInfo {
    /// Connection ID of the replica
    pub conn_id: u64,
    
    /// Address of the replica
    pub addr: SocketAddr,
    
    /// Replication offset acknowledged by the replica
    pub ack_offset: AtomicU64,
    
    /// Last interaction time
    pub last_interaction: Mutex<Instant>,
    
    /// Replica capabilities
    pub capabilities: Vec<String>,
}

impl ReplicaInfo {
    pub fn new(conn_id: u64, addr: SocketAddr) -> Arc<Self> {
        Arc::new(ReplicaInfo {
            conn_id,
            addr,
            ack_offset: AtomicU64::new(0),
            last_interaction: Mutex::new(Instant::now()),
            capabilities: Vec::new(),
        })
    }
    
    /// Update last interaction time
    pub fn touch(&self) {
        *self.last_interaction.lock().unwrap() = Instant::now();
    }
    
    /// Get time since last interaction
    pub fn idle_time(&self) -> std::time::Duration {
        self.last_interaction.lock().unwrap().elapsed()
    }
    
    /// Update acknowledged offset
    pub fn update_ack_offset(&self, offset: u64) {
        self.ack_offset.store(offset, Ordering::SeqCst);
    }
}

/// Generate a unique replication ID
pub fn generate_repl_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..40)
        .map(|_| {
            let n = rng.gen_range(0..62);
            match n {
                0..=9 => (b'0' + n) as u8,
                10..=35 => (b'a' + n - 10) as u8,
                36..=61 => (b'A' + n - 36) as u8,
                _ => unreachable!(),
            }
        })
        .collect();
    
    String::from_utf8(bytes).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_generate_repl_id() {
        let id = generate_repl_id();
        assert_eq!(id.len(), 40);
        assert!(id.chars().all(|c| c.is_ascii_alphanumeric()));
    }
    
    #[test]
    fn test_replica_info() {
        let addr = "127.0.0.1:6379".parse().unwrap();
        let replica = ReplicaInfo::new(1, addr);
        
        assert_eq!(replica.conn_id, 1);
        assert_eq!(replica.ack_offset.load(Ordering::SeqCst), 0);
        
        replica.update_ack_offset(100);
        assert_eq!(replica.ack_offset.load(Ordering::SeqCst), 100);
    }
}