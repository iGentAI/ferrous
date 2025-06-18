//! Storage monitoring for auto-save functionality

use std::sync::{Arc, RwLock, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, Duration};
use std::thread;

use crate::storage::StorageEngine;
use super::rdb::{RdbEngine, RdbConfig};

/// Storage monitor for tracking changes and triggering auto-save
pub struct StorageMonitor {
    /// Number of changes since last save
    changes_since_save: Arc<AtomicU64>,
    
    /// Last check time
    last_check_time: Arc<RwLock<SystemTime>>,
    
    /// Monitor thread handle
    monitor_handle: Option<thread::JoinHandle<()>>,
}

impl StorageMonitor {
    /// Create a new storage monitor
    pub fn new() -> Self {
        Self {
            changes_since_save: Arc::new(AtomicU64::new(0)),
            last_check_time: Arc::new(RwLock::new(SystemTime::now())),
            monitor_handle: None,
        }
    }
    
    /// Start monitoring with given configuration
    pub fn start(
        &mut self,
        storage: Arc<StorageEngine>,
        rdb_engine: Arc<RdbEngine>,
        config: RdbConfig,
    ) {
        if !config.auto_save {
            return;
        }
        
        let changes_since_save = Arc::clone(&self.changes_since_save);
        let last_check_time = Arc::clone(&self.last_check_time);
        
        let handle = thread::spawn(move || {
            Self::monitor_loop(
                storage,
                rdb_engine,
                config,
                changes_since_save,
                last_check_time,
            );
        });
        
        self.monitor_handle = Some(handle);
    }
    
    /// Increment change counter
    pub fn record_change(&self) {
        self.changes_since_save.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Reset change counter
    pub fn reset_changes(&self) {
        self.changes_since_save.store(0, Ordering::Relaxed);
    }
    
    /// Monitor loop
    fn monitor_loop(
        storage: Arc<StorageEngine>,
        rdb_engine: Arc<RdbEngine>,
        config: RdbConfig,
        changes_since_save: Arc<AtomicU64>,
        last_check_time: Arc<RwLock<SystemTime>>,
    ) {
        loop {
            thread::sleep(Duration::from_secs(1)); // Check every second
            
            let now = SystemTime::now();
            let changes = changes_since_save.load(Ordering::Relaxed);
            
            // Check save rules
            for &(seconds, min_changes) in &config.save_rules {
                if changes >= min_changes {
                    let last_check = last_check_time.read().unwrap();
                    let elapsed = now.duration_since(*last_check)
                        .unwrap_or(Duration::from_secs(0));
                    
                    if elapsed >= Duration::from_secs(seconds) {
                        // Time to save
                        drop(last_check);
                        
                        // Perform background save
                        match rdb_engine.bgsave(Arc::clone(&storage)) {
                            Ok(_) => {
                                println!("Auto-save: {} changes in {} seconds, saving...", 
                                    changes, elapsed.as_secs());
                                
                                // Reset counters
                                changes_since_save.store(0, Ordering::Relaxed);
                                *last_check_time.write().unwrap() = now;
                            }
                            Err(e) => {
                                if !rdb_engine.is_bgsave_in_progress() {
                                    eprintln!("Auto-save failed: {}", e);
                                }
                            }
                        }
                        
                        break; // Only trigger one save rule
                    }
                }
            }
        }
    }
}

impl Default for StorageMonitor {
    fn default() -> Self {
        Self::new()
    }
}