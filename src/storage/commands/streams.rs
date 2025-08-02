//! Stream command implementations
//! 
//! Provides Redis-compatible stream operations including XADD, XREAD, XRANGE, and more.

use crate::error::{FerrousError, Result, CommandError};
use crate::protocol::RespFrame;
use crate::storage::{StorageEngine, Value};
use crate::storage::stream::{StreamId, StreamEntry};
use std::sync::Arc;
use std::collections::HashMap;
use std::time::Duration;

/// Handle XADD command - Add entries to a stream
pub fn handle_xadd(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 4 || (parts.len() - 3) % 2 != 0 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xadd' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract ID
    let id_bytes = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid ID format")),
    };
    
    // Parse field-value pairs with pre-allocated capacity
    let num_fields = (parts.len() - 3) / 2;
    let mut fields = HashMap::with_capacity(num_fields);
    for i in (3..parts.len()).step_by(2) {
        let field = match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
            _ => return Ok(RespFrame::error("ERR invalid field format")),
        };
        
        let value = match &parts[i + 1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
            _ => return Ok(RespFrame::error("ERR invalid value format")),
        };
        
        fields.insert(field, value);
    }
    
    // Add to stream
    let result_id = if id_bytes.len() == 1 && id_bytes[0] == b'*' {
        // Auto-generate ID
        storage.xadd(db, key, fields)?
    } else {
        // Parse specific ID using optimized parsing
        let id_str = unsafe { std::str::from_utf8_unchecked(id_bytes) };
        
        let id = match StreamId::from_string(id_str) {
            Some(id) => id,
            None => return Ok(RespFrame::error("ERR Invalid stream ID specified as stream command argument")),
        };
        
        // Validate ID format using methods
        if id.millis() == 0 && id.seq() == 0 {
            return Ok(RespFrame::error("ERR The ID specified in XADD must be greater than 0-0"));
        }
        
        // Handle specific ID addition with proper error conversion
        match storage.xadd_with_id(db, key, id, fields) {
            Ok(result_id) => result_id,
            Err(e) => {
                // Convert storage errors to proper Redis error responses
                let error_msg = format!("{}", e);
                if error_msg.contains("equal or smaller") {
                    return Ok(RespFrame::error("ERR The ID specified in XADD is equal or smaller than the target stream top item"));
                } else if error_msg.contains("already exists") {
                    return Ok(RespFrame::error("ERR The ID specified in XADD is equal or smaller than the target stream top item"));
                } else {
                    return Ok(RespFrame::error(format!("ERR {}", error_msg)));
                }
            }
        }
    };
    
    Ok(RespFrame::BulkString(Some(Arc::new(result_id.to_string().into_bytes()))))
}

/// Handle XRANGE command - Get entries in a range
pub fn handle_xrange(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 4 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xrange' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Parse start ID
    let start_str = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
        _ => return Ok(RespFrame::error("ERR invalid start ID format")),
    };
    
    let start = if start_str == "-" {
        StreamId::min()
    } else {
        match StreamId::from_string(&start_str) {
            Some(id) => id,
            None => return Ok(RespFrame::error("ERR Invalid stream ID specified as stream command argument")),
        }
    };
    
    // Parse end ID
    let end_str = match &parts[3] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
        _ => return Ok(RespFrame::error("ERR invalid end ID format")),
    };
    
    let end = if end_str == "+" {
        StreamId::max()
    } else {
        match StreamId::from_string(&end_str) {
            Some(id) => id,
            None => return Ok(RespFrame::error("ERR Invalid stream ID specified as stream command argument")),
        }
    };
    
    // Parse optional COUNT - support multiple Redis client syntax patterns
    let count = if parts.len() >= 6 {
        // Check for "COUNT n" pattern
        let count_keyword = match &parts[4] {
            RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_uppercase(),
            _ => String::new(),
        };
        
        if count_keyword == "COUNT" {
            match &parts[5] {
                RespFrame::BulkString(Some(bytes)) => {
                    match String::from_utf8_lossy(bytes).parse::<usize>() {
                        Ok(n) => Some(n),
                        Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                    }
                }
                _ => return Ok(RespFrame::error("ERR invalid count format")),
            }
        } else {
            None
        }
    } else {
        None
    };
    
    // Get entries
    let entries = storage.xrange(db, key, start, end, count)?;
    
    // Format response
    let frames: Vec<RespFrame> = entries.into_iter().map(|entry| {
        let mut entry_frames = Vec::new();
        
        // Add ID
        entry_frames.push(RespFrame::from_string(entry.id.to_string()));
        
        // Add fields array
        let mut field_frames = Vec::new();
        for (field, value) in entry.fields {
            field_frames.push(RespFrame::from_bytes(field));
            field_frames.push(RespFrame::from_bytes(value));
        }
        
        entry_frames.push(RespFrame::Array(Some(field_frames)));
        
        RespFrame::Array(Some(entry_frames))
    }).collect();
    
    Ok(RespFrame::Array(Some(frames)))
}

/// Handle XREVRANGE command - Get entries in reverse order
pub fn handle_xrevrange(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 4 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xrevrange' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Note: For XREVRANGE, the order of arguments is reversed (end first, then start)
    // Parse end ID (first in XREVRANGE)
    let end_str = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
        _ => return Ok(RespFrame::error("ERR invalid end ID format")),
    };
    
    let end = if end_str == "+" {
        StreamId::max()
    } else {
        match StreamId::from_string(&end_str) {
            Some(id) => id,
            None => return Ok(RespFrame::error("ERR Invalid stream ID specified as stream command argument")),
        }
    };
    
    // Parse start ID (second in XREVRANGE)
    let start_str = match &parts[3] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
        _ => return Ok(RespFrame::error("ERR invalid start ID format")),
    };
    
    let start = if start_str == "-" {
        StreamId::min()
    } else {
        match StreamId::from_string(&start_str) {
            Some(id) => id,
            None => return Ok(RespFrame::error("ERR Invalid stream ID specified as stream command argument")),
        }
    };
    
    // Parse optional COUNT
    let count = if parts.len() >= 6 && parts[4].as_bulk_string_lossy().map(|s| s.to_uppercase()) == Some("COUNT".to_string()) {
        match &parts[5] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<usize>() {
                    Ok(n) => Some(n),
                    Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid count format")),
        }
    } else if parts.len() == 5 {
        // Try to parse as a number
        match parts[4].as_bulk_string_lossy() {
            Some(s) if s.parse::<usize>().is_ok() => Some(s.parse::<usize>().unwrap()),
            _ => None,
        }
    } else {
        None
    };
    
    // Get entries in reverse
    let entries = storage.xrevrange(db, key, start, end, count)?;
    
    // Format response - same as XRANGE
    let frames: Vec<RespFrame> = entries.into_iter().map(|entry| {
        let mut entry_frames = Vec::new();
        
        // Add ID
        entry_frames.push(RespFrame::from_string(entry.id.to_string()));
        
        // Add fields array
        let mut field_frames = Vec::new();
        for (field, value) in entry.fields {
            field_frames.push(RespFrame::from_bytes(field));
            field_frames.push(RespFrame::from_bytes(value));
        }
        
        entry_frames.push(RespFrame::Array(Some(field_frames)));
        
        RespFrame::Array(Some(entry_frames))
    }).collect();
    
    Ok(RespFrame::Array(Some(frames)))
}

/// Handle XLEN command - Get stream length
pub fn handle_xlen(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xlen' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let len = storage.xlen(db, key)?;
    Ok(RespFrame::Integer(len as i64))
}

/// Handle XREAD command - Read from multiple streams
pub fn handle_xread(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 4 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xread' command"));
    }
    
    let mut i = 1;
    let mut count: Option<usize> = None;
    let mut block_ms: Option<u64> = None;
    
    // Parse optional COUNT and BLOCK
    while i < parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => {
                let arg = String::from_utf8_lossy(bytes).to_uppercase();
                
                if arg == "COUNT" && i + 1 < parts.len() {
                    match &parts[i + 1] {
                        RespFrame::BulkString(Some(bytes)) => {
                            match String::from_utf8_lossy(bytes).parse::<usize>() {
                                Ok(n) => count = Some(n),
                                Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                            }
                        }
                        _ => return Ok(RespFrame::error("ERR invalid count value")),
                    }
                    i += 2;
                } else if arg == "BLOCK" && i + 1 < parts.len() {
                    match &parts[i + 1] {
                        RespFrame::BulkString(Some(bytes)) => {
                            match String::from_utf8_lossy(bytes).parse::<u64>() {
                                Ok(n) => block_ms = Some(n),
                                Err(_) => return Ok(RespFrame::error("ERR timeout is not a float or out of range")),
                            }
                        }
                        _ => return Ok(RespFrame::error("ERR invalid timeout value")),
                    }
                    i += 2;
                } else if arg == "STREAMS" {
                    i += 1;
                    break;
                } else {
                    return Ok(RespFrame::error("ERR syntax error"));
                }
            }
            _ => return Ok(RespFrame::error("ERR syntax error")),
        }
    }
    
    // Now we should be at STREAMS keyword
    if i >= parts.len() {
        return Ok(RespFrame::error("ERR syntax error"));
    }
    
    // Calculate number of keys and validate
    let remaining = parts.len() - i;
    if remaining % 2 != 0 || remaining == 0 {
        return Ok(RespFrame::error("ERR Unbalanced XREAD list of streams: for each stream key an ID or '$' must be specified."));
    }
    
    let num_keys = remaining / 2;
    
    // Parse keys and IDs
    let mut keys_and_ids = Vec::new();
    for j in 0..num_keys {
        let key = match &parts[i + j] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        let id_str = match &parts[i + num_keys + j] {
            RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
            _ => return Ok(RespFrame::error("ERR invalid ID format")),
        };
        
        // Parse ID
        let after_id = if id_str == "$" {
            // For $, we want the actual last ID to read AFTER it
            let all_entries = storage.xrange(db, key, StreamId::min(), StreamId::max(), None)?;
            if let Some(last_entry) = all_entries.last() {
                last_entry.id.clone()
            } else {
                // Empty stream - use 0-0 to read all entries
                StreamId::new(0, 0)
            }
        } else if id_str == "0" || id_str == "0-0" {
            // Read everything after 0-0
            StreamId::new(0, 0)
        } else {
            match StreamId::from_string(&id_str) {
                Some(id) => id,
                None => return Ok(RespFrame::error("ERR Invalid stream ID specified as stream command argument")),
            }
        };
        
        keys_and_ids.push((key, after_id));
    }
    
    // Convert to the format expected by storage.xread  
    let keys_and_ids_refs: Vec<(&[u8], StreamId)> = keys_and_ids.iter()
        .map(|(k, id)| (k.as_slice(), id.clone()))
        .collect();
    
    // Read entries after the specified IDs
    let results = storage.xread(db, keys_and_ids_refs, count, None)?;
    
    // Format response
    if results.is_empty() {
        return Ok(RespFrame::Array(Some(Vec::new())));
    }
    
    // Format non-empty results
    let mut stream_results = Vec::new();
    
    for (key, entries) in results {
        if !entries.is_empty() {
            let mut stream_result = Vec::new();
            
            // Stream key
            stream_result.push(RespFrame::from_bytes(key));
            
            // Entries array
            let entry_frames: Vec<RespFrame> = entries.into_iter().map(|entry| {
                let mut entry_frames = Vec::new();
                
                // Add ID
                entry_frames.push(RespFrame::from_string(entry.id.to_string()));
                
                // Add fields array
                let mut field_frames = Vec::new();
                for (field, value) in entry.fields {
                    field_frames.push(RespFrame::from_bytes(field));
                    field_frames.push(RespFrame::from_bytes(value));
                }
                
                entry_frames.push(RespFrame::Array(Some(field_frames)));
                
                RespFrame::Array(Some(entry_frames))
            }).collect();
            
            stream_result.push(RespFrame::Array(Some(entry_frames)));
            stream_results.push(RespFrame::Array(Some(stream_result)));
        }
    }
    
    Ok(RespFrame::Array(Some(stream_results)))
}

/// Handle XTRIM command - Trim stream to maximum length
pub fn handle_xtrim(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 4 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xtrim' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Check for MAXLEN keyword
    let strategy = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_uppercase(),
        _ => return Ok(RespFrame::error("ERR invalid strategy format")),
    };
    
    if strategy != "MAXLEN" {
        return Ok(RespFrame::error("ERR syntax error"));
    }
    
    // Parse count - handle all Redis client library patterns
    let max_len = if parts.len() == 5 {
        // Pattern: XTRIM stream MAXLEN ~ 5 (default redis-py) or XTRIM stream MAXLEN = 5
        let modifier = match &parts[3] {
            RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
            _ => return Ok(RespFrame::error("ERR syntax error")),
        };
        
        // Accept ~, =, or direct number as modifier
        if modifier == "~" || modifier == "=" {
            // Parse the actual count from the next argument
            match &parts[4] {
                RespFrame::BulkString(Some(bytes)) => {
                    match String::from_utf8_lossy(bytes).parse::<usize>() {
                        Ok(n) => n,
                        Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                    }
                }
                _ => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
            }
        } else if let Ok(n) = modifier.parse::<usize>() {
            // The "modifier" is actually the count (Pattern: XTRIM stream MAXLEN 5 something)
            n
        } else {
            return Ok(RespFrame::error("ERR syntax error"));
        }
    } else if parts.len() == 4 {
        // Pattern: XTRIM stream MAXLEN 5  
        match &parts[3] {
            RespFrame::BulkString(Some(bytes)) => {
                let arg = String::from_utf8_lossy(bytes);
                // Handle case where this might be ~ or = without a count
                if arg == "~" || arg == "=" {
                    return Ok(RespFrame::error("ERR syntax error"));
                }
                match arg.parse::<usize>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                }
            }
            _ => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
        }
    } else {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xtrim' command"));
    };
    
    let trimmed = storage.xtrim(db, key, max_len)?;
    Ok(RespFrame::Integer(trimmed as i64))
}

/// Handle XDEL command - Delete entries from stream
pub fn handle_xdel(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xdel' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Parse IDs to delete
    let mut ids = Vec::new();
    for i in 2..parts.len() {
        let id_str = match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
            _ => return Ok(RespFrame::error("ERR invalid ID format")),
        };
        
        match StreamId::from_string(&id_str) {
            Some(id) => ids.push(id),
            None => return Ok(RespFrame::error("ERR Invalid stream ID specified as stream command argument")),
        }
    }
    
    let deleted = storage.xdel(db, key, ids)?;
    Ok(RespFrame::Integer(deleted as i64))
}