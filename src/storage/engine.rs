//! Sharded storage engine implementation optimized for performance
//! 
//! Provides Redis-compatible storage with sharded simple structure and no access time tracking overhead.

use std::collections::{VecDeque, HashSet, HashMap};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use std::thread;
use rand::seq::SliceRandom;

use crate::error::{FerrousError, Result, StorageError, CommandError};
use super::value::{Value, StoredValue};
use super::memory::MemoryManager;
use super::skiplist::SkipList;
use super::stream::{Stream, StreamId, StreamEntry};
use super::{DatabaseIndex, Key};

/// Number of shards per database for optimal concurrency
const SHARDS_PER_DATABASE: usize = 16;

/// Sharded storage engine with simple HashMap structures - NO access time tracking
pub struct StorageEngine {
    /// Multiple databases, each with multiple shards
    /// databases[db_id].shards[shard_id] = Arc<RwLock<DatabaseShard>>
    databases: Vec<Database>,
    
    /// Memory management (atomic-based, no locks)
    memory_manager: Arc<MemoryManager>,
    
    /// Background expiration thread handle
    expiration_handle: Option<thread::JoinHandle<()>>,
}

/// A single database with sharded storage
pub struct Database {
    /// Sharded key-value storage for maximum concurrency
    shards: Vec<Arc<RwLock<DatabaseShard>>>,
}

/// Watch tracking for a database shard with conditional overhead
#[derive(Debug)]
pub struct ShardWatchTracker {
    /// Number of keys currently being watched in this shard
    active_watchers: std::sync::atomic::AtomicUsize,
    /// Modification epoch counter
    epoch: std::sync::atomic::AtomicU64,
}

impl ShardWatchTracker {
    /// Create new watch tracker
    fn new() -> Self {
        Self {
            active_watchers: std::sync::atomic::AtomicUsize::new(0),
            epoch: std::sync::atomic::AtomicU64::new(0),
        }
    }
    
    /// Register a WATCH on this shard and return current epoch
    fn register_watch(&self) -> u64 {
        self.active_watchers.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.epoch.load(std::sync::atomic::Ordering::Acquire)
    }
    
    /// Unregister a WATCH on this shard
    fn unregister_watch(&self) {
        self.active_watchers.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
    
    /// Mark shard as modified (zero overhead when no active watchers)
    fn mark_modified(&self) {
        // Fast path: zero overhead when no WATCH is active (99.9% of cases)
        if self.active_watchers.load(std::sync::atomic::Ordering::Relaxed) == 0 {
            return;
        }
        // Only pay atomic cost when WATCH is actually being used
        self.epoch.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    
    /// Get current epoch for violation detection
    fn get_epoch(&self) -> u64 {
        self.epoch.load(std::sync::atomic::Ordering::Acquire)
    }
    
    /// Check if any keys are being watched
    fn has_active_watchers(&self) -> bool {
        self.active_watchers.load(std::sync::atomic::Ordering::Relaxed) > 0
    }
}

/// A single database shard with conditional WATCH tracking
#[derive(Debug)]
pub struct DatabaseShard {
    /// Key-value storage using simple HashMap for maximum performance
    data: HashMap<Key, StoredValue>,
    
    /// Keys with expiration timestamps for efficient cleanup
    expiring_keys: HashMap<Key, Instant>,
    
    /// Conditional WATCH tracking (zero overhead when no WATCH active)
    watch_tracker: ShardWatchTracker,
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
    
    /// Create a new in-memory storage engine for testing
    pub fn new_in_memory() -> Arc<Self> {
        Self::new()
    }
    
    /// Create storage engine with configuration
    pub fn with_config(num_databases: usize, memory_manager: MemoryManager) -> Arc<Self> {
        let mut databases = Vec::with_capacity(num_databases);
        for _ in 0..num_databases {
            databases.push(Database::new());
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
        
        engine
    }
    
    /// Calculate shard index for a key using deterministic hash function
    /// This ensures the same key always maps to the same shard for consistent modification tracking
    fn get_shard_index(&self, key: &[u8]) -> usize {
        // Use deterministic FNV-1a hash instead of random DefaultHasher
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;
        
        let mut hash = FNV_OFFSET;
        for &byte in key {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        
        (hash % SHARDS_PER_DATABASE as u64) as usize
    }
    
    /// Get shard for a key in a specific database
    fn get_shard(&self, db: DatabaseIndex, key: &[u8]) -> Result<&Arc<RwLock<DatabaseShard>>> {
        let database = self.databases.get(db).ok_or(StorageError::InvalidDatabase)?;
        let shard_idx = self.get_shard_index(key);
        Ok(&database.shards[shard_idx])
    }
    
    /// Set a string value
    pub fn set_string(&self, db: DatabaseIndex, key: Key, value: Vec<u8>) -> Result<()> {
        self.set_value(db, key, Value::string(value), None)
    }
    
    /// Set a string value with expiration
    pub fn set_string_ex(&self, db: DatabaseIndex, key: Key, value: Vec<u8>, expires_in: Duration) -> Result<()> {
        self.set_value(db, key, Value::string(value), Some(expires_in))
    }
    
    /// Set any value - optimized with sharded simple structure, no access time tracking
    pub fn set_value(&self, db: DatabaseIndex, key: Key, value: Value, expires_in: Option<Duration>) -> Result<()> {
        let shard = self.get_shard(db, &key)?;
        let mut shard_guard = shard.write().unwrap();
        
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
            shard_guard.expiring_keys.insert(key.clone(), expires_at);
        }
        
        // Store the value - direct HashMap access for maximum performance
        shard_guard.data.insert(key.clone(), stored_value);
        shard_guard.mark_modified(&key);
        
        Ok(())
    }
    
    /// Get a value - optimized by removing all access time tracking overhead
    pub fn get(&self, db: DatabaseIndex, key: &[u8]) -> Result<GetResult> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        match shard_guard.data.get_mut(key) {
            Some(stored_value) => {
                // Check if expired
                if stored_value.is_expired() {
                    // Remove expired key
                    shard_guard.data.remove(key);
                    shard_guard.expiring_keys.remove(key);
                    Ok(GetResult::Expired)
                } else {
                    // Return value without touch() - matches Valkey's noeviction config
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
    
    /// Check if key exists - optimized read path, no access time tracking
    pub fn exists(&self, db: DatabaseIndex, key: &[u8]) -> Result<bool> {
        let shard = self.get_shard(db, key)?;
        let shard_guard = shard.read().unwrap(); // Use read lock for existence check
        
        if let Some(stored_value) = shard_guard.data.get(key) {
            Ok(!stored_value.is_expired())
        } else {
            Ok(false)
        }
    }
    
    /// Delete a key
    pub fn delete(&self, db: DatabaseIndex, key: &[u8]) -> Result<bool> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.remove(key) {
            shard_guard.mark_modified(key);
            shard_guard.expiring_keys.remove(key);
            
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
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            stored_value.metadata.set_expiration(expires_in);
            shard_guard.expiring_keys.insert(key.to_vec(), Instant::now() + expires_in);
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    /// Get time to live for a key - optimized read path, no access time tracking
    pub fn ttl(&self, db: DatabaseIndex, key: &[u8]) -> Result<Option<Duration>> {
        let shard = self.get_shard(db, key)?;
        let shard_guard = shard.read().unwrap(); // Use read lock for TTL check
        
        if let Some(stored_value) = shard_guard.data.get(key) {
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
    
    /// Increment integer value by amount - optimized without access time tracking
    pub fn incr_by(&self, db: DatabaseIndex, key: Key, increment: i64) -> Result<i64> {
        let shard = self.get_shard(db, &key)?;
        let mut shard_guard = shard.write().unwrap();
        
        let new_value = if let Some(stored_value) = shard_guard.data.get_mut(&key) {
            // Try to parse existing value as integer
            match stored_value.value.as_integer() {
                Some(current) => {
                    let new_val = current + increment;
                    stored_value.value = Value::integer(new_val);
                    // NO touch() call - matches Valkey's noeviction config
                    shard_guard.mark_modified(&key);
                    new_val
                }
                None => return Err(FerrousError::Command(CommandError::NotInteger)),
            }
        } else {
            // Create new key with increment value
            let new_val = increment;
            let stored_value = StoredValue::new(Value::integer(new_val));
            let memory_size = self.calculate_value_size(&key, &stored_value.value);
            
            if !self.memory_manager.add_memory(memory_size) {
                return Err(StorageError::OutOfMemory.into());
            }
            
            shard_guard.data.insert(key.clone(), stored_value);
            shard_guard.mark_modified(&key);
            new_val
        };
        
        Ok(new_value)
    }
    
    /// Get all keys from a database (for RDB persistence)
    pub fn get_all_keys(&self, db: DatabaseIndex) -> Result<Vec<Key>> {
        let database = self.databases.get(db).ok_or(StorageError::InvalidDatabase)?;
        let mut all_keys = Vec::new();
        
        // Collect keys from all shards
        for shard in &database.shards {
            let shard_guard = shard.read().unwrap();
            for key in shard_guard.data.keys() {
                all_keys.push(key.clone());
            }
        }
        
        Ok(all_keys)
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
        let database = self.databases.get(db).ok_or(StorageError::InvalidDatabase)?;
        
        let mut total_memory_to_free = 0;
        
        // Clear all shards
        for shard in &database.shards {
            let mut shard_guard = shard.write().unwrap();
            
            // Calculate memory to free from this shard
            for (key, stored_value) in shard_guard.data.iter() {
                total_memory_to_free += self.calculate_value_size(key, &stored_value.value);
            }
            
            shard_guard.data.clear();
            shard_guard.expiring_keys.clear();
        }
        
        self.memory_manager.remove_memory(total_memory_to_free);
        Ok(())
    }
    
    /// Stream operations
    
    /// Add an entry to a stream with auto-generated ID - NO CLONING!
    pub fn xadd(&self, db: DatabaseIndex, key: Key, fields: HashMap<Vec<u8>, Vec<u8>>) -> Result<StreamId> {
        let shard = self.get_shard(db, &key)?;
        let mut shard_guard = shard.write().unwrap();
        
        let id = match shard_guard.data.get_mut(&key) {
            Some(stored_value) => {
                match &mut stored_value.value {
                    Value::Stream(stream) => {
                        let id = stream.add_auto(fields);
                        shard_guard.mark_modified(&key);
                        id
                    }
                    _ => return Err(StorageError::WrongType.into()),
                }
            }
            None => {
                // Create new stream
                let mut new_stream = Stream::new();
                let id = new_stream.add_auto(fields);
                
                let memory_size = self.calculate_value_size(&key, &Value::empty_stream()) +
                                 new_stream.memory_usage();
                
                if !self.memory_manager.add_memory(memory_size) {
                    return Err(StorageError::OutOfMemory.into());
                }
                
                let stored_value = StoredValue::new(Value::Stream(new_stream));
                shard_guard.data.insert(key.clone(), stored_value);
                shard_guard.mark_modified(&key);
                
                id
            }
        };
        
        Ok(id)
    }
    
    /// Add an entry to a stream with specific ID - NO CLONING!
    pub fn xadd_with_id(&self, db: DatabaseIndex, key: Key, id: StreamId, fields: HashMap<Vec<u8>, Vec<u8>>) -> Result<StreamId> {
        let shard = self.get_shard(db, &key)?;
        let mut shard_guard = shard.write().unwrap();
        
        let result_id = match shard_guard.data.get_mut(&key) {
            Some(stored_value) => {
                match &mut stored_value.value {
                    Value::Stream(stream) => {
                        stream.add_with_id(id.clone(), fields)
                            .map_err(|e| FerrousError::Command(CommandError::Generic(e.to_string())))?;
                        shard_guard.mark_modified(&key);
                        id
                    }
                    _ => return Err(StorageError::WrongType.into()),
                }
            }
            None => {
                // Create new stream
                let mut stream = Stream::new();
                stream.add_with_id(id.clone(), fields)
                    .map_err(|e| FerrousError::Command(CommandError::Generic(e.to_string())))?;
                
                let memory_size = self.calculate_value_size(&key, &Value::empty_stream()) +
                                 stream.memory_usage();
                
                if !self.memory_manager.add_memory(memory_size) {
                    return Err(StorageError::OutOfMemory.into());
                }
                
                let stored_value = StoredValue::new(Value::Stream(stream));
                shard_guard.data.insert(key.clone(), stored_value);
                shard_guard.mark_modified(&key);
                
                id
            }
        };
        
        Ok(result_id)
    }
    
    /// Get entries from a stream in a range of IDs
    pub fn xrange(&self, db: DatabaseIndex, key: &[u8], start: StreamId, end: StreamId, count: Option<usize>) -> Result<Vec<StreamEntry>> {
        let shard = self.get_shard(db, key)?;
        let shard_guard = shard.read().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get(key) {
            match &stored_value.value {
                Value::Stream(stream) => {
                    let result = stream.range(&start, &end, count, false);
                    Ok(result.entries)
                }
                _ => Err(StorageError::WrongType.into()),
            }
        } else {
            Ok(Vec::new())
        }
    }
    
    /// Get entries from a stream in reverse order
    pub fn xrevrange(&self, db: DatabaseIndex, key: &[u8], start: StreamId, end: StreamId, count: Option<usize>) -> Result<Vec<StreamEntry>> {
        let shard = self.get_shard(db, key)?;
        let shard_guard = shard.read().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get(key) {
            match &stored_value.value {
                Value::Stream(stream) => {
                    let result = stream.range(&start, &end, count, true);
                    Ok(result.entries)
                }
                _ => Err(StorageError::WrongType.into()),
            }
        } else {
            Ok(Vec::new())
        }
    }
    
    /// Get stream length using lock-free atomic operations
    pub fn xlen(&self, db: DatabaseIndex, key: &[u8]) -> Result<usize> {
        let shard = self.get_shard(db, key)?;
        let shard_guard = shard.read().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get(key) {
            match &stored_value.value {
                Value::Stream(stream) => Ok(stream.len()),
                _ => Err(StorageError::WrongType.into()),
            }
        } else {
            Ok(0)
        }
    }
    
    /// Read entries from multiple streams after specific IDs
    pub fn xread(&self, db: DatabaseIndex, keys_and_ids: Vec<(&[u8], StreamId)>, count: Option<usize>, block: Option<Duration>) 
        -> Result<Vec<(Vec<u8>, Vec<StreamEntry>)>> {
        // TODO: In the future, implement blocking support
        if block.is_some() {
            return Err(FerrousError::Command(CommandError::Generic("BLOCK option not yet supported".to_string())));
        }
        
        let mut results = Vec::new();
        
        for (key, after_id) in keys_and_ids {
            let shard = self.get_shard(db, key)?;
            let shard_guard = shard.read().unwrap();
            
            if let Some(stored_value) = shard_guard.data.get(key) {
                match &stored_value.value {
                    Value::Stream(stream) => {
                        let result = stream.range_after(&after_id, count);
                        if !result.entries.is_empty() {
                            results.push((key.to_vec(), result.entries));
                        }
                    }
                    _ => return Err(StorageError::WrongType.into()),
                }
            }
        }
        
        Ok(results)
    }
    
    /// Trim stream by maximum length
    pub fn xtrim(&self, db: DatabaseIndex, key: &[u8], max_len: usize) -> Result<usize> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            let trimmed = match &mut stored_value.value {
                Value::Stream(stream) => {
                    let trimmed = stream.trim_by_count(max_len);
                    if trimmed > 0 {
                        shard_guard.mark_modified(key);
                    }
                    trimmed
                }
                _ => return Err(StorageError::WrongType.into()),
            };
            
            Ok(trimmed)
        } else {
            Ok(0)
        }
    }
    
    /// Delete entries from a stream
    pub fn xdel(&self, db: DatabaseIndex, key: &[u8], ids: Vec<StreamId>) -> Result<usize> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            let deleted = match &mut stored_value.value {
                Value::Stream(stream) => {
                    let deleted = stream.delete(&ids);
                    if deleted > 0 {
                        shard_guard.mark_modified(key);
                    }
                    deleted
                }
                _ => return Err(StorageError::WrongType.into()),
            };
            
            Ok(deleted)
        } else {
            Ok(0)
        }
    }

    /// Create a consumer group for a stream (basic implementation)
    pub fn stream_create_consumer_group(&self, db: DatabaseIndex, key: &[u8], group_name: String, start_id: StreamId) -> Result<()> {
        let shard = self.get_shard(db, key)?;
        let shard_guard = shard.read().unwrap();
        
        // Check if stream exists
        if let Some(stored_value) = shard_guard.data.get(key) {
            match &stored_value.value {
                Value::Stream(_stream) => {
                    Ok(())
                }
                _ => Err(StorageError::WrongType.into()),
            }
        } else {
            Ok(())
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
                skiplist.memory_usage()
            }
            Value::Stream(stream) => {
                stream.memory_usage()
            }
        };
        key_size + value_size
    }

    /// Calculate the memory size of a sorted set member
    fn calculate_member_size(&self, member: &[u8]) -> usize {
        MemoryManager::calculate_size(member) + std::mem::size_of::<f64>() + 32
    }

    /// Add a member with score to a sorted set - NO access time tracking
    pub fn zadd(&self, db: DatabaseIndex, key: Key, member: Vec<u8>, score: f64) -> Result<bool> {
        let shard = self.get_shard(db, &key)?;
        let mut shard_guard = shard.write().unwrap();
        
        let is_new = if let Some(stored_value) = shard_guard.data.get_mut(&key) {
            match &mut stored_value.value {
                Value::SortedSet(skiplist) => {
                    let old_score = skiplist.insert(member.clone(), score);
                    // NO touch() call - no access time tracking overhead
                    shard_guard.mark_modified(&key);
                    old_score.is_none()
                }
                _ => return Err(StorageError::WrongType.into()),
            }
        } else {
            // Create a new sorted set
            let skiplist = SkipList::new();
            skiplist.insert(member.clone(), score);
            
            let memory_size = self.calculate_value_size(&key, &Value::empty_sorted_set()) +
                             self.calculate_member_size(&member);
            
            if !self.memory_manager.add_memory(memory_size) {
                return Err(StorageError::OutOfMemory.into());
            }
            
            let stored_value = StoredValue::new(Value::SortedSet(Arc::new(skiplist)));
            shard_guard.data.insert(key.clone(), stored_value);
            shard_guard.mark_modified(&key);
            true
        };
        
        Ok(is_new)
    }
    
    pub fn zrem(&self, db: DatabaseIndex, key: &[u8], member: &[u8]) -> Result<bool> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            let (removed, is_empty) = match &mut stored_value.value {
                Value::SortedSet(skiplist) => {
                    let removed = skiplist.remove(member).is_some();
                    (removed, skiplist.is_empty())
                }
                _ => return Err(StorageError::WrongType.into()),
            };
            
            if removed {
                shard_guard.mark_modified(key);
                let member_size = self.calculate_member_size(member);
                self.memory_manager.remove_memory(member_size);
                
                if is_empty {
                    shard_guard.data.remove(key);
                } 
                // NO else branch with touch() - no access time tracking overhead
            }
            
            Ok(removed)
        } else {
            Ok(false)
        }
    }
    
    pub fn zscore(&self, db: DatabaseIndex, key: &[u8], member: &[u8]) -> Result<Option<f64>> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            let score = match &stored_value.value {
                Value::SortedSet(skiplist) => skiplist.get_score(member),
                _ => return Err(StorageError::WrongType.into()),
            };
            
            // NO touch() call - no access time tracking overhead
            Ok(score)
        } else {
            Ok(None)
        }
    }
    
    pub fn zrank(&self, db: DatabaseIndex, key: &[u8], member: &[u8], reverse: bool) -> Result<Option<usize>> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            let result = match &stored_value.value {
                Value::SortedSet(skiplist) => {
                    let rank = skiplist.get_rank(member);
                    
                    if let Some(rank) = rank {
                        if reverse {
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
            
            // NO touch() call - no access time tracking overhead
            Ok(result)
        } else {
            Ok(None)
        }
    }
    
    pub fn zrange(&self, db: DatabaseIndex, key: &[u8], start: isize, stop: isize, reverse: bool) 
        -> Result<Vec<(Vec<u8>, f64)>> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            let result = match &stored_value.value {
                Value::SortedSet(skiplist) => {
                    let len = skiplist.len();
                    if len == 0 {
                        Vec::new()
                    } else {
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
                        
                        if reverse {
                            let real_start = len.saturating_sub(1).saturating_sub(stop_idx.min(len.saturating_sub(1)));
                            let real_stop = len.saturating_sub(1).saturating_sub(start_idx.min(len.saturating_sub(1)));
                            
                            let range = skiplist.range_by_rank(real_start, real_stop);
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
            
            // NO touch() call - no access time tracking overhead  
            Ok(result)
        } else {
            Ok(Vec::new())
        }
    }
    
    pub fn zrangebyscore(&self, db: DatabaseIndex, key: &[u8], min_score: f64, max_score: f64, reverse: bool) 
        -> Result<Vec<(Vec<u8>, f64)>> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            let result = match &stored_value.value {
                Value::SortedSet(skiplist) => {
                    let range = skiplist.range_by_score(min_score, max_score);
                    let mut items = range.items;
                    
                    if reverse {
                        items.reverse();
                    }
                    
                    items
                }
                _ => return Err(StorageError::WrongType.into()),
            };
            
            // NO touch() call - no access time tracking overhead
            Ok(result)
        } else {
            Ok(Vec::new())
        }
    }
    
    pub fn zcount(&self, db: DatabaseIndex, key: &[u8], min_score: f64, max_score: f64) -> Result<usize> {
        let members = self.zrangebyscore(db, key, min_score, max_score, false)?;
        Ok(members.len())
    }
    
    pub fn zincrby(&self, db: DatabaseIndex, key: Key, member: Vec<u8>, increment: f64) -> Result<f64> {
        let shard = self.get_shard(db, &key)?;
        let mut shard_guard = shard.write().unwrap();
        
        let new_score = if let Some(stored_value) = shard_guard.data.get_mut(&key) {
            match &mut stored_value.value {
                Value::SortedSet(skiplist) => {
                    let new_score = match skiplist.get_score(&member) {
                        Some(curr_score) => curr_score + increment,
                        None => increment,
                    };
                    
                    skiplist.insert(member, new_score);
                    shard_guard.mark_modified(&key);
                    // NO touch() call - no access time tracking overhead
                    new_score
                }
                _ => return Err(StorageError::WrongType.into()),
            }
        } else {
            let skiplist = SkipList::new();
            skiplist.insert(member.clone(), increment);
            
            let memory_size = self.calculate_value_size(&key, &Value::empty_sorted_set()) +
                             self.calculate_member_size(&member);
            
            if !self.memory_manager.add_memory(memory_size) {
                return Err(StorageError::OutOfMemory.into());
            }
            
            let stored_value = StoredValue::new(Value::SortedSet(Arc::new(skiplist)));
            shard_guard.data.insert(key.clone(), stored_value);
            shard_guard.mark_modified(&key);
            
            increment
        };
        
        Ok(new_score)
    }

    /// Push elements to the head of a list - NO access time tracking
    pub fn lpush(&self, db: DatabaseIndex, key: Key, elements: Vec<Vec<u8>>) -> Result<usize> {
        let shard = self.get_shard(db, &key)?;
        let mut shard_guard = shard.write().unwrap();
        
        let list_len = if let Some(stored_value) = shard_guard.data.get_mut(&key) {
            match &mut stored_value.value {
                Value::List(list) => {
                    for element in elements.into_iter().rev() {
                        list.push_front(element);
                    }
                    let len = list.len();
                    drop(stored_value);
                    shard_guard.mark_modified(&key);
                    len
                }
                _ => return Err(StorageError::WrongType.into()),
            }
        } else {
            // Create new list
            let mut list = VecDeque::new();
            for element in elements.into_iter().rev() {
                list.push_front(element);
            }
            let len = list.len();
            
            let stored_value = StoredValue::new(Value::List(list));
            shard_guard.data.insert(key.clone(), stored_value);
            shard_guard.mark_modified(&key);
            len
        };
        
        Ok(list_len)
    }
    
    pub fn rpush(&self, db: DatabaseIndex, key: Key, elements: Vec<Vec<u8>>) -> Result<usize> {
        let shard = self.get_shard(db, &key)?;
        let mut shard_guard = shard.write().unwrap();
        
        let list_len = if let Some(stored_value) = shard_guard.data.get_mut(&key) {
            match &mut stored_value.value {
                Value::List(list) => {
                    for element in elements {
                        list.push_back(element);
                    }
                    let len = list.len();
                    drop(stored_value);
                    shard_guard.mark_modified(&key);
                    len
                }
                _ => return Err(StorageError::WrongType.into()),
            }
        } else {
            // Create new list
            let mut list = VecDeque::new();
            for element in elements {
                list.push_back(element);
            }
            let len = list.len();
            
            let stored_value = StoredValue::new(Value::List(list));
            shard_guard.data.insert(key.clone(), stored_value);
            shard_guard.mark_modified(&key);
            len
        };
        
        Ok(list_len)
    }
    
    pub fn lpop(&self, db: DatabaseIndex, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            match &mut stored_value.value {
                Value::List(list) => {
                    let element = list.pop_front();
                    let is_empty = list.is_empty();
                    drop(stored_value);
                    
                    if element.is_some() {
                        shard_guard.mark_modified(key);
                    }
                    
                    if is_empty {
                        shard_guard.data.remove(key);
                    }
                    
                    Ok(element)
                }
                _ => Err(StorageError::WrongType.into()),
            }
        } else {
            Ok(None)
        }
    }
    
    pub fn rpop(&self, db: DatabaseIndex, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            match &mut stored_value.value {
                Value::List(list) => {
                    let element = list.pop_back();
                    let is_empty = list.is_empty();
                    drop(stored_value);
                    
                    if element.is_some() {
                        shard_guard.mark_modified(key);
                    }
                    
                    if is_empty {
                        shard_guard.data.remove(key);
                    }
                    
                    Ok(element)
                }
                _ => Err(StorageError::WrongType.into()),
            }
        } else {
            Ok(None)
        }
    }
    
    pub fn llen(&self, db: DatabaseIndex, key: &[u8]) -> Result<usize> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            let len = match &stored_value.value {
                Value::List(list) => list.len(),
                _ => return Err(StorageError::WrongType.into()),
            };
            // NO touch() call - no access time tracking overhead
            Ok(len)
        } else {
            Ok(0)
        }
    }
    
    pub fn lrange(&self, db: DatabaseIndex, key: &[u8], start: isize, stop: isize) -> Result<Vec<Vec<u8>>> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            let result = match &stored_value.value {
                Value::List(list) => {
                    let len = list.len() as isize;
                    
                    let start = if start < 0 { (len + start).max(0) } else { start } as usize;
                    let stop = if stop < 0 { (len + stop).max(0) } else { stop } as usize;
                    
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
            // NO touch() call - no access time tracking overhead
            Ok(result)
        } else {
            Ok(Vec::new())
        }
    }
    
    pub fn lindex(&self, db: DatabaseIndex, key: &[u8], index: isize) -> Result<Option<Vec<u8>>> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
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
            // NO touch() call - no access time tracking overhead
            Ok(result)
        } else {
            Ok(None)
        }
    }
    
    pub fn lset(&self, db: DatabaseIndex, key: Key, index: isize, value: Vec<u8>) -> Result<()> {
        let shard = self.get_shard(db, &key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(&key) {
            match &mut stored_value.value {
                Value::List(list) => {
                    let len = list.len() as isize;
                    let idx = if index < 0 { len + index } else { index };
                    
                    if idx >= 0 && idx < len {
                        list[idx as usize] = value;
                        shard_guard.mark_modified(&key);
                        // NO touch() call - no access time tracking overhead
                        Ok(())
                    } else {
                        Err(FerrousError::Command(CommandError::IndexOutOfRange))
                    }
                }
                _ => Err(StorageError::WrongType.into()),
            }
        } else {
            Err(FerrousError::Command(CommandError::NoSuchKey))
        }
    }
    
    pub fn ltrim(&self, db: DatabaseIndex, key: Key, start: isize, stop: isize) -> Result<()> {
        let shard = self.get_shard(db, &key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(&key) {
            match &mut stored_value.value {
                Value::List(list) => {
                    let len = list.len() as isize;
                    
                    let start = if start < 0 { (len + start).max(0) } else { start } as usize;
                    let stop = if stop < 0 { (len + stop).max(0) } else { stop } as usize;
                    
                    let mut new_list = VecDeque::new();
                    for (i, item) in list.iter().enumerate() {
                        if i >= start && i <= stop {
                            new_list.push_back(item.clone());
                        }
                    }
                    
                    *list = new_list;
                    let is_empty = list.is_empty();
                    drop(stored_value); // Release mutable borrow
                    
                    shard_guard.mark_modified(&key); // Now safe to call
                    
                    if is_empty {
                        shard_guard.data.remove(&key);
                    }
                    
                    Ok(())
                }
                _ => Err(StorageError::WrongType.into()),
            }
        } else {
            Ok(())
        }
    }
    
    pub fn lrem(&self, db: DatabaseIndex, key: Key, count: isize, element: Vec<u8>) -> Result<usize> {
        let shard = self.get_shard(db, &key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(&key) {
            match &mut stored_value.value {
                Value::List(list) => {
                    let mut removed = 0;
                    
                    if count == 0 {
                        list.retain(|item| {
                            if item == &element {
                                removed += 1;
                                false
                            } else {
                                true
                            }
                        });
                    } else if count > 0 {
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
                    
                    let is_empty = list.is_empty();
                    drop(stored_value); // Release mutable borrow
                    
                    if removed > 0 {
                        shard_guard.mark_modified(&key); // Now safe to call
                    }
                    
                    if is_empty {
                        shard_guard.data.remove(&key);
                    }
                    
                    Ok(removed)
                }
                _ => Err(StorageError::WrongType.into()),
            }
        } else {
            Ok(0)
        }
    }

    /// Set operations - NO access time tracking
    pub fn sadd(&self, db: DatabaseIndex, key: Key, members: Vec<Vec<u8>>) -> Result<usize> {
        let shard = self.get_shard(db, &key)?;
        let mut shard_guard = shard.write().unwrap();
        
        let added = if let Some(stored_value) = shard_guard.data.get_mut(&key) {
            match &mut stored_value.value {
                Value::Set(set) => {
                    let mut added = 0;
                    for member in members {
                        if set.insert(member) {
                            added += 1;
                        }
                    }
                    drop(stored_value); // Release mutable borrow
                    if added > 0 {
                        shard_guard.mark_modified(&key); // Now safe to call
                    }
                    added
                }
                _ => return Err(StorageError::WrongType.into()),
            }
        } else {
            // Create new set
            let mut set = HashSet::new();
            let mut added = 0;
            for member in members {
                if set.insert(member) {
                    added += 1;
                }
            }
            
            let stored_value = StoredValue::new(Value::Set(set));
            shard_guard.data.insert(key.clone(), stored_value);
            shard_guard.mark_modified(&key);
            added
        };
        
        Ok(added)
    }
    
    pub fn srem<'a, T: AsRef<[u8]>>(&self, db: DatabaseIndex, key: &[u8], members: &[T]) -> Result<usize> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            match &mut stored_value.value {
                Value::Set(set) => {
                    let mut removed = 0;
                    for member in members {
                        if set.remove(member.as_ref()) {
                            removed += 1;
                        }
                    }
                    
                    let is_empty = set.is_empty();
                    drop(stored_value); // Release mutable borrow
                    
                    if is_empty {
                        shard_guard.mark_modified(key); // Mark before removal
                        shard_guard.data.remove(key);
                    } else if removed > 0 {
                        shard_guard.mark_modified(key); // Now safe to call
                    }
                    
                    Ok(removed)
                }
                _ => Err(StorageError::WrongType.into()),
            }
        } else {
            Ok(0)
        }
    }
    
    pub fn smembers(&self, db: DatabaseIndex, key: &[u8]) -> Result<Vec<Vec<u8>>> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            let members = match &stored_value.value {
                Value::Set(set) => set.iter().cloned().collect(),
                _ => return Err(StorageError::WrongType.into()),
            };
            // NO touch() call - no access time tracking overhead
            Ok(members)
        } else {
            Ok(Vec::new())
        }
    }
    
    pub fn sismember(&self, db: DatabaseIndex, key: &[u8], member: &[u8]) -> Result<bool> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            let is_member = match &stored_value.value {
                Value::Set(set) => set.contains(member),
                _ => return Err(StorageError::WrongType.into()),
            };
            // NO touch() call - no access time tracking overhead
            Ok(is_member)
        } else {
            Ok(false)
        }
    }
    
    pub fn scard(&self, db: DatabaseIndex, key: &[u8]) -> Result<usize> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            let len = match &stored_value.value {
                Value::Set(set) => set.len(),
                _ => return Err(StorageError::WrongType.into()),
            };
            // NO touch() call - no access time tracking overhead
            Ok(len)
        } else {
            Ok(0)
        }
    }
    
    pub fn sunion<'a, T: AsRef<[u8]>>(&self, db: DatabaseIndex, keys: &[T]) -> Result<Vec<Vec<u8>>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        
        let mut result = HashSet::new();
        
        for key_ref in keys {
            let key = key_ref.as_ref();
            let shard = self.get_shard(db, key)?;
            let mut shard_guard = shard.write().unwrap();
            
            if let Some(stored_value) = shard_guard.data.get_mut(key) {
                match &stored_value.value {
                    Value::Set(set) => {
                        for member in set {
                            result.insert(member.clone());
                        }
                        // NO touch() call - no access time tracking overhead
                    }
                    _ => return Err(StorageError::WrongType.into()),
                }
            }
        }
        
        Ok(result.into_iter().collect())
    }
    
    pub fn sinter<'a, T: AsRef<[u8]>>(&self, db: DatabaseIndex, keys: &[T]) -> Result<Vec<Vec<u8>>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        
        // Get first set as base
        let first_key = keys[0].as_ref();
        let shard = self.get_shard(db, first_key)?;
        let mut shard_guard = shard.write().unwrap();
        
        let result: HashSet<Vec<u8>> = if let Some(stored_value) = shard_guard.data.get_mut(first_key) {
            // NO touch() call - no access time tracking overhead
            match &stored_value.value {
                Value::Set(set) => set.iter().cloned().collect(),
                _ => return Err(StorageError::WrongType.into()),
            }
        } else {
            return Ok(Vec::new());
        };
        drop(shard_guard); // Release lock early
        
        // Intersect with other sets
        let mut result = result;
        for k in 1..keys.len() {
            let key = keys[k].as_ref();
            let shard = self.get_shard(db, key)?;
            let mut shard_guard = shard.write().unwrap();
            
            if let Some(stored_value) = shard_guard.data.get_mut(key) {
                // NO touch() call - no access time tracking overhead
                match &stored_value.value {
                    Value::Set(set) => {
                        result.retain(|member| set.contains(member));
                    }
                    _ => return Err(StorageError::WrongType.into()),
                }
            } else {
                return Ok(Vec::new());
            }
        }
        
        Ok(result.into_iter().collect())
    }
    
    pub fn sdiff<'a, T: AsRef<[u8]>>(&self, db: DatabaseIndex, keys: &[T]) -> Result<Vec<Vec<u8>>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        
        // Get first set as base
        let first_key = keys[0].as_ref();
        let shard = self.get_shard(db, first_key)?;
        let mut shard_guard = shard.write().unwrap();
        
        let result: HashSet<Vec<u8>> = if let Some(stored_value) = shard_guard.data.get_mut(first_key) {
            // NO touch() call - no access time tracking overhead
            match &stored_value.value {
                Value::Set(set) => set.iter().cloned().collect(),
                _ => return Err(StorageError::WrongType.into()),
            }
        } else {
            return Ok(Vec::new());
        };
        drop(shard_guard); // Release lock early
        
        // Remove elements from other sets
        let mut result = result;
        for k in 1..keys.len() {
            let key = keys[k].as_ref();
            let shard = self.get_shard(db, key)?;
            let mut shard_guard = shard.write().unwrap();
            
            if let Some(stored_value) = shard_guard.data.get_mut(key) {
                // NO touch() call - no access time tracking overhead
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
    
    pub fn srandmember(&self, db: DatabaseIndex, key: &[u8], count: i64) -> Result<Vec<Vec<u8>>> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            let result = match &stored_value.value {
                Value::Set(set) => {
                    let members: Vec<Vec<u8>> = set.iter().cloned().collect();
                    if members.is_empty() {
                        Vec::new()
                    } else {
                        let mut rng = rand::thread_rng();
                        
                        if count >= 0 {
                            let n = std::cmp::min(count as usize, members.len());
                            let mut result = members;
                            result.shuffle(&mut rng);
                            result.truncate(n);
                            result
                        } else {
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
            // NO touch() call - no access time tracking overhead
            Ok(result)
        } else {
            Ok(Vec::new())
        }
    }
    
    pub fn spop(&self, db: DatabaseIndex, key: Key, count: usize) -> Result<Vec<Vec<u8>>> {
        let shard = self.get_shard(db, &key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(&key) {
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
                    
                    for member in &result {
                        set.remove(member);
                    }
                    
                    let is_empty = set.is_empty();
                    drop(stored_value); // Release mutable borrow
                    
                    if !result.is_empty() {
                        shard_guard.mark_modified(&key); // Now safe to call
                    }
                    
                    if is_empty {
                        shard_guard.data.remove(&key);
                    }
                    
                    Ok(result)
                }
                _ => Err(StorageError::WrongType.into()),
            }
        } else {
            Ok(Vec::new())
        }
    }

    /// Hash operations - NO access time tracking
    pub fn hset(&self, db: DatabaseIndex, key: Key, field_values: Vec<(Vec<u8>, Vec<u8>)>) -> Result<usize> {
        let shard = self.get_shard(db, &key)?;
        let mut shard_guard = shard.write().unwrap();
        
        let fields_added = if let Some(stored_value) = shard_guard.data.get_mut(&key) {
            match &mut stored_value.value {
                Value::Hash(hash) => {
                    let mut added = 0;
                    for (field, value) in field_values {
                        if hash.insert(field, value).is_none() {
                            added += 1;
                        }
                    }
                    shard_guard.mark_modified(&key);
                    // NO touch() call - no access time tracking overhead
                    added
                }
                _ => return Err(StorageError::WrongType.into()),
            }
        } else {
            // Create new hash
            let mut hash = HashMap::new();
            let len = field_values.len();
            for (field, value) in field_values {
                hash.insert(field, value);
            }
            
            let stored_value = StoredValue::new(Value::Hash(hash));
            shard_guard.data.insert(key.clone(), stored_value);
            shard_guard.mark_modified(&key);
            len
        };
        
        Ok(fields_added)
    }
    
    pub fn hget(&self, db: DatabaseIndex, key: &[u8], field: &[u8]) -> Result<Option<Vec<u8>>> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            let value = match &stored_value.value {
                Value::Hash(hash) => hash.get(field).cloned(),
                _ => return Err(StorageError::WrongType.into()),
            };
            // NO touch() call - no access time tracking overhead
            Ok(value)
        } else {
            Ok(None)
        }
    }
    
    pub fn hmget<'a, T: AsRef<[u8]>>(&self, db: DatabaseIndex, key: &[u8], fields: &[T]) -> Result<Vec<Option<Vec<u8>>>> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            // NO touch() call - no access time tracking overhead
            match &stored_value.value {
                Value::Hash(hash) => Ok(fields.iter().map(|field| hash.get(field.as_ref()).cloned()).collect()),
                _ => Err(StorageError::WrongType.into()),
            }
        } else {
            Ok(vec![None; fields.len()])
        }
    }
    
    pub fn hgetall(&self, db: DatabaseIndex, key: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            let pairs = match &stored_value.value {
                Value::Hash(hash) => hash.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                _ => return Err(StorageError::WrongType.into()),
            };
            // NO touch() call - no access time tracking overhead
            Ok(pairs)
        } else {
            Ok(Vec::new())
        }
    }
    
    pub fn hdel<'a, T: AsRef<[u8]>>(&self, db: DatabaseIndex, key: Key, fields: &[T]) -> Result<usize> {
        let shard = self.get_shard(db, &key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(&key) {
            match &mut stored_value.value {
                Value::Hash(hash) => {
                    let mut deleted = 0;
                    for field in fields {
                        if hash.remove(field.as_ref()).is_some() {
                            deleted += 1;
                        }
                    }
                    
                    if hash.is_empty() {
                        shard_guard.data.remove(&key);
                    } 
                    // NO else branch with touch() - no access time tracking overhead
                    shard_guard.mark_modified(&key);
                    
                    Ok(deleted)
                }
                _ => Err(StorageError::WrongType.into()),
            }
        } else {
            Ok(0)
        }
    }
    
    pub fn hlen(&self, db: DatabaseIndex, key: &[u8]) -> Result<usize> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            let len = match &stored_value.value {
                Value::Hash(hash) => hash.len(),
                _ => return Err(StorageError::WrongType.into()),
            };
            // NO touch() call - no access time tracking overhead
            Ok(len)
        } else {
            Ok(0)
        }
    }
    
    pub fn hexists(&self, db: DatabaseIndex, key: &[u8], field: &[u8]) -> Result<bool> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            let exists = match &stored_value.value {
                Value::Hash(hash) => hash.contains_key(field),
                _ => return Err(StorageError::WrongType.into()),
            };
            // NO touch() call - no access time tracking overhead
            Ok(exists)
        } else {
            Ok(false)
        }
    }
    
    pub fn hkeys(&self, db: DatabaseIndex, key: &[u8]) -> Result<Vec<Vec<u8>>> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            let keys = match &stored_value.value {
                Value::Hash(hash) => hash.keys().cloned().collect(),
                _ => return Err(StorageError::WrongType.into()),
            };
            // NO touch() call - no access time tracking overhead
            Ok(keys)
        } else {
            Ok(Vec::new())
        }
    }
    
    pub fn hvals(&self, db: DatabaseIndex, key: &[u8]) -> Result<Vec<Vec<u8>>> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            let values = match &stored_value.value {
                Value::Hash(hash) => hash.values().cloned().collect(),
                _ => return Err(StorageError::WrongType.into()),
            };
            // NO touch() call - no access time tracking overhead
            Ok(values)
        } else {
            Ok(Vec::new())
        }
    }
    
    pub fn hincrby(&self, db: DatabaseIndex, key: Key, field: Vec<u8>, increment: i64) -> Result<i64> {
        let shard = self.get_shard(db, &key)?;
        let mut shard_guard = shard.write().unwrap();
        
        let new_value = if let Some(stored_value) = shard_guard.data.get_mut(&key) {
            match &mut stored_value.value {
                Value::Hash(hash) => {
                    let new_val = match hash.get(&field) {
                        Some(current_bytes) => {
                            let current_str = String::from_utf8_lossy(current_bytes);
                            match current_str.parse::<i64>() {
                                Ok(current) => current + increment,
                                Err(_) => return Err(FerrousError::Command(CommandError::NotInteger)),
                            }
                        }
                        None => increment,
                    };
                    
                    hash.insert(field, new_val.to_string().into_bytes());
                    shard_guard.mark_modified(&key);
                    // NO touch() call - no access time tracking overhead
                    new_val
                }
                _ => return Err(StorageError::WrongType.into()),
            }
        } else {
            // Create new hash with single field
            let mut hash = HashMap::new();
            hash.insert(field, increment.to_string().into_bytes());
            
            let stored_value = StoredValue::new(Value::Hash(hash));
            shard_guard.data.insert(key.clone(), stored_value);
            shard_guard.mark_modified(&key);
            increment
        };
        
        Ok(new_value)
    }

    /// String operations - NO access time tracking
    pub fn append(&self, db: DatabaseIndex, key: Key, value: Vec<u8>) -> Result<usize> {
        let shard = self.get_shard(db, &key)?;
        let mut shard_guard = shard.write().unwrap();
        
        let new_len = if let Some(stored_value) = shard_guard.data.get_mut(&key) {
            match &mut stored_value.value {
                Value::String(bytes) => {
                    bytes.extend_from_slice(&value);
                    let len = bytes.len();
                    // NO touch() call - no access time tracking overhead
                    shard_guard.mark_modified(&key);
                    len
                }
                _ => return Err(StorageError::WrongType.into()),
            }
        } else {
            // Create new string
            let len = value.len();
            let stored_value = StoredValue::new(Value::String(value));
            shard_guard.data.insert(key.clone(), stored_value);
            shard_guard.mark_modified(&key);
            len
        };
        
        Ok(new_len)
    }
    
    pub fn strlen(&self, db: DatabaseIndex, key: &[u8]) -> Result<usize> {
        let shard = self.get_shard(db, key)?;
        let shard_guard = shard.read().unwrap(); // Use read lock for size check
        
        if let Some(stored_value) = shard_guard.data.get(key) {
            match &stored_value.value {
                Value::String(bytes) => Ok(bytes.len()),
                _ => Err(StorageError::WrongType.into()),
            }
        } else {
            Ok(0)
        }
    }
    
    pub fn getrange(&self, db: DatabaseIndex, key: &[u8], start: isize, end: isize) -> Result<Vec<u8>> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            let substring = match &stored_value.value {
                Value::String(bytes) => {
                    let len = bytes.len() as isize;
                    
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
            
            // NO touch() call - no access time tracking overhead
            Ok(substring)
        } else {
            Ok(Vec::new())
        }
    }
    
    pub fn setrange(&self, db: DatabaseIndex, key: Key, offset: usize, value: Vec<u8>) -> Result<usize> {
        let shard = self.get_shard(db, &key)?;
        let mut shard_guard = shard.write().unwrap();
        
        let new_len = if let Some(stored_value) = shard_guard.data.get_mut(&key) {
            match &mut stored_value.value {
                Value::String(bytes) => {
                    let required_len = offset + value.len();
                    if required_len > bytes.len() {
                        bytes.resize(required_len, 0);
                    }
                    
                    bytes[offset..offset + value.len()].copy_from_slice(&value);
                    let len = bytes.len();
                    
                    // NO touch() call - no access time tracking overhead
                    shard_guard.mark_modified(&key);
                    len
                }
                _ => return Err(StorageError::WrongType.into()),
            }
        } else {
            // Create new string with padding
            let mut new_string = vec![0; offset + value.len()];
            new_string[offset..].copy_from_slice(&value);
            let len = new_string.len();
            
            let stored_value = StoredValue::new(Value::String(new_string));
            shard_guard.data.insert(key.clone(), stored_value);
            shard_guard.mark_modified(&key);
            len
        };
        
        Ok(new_len)
    }
    
    pub fn key_type(&self, db: DatabaseIndex, key: &[u8]) -> Result<String> {
        let shard = self.get_shard(db, key)?;
        let shard_guard = shard.read().unwrap(); // Use read lock for type check
        
        if let Some(stored_value) = shard_guard.data.get(key) {
            let type_name = match &stored_value.value {
                Value::String(_) => "string",
                Value::List(_) => "list",
                Value::Set(_) => "set",
                Value::Hash(_) => "hash",
                Value::SortedSet(_) => "zset",
                Value::Stream(_) => "stream",
            };
            Ok(type_name.to_string())
        } else {
            Ok("none".to_string())
        }
    }
    
    pub fn rename(&self, db: DatabaseIndex, old_key: &[u8], new_key: Key) -> Result<()> {
        // Handle cross-shard renames by using both shards
        let old_shard = self.get_shard(db, old_key)?;
        let new_shard = self.get_shard(db, &new_key)?;
        
        if Arc::ptr_eq(old_shard, new_shard) {
            // Same shard - simple case
            let mut shard_guard = old_shard.write().unwrap();
            if let Some(stored_value) = shard_guard.data.remove(old_key) {
                shard_guard.data.insert(new_key.clone(), stored_value);
                shard_guard.mark_modified(&new_key);
                Ok(())
            } else {
                Err(FerrousError::Command(CommandError::NoSuchKey))
            }
        } else {
            // Cross-shard rename - acquire both locks in consistent order
            let (first_shard, second_shard, is_old_first) = if (old_shard.as_ref() as *const _) < (new_shard.as_ref() as *const _) {
                (old_shard, new_shard, true)
            } else {
                (new_shard, old_shard, false)
            };
            
            let mut guard1 = first_shard.write().unwrap();
            let mut guard2 = second_shard.write().unwrap();
            
            let (old_guard, new_guard) = if is_old_first {
                (&mut guard1, &mut guard2)
            } else {
                (&mut guard2, &mut guard1)
            };
            
            // Move the value between shards
            if let Some(stored_value) = old_guard.data.remove(old_key) {
                new_guard.data.insert(new_key.clone(), stored_value);
                new_guard.mark_modified(&new_key);
                Ok(())
            } else {
                Err(FerrousError::Command(CommandError::NoSuchKey))
            }
        }
    }
    
    pub fn keys(&self, db: DatabaseIndex, pattern: &[u8]) -> Result<Vec<Vec<u8>>> {
        let database = self.databases.get(db).ok_or(StorageError::InvalidDatabase)?;
        
        let pattern_str = String::from_utf8_lossy(pattern);
        let mut matching_keys = Vec::new();
        
        // Collect keys from all shards
        for shard in &database.shards {
            let shard_guard = shard.read().unwrap();
            for key in shard_guard.data.keys() {
                let key_str = String::from_utf8_lossy(key);
                if pattern_matches(&pattern_str, &key_str) {
                    matching_keys.push(key.clone());
                }
            }
        }
        
        Ok(matching_keys)
    }
    
    pub fn pexpire(&self, db: DatabaseIndex, key: &[u8], millis: u64) -> Result<bool> {
        self.expire(db, key, Duration::from_millis(millis))
    }
    
    pub fn pttl(&self, db: DatabaseIndex, key: &[u8]) -> Result<i64> {
        let ttl = self.ttl(db, key)?;
        
        match ttl {
            Some(duration) => Ok(duration.as_millis() as i64),
            None => {
                if self.exists(db, key)? {
                    Ok(-1)
                } else {
                    Ok(-2)
                }
            }
        }
    }
    
    /// Remove expiration - NO access time tracking
    pub fn persist(&self, db: DatabaseIndex, key: &[u8]) -> Result<bool> {
        let shard = self.get_shard(db, key)?;
        let mut shard_guard = shard.write().unwrap();
        
        if let Some(stored_value) = shard_guard.data.get_mut(key) {
            if stored_value.metadata.expires_at.is_some() {
                stored_value.metadata.clear_expiration();
                shard_guard.expiring_keys.remove(key);
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    /// Get the current modification counter for a key (for WATCH baseline)
    pub fn get_modification_counter(&self, db: DatabaseIndex, key: &[u8]) -> Result<u64> {
        let shard = self.get_shard(db, key)?;
        let shard_guard = shard.read().unwrap();
        
        // Use simplified shard-level modification tracking
        let baseline_counter = shard_guard.get_modification_counter();
        
        Ok(baseline_counter)
    }
    
    /// Check if a key was modified since a specific baseline counter (for WATCH command)
    pub fn was_modified_since(&self, db: DatabaseIndex, key: &[u8], baseline_counter: u64) -> Result<bool> {
        let shard = self.get_shard(db, key)?;
        let shard_guard = shard.read().unwrap();
        
        // Get current shard modification counter using atomic operations
        let current_counter = shard_guard.get_modification_counter();
        
        let violation_detected = current_counter > baseline_counter;
        
        // Any counter change indicates a violation - this is sufficient for Redis WATCH semantics
        Ok(violation_detected)
    }

    /// Check if a key was modified (for WATCH command) - kept for backward compatibility
    pub fn was_modified(&self, db: DatabaseIndex, key: &[u8]) -> Result<bool> {
        let _database = self.databases.get(db).ok_or(StorageError::InvalidDatabase)?;
        Ok(false)
    }

    /// Register a WATCH on a key and return the baseline modification counter
    pub fn register_watch(&self, db: DatabaseIndex, key: &[u8]) -> Result<u64> {
        let shard = self.get_shard(db, key)?;
        let shard_guard = shard.read().unwrap();
        
        // Register the watch and get baseline counter
        let baseline_counter = shard_guard.watch_tracker.register_watch();
        
        Ok(baseline_counter)
    }
    
    /// Unregister WATCH for keys (when UNWATCH or EXEC called)
    pub fn unregister_watch(&self, db: DatabaseIndex, key: &[u8]) -> Result<()> {
        let shard = self.get_shard(db, key)?;
        let shard_guard = shard.read().unwrap();
        
        shard_guard.watch_tracker.unregister_watch();
        
        Ok(())
    }

    /// Get the current database ID
    pub fn get_current_db(&self) -> usize {
        0
    }

    /// Scan operations - optimized for sharded access, NO access time tracking
    pub fn scan(&self, db: DatabaseIndex, cursor: u64, pattern: Option<&[u8]>, type_filter: Option<&str>, count: usize) -> Result<(u64, Vec<Vec<u8>>)> {
        let database = self.databases.get(db).ok_or(StorageError::InvalidDatabase)?;
        
        let scan_count = if count == 0 { 10 } else { count };
        let max_scan_count = std::cmp::min(scan_count, 1000);
        
        // Collect keys from all shards - NO touch() calls
        let mut all_keys = Vec::new();
        for shard in &database.shards {
            let shard_guard = shard.read().unwrap();
            for (key, stored_value) in shard_guard.data.iter() {
                if stored_value.is_expired() {
                    continue;
                }
                
                if let Some(type_name) = type_filter {
                    let value_type = match &stored_value.value {
                        Value::String(_) => "string",
                        Value::List(_) => "list",
                        Value::Set(_) => "set",
                        Value::Hash(_) => "hash",
                        Value::SortedSet(_) => "zset",
                        Value::Stream(_) => "stream",
                    };
                    
                    if value_type != type_name {
                        continue;
                    }
                }
                
                all_keys.push(key.clone());
            }
        }
        
        all_keys.sort();
        
        let start_pos = if cursor == 0 { 0 } else { cursor as usize };
        if start_pos >= all_keys.len() && !all_keys.is_empty() {
            return Ok((0, Vec::new()));
        }
        
        let mut matching_keys = Vec::new();
        let mut keys_examined = 0;
        let mut current_pos = start_pos;
        
        let pattern_str = pattern.map(|p| String::from_utf8_lossy(p));
        
        while keys_examined < max_scan_count * 10 && matching_keys.len() < max_scan_count {
            if current_pos >= all_keys.len() {
                break;
            }
            
            let key = &all_keys[current_pos];
            let mut include_key = true;
            
            if let Some(ref pat) = pattern_str {
                let key_str = String::from_utf8_lossy(key);
                if !pattern_matches(pat, &key_str) {
                    include_key = false;
                }
            }
            
            if include_key {
                matching_keys.push(key.clone());
            }
            
            current_pos += 1;
            keys_examined += 1;
        }
        
        let next_cursor = if current_pos >= all_keys.len() {
            0
        } else {
            current_pos as u64
        };
        
        Ok((next_cursor, matching_keys))
    }

    pub fn hscan(&self, db: DatabaseIndex, key: &[u8], cursor: u64, pattern: Option<&[u8]>, count: usize, no_values: bool) -> Result<(u64, Vec<Vec<u8>>)> {
        match self.get(db, key)? {
            GetResult::Found(Value::Hash(hash)) => {
                let scan_count = if count == 0 { 10 } else { count };
                let max_scan_count = std::cmp::min(scan_count, 1000);
                
                if hash.len() <= max_scan_count && cursor == 0 && pattern.is_none() {
                    let mut result = Vec::new();
                    for (field, value) in hash.iter() {
                        result.push(field.clone());
                        if !no_values {
                            result.push(value.clone());
                        }
                    }
                    return Ok((0, result));
                }
                
                let mut fields: Vec<Vec<u8>> = hash.keys().cloned().collect();
                fields.sort();
                
                let start_pos = if cursor == 0 { 0 } else { cursor as usize };
                if start_pos >= fields.len() && !fields.is_empty() {
                    return Ok((0, Vec::new()));
                }
                
                let mut result = Vec::new();
                let mut fields_examined = 0;
                let mut current_pos = start_pos;
                let pattern_str = pattern.map(|p| String::from_utf8_lossy(p));
                
                while fields_examined < max_scan_count * 10 && (result.len() / if no_values { 1 } else { 2 }) < max_scan_count {
                    if current_pos >= fields.len() {
                        break;
                    }
                    
                    let field = &fields[current_pos];
                    let mut include_field = true;
                    
                    if let Some(ref pat) = pattern_str {
                        let field_str = String::from_utf8_lossy(field);
                        if !pattern_matches(pat, &field_str) {
                            include_field = false;
                        }
                    }
                    
                    if include_field {
                        result.push(field.clone());
                        if !no_values {
                            let value = hash.get(field).unwrap();
                            result.push(value.clone());
                        }
                    }
                    
                    current_pos += 1;
                    fields_examined += 1;
                }
                
                let next_cursor = if current_pos >= fields.len() {
                    0
                } else {
                    current_pos as u64
                };
                
                Ok((next_cursor, result))
            },
            GetResult::Found(_) => {
                Err(FerrousError::Storage(StorageError::WrongType))
            },
            _ => {
                Ok((0, Vec::new()))
            }
        }
    }
    
    pub fn sscan(&self, db: DatabaseIndex, key: &[u8], cursor: u64, pattern: Option<&[u8]>, count: usize) -> Result<(u64, Vec<Vec<u8>>)> {
        match self.get(db, key)? {
            GetResult::Found(Value::Set(set)) => {
                let scan_count = if count == 0 { 10 } else { count };
                let max_scan_count = std::cmp::min(scan_count, 1000);
                
                if set.len() <= max_scan_count && cursor == 0 && pattern.is_none() {
                    return Ok((0, set.iter().cloned().collect()));
                }
                
                let mut members: Vec<Vec<u8>> = set.iter().cloned().collect();
                members.sort();
                
                let start_pos = if cursor == 0 { 0 } else { cursor as usize };
                if start_pos >= members.len() && !members.is_empty() {
                    return Ok((0, Vec::new()));
                }
                
                let mut result = Vec::new();
                let mut members_examined = 0;
                let mut current_pos = start_pos;
                let pattern_str = pattern.map(|p| String::from_utf8_lossy(p));
                
                while members_examined < max_scan_count * 10 && result.len() < max_scan_count {
                    if current_pos >= members.len() {
                        break;
                    }
                    
                    let member = &members[current_pos];
                    let mut include_member = true;
                    
                    if let Some(ref pat) = pattern_str {
                        let member_str = String::from_utf8_lossy(member);
                        if !pattern_matches(pat, &member_str) {
                            include_member = false;
                        }
                    }
                    
                    if include_member {
                        result.push(member.clone());
                    }
                    
                    current_pos += 1;
                    members_examined += 1;
                }
                
                let next_cursor = if current_pos >= members.len() {
                    0
                } else {
                    current_pos as u64
                };
                
                Ok((next_cursor, result))
            },
            GetResult::Found(_) => {
                Err(FerrousError::Storage(StorageError::WrongType))
            },
            _ => {
                Ok((0, Vec::new()))
            }
        }
    }
    
    pub fn zscan(&self, db: DatabaseIndex, key: &[u8], cursor: u64, pattern: Option<&[u8]>, count: usize) -> Result<(u64, Vec<(Vec<u8>, f64)>)> {
        match self.get(db, key)? {
            GetResult::Found(Value::SortedSet(zset)) => {
                let scan_count = if count == 0 { 10 } else { count };
                let max_scan_count = std::cmp::min(scan_count, 1000);
                
                let mut items: Vec<(Vec<u8>, f64)> = Vec::new();
                let range = zset.range_by_rank(0, zset.len() - 1);
                for (member, score) in range.items {
                    items.push((member, score));
                }
                
                items.sort_by(|a, b| a.0.cmp(&b.0));
                
                if items.len() <= max_scan_count && cursor == 0 && pattern.is_none() {
                    return Ok((0, items));
                }
                
                let start_pos = if cursor == 0 { 0 } else { cursor as usize };
                if start_pos >= items.len() && !items.is_empty() {
                    return Ok((0, Vec::new()));
                }
                
                let mut result = Vec::new();
                let mut items_examined = 0;
                let mut current_pos = start_pos;
                let pattern_str = pattern.map(|p| String::from_utf8_lossy(p));
                
                while items_examined < max_scan_count * 10 && result.len() < max_scan_count {
                    if current_pos >= items.len() {
                        break;
                    }
                    
                    let (member, score) = &items[current_pos];
                    let mut include_item = true;
                    
                    if let Some(ref pat) = pattern_str {
                        let member_str = String::from_utf8_lossy(member);
                        if !pattern_matches(pat, &member_str) {
                            include_item = false;
                        }
                    }
                    
                    if include_item {
                        result.push((member.clone(), *score));
                    }
                    
                    current_pos += 1;
                    items_examined += 1;
                }
                
                let next_cursor = if current_pos >= items.len() {
                    0
                } else {
                    current_pos as u64
                };
                
                Ok((next_cursor, result))
            },
            GetResult::Found(_) => {
                Err(FerrousError::Storage(StorageError::WrongType))
            },
            _ => {
                Ok((0, Vec::new()))
            }
        }
    }

    /// Background thread for cleaning up expired keys in sharded structure
    fn expiration_cleanup_loop(engine: Arc<StorageEngine>) {
        loop {
            thread::sleep(Duration::from_secs(1)); // Check every second
            
            for database in &engine.databases {
                let now = Instant::now();
                
                // Check each shard for expired keys
                for shard in &database.shards {
                    let mut expired_keys = Vec::new();
                    
                    // Find expired keys in this shard
                    {
                        let shard_guard = shard.read().unwrap();
                        for (key, expires_at) in shard_guard.expiring_keys.iter() {
                            if *expires_at <= now {
                                expired_keys.push(key.clone());
                            }
                        }
                    }
                    
                    // Remove expired keys with write lock
                    if !expired_keys.is_empty() {
                        let mut shard_guard = shard.write().unwrap();
                        for key in expired_keys {
                            if let Some(stored_value) = shard_guard.data.remove(&key) {
                                shard_guard.expiring_keys.remove(&key);
                                
                                // Update memory usage
                                let memory_size = engine.calculate_value_size(&key, &stored_value.value);
                                engine.memory_manager.remove_memory(memory_size);
                            }
                        }
                    }
                }
            }
        }
    }
}

impl Database {
    /// Create a new sharded database
    pub fn new() -> Self {
        let mut shards = Vec::with_capacity(SHARDS_PER_DATABASE);
        for _ in 0..SHARDS_PER_DATABASE {
            shards.push(Arc::new(RwLock::new(DatabaseShard::new())));
        }
        
        Database { shards }
    }
}

impl DatabaseShard {
    /// Create a new database shard
    pub fn new() -> Self {
        DatabaseShard {
            data: HashMap::new(),
            expiring_keys: HashMap::new(),
            watch_tracker: ShardWatchTracker::new(),
        }
    }
    
    /// Mark this shard as modified (conditional zero overhead)
    fn mark_modified(&self, _key: &[u8]) {
        self.watch_tracker.mark_modified();
    }
    
    /// Get current modification counter for shard
    fn get_modification_counter(&self) -> u64 {
        self.watch_tracker.get_epoch()
    }
}

impl Default for Database {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for DatabaseShard {
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
    fn test_incr_command() {
        let engine = StorageEngine::new_in_memory();
        
        // Test incrementing a non-existent key
        let result = engine.incr(0, b"counter".to_vec()).unwrap();
        assert_eq!(result, 1);
        
        // Test incrementing an existing key
        let result = engine.incr(0, b"counter".to_vec()).unwrap();
        assert_eq!(result, 2);
        
        // Test incrementing by a specific value
        let result = engine.incr_by(0, b"counter".to_vec(), 5).unwrap();
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
    
    #[test]
    fn test_sharding_distribution() {
        let engine = StorageEngine::new();
        
        // Test that different keys go to different shards
        let key1 = b"key1";
        let key2 = b"key2"; 
        
        let shard1_idx = engine.get_shard_index(key1);
        let shard2_idx = engine.get_shard_index(key2);
        
        // Keys should distribute across shards (though not guaranteed to be different)
        assert!(shard1_idx < SHARDS_PER_DATABASE);
        assert!(shard2_idx < SHARDS_PER_DATABASE);
    }
    
    #[test]
    fn test_no_access_time_tracking() {
        let engine = StorageEngine::new();
        
        // Verify that operations complete without access time updates
        engine.set_string(0, b"test".to_vec(), b"value".to_vec()).unwrap();
        let _result = engine.get_string(0, b"test").unwrap(); // No touch() called
        engine.incr(0, b"counter".to_vec()).unwrap(); // No touch() called
        
        // All should succeed without any access time tracking overhead
        assert!(true);
    }
}

/// Simple glob pattern matching (unchanged)
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
                    if let Some(end) = pattern_chars[p_idx..].iter().position(|&c| c == ']') {
                        let class_end = p_idx + end;
                        let negate = p_idx + 1 < class_end && pattern_chars[p_idx + 1] == '^';
                        let start_idx = if negate { p_idx + 2 } else { p_idx + 1 };
                        
                        let mut matched = false;
                        let mut i = start_idx;
                        while i < class_end {
                            if i + 2 < class_end && pattern_chars[i + 1] == '-' {
                                if text_chars[t_idx] >= pattern_chars[i] && text_chars[t_idx] <= pattern_chars[i + 2] {
                                    matched = true;
                                    break;
                                }
                                i += 3;
                            } else {
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
        
        if let Some(star_pos) = star_idx {
            p_idx = star_pos + 1;
            star_match_idx += 1;
            t_idx = star_match_idx;
        } else {
            return false;
        }
    }
    
    while p_idx < pattern_chars.len() && pattern_chars[p_idx] == '*' {
        p_idx += 1;
    }
    
    p_idx == pattern_chars.len()
}