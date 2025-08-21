//! Consumer group command implementations for Redis Streams
//! 
//! Implements XGROUP, XREADGROUP, XACK, XPENDING, XCLAIM, and XAUTOCLAIM commands

use crate::error::Result;
use crate::protocol::RespFrame;
use crate::storage::{StorageEngine, GetResult};
use crate::storage::stream::{StreamId, Stream};
use crate::storage::value::Value;
use std::sync::Arc;
use std::time::{SystemTime, Duration};

/// Handle XINFO command
pub fn handle_xinfo(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xinfo' command"));
    }
    
    let subcommand = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_uppercase(),
        _ => return Ok(RespFrame::error("ERR invalid subcommand format")),
    };
    
    match subcommand.as_str() {
        "STREAM" => handle_xinfo_stream(storage, db, parts),
        "GROUPS" => handle_xinfo_groups(storage, db, parts),
        "CONSUMERS" => handle_xinfo_consumers(storage, db, parts),
        "HELP" => handle_xinfo_help(),
        _ => Ok(RespFrame::error(format!("ERR unknown subcommand '{}'", subcommand))),
    }
}

/// Handle XINFO STREAM
fn handle_xinfo_stream(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xinfo stream' command"));
    }
    
    let key = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Get the stream
    let stream = match storage.get(db, key)? {
        GetResult::Found(Value::Stream(stream)) => stream,
        GetResult::Found(_) => return Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value")),
        _ => return Ok(RespFrame::error("ERR no such key")),
    };
    
    // Get stream info
    let length = stream.len();
    let first_entry = stream.first_entry();
    let last_entry = stream.last_entry();
    let groups = stream.list_consumer_groups();
    
    let mut info = Vec::new();
    
    // Length
    info.push(RespFrame::from_string("length"));
    info.push(RespFrame::Integer(length as i64));
    
    // Radix tree keys (simplified)
    info.push(RespFrame::from_string("radix-tree-keys"));
    info.push(RespFrame::Integer(1));
    
    // Radix tree nodes (simplified)
    info.push(RespFrame::from_string("radix-tree-nodes"));
    info.push(RespFrame::Integer(2));
    
    // Last generated ID
    info.push(RespFrame::from_string("last-generated-id"));
    if let Some(ref entry) = last_entry {
        info.push(RespFrame::from_string(entry.id.to_string()));
    } else {
        info.push(RespFrame::from_string("0-0"));
    }
    
    // Groups count
    info.push(RespFrame::from_string("groups"));
    info.push(RespFrame::Integer(groups.len() as i64));
    
    // First entry
    info.push(RespFrame::from_string("first-entry"));
    if let Some(entry) = first_entry {
        let mut entry_info = Vec::new();
        entry_info.push(RespFrame::from_string(entry.id.to_string()));
        
        let mut fields = Vec::new();
        for (field, value) in entry.fields {
            fields.push(RespFrame::from_bytes(field));
            fields.push(RespFrame::from_bytes(value));
        }
        entry_info.push(RespFrame::Array(Some(fields)));
        
        info.push(RespFrame::Array(Some(entry_info)));
    } else {
        info.push(RespFrame::null_array());
    }
    
    // Last entry
    info.push(RespFrame::from_string("last-entry"));
    if let Some(entry) = last_entry {
        let mut entry_info = Vec::new();
        entry_info.push(RespFrame::from_string(entry.id.to_string()));
        
        let mut fields = Vec::new();
        for (field, value) in entry.fields {
            fields.push(RespFrame::from_bytes(field));
            fields.push(RespFrame::from_bytes(value));
        }
        entry_info.push(RespFrame::Array(Some(fields)));
        
        info.push(RespFrame::Array(Some(entry_info)));
    } else {
        info.push(RespFrame::null_array());
    }
    
    Ok(RespFrame::Array(Some(info)))
}

/// Handle XINFO GROUPS
fn handle_xinfo_groups(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xinfo groups' command"));
    }
    
    let key = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Get the stream
    let stream = match storage.get(db, key)? {
        GetResult::Found(Value::Stream(stream)) => stream,
        GetResult::Found(_) => return Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value")),
        _ => return Ok(RespFrame::Array(Some(Vec::new()))),
    };
    
    // Get all groups
    let groups = stream.list_consumer_groups();
    
    let mut group_infos = Vec::new();
    
    for group in groups {
        let mut info = Vec::new();
        
        // Group name
        info.push(RespFrame::from_string("name"));
        info.push(RespFrame::from_string(group.name.clone()));
        
        // Consumers count
        info.push(RespFrame::from_string("consumers"));
        let consumer_count = *group.consumer_count.lock().unwrap();
        info.push(RespFrame::Integer(consumer_count as i64));
        
        // Pending count
        info.push(RespFrame::from_string("pending"));
        let pending_count = *group.total_pending.lock().unwrap();
        info.push(RespFrame::Integer(pending_count as i64));
        
        // Last delivered ID
        info.push(RespFrame::from_string("last-delivered-id"));
        let last_id = group.get_last_id();
        info.push(RespFrame::from_string(last_id.to_string()));
        
        group_infos.push(RespFrame::Array(Some(info)));
    }
    
    Ok(RespFrame::Array(Some(group_infos)))
}

/// Handle XINFO CONSUMERS
fn handle_xinfo_consumers(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 4 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xinfo consumers' command"));
    }
    
    let key = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let group_name = match &parts[3] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
        _ => return Ok(RespFrame::error("ERR invalid group name format")),
    };
    
    // Get the stream
    let stream = match storage.get(db, key)? {
        GetResult::Found(Value::Stream(stream)) => stream,
        GetResult::Found(_) => return Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value")),
        _ => return Ok(RespFrame::Array(Some(Vec::new()))),
    };
    
    // Get the consumer group
    let group = match stream.get_consumer_group(&group_name) {
        Some(group) => group,
        None => return Ok(RespFrame::error(format!("NOGROUP No such consumer group {} for stream", group_name))),
    };
    
    // Get consumers
    let consumers = group.consumers.read().unwrap();
    
    let mut consumer_infos = Vec::new();
    
    for (name, consumer) in consumers.iter() {
        let mut info = Vec::new();
        
        // Consumer name
        info.push(RespFrame::from_string("name"));
        info.push(RespFrame::from_string(name.clone()));
        
        // Pending count
        info.push(RespFrame::from_string("pending"));
        info.push(RespFrame::Integer(consumer.pending_count as i64));
        
        // Idle time in milliseconds
        info.push(RespFrame::from_string("idle"));
        let idle_ms = SystemTime::now()
            .duration_since(consumer.last_seen)
            .unwrap_or_default()
            .as_millis() as i64;
        info.push(RespFrame::Integer(idle_ms));
        
        consumer_infos.push(RespFrame::Array(Some(info)));
    }
    
    Ok(RespFrame::Array(Some(consumer_infos)))
}

/// Handle XINFO HELP
fn handle_xinfo_help() -> Result<RespFrame> {
    let help_text = vec![
        "XINFO <subcommand> [<arg> [value] [opt] ...]. Subcommands are:",
        "STREAM <key>",
        "    Show information about a stream.",
        "GROUPS <key>",
        "    Show the consumer groups of a stream.",
        "CONSUMERS <key> <groupname>",
        "    Show consumers of a consumer group.",
        "HELP",
        "    Print this help.",
    ];
    
    let frames: Vec<RespFrame> = help_text
        .iter()
        .map(|line| RespFrame::from_string(line.to_string()))
        .collect();
    
    Ok(RespFrame::Array(Some(frames)))
}

/// Handle XGROUP command family
pub fn handle_xgroup(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xgroup' command"));
    }
    
    let subcommand = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_uppercase(),
        _ => return Ok(RespFrame::error("ERR invalid subcommand format")),
    };
    
    match subcommand.as_str() {
        "CREATE" => handle_xgroup_create(storage, db, parts),
        "DESTROY" => handle_xgroup_destroy(storage, db, parts),
        "CREATECONSUMER" => handle_xgroup_createconsumer(storage, db, parts),
        "DELCONSUMER" => handle_xgroup_delconsumer(storage, db, parts),
        "SETID" => handle_xgroup_setid(storage, db, parts),
        "HELP" => handle_xgroup_help(),
        _ => Ok(RespFrame::error(format!("ERR unknown subcommand '{}'", subcommand))),
    }
}

/// Handle XGROUP CREATE
fn handle_xgroup_create(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    // XGROUP CREATE key group id|$ [MKSTREAM]
    if parts.len() < 5 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xgroup create' command"));
    }
    
    let key = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().to_vec(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let group_name = match &parts[3] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
        _ => return Ok(RespFrame::error("ERR invalid group name format")),
    };
    
    let id_str = match &parts[4] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
        _ => return Ok(RespFrame::error("ERR invalid ID format")),
    };
    
    // Check for MKSTREAM option
    let mkstream = parts.len() > 5 && match &parts[5] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_uppercase() == "MKSTREAM",
        _ => false,
    };
    
    // Get or create the stream
    let stream = match storage.get(db, &key)? {
        GetResult::Found(Value::Stream(stream)) => stream,
        GetResult::Found(_) => return Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value")),
        GetResult::NotFound | GetResult::Expired => {
            if mkstream {
                // Create empty stream
                let new_stream = Stream::new();
                storage.set_value(db, key.clone(), Value::Stream(new_stream.clone()), None)?;
                new_stream
            } else {
                // Stream doesn't exist and MKSTREAM not specified
                return Ok(RespFrame::error("ERR The XGROUP subcommand requires the key to exist"));
            }
        }
        GetResult::WrongType => return Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value")),
    };
    
    // Parse start ID
    let start_id = if id_str == "$" {
        // Use the last entry's ID or 0-0 if empty
        stream.last_entry()
            .map(|e| e.id)
            .unwrap_or(StreamId::new(0, 0))
    } else if id_str == "0" || id_str == "0-0" {
        StreamId::new(0, 0)
    } else {
        match StreamId::from_string(&id_str) {
            Some(id) => id,
            None => return Ok(RespFrame::error("ERR Invalid stream ID specified as stream command argument")),
        }
    };
    
    // Create the consumer group
    match stream.create_consumer_group(group_name, start_id) {
        Ok(()) => Ok(RespFrame::ok()),
        Err(e) if e.contains("already exists") => Ok(RespFrame::error("BUSYGROUP Consumer Group name already exists")),
        Err(e) => Ok(RespFrame::error(format!("ERR {}", e))),
    }
}

/// Handle XGROUP DESTROY
fn handle_xgroup_destroy(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 4 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xgroup destroy' command"));
    }
    
    let key = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let group_name = match &parts[3] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
        _ => return Ok(RespFrame::error("ERR invalid group name format")),
    };
    
    // Get the stream
    let stream = match storage.get(db, key)? {
        GetResult::Found(Value::Stream(stream)) => stream,
        GetResult::Found(_) => return Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value")),
        _ => return Ok(RespFrame::Integer(0)), // Key doesn't exist
    };
    
    // Destroy the group
    let destroyed = stream.destroy_consumer_group(&group_name);
    Ok(RespFrame::Integer(if destroyed { 1 } else { 0 }))
}

/// Handle XGROUP CREATECONSUMER
fn handle_xgroup_createconsumer(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 5 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xgroup createconsumer' command"));
    }
    
    let key = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let group_name = match &parts[3] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
        _ => return Ok(RespFrame::error("ERR invalid group name format")),
    };
    
    let consumer_name = match &parts[4] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
        _ => return Ok(RespFrame::error("ERR invalid consumer name format")),
    };
    
    // Get the stream
    let stream = match storage.get(db, key)? {
        GetResult::Found(Value::Stream(stream)) => stream,
        GetResult::Found(_) => return Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value")),
        _ => return Ok(RespFrame::error("ERR no such key")),
    };
    
    // Get the consumer group
    let group = match stream.get_consumer_group(&group_name) {
        Some(group) => group,
        None => return Ok(RespFrame::error(format!("NOGROUP No such consumer group {} for stream", group_name))),
    };
    
    // Create the consumer
    let created = group.create_consumer(consumer_name);
    Ok(RespFrame::Integer(if created { 1 } else { 0 }))
}

/// Handle XGROUP DELCONSUMER
fn handle_xgroup_delconsumer(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 5 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xgroup delconsumer' command"));
    }
    
    let key = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let group_name = match &parts[3] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
        _ => return Ok(RespFrame::error("ERR invalid group name format")),
    };
    
    let consumer_name = match &parts[4] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
        _ => return Ok(RespFrame::error("ERR invalid consumer name format")),
    };
    
    // Get the stream
    let stream = match storage.get(db, key)? {
        GetResult::Found(Value::Stream(stream)) => stream,
        GetResult::Found(_) => return Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value")),
        _ => return Ok(RespFrame::Integer(0)),
    };
    
    // Get the consumer group
    let group = match stream.get_consumer_group(&group_name) {
        Some(group) => group,
        None => return Ok(RespFrame::Integer(0)),
    };
    
    // Delete the consumer and return pending count
    let pending_removed = group.delete_consumer(&consumer_name);
    Ok(RespFrame::Integer(pending_removed as i64))
}

/// Handle XGROUP SETID
fn handle_xgroup_setid(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 5 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xgroup setid' command"));
    }
    
    let key = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let group_name = match &parts[3] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
        _ => return Ok(RespFrame::error("ERR invalid group name format")),
    };
    
    let id_str = match &parts[4] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
        _ => return Ok(RespFrame::error("ERR invalid ID format")),
    };
    
    // Get the stream
    let stream = match storage.get(db, key)? {
        GetResult::Found(Value::Stream(stream)) => stream,
        GetResult::Found(_) => return Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value")),
        _ => return Ok(RespFrame::error("ERR no such key")),
    };
    
    // Parse the new ID
    let new_id = if id_str == "$" {
        stream.last_entry()
            .map(|e| e.id)
            .unwrap_or(StreamId::new(0, 0))
    } else {
        match StreamId::from_string(&id_str) {
            Some(id) => id,
            None => return Ok(RespFrame::error("ERR Invalid stream ID specified as stream command argument")),
        }
    };
    
    // Get the consumer group
    let group = match stream.get_consumer_group(&group_name) {
        Some(group) => group,
        None => return Ok(RespFrame::error(format!("NOGROUP No such consumer group {} for stream", group_name))),
    };
    
    // Set the ID
    group.set_id(new_id);
    Ok(RespFrame::ok())
}

/// Handle XGROUP HELP
fn handle_xgroup_help() -> Result<RespFrame> {
    let help_text = vec![
        "XGROUP <subcommand> [<arg> [value] [opt] ...]. Subcommands are:",
        "CREATE <key> <groupname> <id or $> [MKSTREAM]",
        "    Create a new consumer group.",
        "SETID <key> <groupname> <id or $>",
        "    Set the current group ID.",
        "DESTROY <key> <groupname>",
        "    Remove the consumer group.",
        "CREATECONSUMER <key> <groupname> <consumername>",
        "    Create a new consumer in the group.",
        "DELCONSUMER <key> <groupname> <consumername>",
        "    Remove the consumer from the group.",
        "HELP",
        "    Print this help.",
    ];
    
    let frames: Vec<RespFrame> = help_text
        .iter()
        .map(|line| RespFrame::from_string(line.to_string()))
        .collect();
    
    Ok(RespFrame::Array(Some(frames)))
}

/// Handle XREADGROUP command
pub fn handle_xreadgroup(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    // XREADGROUP GROUP group consumer [COUNT count] [BLOCK ms] [NOACK] STREAMS key [key ...] id [id ...]
    if parts.len() < 6 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xreadgroup' command"));
    }
    
    let mut i = 1;
    let mut group_name = String::new();
    let mut consumer_name = String::new();
    let mut count: Option<usize> = None;
    let mut block_ms: Option<u64> = None;
    let mut noack = false;
    
    // Parse GROUP keyword
    match &parts[i] {
        RespFrame::BulkString(Some(bytes)) if String::from_utf8_lossy(bytes).to_uppercase() == "GROUP" => {
            i += 1;
        }
        _ => return Ok(RespFrame::error("ERR syntax error")),
    }
    
    // Parse group name
    if i < parts.len() {
        group_name = match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
            _ => return Ok(RespFrame::error("ERR invalid group name")),
        };
        i += 1;
    }
    
    // Parse consumer name
    if i < parts.len() {
        consumer_name = match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
            _ => return Ok(RespFrame::error("ERR invalid consumer name")),
        };
        i += 1;
    }
    
    // Parse optional arguments
    while i < parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => {
                let arg = String::from_utf8_lossy(bytes).to_uppercase();
                
                if arg == "COUNT" && i + 1 < parts.len() {
                    count = match &parts[i + 1] {
                        RespFrame::BulkString(Some(bytes)) => {
                            String::from_utf8_lossy(bytes).parse::<usize>().ok()
                        }
                        _ => None,
                    };
                    i += 2;
                } else if arg == "BLOCK" && i + 1 < parts.len() {
                    block_ms = match &parts[i + 1] {
                        RespFrame::BulkString(Some(bytes)) => {
                            String::from_utf8_lossy(bytes).parse::<u64>().ok()
                        }
                        _ => None,
                    };
                    i += 2;
                } else if arg == "NOACK" {
                    noack = true;
                    i += 1;
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
    
    // Parse keys and IDs
    if i >= parts.len() {
        return Ok(RespFrame::error("ERR syntax error"));
    }
    
    let remaining = parts.len() - i;
    if remaining % 2 != 0 || remaining == 0 {
        return Ok(RespFrame::error("ERR Unbalanced XREADGROUP list of streams"));
    }
    
    let num_keys = remaining / 2;
    let mut results = Vec::new();
    
    for j in 0..num_keys {
        let key = match &parts[i + j] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        let id_str = match &parts[i + num_keys + j] {
            RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
            _ => return Ok(RespFrame::error("ERR invalid ID format")),
        };
        
        // Get the stream
        let stream = match storage.get(db, key)? {
            GetResult::Found(Value::Stream(stream)) => stream,
            GetResult::Found(_) => return Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value")),
            _ => continue, // Skip non-existent keys
        };
        
        // Parse the ID
        let after_id = if id_str == ">" {
            StreamId::max() // Special marker for new entries only
        } else if id_str == "0" || id_str == "0-0" {
            StreamId::new(0, 0)
        } else {
            match StreamId::from_string(&id_str) {
                Some(id) => id,
                None => return Ok(RespFrame::error("ERR Invalid stream ID specified")),
            }
        };
        
        // Read entries for the group
        match stream.read_group(&group_name, &consumer_name, after_id, count, noack) {
            Ok(entries) if !entries.is_empty() => {
                let mut stream_result = Vec::new();
                
                // Stream key
                stream_result.push(RespFrame::from_bytes(key.to_vec()));
                
                // Format entries
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
                results.push(RespFrame::Array(Some(stream_result)));
            }
            Ok(_) => {} // Empty result, skip
            Err(e) if e.contains("NOGROUP") => {
                return Ok(RespFrame::error(e));
            }
            Err(_) => {} // Other errors, skip
        }
    }
    
    // Handle blocking if specified and no results
    if results.is_empty() && block_ms.is_some() {
        // For now, return empty array (blocking not implemented)
        return Ok(RespFrame::null_array());
    }
    
    Ok(RespFrame::Array(Some(results)))
}

/// Handle XACK command
pub fn handle_xack(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 4 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xack' command"));
    }
    
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let group_name = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
        _ => return Ok(RespFrame::error("ERR invalid group name format")),
    };
    
    // Parse IDs to acknowledge
    let mut ids = Vec::new();
    for i in 3..parts.len() {
        let id_str = match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
            _ => return Ok(RespFrame::error("ERR invalid ID format")),
        };
        
        match StreamId::from_string(&id_str) {
            Some(id) => ids.push(id),
            None => return Ok(RespFrame::error("ERR Invalid stream ID specified")),
        }
    }
    
    // Get the stream
    let stream = match storage.get(db, key)? {
        GetResult::Found(Value::Stream(stream)) => stream,
        GetResult::Found(_) => return Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value")),
        _ => return Ok(RespFrame::Integer(0)),
    };
    
    // Acknowledge messages
    match stream.acknowledge_messages(&group_name, &ids) {
        Ok(count) => Ok(RespFrame::Integer(count as i64)),
        Err(e) if e.contains("NOGROUP") => Ok(RespFrame::Integer(0)),
        Err(e) => Ok(RespFrame::error(e)),
    }
}

/// Handle XPENDING command
pub fn handle_xpending(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xpending' command"));
    }
    
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let group_name = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
        _ => return Ok(RespFrame::error("ERR invalid group name format")),
    };
    
    // Get the stream
    let stream = match storage.get(db, key)? {
        GetResult::Found(Value::Stream(stream)) => stream,
        GetResult::Found(_) => return Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value")),
        _ => return Ok(RespFrame::null_array()),
    };
    
    // Get the consumer group
    let group = match stream.get_consumer_group(&group_name) {
        Some(group) => group,
        None => return Ok(RespFrame::null_array()),
    };
    
    if parts.len() == 3 {
        // Simple XPENDING - return summary
        let info = group.get_pending_info();
        
        let mut response = Vec::new();
        
        // Total pending
        response.push(RespFrame::Integer(info.count as i64));
        
        // Min ID
        if let Some(min_id) = info.min_id {
            response.push(RespFrame::from_string(min_id.to_string()));
        } else {
            response.push(RespFrame::null_bulk());
        }
        
        // Max ID
        if let Some(max_id) = info.max_id {
            response.push(RespFrame::from_string(max_id.to_string()));
        } else {
            response.push(RespFrame::null_bulk());
        }
        
        // Consumer list
        let consumer_frames: Vec<RespFrame> = info.consumers
            .into_iter()
            .map(|(name, count)| {
                RespFrame::Array(Some(vec![
                    RespFrame::from_string(name),
                    RespFrame::Integer(count as i64),
                ]))
            })
            .collect();
        response.push(RespFrame::Array(Some(consumer_frames)));
        
        Ok(RespFrame::Array(Some(response)))
    } else {
        // Extended XPENDING with range
        // Parse additional arguments (start, end, count, [consumer])
        if parts.len() < 6 {
            return Ok(RespFrame::error("ERR syntax error"));
        }
        
        let start_str = match &parts[3] {
            RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
            _ => return Ok(RespFrame::error("ERR invalid start ID")),
        };
        
        let end_str = match &parts[4] {
            RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
            _ => return Ok(RespFrame::error("ERR invalid end ID")),
        };
        
        let count = match &parts[5] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<usize>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR value is not an integer")),
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid count")),
        };
        
        let consumer = if parts.len() > 6 {
            match &parts[6] {
                RespFrame::BulkString(Some(bytes)) => Some(String::from_utf8_lossy(bytes).to_string()),
                _ => None,
            }
        } else {
            None
        };
        
        // Parse IDs
        let start = if start_str == "-" {
            None
        } else {
            StreamId::from_string(&start_str)
        };
        
        let end = if end_str == "+" {
            None
        } else {
            StreamId::from_string(&end_str)
        };
        
        // Get pending entries
        let entries = group.get_pending_range(start, end, count, consumer.as_deref());
        
        let frames: Vec<RespFrame> = entries
            .into_iter()
            .map(|entry| {
                RespFrame::Array(Some(vec![
                    RespFrame::from_string(entry.id.to_string()),
                    RespFrame::from_string(entry.consumer),
                    RespFrame::Integer(entry.idle_time as i64),
                    RespFrame::Integer(entry.delivery_count as i64),
                ]))
            })
            .collect();
        
        Ok(RespFrame::Array(Some(frames)))
    }
}

/// Handle XCLAIM command
pub fn handle_xclaim(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    // XCLAIM key group consumer min-idle-time id [id ...] [IDLE ms] [TIME ms] [RETRYCOUNT n] [FORCE] [JUSTID]
    if parts.len() < 6 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xclaim' command"));
    }
    
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let group_name = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
        _ => return Ok(RespFrame::error("ERR invalid group name")),
    };
    
    let consumer_name = match &parts[3] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
        _ => return Ok(RespFrame::error("ERR invalid consumer name")),
    };
    
    let min_idle_ms = match &parts[4] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<u64>() {
                Ok(n) => n,
                Err(_) => return Ok(RespFrame::error("ERR Invalid min-idle-time")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid min-idle-time")),
    };
    
    // Parse IDs and options
    let mut ids = Vec::new();
    let mut force = false;
    let mut justid = false;
    let mut i = 5;
    
    while i < parts.len() {
        let arg_str = match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
            _ => return Ok(RespFrame::error("ERR syntax error")),
        };
        
        let arg_upper = arg_str.to_uppercase();
        
        if arg_upper == "FORCE" {
            force = true;
            i += 1;
        } else if arg_upper == "JUSTID" {
            justid = true;
            i += 1;
        } else if arg_upper == "IDLE" || arg_upper == "TIME" || arg_upper == "RETRYCOUNT" {
            // Skip these options for now (not implemented)
            i += 2;
        } else {
            // Must be an ID
            match StreamId::from_string(&arg_str) {
                Some(id) => {
                    ids.push(id);
                    i += 1;
                }
                None => return Ok(RespFrame::error("ERR Invalid stream ID")),
            }
        }
    }
    
    if ids.is_empty() {
        return Ok(RespFrame::error("ERR syntax error"));
    }
    
    // Get the stream
    let stream = match storage.get(db, key)? {
        GetResult::Found(Value::Stream(stream)) => stream,
        GetResult::Found(_) => return Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value")),
        _ => return Ok(RespFrame::Array(Some(Vec::new()))),
    };
    
    // Claim messages
    match stream.claim_messages(&group_name, &consumer_name, min_idle_ms, &ids, force) {
        Ok(entries) => {
            if justid {
                // Return just IDs
                let id_frames: Vec<RespFrame> = entries
                    .into_iter()
                    .map(|e| RespFrame::from_string(e.id.to_string()))
                    .collect();
                Ok(RespFrame::Array(Some(id_frames)))
            } else {
                // Return full entries
                let entry_frames: Vec<RespFrame> = entries
                    .into_iter()
                    .map(|entry| {
                        let mut frames = Vec::new();
                        frames.push(RespFrame::from_string(entry.id.to_string()));
                        
                        let mut field_frames = Vec::new();
                        for (field, value) in entry.fields {
                            field_frames.push(RespFrame::from_bytes(field));
                            field_frames.push(RespFrame::from_bytes(value));
                        }
                        frames.push(RespFrame::Array(Some(field_frames)));
                        
                        RespFrame::Array(Some(frames))
                    })
                    .collect();
                Ok(RespFrame::Array(Some(entry_frames)))
            }
        }
        Err(e) => Ok(RespFrame::error(e)),
    }
}

/// Handle XAUTOCLAIM command  
pub fn handle_xautoclaim(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    // XAUTOCLAIM key group consumer min-idle-time start [COUNT count] [JUSTID]
    if parts.len() < 6 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xautoclaim' command"));
    }
    
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let group_name = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
        _ => return Ok(RespFrame::error("ERR invalid group name")),
    };
    
    let consumer_name = match &parts[3] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
        _ => return Ok(RespFrame::error("ERR invalid consumer name")),
    };
    
    let min_idle_ms = match &parts[4] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<u64>() {
                Ok(n) => n,
                Err(_) => return Ok(RespFrame::error("ERR Invalid min-idle-time")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid min-idle-time")),
    };
    
    let start_str = match &parts[5] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
        _ => return Ok(RespFrame::error("ERR invalid start ID")),
    };
    
    let start_id = if start_str == "0" || start_str == "0-0" {
        StreamId::new(0, 0)
    } else {
        match StreamId::from_string(&start_str) {
            Some(id) => id,
            None => return Ok(RespFrame::error("ERR Invalid stream ID")),
        }
    };
    
    // Parse optional COUNT and JUSTID
    let mut count = 100; // Default
    let mut justid = false;
    let mut i = 6;
    
    while i < parts.len() {
        let arg = match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_uppercase(),
            _ => break,
        };
        
        if arg == "COUNT" && i + 1 < parts.len() {
            count = match &parts[i + 1] {
                RespFrame::BulkString(Some(bytes)) => {
                    match String::from_utf8_lossy(bytes).parse() {
                        Ok(n) => n,
                        Err(_) => return Ok(RespFrame::error("ERR value is not an integer")),
                    }
                }
                _ => return Ok(RespFrame::error("ERR syntax error")),
            };
            i += 2;
        } else if arg == "JUSTID" {
            justid = true;
            i += 1;
        } else {
            return Ok(RespFrame::error("ERR syntax error"));
        }
    }
    
    // Get the stream
    let stream = match storage.get(db, key)? {
        GetResult::Found(Value::Stream(stream)) => stream,
        GetResult::Found(_) => return Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value")),
        _ => {
            // Return empty result with "0-0" as next
            return Ok(RespFrame::Array(Some(vec![
                RespFrame::from_string("0-0"),
                RespFrame::Array(Some(Vec::new())),
            ])));
        }
    };
    
    // Auto-claim messages
    match stream.auto_claim_messages(&group_name, &consumer_name, min_idle_ms, start_id, count) {
        Ok((entries, next_start)) => {
            let mut response = Vec::new();
            
            // Next start ID
            response.push(RespFrame::from_string(next_start.to_string()));
            
            // Claimed entries
            if justid {
                let id_frames: Vec<RespFrame> = entries
                    .into_iter()
                    .map(|e| RespFrame::from_string(e.id.to_string()))
                    .collect();
                response.push(RespFrame::Array(Some(id_frames)));
            } else {
                let entry_frames: Vec<RespFrame> = entries
                    .into_iter()
                    .map(|entry| {
                        let mut frames = Vec::new();
                        frames.push(RespFrame::from_string(entry.id.to_string()));
                        
                        let mut field_frames = Vec::new();
                        for (field, value) in entry.fields {
                            field_frames.push(RespFrame::from_bytes(field));
                            field_frames.push(RespFrame::from_bytes(value));
                        }
                        frames.push(RespFrame::Array(Some(field_frames)));
                        
                        RespFrame::Array(Some(frames))
                    })
                    .collect();
                response.push(RespFrame::Array(Some(entry_frames)));
            }
            
            Ok(RespFrame::Array(Some(response)))
        }
        Err(e) => Ok(RespFrame::error(e)),
    }
}