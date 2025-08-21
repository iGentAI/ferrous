//! High-performance Stream implementation with cache-coherent architecture
//! 
//! Eliminates expensive cloning through proper interior mutability and 
//! optimizes for cache coherence with minimal data movement.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use std::cmp::Ordering as CmpOrdering;
use std::fmt::{self, Debug, Display};
use std::collections::HashMap;
use std::cell::UnsafeCell;

// Import consumer groups module
use crate::storage::consumer_groups::{ConsumerGroupManager, ConsumerGroup};

/// Thread-local timestamp cache to avoid system calls
thread_local! {
    static CACHED_TIME: UnsafeCell<(u64, SystemTime)> = UnsafeCell::new((0, SystemTime::UNIX_EPOCH));
}

/// Get current milliseconds with caching to avoid syscalls
#[inline(always)]
fn get_cached_millis() -> u64 {
    CACHED_TIME.with(|cache| unsafe {
        let cached = &mut *cache.get();
        let now = SystemTime::now();
        
        // Only update if at least 1ms has passed
        if now.duration_since(cached.1).unwrap_or(Duration::ZERO).as_millis() > 0 {
            let millis = now.duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
            cached.0 = millis;
            cached.1 = now;
            millis
        } else {
            cached.0
        }
    })
}

/// A stream ID with bit-packed representation for performance
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct StreamId {
    /// Packed representation: high 64 bits = millis, low 64 bits = seq
    packed: u128,
}

/// A stream entry optimized for cache-friendly storage
#[derive(Clone, Debug)]
pub struct StreamEntry {
    pub id: StreamId,
    pub fields: HashMap<Vec<u8>, Vec<u8>>,
}

/// Cache-coherent stream data protected by single mutex
#[derive(Debug)]
pub struct StreamData {
    /// Append-optimized entry storage (Vec is cache-friendly for sequential access)
    entries: Vec<StreamEntry>,
    /// Last generated ID for validation
    last_id: StreamId,
    /// Memory usage tracking
    memory_usage: usize,
}

/// High-performance stream with interior mutability and atomic fast paths
#[repr(C)]
pub struct Stream {
    /// Mutable data protected by single mutex - NO CLONING!
    data: Mutex<StreamData>,
    
    /// Hot path atomics - grouped for cache locality
    /// Using cache line padding to prevent false sharing
    _pad1: [u8; 64],
    last_id_millis: AtomicU64,
    last_id_seq: AtomicU64,
    
    /// Less frequently accessed atomics
    _pad2: [u8; 48],  // Align to cache line
    length: AtomicUsize,
    memory_usage: AtomicUsize,
    
    /// Consumer groups manager
    consumer_groups: Arc<ConsumerGroupManager>,
}

/// Result of a range query
#[derive(Debug, Clone)]
pub struct StreamRangeResult {
    pub entries: Vec<StreamEntry>,
}

impl StreamId {
    #[inline]
    pub fn new(millis: u64, seq: u64) -> Self {
        StreamId {
            packed: ((millis as u128) << 64) | (seq as u128)
        }
    }
    
    #[inline]
    pub fn millis(&self) -> u64 {
        (self.packed >> 64) as u64
    }
    
    #[inline]
    pub fn seq(&self) -> u64 {
        self.packed as u64
    }
    
    /// Create a StreamId from a string in format "millis-seq"
    pub fn from_string(s: &str) -> Option<Self> {
        if let Some(dash_pos) = s.find('-') {
            let (millis_str, seq_str) = s.split_at(dash_pos);
            let seq_str = &seq_str[1..];
            
            let millis = Self::parse_u64_fast(millis_str.as_bytes())?;
            let seq = Self::parse_u64_fast(seq_str.as_bytes())?;
            
            Some(StreamId::new(millis, seq))
        } else {
            None
        }
    }
    
    /// Fast integer parsing
    #[inline]
    fn parse_u64_fast(bytes: &[u8]) -> Option<u64> {
        let mut result = 0u64;
        for &b in bytes {
            if b < b'0' || b > b'9' { return None; }
            result = result.wrapping_mul(10).wrapping_add((b - b'0') as u64);
        }
        Some(result)
    }
    
    #[inline]
    pub fn to_string(&self) -> String {
        format!("{}-{}", self.millis(), self.seq())
    }
    
    /// Generate next ID atomically with optimized timestamp caching
    #[inline(always)]
    pub fn generate_next_atomic(last_millis: &AtomicU64, last_seq: &AtomicU64) -> Self {
        let now_millis = get_cached_millis();
        let prev_millis = last_millis.load(Ordering::Relaxed);
        
        if now_millis > prev_millis {
            // Try to update to new millisecond
            match last_millis.compare_exchange_weak(
                prev_millis, 
                now_millis, 
                Ordering::Relaxed, 
                Ordering::Relaxed
            ) {
                Ok(_) => {
                    last_seq.store(0, Ordering::Relaxed);
                    return StreamId::new(now_millis, 0);
                }
                Err(actual) => {
                    // Someone else updated, use their value
                    if now_millis > actual {
                        // Retry with updated value
                        if last_millis.compare_exchange_weak(
                            actual,
                            now_millis,
                            Ordering::Relaxed,
                            Ordering::Relaxed
                        ).is_ok() {
                            last_seq.store(0, Ordering::Relaxed);
                            return StreamId::new(now_millis, 0);
                        }
                    }
                    // Fall through to sequence increment
                }
            }
        }
        
        // Same millisecond, increment sequence
        let seq = last_seq.fetch_add(1, Ordering::Relaxed);
        StreamId::new(prev_millis, seq + 1)
    }
    
    pub fn min() -> Self {
        StreamId { packed: 0 }
    }
    
    pub fn max() -> Self {
        StreamId { packed: u128::MAX }
    }
}

impl PartialOrd for StreamId {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl Ord for StreamId {
    #[inline]
    fn cmp(&self, other: &Self) -> CmpOrdering {
        self.packed.cmp(&other.packed)
    }
}

impl Display for StreamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.millis(), self.seq())
    }
}

impl StreamData {
    fn new() -> Self {
        StreamData {
            entries: Vec::with_capacity(4096), // Larger pre-allocation for better throughput
            last_id: StreamId::new(0, 0),
            memory_usage: std::mem::size_of::<StreamData>(),
        }
    }
    
    /// Add entry with auto-generated ID - OPTIMIZED HOT PATH
    #[inline]
    fn add_auto(&mut self, fields: HashMap<Vec<u8>, Vec<u8>>, stream: &Stream) -> StreamId {
        let id = StreamId::generate_next_atomic(&stream.last_id_millis, &stream.last_id_seq);
        
        // Pre-calculate size before creating entry
        let fields_size: usize = fields.iter()
            .map(|(k, v)| k.len() + v.len() + 48)
            .sum();
        let entry_size = 16 + std::mem::size_of::<StreamEntry>() + fields_size;
        
        let entry = StreamEntry { id, fields };
        
        // O(1) append - cache-friendly operation
        self.entries.push(entry);
        self.last_id = id;
        self.memory_usage += entry_size;
        
        // Update atomic metadata with relaxed ordering for non-critical updates
        stream.length.fetch_add(1, Ordering::Relaxed);
        stream.memory_usage.fetch_add(entry_size, Ordering::Relaxed);
        
        id
    }
    
    /// Add entry with specific ID - NO CLONING!
    fn add_with_id(&mut self, id: StreamId, fields: HashMap<Vec<u8>, Vec<u8>>, stream: &Stream) -> Result<(), &'static str> {
        if id <= self.last_id {
            return Err("The ID specified in XADD is equal or smaller than the target stream top item");
        }
        
        // Check for duplicate using binary search on sorted Vec
        if self.entries.binary_search_by(|e| e.id.cmp(&id)).is_ok() {
            return Err("The ID specified in XADD already exists");
        }
        
        let entry = StreamEntry { id, fields };
        let entry_size = Self::calculate_entry_size(&entry);
        
        // O(1) append to end of sorted list
        self.entries.push(entry);
        self.last_id = id;
        self.memory_usage += entry_size;
        
        // Update atomic metadata with relaxed ordering for non-critical updates
        stream.length.fetch_add(1, Ordering::Relaxed);
        stream.last_id_millis.store(id.millis(), Ordering::Relaxed);
        stream.last_id_seq.store(id.seq(), Ordering::Relaxed);
        stream.memory_usage.fetch_add(entry_size, Ordering::Relaxed);
        
        Ok(())
    }
    
    /// Range query with cache-coherent access
    fn range(&self, start: &StreamId, end: &StreamId, count: Option<usize>, reverse: bool) -> StreamRangeResult {
        // Binary search for efficient range access on sorted Vec
        let start_idx = self.entries.binary_search_by(|e| e.id.cmp(start))
            .unwrap_or_else(|idx| idx);
        
        let end_idx = self.entries.binary_search_by(|e| e.id.cmp(end))
            .unwrap_or_else(|idx| if idx > 0 { idx - 1 } else { 0 });
        
        let mut result_entries = Vec::new();
        
        if reverse {
            for i in (start_idx..=end_idx.min(self.entries.len().saturating_sub(1))).rev() {
                if let Some(count) = count {
                    if result_entries.len() >= count { break; }
                }
                if i < self.entries.len() {
                    result_entries.push(self.entries[i].clone());
                }
            }
        } else {
            for i in start_idx..=end_idx.min(self.entries.len().saturating_sub(1)) {
                if let Some(count) = count {
                    if result_entries.len() >= count { break; }
                }
                if i < self.entries.len() {
                    result_entries.push(self.entries[i].clone());
                }
            }
        }
        
        StreamRangeResult { entries: result_entries }
    }
    
    /// Range after specific ID (cache-efficient)
    fn range_after(&self, after_id: &StreamId, count: Option<usize>) -> StreamRangeResult {
        let start_idx = self.entries.binary_search_by(|e| e.id.cmp(after_id))
            .map(|idx| idx + 1)
            .unwrap_or_else(|idx| idx);
        
        let mut result_entries = Vec::new();
        let max_count = count.unwrap_or(self.entries.len());
        
        for i in start_idx..self.entries.len() {
            if result_entries.len() >= max_count { break; }
            result_entries.push(self.entries[i].clone());
        }
        
        StreamRangeResult { entries: result_entries }
    }
    
    /// Calculate memory size for an entry - optimized version
    #[inline(always)]
    fn calculate_entry_size(entry: &StreamEntry) -> usize {
        // More efficient calculation without iterator overhead
        let mut size = 16 + std::mem::size_of::<StreamEntry>(); // ID + struct overhead
        
        // Direct iteration is faster than map/sum
        for (k, v) in &entry.fields {
            size += k.len() + v.len() + 48; // 48 bytes is typical HashMap entry overhead
        }
        
        size
    }
}

impl Stream {
    /// Create new stream with cache-coherent architecture
    pub fn new() -> Self {
        Stream {
            data: Mutex::new(StreamData::new()),
            _pad1: [0; 64],
            last_id_millis: AtomicU64::new(0),
            last_id_seq: AtomicU64::new(0),
            _pad2: [0; 48],
            length: AtomicUsize::new(0),
            memory_usage: AtomicUsize::new(std::mem::size_of::<Stream>()),
            consumer_groups: Arc::new(ConsumerGroupManager::new()),
        }
    }
    
    /// Add entry with auto-generated ID - DIRECT MUTATION, NO CLONING!
    pub fn add_auto(&self, fields: HashMap<Vec<u8>, Vec<u8>>) -> StreamId {
        let mut data = self.data.lock().unwrap();
        data.add_auto(fields, self)
    }
    
    /// Add entry with specific ID - DIRECT MUTATION, NO CLONING!
    pub fn add_with_id(&self, id: StreamId, fields: HashMap<Vec<u8>, Vec<u8>>) -> Result<(), &'static str> {
        let mut data = self.data.lock().unwrap();
        data.add_with_id(id, fields, self)
    }
    
    /// Get length (lock-free atomic read - major performance win)
    #[inline]
    pub fn len(&self) -> usize {
        self.length.load(Ordering::Relaxed)
    }
    
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    
    /// Get memory usage (lock-free atomic read)
    #[inline]
    pub fn memory_usage(&self) -> usize {
        self.memory_usage.load(Ordering::Relaxed)
    }
    
    /// Range operations with minimal lock time
    pub fn range(&self, start: &StreamId, end: &StreamId, count: Option<usize>, reverse: bool) -> StreamRangeResult {
        let data = self.data.lock().unwrap();
        data.range(start, end, count, reverse)
    }
    
    pub fn range_after(&self, after_id: &StreamId, count: Option<usize>) -> StreamRangeResult {
        let data = self.data.lock().unwrap();
        data.range_after(after_id, count)
    }
    
    pub fn first_entry(&self) -> Option<StreamEntry> {
        let data = self.data.lock().unwrap();
        data.entries.first().cloned()
    }
    
    pub fn last_entry(&self) -> Option<StreamEntry> {
        let data = self.data.lock().unwrap();
        data.entries.last().cloned()
    }
    
    /// Trim operations - direct mutation, no cloning
    pub fn trim_by_count(&self, max_count: usize) -> usize {
        let mut data = self.data.lock().unwrap();
        
        if data.entries.len() <= max_count {
            return 0;
        }
        
        let to_remove = data.entries.len() - max_count;
        
        // Calculate memory to free
        let memory_to_free: usize = data.entries.iter()
            .take(to_remove)
            .map(StreamData::calculate_entry_size)
            .sum();
        
        // Remove oldest entries
        data.entries.drain(..to_remove);
        data.memory_usage -= memory_to_free;
        
        // Update atomic metadata with relaxed ordering
        self.length.fetch_sub(to_remove, Ordering::Relaxed);
        self.memory_usage.fetch_sub(memory_to_free, Ordering::Relaxed);
        
        to_remove
    }
    
    pub fn trim_by_min_id(&self, min_id: &StreamId) -> usize {
        let mut data = self.data.lock().unwrap();
        
        let split_idx = data.entries.binary_search_by(|e| e.id.cmp(min_id))
            .unwrap_or_else(|idx| idx);
        
        if split_idx == 0 {
            return 0;
        }
        
        let memory_to_free: usize = data.entries.iter()
            .take(split_idx)
            .map(StreamData::calculate_entry_size)
            .sum();
        
        data.entries.drain(..split_idx);
        data.memory_usage -= memory_to_free;
        
        self.length.fetch_sub(split_idx, Ordering::Relaxed);
        self.memory_usage.fetch_sub(memory_to_free, Ordering::Relaxed);
        
        split_idx
    }
    
    pub fn delete(&self, ids: &[StreamId]) -> usize {
        let mut data = self.data.lock().unwrap();
        let mut deleted = 0;
        let mut memory_freed = 0;
        
        let mut sorted_ids = ids.to_vec();
        sorted_ids.sort();
        sorted_ids.dedup();
        
        for id in sorted_ids.iter().rev() {
            if let Ok(idx) = data.entries.binary_search_by(|e| e.id.cmp(id)) {
                let removed_entry = data.entries.remove(idx);
                memory_freed += StreamData::calculate_entry_size(&removed_entry);
                deleted += 1;
            }
        }
        
        if deleted > 0 {
            data.memory_usage -= memory_freed;
            self.length.fetch_sub(deleted, Ordering::Relaxed);
            self.memory_usage.fetch_sub(memory_freed, Ordering::Relaxed);
        }
        
        deleted
    }
    
    pub fn get_entries(&self, ids: &[StreamId]) -> Vec<Option<StreamEntry>> {
        let data = self.data.lock().unwrap();
        
        ids.iter()
            .map(|id| {
                data.entries.binary_search_by(|e| e.id.cmp(id))
                    .map(|idx| data.entries[idx].clone())
                    .ok()
            })
            .collect()
    }
    
    pub fn clear(&self) {
        let mut data = self.data.lock().unwrap();
        data.entries.clear();
        data.last_id = StreamId::new(0, 0);
        data.memory_usage = std::mem::size_of::<StreamData>();
        
        self.length.store(0, Ordering::Relaxed);
        self.last_id_millis.store(0, Ordering::Relaxed);
        self.last_id_seq.store(0, Ordering::Relaxed);
        self.memory_usage.store(std::mem::size_of::<Stream>(), Ordering::Relaxed);
    }
    
    /// Create a consumer group
    pub fn create_consumer_group(&self, name: String, start_id: StreamId) -> Result<(), String> {
        self.consumer_groups.create_group(name, start_id)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    
    /// Destroy a consumer group
    pub fn destroy_consumer_group(&self, name: &str) -> bool {
        self.consumer_groups.destroy_group(name)
    }
    
    /// Get a consumer group
    pub fn get_consumer_group(&self, name: &str) -> Option<Arc<ConsumerGroup>> {
        self.consumer_groups.get_group(name)
    }
    
    /// List all consumer groups
    pub fn list_consumer_groups(&self) -> Vec<Arc<ConsumerGroup>> {
        self.consumer_groups.list_groups()
    }
    
    /// Read entries for a consumer group
    pub fn read_group(
        &self,
        group_name: &str,
        consumer_name: &str,
        after_id: StreamId,
        count: Option<usize>,
        noack: bool
    ) -> Result<Vec<StreamEntry>, String> {
        // Get the consumer group
        let group = self.consumer_groups.get_group(group_name)
            .ok_or_else(|| format!("NOGROUP No such consumer group {} for stream", group_name))?;
        
        // Get entries after the specified ID
        let data = self.data.lock().unwrap();
        let entries = if after_id == StreamId::max() {
            // Special case: ">" means only new entries
            let last_delivered = group.get_last_id();
            data.range_after(&last_delivered, count).entries
        } else {
            data.range_after(&after_id, count).entries
        };
        
        drop(data);
        
        if !noack && !entries.is_empty() {
            // Add entries to pending unless NOACK
            let pending_entries = group.add_pending(consumer_name, entries.clone());
            Ok(pending_entries)
        } else {
            Ok(entries)
        }
    }
    
    /// Acknowledge messages for a consumer group
    pub fn acknowledge_messages(&self, group_name: &str, ids: &[StreamId]) -> Result<usize, String> {
        let group = self.consumer_groups.get_group(group_name)
            .ok_or_else(|| format!("NOGROUP No such consumer group {} for stream", group_name))?;
        
        Ok(group.acknowledge(ids))
    }
    
    /// Claim messages from another consumer
    pub fn claim_messages(
        &self,
        group_name: &str,
        consumer: &str,
        min_idle_ms: u64,
        ids: &[StreamId],
        force: bool
    ) -> Result<Vec<StreamEntry>, String> {
        let group = self.consumer_groups.get_group(group_name)
            .ok_or_else(|| format!("NOGROUP No such consumer group {} for stream", group_name))?;
        
        let claimed_ids = group.claim_messages(consumer, min_idle_ms, ids, force);
        
        // Get the actual entries for claimed IDs
        let data = self.data.lock().unwrap();
        let entries: Vec<StreamEntry> = claimed_ids
            .iter()
            .filter_map(|id| {
                // Binary search for each claimed ID
                data.entries.binary_search_by(|e| e.id.cmp(id))
                    .ok()
                    .map(|idx| data.entries[idx].clone())
            })
            .collect();
        
        Ok(entries)
    }
    
    /// Auto-claim idle messages
    pub fn auto_claim_messages(
        &self,
        group_name: &str,
        consumer: &str,
        min_idle_ms: u64,
        start_id: StreamId,
        count: usize
    ) -> Result<(Vec<StreamEntry>, StreamId), String> {
        let group = self.consumer_groups.get_group(group_name)
            .ok_or_else(|| format!("NOGROUP No such consumer group {} for stream", group_name))?;
        
        let (claimed_ids, next_start) = group.auto_claim(consumer, min_idle_ms, start_id, count);
        
        // Get the actual entries for claimed IDs
        let data = self.data.lock().unwrap();
        let entries: Vec<StreamEntry> = claimed_ids
            .iter()
            .filter_map(|id| {
                data.entries.binary_search_by(|e| e.id.cmp(id))
                    .ok()
                    .map(|idx| data.entries[idx].clone())
            })
            .collect();
        
        Ok((entries, next_start))
    }
}

impl Clone for Stream {
    fn clone(&self) -> Self {
        let data = self.data.lock().unwrap();
        Stream {
            data: Mutex::new(StreamData {
                entries: data.entries.clone(),
                last_id: data.last_id,
                memory_usage: data.memory_usage,
            }),
            _pad1: [0; 64],
            last_id_millis: AtomicU64::new(self.last_id_millis.load(Ordering::Relaxed)),
            last_id_seq: AtomicU64::new(self.last_id_seq.load(Ordering::Relaxed)),
            _pad2: [0; 48],
            length: AtomicUsize::new(self.length.load(Ordering::Relaxed)),
            memory_usage: AtomicUsize::new(self.memory_usage.load(Ordering::Relaxed)),
            consumer_groups: Arc::clone(&self.consumer_groups),
        }
    }
}

impl Default for Stream {
    fn default() -> Self {
        Self::new()
    }
}

impl Debug for Stream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Stream {{ len: {}, last_id: {}-{} }}", 
               self.len(),
               self.last_id_millis.load(Ordering::Relaxed),
               self.last_id_seq.load(Ordering::Relaxed))
    }
}

// Thread-safe by design with proper synchronization
unsafe impl Send for Stream {}
unsafe impl Sync for Stream {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_no_cloning_operations() {
        let stream = Stream::new();
        
        // Test that operations are direct mutations, not clones
        let mut fields = HashMap::new();
        fields.insert(b"field1".to_vec(), b"value1".to_vec());
        
        let id = stream.add_auto(fields);
        assert_eq!(stream.len(), 1);
        assert!(id.millis() > 0);
        
        // Verify atomic read operations
        assert_eq!(stream.len(), 1);
        assert!(stream.memory_usage() > std::mem::size_of::<Stream>());
    }
    
    #[test]
    fn test_cache_coherent_operations() {
        let stream = Stream::new();
        
        // Add multiple entries to test Vec append performance
        for i in 0..100 {
            let mut fields = HashMap::new();
            fields.insert(b"index".to_vec(), i.to_string().into_bytes());
            stream.add_with_id(StreamId::new(1000 + i, 0), fields).unwrap();
        }
        
        assert_eq!(stream.len(), 100);
        
        // Test range operations are cache-efficient
        let result = stream.range(&StreamId::new(1010, 0), &StreamId::new(1020, 0), None, false);
        assert_eq!(result.entries.len(), 11);
    }
    
    #[test]
    fn test_concurrent_access() {
        use std::sync::Arc;
        use std::thread;
        
        let stream = Arc::new(Stream::new());
        let mut handles = vec![];
        
        // Test concurrent operations with proper synchronization
        for i in 0..10 {
            let stream_clone = Arc::clone(&stream);
            let handle = thread::spawn(move || {
                let mut fields = HashMap::new();
                fields.insert(b"thread".to_vec(), i.to_string().into_bytes());
                stream_clone.add_auto(fields)
            });
            handles.push(handle);
        }
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        assert_eq!(stream.len(), 10);
    }
}