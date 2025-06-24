//! Generational arena implementation for Lua objects

use std::marker::PhantomData;

/// A handle into a generational arena
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handle {
    /// Index into the arena
    pub index: u32,
    
    /// Generation count for detecting stale references
    pub generation: u32,
}

impl Handle {
    /// Create a new handle
    pub fn new(index: u32, generation: u32) -> Self {
        Handle { index, generation }
    }
}

/// A slot in the generational arena
#[derive(Debug)]
struct Slot<T> {
    /// The stored value (if occupied)
    value: Option<T>,
    
    /// Generation counter
    generation: u32,
}

/// A generational arena for storing values
#[derive(Debug)]
pub struct Arena<T> {
    /// Storage slots
    slots: Vec<Slot<T>>,
    
    /// Free list (indices of empty slots)
    free_list: Vec<u32>,
    
    /// Total number of occupied slots
    occupied: usize,
}

impl<T> Arena<T> {
    /// Create a new empty arena
    pub fn new() -> Self {
        Arena {
            slots: Vec::new(),
            free_list: Vec::new(),
            occupied: 0,
        }
    }
    
    /// Create a new arena with pre-allocated capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Arena {
            slots: Vec::with_capacity(capacity),
            free_list: Vec::new(),
            occupied: 0,
        }
    }
    
    /// Insert a value into the arena, returning its handle
    pub fn insert(&mut self, value: T) -> Handle {
        if let Some(index) = self.free_list.pop() {
            // Reuse a free slot
            let slot = &mut self.slots[index as usize];
            slot.value = Some(value);
            slot.generation = slot.generation.wrapping_add(1);
            self.occupied += 1;
            Handle::new(index, slot.generation)
        } else {
            // Allocate a new slot
            let index = self.slots.len() as u32;
            self.slots.push(Slot {
                value: Some(value),
                generation: 0,
            });
            self.occupied += 1;
            Handle::new(index, 0)
        }
    }
    
    /// Remove a value from the arena
    pub fn remove(&mut self, handle: Handle) -> Option<T> {
        if let Some(slot) = self.slots.get_mut(handle.index as usize) {
            if slot.generation == handle.generation {
                if let Some(value) = slot.value.take() {
                    self.free_list.push(handle.index);
                    self.occupied -= 1;
                    return Some(value);
                }
            }
        }
        None
    }
    
    /// Get a reference to a value in the arena
    pub fn get(&self, handle: Handle) -> Option<&T> {
        self.slots.get(handle.index as usize)
            .and_then(|slot| {
                if slot.generation == handle.generation {
                    slot.value.as_ref()
                } else {
                    None
                }
            })
    }
    
    /// Get a mutable reference to a value in the arena
    pub fn get_mut(&mut self, handle: Handle) -> Option<&mut T> {
        self.slots.get_mut(handle.index as usize)
            .and_then(|slot| {
                if slot.generation == handle.generation {
                    slot.value.as_mut()
                } else {
                    None
                }
            })
    }
    
    /// Check if a handle is valid
    pub fn contains(&self, handle: Handle) -> bool {
        self.slots.get(handle.index as usize)
            .map(|slot| slot.generation == handle.generation && slot.value.is_some())
            .unwrap_or(false)
    }
    
    /// Get the number of occupied slots
    pub fn len(&self) -> usize {
        self.occupied
    }
    
    /// Check if the arena is empty
    pub fn is_empty(&self) -> bool {
        self.occupied == 0
    }
    
    /// Clear all values from the arena
    pub fn clear(&mut self) {
        self.slots.clear();
        self.free_list.clear();
        self.occupied = 0;
    }
    
    /// Iterate over all occupied slots
    pub fn iter(&self) -> ArenaIter<T> {
        ArenaIter {
            arena: self,
            index: 0,
        }
    }
    
    /// Iterate over all occupied slots mutably
    pub fn iter_mut(&mut self) -> ArenaIterMut<T> {
        ArenaIterMut {
            slots: &mut self.slots,
            index: 0,
            remaining: self.occupied,
        }
    }
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Iterator over arena values
pub struct ArenaIter<'a, T> {
    arena: &'a Arena<T>,
    index: usize,
}

impl<'a, T> Iterator for ArenaIter<'a, T> {
    type Item = (Handle, &'a T);
    
    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.arena.slots.len() {
            let idx = self.index;
            self.index += 1;
            
            if let Some(slot) = self.arena.slots.get(idx) {
                if let Some(ref value) = slot.value {
                    let handle = Handle::new(idx as u32, slot.generation);
                    return Some((handle, value));
                }
            }
        }
        None
    }
}

/// Mutable iterator over arena values
pub struct ArenaIterMut<'a, T> {
    slots: &'a mut [Slot<T>],
    index: usize,
    remaining: usize,
}

impl<'a, T> Iterator for ArenaIterMut<'a, T> {
    type Item = (Handle, &'a mut T);
    
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        
        while self.index < self.slots.len() {
            let idx = self.index;
            self.index += 1;
            
            // Use unsafe to work around borrowing limitations
            unsafe {
                let slot = self.slots.get_unchecked_mut(idx);
                if let Some(ref mut value) = slot.value {
                    self.remaining -= 1;
                    let handle = Handle::new(idx as u32, slot.generation);
                    // Convert to a raw pointer and back to extend lifetime
                    let value_ptr = value as *mut T;
                    return Some((handle, &mut *value_ptr));
                }
            }
        }
        None
    }
}

/// A typed wrapper around Handle for better type safety
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypedHandle<T> {
    handle: Handle,
    _phantom: PhantomData<T>,
}

impl<T> TypedHandle<T> {
    /// Create a new typed handle
    pub fn new(handle: Handle) -> Self {
        TypedHandle {
            handle,
            _phantom: PhantomData,
        }
    }
    
    /// Get the underlying handle
    pub fn handle(&self) -> Handle {
        self.handle
    }
}

/// An arena that uses typed handles
pub struct TypedArena<T> {
    arena: Arena<T>,
}

impl<T> TypedArena<T> {
    /// Create a new typed arena
    pub fn new() -> Self {
        TypedArena {
            arena: Arena::new(),
        }
    }
    
    /// Insert a value, returning a typed handle
    pub fn insert(&mut self, value: T) -> TypedHandle<T> {
        TypedHandle::new(self.arena.insert(value))
    }
    
    /// Remove a value
    pub fn remove(&mut self, handle: TypedHandle<T>) -> Option<T> {
        self.arena.remove(handle.handle)
    }
    
    /// Get a reference to a value
    pub fn get(&self, handle: TypedHandle<T>) -> Option<&T> {
        self.arena.get(handle.handle)
    }
    
    /// Get a mutable reference to a value
    pub fn get_mut(&mut self, handle: TypedHandle<T>) -> Option<&mut T> {
        self.arena.get_mut(handle.handle)
    }
    
    /// Check if a handle is valid
    pub fn contains(&self, handle: TypedHandle<T>) -> bool {
        self.arena.contains(handle.handle)
    }
    
    /// Get the number of values
    pub fn len(&self) -> usize {
        self.arena.len()
    }
    
    /// Check if empty  
    pub fn is_empty(&self) -> bool {
        self.arena.is_empty()
    }
    
    /// Clear all values
    pub fn clear(&mut self) {
        self.arena.clear()
    }
}

impl<T> Default for TypedArena<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_arena_basic() {
        let mut arena = Arena::new();
        
        // Insert some values
        let h1 = arena.insert("hello");
        let h2 = arena.insert("world");
        
        // Get values
        assert_eq!(arena.get(h1), Some(&"hello"));
        assert_eq!(arena.get(h2), Some(&"world"));
        
        // Remove a value
        assert_eq!(arena.remove(h1), Some("hello"));
        assert_eq!(arena.get(h1), None);
        
        // Reuse the slot
        let h3 = arena.insert("reused");
        assert_eq!(h3.index, h1.index);
        assert_ne!(h3.generation, h1.generation);
    }
    
    #[test]
    fn test_stale_handles() {
        let mut arena = Arena::new();
        
        let h1 = arena.insert(42);
        arena.remove(h1);
        let h2 = arena.insert(84);
        
        // h1 is stale and should not access the new value
        assert_eq!(arena.get(h1), None);
        assert_eq!(arena.get(h2), Some(&84));
    }
}