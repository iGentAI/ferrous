//! Consumer Groups implementation for Stream functionality
//! 
//! Provides Redis-compatible consumer group operations for streams including
//! XGROUP commands, XREADGROUP, XACK, XCLAIM, and XPENDING.

use std::collections::{HashMap, BTreeSet};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, Duration};
use crate::storage::stream::{StreamId, StreamEntry};

/// Consumer group information
#[derive(Debug, Clone)]
pub struct ConsumerGroup {
    /// Group name
    pub name: String,
    
    /// Stream key this group belongs to
    pub stream_key: Vec<u8>,
    
    /// Last delivered ID
    pub last_delivered_id: StreamId,
    
    /// Pending entries (delivered but not acknowledged)
    pub pending: HashMap<StreamId, PendingEntry>,
    
    /// Consumers in this group
    pub consumers: HashMap<String, Consumer>,
    
    /// Creation time
    pub created_at: SystemTime,
}

/// Individual consumer in a group
#[derive(Debug, Clone)]
pub struct Consumer {
    /// Consumer name
    pub name: String,
    
    /// Last seen timestamp
    pub last_seen: SystemTime,
    
    /// Total number of pending messages for this consumer
    pub pending_count: usize,
    
    /// IDs of pending messages for this consumer
    pub pending_ids: BTreeSet<StreamId>,
}

/// Pending entry information
#[derive(Debug, Clone)]
pub struct PendingEntry {
    /// Stream ID
    pub id: StreamId,
    
    /// Consumer who received this entry
    pub consumer: String,
    
    /// Time when entry was delivered
    pub delivery_time: SystemTime,
    
    /// Number of times this entry has been delivered
    pub delivery_count: u64,
}

/// Consumer group manager for a stream
pub struct ConsumerGroupManager {
    /// Inner data protected by RwLock for thread safety
    inner: Arc<RwLock<ConsumerGroupManagerInner>>,
}

/// Inner consumer group manager data
struct ConsumerGroupManagerInner {
    /// Map of group name to group data
    groups: HashMap<String, ConsumerGroup>,
    
    /// Stream key this manager belongs to
    stream_key: Vec<u8>,
}

/// Result of XREADGROUP operation
#[derive(Debug)]
pub struct XReadGroupResult {
    pub entries: Vec<StreamEntry>,
    pub last_delivered_id: StreamId,
}

impl ConsumerGroup {
    /// Create a new consumer group
    pub fn new(name: String, stream_key: Vec<u8>, start_id: StreamId) -> Self {
        ConsumerGroup {
            name,
            stream_key,
            last_delivered_id: start_id,
            pending: HashMap::new(),
            consumers: HashMap::new(),
            created_at: SystemTime::now(),
        }
    }
    
    /// Add a consumer to the group
    pub fn add_consumer(&mut self, consumer_name: String) {
        if !self.consumers.contains_key(&consumer_name) {
            self.consumers.insert(consumer_name.clone(), Consumer {
                name: consumer_name,
                last_seen: SystemTime::now(),
                pending_count: 0,
                pending_ids: BTreeSet::new(),
            });
        }
    }
    
    /// Remove a consumer from the group
    pub fn remove_consumer(&mut self, consumer_name: &str) -> bool {
        if let Some(consumer) = self.consumers.remove(consumer_name) {
            // Move consumer's pending messages to the group's undelivered state
            for id in consumer.pending_ids {
                self.pending.remove(&id);
            }
            true
        } else {
            false
        }
    }
    
    /// Mark entries as pending for a consumer
    pub fn add_pending(&mut self, consumer_name: &str, entries: &[StreamEntry]) -> Result<(), &'static str> {
        let consumer = self.consumers.get_mut(consumer_name)
            .ok_or("Consumer not found")?;
        
        for entry in entries {
            let pending_entry = PendingEntry {
                id: entry.id.clone(),
                consumer: consumer_name.to_string(),
                delivery_time: SystemTime::now(),
                delivery_count: 1,
            };
            
            self.pending.insert(entry.id.clone(), pending_entry);
            consumer.pending_ids.insert(entry.id.clone());
            consumer.pending_count += 1;
        }
        
        consumer.last_seen = SystemTime::now();
        Ok(())
    }
    
    /// Acknowledge entries (remove from pending)
    pub fn acknowledge(&mut self, consumer_name: &str, ids: &[StreamId]) -> usize {
        let mut acknowledged = 0;
        
        for id in ids {
            if let Some(pending) = self.pending.get(id) {
                // Only allow acknowledgment by the consumer who received it
                if pending.consumer == consumer_name {
                    self.pending.remove(id);
                    acknowledged += 1;
                    
                    // Update consumer
                    if let Some(consumer) = self.consumers.get_mut(consumer_name) {
                        consumer.pending_ids.remove(id);
                        consumer.pending_count = consumer.pending_count.saturating_sub(1);
                    }
                }
            }
        }
        
        acknowledged
    }
    
    /// Get pending entries for the group or specific consumer
    pub fn get_pending(&self, consumer_name: Option<&str>, start: Option<StreamId>, end: Option<StreamId>, count: Option<usize>) -> Vec<PendingEntry> {
        let mut entries: Vec<_> = if let Some(consumer) = consumer_name {
            // Get pending for specific consumer
            self.pending.values()
                .filter(|p| p.consumer == consumer)
                .cloned()
                .collect()
        } else {
            // Get all pending entries
            self.pending.values().cloned().collect()
        };
        
        // Sort by stream ID
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        
        // Apply range filter if specified
        if let (Some(start), Some(end)) = (start, end) {
            entries.retain(|p| p.id >= start && p.id <= end);
        }
        
        // Apply count limit
        if let Some(count) = count {
            entries.truncate(count);
        }
        
        entries
    }
    
    /// Claim idle entries from other consumers
    pub fn claim_entries(&mut self, consumer_name: &str, min_idle_time: Duration, ids: &[StreamId]) -> Vec<PendingEntry> {
        let now = SystemTime::now();
        let mut claimed = Vec::new();
        
        for id in ids {
            if let Some(pending) = self.pending.get_mut(id) {
                // Check if entry is idle long enough
                if let Ok(idle_time) = now.duration_since(pending.delivery_time) {
                    if idle_time >= min_idle_time {
                        // Update ownership
                        let old_consumer = pending.consumer.clone();
                        pending.consumer = consumer_name.to_string();
                        pending.delivery_time = now;
                        pending.delivery_count += 1;
                        
                        // Update consumer records
                        if let Some(old_cons) = self.consumers.get_mut(&old_consumer) {
                            old_cons.pending_ids.remove(id);
                            old_cons.pending_count = old_cons.pending_count.saturating_sub(1);
                        }
                        
                        if let Some(new_cons) = self.consumers.get_mut(consumer_name) {
                            new_cons.pending_ids.insert(id.clone());
                            new_cons.pending_count += 1;
                            new_cons.last_seen = now;
                        }
                        
                        claimed.push(pending.clone());
                    }
                }
            }
        }
        
        claimed
    }
}

impl Consumer {
    /// Create a new consumer
    pub fn new(name: String) -> Self {
        Consumer {
            name,
            last_seen: SystemTime::now(),
            pending_count: 0,
            pending_ids: BTreeSet::new(),
        }
    }
    
    /// Get idle time since last seen
    pub fn idle_time(&self) -> Duration {
        SystemTime::now().duration_since(self.last_seen).unwrap_or(Duration::from_secs(0))
    }
}

impl ConsumerGroupManager {
    /// Create a new consumer group manager for a stream
    pub fn new(stream_key: Vec<u8>) -> Self {
        ConsumerGroupManager {
            inner: Arc::new(RwLock::new(ConsumerGroupManagerInner {
                groups: HashMap::new(),
                stream_key,
            })),
        }
    }
    
    /// Create a new consumer group
    pub fn create_group(&self, group_name: String, start_id: StreamId) -> Result<bool, &'static str> {
        let mut inner = self.inner.write().unwrap();
        
        if inner.groups.contains_key(&group_name) {
            return Ok(false); // Group already exists
        }
        
        let group = ConsumerGroup::new(group_name.clone(), inner.stream_key.clone(), start_id);
        inner.groups.insert(group_name, group);
        
        Ok(true)
    }
    
    /// Destroy a consumer group
    pub fn destroy_group(&self, group_name: &str) -> Result<bool, &'static str> {
        let mut inner = self.inner.write().unwrap();
        
        Ok(inner.groups.remove(group_name).is_some())
    }
    
    /// Create a consumer in a group
    pub fn create_consumer(&self, group_name: &str, consumer_name: &str) -> Result<bool, &'static str> {
        let mut inner = self.inner.write().unwrap();
        
        let group = inner.groups.get_mut(group_name)
            .ok_or("NOGROUP No such key 'stream_key' or consumer group 'group_name' in XGROUP")?;
        
        let is_new = !group.consumers.contains_key(consumer_name);
        group.add_consumer(consumer_name.to_string());
        
        Ok(is_new)
    }
    
    /// Delete a consumer from a group
    pub fn delete_consumer(&self, group_name: &str, consumer_name: &str) -> Result<usize, &'static str> {
        let mut inner = self.inner.write().unwrap();
        
        let group = inner.groups.get_mut(group_name)
            .ok_or("NOGROUP No such key 'stream_key' or consumer group 'group_name' in XGROUP")?;
        
        if group.remove_consumer(consumer_name) {
            Ok(group.pending.len()) // Return number of pending messages that were deleted
        } else {
            Ok(0) // Consumer didn't exist
        }
    }
    
    /// Read from stream for a consumer group
    pub fn read_group(&self, group_name: &str, consumer_name: &str, new_entries: Vec<StreamEntry>, count: Option<usize>) -> Result<XReadGroupResult, &'static str> {
        let mut inner = self.inner.write().unwrap();
        
        let group = inner.groups.get_mut(group_name)
            .ok_or("NOGROUP No such key 'stream_key' or consumer group 'group_name' in XGROUP")?;
        
        // Ensure consumer exists
        group.add_consumer(consumer_name.to_string());
        
        // Apply count limit if specified
        let mut entries = new_entries;
        if let Some(count) = count {
            entries.truncate(count);
        }
        
        // Mark entries as pending if any were delivered
        if !entries.is_empty() {
            let last_id = entries.last().unwrap().id.clone();
            
            // Add to pending
            group.add_pending(consumer_name, &entries)
                .map_err(|_| "Failed to add pending entries")?;
            
            // Update last delivered ID
            if last_id > group.last_delivered_id {
                group.last_delivered_id = last_id.clone();
            }
            
            Ok(XReadGroupResult {
                entries,
                last_delivered_id: last_id,
            })
        } else {
            Ok(XReadGroupResult {
                entries: Vec::new(),
                last_delivered_id: group.last_delivered_id.clone(),
            })
        }
    }
    
    /// Acknowledge entries for a consumer group
    pub fn acknowledge(&self, group_name: &str, ids: &[StreamId]) -> Result<usize, &'static str> {
        let mut inner = self.inner.write().unwrap();
        
        let group = inner.groups.get_mut(group_name)
            .ok_or("NOGROUP No such key 'stream_key' or consumer group 'group_name' in XGROUP")?;
        
        let mut acknowledged = 0;
        for id in ids {
            if group.pending.remove(id).is_some() {
                acknowledged += 1;
                
                // Remove from all consumers' pending lists
                for consumer in group.consumers.values_mut() {
                    if consumer.pending_ids.remove(id) {
                        consumer.pending_count = consumer.pending_count.saturating_sub(1);
                    }
                }
            }
        }
        
        Ok(acknowledged)
    }
    
    /// Get pending information for a group
    pub fn get_pending_info(&self, group_name: &str, consumer_name: Option<&str>, start: Option<StreamId>, end: Option<StreamId>, count: Option<usize>) -> Result<Vec<PendingEntry>, &'static str> {
        let inner = self.inner.read().unwrap();
        
        let group = inner.groups.get(group_name)
            .ok_or("NOGROUP No such key 'stream_key' or consumer group 'group_name' in XGROUP")?;
        
        Ok(group.get_pending(consumer_name, start, end, count))
    }
    
    /// Claim entries from other consumers
    pub fn claim_entries(&self, group_name: &str, consumer_name: &str, min_idle_millis: u64, ids: &[StreamId]) -> Result<Vec<PendingEntry>, &'static str> {
        let mut inner = self.inner.write().unwrap();
        
        let group = inner.groups.get_mut(group_name)
            .ok_or("NOGROUP No such key 'stream_key' or consumer group 'group_name' in XGROUP")?;
        
        // Ensure consumer exists
        group.add_consumer(consumer_name.to_string());
        
        let min_idle_time = Duration::from_millis(min_idle_millis);
        Ok(group.claim_entries(consumer_name, min_idle_time, ids))
    }
    
    /// List all groups
    pub fn list_groups(&self) -> Vec<String> {
        let inner = self.inner.read().unwrap();
        inner.groups.keys().cloned().collect()
    }
    
    /// Get group information
    pub fn get_group_info(&self, group_name: &str) -> Option<ConsumerGroup> {
        let inner = self.inner.read().unwrap();
        inner.groups.get(group_name).cloned()
    }
    
    /// Get group summary for XPENDING
    pub fn get_pending_summary(&self, group_name: &str) -> Result<(usize, Option<StreamId>, Option<StreamId>, HashMap<String, usize>), &'static str> {
        let inner = self.inner.read().unwrap();
        
        let group = inner.groups.get(group_name)
            .ok_or("NOGROUP No such key 'stream_key' or consumer group 'group_name' in XGROUP")?;
        
        if group.pending.is_empty() {
            return Ok((0, None, None, HashMap::new()));
        }
        
        // Find min/max IDs and count by consumer
        let mut min_id = StreamId::max();
        let mut max_id = StreamId::min();
        let mut consumer_counts: HashMap<String, usize> = HashMap::new();
        
        for (id, entry) in &group.pending {
            if *id < min_id {
                min_id = id.clone();
            }
            if *id > max_id {
                max_id = id.clone();
            }
            *consumer_counts.entry(entry.consumer.clone()).or_insert(0) += 1;
        }
        
        Ok((group.pending.len(), Some(min_id), Some(max_id), consumer_counts))
    }
    
    /// Set the last delivered ID for a group
    pub fn set_id(&self, group_name: &str, id: StreamId) -> Result<(), &'static str> {
        let mut inner = self.inner.write().unwrap();
        
        let group = inner.groups.get_mut(group_name)
            .ok_or("NOGROUP No such key 'stream_key' or consumer group 'group_name' in XGROUP")?;
        
        group.last_delivered_id = id;
        Ok(())
    }
}

impl Default for ConsumerGroupManager {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    
    #[test]
    fn test_consumer_group_creation() {
        let manager = ConsumerGroupManager::new(b"stream:test".to_vec());
        
        // Create a group
        let created = manager.create_group("group1".to_string(), StreamId::new(0, 0)).unwrap();
        assert!(created);
        
        // Try to create the same group again
        let created = manager.create_group("group1".to_string(), StreamId::new(0, 0)).unwrap();
        assert!(!created); // Already exists
        
        // Check group exists
        let groups = manager.list_groups();
        assert!(groups.contains(&"group1".to_string()));
    }
    
    #[test]
    fn test_consumer_management() {
        let manager = ConsumerGroupManager::new(b"stream:test".to_vec());
        
        // Create group and consumer
        manager.create_group("group1".to_string(), StreamId::new(0, 0)).unwrap();
        let created = manager.create_consumer("group1", "consumer1").unwrap();
        assert!(created);
        
        // Create same consumer again
        let created = manager.create_consumer("group1", "consumer1").unwrap();
        assert!(!created); // Already exists
        
        // Delete consumer
        let deleted = manager.delete_consumer("group1", "consumer1").unwrap();
        assert_eq!(deleted, 0); // No pending messages
        
        // Delete non-existent consumer
        let deleted = manager.delete_consumer("group1", "nonexistent").unwrap();
        assert_eq!(deleted, 0);
    }
    
    #[test]
    fn test_pending_and_acknowledge() {
        let manager = ConsumerGroupManager::new(b"stream:test".to_vec());
        
        // Create group and consumer
        manager.create_group("group1".to_string(), StreamId::new(0, 0)).unwrap();
        manager.create_consumer("group1", "consumer1").unwrap();
        
        // Create some entries
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
        
        // Simulate reading (adds to pending)
        let result = manager.read_group("group1", "consumer1", entries, None).unwrap();
        assert_eq!(result.entries.len(), 2);
        
        // Check pending
        let pending = manager.get_pending_info("group1", Some("consumer1"), None, None, None).unwrap();
        assert_eq!(pending.len(), 2);
        
        // Acknowledge one entry
        let ids = vec![StreamId::new(1000, 0)];
        let acked = manager.acknowledge("group1", &ids).unwrap();
        assert_eq!(acked, 1);
        
        // Check pending again
        let pending = manager.get_pending_info("group1", Some("consumer1"), None, None, None).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, StreamId::new(1000, 1));
    }
}