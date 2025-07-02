//! Generational Arena Implementation
//! 
//! This module provides a type-safe, generational arena for managing
//! heap-allocated objects with handle-based access.

use std::marker::PhantomData;
use super::error::{LuaError, LuaResult};

/// Entry in the arena that can be either occupied or free
#[derive(Debug)]
enum Entry<T> {
    /// An occupied slot containing a value and its generation
    Occupied { value: T, generation: u32 },
    
    /// A free slot with a link to the next free slot
    Free { next_free: Option<usize> },
}

/// A generational arena for storing values of type T
pub struct Arena<T> {
    /// Storage for all entries
    entries: Vec<Entry<T>>,
    
    /// Head of the free list
    free_head: Option<usize>,
    
    /// Current generation counter
    generation: u32,
}

/// A handle to a value in the arena
pub struct Handle<T> {
    /// Index in the arena
    pub(crate) index: u32,
    
    /// Generation number for validation
    pub(crate) generation: u32,
    
    /// Type marker
    _phantom: PhantomData<T>,
}

impl<T> Copy for Handle<T> {}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && self.generation == other.generation
    }
}

impl<T> Eq for Handle<T> {}

impl<T> std::hash::Hash for Handle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
        self.generation.hash(state);
    }
}

impl<T> std::fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handle")
            .field("index", &self.index)
            .field("generation", &self.generation)
            .finish()
    }
}

impl<T> Handle<T> {
    /// Create a new handle with specific index and generation
    /// This is primarily for internal use by the transaction system
    pub(crate) fn from_raw_parts(index: u32, generation: u32) -> Self {
        Handle {
            index,
            generation,
            _phantom: PhantomData,
        }
    }
}

impl<T> Arena<T> {
    /// Create a new empty arena
    pub fn new() -> Self {
        Arena {
            entries: Vec::new(),
            free_head: None,
            generation: 0,
        }
    }
    
    /// Create a new arena with capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Arena {
            entries: Vec::with_capacity(capacity),
            free_head: None,
            generation: 0,
        }
    }
    
    /// Insert a value into the arena and return a handle to it
    pub fn insert(&mut self, value: T) -> Handle<T> {
        // Increment generation for each insert
        self.generation = self.generation.wrapping_add(1);
        
        let index = if let Some(free_index) = self.free_head {
            // Reuse a free slot
            let next_free = match &self.entries[free_index] {
                Entry::Free { next_free } => *next_free,
                _ => unreachable!("Free list corruption"),
            };
            
            self.free_head = next_free;
            self.entries[free_index] = Entry::Occupied {
                value,
                generation: self.generation,
            };
            
            free_index
        } else {
            // Allocate a new slot
            let index = self.entries.len();
            self.entries.push(Entry::Occupied {
                value,
                generation: self.generation,
            });
            
            index
        };
        
        Handle {
            index: index as u32,
            generation: self.generation,
            _phantom: PhantomData,
        }
    }
    
    /// Get a reference to a value by handle
    pub fn get(&self, handle: Handle<T>) -> Option<&T> {
        let index = handle.index as usize;
        
        if index >= self.entries.len() {
            return None;
        }
        
        match &self.entries[index] {
            Entry::Occupied { value, generation } if *generation == handle.generation => {
                Some(value)
            }
            _ => None,
        }
    }
    
    /// Get a mutable reference to a value by handle
    pub fn get_mut(&mut self, handle: Handle<T>) -> Option<&mut T> {
        let index = handle.index as usize;
        
        if index >= self.entries.len() {
            return None;
        }
        
        match &mut self.entries[index] {
            Entry::Occupied { value, generation } if *generation == handle.generation => {
                Some(value)
            }
            _ => None,
        }
    }
    
    /// Remove a value from the arena and return it
    pub fn remove(&mut self, handle: Handle<T>) -> Option<T> {
        let index = handle.index as usize;
        
        if index >= self.entries.len() {
            return None;
        }
        
        match &self.entries[index] {
            Entry::Occupied { generation, .. } if *generation == handle.generation => {
                // Replace with free entry
                let old_entry = std::mem::replace(
                    &mut self.entries[index],
                    Entry::Free { next_free: self.free_head }
                );
                
                // Update free list head
                self.free_head = Some(index);
                
                // Extract value
                match old_entry {
                    Entry::Occupied { value, .. } => Some(value),
                    _ => unreachable!(),
                }
            }
            _ => None,
        }
    }
    
    /// Check if a handle is valid
    pub fn contains(&self, handle: Handle<T>) -> bool {
        self.get(handle).is_some()
    }
    
    /// Get the number of occupied entries
    pub fn len(&self) -> usize {
        self.entries.iter().filter(|e| matches!(e, Entry::Occupied { .. })).count()
    }
    
    /// Check if the arena is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    
    /// Clear all entries from the arena
    pub fn clear(&mut self) {
        self.entries.clear();
        self.free_head = None;
        // Don't reset generation to ensure old handles remain invalid
    }
    
    /// Get the current capacity of the arena
    pub fn capacity(&self) -> usize {
        self.entries.capacity()
    }
    
    /// Reserve capacity for at least `additional` more elements
    pub fn reserve(&mut self, additional: usize) {
        self.entries.reserve(additional);
    }
    
    /// Check if adding a new element might cause reallocation
    /// 
    /// This is used to determine if we need to validate handles before
    /// performing an operation that might cause reallocation, which could
    /// invalidate existing handles if they're not properly validated.
    pub fn might_reallocate_on_insert(&self) -> bool {
        // We need reallocation if we have no free slots AND we're at capacity
        self.free_head.is_none() && self.entries.len() == self.entries.capacity()
    }
    
    /// Validate a handle against this arena
    pub fn validate_handle(&self, handle: &Handle<T>) -> LuaResult<()> {
        let index = handle.index as usize;
        
        if index >= self.entries.len() {
            return Err(LuaError::InvalidHandle);
        }
        
        match &self.entries[index] {
            Entry::Occupied { generation, .. } if *generation == handle.generation => Ok(()),
            Entry::Occupied { .. } => Err(LuaError::StaleHandle),
            Entry::Free { .. } => Err(LuaError::InvalidHandle),
        }
    }
    
    /// Get the generation of a specific index (for validation)
    pub(crate) fn get_generation(&self, index: u32) -> Option<u32> {
        let index = index as usize;
        
        if index >= self.entries.len() {
            return None;
        }
        
        match &self.entries[index] {
            Entry::Occupied { generation, .. } => Some(*generation),
            Entry::Free { .. } => None,
        }
    }
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_arena_insert_and_get() {
        let mut arena = Arena::new();
        
        // Insert a value
        let handle = arena.insert(42);
        
        // Get the value back
        assert_eq!(arena.get(handle), Some(&42));
        
        // Mutable access
        if let Some(value) = arena.get_mut(handle) {
            *value = 84;
        }
        
        assert_eq!(arena.get(handle), Some(&84));
    }
    
    #[test]
    fn test_arena_remove() {
        let mut arena = Arena::new();
        
        // Insert and remove
        let handle = arena.insert("hello");
        assert_eq!(arena.remove(handle), Some("hello"));
        
        // Handle should be invalid after removal
        assert_eq!(arena.get(handle), None);
    }
    
    #[test]
    fn test_arena_generation_validation() {
        let mut arena = Arena::new();
        
        // Insert, remove, and insert again
        let handle1 = arena.insert(1);
        arena.remove(handle1);
        let handle2 = arena.insert(2);
        
        // Old handle should be invalid
        assert_eq!(arena.get(handle1), None);
        
        // New handle should be valid
        assert_eq!(arena.get(handle2), Some(&2));
        
        // Generations should differ
        assert_ne!(handle1.generation, handle2.generation);
    }
    
    #[test]
    fn test_arena_free_list_reuse() {
        let mut arena = Arena::new();
        
        // Insert multiple values
        let h1 = arena.insert(1);
        let h2 = arena.insert(2);
        let h3 = arena.insert(3);
        
        // Remove middle value
        arena.remove(h2);
        
        // New insert should reuse the slot
        let h4 = arena.insert(4);
        assert_eq!(h4.index, h2.index);
        assert_ne!(h4.generation, h2.generation);
        
        // Verify all values
        assert_eq!(arena.get(h1), Some(&1));
        assert_eq!(arena.get(h2), None);
        assert_eq!(arena.get(h3), Some(&3));
        assert_eq!(arena.get(h4), Some(&4));
    }
    
    #[test]
    fn test_arena_validate_handle() {
        let mut arena = Arena::new();
        
        let handle = arena.insert(42);
        
        // Valid handle
        assert!(arena.validate_handle(&handle).is_ok());
        
        // Remove and check validation
        arena.remove(handle);
        assert!(matches!(
            arena.validate_handle(&handle),
            Err(LuaError::InvalidHandle)
        ));
    }
}