//! Blocking operations implementation for BLPOP/BRPOP commands
//! 
//! This module implements the zero-overhead blocking subsystem that enables
//! Redis blocking list operations while maintaining Ferrous's excellent performance.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock, Mutex};
use std::time::Instant;
use crossbeam::queue::SegQueue;
use crate::error::Result;
use crate::storage::DatabaseIndex;
use super::connection::{BlockingOp};

/// Information about a blocked client
#[derive(Debug, Clone)]
pub struct BlockedClient {
    pub conn_id: u64,
    pub blocked_at: Instant,
    pub deadline: Option<Instant>,
    pub op_type: BlockingOp,
}

/// Wake-up request for blocked clients
#[derive(Debug)]
pub struct WakeupRequest {
    pub conn_id: u64,
    pub db: DatabaseIndex,
    pub key: Vec<u8>,
    pub op_type: BlockingOp,
}

/// Per-database blocking registry
pub struct BlockingRegistry {
    /// Map of key -> waiting clients (ordered by arrival time)
    blocked_on_key: HashMap<Vec<u8>, VecDeque<BlockedClient>>,
    /// Quick lookup for which keys have blocked clients
    blocked_keys: std::collections::HashSet<Vec<u8>>,
}

impl BlockingRegistry {
    pub fn new() -> Self {
        Self {
            blocked_on_key: HashMap::new(),
            blocked_keys: std::collections::HashSet::new(),
        }
    }
    
    /// Register a client as blocked on a set of keys
    pub fn register_blocked_client(&mut self, client: BlockedClient, keys: &[(DatabaseIndex, Vec<u8>)]) {
        for (_db, key) in keys {
            self.blocked_on_key
                .entry(key.clone())
                .or_insert_with(VecDeque::new)
                .push_back(client.clone());
            self.blocked_keys.insert(key.clone());
        }
    }
    
    /// Remove a client from all blocked keys (when connection closes or times out)
    pub fn unregister_client(&mut self, conn_id: u64) {
        let mut keys_to_remove = Vec::new();
        
        for (key, clients) in self.blocked_on_key.iter_mut() {
            clients.retain(|client| client.conn_id != conn_id);
            if clients.is_empty() {
                keys_to_remove.push(key.clone());
            }
        }
        
        for key in keys_to_remove {
            self.blocked_on_key.remove(&key);
            self.blocked_keys.remove(&key);
        }
    }
    
    /// Check if any clients are blocked on a key
    pub fn has_blocked_clients(&self, key: &[u8]) -> bool {
        self.blocked_keys.contains(key)
    }
    
    /// Pop the first waiting client for a key
    pub fn pop_first_waiter(&mut self, key: &[u8]) -> Option<BlockedClient> {
        if let Some(clients) = self.blocked_on_key.get_mut(key) {
            let client = clients.pop_front();
            if clients.is_empty() {
                self.blocked_on_key.remove(key);
                self.blocked_keys.remove(key);
            }
            client
        } else {
            None
        }
    }
    
    /// Get expired clients based on current time
    pub fn get_expired_clients(&mut self, now: Instant) -> Vec<u64> {
        let mut expired = Vec::new();
        let mut keys_to_update = Vec::new();
        
        for (key, clients) in self.blocked_on_key.iter_mut() {
            let mut expired_in_key = Vec::new();
            
            // Find expired clients in this key's queue
            for (i, client) in clients.iter().enumerate() {
                if let Some(deadline) = client.deadline {
                    if now >= deadline {
                        expired_in_key.push(i);
                    }
                }
            }
            
            // Remove expired clients (in reverse order to maintain indices)
            for &i in expired_in_key.iter().rev() {
                if let Some(client) = clients.remove(i) {
                    expired.push(client.conn_id);
                }
            }
            
            if clients.is_empty() {
                keys_to_update.push(key.clone());
            }
        }
        
        // Clean up empty key entries
        for key in keys_to_update {
            self.blocked_on_key.remove(&key);
            self.blocked_keys.remove(&key);
        }
        
        expired
    }
}

/// Global blocking manager
pub struct BlockingManager {
    /// Per-database registries
    registries: Vec<RwLock<BlockingRegistry>>,
    /// Lock-free queue for wake-up requests
    wake_queue: Arc<SegQueue<WakeupRequest>>,
    /// Shutdown signal
    shutdown: Arc<Mutex<bool>>,
}

impl BlockingManager {
    pub fn new(num_databases: usize) -> Self {
        let mut registries = Vec::with_capacity(num_databases);
        for _ in 0..num_databases {
            registries.push(RwLock::new(BlockingRegistry::new()));
        }
        
        Self {
            registries,
            wake_queue: Arc::new(SegQueue::new()),
            shutdown: Arc::new(Mutex::new(false)),
        }
    }
    
    /// Register a client as blocked on multiple keys
    pub fn register_blocked(&self, db: DatabaseIndex, conn_id: u64, keys: Vec<Vec<u8>>, op_type: BlockingOp, deadline: Option<Instant>) -> Result<()> {
        if db >= self.registries.len() {
            return Err(crate::error::FerrousError::Storage(crate::error::StorageError::InvalidDatabase));
        }
        
        let client = BlockedClient {
            conn_id,
            blocked_at: Instant::now(),
            deadline,
            op_type,
        };
        
        let keys_with_db: Vec<(DatabaseIndex, Vec<u8>)> = keys.into_iter().map(|k| (db, k)).collect();
        
        let mut registry = self.registries[db].write().unwrap();
        registry.register_blocked_client(client, &keys_with_db);
        
        Ok(())
    }
    
    /// Unregister a client from all blocked operations
    pub fn unregister_client(&self, db: DatabaseIndex, conn_id: u64) -> Result<()> {
        if db >= self.registries.len() {
            return Ok(()); // Invalid DB, nothing to unregister
        }
        
        let mut registry = self.registries[db].write().unwrap();
        registry.unregister_client(conn_id);
        
        Ok(())
    }
    
    /// Check if any clients are blocked on a key (fast read-only check)
    pub fn has_blocked_clients(&self, db: DatabaseIndex, key: &[u8]) -> bool {
        if db >= self.registries.len() {
            return false;
        }
        
        let registry = self.registries[db].read().unwrap();
        registry.has_blocked_clients(key)
    }
    
    /// Notify that a key has received data (called from LPUSH/RPUSH)
    pub fn notify_key_ready(&self, db: DatabaseIndex, key: &[u8]) {
        if db >= self.registries.len() {
            return;
        }
        
        loop {
            let client = {
                let mut registry = self.registries[db].write().unwrap();
                match registry.pop_first_waiter(key) {
                    Some(c) => c,
                    None => break, // No more clients waiting
                }
            };
            
            // In Redis, any push that makes the list non-empty should wake blocked clients
            self.wake_queue.push(WakeupRequest {
                conn_id: client.conn_id,
                db,
                key: key.to_vec(),
                op_type: client.op_type,
            });
        }
    }
    
    /// Process wake-up queue (called from main server loop)
    pub fn process_wakeups(&self) -> Vec<WakeupRequest> {
        let mut wakeups = Vec::new();
        
        // Drain up to 32 wake-ups at once for batching
        while wakeups.len() < 32 {
            match self.wake_queue.pop() {
                Some(req) => wakeups.push(req),
                None => break,
            }
        }
        
        wakeups
    }
    
    /// Process timeouts (should be called periodically)
    pub fn process_timeouts(&self) -> Vec<u64> {
        let now = Instant::now();
        let mut all_expired = Vec::new();
        
        for registry in &self.registries {
            let mut registry_guard = registry.write().unwrap();
            let expired = registry_guard.get_expired_clients(now);
            all_expired.extend(expired);
        }
        
        all_expired
    }
    
    /// Get a reference to the wake queue for checking if work is available
    pub fn has_pending_wakeups(&self) -> bool {
        !self.wake_queue.is_empty()
    }
    
    /// Shutdown the blocking manager
    pub fn shutdown(&self) {
        let mut shutdown = self.shutdown.lock().unwrap();
        *shutdown = true;
    }
}

impl Default for BlockingRegistry {
    fn default() -> Self {
        Self::new()
    }
}