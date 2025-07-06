//! Basic tests for Lua VM components
//! 
//! This module contains tests that verify the basic functionality
//! of the arena, heap, and transaction systems.

#[cfg(test)]
mod tests {
    use crate::lua::{
        arena::Arena,
        heap::LuaHeap,
        transaction::HeapTransaction,
        value::Value,
        error::LuaResult,
    };
    
    #[test]
    fn test_arena_basic_operations() {
        let mut arena: Arena<i32> = Arena::new();
        
        // Insert values
        let h1 = arena.insert(42);
        let h2 = arena.insert(84);
        
        // Retrieve values
        assert_eq!(arena.get(h1), Some(&42));
        assert_eq!(arena.get(h2), Some(&84));
        
        // Remove a value
        let removed = arena.remove(h1);
        assert_eq!(removed, Some(42));
        assert_eq!(arena.get(h1), None);
        
        // Reuse slot
        let h3 = arena.insert(126);
        assert_eq!(h3.index, h1.index); // Same slot
        assert_ne!(h3.generation, h1.generation); // Different generation
    }
    
    #[test]
    fn test_heap_initialization() -> LuaResult<()> {
        let heap = LuaHeap::new()?;
        
        // Verify core structures exist
        let _globals = heap.globals()?;
        let _registry = heap.registry()?;
        let _main_thread = heap.main_thread()?;
        
        Ok(())
    }
    
    #[test]
    fn test_string_creation_and_interning() -> LuaResult<()> {
        let mut heap = LuaHeap::new()?;
        
        // Create strings
        let s1 = heap.create_string_internal("hello")?;
        let s2 = heap.create_string_internal("world")?;
        let s3 = heap.create_string_internal("hello")?; // Should be interned
        
        // Verify interning
        assert_eq!(s1, s3);
        assert_ne!(s1, s2);
        
        // Verify content
        let str1 = heap.get_string(s1)?;
        let str_content = str1.to_str().map_err(|_| crate::lua::error::LuaError::RuntimeError("Invalid UTF-8".to_string()))?;
        assert_eq!(str_content, "hello");
        
        Ok(())
    }
    
    #[test]
    fn test_transaction_basic_operations() -> LuaResult<()> {
        let mut heap = LuaHeap::new()?;
        
        // Create objects using transaction
        let (table, key, value) = {
            let mut tx = HeapTransaction::new(&mut heap);
            
            let table = tx.create_table()?;
            let key = tx.create_string("key")?;
            let value = tx.create_string("value")?;
            
            tx.commit()?;
            
            (table, key, value)
        };
        
        // Use objects in another transaction
        {
            let mut tx = HeapTransaction::new(&mut heap);
            
            tx.set_table_field(
                table.clone(),
                Value::String(key.clone()),
                Value::String(value.clone())
            )?;
            
            tx.commit()?;
        }
        
        // Verify the field was set
        {
            let mut tx = HeapTransaction::new(&mut heap);
            let result = tx.read_table_field(table, &Value::String(key))?;
            
            match result {
                Value::String(s) if s == value => {
                    // Success!
                }
                _ => panic!("Expected string value"),
            }
            
            tx.commit()?;
        }
        
        Ok(())
    }
    
    #[test]
    fn test_value_types() {
        // Test nil
        assert!(Value::Nil.is_nil());
        assert!(Value::Nil.is_falsey());
        
        // Test boolean
        assert!(!Value::Boolean(true).is_falsey());
        assert!(Value::Boolean(false).is_falsey());
        
        // Test number
        let num = Value::Number(42.0);
        assert!(num.is_number());
        assert_eq!(num.to_number(), Some(42.0));
        assert!(!num.is_falsey());
        
        // Test type names
        assert_eq!(Value::Nil.type_name(), "nil");
        assert_eq!(Value::Boolean(true).type_name(), "boolean");
        assert_eq!(Value::Number(0.0).type_name(), "number");
    }
}