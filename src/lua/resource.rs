//! Resource Tracking for Lua VM
//!
//! This module provides principled resource tracking to prevent infinite loops
//! and excessive resource consumption while properly supporting circular data
//! structures.

use std::collections::HashSet;
use super::error::{LuaError, LuaResult};

/// Resource limits configuration
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum memory that can be allocated by string operations
    pub max_string_memory: usize,
    
    /// Maximum depth for operations that generate new data (not traversal)
    pub max_generation_depth: usize,
    
    /// Maximum number of operations in a single transaction
    pub max_transaction_ops: usize,
    
    /// Maximum call stack depth
    pub max_call_depth: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        ResourceLimits {
            max_string_memory: 100_000_000,      // 100MB
            max_generation_depth: 100,           // Deep enough for legitimate use
            max_transaction_ops: 10000,          // Prevent runaway transactions
            max_call_depth: 1000,               // Standard Lua limit
        }
    }
}

/// Tracks resource consumption for a specific operation
#[derive(Debug, Clone)]
pub struct ResourceTracker {
    /// Configured limits
    limits: ResourceLimits,
    
    /// Current string memory allocated in this operation
    string_memory_used: usize,
    
    /// Current generation depth (for operations that create new data)
    generation_depth: usize,
    
    /// Number of operations performed in current transaction
    transaction_ops: usize,
    
    /// Current call stack depth
    call_depth: usize,
    
    /// Visited set for traversal operations (uses type-erased handles)
    visited: HashSet<u64>,
}

impl ResourceTracker {
    /// Create a new resource tracker with specified limits
    pub fn new(limits: ResourceLimits) -> Self {
        ResourceTracker {
            limits,
            string_memory_used: 0,
            generation_depth: 0,
            transaction_ops: 0,
            call_depth: 0,
            visited: HashSet::new(),
        }
    }
    
    /// Get the configured limits (read-only)
    pub fn limits(&self) -> &ResourceLimits {
        &self.limits
    }
    
    /// Track string memory allocation
    pub fn track_string_allocation(&mut self, size: usize) -> LuaResult<()> {
        self.string_memory_used = self.string_memory_used.saturating_add(size);
        if self.string_memory_used > self.limits.max_string_memory {
            Err(LuaError::ResourceLimit {
                resource: "string memory".to_string(),
                limit: self.limits.max_string_memory,
                used: self.string_memory_used,
                context: "String operations have exceeded memory limit".to_string(),
            })
        } else {
            Ok(())
        }
    }
    
    /// Enter a generation context (operations that create new data)
    pub fn enter_generation(&mut self) -> LuaResult<GenerationGuard> {
        self.generation_depth += 1;
        if self.generation_depth > self.limits.max_generation_depth {
            self.generation_depth -= 1; // Restore for error reporting
            Err(LuaError::ResourceLimit {
                resource: "generation depth".to_string(),
                limit: self.limits.max_generation_depth,
                used: self.generation_depth + 1,
                context: "Data generation depth limit exceeded. This likely indicates infinite recursion in data generation, not circular references.".to_string(),
            })
        } else {
            Ok(GenerationGuard { tracker: self })
        }
    }
    
    /// Track a transaction operation
    pub fn track_operation(&mut self) -> LuaResult<()> {
        self.transaction_ops += 1;
        if self.transaction_ops > self.limits.max_transaction_ops {
            Err(LuaError::ResourceLimit {
                resource: "transaction operations".to_string(),
                limit: self.limits.max_transaction_ops,
                used: self.transaction_ops,
                context: "Too many operations in a single transaction".to_string(),
            })
        } else {
            Ok(())
        }
    }
    
    /// Enter a function call
    pub fn enter_call(&mut self) -> LuaResult<CallGuard> {
        self.call_depth += 1;
        if self.call_depth > self.limits.max_call_depth {
            self.call_depth -= 1; // Restore for error reporting
            Err(LuaError::ResourceLimit {
                resource: "call depth".to_string(),
                limit: self.limits.max_call_depth,
                used: self.call_depth + 1,
                context: "Call stack depth limit exceeded".to_string(),
            })
        } else {
            Ok(CallGuard { tracker: self })
        }
    }
    
    /// Check if we've visited this handle (for traversal operations)
    pub fn check_visited(&mut self, handle_id: u64) -> bool {
        !self.visited.insert(handle_id)
    }
    
    /// Clear the visited set (for new traversal)
    pub fn clear_visited(&mut self) {
        self.visited.clear();
    }
    
    /// Get current resource usage summary
    pub fn usage_summary(&self) -> String {
        format!(
            "Resources: string_memory={}/{}, generation_depth={}/{}, ops={}/{}, calls={}/{}",
            self.string_memory_used, self.limits.max_string_memory,
            self.generation_depth, self.limits.max_generation_depth,
            self.transaction_ops, self.limits.max_transaction_ops,
            self.call_depth, self.limits.max_call_depth
        )
    }
}

/// RAII guard for generation depth tracking
pub struct GenerationGuard<'a> {
    tracker: &'a mut ResourceTracker,
}

impl<'a> Drop for GenerationGuard<'a> {
    fn drop(&mut self) {
        self.tracker.generation_depth = self.tracker.generation_depth.saturating_sub(1);
    }
}

/// RAII guard for call depth tracking
pub struct CallGuard<'a> {
    tracker: &'a mut ResourceTracker,
}

impl<'a> Drop for CallGuard<'a> {
    fn drop(&mut self) {
        self.tracker.call_depth = self.tracker.call_depth.saturating_sub(1);
    }
}

/// Context for string concatenation operations
#[derive(Debug)]
pub struct ConcatContext {
    /// Parts collected so far
    pub parts: Vec<String>,
    
    /// Total length accumulated
    pub total_length: usize,
}

impl ConcatContext {
    pub fn new() -> Self {
        ConcatContext {
            parts: Vec::new(),
            total_length: 0,
        }
    }
    
    pub fn add_part(&mut self, part: String) {
        self.total_length += part.len();
        self.parts.push(part);
    }
    
    pub fn finish(self) -> String {
        self.parts.join("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_string_memory_tracking() {
        let limits = ResourceLimits {
            max_string_memory: 1000,
            ..Default::default()
        };
        let mut tracker = ResourceTracker::new(limits);
        
        // Should succeed
        assert!(tracker.track_string_allocation(500).is_ok());
        assert!(tracker.track_string_allocation(400).is_ok());
        
        // Should fail - exceeds limit
        assert!(tracker.track_string_allocation(200).is_err());
    }
    
    #[test]
    fn test_generation_depth() {
        let limits = ResourceLimits {
            max_generation_depth: 3,
            ..Default::default()
        };
        let mut tracker = ResourceTracker::new(limits);
        
        // Should succeed up to limit
        let _g1 = tracker.enter_generation().unwrap();
        let _g2 = tracker.enter_generation().unwrap();
        let _g3 = tracker.enter_generation().unwrap();
        
        // Should fail - exceeds limit
        assert!(tracker.enter_generation().is_err());
    }
    
    #[test]
    fn test_visited_tracking() {
        let mut tracker = ResourceTracker::new(Default::default());
        
        // First visit should return false (not visited)
        assert!(!tracker.check_visited(123));
        
        // Second visit should return true (already visited)
        assert!(tracker.check_visited(123));
        
        // Different handle should return false
        assert!(!tracker.check_visited(456));
        
        // Clear and recheck
        tracker.clear_visited();
        assert!(!tracker.check_visited(123));
    }
}