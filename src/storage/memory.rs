//! Memory management for storage engine
//! 
//! Tracks memory usage and implements eviction policies.

use std::sync::atomic::{AtomicUsize, Ordering};

/// Memory manager for tracking usage and eviction
pub struct MemoryManager {
    /// Current memory usage in bytes
    used_memory: AtomicUsize,
    
    /// Maximum memory limit (0 = no limit)
    max_memory: usize,
    
    /// Eviction policy
    policy: EvictionPolicy,
}

/// Available eviction policies
#[derive(Debug, Clone, Copy)]
pub enum EvictionPolicy {
    /// No eviction - return out of memory error
    NoEviction,
    
    /// Remove any key according to LRU
    AllKeysLru,
    
    /// Remove keys with expire set according to LRU
    VolatileLru,
    
    /// Remove any key at random
    AllKeysRandom,
    
    /// Remove keys with expire set at random
    VolatileRandom,
    
    /// Remove keys with expire set according to TTL
    VolatileTtl,
}

impl MemoryManager {
    /// Create a new memory manager
    pub fn new(max_memory: usize, policy: EvictionPolicy) -> Self {
        MemoryManager {
            used_memory: AtomicUsize::new(0),
            max_memory,
            policy,
        }
    }
    
    /// Create memory manager with no limits
    pub fn unlimited() -> Self {
        MemoryManager {
            used_memory: AtomicUsize::new(0),
            max_memory: 0,
            policy: EvictionPolicy::NoEviction,
        }
    }
    
    /// Add memory usage
    pub fn add_memory(&self, bytes: usize) -> bool {
        let old_usage = self.used_memory.fetch_add(bytes, Ordering::Relaxed);
        let new_usage = old_usage + bytes;
        
        // Check if we exceeded the limit
        if self.max_memory > 0 && new_usage > self.max_memory {
            // For now, just track, TODO: implement eviction
            false
        } else {
            true
        }
    }
    
    /// Remove memory usage
    pub fn remove_memory(&self, bytes: usize) {
        self.used_memory.fetch_sub(bytes, Ordering::Relaxed);
    }
    
    /// Get current memory usage
    pub fn used_memory(&self) -> usize {
        self.used_memory.load(Ordering::Relaxed)
    }
    
    /// Get maximum memory limit
    pub fn max_memory(&self) -> usize {
        self.max_memory
    }
    
    /// Get eviction policy
    pub fn policy(&self) -> EvictionPolicy {
        self.policy
    }
    
    /// Calculate approximate size of data in bytes
    pub fn calculate_size(data: &[u8]) -> usize {
        data.len() + std::mem::size_of::<Vec<u8>>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_memory_tracking() {
        let manager = MemoryManager::new(100, EvictionPolicy::NoEviction);
        
        assert_eq!(manager.used_memory(), 0);
        
        assert!(manager.add_memory(50));
        assert_eq!(manager.used_memory(), 50);
        
        assert!(manager.add_memory(30));
        assert_eq!(manager.used_memory(), 80);
        
        // This should exceed the limit
        assert!(!manager.add_memory(30));
        assert_eq!(manager.used_memory(), 110); // Still tracks it
        
        manager.remove_memory(60);
        assert_eq!(manager.used_memory(), 50);
    }
    
    #[test]
    fn test_unlimited_memory() {
        let manager = MemoryManager::unlimited();
        
        assert!(manager.add_memory(1_000_000));
        assert_eq!(manager.used_memory(), 1_000_000);
    }
}