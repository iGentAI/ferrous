//! Main storage engine implementation
//! 
//! Provides Redis-compatible storage with multiple databases, expiration,
//! and memory management.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use std::thread;
use crate::error::{FerrousError, Result, StorageError};
use super::value::{Value, StoredValue};
use super::memory::MemoryManager;
use super::skiplist::SkipList;
use super::{DatabaseIndex, Key};

/// Main storage engine
pub struct StorageEngine {
    /// Multiple databases (like Redis)
    databases: Vec<Arc<RwLock<Database>>>,
    
    /// Memory management
    memory_manager: Arc<MemoryManager>,
    
    /// Background expiration thread handle
    expiration_handle: Option<thread::JoinHandle<()>>,
}

/// A single database instance
#[derive(Debug)]
pub struct Database {
    /// Key-value storage
    data: HashMap<Key, StoredValue>,
    
    /// Keys with expiration timestamps for efficient cleanup
    expiring_keys: HashMap<Key, Instant>,
}

/// Result of a GET operation
#[derive(Debug)]
pub enum GetResult {
    /// Key exists and value returned
    Found(Value),
    
    /// Key doesn't exist
    NotFound,
    
    /// Key exists but has wrong type for operation
    WrongType,
    
    /// Key existed but expired
    Expired,
}

impl StorageEngine {
    /// Create a new storage engine with default settings
    pub fn new() -> Arc<Self> {
        Self::with_config(16, MemoryManager::unlimited())
    }
    
    /// Create storage engine with configuration
    pub fn with_config(num_databases: usize, memory_manager: MemoryManager) -> Arc<Self> {
        let mut databases = Vec::with_capacity(num_databases);
        for _ in 0..num_databases {
            databases.push(Arc::new(RwLock::new(Database::new())));
        }
        
        let engine = Arc::new(StorageEngine {
            databases,
            memory_manager: Arc::new(memory_manager),
            expiration_handle: None,
        });
        
        // Start expiration cleanup thread
        let engine_clone = Arc::clone(&engine);
        let _handle = thread::spawn(move || {
            Self::expiration_cleanup_loop(engine_clone);
        });
        
        // Note: This is a bit of a hack, we can't store the handle in the Arc
        // In a real implementation, we'd use a different pattern
        
        engine
    }
    
    /// Set a string value
    pub fn set_string(&self, db: DatabaseIndex, key: Key, value: Vec<u8>) -> Result<()> {
        self.set_value(db, key, Value::string(value), None)
    }
    
    /// Set a string value with expiration
    pub fn set_string_ex(&self, db: DatabaseIndex, key: Key, value: Vec<u8>, expires_in: Duration) -> Result<()> {
        self.set_value(db, key, Value::string(value), Some(expires_in))
    }
    
    /// Set any value
    pub fn set_value(&self, db: DatabaseIndex, key: Key, value: Value, expires_in: Option<Duration>) -> Result<()> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        // Calculate memory usage
        let memory_size = self.calculate_value_size(&key, &value);
        
        // Check memory limits
        if !self.memory_manager.add_memory(memory_size) {
            return Err(StorageError::OutOfMemory.into());
        }
        
        // Create stored value
        let stored_value = if let Some(expires_in) = expires_in {
            StoredValue::with_expiration(value, expires_in)
        } else {
            StoredValue::new(value)
        };
        
        // Track expiration if needed
        if let Some(expires_at) = stored_value.metadata.expires_at {
            db_guard.expiring_keys.insert(key.clone(), expires_at);
        }
        
        // Store the value
        db_guard.data.insert(key, stored_value);
        
        Ok(())
    }
    
    /// Get a value
    pub fn get(&self, db: DatabaseIndex, key: &[u8]) -> Result<GetResult> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                // Check if expired
                if stored_value.is_expired() {
                    // Remove expired key
                    db_guard.data.remove(key);
                    db_guard.expiring_keys.remove(key);
                    Ok(GetResult::Expired)
                } else {
                    // Update access time and return value
                    stored_value.touch();
                    Ok(GetResult::Found(stored_value.value.clone()))
                }
            }
            None => Ok(GetResult::NotFound),
        }
    }
    
    /// Get string value
    pub fn get_string(&self, db: DatabaseIndex, key: &[u8]) -> Result<Option<Vec<u8>>> {
        match self.get(db, key)? {
            GetResult::Found(Value::String(bytes)) => Ok(Some(bytes)),
            GetResult::Found(_) => Err(StorageError::WrongType.into()),
            GetResult::NotFound | GetResult::Expired => Ok(None),
            GetResult::WrongType => Err(StorageError::WrongType.into()),
        }
    }
    
    /// Check if key exists
    pub fn exists(&self, db: DatabaseIndex, key: &[u8]) -> Result<bool> {
        match self.get(db, key)? {
            GetResult::Found(_) => Ok(true),
            _ => Ok(false),
        }
    }
    
    /// Delete a key
    pub fn delete(&self, db: DatabaseIndex, key: &[u8]) -> Result<bool> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        if let Some(stored_value) = db_guard.data.remove(key) {
            db_guard.expiring_keys.remove(key);
            
            // Update memory usage
            let memory_size = self.calculate_value_size(key, &stored_value.value);
            self.memory_manager.remove_memory(memory_size);
            
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    /// Set expiration on a key
    pub fn expire(&self, db: DatabaseIndex, key: &[u8], expires_in: Duration) -> Result<bool> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        if let Some(stored_value) = db_guard.data.get_mut(key) {
            stored_value.metadata.set_expiration(expires_in);
            db_guard.expiring_keys.insert(key.to_vec(), Instant::now() + expires_in);
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    /// Get time to live for a key
    pub fn ttl(&self, db: DatabaseIndex, key: &[u8]) -> Result<Option<Duration>> {
        let database = self.get_database(db)?;
        let db_guard = database.read().unwrap();
        
        if let Some(stored_value) = db_guard.data.get(key) {
            if let Some(expires_at) = stored_value.metadata.expires_at {
                let now = Instant::now();
                if expires_at > now {
                    Ok(Some(expires_at - now))
                } else {
                    Ok(Some(Duration::from_secs(0))) // Expired
                }
            } else {
                Ok(None) // No expiration set
            }
        } else {
            Ok(None) // Key doesn't exist
        }
    }
    
    /// Increment integer value
    pub fn incr(&self, db: DatabaseIndex, key: Key) -> Result<i64> {
        self.incr_by(db, key, 1)
    }
    
    /// Increment integer value by amount
    pub fn incr_by(&self, db: DatabaseIndex, key: Key, increment: i64) -> Result<i64> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        let new_value = match db_guard.data.get(&key) {
            Some(stored_value) => {
                // Try to parse existing value as integer
                match stored_value.value.as_integer() {
                    Some(current) => current + increment,
                    None => return Err(FerrousError::Command(crate::error::CommandError::NotInteger)),
                }
            }
            None => increment, // Start from increment if key doesn't exist
        };
        
        // Store the new value
        let stored_value = StoredValue::new(Value::integer(new_value));
        let memory_size = self.calculate_value_size(&key, &stored_value.value);
        
        if !self.memory_manager.add_memory(memory_size) {
            return Err(StorageError::OutOfMemory.into());
        }
        
        db_guard.data.insert(key, stored_value);
        Ok(new_value)
    }
    
    /// Get all keys from a database (for RDB persistence)
    pub fn get_all_keys(&self, db: DatabaseIndex) -> Result<Vec<Key>> {
        let database = self.get_database(db)?;
        let db_guard = database.read().unwrap();
        
        let keys: Vec<Key> = db_guard.data.keys().cloned().collect();
        Ok(keys)
    }
    
    /// Get database count
    pub fn database_count(&self) -> usize {
        self.databases.len()
    }
    
    /// Flush all data from a database
    pub fn flush_db(&self, db: DatabaseIndex) -> Result<()> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        // Calculate memory to free
        let mut memory_to_free = 0;
        for (key, stored_value) in db_guard.data.iter() {
            memory_to_free += self.calculate_value_size(key, &stored_value.value);
        }
        
        db_guard.data.clear();
        db_guard.expiring_keys.clear();
        
        self.memory_manager.remove_memory(memory_to_free);
        Ok(())
    }
    
    /// Get database reference
    fn get_database(&self, db: DatabaseIndex) -> Result<&Arc<RwLock<Database>>> {
        self.databases.get(db).ok_or_else(|| {
            StorageError::InvalidDatabase.into()
        })
    }
    
    /// Add a member with score to a sorted set
    pub fn zadd(&self, db: DatabaseIndex, key: Key, member: Vec<u8>, score: f64) -> Result<bool> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        // Check if key exists and is a sorted set
        let is_new = match db_guard.data.get_mut(&key) {
            Some(stored_value) => {
                // Check if it's a sorted set
                match &mut stored_value.value {
                    Value::SortedSet(skiplist) => {
                        // Try to insert/update the member
                        let old_score = skiplist.insert(member.clone(), score);
                        stored_value.touch();
                        old_score.is_none() // Return true if new member
                    }
                    _ => return Err(StorageError::WrongType.into()),
                }
            }
            None => {
                // Create a new sorted set
                let skiplist = SkipList::new();
                skiplist.insert(member.clone(), score);
                
                // Calculate memory usage
                let memory_size = self.calculate_value_size(&key, &Value::empty_sorted_set()) +
                                 self.calculate_member_size(&member);
                
                if !self.memory_manager.add_memory(memory_size) {
                    return Err(StorageError::OutOfMemory.into());
                }
                
                let stored_value = StoredValue::new(Value::SortedSet(Arc::new(skiplist)));
                db_guard.data.insert(key, stored_value);
                true // New member in new set
            }
        };
        
        Ok(is_new)
    }
    
    /// Remove a member from a sorted set
    pub fn zrem(&self, db: DatabaseIndex, key: &[u8], member: &[u8]) -> Result<bool> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        // Check if key exists and is a sorted set
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                let (removed, is_empty) = match &mut stored_value.value {
                    Value::SortedSet(skiplist) => {
                        let removed = skiplist.remove(member).is_some();
                        (removed, skiplist.is_empty())
                    }
                    _ => return Err(StorageError::WrongType.into()),
                };
                
                if removed {
                    // Update memory usage
                    let member_size = self.calculate_member_size(member);
                    self.memory_manager.remove_memory(member_size);
                    
                    // Delete the key if the set is now empty
                    if is_empty {
                        db_guard.data.remove(key);
                    } else {
                        stored_value.touch();
                    }
                }
                
                Ok(removed)
            }
            None => Ok(false), // Key doesn't exist
        }
    }
    
    /// Get the score of a member in a sorted set
    pub fn zscore(&self, db: DatabaseIndex, key: &[u8], member: &[u8]) -> Result<Option<f64>> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        // Check if key exists and is a sorted set
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                let score = match &stored_value.value {
                    Value::SortedSet(skiplist) => skiplist.get_score(member),
                    _ => return Err(StorageError::WrongType.into()),
                };
                
                stored_value.touch();
                Ok(score)
            }
            None => Ok(None), // Key doesn't exist
        }
    }
    
    /// Get the rank of a member in a sorted set (0-based)
    pub fn zrank(&self, db: DatabaseIndex, key: &[u8], member: &[u8], reverse: bool) -> Result<Option<usize>> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        // Check if key exists and is a sorted set
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                let result = match &stored_value.value {
                    Value::SortedSet(skiplist) => {
                        let rank = skiplist.get_rank(member);
                        
                        if let Some(rank) = rank {
                            if reverse {
                                // For ZREVRANK, invert the rank
                                Some(skiplist.len() - 1 - rank)
                            } else {
                                Some(rank)
                            }
                        } else {
                            None
                        }
                    }
                    _ => return Err(StorageError::WrongType.into()),
                };
                
                stored_value.touch();
                Ok(result)
            }
            None => Ok(None), // Key doesn't exist
        }
    }
    
    /// Get a range of members by rank from a sorted set
    pub fn zrange(&self, db: DatabaseIndex, key: &[u8], start: isize, stop: isize, reverse: bool) 
        -> Result<Vec<(Vec<u8>, f64)>> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        // Check if key exists and is a sorted set
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                let result = match &stored_value.value {
                    Value::SortedSet(skiplist) => {
                        let len = skiplist.len();
                        if len == 0 {
                            Vec::new()
                        } else {
                            // Convert negative indices and clamp to valid range
                            let start_idx = if start < 0 { 
                                (len as isize + start).max(0) as usize
                            } else {
                                start as usize
                            };
                            
                            let stop_idx = if stop < 0 {
                                (len as isize + stop).max(0) as usize
                            } else {
                                stop as usize
                            };
                            
                            // Handle reverse ordering
                            if reverse {
                                let real_start = len.saturating_sub(1).saturating_sub(stop_idx.min(len.saturating_sub(1)));
                                let real_stop = len.saturating_sub(1).saturating_sub(start_idx.min(len.saturating_sub(1)));
                                
                                let range = skiplist.range_by_rank(real_start, real_stop);
                                // Reverse the results
                                let mut items = range.items;
                                items.reverse();
                                items
                            } else {
                                if start_idx >= len || start_idx > stop_idx {
                                    Vec::new()
                                } else {
                                    let start_idx = start_idx.min(len - 1);
                                    let stop_idx = stop_idx.min(len - 1);
                                    
                                    let range = skiplist.range_by_rank(start_idx, stop_idx);
                                    range.items
                                }
                            }
                        }
                    }
                    _ => return Err(StorageError::WrongType.into()),
                };
                
                stored_value.touch();
                Ok(result)
            }
            None => Ok(Vec::new()), // Key doesn't exist
        }
    }
    
    /// Get a range of members by score from a sorted set
    pub fn zrangebyscore(&self, db: DatabaseIndex, key: &[u8], min_score: f64, max_score: f64, reverse: bool) 
        -> Result<Vec<(Vec<u8>, f64)>> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        // Check if key exists and is a sorted set
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                let result = match &stored_value.value {
                    Value::SortedSet(skiplist) => {
                        // Get the range
                        let range = skiplist.range_by_score(min_score, max_score);
                        let mut items = range.items;
                        
                        if reverse {
                            items.reverse();
                        }
                        
                        items
                    }
                    _ => return Err(StorageError::WrongType.into()),
                };
                
                stored_value.touch();
                Ok(result)
            }
            None => Ok(Vec::new()), // Key doesn't exist
        }
    }
    
    /// Count members within score range
    pub fn zcount(&self, db: DatabaseIndex, key: &[u8], min_score: f64, max_score: f64) -> Result<usize> {
        let members = self.zrangebyscore(db, key, min_score, max_score, false)?;
        Ok(members.len())
    }
    
    /// Increment score of a member
    pub fn zincrby(&self, db: DatabaseIndex, key: Key, member: Vec<u8>, increment: f64) -> Result<f64> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        // Check if key exists and is a sorted set
        match db_guard.data.get_mut(&key) {
            Some(stored_value) => {
                // Check if it's a sorted set
                match &mut stored_value.value {
                    Value::SortedSet(skiplist) => {
                        // Try to get current score
                        let new_score = match skiplist.get_score(&member) {
                            Some(curr_score) => curr_score + increment,
                            None => increment,
                        };
                        
                        // Update the score
                        skiplist.insert(member, new_score);
                        stored_value.touch();
                        
                        Ok(new_score)
                    }
                    _ => Err(StorageError::WrongType.into()),
                }
            }
            None => {
                // Create a new sorted set
                let skiplist = SkipList::new();
                skiplist.insert(member.clone(), increment);
                
                // Calculate memory usage
                let memory_size = self.calculate_value_size(&key, &Value::empty_sorted_set()) +
                                 self.calculate_member_size(&member);
                
                if !self.memory_manager.add_memory(memory_size) {
                    return Err(StorageError::OutOfMemory.into());
                }
                
                let stored_value = StoredValue::new(Value::SortedSet(Arc::new(skiplist)));
                db_guard.data.insert(key, stored_value);
                
                Ok(increment)
            }
        }
    }
    
    /// Calculate the memory size of a sorted set member
    fn calculate_member_size(&self, member: &[u8]) -> usize {
        // Member size + Score size + Node overhead
        MemoryManager::calculate_size(member) + std::mem::size_of::<f64>() + 32
    }

    /// Calculate memory size for a key-value pair
    fn calculate_value_size(&self, key: &[u8], value: &Value) -> usize {
        let key_size = MemoryManager::calculate_size(key);
        let value_size = match value {
            Value::String(bytes) => MemoryManager::calculate_size(bytes),
            Value::List(list) => {
                list.iter().map(|item| MemoryManager::calculate_size(item)).sum::<usize>()
                    + std::mem::size_of::<VecDeque<Vec<u8>>>()
            }
            Value::Set(set) => {
                set.iter().map(|item| MemoryManager::calculate_size(item)).sum::<usize>()
                    + std::mem::size_of::<std::collections::HashSet<Vec<u8>>>()
            }
            Value::Hash(hash) => {
                hash.iter().map(|(k, v)| MemoryManager::calculate_size(k) + MemoryManager::calculate_size(v)).sum::<usize>()
                    + std::mem::size_of::<HashMap<Vec<u8>, Vec<u8>>>()
            }
            Value::SortedSet(skiplist) => {
                // Use the memory_usage method directly from the SkipList
                skiplist.memory_usage()
            }
        };
        key_size + value_size
    }
    
    /// Background thread for cleaning up expired keys
    fn expiration_cleanup_loop(engine: Arc<StorageEngine>) {
        loop {
            thread::sleep(Duration::from_secs(1)); // Check every second
            
            for (_db_idx, database) in engine.databases.iter().enumerate() {
                let mut db_guard = match database.write() {
                    Ok(guard) => guard,
                    Err(_) => continue, // Skip if poisoned
                };
                
                let now = Instant::now();
                let mut expired_keys = Vec::new();
                
                // Find expired keys
                for (key, expires_at) in db_guard.expiring_keys.iter() {
                    if *expires_at <= now {
                        expired_keys.push(key.clone());
                    }
                }
                
                // Remove expired keys
                for key in expired_keys {
                    if let Some(stored_value) = db_guard.data.remove(&key) {
                        db_guard.expiring_keys.remove(&key);
                        
                        // Update memory usage
                        let memory_size = engine.calculate_value_size(&key, &stored_value.value);
                        engine.memory_manager.remove_memory(memory_size);
                    }
                }
            }
        }
    }
}

impl Database {
    /// Create a new database
    pub fn new() -> Self {
        Database {
            data: HashMap::new(),
            expiring_keys: HashMap::new(),
        }
    }
}

impl Default for Database {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_operations() {
        let engine = StorageEngine::new();
        
        // Test set/get
        engine.set_string(0, b"test".to_vec(), b"value".to_vec()).unwrap();
        let result = engine.get_string(0, b"test").unwrap();
        assert_eq!(result, Some(b"value".to_vec()));
        
        // Test non-existent key
        let result = engine.get_string(0, b"nonexistent").unwrap();
        assert_eq!(result, None);
        
        // Test delete
        assert!(engine.delete(0, b"test").unwrap());
        let result = engine.get_string(0, b"test").unwrap();
        assert_eq!(result, None);
    }
    
    #[test]
    fn test_increment() {
        let engine = StorageEngine::new();
        
        // Increment non-existent key
        let result = engine.incr(b"counter".to_vec(), 0).unwrap();
        assert_eq!(result, 1);
        
        // Increment existing key
        let result = engine.incr(b"counter".to_vec(), 0).unwrap();
        assert_eq!(result, 2);
        
        // Increment by specific amount
        let result = engine.incr_by(b"counter".to_vec(), 0, 5).unwrap();
        assert_eq!(result, 7);
    }
    
    #[test]
    fn test_expiration() {
        let engine = StorageEngine::new();
        
        // Set with short expiration
        engine.set_string_ex(0, b"temp".to_vec(), b"value".to_vec(), Duration::from_millis(1)).unwrap();
        
        // Should exist initially
        assert!(engine.exists(0, b"temp").unwrap());
        
        // Wait for expiration
        thread::sleep(Duration::from_millis(5));
        
        // Should not exist after expiration
        assert!(!engine.exists(0, b"temp").unwrap());
    }
}