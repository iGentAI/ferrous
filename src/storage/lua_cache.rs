//! Global Lua script cache with zero-overhead lazy locking
//! 
//! This module provides a thread-safe script cache that is shared
//! across all connections, implementing lazy locking to avoid
//! performance impact on non-Lua operations.

use std::sync::{Arc, RwLock};
use std::collections::HashMap;

/// Global script cache shared across all connections
pub struct GlobalScriptCache {
    scripts: Arc<RwLock<HashMap<String, String>>>,
}

impl GlobalScriptCache {
    /// Create a new global script cache
    pub fn new() -> Self {
        GlobalScriptCache {
            scripts: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Insert a script into the cache
    pub fn insert(&self, sha1: String, script: String) -> Result<(), crate::error::FerrousError> {
        let mut cache = self.scripts.write()
            .map_err(|_| crate::error::FerrousError::Connection("Script cache lock poisoned".into()))?;
        cache.insert(sha1, script);
        Ok(())
    }
    
    /// Get a script from the cache
    pub fn get(&self, sha1: &str) -> Result<Option<String>, crate::error::FerrousError> {
        let cache = self.scripts.read()
            .map_err(|_| crate::error::FerrousError::Connection("Script cache lock poisoned".into()))?;
        Ok(cache.get(sha1).cloned())
    }
    
    /// Check if a script exists in the cache
    pub fn contains_key(&self, sha1: &str) -> Result<bool, crate::error::FerrousError> {
        let cache = self.scripts.read()
            .map_err(|_| crate::error::FerrousError::Connection("Script cache lock poisoned".into()))?;
        Ok(cache.contains_key(sha1))
    }
    
    /// Clear all scripts from the cache
    pub fn clear(&self) -> Result<(), crate::error::FerrousError> {
        let mut cache = self.scripts.write()
            .map_err(|_| crate::error::FerrousError::Connection("Script cache lock poisoned".into()))?;
        cache.clear();
        Ok(())
    }
}

// Clone implementation for Arc sharing
impl Clone for GlobalScriptCache {
    fn clone(&self) -> Self {
        GlobalScriptCache {
            scripts: Arc::clone(&self.scripts),
        }
    }
}

/// Zero-overhead trait for script caching
pub trait ScriptCaching: Send + Sync {
    /// Insert a script into the cache (only called for SCRIPT LOAD)
    fn insert(&self, sha1: String, script: String) -> Result<(), crate::error::FerrousError>;
    
    /// Get a script from the cache (only called for EVALSHA)
    fn get(&self, sha1: &str) -> Result<Option<String>, crate::error::FerrousError>;
    
    /// Check if a script exists (only called for SCRIPT EXISTS)
    fn contains_key(&self, sha1: &str) -> Result<bool, crate::error::FerrousError>;
    
    /// Clear all scripts (only called for SCRIPT FLUSH) 
    fn clear(&self) -> Result<(), crate::error::FerrousError>;
}

impl ScriptCaching for GlobalScriptCache {
    #[inline]
    fn insert(&self, sha1: String, script: String) -> Result<(), crate::error::FerrousError> {
        self.insert(sha1, script)
    }
    
    #[inline]
    fn get(&self, sha1: &str) -> Result<Option<String>, crate::error::FerrousError> {
        self.get(sha1)
    }
    
    #[inline]
    fn contains_key(&self, sha1: &str) -> Result<bool, crate::error::FerrousError> {
        self.contains_key(sha1)
    }
    
    #[inline]
    fn clear(&self) -> Result<(), crate::error::FerrousError> {
        self.clear()
    }
}

/// Null script cache for testing or disabled scenarios
pub struct NullScriptCache;

impl ScriptCaching for NullScriptCache {
    #[inline]
    fn insert(&self, _sha1: String, _script: String) -> Result<(), crate::error::FerrousError> {
        Ok(())
    }
    
    #[inline]
    fn get(&self, _sha1: &str) -> Result<Option<String>, crate::error::FerrousError> {
        Ok(None)
    }
    
    #[inline]
    fn contains_key(&self, _sha1: &str) -> Result<bool, crate::error::FerrousError> {
        Ok(false)
    }
    
    #[inline]
    fn clear(&self) -> Result<(), crate::error::FerrousError> {
        Ok(())
    }
}