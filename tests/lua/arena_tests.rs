//! Unit tests for the Arena memory management system

#[cfg(test)]
mod arena_tests {
    use ferrous::lua::arena::{Arena, Handle};
    use std::marker::PhantomData;

    #[derive(Debug, Clone, PartialEq)]
    struct TestObject {
        value: i32,
        name: String,
    }

    #[test]
    fn test_arena_create() {
        let arena: Arena<TestObject> = Arena::new();
        assert_eq!(arena.len(), 0);
    }

    #[test]
    fn test_arena_insert_and_get() {
        let mut arena = Arena::new();
        
        let obj = TestObject {
            value: 42,
            name: "test".to_string(),
        };
        
        let handle = arena.insert(obj.clone());
        
        assert_eq!(arena.len(), 1);
        assert_eq!(arena.get(&handle), Some(&obj));
    }

    #[test]
    fn test_arena_multiple_inserts() {
        let mut arena = Arena::new();
        
        let handles: Vec<_> = (0..10).map(|i| {
            arena.insert(TestObject {
                value: i,
                name: format!("test{}", i),
            })
        }).collect();
        
        assert_eq!(arena.len(), 10);
        
        for (i, handle) in handles.iter().enumerate() {
            let obj = arena.get(handle).unwrap();
            assert_eq!(obj.value, i as i32);
            assert_eq!(obj.name, format!("test{}", i));
        }
    }

    #[test]
    fn test_arena_remove() {
        let mut arena = Arena::new();
        
        let handle1 = arena.insert(TestObject { value: 1, name: "one".to_string() });
        let handle2 = arena.insert(TestObject { value: 2, name: "two".to_string() });
        let handle3 = arena.insert(TestObject { value: 3, name: "three".to_string() });
        
        assert_eq!(arena.len(), 3);
        
        // Remove the middle one
        let removed = arena.remove(&handle2);
        assert_eq!(removed, Some(TestObject { value: 2, name: "two".to_string() }));
        assert_eq!(arena.len(), 2);
        
        // Verify it's gone
        assert_eq!(arena.get(&handle2), None);
        
        // Verify others are still there
        assert!(arena.get(&handle1).is_some());
        assert!(arena.get(&handle3).is_some());
    }

    #[test]
    fn test_arena_reuse_slots() {
        let mut arena = Arena::new();
        
        // Insert and remove to create a free slot
        let handle1 = arena.insert(TestObject { value: 1, name: "one".to_string() });
        arena.remove(&handle1);
        
        // Insert new item - should reuse the slot
        let handle2 = arena.insert(TestObject { value: 2, name: "two".to_string() });
        
        // The index should be the same, but generation different
        assert_eq!(handle1.index(), handle2.index());
        assert_ne!(handle1.generation(), handle2.generation());
    }

    #[test]
    fn test_arena_generation_prevents_use_after_free() {
        let mut arena = Arena::new();
        
        let handle1 = arena.insert(TestObject { value: 1, name: "one".to_string() });
        arena.remove(&handle1);
        
        // Old handle should not work
        assert_eq!(arena.get(&handle1), None);
        
        // Insert new item in same slot
        let _handle2 = arena.insert(TestObject { value: 2, name: "two".to_string() });
        
        // Old handle still should not work
        assert_eq!(arena.get(&handle1), None);
    }

    #[test]
    fn test_arena_clear() {
        let mut arena = Arena::new();
        
        let _handles: Vec<_> = (0..10).map(|i| {
            arena.insert(TestObject {
                value: i,
                name: format!("test{}", i),
            })
        }).collect();
        
        assert_eq!(arena.len(), 10);
        
        arena.clear();
        
        assert_eq!(arena.len(), 0);
    }

    #[test]
    fn test_arena_get_mut() {
        let mut arena = Arena::new();
        
        let handle = arena.insert(TestObject {
            value: 42,
            name: "test".to_string(),
        });
        
        // Modify through mutable reference
        if let Some(obj) = arena.get_mut(&handle) {
            obj.value = 100;
            obj.name = "modified".to_string();
        }
        
        // Verify changes
        let obj = arena.get(&handle).unwrap();
        assert_eq!(obj.value, 100);
        assert_eq!(obj.name, "modified");
    }

    #[test]
    fn test_arena_contains() {
        let mut arena = Arena::new();
        
        let handle1 = arena.insert(TestObject { value: 1, name: "one".to_string() });
        
        assert!(arena.contains(&handle1));
        
        arena.remove(&handle1);
        
        assert!(!arena.contains(&handle1));
    }

    #[test]
    fn test_arena_iter() {
        let mut arena = Arena::new();
        
        let handles: Vec<_> = (0..5).map(|i| {
            arena.insert(TestObject {
                value: i,
                name: format!("test{}", i),
            })
        }).collect();
        
        // Remove one to test iteration with holes
        arena.remove(&handles[2]);
        
        let mut values: Vec<i32> = arena.iter()
            .map(|(_, obj)| obj.value)
            .collect();
        values.sort();
        
        assert_eq!(values, vec![0, 1, 3, 4]);
    }

    #[test]
    fn test_arena_drain() {
        let mut arena = Arena::new();
        
        for i in 0..5 {
            arena.insert(TestObject {
                value: i,
                name: format!("test{}", i),
            });
        }
        
        let drained: Vec<TestObject> = arena.drain().collect();
        
        assert_eq!(arena.len(), 0);
        assert_eq!(drained.len(), 5);
        
        let mut values: Vec<i32> = drained.iter().map(|obj| obj.value).collect();
        values.sort();
        assert_eq!(values, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn test_arena_capacity_management() {
        let mut arena = Arena::new();
        
        // Insert many items to test capacity growth
        let handles: Vec<_> = (0..1000).map(|i| {
            arena.insert(TestObject {
                value: i,
                name: format!("test{}", i),
            })
        }).collect();
        
        assert_eq!(arena.len(), 1000);
        
        // Remove half
        for i in (0..1000).step_by(2) {
            arena.remove(&handles[i]);
        }
        
        assert_eq!(arena.len(), 500);
        
        // Verify remaining items
        for i in (1..1000).step_by(2) {
            let obj = arena.get(&handles[i]).unwrap();
            assert_eq!(obj.value, i as i32);
        }
    }

    #[test]
    fn test_typed_handle() {
        use ferrous::lua::arena::TypedHandle;
        
        #[derive(Debug, Clone, Copy)]
        struct StringHandle(Handle<String>);
        
        #[derive(Debug, Clone, Copy)]
        struct NumberHandle(Handle<i32>);
        
        let mut string_arena = Arena::new();
        let mut number_arena = Arena::new();
        
        let str_handle = StringHandle(string_arena.insert("hello".to_string()));
        let num_handle = NumberHandle(number_arena.insert(42));
        
        // Type safety - these would not compile if uncommented:
        // let _ = string_arena.get(&num_handle.0);
        // let _ = number_arena.get(&str_handle.0);
        
        assert_eq!(string_arena.get(&str_handle.0), Some(&"hello".to_string()));
        assert_eq!(number_arena.get(&num_handle.0), Some(&42));
    }

    #[test]
    fn test_handle_size() {
        use std::mem::size_of;
        
        // Ensure handles are small and efficient
        assert_eq!(size_of::<Handle<TestObject>>(), 16); // index (8) + generation (4) + phantom (0) + padding
    }
}