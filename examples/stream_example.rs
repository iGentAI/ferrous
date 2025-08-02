//! Example demonstrating Ferrous Streams for time-series data
//!
//! This example shows how to:
//! - Add entries to streams with auto-generated IDs
//! - Query streams by ID ranges
//! - Implement a simple sensor data collection system

use ferrous::storage::engine::StorageEngine;
use ferrous::storage::stream::StreamId;
use std::collections::HashMap;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn main() {
    println!("Ferrous Stream Example - Sensor Data Collection\n");
    
    // Create storage engine
    let engine = StorageEngine::new();
    
    // Simulate temperature sensor readings
    println!("Adding temperature sensor readings...");
    
    for i in 0..5 {
        let mut fields = HashMap::new();
        fields.insert(b"temperature".to_vec(), format!("{:.1}", 20.0 + i as f64 * 0.5).into_bytes());
        fields.insert(b"humidity".to_vec(), format!("{}", 50 + i * 2).into_bytes());
        fields.insert(b"sensor_id".to_vec(), b"sensor_01".to_vec());
        
        let id = engine.xadd(0, b"temperature:readings".to_vec(), fields).unwrap();
        println!("Added reading with ID: {}", id);
        
        // Small delay to ensure different timestamps
        thread::sleep(Duration::from_millis(10));
    }
    
    // Query all readings
    println!("\nAll sensor readings:");
    let entries = engine.xrange(
        0, 
        b"temperature:readings", 
        StreamId::min(), 
        StreamId::max(), 
        None
    ).unwrap();
    
    for entry in &entries {
        let temp = String::from_utf8_lossy(&entry.fields[&b"temperature".to_vec()]);
        let humidity = String::from_utf8_lossy(&entry.fields[&b"humidity".to_vec()]);
        println!("  {} - Temperature: {}°C, Humidity: {}%", entry.id, temp, humidity);
    }
    
    // Simulate event log
    println!("\nAdding system events...");
    
    let events = vec![
        ("user_login", "user123"),
        ("api_request", "/api/v1/data"),
        ("user_logout", "user123"),
    ];
    
    let mut event_ids = Vec::new();
    
    for (event_type, details) in events {
        let mut fields = HashMap::new();
        fields.insert(b"event".to_vec(), event_type.as_bytes().to_vec());
        fields.insert(b"details".to_vec(), details.as_bytes().to_vec());
        
        // Add timestamp using SystemTime
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        fields.insert(b"timestamp".to_vec(), timestamp.to_string().into_bytes());
        
        let id = engine.xadd(0, b"system:events".to_vec(), fields).unwrap();
        event_ids.push(id.clone());
        println!("Logged event '{}' with ID: {}", event_type, id);
    }
    
    // Query recent events
    println!("\nRecent events (last 2):");
    let recent = engine.xrevrange(
        0,
        b"system:events",
        StreamId::max(),
        StreamId::min(),
        Some(2)
    ).unwrap();
    
    for entry in recent {
        let event = String::from_utf8_lossy(&entry.fields[&b"event".to_vec()]);
        let details = String::from_utf8_lossy(&entry.fields[&b"details".to_vec()]);
        println!("  {} - {}: {}", entry.id, event, details);
    }
    
    // Demonstrate trimming
    println!("\nStream lengths before trimming:");
    println!("  temperature:readings: {} entries", engine.xlen(0, b"temperature:readings").unwrap());
    println!("  system:events: {} entries", engine.xlen(0, b"system:events").unwrap());
    
    // Keep only last 3 temperature readings
    let trimmed = engine.xtrim(0, b"temperature:readings", 3).unwrap();
    println!("\nTrimmed {} old temperature readings", trimmed);
    println!("  temperature:readings: {} entries remaining", engine.xlen(0, b"temperature:readings").unwrap());
    
    // Demonstrate XREAD for monitoring new entries
    println!("\nMonitoring for new entries after specific IDs...");
    
    let last_temp_id = entries.last().unwrap().id.clone();
    let last_event_id = event_ids.last().unwrap().clone();
    
    // Add one more entry to each stream
    let mut fields = HashMap::new();
    fields.insert(b"temperature".to_vec(), b"23.5".to_vec());
    fields.insert(b"humidity".to_vec(), b"65".to_vec());
    fields.insert(b"sensor_id".to_vec(), b"sensor_01".to_vec());
    engine.xadd(0, b"temperature:readings".to_vec(), fields).unwrap();
    
    let mut fields = HashMap::new();
    fields.insert(b"event".to_vec(), b"system_alert".to_vec());
    fields.insert(b"details".to_vec(), b"High temperature detected".to_vec());
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    fields.insert(b"timestamp".to_vec(), timestamp.to_string().into_bytes());
    engine.xadd(0, b"system:events".to_vec(), fields).unwrap();
    
    // Read new entries
    let keys_and_ids = vec![
        (b"temperature:readings".as_ref(), last_temp_id),
        (b"system:events".as_ref(), last_event_id),
    ];
    
    let new_entries = engine.xread(0, keys_and_ids, None, None).unwrap();
    
    println!("\nNew entries detected:");
    for (stream_key, entries) in new_entries {
        let stream_name = String::from_utf8_lossy(&stream_key);
        println!("  Stream '{}': {} new entries", stream_name, entries.len());
        
        for entry in entries {
            if stream_name.contains("temperature") {
                let temp = String::from_utf8_lossy(&entry.fields[&b"temperature".to_vec()]);
                println!("    {} - New temperature: {}°C", entry.id, temp);
            } else {
                let event = String::from_utf8_lossy(&entry.fields[&b"event".to_vec()]);
                println!("    {} - New event: {}", entry.id, event);
            }
        }
    }
    
    println!("\nStream example completed!");
}