//! Publish/Subscribe messaging system
//! 
//! This module implements Redis-compatible pub/sub functionality including:
//! - Channel subscriptions (SUBSCRIBE/UNSUBSCRIBE)
//! - Pattern subscriptions (PSUBSCRIBE/PUNSUBSCRIBE)
//! - Message publishing (PUBLISH)
//! - Thread-safe subscription management
//! - Efficient message delivery

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use crate::error::{FerrousError, Result};
use crate::protocol::RespFrame;

/// A subscription pattern or channel name
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Subscription {
    /// Direct channel subscription
    Channel(Vec<u8>),
    /// Pattern subscription (glob-style)
    Pattern(Vec<u8>),
}

/// Information about a subscriber
#[derive(Debug)]
pub struct SubscriberInfo {
    /// Connection ID
    pub connection_id: u64,
    /// Subscribed channels
    pub channels: HashSet<Vec<u8>>,
    /// Subscribed patterns
    pub patterns: HashSet<Vec<u8>>,
}

/// Result of a subscription operation
#[derive(Debug)]
pub struct SubResult {
    /// The subscription that was modified
    pub subscription: Subscription,
    /// Number of subscriptions this connection now has
    pub num_subscriptions: usize,
    /// Whether this was a new subscription (true) or already existed (false)
    pub is_new: bool,
}

/// Result of publishing a message
#[derive(Debug)]
pub struct PublishResult {
    /// Number of clients that received the message
    pub num_receivers: usize,
}

/// Manages all pub/sub subscriptions
pub struct PubSubManager {
    /// Channel subscriptions: channel -> set of connection IDs
    channels: Mutex<HashMap<Vec<u8>, HashSet<u64>>>,
    
    /// Pattern subscriptions: pattern -> set of connection IDs
    patterns: Mutex<HashMap<Vec<u8>, HashSet<u64>>>,
    
    /// Connection subscriptions: connection_id -> (channels, patterns)
    connections: Mutex<HashMap<u64, SubscriberInfo>>,
}

impl PubSubManager {
    /// Create a new pub/sub manager
    pub fn new() -> Arc<Self> {
        Arc::new(PubSubManager {
            channels: Mutex::new(HashMap::new()),
            patterns: Mutex::new(HashMap::new()),
            connections: Mutex::new(HashMap::new()),
        })
    }
    
    /// Subscribe a connection to one or more channels
    pub fn subscribe(&self, connection_id: u64, channels: Vec<Vec<u8>>) -> Result<Vec<SubResult>> {
        let mut results = Vec::new();
        
        let mut channel_subs = self.channels.lock().unwrap();
        let mut conn_subs = self.connections.lock().unwrap();
        
        // Ensure connection info exists  
        let conn_info = conn_subs.entry(connection_id).or_insert_with(|| {
            SubscriberInfo {
                connection_id,
                channels: HashSet::new(),
                patterns: HashSet::new(),
            }
        });
        
        for channel in channels {
            // Check if already subscribed
            let is_new = conn_info.channels.insert(channel.clone());
            
            // Add to channel subscribers
            if is_new {
                channel_subs
                    .entry(channel.clone())
                    .or_insert_with(HashSet::new)
                    .insert(connection_id);
            }
            
            let total_subs = conn_info.channels.len() + conn_info.patterns.len();
            
            results.push(SubResult {
                subscription: Subscription::Channel(channel),
                num_subscriptions: total_subs,
                is_new,
            });
        }
        
        Ok(results)
    }
    
    /// Unsubscribe a connection from one or more channels
    pub fn unsubscribe(&self, connection_id: u64, channels: Option<Vec<Vec<u8>>>) -> Result<Vec<SubResult>> {
        let mut results = Vec::new();
        
        let mut channel_subs = self.channels.lock().unwrap();
        let mut conn_subs = self.connections.lock().unwrap();
        
        let conn_info = match conn_subs.get_mut(&connection_id) {
            Some(info) => info,
            None => return Ok(results), // Connection has no subscriptions
        };
        
        // Determine which channels to unsubscribe from
        let channels_to_remove: Vec<Vec<u8>> = match channels {
            Some(chans) => chans,
            None => conn_info.channels.iter().cloned().collect(), // Unsubscribe from all
        };
        
        for channel in channels_to_remove {
            let was_subscribed = conn_info.channels.remove(&channel);
            
            if was_subscribed {
                // Remove from channel subscribers
                if let Some(subscribers) = channel_subs.get_mut(&channel) {
                    subscribers.remove(&connection_id);
                    if subscribers.is_empty() {
                        channel_subs.remove(&channel);
                    }
                }
            }
            
            let total_subs = conn_info.channels.len() + conn_info.patterns.len();
            
            results.push(SubResult {
                subscription: Subscription::Channel(channel),
                num_subscriptions: total_subs,
                is_new: false,
            });
        }
        
        // Clean up connection if no subscriptions remain
        if conn_info.channels.is_empty() && conn_info.patterns.is_empty() {
            conn_subs.remove(&connection_id);
        }
        
        Ok(results)
    }
    
    /// Subscribe a connection to one or more patterns
    pub fn psubscribe(&self, connection_id: u64, patterns: Vec<Vec<u8>>) -> Result<Vec<SubResult>> {
        let mut results = Vec::new();
        
        let mut pattern_subs = self.patterns.lock().unwrap();
        let mut conn_subs = self.connections.lock().unwrap();
        
        // Ensure connection info exists
        let conn_info = conn_subs.entry(connection_id).or_insert_with(|| {
            SubscriberInfo {
                connection_id,
                channels: HashSet::new(),
                patterns: HashSet::new(),
            }
        });
        
        for pattern in patterns {
            // Check if already subscribed
            let is_new = conn_info.patterns.insert(pattern.clone());
            
            // Add to pattern subscribers
            if is_new {
                pattern_subs
                    .entry(pattern.clone())
                    .or_insert_with(HashSet::new)
                    .insert(connection_id);
            }
            
            let total_subs = conn_info.channels.len() + conn_info.patterns.len();
            
            results.push(SubResult {
                subscription: Subscription::Pattern(pattern),
                num_subscriptions: total_subs,
                is_new,
            });
        }
        
        Ok(results)
    }
    
    /// Unsubscribe a connection from one or more patterns
    pub fn punsubscribe(&self, connection_id: u64, patterns: Option<Vec<Vec<u8>>>) -> Result<Vec<SubResult>> {
        let mut results = Vec::new();
        
        let mut pattern_subs = self.patterns.lock().unwrap();
        let mut conn_subs = self.connections.lock().unwrap();
        
        let conn_info = match conn_subs.get_mut(&connection_id) {
            Some(info) => info,
            None => return Ok(results), // Connection has no subscriptions
        };
        
        // Determine which patterns to unsubscribe from
        let patterns_to_remove: Vec<Vec<u8>> = match patterns {
            Some(pats) => pats,
            None => conn_info.patterns.iter().cloned().collect(), // Unsubscribe from all
        };
        
        for pattern in patterns_to_remove {
            let was_subscribed = conn_info.patterns.remove(&pattern);
            
            if was_subscribed {
                // Remove from pattern subscribers
                if let Some(subscribers) = pattern_subs.get_mut(&pattern) {
                    subscribers.remove(&connection_id);
                    if subscribers.is_empty() {
                        pattern_subs.remove(&pattern);
                    }
                }
            }
            
            let total_subs = conn_info.channels.len() + conn_info.patterns.len();
            
            results.push(SubResult {
                subscription: Subscription::Pattern(pattern),
                num_subscriptions: total_subs,
                is_new: false,
            });
        }
        
        // Clean up connection if no subscriptions remain
        if conn_info.channels.is_empty() && conn_info.patterns.is_empty() {
            conn_subs.remove(&connection_id);
        }
        
        Ok(results)
    }
    
    /// Publish a message to a channel
    /// Returns list of (connection_id, matching_pattern) for all subscribers
    pub fn publish(&self, channel: &[u8], _message: &[u8]) -> Result<Vec<(u64, Option<Vec<u8>>)>> {
        let mut receivers = Vec::new();
        let mut seen_connections = HashSet::new();
        
        // Find direct channel subscribers
        {
            let channel_subs = self.channels.lock().unwrap();
            if let Some(subscribers) = channel_subs.get(channel) {
                for &conn_id in subscribers {
                    if seen_connections.insert(conn_id) {
                        receivers.push((conn_id, None));
                    }
                }
            }
        }
        
        // Find pattern subscribers
        {
            let pattern_subs = self.patterns.lock().unwrap();
            for (pattern, subscribers) in pattern_subs.iter() {
                if pattern_matches(pattern, channel) {
                    for &conn_id in subscribers {
                        if seen_connections.insert(conn_id) {
                            receivers.push((conn_id, Some(pattern.clone())));
                        }
                    }
                }
            }
        }
        
        Ok(receivers)
    }
    
    /// Remove all subscriptions for a connection
    pub fn unsubscribe_all(&self, connection_id: u64) -> Result<()> {
        // Remove from channel subscriptions
        {
            let mut channel_subs = self.channels.lock().unwrap();
            let mut to_remove = Vec::new();
            
            for (channel, subscribers) in channel_subs.iter_mut() {
                subscribers.remove(&connection_id);
                if subscribers.is_empty() {
                    to_remove.push(channel.clone());
                }
            }
            
            for channel in to_remove {
                channel_subs.remove(&channel);
            }
        }
        
        // Remove from pattern subscriptions
        {
            let mut pattern_subs = self.patterns.lock().unwrap();
            let mut to_remove = Vec::new();
            
            for (pattern, subscribers) in pattern_subs.iter_mut() {
                subscribers.remove(&connection_id);
                if subscribers.is_empty() {
                    to_remove.push(pattern.clone());
                }
            }
            
            for pattern in to_remove {
                pattern_subs.remove(&pattern);
            }
        }
        
        // Remove connection info
        {
            let mut conn_subs = self.connections.lock().unwrap();
            conn_subs.remove(&connection_id);
        }
        
        Ok(())
    }
    
    /// Get subscription info for a connection
    pub fn get_subscription_info(&self, connection_id: u64) -> Option<SubscriberInfo> {
        let conn_subs = self.connections.lock().unwrap();
        conn_subs.get(&connection_id).map(|info| {
            SubscriberInfo {
                connection_id: info.connection_id,
                channels: info.channels.clone(),
                patterns: info.patterns.clone(),
            }
        })
    }
    
    /// Check if a connection has any subscriptions
    pub fn is_subscribed(&self, connection_id: u64) -> bool {
        let conn_subs = self.connections.lock().unwrap();
        conn_subs.contains_key(&connection_id)
    }
    
    /// Get the number of subscribers for a specific channel
    pub fn channel_subscriber_count(&self, channel: &[u8]) -> usize {
        let channel_subs = self.channels.lock().unwrap();
        channel_subs.get(channel).map(|s| s.len()).unwrap_or(0)
    }
}

/// Check if a pattern matches a channel name
/// Supports glob-style patterns with * and ?
pub fn pattern_matches(pattern: &[u8], channel: &[u8]) -> bool {
    let mut p_idx = 0;
    let mut c_idx = 0;
    let mut star_idx = None;
    let mut star_match_idx = 0;
    
    while c_idx < channel.len() {
        if p_idx < pattern.len() {
            match pattern[p_idx] {
                b'?' => {
                    // ? matches any single character
                    p_idx += 1;
                    c_idx += 1;
                    continue;
                }
                b'*' => {
                    // * matches zero or more characters
                    star_idx = Some(p_idx);
                    star_match_idx = c_idx;
                    p_idx += 1;
                    continue;
                }
                b'\\' if p_idx + 1 < pattern.len() => {
                    // Escaped character
                    if pattern[p_idx + 1] == channel[c_idx] {
                        p_idx += 2;
                        c_idx += 1;
                        continue;
                    }
                }
                _ => {
                    // Regular character match
                    if pattern[p_idx] == channel[c_idx] {
                        p_idx += 1;
                        c_idx += 1;
                        continue;
                    }
                }
            }
        }
        
        // No match, try to backtrack to last *
        if let Some(star_pos) = star_idx {
            p_idx = star_pos + 1;
            star_match_idx += 1;
            c_idx = star_match_idx;
        } else {
            return false;
        }
    }
    
    // Skip trailing * in pattern
    while p_idx < pattern.len() && pattern[p_idx] == b'*' {
        p_idx += 1;
    }
    
    p_idx == pattern.len()
}

/// Format a pub/sub message frame
pub fn format_message(channel: &[u8], message: &[u8]) -> RespFrame {
    RespFrame::Array(Some(vec![
        RespFrame::from_string("message"),
        RespFrame::from_bytes(channel.to_vec()),
        RespFrame::from_bytes(message.to_vec()),
    ]))
}

/// Format a pub/sub pmessage frame (for pattern subscriptions)
pub fn format_pmessage(pattern: &[u8], channel: &[u8], message: &[u8]) -> RespFrame {
    RespFrame::Array(Some(vec![
        RespFrame::from_string("pmessage"),
        RespFrame::from_bytes(pattern.to_vec()),
        RespFrame::from_bytes(channel.to_vec()),
        RespFrame::from_bytes(message.to_vec()),
    ]))
}

/// Format a subscribe confirmation
pub fn format_subscribe_response(channel: &[u8], num_subs: usize) -> RespFrame {
    RespFrame::Array(Some(vec![
        RespFrame::from_string("subscribe"),
        RespFrame::from_bytes(channel.to_vec()),
        RespFrame::Integer(num_subs as i64),
    ]))
}

/// Format a psubscribe confirmation
pub fn format_psubscribe_response(pattern: &[u8], num_subs: usize) -> RespFrame {
    RespFrame::Array(Some(vec![
        RespFrame::from_string("psubscribe"),
        RespFrame::from_bytes(pattern.to_vec()),
        RespFrame::Integer(num_subs as i64),
    ]))
}

/// Format an unsubscribe confirmation
pub fn format_unsubscribe_response(channel: &[u8], num_subs: usize) -> RespFrame {
    RespFrame::Array(Some(vec![
        RespFrame::from_string("unsubscribe"),
        RespFrame::from_bytes(channel.to_vec()),
        RespFrame::Integer(num_subs as i64),
    ]))
}

/// Format a punsubscribe confirmation
pub fn format_punsubscribe_response(pattern: &[u8], num_subs: usize) -> RespFrame {
    RespFrame::Array(Some(vec![
        RespFrame::from_string("punsubscribe"),
        RespFrame::from_bytes(pattern.to_vec()),
        RespFrame::Integer(num_subs as i64),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pattern_matching() {
        // Exact match
        assert!(pattern_matches(b"hello", b"hello"));
        assert!(!pattern_matches(b"hello", b"world"));
        
        // ? wildcard
        assert!(pattern_matches(b"h?llo", b"hello"));
        assert!(pattern_matches(b"h?llo", b"hallo"));
        assert!(!pattern_matches(b"h?llo", b"hllo"));
        
        // * wildcard
        assert!(pattern_matches(b"h*", b"hello"));
        assert!(pattern_matches(b"h*", b"h"));
        assert!(pattern_matches(b"*llo", b"hello"));
        assert!(pattern_matches(b"*llo", b"llo"));
        assert!(pattern_matches(b"h*o", b"hello"));
        assert!(pattern_matches(b"*", b"anything"));
        
        // Complex patterns
        assert!(pattern_matches(b"h*l?o", b"hello"));
        assert!(pattern_matches(b"news.*", b"news.sports"));
        assert!(pattern_matches(b"news.*", b"news.weather"));
        assert!(!pattern_matches(b"news.*", b"news"));
        
        // Edge cases
        assert!(pattern_matches(b"", b""));
        assert!(!pattern_matches(b"", b"a"));
        assert!(pattern_matches(b"*", b""));
    }
    
    #[test]
    fn test_pubsub_subscribe() {
        let pubsub = PubSubManager::new();
        
        // Subscribe to channels
        let results = pubsub.subscribe(1, vec![b"channel1".to_vec(), b"channel2".to_vec()]).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].num_subscriptions, 1);
        assert_eq!(results[1].num_subscriptions, 2);
        assert!(results[0].is_new);
        assert!(results[1].is_new);
        
        // Subscribe again (should not be new)
        let results = pubsub.subscribe(1, vec![b"channel1".to_vec()]).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].num_subscriptions, 2);
        assert!(!results[0].is_new);
    }
    
    #[test]
    fn test_pubsub_publish() {
        let pubsub = PubSubManager::new();
        
        // Subscribe connections
        pubsub.subscribe(1, vec![b"news".to_vec()]).unwrap();
        pubsub.subscribe(2, vec![b"news".to_vec()]).unwrap();
        pubsub.psubscribe(3, vec![b"news*".to_vec()]).unwrap();
        
        // Publish message
        let receivers = pubsub.publish(b"news", b"hello").unwrap();
        assert_eq!(receivers.len(), 3);
        
        // Publish to pattern-matched channel
        let receivers = pubsub.publish(b"news.sports", b"goal!").unwrap();
        assert_eq!(receivers.len(), 1); // Only pattern subscriber
        assert_eq!(receivers[0].0, 3);
        assert!(receivers[0].1.is_some());
    }
}