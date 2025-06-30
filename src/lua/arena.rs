//! Generational Arena Implementation
//!
//! This module provides a type-safe, handle-based memory management system
//! that avoids the common issues with raw pointers while providing efficient
//! access to dynamic objects.

use std::marker::PhantomData;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::collections::HashMap;

use super::error::{LuaError, Result};

/// A handle to an object in an arena
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Handle<T> {
    /// Index in the arena
    pub index: u32,
    
    /// Generation - incremented when an object is freed
    pub generation: u32,
    
    /// Phantom data for type safety
    pub _phantom: PhantomData<T>,
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Handle {
            index: self.index,
            generation: self.generation,
            _phantom: PhantomData,
        }
    }
}

impl<T> Copy for Handle<T> {}

impl<T> Handle<T> {
    /// Create a new handle
    pub fn new(index: u32, generation: u32) -> Self {
        Handle {
            index,
            generation,
            _phantom: PhantomData,
        }
    }
}

impl<T> Default for Handle<T> {
    fn default() -> Self {
        Handle {
            index: 0,
            generation: 0,
            _phantom: PhantomData,
        }
    }
}

/// An entry in the arena
enum Entry<T> {
    /// An occupied slot
    Occupied {
        /// The value
        value: T,
        
        /// Generation
        generation: u32,
    },
    
    /// A free slot
    Free {
        /// Next free index
        next_free: Option<usize>,
    },
}

/// A generational arena
pub struct Arena<T> {
    /// The entries in the arena
    entries: Vec<Entry<T>>,
    
    /// Head of the free list
    free_head: Option<usize>,
    
    /// Current generation
    generation: u32,
}

impl<T> Arena<T> {
    /// Create a new arena
    pub fn new() -> Self {
        Arena {
            entries: Vec::new(),
            free_head: None,
            generation: 0,
        }
    }
    
    /// Insert a value into the arena
    pub fn insert(&mut self, value: T) -> Handle<T> {
        // Increment generation to avoid ABA problem
        self.generation = self.generation.wrapping_add(1);
        
        // If we have a free slot, use it
        if let Some(index) = self.free_head {
            match &mut self.entries[index] {
                Entry::Free { next_free } => {
                    // Update free head
                    self.free_head = *next_free;
                    
                    // Store value
                    self.entries[index] = Entry::Occupied {
                        value,
                        generation: self.generation,
                    };
                    
                    // Return handle
                    Handle {
                        index: index as u32,
                        generation: self.generation,
                        _phantom: PhantomData,
                    }
                }
                _ => unreachable!("Free list invariant violation"),
            }
        } else {
            // No free slots, add a new one
            let index = self.entries.len();
            
            // Store value
            self.entries.push(Entry::Occupied {
                value,
                generation: self.generation,
            });
            
            // Return handle
            Handle {
                index: index as u32,
                generation: self.generation,
                _phantom: PhantomData,
            }
        }
    }
    
    /// Get a reference to a value in the arena
    pub fn get(&self, handle: Handle<T>) -> Option<&T> {
        if handle.index as usize >= self.entries.len() {
            return None;
        }
        
        match &self.entries[handle.index as usize] {
            Entry::Occupied { value, generation } => {
                if *generation == handle.generation {
                    Some(value)
                } else {
                    None
                }
            }
            Entry::Free { .. } => None,
        }
    }
    
    /// Get a mutable reference to a value in the arena
    pub fn get_mut(&mut self, handle: Handle<T>) -> Option<&mut T> {
        if handle.index as usize >= self.entries.len() {
            return None;
        }
        
        match &mut self.entries[handle.index as usize] {
            Entry::Occupied { value, generation } => {
                if *generation == handle.generation {
                    Some(value)
                } else {
                    None
                }
            }
            Entry::Free { .. } => None,
        }
    }
    
    /// Remove a value from the arena
    pub fn remove(&mut self, handle: Handle<T>) -> Option<T> {
        if handle.index as usize >= self.entries.len() {
            return None;
        }
        
        match &self.entries[handle.index as usize] {
            Entry::Occupied { generation, .. } => {
                if *generation != handle.generation {
                    return None;
                }
            }
            Entry::Free { .. } => return None,
        }
        
        // Take the entry and replace it with a free entry
        let entry = std::mem::replace(
            &mut self.entries[handle.index as usize],
            Entry::Free {
                next_free: self.free_head,
            },
        );
        
        // Update free head
        self.free_head = Some(handle.index as usize);
        
        // Extract value
        match entry {
            Entry::Occupied { value, .. } => Some(value),
            Entry::Free { .. } => unreachable!(),
        }
    }
    
    /// Check if the arena contains a handle
    pub fn contains(&self, handle: Handle<T>) -> bool {
        if handle.index as usize >= self.entries.len() {
            return false;
        }
        
        match &self.entries[handle.index as usize] {
            Entry::Occupied { generation, .. } => *generation == handle.generation,
            Entry::Free { .. } => false,
        }
    }
    
    /// Get the current generation
    pub fn generation(&self) -> u32 {
        self.generation
    }
    
    /// Get the number of entries in the arena
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    
    /// Check if the arena is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    
    /// Clear the arena
    pub fn clear(&mut self) {
        self.entries.clear();
        self.free_head = None;
        self.generation = 0;
    }
    
    /// Iterate over all valid entries
    pub fn iter(&self) -> impl Iterator<Item = (Handle<T>, &T)> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| match entry {
                Entry::Occupied { value, generation } => Some((
                    Handle {
                        index: index as u32,
                        generation: *generation,
                        _phantom: PhantomData,
                    },
                    value,
                )),
                Entry::Free { .. } => None,
            })
    }
    
    /// Iterate over all valid entries mutably
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Handle<T>, &mut T)> {
        self.entries
            .iter_mut()
            .enumerate()
            .filter_map(|(index, entry)| match entry {
                Entry::Occupied { value, generation } => {
                    let gen = *generation;
                    Some((
                        Handle {
                            index: index as u32,
                            generation: gen,
                            _phantom: PhantomData,
                        },
                        value,
                    ))
                }
                Entry::Free { .. } => None,
            })
    }
}

/// A typed handle for an object in an arena
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TypedHandle<T>(pub Handle<T>);

impl<T> Default for TypedHandle<T> {
    fn default() -> Self {
        TypedHandle(Handle::default())
    }
}

/// A registry for handles
pub struct HandleRegistry<K, V> {
    /// Map from keys to handles
    map: HashMap<K, V>,
}

impl<K: Eq + Hash, V> HandleRegistry<K, V> {
    /// Create a new registry
    pub fn new() -> Self {
        HandleRegistry {
            map: HashMap::new(),
        }
    }
    
    /// Insert a handle into the registry
    pub fn insert(&mut self, key: K, handle: V) {
        self.map.insert(key, handle);
    }
    
    /// Get a handle from the registry
    pub fn get(&self, key: &K) -> Option<&V> {
        self.map.get(key)
    }
    
    /// Remove a handle from the registry
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.map.remove(key)
    }
    
    /// Check if the registry contains a key
    pub fn contains_key(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }
    
    /// Clear the registry
    pub fn clear(&mut self) {
        self.map.clear();
    }
    
    /// Get the number of handles in the registry
    pub fn len(&self) -> usize {
        self.map.len()
    }
    
    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
    
    /// Iterate over all handles
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.map.iter()
    }
}

/// Validation scope for handles
pub struct ValidScope<'heap> {
    /// Heap generation
    generation: u32,
    
    /// Phantom data for heap lifetime
    _phantom: PhantomData<&'heap ()>,
}

impl<'heap> ValidScope<'heap> {
    /// Create a new validation scope
    pub fn new(generation: u32) -> Self {
        ValidScope {
            generation,
            _phantom: PhantomData,
        }
    }
    
    /// Validate a handle
    pub fn validate<T>(&self, handle: Handle<T>) -> Result<ValidHandle<'heap, T>> {
        if handle.generation != self.generation {
            return Err(LuaError::StaleHandle(handle.index, handle.generation));
        }
        
        Ok(ValidHandle {
            handle,
            _scope: PhantomData,
        })
    }
}

/// A handle validated within a scope
pub struct ValidHandle<'scope, T> {
    /// The handle
    pub handle: Handle<T>,
    
    /// Phantom data for scope lifetime
    _scope: PhantomData<&'scope ()>,
}

impl<'scope, T> ValidHandle<'scope, T> {
    /// Get the underlying handle
    pub fn handle(&self) -> Handle<T> {
        self.handle
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_arena_basics() {
        let mut arena = Arena::new();
        
        // Insert some values
        let h1 = arena.insert(42);
        let h2 = arena.insert(43);
        
        // Get values
        assert_eq!(arena.get(h1), Some(&42));
        assert_eq!(arena.get(h2), Some(&43));
        
        // Modify a value
        if let Some(v) = arena.get_mut(h1) {
            *v = 100;
        }
        
        assert_eq!(arena.get(h1), Some(&100));
        
        // Remove a value
        assert_eq!(arena.remove(h1), Some(100));
        assert_eq!(arena.get(h1), None);
        
        // Handle is no longer valid
        assert!(!arena.contains(h1));
        
        // But the other handle is still valid
        assert!(arena.contains(h2));
        assert_eq!(arena.get(h2), Some(&43));
        
        // Reuse the slot
        let h3 = arena.insert(200);
        
        // Different handle, same slot
        assert_ne!(h1.index, h3.index);
        assert_ne!(h1.generation, h3.generation);
        
        // Get the value
        assert_eq!(arena.get(h3), Some(&200));
    }
    
    #[test]
    fn test_arena_generation_wrapping() {
        let mut arena = Arena::<i32>::new();
        
        // Force generation to wrap
        arena.generation = u32::MAX;
        
        // Insert a value
        let h1 = arena.insert(42);
        
        // Generation wrapped to 0
        assert_eq!(h1.generation, 0);
        assert_eq!(arena.generation, 0);
        
        // Value is still accessible
        assert_eq!(arena.get(h1), Some(&42));
    }
    
    #[test]
    fn test_handle_registry() {
        let mut registry = HandleRegistry::new();
        
        // Insert some handles
        registry.insert("foo", 42);
        registry.insert("bar", 43);
        
        // Get handles
        assert_eq!(registry.get(&"foo"), Some(&42));
        assert_eq!(registry.get(&"bar"), Some(&43));
        
        // Remove a handle
        assert_eq!(registry.remove(&"foo"), Some(42));
        assert_eq!(registry.get(&"foo"), None);
        
        // Check if it contains a key
        assert!(!registry.contains_key(&"foo"));
        assert!(registry.contains_key(&"bar"));
        
        // Clear the registry
        registry.clear();
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());
    }
    
    #[test]
    fn test_validation_scope() {
        // Create a handle with a specific generation
        let handle = Handle::<i32> {
            index: 1,
            generation: 42,
            _phantom: PhantomData,
        };
        
        // Create a scope with the same generation
        let scope = ValidScope::new(42);
        
        // Validate the handle
        let valid_handle = scope.validate(handle);
        assert!(valid_handle.is_ok());
        
        // Create a scope with a different generation
        let scope = ValidScope::new(43);
        
        // Validate the handle
        let valid_handle = scope.validate(handle);
        assert!(valid_handle.is_err());
    }
}