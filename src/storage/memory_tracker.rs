//! Memory tracking system
//! 
//! Provides detailed memory usage tracking for databases and data structures.
//! This enhances observability for production environments.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use crate::storage::memory::MemoryCategory;

/// Memory tracking statistics
pub struct MemoryStats {
    /// Total memory usage
    pub total_used: AtomicUsize,
    
    /// Peak memory usage
    pub peak_used: AtomicUsize,
    
    /// Memory used by keys (key names)
    pub keys_size: AtomicUsize,
    
    /// Memory used by key metadata
    pub keys_overhead: AtomicUsize,
    
    /// Memory used by string values
    pub strings_size: AtomicUsize,
    
    /// Memory used by list values
    pub lists_size: AtomicUsize,
    
    /// Memory used by set values
    pub sets_size: AtomicUsize,
    
    /// Memory used by hash values
    pub hashes_size: AtomicUsize,
    
    /// Memory used by sorted set values
    pub zsets_size: AtomicUsize,
    
    /// Per-database memory usage
    pub db_memory: Arc<RwLock<HashMap<usize, usize>>>,
    
    /// Last sample time
    pub last_sample: Arc<RwLock<Instant>>,
    
    /// Memory sampling interval in milliseconds
    pub sample_interval: AtomicUsize,
}

impl MemoryStats {
    /// Create a new MemoryStats instance
    pub fn new() -> Self {
        MemoryStats {
            total_used: AtomicUsize::new(0),
            peak_used: AtomicUsize::new(0),
            keys_size: AtomicUsize::new(0),
            keys_overhead: AtomicUsize::new(0),
            strings_size: AtomicUsize::new(0),
            lists_size: AtomicUsize::new(0),
            sets_size: AtomicUsize::new(0),
            hashes_size: AtomicUsize::new(0),
            zsets_size: AtomicUsize::new(0),
            db_memory: Arc::new(RwLock::new(HashMap::new())),
            last_sample: Arc::new(RwLock::new(Instant::now())),
            sample_interval: AtomicUsize::new(5000), // 5 seconds by default
        }
    }
    
    /// Record memory usage for a key
    pub fn record_key_memory(&self, db: usize, key_size: usize, value_size: usize, category: MemoryCategory) {
        // Update category-specific counters
        match category {
            MemoryCategory::String => self.strings_size.fetch_add(value_size, Ordering::Relaxed),
            MemoryCategory::List => self.lists_size.fetch_add(value_size, Ordering::Relaxed),
            MemoryCategory::Set => self.sets_size.fetch_add(value_size, Ordering::Relaxed),
            MemoryCategory::Hash => self.hashes_size.fetch_add(value_size, Ordering::Relaxed),
            MemoryCategory::SortedSet => self.zsets_size.fetch_add(value_size, Ordering::Relaxed),
            MemoryCategory::Overhead => self.keys_overhead.fetch_add(value_size, Ordering::Relaxed),
        };
        
        // Update key size counter
        self.keys_size.fetch_add(key_size, Ordering::Relaxed);
        
        // Update total memory counter
        let total = key_size + value_size;
        let old_total = self.total_used.fetch_add(total, Ordering::Relaxed);
        let new_total = old_total + total;
        
        // Update peak memory if needed
        let peak = self.peak_used.load(Ordering::Relaxed);
        if new_total > peak {
            self.peak_used.store(new_total, Ordering::Relaxed);
        }
        
        // Update per-database memory usage
        let mut db_memory = self.db_memory.write().unwrap();
        *db_memory.entry(db).or_insert(0) += total;
    }
    
    /// Remove memory usage for a key
    pub fn remove_key_memory(&self, db: usize, key_size: usize, value_size: usize, category: MemoryCategory) {
        // Update category-specific counters
        match category {
            MemoryCategory::String => self.strings_size.fetch_sub(value_size, Ordering::Relaxed),
            MemoryCategory::List => self.lists_size.fetch_sub(value_size, Ordering::Relaxed),
            MemoryCategory::Set => self.sets_size.fetch_sub(value_size, Ordering::Relaxed),
            MemoryCategory::Hash => self.hashes_size.fetch_sub(value_size, Ordering::Relaxed),
            MemoryCategory::SortedSet => self.zsets_size.fetch_sub(value_size, Ordering::Relaxed),
            MemoryCategory::Overhead => self.keys_overhead.fetch_sub(value_size, Ordering::Relaxed),
        };
        
        // Update key size counter
        self.keys_size.fetch_sub(key_size, Ordering::Relaxed);
        
        // Update total memory counter
        let total = key_size + value_size;
        self.total_used.fetch_sub(total, Ordering::Relaxed);
        
        // Update per-database memory usage
        let mut db_memory = self.db_memory.write().unwrap();
        if let Some(size) = db_memory.get_mut(&db) {
            *size = size.saturating_sub(total);
        }
    }
    
    /// Get memory usage for a specific database
    pub fn get_db_memory(&self, db: usize) -> usize {
        let db_memory = self.db_memory.read().unwrap();
        *db_memory.get(&db).unwrap_or(&0)
    }
    
    /// Check if it's time to sample memory usage
    pub fn should_sample(&self) -> bool {
        let last_sample = *self.last_sample.read().unwrap();
        let interval_millis = self.sample_interval.load(Ordering::Relaxed);
        let elapsed = last_sample.elapsed().as_millis() as usize;
        
        elapsed >= interval_millis
    }
    
    /// Update the last sample time
    pub fn update_sample_time(&self) {
        let mut last_sample = self.last_sample.write().unwrap();
        *last_sample = Instant::now();
    }
    
    /// Set the sampling interval
    pub fn set_sample_interval(&self, interval_millis: usize) {
        self.sample_interval.store(interval_millis, Ordering::Relaxed);
    }
    
    /// Get all memory statistics as a HashMap
    pub fn get_stats(&self) -> HashMap<String, usize> {
        let mut stats = HashMap::new();
        
        stats.insert("total.allocated".to_string(), self.total_used.load(Ordering::Relaxed));
        stats.insert("peak.allocated".to_string(), self.peak_used.load(Ordering::Relaxed));
        stats.insert("keys.size".to_string(), self.keys_size.load(Ordering::Relaxed));
        stats.insert("keys.overhead".to_string(), self.keys_overhead.load(Ordering::Relaxed));
        stats.insert("strings.size".to_string(), self.strings_size.load(Ordering::Relaxed));
        stats.insert("lists.size".to_string(), self.lists_size.load(Ordering::Relaxed));
        stats.insert("sets.size".to_string(), self.sets_size.load(Ordering::Relaxed));
        stats.insert("hashes.size".to_string(), self.hashes_size.load(Ordering::Relaxed));
        stats.insert("zsets.size".to_string(), self.zsets_size.load(Ordering::Relaxed));
        
        // Add per-database memory usage
        let db_memory = self.db_memory.read().unwrap();
        for (db, size) in db_memory.iter() {
            stats.insert(format!("db{}.memory", db), *size);
        }
        
        stats
    }
    
    /// Reset all memory statistics
    pub fn reset(&self) {
        self.total_used.store(0, Ordering::Relaxed);
        self.peak_used.store(0, Ordering::Relaxed);
        self.keys_size.store(0, Ordering::Relaxed);
        self.keys_overhead.store(0, Ordering::Relaxed);
        self.strings_size.store(0, Ordering::Relaxed);
        self.lists_size.store(0, Ordering::Relaxed);
        self.sets_size.store(0, Ordering::Relaxed);
        self.hashes_size.store(0, Ordering::Relaxed);
        self.zsets_size.store(0, Ordering::Relaxed);
        
        let mut db_memory = self.db_memory.write().unwrap();
        db_memory.clear();
    }
}

impl Default for MemoryStats {
    fn default() -> Self {
        Self::new()
    }
}