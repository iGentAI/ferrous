//! Main storage engine implementation
//! 
//! Provides Redis-compatible storage with multiple databases, expiration,
//! and memory management.

use std::collections::{HashMap, VecDeque, HashSet};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use std::thread;
use rand::seq::SliceRandom;
use crate::error::{FerrousError, Result, StorageError, CommandError};
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
    
    /// Key modification tracking for WATCH
    modified_keys: HashMap<Key, u64>,
    
    /// Modification counter
    modification_counter: u64,
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
        db_guard.data.insert(key.clone(), stored_value);
        db_guard.mark_modified(&key);
        
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
        
        db_guard.data.insert(key.clone(), stored_value);
        db_guard.mark_modified(&key);
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
    
    /// Get current memory usage in bytes
    pub fn memory_usage(&self) -> usize {
        self.memory_manager.used_memory()
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
                db_guard.data.insert(key.clone(), stored_value);
                db_guard.mark_modified(&key);
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
                db_guard.data.insert(key.clone(), stored_value);
                db_guard.mark_modified(&key);
                
                Ok(increment)
            }
        }
    }
    
    /// Calculate the memory size of a sorted set member
    fn calculate_member_size(&self, member: &[u8]) -> usize {
        // Member size + Score size + Node overhead
        MemoryManager::calculate_size(member) + std::mem::size_of::<f64>() + 32
    }

    /// Push elements to the head of a list
    pub fn lpush(&self, db: DatabaseIndex, key: Key, elements: Vec<Vec<u8>>) -> Result<usize> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        let list = match db_guard.data.get_mut(&key) {
            Some(stored_value) => {
                match &stored_value.value {
                    Value::List(ref list) => {
                        // Clone the existing list
                        let mut new_list = list.clone();
                        
                        // Push to cloned list
                        for element in elements.into_iter().rev() {
                            new_list.push_front(element);
                        }
                        
                        let len = new_list.len();
                        
                        // Replace the list value
                        stored_value.value = Value::List(new_list);
                        stored_value.touch();
                        db_guard.mark_modified(&key);
                        len
                    }
                    _ => return Err(StorageError::WrongType.into()),
                }
            }
            None => {
                // Create new list
                let mut list = VecDeque::new();
                for element in elements.into_iter().rev() {
                    list.push_front(element);
                }
                let len = list.len();
                
                let stored_value = StoredValue::new(Value::List(list));
                db_guard.data.insert(key.clone(), stored_value);
                db_guard.mark_modified(&key);
                len
            }
        };
        
        Ok(list)
    }
    
    /// Push elements to the tail of a list
    pub fn rpush(&self, db: DatabaseIndex, key: Key, elements: Vec<Vec<u8>>) -> Result<usize> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        let list = match db_guard.data.get_mut(&key) {
            Some(stored_value) => {
                match &stored_value.value {
                    Value::List(ref list) => {
                        // Clone the existing list
                        let mut new_list = list.clone();
                        
                        // Push to cloned list
                        for element in elements {
                            new_list.push_back(element);
                        }
                        
                        let len = new_list.len();
                        
                        // Replace the list value
                        stored_value.value = Value::List(new_list);
                        stored_value.touch();
                        db_guard.mark_modified(&key);
                        len
                    }
                    _ => return Err(StorageError::WrongType.into()),
                }
            }
            None => {
                // Create new list
                let mut list = VecDeque::new();
                for element in elements {
                    list.push_back(element);
                }
                let len = list.len();
                
                let stored_value = StoredValue::new(Value::List(list));
                db_guard.data.insert(key.clone(), stored_value);
                db_guard.mark_modified(&key);
                len
            }
        };
        
        Ok(list)
    }
    
    /// Pop element from head of list
    pub fn lpop(&self, db: DatabaseIndex, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                match &mut stored_value.value {
                    Value::List(list) => {
                        let element = list.pop_front();
                        
                        // Remove empty list
                        if list.is_empty() {
                            db_guard.data.remove(key);
                        } else {
                            stored_value.touch();
                        }
                        
                        Ok(element)
                    }
                    _ => Err(StorageError::WrongType.into()),
                }
            }
            None => Ok(None),
        }
    }
    
    /// Pop element from tail of list
    pub fn rpop(&self, db: DatabaseIndex, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                match &mut stored_value.value {
                    Value::List(list) => {
                        let element = list.pop_back();
                        
                        // Remove empty list
                        if list.is_empty() {
                            db_guard.data.remove(key);
                        } else {
                            stored_value.touch();
                        }
                        
                        Ok(element)
                    }
                    _ => Err(StorageError::WrongType.into()),
                }
            }
            None => Ok(None),
        }
    }
    
    /// Get list length
    pub fn llen(&self, db: DatabaseIndex, key: &[u8]) -> Result<usize> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                // First get the length, then touch
                let len = match &stored_value.value {
                    Value::List(list) => list.len(),
                    _ => return Err(StorageError::WrongType.into()),
                };
                stored_value.touch();
                Ok(len)
            }
            None => Ok(0),
        }
    }
    
    /// Get range of elements from list
    pub fn lrange(&self, db: DatabaseIndex, key: &[u8], start: isize, stop: isize) -> Result<Vec<Vec<u8>>> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                // Extract data first, then touch
                let result = match &stored_value.value {
                    Value::List(list) => {
                        let len = list.len() as isize;
                        
                        // Convert negative indices
                        let start = if start < 0 { (len + start).max(0) } else { start } as usize;
                        let stop = if stop < 0 { (len + stop).max(0) } else { stop } as usize;
                        
                        // Collect range
                        let mut result = Vec::new();
                        for (i, item) in list.iter().enumerate() {
                            if i >= start && i <= stop {
                                result.push(item.clone());
                            }
                            if i > stop {
                                break;
                            }
                        }
                        result
                    }
                    _ => return Err(StorageError::WrongType.into()),
                };
                stored_value.touch();
                Ok(result)
            }
            None => Ok(Vec::new()),
        }
    }
    
    /// Get element at index
    pub fn lindex(&self, db: DatabaseIndex, key: &[u8], index: isize) -> Result<Option<Vec<u8>>> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                // Get element first, then touch
                let result = match &stored_value.value {
                    Value::List(list) => {
                        let len = list.len() as isize;
                        let idx = if index < 0 { len + index } else { index };
                        
                        if idx >= 0 && idx < len {
                            list.get(idx as usize).cloned()
                        } else {
                            None
                        }
                    }
                    _ => return Err(StorageError::WrongType.into()),
                };
                stored_value.touch();
                Ok(result)
            }
            None => Ok(None),
        }
    }
    
    /// Set element at index
    pub fn lset(&self, db: DatabaseIndex, key: Key, index: isize, value: Vec<u8>) -> Result<()> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(&key) {
            Some(stored_value) => {
                match &mut stored_value.value {
                    Value::List(list) => {
                        let len = list.len() as isize;
                        let idx = if index < 0 { len + index } else { index };
                        
                        if idx >= 0 && idx < len {
                            list[idx as usize] = value;
                            stored_value.touch();
                            Ok(())
                        } else {
                            Err(FerrousError::Command(CommandError::IndexOutOfRange))
                        }
                    }
                    _ => Err(StorageError::WrongType.into()),
                }
            }
            None => Err(FerrousError::Command(CommandError::NoSuchKey)),
        }
    }
    
    /// Trim list to specified range
    pub fn ltrim(&self, db: DatabaseIndex, key: Key, start: isize, stop: isize) -> Result<()> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(&key) {
            Some(stored_value) => {
                match &mut stored_value.value {
                    Value::List(list) => {
                        let len = list.len() as isize;
                        
                        // Convert negative indices
                        let start = if start < 0 { (len + start).max(0) } else { start } as usize;
                        let stop = if stop < 0 { (len + stop).max(0) } else { stop } as usize;
                        
                        // Create new list with range
                        let mut new_list = VecDeque::new();
                        for (i, item) in list.iter().enumerate() {
                            if i >= start && i <= stop {
                                new_list.push_back(item.clone());
                            }
                        }
                        
                        *list = new_list;
                        
                        // Remove empty list
                        if list.is_empty() {
                            db_guard.data.remove(&key);
                        } else {
                            stored_value.touch();
                        }
                        
                        Ok(())
                    }
                    _ => Err(StorageError::WrongType.into()),
                }
            }
            None => Ok(()),
        }
    }
    
    /// Remove elements from list
    pub fn lrem(&self, db: DatabaseIndex, key: Key, count: isize, element: Vec<u8>) -> Result<usize> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(&key) {
            Some(stored_value) => {
                match &mut stored_value.value {
                    Value::List(list) => {
                        let mut removed = 0;
                        
                        if count == 0 {
                            // Remove all occurrences
                            list.retain(|item| {
                                if item == &element {
                                    removed += 1;
                                    false
                                } else {
                                    true
                                }
                            });
                        } else if count > 0 {
                            // Remove from head
                            let mut new_list = VecDeque::new();
                            let mut to_remove = count as usize;
                            
                            for item in list.drain(..) {
                                if item == element && to_remove > 0 {
                                    to_remove -= 1;
                                    removed += 1;
                                } else {
                                    new_list.push_back(item);
                                }
                            }
                            
                            *list = new_list;
                        } else {
                            // Remove from tail
                            let mut new_list = VecDeque::new();
                            let mut to_remove = (-count) as usize;
                            
                            for item in list.drain(..).rev() {
                                if item == element && to_remove > 0 {
                                    to_remove -= 1;
                                    removed += 1;
                                } else {
                                    new_list.push_front(item);
                                }
                            }
                            
                            *list = new_list;
                        }
                        
                        // Remove empty list
                        if list.is_empty() {
                            db_guard.data.remove(&key);
                        } else {
                            stored_value.touch();
                        }
                        
                        Ok(removed)
                    }
                    _ => Err(StorageError::WrongType.into()),
                }
            }
            None => Ok(0),
        }
    }

    /// Add members to a set
    pub fn sadd(&self, db: DatabaseIndex, key: Key, members: Vec<Vec<u8>>) -> Result<usize> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        let added = match db_guard.data.get_mut(&key) {
            Some(stored_value) => {
                match &mut stored_value.value {
                    Value::Set(set) => {
                        // Add to existing set
                        let mut added = 0;
                        for member in members {
                            if set.insert(member) {
                                added += 1;
                            }
                        }
                        stored_value.touch();
                        added
                    }
                    _ => return Err(StorageError::WrongType.into()),
                }
            }
            None => {
                // Create new set
                let mut set = HashSet::new();
                let mut added = 0;
                for member in members {
                    if set.insert(member) {
                        added += 1;
                    }
                }
                
                let stored_value = StoredValue::new(Value::Set(set));
                db_guard.data.insert(key.clone(), stored_value);
                db_guard.mark_modified(&key);
                added
            }
        };
        
        Ok(added)
    }
    
    /// Remove members from a set - Support Vec<&Vec<u8>> arguments for command handlers
    pub fn srem<'a, T: AsRef<[u8]>>(&self, db: DatabaseIndex, key: &[u8], members: &[T]) -> Result<usize> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                match &mut stored_value.value {
                    Value::Set(set) => {
                        let mut removed = 0;
                        for member in members {
                            if set.remove(member.as_ref()) {
                                removed += 1;
                            }
                        }
                        
                        // Remove empty set
                        if set.is_empty() {
                            db_guard.data.remove(key);
                        } else {
                            stored_value.touch();
                        }
                        db_guard.mark_modified(key);
                        
                        Ok(removed)
                    }
                    _ => Err(StorageError::WrongType.into()),
                }
            }
            None => Ok(0),
        }
    }
    
    /// Get all members of a set
    pub fn smembers(&self, db: DatabaseIndex, key: &[u8]) -> Result<Vec<Vec<u8>>> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                // Clone members first, then touch
                let members = match &stored_value.value {
                    Value::Set(set) => set.iter().cloned().collect(),
                    _ => return Err(StorageError::WrongType.into()),
                };
                stored_value.touch();
                Ok(members)
            }
            None => Ok(Vec::new()),
        }
    }
    
    /// Check if member exists in set
    pub fn sismember(&self, db: DatabaseIndex, key: &[u8], member: &[u8]) -> Result<bool> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                // Check membership first, then touch
                let is_member = match &stored_value.value {
                    Value::Set(set) => set.contains(member),
                    _ => return Err(StorageError::WrongType.into()),
                };
                stored_value.touch();
                Ok(is_member)
            }
            None => Ok(false),
        }
    }
    
    /// Get cardinality of a set
    pub fn scard(&self, db: DatabaseIndex, key: &[u8]) -> Result<usize> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                // Get length first, then touch
                let len = match &stored_value.value {
                    Value::Set(set) => set.len(),
                    _ => return Err(StorageError::WrongType.into()),
                };
                stored_value.touch();
                Ok(len)
            }
            None => Ok(0),
        }
    }
    
    /// Get union of multiple sets - Support Vec<&Vec<u8>> arguments for command handlers
    pub fn sunion<'a, T: AsRef<[u8]>>(&self, db: DatabaseIndex, keys: &[T]) -> Result<Vec<Vec<u8>>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        let mut result = HashSet::new();
        
        for key in keys {
            if let Some(stored_value) = db_guard.data.get_mut(key.as_ref()) {
                match &stored_value.value {
                    Value::Set(set) => {
                        for member in set {
                            result.insert(member.clone());
                        }
                        stored_value.touch();
                    }
                    _ => return Err(StorageError::WrongType.into()),
                }
            }
        }
        
        Ok(result.into_iter().collect())
    }
    
    /// Get intersection of multiple sets - Support Vec<&Vec<u8>> arguments for command handlers
    pub fn sinter<'a, T: AsRef<[u8]>>(&self, db: DatabaseIndex, keys: &[T]) -> Result<Vec<Vec<u8>>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        // Get first set as base
        let first_key = keys[0].as_ref();
        let result: HashSet<Vec<u8>> = match db_guard.data.get_mut(first_key) {
            Some(stored_value) => {
                stored_value.touch();
                match &stored_value.value {
                    Value::Set(set) => set.iter().cloned().collect(),
                    _ => return Err(StorageError::WrongType.into()),
                }
            }
            None => return Ok(Vec::new()), // Empty set
        };
        
        // Intersect with other sets
        let mut result: HashSet<Vec<u8>> = result;
        for k in 1..keys.len() {
            let key = keys[k].as_ref();
            if let Some(stored_value) = db_guard.data.get_mut(key) {
                stored_value.touch();
                match &stored_value.value {
                    Value::Set(set) => {
                        result.retain(|member| set.contains(member));
                    }
                    _ => return Err(StorageError::WrongType.into()),
                }
            } else {
                // Non-existent key means empty intersection
                return Ok(Vec::new());
            }
        }
        
        Ok(result.into_iter().collect())
    }
    
    /// Get difference of sets - Support Vec<&Vec<u8>> arguments for command handlers
    pub fn sdiff<'a, T: AsRef<[u8]>>(&self, db: DatabaseIndex, keys: &[T]) -> Result<Vec<Vec<u8>>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        // Get first set as base
        let first_key = keys[0].as_ref();
        let result: HashSet<Vec<u8>> = match db_guard.data.get_mut(first_key) {
            Some(stored_value) => {
                stored_value.touch();
                match &stored_value.value {
                    Value::Set(set) => set.iter().cloned().collect(),
                    _ => return Err(StorageError::WrongType.into()),
                }
            }
            None => return Ok(Vec::new()),
        };
        
        // Remove elements from other sets
        let mut result = result;
        for k in 1..keys.len() {
            let key = keys[k].as_ref();
            if let Some(stored_value) = db_guard.data.get_mut(key) {
                stored_value.touch();
                match &stored_value.value {
                    Value::Set(set) => {
                        for member in set {
                            result.remove(member);
                        }
                    }
                    _ => return Err(StorageError::WrongType.into()),
                }
            }
        }
        
        Ok(result.into_iter().collect())
    }
    
    /// Get random members from a set
    pub fn srandmember(&self, db: DatabaseIndex, key: &[u8], count: i64) -> Result<Vec<Vec<u8>>> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                // Extract and process members first, then touch
                let result = match &stored_value.value {
                    Value::Set(set) => {
                        let members: Vec<Vec<u8>> = set.iter().cloned().collect();
                        if members.is_empty() {
                            Vec::new()
                        } else {
                            let mut rng = rand::thread_rng();
                            
                            if count >= 0 {
                                // Return unique members
                                let n = std::cmp::min(count as usize, members.len());
                                let mut result = members;
                                result.shuffle(&mut rng);
                                result.truncate(n);
                                result
                            } else {
                                // Allow duplicates
                                let n = (-count) as usize;
                                let mut result = Vec::with_capacity(n);
                                for _ in 0..n {
                                    if let Some(member) = members.choose(&mut rng) {
                                        result.push(member.clone());
                                    }
                                }
                                result
                            }
                        }
                    }
                    _ => return Err(StorageError::WrongType.into()),
                };
                stored_value.touch();
                Ok(result)
            }
            None => Ok(Vec::new()),
        }
    }
    
    /// Pop random members from a set
    pub fn spop(&self, db: DatabaseIndex, key: Key, count: usize) -> Result<Vec<Vec<u8>>> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(&key) {
            Some(stored_value) => {
                match &mut stored_value.value {
                    Value::Set(set) => {
                        let mut members: Vec<Vec<u8>> = set.iter().cloned().collect();
                        if members.is_empty() {
                            return Ok(Vec::new());
                        }
                        
                        let mut rng = rand::thread_rng();
                        members.shuffle(&mut rng);
                        
                        let n = std::cmp::min(count, members.len());
                        let result: Vec<Vec<u8>> = members.drain(..n).collect();
                        
                        // Remove popped members
                        for member in &result {
                            set.remove(member);
                        }
                        
                        // Remove empty set
                        if set.is_empty() {
                            db_guard.data.remove(&key);
                        } else {
                            stored_value.touch();
                        }
                        
                        Ok(result)
                    }
                    _ => Err(StorageError::WrongType.into()),
                }
            }
            None => Ok(Vec::new()),
        }
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

    /// Set hash fields
    pub fn hset(&self, db: DatabaseIndex, key: Key, field_values: Vec<(Vec<u8>, Vec<u8>)>) -> Result<usize> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        let fields_added = match db_guard.data.get_mut(&key) {
            Some(stored_value) => {
                match &mut stored_value.value {
                    Value::Hash(hash) => {
                        // Set fields in existing hash
                        let mut added = 0;
                        for (field, value) in field_values {
                            if hash.insert(field, value).is_none() {
                                added += 1;
                            }
                        }
                        stored_value.touch();
                        added
                    }
                    _ => return Err(StorageError::WrongType.into()),
                }
            }
            None => {
                // Create new hash
                let mut hash = HashMap::new();
                let len = field_values.len();
                for (field, value) in field_values {
                    hash.insert(field, value);
                }
                
                let stored_value = StoredValue::new(Value::Hash(hash));
                db_guard.data.insert(key.clone(), stored_value);
                db_guard.mark_modified(&key);
                len // All fields are new
            }
        };
        
        Ok(fields_added)
    }
    
    /// Get hash field value
    pub fn hget(&self, db: DatabaseIndex, key: &[u8], field: &[u8]) -> Result<Option<Vec<u8>>> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                // Get value first, then touch
                let value = match &stored_value.value {
                    Value::Hash(hash) => hash.get(field).cloned(),
                    _ => return Err(StorageError::WrongType.into()),
                };
                stored_value.touch();
                Ok(value)
            }
            None => Ok(None),
        }
    }
    
    /// Get multiple hash field values - Support Vec<&Vec<u8>> arguments for command handlers
    pub fn hmget<'a, T: AsRef<[u8]>>(&self, db: DatabaseIndex, key: &[u8], fields: &[T]) -> Result<Vec<Option<Vec<u8>>>> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                stored_value.touch();
                match &stored_value.value {
                    Value::Hash(hash) => Ok(fields.iter().map(|field| hash.get(field.as_ref()).cloned()).collect()),
                    _ => Err(StorageError::WrongType.into()),
                }
            }
            None => Ok(vec![None; fields.len()]),
        }
    }
    
    /// Get all hash fields and values
    pub fn hgetall(&self, db: DatabaseIndex, key: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                // Clone pairs first, then touch
                let pairs = match &stored_value.value {
                    Value::Hash(hash) => hash.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                    _ => return Err(StorageError::WrongType.into()),
                };
                stored_value.touch();
                Ok(pairs)
            }
            None => Ok(Vec::new()),
        }
    }
    
    /// Delete hash fields - Support Vec<&Vec<u8>> arguments for command handlers
    pub fn hdel<'a, T: AsRef<[u8]>>(&self, db: DatabaseIndex, key: Key, fields: &[T]) -> Result<usize> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(&key) {
            Some(stored_value) => {
                match &mut stored_value.value {
                    Value::Hash(hash) => {
                        let mut deleted = 0;
                        for field in fields {
                            if hash.remove(field.as_ref()).is_some() {
                                deleted += 1;
                            }
                        }
                        
                        // Remove empty hash
                        if hash.is_empty() {
                            db_guard.data.remove(&key);
                        } else {
                            stored_value.touch();
                        }
                        db_guard.mark_modified(&key);
                        
                        Ok(deleted)
                    }
                    _ => Err(StorageError::WrongType.into()),
                }
            }
            None => Ok(0),
        }
    }
    
    /// Get hash length
    pub fn hlen(&self, db: DatabaseIndex, key: &[u8]) -> Result<usize> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                // Get length first, then touch
                let len = match &stored_value.value {
                    Value::Hash(hash) => hash.len(),
                    _ => return Err(StorageError::WrongType.into()),
                };
                stored_value.touch();
                Ok(len)
            }
            None => Ok(0),
        }
    }
    
    /// Check if hash field exists
    pub fn hexists(&self, db: DatabaseIndex, key: &[u8], field: &[u8]) -> Result<bool> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                // Check existence first, then touch
                let exists = match &stored_value.value {
                    Value::Hash(hash) => hash.contains_key(field),
                    _ => return Err(StorageError::WrongType.into()),
                };
                stored_value.touch();
                Ok(exists)
            }
            None => Ok(false),
        }
    }
    
    /// Get all hash field names
    pub fn hkeys(&self, db: DatabaseIndex, key: &[u8]) -> Result<Vec<Vec<u8>>> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                // Get keys first, then touch
                let keys = match &stored_value.value {
                    Value::Hash(hash) => hash.keys().cloned().collect(),
                    _ => return Err(StorageError::WrongType.into()),
                };
                stored_value.touch();
                Ok(keys)
            }
            None => Ok(Vec::new()),
        }
    }
    
    /// Get all hash values
    pub fn hvals(&self, db: DatabaseIndex, key: &[u8]) -> Result<Vec<Vec<u8>>> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                // Get values first, then touch
                let values = match &stored_value.value {
                    Value::Hash(hash) => hash.values().cloned().collect(),
                    _ => return Err(StorageError::WrongType.into()),
                };
                stored_value.touch();
                Ok(values)
            }
            None => Ok(Vec::new()),
        }
    }
    
    /// Increment hash field by integer value
    pub fn hincrby(&self, db: DatabaseIndex, key: Key, field: Vec<u8>, increment: i64) -> Result<i64> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        let new_value = match db_guard.data.get_mut(&key) {
            Some(stored_value) => {
                match &mut stored_value.value {
                    Value::Hash(hash) => {
                        let new_val = match hash.get(&field) {
                            Some(current_bytes) => {
                                // Parse current value as integer
                                let current_str = String::from_utf8_lossy(current_bytes);
                                match current_str.parse::<i64>() {
                                    Ok(current) => current + increment,
                                    Err(_) => return Err(FerrousError::Command(CommandError::NotInteger)),
                                }
                            }
                            None => increment,
                        };
                        
                        hash.insert(field, new_val.to_string().into_bytes());
                        stored_value.touch();
                        new_val
                    }
                    _ => return Err(StorageError::WrongType.into()),
                }
            }
            None => {
                // Create new hash with single field
                let mut hash = HashMap::new();
                hash.insert(field, increment.to_string().into_bytes());
                
                let stored_value = StoredValue::new(Value::Hash(hash));
                db_guard.data.insert(key.clone(), stored_value);
                db_guard.mark_modified(&key);
                increment
            }
        };
        
        Ok(new_value)
    }

    /// Append value to a string
    pub fn append(&self, db: DatabaseIndex, key: Key, value: Vec<u8>) -> Result<usize> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        let new_len = match db_guard.data.get_mut(&key) {
            Some(stored_value) => {
                // Create code path without nested mutable borrow
                match &stored_value.value {
                    Value::String(ref bytes) => {
                        // Get a copy of the current bytes
                        let mut new_bytes = bytes.clone();
                        // Append the new value
                        new_bytes.extend_from_slice(&value);
                        let len = new_bytes.len();
                        
                        // Replace the string value
                        stored_value.value = Value::String(new_bytes);
                        stored_value.touch();
                        db_guard.mark_modified(&key);
                        len
                    }
                    _ => return Err(StorageError::WrongType.into()),
                }
            }
            None => {
                // Create new string
                let len = value.len();
                let stored_value = StoredValue::new(Value::String(value));
                db_guard.data.insert(key.clone(), stored_value);
                db_guard.mark_modified(&key);
                len
            }
        };
        
        Ok(new_len)
    }
    
    /// Get string length
    pub fn strlen(&self, db: DatabaseIndex, key: &[u8]) -> Result<usize> {
        let database = self.get_database(db)?;
        let db_guard = database.read().unwrap();
        
        match db_guard.data.get(key) {
            Some(stored_value) => {
                match &stored_value.value {
                    Value::String(bytes) => Ok(bytes.len()),
                    _ => Err(StorageError::WrongType.into()),
                }
            }
            None => Ok(0),
        }
    }
    
    /// Get substring
    pub fn getrange(&self, db: DatabaseIndex, key: &[u8], start: isize, end: isize) -> Result<Vec<u8>> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        match db_guard.data.get_mut(key) {
            Some(stored_value) => {
                // Get substring first, then touch
                let substring = match &stored_value.value {
                    Value::String(bytes) => {
                        let len = bytes.len() as isize;
                        
                        // Convert negative indices
                        let start = if start < 0 {
                            std::cmp::max(0, len + start) as usize
                        } else {
                            start as usize
                        };
                        
                        let end = if end < 0 {
                            std::cmp::max(-1, len + end) as usize
                        } else {
                            std::cmp::min(end as usize, len as usize - 1)
                        };
                        
                        if start > end || start >= bytes.len() {
                            Vec::new()
                        } else {
                            bytes[start..=end].to_vec()
                        }
                    }
                    _ => return Err(StorageError::WrongType.into()),
                };
                
                stored_value.touch();
                Ok(substring)
            }
            None => Ok(Vec::new()),
        }
    }
    
    /// Set substring
    pub fn setrange(&self, db: DatabaseIndex, key: Key, offset: usize, value: Vec<u8>) -> Result<usize> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        let new_len = match db_guard.data.get_mut(&key) {
            Some(stored_value) => {
                // Create code path without nested mutable borrow
                match &stored_value.value {
                    Value::String(ref bytes) => {
                        // Get a copy of the current bytes
                        let mut new_bytes = bytes.clone();
                        
                        // Extend if needed
                        let required_len = offset + value.len();
                        if required_len > new_bytes.len() {
                            new_bytes.resize(required_len, 0);
                        }
                        
                        // Set the range
                        new_bytes[offset..offset + value.len()].copy_from_slice(&value);
                        let len = new_bytes.len();
                        
                        // Replace the string value
                        stored_value.value = Value::String(new_bytes);
                        stored_value.touch();
                        db_guard.mark_modified(&key);
                        len
                    }
                    _ => return Err(StorageError::WrongType.into()),
                }
            }
            None => {
                // Create new string with padding
                let mut new_string = vec![0; offset + value.len()];
                new_string[offset..].copy_from_slice(&value);
                let len = new_string.len();
                
                let stored_value = StoredValue::new(Value::String(new_string));
                db_guard.data.insert(key.clone(), stored_value);
                db_guard.mark_modified(&key);
                len
            }
        };
        
        Ok(new_len)
    }
    
    /// Get key type
    pub fn key_type(&self, db: DatabaseIndex, key: &[u8]) -> Result<String> {
        let database = self.get_database(db)?;
        let db_guard = database.read().unwrap();
        
        match db_guard.data.get(key) {
            Some(stored_value) => {
                let type_name = match &stored_value.value {
                    Value::String(_) => "string",
                    Value::List(_) => "list",
                    Value::Set(_) => "set",
                    Value::Hash(_) => "hash",
                    Value::SortedSet(_) => "zset",
                };
                Ok(type_name.to_string())
            }
            None => Ok("none".to_string()),
        }
    }
    
    /// Rename a key
    pub fn rename(&self, db: DatabaseIndex, old_key: &[u8], new_key: Key) -> Result<()> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        // Get the value
        let stored_value = db_guard.data.remove(old_key)
            .ok_or_else(|| FerrousError::Command(CommandError::NoSuchKey))?;
        
        // Insert with new key (overwrites if exists)
        db_guard.data.insert(new_key.clone(), stored_value);
        db_guard.mark_modified(&new_key);
        
        Ok(())
    }
    
    /// Find keys matching pattern
    pub fn keys(&self, db: DatabaseIndex, pattern: &[u8]) -> Result<Vec<Vec<u8>>> {
        let database = self.get_database(db)?;
        let db_guard = database.read().unwrap();
        
        let pattern_str = String::from_utf8_lossy(pattern);
        let mut matching_keys = Vec::new();
        
        for key in db_guard.data.keys() {
            let key_str = String::from_utf8_lossy(key);
            if pattern_matches(&pattern_str, &key_str) {
                matching_keys.push(key.clone());
            }
        }
        
        Ok(matching_keys)
    }
    
    /// Set expiration in milliseconds
    pub fn pexpire(&self, db: DatabaseIndex, key: &[u8], millis: u64) -> Result<bool> {
        self.expire(db, key, Duration::from_millis(millis))
    }
    
    /// Get TTL in milliseconds
    pub fn pttl(&self, db: DatabaseIndex, key: &[u8]) -> Result<i64> {
        let ttl = self.ttl(db, key)?;
        
        match ttl {
            Some(duration) => Ok(duration.as_millis() as i64),
            None => {
                if self.exists(db, key)? {
                    Ok(-1) // Key exists but no expiration
                } else {
                    Ok(-2) // Key doesn't exist
                }
            }
        }
    }
    
    /// Remove expiration
    pub fn persist(&self, db: DatabaseIndex, key: &[u8]) -> Result<bool> {
        let database = self.get_database(db)?;
        let mut db_guard = database.write().unwrap();
        
        if let Some(stored_value) = db_guard.data.get_mut(key) {
            if stored_value.metadata.expires_at.is_some() {
                stored_value.metadata.clear_expiration();
                db_guard.expiring_keys.remove(key);
                Ok(true)
            } else {
                Ok(false) // Key exists but has no expiration
            }
        } else {
            Ok(false) // Key doesn't exist
        }
    }

    /// Check if a key was modified (for WATCH command)
    pub fn was_modified(&self, db: DatabaseIndex, key: &[u8]) -> Result<bool> {
        let database = self.get_database(db)?;
        let db_guard = database.read().unwrap();
        
        // For now, always return false (no tracking implemented)
        // In a full implementation, we'd track modification versions
        Ok(false)
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
            modified_keys: HashMap::new(),
            modification_counter: 0,
        }
    }
    
    /// Mark a key as modified
    fn mark_modified(&mut self, key: &[u8]) {
        self.modification_counter += 1;
        self.modified_keys.insert(key.to_vec(), self.modification_counter);
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

/// Simple glob pattern matching
fn pattern_matches(pattern: &str, text: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();
    
    let mut p_idx = 0;
    let mut t_idx = 0;
    let mut star_idx = None;
    let mut star_match_idx = 0;
    
    while t_idx < text_chars.len() {
        if p_idx < pattern_chars.len() {
            match pattern_chars[p_idx] {
                '?' => {
                    p_idx += 1;
                    t_idx += 1;
                    continue;
                }
                '*' => {
                    star_idx = Some(p_idx);
                    star_match_idx = t_idx;
                    p_idx += 1;
                    continue;
                }
                '[' => {
                    // Character class - simplified implementation
                    if let Some(end) = pattern_chars[p_idx..].iter().position(|&c| c == ']') {
                        let class_end = p_idx + end;
                        let negate = p_idx + 1 < class_end && pattern_chars[p_idx + 1] == '^';
                        let start_idx = if negate { p_idx + 2 } else { p_idx + 1 };
                        
                        let mut matched = false;
                        let mut i = start_idx;
                        while i < class_end {
                            if i + 2 < class_end && pattern_chars[i + 1] == '-' {
                                // Range
                                if text_chars[t_idx] >= pattern_chars[i] && text_chars[t_idx] <= pattern_chars[i + 2] {
                                    matched = true;
                                    break;
                                }
                                i += 3;
                            } else {
                                // Single char
                                if text_chars[t_idx] == pattern_chars[i] {
                                    matched = true;
                                    break;
                                }
                                i += 1;
                            }
                        }
                        
                        if matched != negate {
                            p_idx = class_end + 1;
                            t_idx += 1;
                            continue;
                        }
                    }
                }
                '\\' if p_idx + 1 < pattern_chars.len() => {
                    // Escaped character
                    if pattern_chars[p_idx + 1] == text_chars[t_idx] {
                        p_idx += 2;
                        t_idx += 1;
                        continue;
                    }
                }
                _ => {
                    if pattern_chars[p_idx] == text_chars[t_idx] {
                        p_idx += 1;
                        t_idx += 1;
                        continue;
                    }
                }
            }
        }
        
        // No match, try to backtrack to last *
        if let Some(star_pos) = star_idx {
            p_idx = star_pos + 1;
            star_match_idx += 1;
            t_idx = star_match_idx;
        } else {
            return false;
        }
    }
    
    // Skip trailing * in pattern
    while p_idx < pattern_chars.len() && pattern_chars[p_idx] == '*' {
        p_idx += 1;
    }
    
    p_idx == pattern_chars.len()
}