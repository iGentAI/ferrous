//! Stream implementation for time-series data
//! 
//! Provides a Redis-compatible stream data structure optimized for time-ordered entries.
//! Uses BTreeMap for efficient range queries and ID-based ordering.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use std::cmp::Ordering;
use std::fmt::{self, Debug, Display};

/// A stream ID consisting of milliseconds timestamp and sequence number
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub struct StreamId {
    /// Milliseconds since Unix epoch
    pub millis: u64,
    /// Sequence number for entries at the same millisecond
    pub seq: u64,
}

/// A stream entry containing field-value pairs
#[derive(Clone, Debug)]
pub struct StreamEntry {
    /// The unique ID of this entry
    pub id: StreamId,
    /// Field-value pairs stored in the entry
    pub fields: HashMap<Vec<u8>, Vec<u8>>,
}

/// Thread-safe stream data structure
pub struct Stream {
    /// Inner data protected by RwLock
    inner: Arc<RwLock<StreamInner>>,
}

/// Inner stream data using BTreeMap for time-series ordering
struct StreamInner {
    /// Entries ordered by StreamId
    entries: BTreeMap<StreamId, StreamEntry>,
    /// Last generated ID for auto-generation
    last_id: StreamId,
    /// Number of entries
    length: usize,
    /// Memory usage in bytes
    memory_usage: usize,
}

/// Result of a range query
#[derive(Debug, Clone)]
pub struct StreamRangeResult {
    pub entries: Vec<StreamEntry>,
}

impl StreamId {
    /// Create a new StreamId with specific timestamp and sequence
    pub fn new(millis: u64, seq: u64) -> Self {
        StreamId { millis, seq }
    }
    
    /// Create a StreamId from a string in format "millis-seq"
    pub fn from_string(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 2 {
            return None;
        }
        
        let millis = parts[0].parse::<u64>().ok()?;
        let seq = parts[1].parse::<u64>().ok()?;
        
        Some(StreamId { millis, seq })
    }
    
    /// Convert StreamId to string format "millis-seq"
    pub fn to_string(&self) -> String {
        format!("{}-{}", self.millis, self.seq)
    }
    
    /// Get current time as milliseconds since Unix epoch
    fn current_time_millis() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
    
    /// Generate next ID based on current time and last ID
    pub fn generate_next(last_id: &StreamId) -> Self {
        let now_millis = Self::current_time_millis();
        
        if now_millis > last_id.millis {
            // New timestamp, start sequence at 0
            StreamId::new(now_millis, 0)
        } else {
            // Same timestamp, increment sequence
            StreamId::new(last_id.millis, last_id.seq + 1)
        }
    }
    
    /// Minimum possible ID
    pub fn min() -> Self {
        StreamId::new(0, 0)
    }
    
    /// Maximum possible ID
    pub fn max() -> Self {
        StreamId::new(u64::MAX, u64::MAX)
    }
}

impl PartialOrd for StreamId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for StreamId {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.millis.cmp(&other.millis) {
            Ordering::Equal => self.seq.cmp(&other.seq),
            ord => ord,
        }
    }
}

impl Display for StreamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.millis, self.seq)
    }
}

impl Stream {
    /// Create a new empty stream
    pub fn new() -> Self {
        Stream {
            inner: Arc::new(RwLock::new(StreamInner {
                entries: BTreeMap::new(),
                last_id: StreamId::new(0, 0),
                length: 0,
                memory_usage: std::mem::size_of::<StreamInner>(),
            }))
        }
    }

    
    /// Add an entry to the stream with auto-generated ID
    pub fn add_auto(&self, fields: HashMap<Vec<u8>, Vec<u8>>) -> StreamId {
        let mut inner = self.inner.write().unwrap();
        
        // Generate next ID
        let id = StreamId::generate_next(&inner.last_id);
        
        // Create entry
        let entry = StreamEntry {
            id: id.clone(),
            fields,
        };
        
        // Calculate memory usage
        let entry_size = Self::calculate_entry_size(&entry);
        
        // Insert entry
        inner.entries.insert(id.clone(), entry);
        inner.last_id = id.clone();
        inner.length += 1;
        inner.memory_usage += entry_size;
        
        id
    }
    
    /// Add an entry to the stream with specific ID
    pub fn add_with_id(&self, id: StreamId, fields: HashMap<Vec<u8>, Vec<u8>>) -> Result<(), &'static str> {
        let mut inner = self.inner.write().unwrap();
        
        // Validate ID is greater than last ID
        if id <= inner.last_id {
            return Err("The ID specified in XADD is equal or smaller than the target stream top item");
        }
        
        // Check if ID already exists in the stream (duplicate detection)
        if inner.entries.contains_key(&id) {
            return Err("The ID specified in XADD already exists");
        }
        
        // Create entry
        let entry = StreamEntry {
            id: id.clone(),
            fields,
        };
        
        // Calculate memory usage
        let entry_size = Self::calculate_entry_size(&entry);
        
        // Insert entry
        inner.entries.insert(id.clone(), entry);
        inner.last_id = id;
        inner.length += 1;
        inner.memory_usage += entry_size;
        
        Ok(())
    }
    
    /// Get entries in a range of IDs (inclusive)
    pub fn range(&self, start: &StreamId, end: &StreamId, count: Option<usize>, reverse: bool) -> StreamRangeResult {
        let inner = self.inner.read().unwrap();
        
        let entries: Vec<StreamEntry> = if reverse {
            let range = inner.entries.range(start..=end);
            let mut collected: Vec<_> = range.map(|(id, entry)| StreamEntry {
                id: id.clone(),
                fields: entry.fields.clone(),
            }).collect();
            
            collected.reverse();
            
            if let Some(count) = count {
                collected.truncate(count);
            }
            collected
        } else {
            let range = inner.entries.range(start..=end);
            let mut entries: Vec<_> = range.map(|(id, entry)| StreamEntry {
                id: id.clone(), 
                fields: entry.fields.clone(),
            }).collect();
            
            if let Some(count) = count {
                entries.truncate(count);
            }
            entries
        };
        
        StreamRangeResult { entries }
    }
    
    /// Get entries after a specific ID
    pub fn range_after(&self, after_id: &StreamId, count: Option<usize>) -> StreamRangeResult {
        let inner = self.inner.read().unwrap();
        
        // Find entries after the given ID
        let range = inner.entries.range((std::ops::Bound::Excluded(after_id), std::ops::Bound::Unbounded));
        
        let mut entries: Vec<_> = range.map(|(_, entry)| entry.clone()).collect();
        if let Some(count) = count {
            entries.truncate(count);
        }
        
        StreamRangeResult { entries }
    }
    
    /// Get the last entry in the stream
    pub fn last_entry(&self) -> Option<StreamEntry> {
        let inner = self.inner.read().unwrap();
        inner.entries.iter().next_back().map(|(_, entry)| entry.clone())
    }
    
    /// Get the first entry in the stream
    pub fn first_entry(&self) -> Option<StreamEntry> {
        let inner = self.inner.read().unwrap();
        inner.entries.iter().next().map(|(_, entry)| entry.clone())
    }
    
    /// Get the number of entries
    pub fn len(&self) -> usize {
        self.inner.read().unwrap().length
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    
    /// Get total memory usage in bytes
    pub fn memory_usage(&self) -> usize {
        self.inner.read().unwrap().memory_usage
    }
    
    /// Trim the stream by count (keep only the newest N entries)
    pub fn trim_by_count(&self, max_count: usize) -> usize {
        let mut inner = self.inner.write().unwrap();
        
        if inner.length <= max_count {
            return 0;
        }
        
        let to_remove = inner.length - max_count;
        let mut removed = 0;
        
        // Collect IDs to remove
        let ids_to_remove: Vec<StreamId> = inner.entries
            .iter()
            .take(to_remove)
            .map(|(id, _)| id.clone())
            .collect();
        
        // Remove entries
        for id in ids_to_remove {
            if let Some(entry) = inner.entries.remove(&id) {
                inner.memory_usage -= Self::calculate_entry_size(&entry);
                removed += 1;
            }
        }
        
        inner.length -= removed;
        removed
    }
    
    /// Trim the stream by minimum ID (remove entries older than min_id)
    pub fn trim_by_min_id(&self, min_id: &StreamId) -> usize {
        let mut inner = self.inner.write().unwrap();
        
        // Collect IDs to remove
        let ids_to_remove: Vec<StreamId> = inner.entries
            .range(..min_id)
            .map(|(id, _)| id.clone())
            .collect();
        
        let mut removed = 0;
        
        // Remove entries
        for id in ids_to_remove {
            if let Some(entry) = inner.entries.remove(&id) {
                inner.memory_usage -= Self::calculate_entry_size(&entry);
                removed += 1;
            }
        }
        
        inner.length -= removed;
        removed
    }
    
    /// Delete specific entries by ID
    pub fn delete(&self, ids: &[StreamId]) -> usize {
        let mut inner = self.inner.write().unwrap();
        let mut deleted = 0;
        
        for id in ids {
            if let Some(entry) = inner.entries.remove(id) {
                inner.memory_usage -= Self::calculate_entry_size(&entry);
                deleted += 1;
            }
        }
        
        inner.length -= deleted;
        deleted
    }
    
    /// Get specific entries by ID
    pub fn get_entries(&self, ids: &[StreamId]) -> Vec<Option<StreamEntry>> {
        let inner = self.inner.read().unwrap();
        
        ids.iter()
            .map(|id| inner.entries.get(id).cloned())
            .collect()
    }
    
    /// Clear all entries
    pub fn clear(&self) {
        let mut inner = self.inner.write().unwrap();
        inner.entries.clear();
        inner.last_id = StreamId::new(0, 0);
        inner.length = 0;
        inner.memory_usage = std::mem::size_of::<StreamInner>();
    }

    
    /// Calculate memory size for an entry
    fn calculate_entry_size(entry: &StreamEntry) -> usize {
        let id_size = std::mem::size_of::<StreamId>();
        let fields_size: usize = entry.fields.iter()
            .map(|(k, v)| k.len() + v.len() + std::mem::size_of::<(Vec<u8>, Vec<u8>)>())
            .sum();
        
        id_size + fields_size + std::mem::size_of::<StreamEntry>()
    }
}

impl Default for Stream {
    fn default() -> Self {
        Self::new()
    }
}

impl Debug for Stream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.inner.read().unwrap();
        write!(f, "Stream {{ len: {}, last_id: {} }}", inner.length, inner.last_id)
    }
}

// Make Stream thread-safe
unsafe impl Send for Stream {}
unsafe impl Sync for Stream {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_stream_id_ordering() {
        let id1 = StreamId::new(1000, 0);
        let id2 = StreamId::new(1000, 1);
        let id3 = StreamId::new(1001, 0);
        
        assert!(id1 < id2);
        assert!(id2 < id3);
        assert!(id1 < id3);
    }
    
    #[test]
    fn test_stream_id_parsing() {
        let id = StreamId::from_string("1526919030474-55").unwrap();
        assert_eq!(id.millis, 1526919030474);
        assert_eq!(id.seq, 55);
        assert_eq!(id.to_string(), "1526919030474-55");
        
        assert!(StreamId::from_string("invalid").is_none());
        assert!(StreamId::from_string("123").is_none());
        assert!(StreamId::from_string("123-456-789").is_none());
    }
    
    #[test]
    fn test_add_auto() {
        let stream = Stream::new();
        
        let mut fields1 = HashMap::new();
        fields1.insert(b"field1".to_vec(), b"value1".to_vec());
        
        let id1 = stream.add_auto(fields1);
        assert!(id1.millis > 0);
        assert_eq!(id1.seq, 0);
        
        // Add another entry immediately
        let mut fields2 = HashMap::new();
        fields2.insert(b"field2".to_vec(), b"value2".to_vec());
        
        let id2 = stream.add_auto(fields2);
        assert!(id2 > id1);
        
        assert_eq!(stream.len(), 2);
    }
    
    #[test]
    fn test_add_with_id() {
        let stream = Stream::new();
        
        let mut fields = HashMap::new();
        fields.insert(b"key".to_vec(), b"value".to_vec());
        
        // Add with specific ID
        let id1 = StreamId::new(1000, 0);
        stream.add_with_id(id1.clone(), fields.clone()).unwrap();
        
        // Try to add with same ID - should fail
        let result = stream.add_with_id(id1.clone(), fields.clone());
        assert!(result.is_err());
        
        // Add with greater ID - should succeed
        let id2 = StreamId::new(1000, 1);
        stream.add_with_id(id2, fields).unwrap();
        
        assert_eq!(stream.len(), 2);
    }
    
    #[test]
    fn test_range_queries() {
        let stream = Stream::new();
        
        // Add entries with specific IDs
        let ids = vec![
            StreamId::new(1000, 0),
            StreamId::new(1000, 1),
            StreamId::new(1001, 0),
            StreamId::new(1002, 0),
        ];
        
        for id in &ids {
            let mut fields = HashMap::new();
            fields.insert(b"id".to_vec(), id.to_string().into_bytes());
            stream.add_with_id(id.clone(), fields).unwrap();
        }
        
        // Test full range
        let result = stream.range(&StreamId::min(), &StreamId::max(), None, false);
        assert_eq!(result.entries.len(), 4);
        
        // Test partial range
        let start = StreamId::new(1000, 1);
        let end = StreamId::new(1001, 0);
        let result = stream.range(&start, &end, None, false);
        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.entries[0].id, ids[1]);
        assert_eq!(result.entries[1].id, ids[2]);
        
        // Test with count limit
        let result = stream.range(&StreamId::min(), &StreamId::max(), Some(2), false);
        assert_eq!(result.entries.len(), 2);
        
        // Test reverse
        let result = stream.range(&StreamId::min(), &StreamId::max(), None, true);
        assert_eq!(result.entries.len(), 4);
        assert_eq!(result.entries[0].id, ids[3]);
    }
    
    #[test]
    fn test_trim() {
        let stream = Stream::new();
        
        // Add 5 entries
        for i in 0..5 {
            let mut fields = HashMap::new();
            fields.insert(b"num".to_vec(), i.to_string().into_bytes());
            stream.add_with_id(StreamId::new(1000 + i, 0), fields).unwrap();
        }
        
        // Trim by count
        let removed = stream.trim_by_count(3);
        assert_eq!(removed, 2);
        assert_eq!(stream.len(), 3);
        
        // Verify oldest entries were removed
        let first = stream.first_entry().unwrap();
        assert_eq!(first.id, StreamId::new(1002, 0));
        
        // Trim by min ID
        let removed = stream.trim_by_min_id(&StreamId::new(1003, 0));
        assert_eq!(removed, 1);
        assert_eq!(stream.len(), 2);
    }
}