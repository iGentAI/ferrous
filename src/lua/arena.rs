//! Generational Arena Memory Management for Lua VM
//! 
//! This module implements a generational arena allocator that provides:
//! - Type-safe handles instead of raw pointers
//! - Generation tracking to prevent use-after-free
//! - Efficient allocation and deallocation
//! - No unsafe code required

use std::marker::PhantomData;
use std::fmt;

/// A handle to an object in the arena
#[derive(PartialEq, Eq, Hash)]
pub struct Handle<T> {
    /// Index in the arena
    index: usize,
    /// Generation number to detect stale handles
    generation: u32,
    /// Type marker
    _phantom: PhantomData<T>,
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
    /// Get the index
    pub fn index(&self) -> usize {
        self.index
    }
    
    /// Get the generation
    pub fn generation(&self) -> u32 {
        self.generation
    }
    
    /// Create a new handle directly (for testing/creating dummy handles)
    pub fn new(index: u32, generation: u32) -> Self {
        Handle {
            index: index as usize,
            generation,
            _phantom: PhantomData,
        }
    }
}

impl<T> fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Handle({}:{})", self.index, self.generation)
    }
}

/// An entry in the arena
enum Entry<T> {
    /// Occupied slot with value and generation
    Occupied {
        value: T,
        generation: u32,
    },
    /// Free slot pointing to next free
    Free {
        next_free: Option<usize>,
    },
}

/// A generational arena for storing objects
pub struct Arena<T> {
    /// Storage for entries
    entries: Vec<Entry<T>>,
    /// Head of the free list
    free_head: Option<usize>,
    /// Current generation
    generation: u32,
    /// Number of occupied entries
    len: usize,
}

impl<T> Arena<T> {
    /// Create a new empty arena
    pub fn new() -> Self {
        Arena {
            entries: Vec::new(),
            free_head: None,
            generation: 0,
            len: 0,
        }
    }
    
    /// Get the number of items in the arena
    pub fn len(&self) -> usize {
        self.len
    }
    
    /// Check if the arena is empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    
    /// Insert a value and return a handle
    pub fn insert(&mut self, value: T) -> Handle<T> {
        // Increment generation (wrapping is OK)
        self.generation = self.generation.wrapping_add(1);
        
        // Get an index to use
        let index = if let Some(free_index) = self.free_head {
            // Reuse a free slot
            match &mut self.entries[free_index] {
                Entry::Free { next_free } => {
                    self.free_head = *next_free;
                    self.entries[free_index] = Entry::Occupied {
                        value,
                        generation: self.generation,
                    };
                    free_index
                }
                _ => unreachable!("free_head pointed to occupied entry"),
            }
        } else {
            // Allocate new slot
            let index = self.entries.len();
            self.entries.push(Entry::Occupied {
                value,
                generation: self.generation,
            });
            index
        };
        
        self.len += 1;
        
        Handle {
            index,
            generation: self.generation,
            _phantom: PhantomData,
        }
    }
    
    /// Get a reference to a value by handle
    pub fn get(&self, handle: &Handle<T>) -> Option<&T> {
        if handle.index >= self.entries.len() {
            return None;
        }
        
        match &self.entries[handle.index] {
            Entry::Occupied { value, generation } if *generation == handle.generation => {
                Some(value)
            }
            _ => None,
        }
    }
    
    /// Get a mutable reference to a value by handle
    pub fn get_mut(&mut self, handle: &Handle<T>) -> Option<&mut T> {
        if handle.index >= self.entries.len() {
            return None;
        }
        
        match &mut self.entries[handle.index] {
            Entry::Occupied { value, generation } if *generation == handle.generation => {
                Some(value)
            }
            _ => None,
        }
    }
    
    /// Remove a value by handle and return it
    pub fn remove(&mut self, handle: &Handle<T>) -> Option<T> {
        if handle.index >= self.entries.len() {
            return None;
        }
        
        match &mut self.entries[handle.index] {
            Entry::Occupied { generation, .. } if *generation == handle.generation => {
                // Valid handle, remove it
                let old_entry = std::mem::replace(
                    &mut self.entries[handle.index],
                    Entry::Free {
                        next_free: self.free_head,
                    },
                );
                
                self.free_head = Some(handle.index);
                self.len -= 1;
                
                match old_entry {
                    Entry::Occupied { value, .. } => Some(value),
                    _ => unreachable!(),
                }
            }
            _ => None,
        }
    }
    
    /// Check if a handle is valid
    pub fn contains(&self, handle: &Handle<T>) -> bool {
        if handle.index >= self.entries.len() {
            return false;
        }
        
        matches!(
            &self.entries[handle.index],
            Entry::Occupied { generation, .. } if *generation == handle.generation
        )
    }
    
    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
        self.free_head = None;
        self.len = 0;
        // Don't reset generation to prevent old handles from becoming valid again
    }
    
    /// Iterate over all values
    pub fn iter(&self) -> impl Iterator<Item = (Handle<T>, &T)> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(move |(index, entry)| {
                match entry {
                    Entry::Occupied { value, generation } => {
                        Some((
                            Handle {
                                index,
                                generation: *generation,
                                _phantom: PhantomData,
                            },
                            value,
                        ))
                    }
                    Entry::Free { .. } => None,
                }
            })
    }
    
    /// Iterate over all values mutably
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Handle<T>, &mut T)> {
        self.entries
            .iter_mut()
            .enumerate()
            .filter_map(move |(index, entry)| {
                match entry {
                    Entry::Occupied { value, generation } => {
                        let handle = Handle {
                            index,
                            generation: *generation,
                            _phantom: PhantomData,
                        };
                        Some((handle, value))
                    }
                    Entry::Free { .. } => None,
                }
            })
    }
    
    /// Drain all values from the arena
    pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        self.len = 0;
        self.free_head = None;
        self.entries.drain(..).filter_map(|entry| {
            match entry {
                Entry::Occupied { value, .. } => Some(value),
                Entry::Free { .. } => None,
            }
        })
    }
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// A typed handle wrapper for compile-time type safety
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypedHandle<T>(pub Handle<T>);

impl<T> TypedHandle<T> {
    /// Create a new typed handle
    pub fn new(handle: Handle<T>) -> Self {
        TypedHandle(handle)
    }
    
    /// Get the underlying handle
    pub fn handle(&self) -> &Handle<T> {
        &self.0
    }
}

impl<T> fmt::Debug for TypedHandle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TypedHandle({:?})", self.0)
    }
}

/// A validation scope for safe handle usage
pub struct ValidScope<'a> {
    generation: u32,
    _phantom: PhantomData<&'a ()>,
}

impl<'a> ValidScope<'a> {
    /// Create a new validation scope
    pub fn new(generation: u32) -> Self {
        ValidScope {
            generation,
            _phantom: PhantomData,
        }
    }
    
    /// Validate a handle within this scope
    pub fn validate<T>(&self, handle: &Handle<T>) -> Option<ValidHandle<'a, T>> {
        if handle.generation == self.generation {
            Some(ValidHandle {
                handle: *handle,
                _scope: PhantomData,
            })
        } else {
            None
        }
    }
}

/// A validated handle that's guaranteed to be valid within a scope
#[derive(Clone, Copy)]
pub struct ValidHandle<'a, T> {
    handle: Handle<T>,
    _scope: PhantomData<&'a ()>,
}

impl<'a, T> ValidHandle<'a, T> {
    /// Get the underlying handle
    pub fn handle(&self) -> Handle<T> {
        self.handle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_basic() {
        let mut arena = Arena::new();
        
        let h1 = arena.insert("hello");
        let h2 = arena.insert("world");
        
        assert_eq!(arena.get(&h1), Some(&"hello"));
        assert_eq!(arena.get(&h2), Some(&"world"));
        assert_eq!(arena.len(), 2);
    }
    
    #[test]
    fn test_arena_remove() {
        let mut arena = Arena::new();
        
        let h1 = arena.insert(1);
        let h2 = arena.insert(2);
        let h3 = arena.insert(3);
        
        assert_eq!(arena.remove(&h2), Some(2));
        assert_eq!(arena.get(&h2), None);
        assert_eq!(arena.len(), 2);
        
        // h1 and h3 should still be valid
        assert_eq!(arena.get(&h1), Some(&1));
        assert_eq!(arena.get(&h3), Some(&3));
    }
    
    #[test]
    fn test_arena_generation() {
        let mut arena = Arena::new();
        
        let h1 = arena.insert("first");
        arena.remove(&h1);
        let h2 = arena.insert("second");
        
        // Old handle should be invalid even though index is reused
        assert_eq!(arena.get(&h1), None);
        assert_eq!(arena.get(&h2), Some(&"second"));
    }
}