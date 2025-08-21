//! Consumer Groups implementation for Redis Streams
//! 
//! Provides distributed message consumption with acknowledgments,
//! pending entry lists, and ownership management.

use std::collections::{HashMap, BTreeMap};
use std::sync::{Arc, RwLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::storage::stream::{StreamId, StreamEntry};

/// Consumer group for coordinated stream consumption
#[derive(Debug, Clone)]
pub struct ConsumerGroup {
    /// Group name
    pub name: String,
    
    /// Last delivered ID for this group
    pub last_delivered_id: Arc<Mutex<StreamId>>,
    
    /// Stream ID to start reading from
    pub stream_id: StreamId,
    
    /// Pending entries list (PEL) - entries delivered but not acknowledged
    pub pending: Arc<RwLock<PendingEntryList>>,
    
    /// Consumers in this group
    pub consumers: Arc<RwLock<HashMap<String, Consumer>>>,
    
    /// Creation time
    pub created_at: SystemTime,
    
    /// Number of consumers
    pub consumer_count: Arc<Mutex<usize>>,
    
    /// Total pending messages across all consumers
    pub total_pending: Arc<Mutex<usize>>,
}

/// Individual consumer within a group
#[derive(Debug, Clone)]
pub struct Consumer {
    /// Consumer name
    pub name: String,
    
    /// Number of pending messages for this consumer
    pub pending_count: usize,
    
    /// Last seen time (last XREADGROUP call)
    pub last_seen: SystemTime,
    
    /// Idle time in milliseconds
    pub idle_time: u64,
}

/// Entry in the pending entries list
#[derive(Debug, Clone)]
pub struct PendingEntry {
    /// Stream entry ID
    pub id: StreamId,
    
    /// Consumer that claimed this entry
    pub consumer: String,
    
    /// Time when entry was delivered
    pub delivered_at: SystemTime,
    
    /// Number of times this entry was delivered
    pub delivery_count: u32,
    
    /// Last delivery time
    pub last_delivery: SystemTime,
}

/// Pending entries list - tracks unacknowledged messages
#[derive(Debug)]
pub struct PendingEntryList {
    /// Entries by ID for fast lookup
    entries_by_id: BTreeMap<StreamId, PendingEntry>,
    
    /// Entries by consumer for consumer-specific queries
    entries_by_consumer: HashMap<String, Vec<StreamId>>,
    
    /// Global minimum pending ID
    min_pending_id: Option<StreamId>,
    
    /// Global maximum pending ID  
    max_pending_id: Option<StreamId>,
}

/// Result of XPENDING command
#[derive(Debug, Clone)]
pub struct PendingInfo {
    /// Total number of pending messages
    pub count: usize,
    
    /// Smallest pending ID
    pub min_id: Option<StreamId>,
    
    /// Greatest pending ID
    pub max_id: Option<StreamId>,
    
    /// Per-consumer pending counts
    pub consumers: Vec<(String, usize)>,
}

/// Detailed pending entry information
#[derive(Debug, Clone)]
pub struct PendingEntryInfo {
    /// Entry ID
    pub id: StreamId,
    
    /// Consumer name
    pub consumer: String,
    
    /// Milliseconds since last delivery
    pub idle_time: u64,
    
    /// Number of deliveries
    pub delivery_count: u32,
}

impl ConsumerGroup {
    /// Create a new consumer group
    pub fn new(name: String, stream_id: StreamId) -> Self {
        ConsumerGroup {
            name: name.clone(),
            last_delivered_id: Arc::new(Mutex::new(StreamId::new(0, 0))),
            stream_id,
            pending: Arc::new(RwLock::new(PendingEntryList::new())),
            consumers: Arc::new(RwLock::new(HashMap::new())),
            created_at: SystemTime::now(),
            consumer_count: Arc::new(Mutex::new(0)),
            total_pending: Arc::new(Mutex::new(0)),
        }
    }
    
    /// Create or get a consumer
    pub fn create_consumer(&self, consumer_name: String) -> bool {
        let mut consumers = self.consumers.write().unwrap();
        
        if consumers.contains_key(&consumer_name) {
            // Consumer already exists, just update last seen
            if let Some(consumer) = consumers.get_mut(&consumer_name) {
                consumer.last_seen = SystemTime::now();
                consumer.idle_time = 0;
            }
            false
        } else {
            // Create new consumer
            consumers.insert(consumer_name.clone(), Consumer {
                name: consumer_name,
                pending_count: 0,
                last_seen: SystemTime::now(),
                idle_time: 0,
            });
            
            let mut count = self.consumer_count.lock().unwrap();
            *count += 1;
            true
        }
    }
    
    /// Delete a consumer and return number of pending messages removed
    pub fn delete_consumer(&self, consumer_name: &str) -> usize {
        let mut consumers = self.consumers.write().unwrap();
        
        if let Some(consumer) = consumers.remove(consumer_name) {
            // Remove all pending entries for this consumer
            let mut pending = self.pending.write().unwrap();
            let removed = pending.remove_consumer_entries(consumer_name);
            
            let mut count = self.consumer_count.lock().unwrap();
            *count = count.saturating_sub(1);
            
            let mut total = self.total_pending.lock().unwrap();
            *total = total.saturating_sub(removed);
            
            removed
        } else {
            0
        }
    }
    
    /// Add entries to pending list when delivered via XREADGROUP
    pub fn add_pending(&self, consumer: &str, entries: Vec<StreamEntry>) -> Vec<StreamEntry> {
        let mut pending = self.pending.write().unwrap();
        let now = SystemTime::now();
        
        // Ensure consumer exists
        self.create_consumer(consumer.to_string());
        
        // Add each entry to pending list
        for entry in &entries {
            let pending_entry = PendingEntry {
                id: entry.id,
                consumer: consumer.to_string(),
                delivered_at: now,
                delivery_count: 1,
                last_delivery: now,
            };
            
            pending.add_entry(pending_entry);
        }
        
        // Update consumer's pending count
        let mut consumers = self.consumers.write().unwrap();
        if let Some(consumer_obj) = consumers.get_mut(consumer) {
            consumer_obj.pending_count += entries.len();
            consumer_obj.last_seen = now;
            consumer_obj.idle_time = 0;
        }
        
        // Update total pending
        let mut total = self.total_pending.lock().unwrap();
        *total += entries.len();
        
        // Update last delivered ID
        if let Some(last_entry) = entries.last() {
            let mut last_id = self.last_delivered_id.lock().unwrap();
            if last_entry.id > *last_id {
                *last_id = last_entry.id;
            }
        }
        
        entries
    }
    
    /// Acknowledge messages, removing them from pending
    pub fn acknowledge(&self, ids: &[StreamId]) -> usize {
        let mut pending = self.pending.write().unwrap();
        let mut consumers = self.consumers.write().unwrap();
        let mut acked = 0;
        
        for id in ids {
            if let Some(entry) = pending.remove_entry(id) {
                // Update consumer's pending count
                if let Some(consumer) = consumers.get_mut(&entry.consumer) {
                    consumer.pending_count = consumer.pending_count.saturating_sub(1);
                }
                acked += 1;
            }
        }
        
        // Update total pending
        if acked > 0 {
            let mut total = self.total_pending.lock().unwrap();
            *total = total.saturating_sub(acked);
        }
        
        acked
    }
    
    /// Claim ownership of messages from another consumer
    pub fn claim_messages(
        &self, 
        new_consumer: &str, 
        min_idle_ms: u64, 
        ids: &[StreamId],
        force: bool
    ) -> Vec<StreamId> {
        let mut pending = self.pending.write().unwrap();
        let mut claimed = Vec::new();
        let now = SystemTime::now();
        
        // Ensure new consumer exists
        self.create_consumer(new_consumer.to_string());
        
        for id in ids {
            if let Some(entry) = pending.get_entry_mut(id) {
                // Check idle time if not forcing
                if !force {
                    let idle_ms = now.duration_since(entry.last_delivery)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    
                    if idle_ms < min_idle_ms {
                        continue;
                    }
                }
                
                // Update consumer pending counts
                let mut consumers = self.consumers.write().unwrap();
                
                // Decrease old consumer's count
                if let Some(old_consumer) = consumers.get_mut(&entry.consumer) {
                    old_consumer.pending_count = old_consumer.pending_count.saturating_sub(1);
                }
                
                // Increase new consumer's count
                if let Some(new_consumer_obj) = consumers.get_mut(new_consumer) {
                    new_consumer_obj.pending_count += 1;
                }
                
                drop(consumers);
                
                // Transfer ownership
                pending.transfer_ownership(id, new_consumer.to_string());
                claimed.push(*id);
            }
        }
        
        claimed
    }
    
    /// Get pending entries information
    pub fn get_pending_info(&self) -> PendingInfo {
        let pending = self.pending.read().unwrap();
        let consumers = self.consumers.read().unwrap();
        
        let consumer_counts: Vec<(String, usize)> = consumers
            .iter()
            .filter(|(_, c)| c.pending_count > 0)
            .map(|(name, c)| (name.clone(), c.pending_count))
            .collect();
        
        PendingInfo {
            count: pending.len(),
            min_id: pending.min_id(),
            max_id: pending.max_id(),
            consumers: consumer_counts,
        }
    }
    
    /// Get detailed pending entries within range
    pub fn get_pending_range(
        &self,
        start: Option<StreamId>,
        end: Option<StreamId>,
        count: usize,
        consumer: Option<&str>
    ) -> Vec<PendingEntryInfo> {
        let pending = self.pending.read().unwrap();
        let now = SystemTime::now();
        
        pending.get_range(start, end, count, consumer)
            .into_iter()
            .map(|entry| {
                let idle_ms = now.duration_since(entry.last_delivery)
                    .unwrap_or_default()
                    .as_millis() as u64;
                
                PendingEntryInfo {
                    id: entry.id,
                    consumer: entry.consumer.clone(),
                    idle_time: idle_ms,
                    delivery_count: entry.delivery_count,
                }
            })
            .collect()
    }
    
    /// Auto-claim idle messages
    pub fn auto_claim(
        &self,
        consumer: &str,
        min_idle_ms: u64,
        start: StreamId,
        count: usize
    ) -> (Vec<StreamId>, StreamId) {
        let pending = self.pending.read().unwrap();
        let now = SystemTime::now();
        
        let idle_entries: Vec<StreamId> = pending
            .get_entries_after(start)
            .into_iter()
            .filter(|entry| {
                let idle_ms = now.duration_since(entry.last_delivery)
                    .unwrap_or_default()
                    .as_millis() as u64;
                idle_ms >= min_idle_ms
            })
            .take(count)
            .map(|e| e.id)
            .collect();
        
        drop(pending);
        
        // Claim the idle entries
        let claimed = self.claim_messages(consumer, min_idle_ms, &idle_entries, false);
        
        // Calculate next start ID
        let next_start = if let Some(last) = claimed.last() {
            // Return ID after the last claimed
            StreamId::new(last.millis(), last.seq() + 1)
        } else {
            StreamId::new(0, 0)
        };
        
        (claimed, next_start)
    }
    
    /// Set the last delivered ID for the group
    pub fn set_id(&self, id: StreamId) {
        let mut last_id = self.last_delivered_id.lock().unwrap();
        *last_id = id;
    }
    
    /// Get the last delivered ID
    pub fn get_last_id(&self) -> StreamId {
        let last_id = self.last_delivered_id.lock().unwrap();
        *last_id
    }
}

impl PendingEntryList {
    /// Create a new pending entry list
    pub fn new() -> Self {
        PendingEntryList {
            entries_by_id: BTreeMap::new(),
            entries_by_consumer: HashMap::new(),
            min_pending_id: None,
            max_pending_id: None,
        }
    }
    
    /// Add an entry to the pending list
    pub fn add_entry(&mut self, entry: PendingEntry) {
        let id = entry.id;
        let consumer = entry.consumer.clone();
        
        // Add to ID index
        self.entries_by_id.insert(id, entry);
        
        // Add to consumer index
        self.entries_by_consumer
            .entry(consumer)
            .or_insert_with(Vec::new)
            .push(id);
        
        // Update min/max
        self.update_bounds();
    }
    
    /// Remove an entry by ID
    pub fn remove_entry(&mut self, id: &StreamId) -> Option<PendingEntry> {
        if let Some(entry) = self.entries_by_id.remove(id) {
            // Remove from consumer index
            if let Some(consumer_entries) = self.entries_by_consumer.get_mut(&entry.consumer) {
                consumer_entries.retain(|&x| x != *id);
                if consumer_entries.is_empty() {
                    self.entries_by_consumer.remove(&entry.consumer);
                }
            }
            
            // Update bounds
            self.update_bounds();
            
            Some(entry)
        } else {
            None
        }
    }
    
    /// Get mutable reference to an entry
    pub fn get_entry_mut(&mut self, id: &StreamId) -> Option<&mut PendingEntry> {
        self.entries_by_id.get_mut(id)
    }
    
    /// Transfer ownership of an entry to a new consumer
    pub fn transfer_ownership(&mut self, id: &StreamId, new_consumer: String) {
        if let Some(entry) = self.entries_by_id.get_mut(id) {
            let old_consumer = entry.consumer.clone();
            
            // Remove from old consumer's index
            if let Some(old_entries) = self.entries_by_consumer.get_mut(&old_consumer) {
                old_entries.retain(|&x| x != *id);
                if old_entries.is_empty() {
                    self.entries_by_consumer.remove(&old_consumer);
                }
            }
            
            // Update entry
            entry.consumer = new_consumer.clone();
            entry.delivery_count += 1;
            entry.last_delivery = SystemTime::now();
            
            // Add to new consumer's index
            self.entries_by_consumer
                .entry(new_consumer)
                .or_insert_with(Vec::new)
                .push(*id);
        }
    }
    
    /// Remove all entries for a consumer
    pub fn remove_consumer_entries(&mut self, consumer: &str) -> usize {
        if let Some(entries) = self.entries_by_consumer.remove(consumer) {
            let count = entries.len();
            for id in entries {
                self.entries_by_id.remove(&id);
            }
            self.update_bounds();
            count
        } else {
            0
        }
    }
    
    /// Get entries within a range
    pub fn get_range(
        &self,
        start: Option<StreamId>,
        end: Option<StreamId>,
        count: usize,
        consumer: Option<&str>
    ) -> Vec<PendingEntry> {
        let iter: Box<dyn Iterator<Item = &PendingEntry>> = if let Some(consumer_name) = consumer {
            // Filter by consumer
            if let Some(consumer_ids) = self.entries_by_consumer.get(consumer_name) {
                Box::new(
                    consumer_ids.iter()
                        .filter_map(|id| self.entries_by_id.get(id))
                )
            } else {
                Box::new(std::iter::empty())
            }
        } else {
            // All entries in range
            let start = start.unwrap_or(StreamId::min());
            let end = end.unwrap_or(StreamId::max());
            
            Box::new(
                self.entries_by_id
                    .range(start..=end)
                    .map(|(_, entry)| entry)
            )
        };
        
        iter.take(count).cloned().collect()
    }
    
    /// Get entries after a specific ID
    pub fn get_entries_after(&self, after: StreamId) -> Vec<PendingEntry> {
        self.entries_by_id
            .range((std::ops::Bound::Excluded(after), std::ops::Bound::Unbounded))
            .map(|(_, entry)| entry.clone())
            .collect()
    }
    
    /// Get the number of pending entries
    pub fn len(&self) -> usize {
        self.entries_by_id.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entries_by_id.is_empty()
    }
    
    /// Get minimum pending ID
    pub fn min_id(&self) -> Option<StreamId> {
        self.min_pending_id
    }
    
    /// Get maximum pending ID
    pub fn max_id(&self) -> Option<StreamId> {
        self.max_pending_id
    }
    
    /// Update min/max bounds
    fn update_bounds(&mut self) {
        self.min_pending_id = self.entries_by_id.keys().min().copied();
        self.max_pending_id = self.entries_by_id.keys().max().copied();
    }
}

/// Manager for all consumer groups of a stream
#[derive(Debug)]
pub struct ConsumerGroupManager {
    /// Groups by name
    groups: Arc<RwLock<HashMap<String, Arc<ConsumerGroup>>>>,
}

impl ConsumerGroupManager {
    /// Create a new consumer group manager
    pub fn new() -> Self {
        ConsumerGroupManager {
            groups: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Create a new consumer group
    pub fn create_group(&self, name: String, start_id: StreamId) -> Result<(), String> {
        let mut groups = self.groups.write().unwrap();
        
        if groups.contains_key(&name) {
            return Err("BUSYGROUP Consumer Group name already exists".to_string());
        }
        
        let group = Arc::new(ConsumerGroup::new(name.clone(), start_id));
        groups.insert(name, group);
        Ok(())
    }
    
    /// Destroy a consumer group
    pub fn destroy_group(&self, name: &str) -> bool {
        let mut groups = self.groups.write().unwrap();
        groups.remove(name).is_some()
    }
    
    /// Get a consumer group by name
    pub fn get_group(&self, name: &str) -> Option<Arc<ConsumerGroup>> {
        let groups = self.groups.read().unwrap();
        groups.get(name).cloned()
    }
    
    /// List all groups
    pub fn list_groups(&self) -> Vec<Arc<ConsumerGroup>> {
        let groups = self.groups.read().unwrap();
        groups.values().cloned().collect()
    }
    
    /// Get group count
    pub fn group_count(&self) -> usize {
        let groups = self.groups.read().unwrap();
        groups.len()
    }
}

impl Default for ConsumerGroupManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_consumer_group_creation() {
        let group = ConsumerGroup::new("mygroup".to_string(), StreamId::new(0, 0));
        assert_eq!(group.name, "mygroup");
        assert_eq!(*group.consumer_count.lock().unwrap(), 0);
    }
    
    #[test]
    fn test_consumer_management() {
        let group = ConsumerGroup::new("mygroup".to_string(), StreamId::new(0, 0));
        
        // Create consumer
        assert!(group.create_consumer("consumer1".to_string()));
        assert!(!group.create_consumer("consumer1".to_string())); // Already exists
        
        assert_eq!(*group.consumer_count.lock().unwrap(), 1);
        
        // Delete consumer
        let removed = group.delete_consumer("consumer1");
        assert_eq!(removed, 0); // No pending messages
        assert_eq!(*group.consumer_count.lock().unwrap(), 0);
    }
    
    #[test]
    fn test_pending_entry_management() {
        let group = ConsumerGroup::new("mygroup".to_string(), StreamId::new(0, 0));
        
        // Add pending entries
        let entries = vec![
            StreamEntry {
                id: StreamId::new(1000, 0),
                fields: HashMap::new(),
            },
            StreamEntry {
                id: StreamId::new(1000, 1),
                fields: HashMap::new(),
            },
        ];
        
        group.add_pending("consumer1", entries);
        assert_eq!(*group.total_pending.lock().unwrap(), 2);
        
        // Acknowledge one entry
        let acked = group.acknowledge(&[StreamId::new(1000, 0)]);
        assert_eq!(acked, 1);
        assert_eq!(*group.total_pending.lock().unwrap(), 1);
    }
    
    #[test]
    fn test_consumer_group_manager() {
        let manager = ConsumerGroupManager::new();
        
        // Create group
        assert!(manager.create_group("group1".to_string(), StreamId::new(0, 0)).is_ok());
        assert!(manager.create_group("group1".to_string(), StreamId::new(0, 0)).is_err()); // Duplicate
        
        // Get group
        assert!(manager.get_group("group1").is_some());
        assert!(manager.get_group("nonexistent").is_none());
        
        // Destroy group
        assert!(manager.destroy_group("group1"));
        assert!(!manager.destroy_group("group1")); // Already destroyed
    }
}