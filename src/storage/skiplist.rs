//! Skip list implementation for sorted sets
//! 
//! Provides a probabilistic data structure with O(log n) operations
//! for maintaining scored members in sorted order.

use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::cmp::{Ordering, min};
use std::fmt::{self, Debug};
use rand::Rng;

/// Maximum number of levels in the skip list
const MAX_LEVEL: usize = 32;

/// Probability of promoting a node to the next level
const PROBABILITY: f64 = 0.5;

/// A node in the skip list
struct SkipListNode<K, V> {
    /// The key (member in Redis terms)
    key: K,
    /// The value (score in Redis terms)
    value: V,
    /// Forward pointers at each level
    forward: Vec<Option<*mut SkipListNode<K, V>>>,
}

/// Thread-safe skip list implementation
pub struct SkipList<K, V> {
    /// Inner data protected by RwLock
    inner: Arc<RwLock<SkipListInner<K, V>>>,
    /// Random number generator for level generation
    rng: Arc<RwLock<rand::rngs::ThreadRng>>,
}

/// Inner skip list data
struct SkipListInner<K, V> {
    /// Sentinel head node
    head: *mut SkipListNode<K, V>,
    /// Current maximum level
    level: usize,
    /// Number of elements
    length: usize,
    /// Memory usage in bytes
    memory_usage: usize,
}

/// Result of a range query
#[derive(Debug, Clone)]
pub struct RangeResult<K, V> {
    pub items: Vec<(K, V)>,
}

// Safety: We ensure thread safety through RwLock
unsafe impl<K: Send, V: Send> Send for SkipListInner<K, V> {}
unsafe impl<K: Send, V: Send> Sync for SkipListInner<K, V> {}

impl<K, V> SkipList<K, V>
where
    K: Clone + Ord + Debug,
    V: Clone + PartialOrd + Debug,
{
    /// Create a new empty skip list
    pub fn new() -> Self 
    where
        K: Default,
        V: Default,
    {
        let head = Box::into_raw(Box::new(SkipListNode {
            key: Default::default(),    // Sentinel value, never accessed
            value: Default::default(),  // Sentinel value, never accessed
            forward: vec![None; MAX_LEVEL],
        }));

        SkipList {
            inner: Arc::new(RwLock::new(SkipListInner {
                head,
                level: 0,
                length: 0,
                memory_usage: std::mem::size_of::<SkipListNode<K, V>>() + MAX_LEVEL * std::mem::size_of::<Option<*mut SkipListNode<K, V>>>(),
            })),
            rng: Arc::new(RwLock::new(rand::thread_rng())),
        }
    }

    /// Insert or update a key-value pair
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        let mut inner = self.inner.write().unwrap();
        let mut update = vec![std::ptr::null_mut(); MAX_LEVEL];
        
        // Find position and collect update pointers
        let mut current = inner.head;
        for i in (0..=inner.level).rev() {
            unsafe {
                while let Some(next) = (*current).forward[i] {
                    match self.compare_nodes(&(*next).value, &(*next).key, &value, &key) {
                        Ordering::Less => current = next,
                        _ => break,
                    }
                }
                update[i] = current;
            }
        }

        // Check if key already exists
        unsafe {
            current = if let Some(next) = (*current).forward[0] {
                next
            } else {
                std::ptr::null_mut()
            };

            if !current.is_null() && (*current).key == key {
                // Update existing value
                let old_value = (*current).value.clone();
                (*current).value = value;
                return Some(old_value);
            }
        }

        // Generate random level for new node
        let new_level = self.random_level();
        
        // Update list level if necessary
        if new_level > inner.level {
            for i in (inner.level + 1)..=new_level {
                update[i] = inner.head;
            }
            inner.level = new_level;
        }

        // Create new node
        let new_node = Box::into_raw(Box::new(SkipListNode {
            key,
            value,
            forward: vec![None; new_level + 1],
        }));

        // Insert node at each level
        unsafe {
            for i in 0..=new_level {
                (*new_node).forward[i] = (*update[i]).forward[i];
                (*update[i]).forward[i] = Some(new_node);
            }
        }

        inner.length += 1;
        inner.memory_usage += self.calculate_node_size(new_level + 1);
        
        None
    }

    /// Remove a key from the skip list
    pub fn remove<Q>(&self, key: &Q) -> Option<V> 
    where 
        K: std::borrow::Borrow<Q>,
        Q: Ord + ?Sized,
    {
        let mut inner = self.inner.write().unwrap();
        let mut update = vec![std::ptr::null_mut(); MAX_LEVEL];
        
        // Find node and collect update pointers
        let mut current = inner.head;
        unsafe {
            for i in (0..=inner.level).rev() {
                while let Some(next) = (*current).forward[i] {
                    match (*next).key.borrow().cmp(key) {
                        Ordering::Less => current = next,
                        _ => break,
                    }
                }
                update[i] = current;
            }

            // Check if we found the key
            if let Some(target) = (*current).forward[0] {
                if (*target).key.borrow().cmp(key) == Ordering::Equal {
                    // Remove from each level
                    for i in 0..=inner.level {
                        if let Some(next) = (*update[i]).forward[i] {
                            if next == target {
                                (*update[i]).forward[i] = (*target).forward[i];
                            } else {
                                break;
                            }
                        }
                    }

                    // Update list level
                    while inner.level > 0 && (*inner.head).forward[inner.level].is_none() {
                        inner.level -= 1;
                    }

                    let removed_value = (*target).value.clone();
                    let node_levels = (*target).forward.len();
                    
                    // Deallocate node
                    Box::from_raw(target);
                    
                    inner.length -= 1;
                    inner.memory_usage -= self.calculate_node_size(node_levels);
                    
                    return Some(removed_value);
                }
            }
        }
        
        None
    }

    /// Get the score (value) for a key
    pub fn get_score<Q>(&self, key: &Q) -> Option<V> 
    where 
        K: std::borrow::Borrow<Q>,
        Q: Ord + ?Sized,
    {
        let inner = self.inner.read().unwrap();
        self.find_node(&inner, key).map(|node| unsafe { (*node).value.clone() })
    }

    /// Get the rank (0-based position) of a key
    pub fn get_rank<Q>(&self, key: &Q) -> Option<usize> 
    where 
        K: std::borrow::Borrow<Q>,
        Q: Ord + ?Sized,
    {
        let inner = self.inner.read().unwrap();
        let mut rank = 0;
        let mut current = inner.head;
        
        unsafe {
            for i in (0..=inner.level).rev() {
                while let Some(next) = (*current).forward[i] {
                    if (*next).key.borrow().cmp(key) == Ordering::Less {
                        // Count nodes we're skipping at level 0
                        if i == 0 {
                            rank += 1;
                        } else {
                            // Count nodes between current and next at level 0
                            rank += self.count_nodes_between(current, next);
                        }
                        current = next;
                    } else {
                        break;
                    }
                }
            }

            // Check if we found the key
            if let Some(next) = (*current).forward[0] {
                if (*next).key.borrow().cmp(key) == Ordering::Equal {
                    return Some(rank);
                }
            }
        }
        
        None
    }

    /// Get element by rank (0-based)
    pub fn get_by_rank(&self, rank: usize) -> Option<(K, V)> {
        let inner = self.inner.read().unwrap();
        
        if rank >= inner.length {
            return None;
        }

        let mut current = inner.head;
        let mut traversed = 0;
        
        unsafe {
            // Skip to the target rank
            while traversed <= rank {
                if let Some(next) = (*current).forward[0] {
                    if traversed == rank {
                        return Some(((*next).key.clone(), (*next).value.clone()));
                    }
                    current = next;
                    traversed += 1;
                } else {
                    break;
                }
            }
        }
        
        None
    }

    /// Get a range of elements by rank (inclusive)
    pub fn range_by_rank(&self, start_rank: usize, end_rank: usize) -> RangeResult<K, V> {
        let inner = self.inner.read().unwrap();
        let mut items = Vec::new();
        
        if start_rank >= inner.length {
            return RangeResult { items };
        }

        let mut current = inner.head;
        let mut rank = 0;
        
        unsafe {
            // Skip to start rank
            while rank < start_rank {
                if let Some(next) = (*current).forward[0] {
                    current = next;
                    rank += 1;
                } else {
                    break;
                }
            }

            // Collect elements in range
            while rank <= end_rank && rank < inner.length {
                if let Some(next) = (*current).forward[0] {
                    items.push(((*next).key.clone(), (*next).value.clone()));
                    current = next;
                    rank += 1;
                } else {
                    break;
                }
            }
        }
        
        RangeResult { items }
    }

    /// Get a range of elements by score (inclusive)
    pub fn range_by_score(&self, min_score: V, max_score: V) -> RangeResult<K, V> {
        let inner = self.inner.read().unwrap();
        let mut items = Vec::new();
        let mut current = inner.head;
        
        unsafe {
            // Skip to first element >= min_score
            for i in (0..=inner.level).rev() {
                while let Some(next) = (*current).forward[i] {
                    if (*next).value < min_score {
                        current = next;
                    } else {
                        break;
                    }
                }
            }

            // Collect elements in score range
            if let Some(next) = (*current).forward[0] {
                current = next;
            } else {
                return RangeResult { items };
            }

            while !current.is_null() && (*current).value <= max_score {
                items.push(((*current).key.clone(), (*current).value.clone()));
                
                // Advance to next
                if let Some(next) = (*current).forward[0] {
                    current = next;
                } else {
                    break;
                }
            }
        }
        
        RangeResult { items }
    }

    /// Get the number of elements
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

    /// Clear all elements
    pub fn clear(&self) {
        let mut inner = self.inner.write().unwrap();
        
        // Deallocate all nodes
        unsafe {
            let mut current = inner.head;
            while let Some(next) = (*current).forward[0] {
                (*current).forward[0] = (*next).forward[0];
                Box::from_raw(next);
            }
        }

        // Reset state
        inner.level = 0;
        inner.length = 0;
        inner.memory_usage = std::mem::size_of::<SkipListNode<K, V>>() + MAX_LEVEL * std::mem::size_of::<Option<*mut SkipListNode<K, V>>>();
        
        // Clear forward pointers in head
        unsafe {
            for i in 0..MAX_LEVEL {
                (*inner.head).forward[i] = None;
            }
        }
    }

    /// Get all items in the skip list
    pub fn get_all_items(&self) -> Vec<(K, V)> {
        let inner = self.inner.read().unwrap();
        let mut items = Vec::with_capacity(inner.length);
        
        let mut current = inner.head;
        
        unsafe {
            // Start with the first element
            if let Some(next) = (*current).forward[0] {
                current = next;
            } else {
                return items; // Empty list
            }
            
            // Iterate through all elements
            while !current.is_null() {
                items.push(((*current).key.clone(), (*current).value.clone()));
                
                current = match (*current).forward[0] {
                    Some(next) => next,
                    None => break,
                };
            }
        }
        
        items
    }

    // Helper methods

    /// Compare nodes by (value, key) for sorted set ordering
    fn compare_nodes(&self, v1: &V, k1: &K, v2: &V, k2: &K) -> Ordering {
        match v1.partial_cmp(v2) {
            Some(Ordering::Equal) => k1.cmp(k2),
            Some(ord) => ord,
            None => {
                // Handle NaN by treating it as greater than any other value
                if self.is_nan(v1) && self.is_nan(v2) {
                    k1.cmp(k2)
                } else if self.is_nan(v1) {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            }
        }
    }

    /// Check if value is NaN (for f64)
    fn is_nan(&self, v: &V) -> bool {
        v.partial_cmp(v).is_none()
    }

    /// Find a node by key
    fn find_node<Q>(&self, inner: &RwLockReadGuard<SkipListInner<K, V>>, key: &Q) -> Option<*mut SkipListNode<K, V>> 
    where 
        K: std::borrow::Borrow<Q>,
        Q: Ord + ?Sized,
    {
        let mut current = inner.head;
        
        unsafe {
            for i in (0..=inner.level).rev() {
                while let Some(next) = (*current).forward[i] {
                    match (*next).key.borrow().cmp(key) {
                        Ordering::Less => current = next,
                        Ordering::Equal => return Some(next),
                        Ordering::Greater => break,
                    }
                }
            }

            if let Some(next) = (*current).forward[0] {
                if (*next).key.borrow().cmp(key) == Ordering::Equal {
                    return Some(next);
                }
            }
        }
        
        None
    }

    /// Count nodes between two pointers at level 0
    fn count_nodes_between(&self, start: *mut SkipListNode<K, V>, end: *mut SkipListNode<K, V>) -> usize {
        let mut count = 0;
        let mut current = start;
        
        unsafe {
            while let Some(next) = (*current).forward[0] {
                if next == end {
                    break;
                }
                count += 1;
                current = next;
            }
        }
        
        count
    }

    /// Generate random level for new node
    fn random_level(&self) -> usize {
        let mut level = 0;
        let mut rng = self.rng.write().unwrap();
        
        while level < MAX_LEVEL - 1 && rng.gen::<f64>() < PROBABILITY {
            level += 1;
        }
        
        level
    }

    /// Calculate memory size for a node with given number of levels
    fn calculate_node_size(&self, levels: usize) -> usize {
        std::mem::size_of::<SkipListNode<K, V>>() + 
        levels * std::mem::size_of::<Option<*mut SkipListNode<K, V>>>()
    }
}

impl<K, V> Drop for SkipList<K, V> {
    fn drop(&mut self) {
        if let Ok(inner) = Arc::try_unwrap(self.inner.clone()) {
            let inner = inner.into_inner().unwrap();
            
            // Deallocate all nodes
            unsafe {
                let mut current = inner.head;
                while let Some(next) = (*current).forward[0] {
                    (*current).forward[0] = (*next).forward[0];
                    Box::from_raw(next);
                }
                
                // Deallocate head
                Box::from_raw(inner.head);
            }
        }
    }
}

impl<K: Clone + Ord + Debug + Default, V: Clone + PartialOrd + Debug + Default> Default for SkipList<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Clone + Ord + Debug, V: Clone + PartialOrd + Debug> Debug for SkipList<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.inner.read().unwrap();
        write!(f, "SkipList {{ len: {}, level: {} }}", inner.length, inner.level)
    }
}

// Make SkipList thread-safe
unsafe impl<K: Send, V: Send> Send for SkipList<K, V> {}
unsafe impl<K: Send, V: Send> Sync for SkipList<K, V> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let list: SkipList<Vec<u8>, f64> = SkipList::new();
        
        // Insert some values
        assert_eq!(list.insert(b"apple".to_vec(), 3.0), None);
        assert_eq!(list.insert(b"banana".to_vec(), 1.0), None);
        assert_eq!(list.insert(b"cherry".to_vec(), 2.0), None);
        
        // Test length
        assert_eq!(list.len(), 3);
        
        // Test get_score
        assert_eq!(list.get_score(&b"banana".to_vec()), Some(1.0));
        assert_eq!(list.get_score(&b"cherry".to_vec()), Some(2.0));
        assert_eq!(list.get_score(&b"apple".to_vec()), Some(3.0));
        assert_eq!(list.get_score(&b"durian".to_vec()), None);
        
        // Update existing
        assert_eq!(list.insert(b"banana".to_vec(), 1.5), Some(1.0));
        assert_eq!(list.get_score(&b"banana".to_vec()), Some(1.5));
        assert_eq!(list.len(), 3); // Length shouldn't change
    }

    #[test]
    fn test_ranking() {
        let list: SkipList<Vec<u8>, f64> = SkipList::new();
        
        list.insert(b"a".to_vec(), 1.0);
        list.insert(b"b".to_vec(), 2.0);
        list.insert(b"c".to_vec(), 3.0);
        list.insert(b"d".to_vec(), 4.0);
        
        // Test get_rank
        assert_eq!(list.get_rank(&b"a".to_vec()), Some(0));
        assert_eq!(list.get_rank(&b"b".to_vec()), Some(1));
        assert_eq!(list.get_rank(&b"c".to_vec()), Some(2));
        assert_eq!(list.get_rank(&b"d".to_vec()), Some(3));
        assert_eq!(list.get_rank(&b"e".to_vec()), None);
        
        // Test get_by_rank
        assert_eq!(list.get_by_rank(0), Some((b"a".to_vec(), 1.0)));
        assert_eq!(list.get_by_rank(1), Some((b"b".to_vec(), 2.0)));
        assert_eq!(list.get_by_rank(2), Some((b"c".to_vec(), 3.0)));
        assert_eq!(list.get_by_rank(3), Some((b"d".to_vec(), 4.0)));
        assert_eq!(list.get_by_rank(4), None);
    }

    #[test]
    fn test_range_queries() {
        let list: SkipList<Vec<u8>, f64> = SkipList::new();
        
        list.insert(b"a".to_vec(), 1.0);
        list.insert(b"b".to_vec(), 2.0);
        list.insert(b"c".to_vec(), 3.0);
        list.insert(b"d".to_vec(), 4.0);
        list.insert(b"e".to_vec(), 5.0);
        
        // Test range_by_rank
        let range = list.range_by_rank(1, 3);
        assert_eq!(range.items.len(), 3);
        assert_eq!(range.items[0], (b"b".to_vec(), 2.0));
        assert_eq!(range.items[1], (b"c".to_vec(), 3.0));
        assert_eq!(range.items[2], (b"d".to_vec(), 4.0));
        
        // Test range_by_score
        let range = list.range_by_score(2.0, 4.0);
        assert_eq!(range.items.len(), 3);
        assert_eq!(range.items[0], (b"b".to_vec(), 2.0));
        assert_eq!(range.items[1], (b"c".to_vec(), 3.0));
        assert_eq!(range.items[2], (b"d".to_vec(), 4.0));
    }

    #[test]
    fn test_removal() {
        let list: SkipList<Vec<u8>, f64> = SkipList::new();
        
        list.insert(b"a".to_vec(), 1.0);
        list.insert(b"b".to_vec(), 2.0);
        list.insert(b"c".to_vec(), 3.0);
        
        // Remove middle element
        assert_eq!(list.remove(&b"b".to_vec()), Some(2.0));
        assert_eq!(list.len(), 2);
        assert_eq!(list.get_score(&b"b".to_vec()), None);
        
        // Check ranks updated correctly
        assert_eq!(list.get_rank(&b"a".to_vec()), Some(0));
        assert_eq!(list.get_rank(&b"c".to_vec()), Some(1));
        
        // Remove non-existent
        assert_eq!(list.remove(&b"d".to_vec()), None);
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_same_scores() {
        let list: SkipList<Vec<u8>, f64> = SkipList::new();
        
        // Elements with same score should be ordered by key
        list.insert(b"c".to_vec(), 1.0);
        list.insert(b"a".to_vec(), 1.0);
        list.insert(b"b".to_vec(), 1.0);
        
        assert_eq!(list.get_by_rank(0), Some((b"a".to_vec(), 1.0)));
        assert_eq!(list.get_by_rank(1), Some((b"b".to_vec(), 1.0)));
        assert_eq!(list.get_by_rank(2), Some((b"c".to_vec(), 1.0)));
    }
}