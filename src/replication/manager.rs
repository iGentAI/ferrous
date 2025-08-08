//! Replication manager - coordinates all replication activities

use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;
use std::thread;
use crate::error::{FerrousError, Result};
use crate::protocol::RespFrame;
use crate::storage::StorageEngine;
use super::{ReplicaInfo, ReplicationConfig, ReplicationBacklog, generate_repl_id};
use super::client::{start_background_replication, ReplicationClientConfig};

/// The role of the server in replication
#[derive(Debug, Clone)]
pub enum ReplicationRole {
    /// This server is a master
    Master {
        /// Replication ID
        repl_id: String,
        
        /// Second replication ID (for PSYNC2)
        repl_id2: String,
        
        /// Replication offset
        repl_offset: Arc<AtomicU64>,
        
        /// Second replication offset 
        repl_offset2: Arc<AtomicU64>,
    },
    
    /// This server is a replica
    Replica {
        /// Master address
        master_addr: SocketAddr,
        
        /// Master link status
        master_link_status: MasterLinkStatus,
        
        /// Master replication ID
        master_repl_id: String,
        
        /// Replication offset
        repl_offset: u64,
    },
}

impl PartialEq for ReplicationRole {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Master { repl_id: id1, .. }, Self::Master { repl_id: id2, .. }) => id1 == id2,
            (Self::Replica { master_addr: addr1, .. }, Self::Replica { master_addr: addr2, .. }) => addr1 == addr2,
            _ => false,
        }
    }
}

/// Master link status for replicas
#[derive(Debug, Clone, PartialEq)]
pub enum MasterLinkStatus {
    /// Connecting to master
    Connecting,
    
    /// Performing synchronization
    Synchronizing,
    
    /// Link is up and replication is active
    Up,
    
    /// Link is down
    Down,
}

/// Current replication state
#[derive(Debug)]
pub enum ReplicationState {
    /// Not replicating
    None,
    
    /// Waiting for BGSAVE to start
    WaitBgsaveStart,
    
    /// Waiting for BGSAVE to end
    WaitBgsaveEnd,
    
    /// Sending RDB file
    SendingRdb,
    
    /// Online - normal replication
    Online,
}

/// Main replication manager
pub struct ReplicationManager {
    /// Current role (master or replica)
    role: Arc<RwLock<ReplicationRole>>,
    
    /// Replication configuration
    config: ReplicationConfig,
    
    /// Connected replicas (for master)
    replicas: Arc<Mutex<HashMap<u64, Arc<ReplicaInfo>>>>,
    
    /// Replication backlog (for master)
    backlog: Arc<ReplicationBacklog>,
    
    /// Current replication state
    state: Arc<Mutex<ReplicationState>>,
    
    /// Flag to pause replication
    paused: AtomicBool,
    
    /// Master connection ID (for replica)
    master_conn_id: Arc<Mutex<Option<u64>>>,
    
    /// Handle to stop background replication
    replication_stop_flag: Arc<Mutex<Option<Arc<AtomicBool>>>>,
}

impl ReplicationManager {
    /// Create a new replication manager
    pub fn new(config: ReplicationConfig) -> Arc<Self> {
        let backlog_size = config.backlog_size;
        
        Arc::new(ReplicationManager {
            role: Arc::new(RwLock::new(ReplicationRole::Master {
                repl_id: generate_repl_id(),
                repl_id2: "0000000000000000000000000000000000000000".to_string(),
                repl_offset: Arc::new(AtomicU64::new(0)),
                repl_offset2: Arc::new(AtomicU64::new(0)),
            })),
            config,
            replicas: Arc::new(Mutex::new(HashMap::new())),
            backlog: Arc::new(ReplicationBacklog::new(backlog_size)),
            state: Arc::new(Mutex::new(ReplicationState::None)),
            paused: AtomicBool::new(false),
            master_conn_id: Arc::new(Mutex::new(None)),
            replication_stop_flag: Arc::new(Mutex::new(None)),
        })
    }
    
    /// Get current role
    pub fn role(&self) -> ReplicationRole {
        self.role.read().unwrap().clone()
    }
    
    /// Check if this server is a master
    pub fn is_master(&self) -> bool {
        matches!(*self.role.read().unwrap(), ReplicationRole::Master { .. })
    }
    
    /// Check if this server is a replica
    pub fn is_replica(&self) -> bool {
        matches!(*self.role.read().unwrap(), ReplicationRole::Replica { .. })
    }
    
    /// Get current replication offset
    pub fn get_repl_offset(&self) -> u64 {
        match &*self.role.read().unwrap() {
            ReplicationRole::Master { repl_offset, .. } => {
                repl_offset.load(Ordering::SeqCst)
            }
            ReplicationRole::Replica { repl_offset, .. } => *repl_offset,
        }
    }
    
    /// Set master (for REPLICAOF command)
    pub fn set_master(&self, master_addr: Option<SocketAddr>, storage: Arc<StorageEngine>) -> Result<()> {
        // Stop any existing replication
        {
            let mut stop_flag = self.replication_stop_flag.lock().unwrap();
            if let Some(flag) = stop_flag.take() {
                flag.store(true, Ordering::SeqCst);
                // Give the replication thread time to stop
                thread::sleep(Duration::from_millis(100));
            }
        }
        
        let mut role = self.role.write().unwrap();
        
        match master_addr {
            Some(addr) => {
                // Becoming a replica
                *role = ReplicationRole::Replica {
                    master_addr: addr,
                    master_link_status: MasterLinkStatus::Connecting,
                    master_repl_id: String::new(),
                    repl_offset: 0,
                };
                
                // Clear connected replicas
                self.replicas.lock().unwrap().clear();
                
                // Reset state
                *self.state.lock().unwrap() = ReplicationState::None;
                
                // Start background replication
                let config = ReplicationClientConfig::default();
                let stop_flag = start_background_replication(
                    addr,
                    config,
                    Arc::new(self.clone()),
                    Arc::clone(&storage),
                );
                
                // Store stop flag
                *self.replication_stop_flag.lock().unwrap() = Some(stop_flag);
                
                Ok(())
            }
            None => {
                // Becoming a master (REPLICAOF NO ONE)
                *role = ReplicationRole::Master {
                    repl_id: generate_repl_id(),
                    repl_id2: "0000000000000000000000000000000000000000".to_string(),
                    repl_offset: Arc::new(AtomicU64::new(0)),
                    repl_offset2: Arc::new(AtomicU64::new(0)),
                };
                
                // Reset state
                *self.state.lock().unwrap() = ReplicationState::None;
                
                Ok(())
            }
        }
    }
    
    /// Add a replica (for master)
    pub fn add_replica(&self, replica: Arc<ReplicaInfo>) -> Result<()> {
        if !self.is_master() {
            return Err(FerrousError::Command(
                crate::error::CommandError::Generic("not a master".into())
            ));
        }
        
        let mut replicas = self.replicas.lock().unwrap();
        replicas.insert(replica.conn_id, replica);
        
        Ok(())
    }
    
    /// Remove a replica (for master)
    pub fn remove_replica(&self, conn_id: u64) -> Option<Arc<ReplicaInfo>> {
        let mut replicas = self.replicas.lock().unwrap();
        replicas.remove(&conn_id)
    }
    
    /// Get all connected replicas (for master)
    pub fn get_replicas(&self) -> Vec<Arc<ReplicaInfo>> {
        let replicas = self.replicas.lock().unwrap();
        replicas.values().cloned().collect()
    }
    
    /// Propagate command to all replicas (for master)
    pub fn propagate_command(&self, cmd_frame: &RespFrame) -> Result<Vec<u64>> {
        if !self.is_master() {
            return Ok(Vec::new());
        }
        
        // Add to backlog
        self.backlog.append_command(cmd_frame)?;
        
        // Get current replication offset
        let offset = match &*self.role.read().unwrap() {
            ReplicationRole::Master { repl_offset, .. } => {
                repl_offset.fetch_add(1, Ordering::SeqCst)
            }
            _ => return Ok(Vec::new()),
        };
        
        // Get replica connection IDs
        let replicas = self.replicas.lock().unwrap();
        let replica_ids: Vec<u64> = replicas.keys().cloned().collect();
        
        Ok(replica_ids)
    }
    
    /// Update master link status (for replica)
    pub fn update_master_link_status(&self, status: MasterLinkStatus) -> Result<()> {
        let mut role = self.role.write().unwrap();
        
        match &mut *role {
            ReplicationRole::Replica { master_link_status, .. } => {
                *master_link_status = status;
                Ok(())
            }
            _ => Err(FerrousError::Command(
                crate::error::CommandError::Generic("not a replica".into())
            )),
        }
    }
    
    /// Update replication offset (for replica)
    pub fn update_replica_offset(&self, offset: u64, master_repl_id: Option<String>) -> Result<()> {
        let mut role = self.role.write().unwrap();
        
        match &mut *role {
            ReplicationRole::Replica { repl_offset, master_repl_id: stored_id, .. } => {
                *repl_offset = offset;
                if let Some(id) = master_repl_id {
                    *stored_id = id;
                }
                Ok(())
            }
            _ => Err(FerrousError::Command(
                crate::error::CommandError::Generic("not a replica".into())
            )),
        }
    }
    
    /// Check if replication is paused
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }
    
    /// Pause replication
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }
    
    /// Resume replication
    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }
    
    /// Get replication info for INFO command
    pub fn get_info(&self) -> HashMap<String, String> {
        let mut info = HashMap::new();
        
        match &*self.role.read().unwrap() {
            ReplicationRole::Master { repl_id, repl_offset, .. } => {
                info.insert("role".to_string(), "master".to_string());
                info.insert("repl_id".to_string(), repl_id.clone());
                info.insert("repl_offset".to_string(), 
                    repl_offset.load(Ordering::SeqCst).to_string());
                
                let replicas = self.replicas.lock().unwrap();
                info.insert("connected_slaves".to_string(), replicas.len().to_string());
                
                // Add replica info
                for (idx, (_, replica)) in replicas.iter().enumerate() {
                    let key = format!("slave{}", idx);
                    let value = format!("ip={},port=0,state=online,offset={},lag=0",
                        replica.addr.ip(),
                        replica.ack_offset.load(Ordering::SeqCst));
                    info.insert(key, value);
                }
            }
            ReplicationRole::Replica { master_addr, master_link_status, repl_offset, .. } => {
                info.insert("role".to_string(), "slave".to_string());
                info.insert("master_host".to_string(), master_addr.ip().to_string());
                info.insert("master_port".to_string(), master_addr.port().to_string());
                info.insert("master_link_status".to_string(), 
                    match master_link_status {
                        MasterLinkStatus::Up => "up",
                        MasterLinkStatus::Down => "down",
                        MasterLinkStatus::Connecting => "connecting",
                        MasterLinkStatus::Synchronizing => "sync",
                    }.to_string()
                );
                info.insert("slave_repl_offset".to_string(), repl_offset.to_string());
            }
        }
        
        info
    }
    
    /// Get backlog data from a specific offset
    pub fn get_backlog_data(&self, offset: u64) -> Result<Vec<u8>> {
        self.backlog.get_data_from_offset(offset)
    }
}

impl Clone for ReplicationManager {
    fn clone(&self) -> Self {
        Self {
            role: Arc::clone(&self.role),
            config: self.config.clone(),
            replicas: Arc::clone(&self.replicas),
            backlog: Arc::clone(&self.backlog),
            state: Arc::clone(&self.state),
            paused: AtomicBool::new(self.paused.load(Ordering::SeqCst)),
            master_conn_id: Arc::clone(&self.master_conn_id),
            replication_stop_flag: Arc::clone(&self.replication_stop_flag),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_replication_manager_creation() {
        let config = ReplicationConfig::default();
        let manager = ReplicationManager::new(config);
        
        assert!(manager.is_master());
        assert!(!manager.is_replica());
    }
    
    #[test]
    fn test_set_master() {
        let config = ReplicationConfig::default();
        let manager = ReplicationManager::new(config);
        let storage = Arc::new(StorageEngine::new());
        
        // Set as replica
        let addr = "127.0.0.1:6379".parse().unwrap();
        manager.set_master(Some(addr), Arc::clone(&storage)).unwrap();
        assert!(manager.is_replica());
        
        // Set back to master
        manager.set_master(None, Arc::clone(&storage)).unwrap();
        assert!(manager.is_master());
    }
}