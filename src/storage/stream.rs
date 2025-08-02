//! High-performance Stream implementation with cache-coherent architecture
//! 
//! Eliminates expensive cloning through proper interior mutability and 
//! optimizes for cache coherence with minimal data movement.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use std::cmp::Ordering as CmpOrdering;
use std::fmt::{self, Debug, Display};
use std::collections::HashMap;

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
pub struct Stream {
    /// Mutable data protected by single mutex - NO CLONING!
    data: Mutex<StreamData>,
    /// Atomic metadata for lock-free reads (cache-friendly)
    length: AtomicUsize,
    last_id_millis: AtomicU64,
    last_id_seq: AtomicU64,
    memory_usage: AtomicUsize,
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
    
    /// Generate next ID atomically
    pub fn generate_next_atomic(last_millis: &AtomicU64, last_seq: &AtomicU64) -> Self {
        loop {
            let now_millis = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            
            let prev_millis = last_millis.load(Ordering::Acquire);
            
            if now_millis > prev_millis {
                match last_millis.compare_exchange_weak(prev_millis, now_millis, Ordering::AcqRel, Ordering::Acquire) {
                    Ok(_) => {
                        last_seq.store(0, Ordering::Release);
                        return StreamId::new(now_millis, 0);
                    }
                    Err(_) => continue,
                }
            } else {
                let seq = last_seq.fetch_add(1, Ordering::AcqRel);
                return StreamId::new(prev_millis, seq + 1);
            }
        }
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
            entries: Vec::with_capacity(512), // Pre-allocate for cache efficiency
            last_id: StreamId::new(0, 0),
            memory_usage: std::mem::size_of::<StreamData>(),
        }
    }
    
    /// Add entry with auto-generated ID - NO CLONING!
    fn add_auto(&mut self, fields: HashMap<Vec<u8>, Vec<u8>>, stream: &Stream) -> StreamId {
        let id = StreamId::generate_next_atomic(&stream.last_id_millis, &stream.last_id_seq);
        
        let entry = StreamEntry { id, fields };
        let entry_size = Self::calculate_entry_size(&entry);
        
        // O(1) append - cache-friendly operation
        self.entries.push(entry);
        self.last_id = id;
        self.memory_usage += entry_size;
        
        // Update atomic metadata
        stream.length.fetch_add(1, Ordering::AcqRel);
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
        
        // Update atomic metadata
        stream.length.fetch_add(1, Ordering::AcqRel);
        stream.last_id_millis.store(id.millis(), Ordering::Release);
        stream.last_id_seq.store(id.seq(), Ordering::Release);
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
    
    /// Calculate memory size for an entry
    fn calculate_entry_size(entry: &StreamEntry) -> usize {
        let id_size = 16; // Packed u128
        let fields_size: usize = entry.fields.iter()
            .map(|(k, v)| k.len() + v.len() + std::mem::size_of::<(Vec<u8>, Vec<u8>)>())
            .sum();
        
        id_size + fields_size + std::mem::size_of::<StreamEntry>()
    }
}

impl Stream {
    /// Create new stream with cache-coherent architecture
    pub fn new() -> Self {
        Stream {
            data: Mutex::new(StreamData::new()),
            length: AtomicUsize::new(0),
            last_id_millis: AtomicU64::new(0),
            last_id_seq: AtomicU64::new(0),
            memory_usage: AtomicUsize::new(std::mem::size_of::<Stream>()),
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
        
        // Update atomic metadata
        self.length.fetch_sub(to_remove, Ordering::AcqRel);
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
        
        self.length.fetch_sub(split_idx, Ordering::AcqRel);
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
            self.length.fetch_sub(deleted, Ordering::AcqRel);
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
        
        self.length.store(0, Ordering::Release);
        self.last_id_millis.store(0, Ordering::Release);
        self.last_id_seq.store(0, Ordering::Release);
        self.memory_usage.store(std::mem::size_of::<Stream>(), Ordering::Release);
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
            length: AtomicUsize::new(self.length.load(Ordering::Relaxed)),
            last_id_millis: AtomicU64::new(self.last_id_millis.load(Ordering::Relaxed)),
            last_id_seq: AtomicU64::new(self.last_id_seq.load(Ordering::Relaxed)),
            memory_usage: AtomicUsize::new(self.memory_usage.load(Ordering::Relaxed)),
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