//! MEMORY command implementations
//! 
//! Provides Redis-compatible commands for memory usage reporting and management.

use std::collections::HashMap;
use std::sync::Arc;
use crate::error::{Result, CommandError};
use crate::protocol::RespFrame;
use crate::storage::{StorageEngine, Value};
use crate::storage::memory::MemoryManager;

/// Handle MEMORY command and its subcommands
pub fn handle_memory(
    parts: &[RespFrame], 
    storage: &Arc<StorageEngine>,
    db: usize,
) -> Result<RespFrame> {
    if parts.len() < 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'memory' command"));
    }
    
    // Extract subcommand
    let subcommand = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            String::from_utf8_lossy(bytes).to_uppercase()
        },
        _ => return Ok(RespFrame::error("ERR invalid subcommand format")),
    };
    
    match subcommand.as_str() {
        "USAGE" => handle_memory_usage(parts, storage, db),
        "STATS" => handle_memory_stats(storage),
        "DOCTOR" => handle_memory_doctor(storage),
        "HELP" => handle_memory_help(),
        _ => Ok(RespFrame::error(format!("ERR Unknown subcommand or wrong number of arguments for 'memory {}'", subcommand)))
    }
}

/// Handle MEMORY USAGE command to report memory consumption of a key
/// 
/// This command estimates the memory usage of a key and its value
pub fn handle_memory_usage(parts: &[RespFrame], storage: &Arc<StorageEngine>, db: usize) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'memory usage' command"));
    }
    
    // Extract key
    let key = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Check if key exists
    if !storage.exists(db, key)? {
        return Ok(RespFrame::error("ERR no such key"));
    }
    
    // Get the memory usage
    let memory_usage = calculate_key_memory_usage(storage, db, key)?;
    
    Ok(RespFrame::Integer(memory_usage as i64))
}

/// Helper function to calculate memory usage of a key
fn calculate_key_memory_usage(storage: &Arc<StorageEngine>, db: usize, key: &[u8]) -> Result<usize> {
    // First, see what type of value we're dealing with
    let key_type = storage.key_type(db, key)?;

    // Calculate base memory (key + metadata overhead)
    let mut total_size = MemoryManager::calculate_size(key);
    
    // Add value-specific memory usage
    match key_type.as_str() {
        "string" => {
            if let Ok(Some(value)) = storage.get_string(db, key) {
                total_size += MemoryManager::calculate_size(&value);
                total_size += std::mem::size_of::<Value>(); // Value enum overhead
            }
        },
        "list" => {
            // Approximate calculation based on elements
            if let Ok(list_len) = storage.llen(db, key) {
                if list_len > 0 {
                    // Sample a few items to get average size
                    let mut sample_size = 0;
                    let mut sample_count = 0;
                    
                    // Try to get a few elements from different positions
                    let positions = [0, list_len/2, list_len.saturating_sub(1)];
                    for pos in positions.iter().take_while(|&&p| p < list_len) {
                        if let Ok(Some(item)) = storage.lindex(db, key, *pos as isize) {
                            sample_size += MemoryManager::calculate_size(&item);
                            sample_count += 1;
                        }
                    }
                    
                    let avg_item_size = if sample_count > 0 {
                        sample_size / sample_count
                    } else {
                        0
                    };
                    
                    total_size += avg_item_size * list_len; // Element size
                    total_size += std::mem::size_of::<std::collections::VecDeque<Vec<u8>>>(); // List struct overhead
                }
            }
        },
        "set" => {
            // Approximate calculation based on members
            if let Ok(set_size) = storage.scard(db, key) {
                if set_size > 0 {
                    if let Ok(members) = storage.smembers(db, key) {
                        let mut member_size = 0;
                        for member in members.iter().take(std::cmp::min(set_size, 10)) {
                            member_size += MemoryManager::calculate_size(member);
                        }
                        
                        let avg_member_size = if members.len() > 0 {
                            member_size / members.len()
                        } else {
                            0
                        };
                        
                        total_size += avg_member_size * set_size; // Member size
                        total_size += std::mem::size_of::<std::collections::HashSet<Vec<u8>>>(); // Set struct overhead
                    }
                }
            }
        },
        "hash" => {
            // Get all fields and values and calculate size
            if let Ok(hash_len) = storage.hlen(db, key) {
                if hash_len > 0 {
                    if let Ok(pairs) = storage.hgetall(db, key) {
                        for (field, value) in pairs {
                            total_size += MemoryManager::calculate_size(&field);
                            total_size += MemoryManager::calculate_size(&value);
                        }
                        
                        total_size += std::mem::size_of::<HashMap<Vec<u8>, Vec<u8>>>(); // Hash struct overhead
                    }
                }
            }
        },
        "zset" => {
            // Estimate based on a sample of sorted set entries
            // We're making a simplified calculation here - for more accuracy we'd need access to internal SkipList details
            // But this gives a reasonable approximation
            if let Ok(range) = storage.zrange(db, key, 0, -1, false) {
                for (member, _) in range {
                    total_size += MemoryManager::calculate_size(&member);
                    total_size += std::mem::size_of::<f64>(); // Score size
                }
                
                // Add overhead for SkipList structure
                total_size += std::mem::size_of::<crate::storage::skiplist::SkipList<Vec<u8>, f64>>(); // SkipList overhead
            }
        },
        _ => {
            // Unknown type or none
            // Only count key overhead already added above
        }
    }
    
    // Include standard metadata overhead
    total_size += std::mem::size_of::<super::super::value::StoredValue>(); // StoredValue overhead
    
    Ok(total_size)
}

/// Handle MEMORY STATS command to report server memory usage statistics
pub fn handle_memory_stats(storage: &Arc<StorageEngine>) -> Result<RespFrame> {
    let mut stats_data = Vec::new();
    
    // Get total memory usage
    stats_data.push(("total.allocated".to_string(), format!("{}", storage.memory_usage())));
    
    // Memory fragmentation ratio (simplified as we don't track RSS)
    stats_data.push(("mem_fragmentation_ratio".to_string(), "1.0".to_string()));
    
    // Memory allocator stats (simplified)
    stats_data.push(("allocator".to_string(), "rust-memory".to_string()));
    
    // Per-database memory usage (if we had more detailed tracking)
    let db_count = storage.database_count();
    for db in 0..db_count {
        if let Ok(keys) = storage.get_all_keys(db) {
            let key_count = keys.len();  // Store the length before moving the keys
            let mut db_size = 0;
            for key in &keys {  // Use a reference to avoid moving keys
                if let Ok(usage) = calculate_key_memory_usage(storage, db, key) {
                    db_size += usage;
                }
            }
            stats_data.push((format!("db{}.overhead", db), format!("{}", db_size)));
            stats_data.push((format!("db{}.keys", db), format!("{}", key_count)));
        }
    }
    
    // Convert stats to RESP format
    let mut resp_array = Vec::new();
    for (key, value) in stats_data {
        resp_array.push(RespFrame::from_string(key));
        resp_array.push(RespFrame::from_string(value));
    }
    
    Ok(RespFrame::Array(Some(resp_array)))
}

/// Handle MEMORY DOCTOR command to report memory health assessment
pub fn handle_memory_doctor(storage: &Arc<StorageEngine>) -> Result<RespFrame> {
    // Perform basic memory health checks
    let mut report = String::new();
    
    // Check total memory usage
    let total_memory = storage.memory_usage();
    report.push_str(&format!("Memory usage: {} bytes\n", total_memory));
    
    // Check for large keys
    let mut largest_keys = Vec::new();
    for db in 0..storage.database_count() {
        if let Ok(keys) = storage.get_all_keys(db) {
            for key in keys {
                if let Ok(usage) = calculate_key_memory_usage(storage, db, &key) {
                    largest_keys.push((db, key.clone(), usage));
                }
                
                // Only track most extreme cases to avoid excessive memory use in the analysis
                largest_keys.sort_by(|a, b| b.2.cmp(&a.2));
                if largest_keys.len() > 10 {
                    largest_keys.truncate(10);
                }
            }
        }
    }
    
    // Report on largest keys if any are found
    if !largest_keys.is_empty() {
        report.push_str("\nLargest keys found:\n");
        for (db, key, size) in largest_keys {
            let key_str = String::from_utf8_lossy(&key);
            let key_type = storage.key_type(db, &key).unwrap_or_else(|_| "unknown".to_string());
            report.push_str(&format!("db={} key='{}' type={} size={} bytes\n", 
                            db, key_str, key_type, size));
        }
    }
    
    // Add general advice
    report.push_str("\nMemory efficiency tips:\n");
    report.push_str("1. Use EXPIRE for temporary keys\n");
    report.push_str("2. Use MEMORY USAGE to identify large keys\n");
    report.push_str("3. Consider using smaller key names to save memory\n");
    
    Ok(RespFrame::from_string(report))
}

/// Handle MEMORY HELP command
pub fn handle_memory_help() -> Result<RespFrame> {
    let help_text = r#"MEMORY USAGE <key> - Report the memory usage in bytes of a key and its value
MEMORY STATS - Report memory usage statistics
MEMORY DOCTOR - Report about memory problems and provide recommendations
MEMORY HELP - Show this help"#;
    
    Ok(RespFrame::from_string(help_text))
}