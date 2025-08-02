#[cfg(test)]
mod tests {
    use super::super::engine::StorageEngine;
    use super::super::stream::{StreamId, StreamEntry};
    use std::collections::HashMap;
    
    #[test]
    fn test_xadd_auto_id() {
        let engine = StorageEngine::new();
        
        let mut fields = HashMap::new();
        fields.insert(b"temperature".to_vec(), b"25.5".to_vec());
        fields.insert(b"humidity".to_vec(), b"60".to_vec());
        
        // Add entry with auto-generated ID
        let id1 = engine.xadd(0, b"sensor:1".to_vec(), fields.clone()).unwrap();
        assert!(id1.millis() > 0);
        
        // Add another entry
        let id2 = engine.xadd(0, b"sensor:1".to_vec(), fields).unwrap();
        assert!(id2 > id1);
        
        // Check stream length
        let len = engine.xlen(0, b"sensor:1").unwrap();
        assert_eq!(len, 2);
    }
    
    #[test]
    fn test_xadd_with_specific_id() {
        let engine = StorageEngine::new();
        
        let mut fields = HashMap::new();
        fields.insert(b"event".to_vec(), b"login".to_vec());
        
        // Add with specific ID
        let id = StreamId::new(1526919030474, 0);
        let result_id = engine.xadd_with_id(0, b"events".to_vec(), id.clone(), fields.clone()).unwrap();
        assert_eq!(result_id, id);
        
        // Try to add with same or lower ID - should fail
        let result = engine.xadd_with_id(0, b"events".to_vec(), id, fields.clone());
        assert!(result.is_err());
        
        // Add with higher ID - should succeed
        let id2 = StreamId::new(1526919030474, 1);
        engine.xadd_with_id(0, b"events".to_vec(), id2, fields).unwrap();
    }
    
    #[test]
    fn test_xrange() {
        let engine = StorageEngine::new();
        
        // Add multiple entries
        let ids = vec![
            StreamId::new(1000, 0),
            StreamId::new(1000, 1),
            StreamId::new(1001, 0),
            StreamId::new(1002, 0),
        ];
        
        for (i, id) in ids.iter().enumerate() {
            let mut fields = HashMap::new();
            fields.insert(b"index".to_vec(), i.to_string().into_bytes());
            engine.xadd_with_id(0, b"mystream".to_vec(), id.clone(), fields).unwrap();
        }
        
        // Query full range
        let entries = engine.xrange(0, b"mystream", StreamId::min(), StreamId::max(), None).unwrap();
        assert_eq!(entries.len(), 4);
        
        // Query partial range
        let entries = engine.xrange(0, b"mystream", ids[1].clone(), ids[2].clone(), None).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, ids[1]);
        assert_eq!(entries[1].id, ids[2]);
        
        // Query with count limit
        let entries = engine.xrange(0, b"mystream", StreamId::min(), StreamId::max(), Some(2)).unwrap();
        assert_eq!(entries.len(), 2);
    }
    
    #[test]
    fn test_xrevrange() {
        let engine = StorageEngine::new();
        
        // Add entries
        let ids = vec![
            StreamId::new(1000, 0),
            StreamId::new(1001, 0),
            StreamId::new(1002, 0),
        ];
        
        for id in &ids {
            let mut fields = HashMap::new();
            fields.insert(b"id".to_vec(), id.to_string().into_bytes());
            engine.xadd_with_id(0, b"stream".to_vec(), id.clone(), fields).unwrap();
        }
        
        // Query in reverse
        let entries = engine.xrevrange(0, b"stream", StreamId::min(), StreamId::max(), None).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].id, ids[2]); // Should be reversed
        assert_eq!(entries[2].id, ids[0]);
    }
    
    #[test]
    fn test_xread() {
        let engine = StorageEngine::new();
        
        // Add entries to multiple streams
        let mut fields1 = HashMap::new();
        fields1.insert(b"value".to_vec(), b"1".to_vec());
        let id1 = engine.xadd(0, b"stream1".to_vec(), fields1).unwrap();
        
        let mut fields2 = HashMap::new();
        fields2.insert(b"value".to_vec(), b"2".to_vec());
        let id2 = engine.xadd(0, b"stream2".to_vec(), fields2).unwrap();
        
        // Read from both streams
        let keys_and_ids = vec![
            (b"stream1".as_ref(), StreamId::new(0, 0)),
            (b"stream2".as_ref(), StreamId::new(0, 0)),
        ];
        
        let results = engine.xread(0, keys_and_ids, None, None).unwrap();
        assert_eq!(results.len(), 2);
        
        // Read only new entries after specific IDs
        let keys_and_ids = vec![
            (b"stream1".as_ref(), id1.clone()),
            (b"stream2".as_ref(), id2.clone()),
        ];
        
        let results = engine.xread(0, keys_and_ids, None, None).unwrap();
        assert_eq!(results.len(), 0); // No new entries
        
        // Add more entries and read again
        let mut fields3 = HashMap::new();
        fields3.insert(b"value".to_vec(), b"3".to_vec());
        engine.xadd(0, b"stream1".to_vec(), fields3).unwrap();
        
        let keys_and_ids = vec![(b"stream1".as_ref(), id1)];
        let results = engine.xread(0, keys_and_ids, None, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1.len(), 1); // One new entry
    }
    
    #[test]
    fn test_xtrim() {
        let engine = StorageEngine::new();
        
        // Add 10 entries
        for i in 0..10 {
            let mut fields = HashMap::new();
            fields.insert(b"num".to_vec(), i.to_string().into_bytes());
            engine.xadd_with_id(0, b"stream".to_vec(), StreamId::new(1000 + i, 0), fields).unwrap();
        }
        
        // Trim to keep only 5 newest
        let trimmed = engine.xtrim(0, b"stream", 5).unwrap();
        assert_eq!(trimmed, 5);
        
        // Check remaining entries
        let len = engine.xlen(0, b"stream").unwrap();
        assert_eq!(len, 5);
        
        // Verify oldest entries were removed
        let entries = engine.xrange(0, b"stream", StreamId::min(), StreamId::max(), None).unwrap();
        assert_eq!(entries[0].id, StreamId::new(1005, 0)); // First 5 were trimmed
    }
    
    #[test]
    fn test_xdel() {
        let engine = StorageEngine::new();
        
        // Add entries
        let ids = vec![
            StreamId::new(1000, 0),
            StreamId::new(1001, 0),
            StreamId::new(1002, 0),
        ];
        
        for id in &ids {
            let mut fields = HashMap::new();
            fields.insert(b"data".to_vec(), b"test".to_vec());
            engine.xadd_with_id(0, b"stream".to_vec(), id.clone(), fields).unwrap();
        }
        
        // Delete specific entries
        let deleted = engine.xdel(0, b"stream", vec![ids[0].clone(), ids[2].clone()]).unwrap();
        assert_eq!(deleted, 2);
        
        // Check remaining
        let len = engine.xlen(0, b"stream").unwrap();
        assert_eq!(len, 1);
        
        let entries = engine.xrange(0, b"stream", StreamId::min(), StreamId::max(), None).unwrap();
        assert_eq!(entries[0].id, ids[1]);
    }
    
    #[test]
    fn test_stream_type_check() {
        let engine = StorageEngine::new();
        
        // Create a stream
        let mut fields = HashMap::new();
        fields.insert(b"test".to_vec(), b"value".to_vec());
        engine.xadd(0, b"mystream".to_vec(), fields).unwrap();
        
        // Check type
        let type_name = engine.key_type(0, b"mystream").unwrap();
        assert_eq!(type_name, "stream");
        
        // Try to use stream key as different type - should fail
        let result = engine.lpush(0, b"mystream".to_vec(), vec![b"item".to_vec()]);
        assert!(result.is_err());
    }
}