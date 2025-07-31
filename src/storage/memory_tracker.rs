//! Lock-free memory tracking system
//! 
//! Provides detailed memory usage tracking using atomic operations for maximum performance.
//! This eliminates the expensive RwLock contention that was causing performance regression.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::storage::memory::MemoryCategory;

/// Lock-free memory tracking statistics using atomic operations
pub struct MemoryStats {
    /// Total memory usage (atomic counter)
    pub total_used: AtomicUsize,
    
    /// Peak memory usage (atomic counter)
    pub peak_used: AtomicUsize,
    
    /// Memory used by keys (key names) - atomic
    pub keys_size: AtomicUsize,
    
    /// Memory used by key metadata - atomic
    pub keys_overhead: AtomicUsize,
    
    /// Memory used by string values - atomic
    pub strings_size: AtomicUsize,
    
    /// Memory used by list values - atomic
    pub lists_size: AtomicUsize,
    
    /// Memory used by set values - atomic
    pub sets_size: AtomicUsize,
    
    /// Memory used by hash values - atomic
    pub hashes_size: AtomicUsize,
    
    /// Memory used by sorted set values - atomic
    pub zsets_size: AtomicUsize,
    
    /// Per-database memory usage (atomic counters) - one per database
    pub db_memory: Vec<AtomicUsize>,
    
    /// Last sample time (still needs protection for timestamp updates)
    pub last_sample: Arc<RwLock<Instant>>,
    
    /// Memory sampling interval in milliseconds
    pub sample_interval: AtomicUsize,
}

impl MemoryStats {
    /// Create a new lock-free MemoryStats instance
    pub fn new(num_databases: usize) -> Self {
        // Pre-allocate atomic counters for all databases
        let mut db_memory = Vec::with_capacity(num_databases);
        for _ in 0..num_databases {
            db_memory.push(AtomicUsize::new(0));
        }
        
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
            db_memory,
            last_sample: Arc::new(RwLock::new(Instant::now())),
            sample_interval: AtomicUsize::new(5000), // 5 seconds by default
        }
    }
    
    /// Record memory usage for a key - NOW LOCK-FREE!
    pub fn record_key_memory(&self, db: usize, key_size: usize, value_size: usize, category: MemoryCategory) {
        // Update category-specific counters atomically
        match category {
            MemoryCategory::String => { self.strings_size.fetch_add(value_size, Ordering::Relaxed); },
            MemoryCategory::List => { self.lists_size.fetch_add(value_size, Ordering::Relaxed); },
            MemoryCategory::Set => { self.sets_size.fetch_add(value_size, Ordering::Relaxed); },
            MemoryCategory::Hash => { self.hashes_size.fetch_add(value_size, Ordering::Relaxed); },
            MemoryCategory::SortedSet => { self.zsets_size.fetch_add(value_size, Ordering::Relaxed); },
            MemoryCategory::Overhead => { self.keys_overhead.fetch_add(value_size, Ordering::Relaxed); },
        };
        
        // Update key size counter atomically
        self.keys_size.fetch_add(key_size, Ordering::Relaxed);
        
        // Update total memory counter atomically
        let total = key_size + value_size;
        let old_total = self.total_used.fetch_add(total, Ordering::Relaxed);
        let new_total = old_total + total;
        
        // Update peak memory if needed (atomic compare-and-swap)
        loop {
            let peak = self.peak_used.load(Ordering::Relaxed);
            if new_total <= peak {
                break; // No need to update peak
            }
            if self.peak_used.compare_exchange_weak(peak, new_total, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
                break; // Successfully updated peak
            }
            // Loop will retry if another thread updated peak concurrently
        }
        
        // Update per-database memory usage atomically
        if db < self.db_memory.len() {
            self.db_memory[db].fetch_add(total, Ordering::Relaxed);
        }
    }
    
    /// Remove memory usage for a key - NOW LOCK-FREE!
    pub fn remove_key_memory(&self, db: usize, key_size: usize, value_size: usize, category: MemoryCategory) {
        // Update category-specific counters atomically
        match category {
            MemoryCategory::String => { self.strings_size.fetch_sub(value_size, Ordering::Relaxed); },
            MemoryCategory::List => { self.lists_size.fetch_sub(value_size, Ordering::Relaxed); },
            MemoryCategory::Set => { self.sets_size.fetch_sub(value_size, Ordering::Relaxed); },
            MemoryCategory::Hash => { self.hashes_size.fetch_sub(value_size, Ordering::Relaxed); },
            MemoryCategory::SortedSet => { self.zsets_size.fetch_sub(value_size, Ordering::Relaxed); },
            MemoryCategory::Overhead => { self.keys_overhead.fetch_sub(value_size, Ordering::Relaxed); },
        };
        
        // Update key size counter atomically
        self.keys_size.fetch_sub(key_size, Ordering::Relaxed);
        
        // Update total memory counter atomically
        let total = key_size + value_size;
        self.total_used.fetch_sub(total, Ordering::Relaxed);
        
        // Update per-database memory usage atomically
        if db < self.db_memory.len() {
            self.db_memory[db].fetch_sub(total, Ordering::Relaxed);
        }
    }
    
    /// Get memory usage for a specific database - lock-free read
    pub fn get_db_memory(&self, db: usize) -> usize {
        if db < self.db_memory.len() {
            self.db_memory[db].load(Ordering::Relaxed)
        } else {
            0
        }
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
    
    /// Set the sampling interval atomically
    pub fn set_sample_interval(&self, interval_millis: usize) {
        self.sample_interval.store(interval_millis, Ordering::Relaxed);
    }
    
    /// Get all memory statistics as a HashMap - lock-free reads
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
        
        // Add per-database memory usage atomically
        for (db, atomic_size) in self.db_memory.iter().enumerate() {
            stats.insert(format!("db{}.memory", db), atomic_size.load(Ordering::Relaxed));
        }
        
        stats
    }
    
    /// Reset all memory statistics atomically
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
        
        // Reset per-database counters
        for atomic_size in &self.db_memory {
            atomic_size.store(0, Ordering::Relaxed);
        }
    }
}

impl Default for MemoryStats {
    fn default() -> Self {
        Self::new(16) // Default 16 databases
    }
}