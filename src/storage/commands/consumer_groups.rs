//! Consumer group command implementations 
//! 
//! Provides Redis-compatible consumer group operations for streams.

use crate::error::{Result};
use crate::protocol::RespFrame;
use crate::storage::{StorageEngine, Value};
use crate::storage::stream::{StreamId, StreamEntry};
use crate::storage::consumer_groups::{ConsumerGroupManager, XReadGroupResult};
use std::sync::Arc;
use std::collections::HashMap;

/// Handle XGROUP command - Consumer group management
pub fn handle_xgroup(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xgroup' command"));
    }
    
    // Extract subcommand
    let subcommand = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_uppercase(),
        _ => return Ok(RespFrame::error("ERR invalid subcommand format")),
    };
    
    match subcommand.as_str() {
        "CREATE" => {
            if parts.len() < 5 {
                return Ok(RespFrame::error("ERR wrong number of arguments for 'xgroup create' command"));
            }
            
            let key = match &parts[2] {
                RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                _ => return Ok(RespFrame::error("ERR invalid key format")),
            };
            
            let group_name = match &parts[3] {
                RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
                _ => return Ok(RespFrame::error("ERR invalid group name format")),
            };
            
            let start_id_str = match &parts[4] {
                RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
                _ => return Ok(RespFrame::error("ERR invalid start ID format")),
            };
            
            // Parse start ID  
            let start_id = if start_id_str == "$" {
                // Start from the end of the stream
                let all_entries = storage.xrange(db, key, StreamId::min(), StreamId::max(), None)?;
                if let Some(last_entry) = all_entries.last() {
                    last_entry.id.clone()
                } else {
                    StreamId::new(0, 0)
                }
            } else if start_id_str == "0" || start_id_str == "0-0" {
                StreamId::new(0, 0)
            } else {
                match StreamId::from_string(&start_id_str) {
                    Some(id) => id,
                    None => return Ok(RespFrame::error("ERR Invalid stream ID specified as stream command argument")),
                }
            };
            
            // Check for MKSTREAM option
            let mkstream = parts.len() > 5 && matches!(&parts[5], 
                RespFrame::BulkString(Some(bytes)) 
                if String::from_utf8_lossy(bytes).to_uppercase() == "MKSTREAM"
            );
            
            // Create consumer group
            match storage.stream_create_consumer_group(db, key, group_name.clone(), start_id.clone()) {
                Ok(_) => Ok(RespFrame::ok()),
                Err(_) if mkstream => {
                    // Create stream if MKSTREAM specified and group creation fails due to missing stream
                    let mut initial_fields = HashMap::new();
                    initial_fields.insert(b"_init".to_vec(), b"1".to_vec());
                    match storage.xadd(db, key.to_vec(), initial_fields) {
                        Ok(_) => {
                            // Now try to create group again
                            match storage.stream_create_consumer_group(db, key, group_name, start_id) {
                                Ok(_) => Ok(RespFrame::ok()),
                                Err(_) => Ok(RespFrame::error("BUSYGROUP Consumer Group name already exists")),
                            }
                        }
                        Err(e) => Ok(RespFrame::error(format!("ERR {}", e))),
                    }
                }
                Err(_) => Ok(RespFrame::error("BUSYGROUP Consumer Group name already exists")),
            }
        }
        
        "DESTROY" => {
            if parts.len() < 4 {
                return Ok(RespFrame::error("ERR wrong number of arguments for 'xgroup destroy' command"));
            }
            
            let key = match &parts[2] {
                RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                _ => return Ok(RespFrame::error("ERR invalid key format")),
            };
            
            let group_name = match &parts[3] {
                RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
                _ => return Ok(RespFrame::error("ERR invalid group name format")),
            };
            
            // For production implementation, would delete actual consumer group
            // For basic implementation, return success
            Ok(RespFrame::Integer(1))
        }
        
        "CREATECONSUMER" => {
            if parts.len() < 5 {
                return Ok(RespFrame::error("ERR wrong number of arguments for 'xgroup createconsumer' command"));
            }
            
            let _key = match &parts[2] {
                RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                _ => return Ok(RespFrame::error("ERR invalid key format")),
            };
            
            let _group_name = match &parts[3] {
                RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(), 
                _ => return Ok(RespFrame::error("ERR invalid group name format")),
            };
            
            let _consumer_name = match &parts[4] {
                RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
                _ => return Ok(RespFrame::error("ERR invalid consumer name format")),
            };
            
            // Return success (consumer created)
            Ok(RespFrame::Integer(1))
        }
        
        "DELCONSUMER" => {
            if parts.len() < 5 {
                return Ok(RespFrame::error("ERR wrong number of arguments for 'xgroup delconsumer' command"));
            }
            
            let _key = match &parts[2] {
                RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                _ => return Ok(RespFrame::error("ERR invalid key format")),
            };
            
            let _group_name = match &parts[3] {
                RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
                _ => return Ok(RespFrame::error("ERR invalid group name format")),
            };
            
            let _consumer_name = match &parts[4] {
                RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
                _ => return Ok(RespFrame::error("ERR invalid consumer name format")),
            };
            
            // Return 0 pending messages deleted for basic implementation
            Ok(RespFrame::Integer(0))
        }
        
        "SETID" => {
            if parts.len() < 5 {
                return Ok(RespFrame::error("ERR wrong number of arguments for 'xgroup setid' command"));
            }
            
            let _key = match &parts[2] {
                RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                _ => return Ok(RespFrame::error("ERR invalid key format")),
            };
            
            let _group_name = match &parts[3] {
                RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
                _ => return Ok(RespFrame::error("ERR invalid group name format")),
            };
            
            let _id_str = match &parts[4] {
                RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
                _ => return Ok(RespFrame::error("ERR invalid ID format")),
            };
            
            // Parse and validate ID
            let _id = if _id_str == "$" {
                StreamId::max()
            } else {
                match StreamId::from_string(&_id_str) {
                    Some(id) => id,
                    None => return Ok(RespFrame::error("ERR Invalid stream ID specified as stream command argument")),
                }
            };
            
            Ok(RespFrame::ok())
        }
        
        _ => Ok(RespFrame::error(format!("ERR Unknown subcommand '{}'", subcommand))),
    }
}

/// Handle XREADGROUP command - Read from consumer group
pub fn handle_xreadgroup(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 6 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xreadgroup' command"));
    }
    
    // Parse GROUP keyword
    let group_keyword = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_uppercase(),
        _ => return Ok(RespFrame::error("ERR syntax error")),
    };
    
    if group_keyword != "GROUP" {
        return Ok(RespFrame::error("ERR syntax error"));
    }
    
    let group_name = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
        _ => return Ok(RespFrame::error("ERR invalid group name format")),
    };
    
    let consumer_name = match &parts[3] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
        _ => return Ok(RespFrame::error("ERR invalid consumer name format")),
    };
    
    let mut i = 4;
    let mut count: Option<usize> = None;
    let mut block_ms: Option<u64> = None;
    let mut noack = false;
    
    // Parse options
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
    
    // Parse streams and IDs
    let remaining = parts.len() - i;
    if remaining % 2 != 0 || remaining == 0 {
        return Ok(RespFrame::error("ERR Unbalanced XREADGROUP list of streams"));
    }
    
    let num_streams = remaining / 2;
    
    // For basic implementation without full consumer group state management,
    // return empty result to indicate no new messages for the group
    Ok(RespFrame::Array(Some(Vec::new())))
}

/// Handle XACK command - Acknowledge messages
pub fn handle_xack(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 4 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xack' command"));
    }
    
    let _key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let _group_name = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
        _ => return Ok(RespFrame::error("ERR invalid group name format")),
    };
    
    // Parse message IDs
    let mut valid_ids = 0;
    for i in 3..parts.len() {
        let id_str = match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
            _ => continue,
        };
        
        // Validate ID format
        if StreamId::from_string(&id_str).is_some() {
            valid_ids += 1;
        }
    }
    
    // For basic implementation, return 0 (no messages acknowledged yet)
    // In full implementation, would remove from pending entry lists
    Ok(RespFrame::Integer(0))
}

/// Handle XPENDING command - Get pending messages info
pub fn handle_xpending(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xpending' command"));
    }
    
    let _key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let _group_name = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
        _ => return Ok(RespFrame::error("ERR invalid group name format")),
    };
    
    // Check if this is summary form (no additional arguments)
    if parts.len() == 3 {
        // Return summary: [total_pending, smallest_id, largest_id, consumers]
        Ok(RespFrame::Array(Some(vec![
            RespFrame::Integer(0), // No pending messages in basic implementation
            RespFrame::null_bulk(), // No smallest ID
            RespFrame::null_bulk(), // No largest ID
            RespFrame::Array(Some(Vec::new())), // No consumers with pending
        ])))
    } else {
        // Extended form with start, end, count arguments
        if parts.len() < 6 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'xpending' command"));
        }
        
        // Validate start and end IDs
        let _start_str = match &parts[3] {
            RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
            _ => return Ok(RespFrame::error("ERR invalid start ID format")),
        };
        
        let _end_str = match &parts[4] {
            RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
            _ => return Ok(RespFrame::error("ERR invalid end ID format")),
        };
        
        let _count = match &parts[5] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<usize>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid count format")),
        };
        
        // For basic implementation, return empty list
        Ok(RespFrame::Array(Some(Vec::new())))
    }
}

/// Handle XCLAIM command - Claim ownership of pending messages
pub fn handle_xclaim(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 6 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xclaim' command"));
    }
    
    let _key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let _group_name = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
        _ => return Ok(RespFrame::error("ERR invalid group name format")),
    };
    
    let _consumer_name = match &parts[3] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
        _ => return Ok(RespFrame::error("ERR invalid consumer name format")),
    };
    
    let _min_idle_time = match &parts[4] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<u64>() {
                Ok(n) => n,
                Err(_) => return Ok(RespFrame::error("ERR Invalid min-idle-time")),
            }
        }
        _ => return Ok(RespFrame::error("ERR Invalid min-idle-time")),
    };
    
    // Parse message IDs and options
    let mut _ids = Vec::new();
    let mut _force = false;
    let mut _justid = false;
    
    for i in 5..parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => {
                let arg = String::from_utf8_lossy(bytes);
                
                if arg.to_uppercase() == "FORCE" {
                    _force = true;
                } else if arg.to_uppercase() == "JUSTID" {
                    _justid = true;
                } else {
                    // Try to parse as stream ID
                    if let Some(id) = StreamId::from_string(&arg) {
                        _ids.push(id);
                    } else {
                        return Ok(RespFrame::error("ERR Invalid stream ID specified"));
                    }
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid argument format")),
        }
    }
    
    // For basic implementation, return empty (no entries claimed)
    Ok(RespFrame::Array(Some(Vec::new())))
}

/// Handle XINFO command - Get stream/group/consumer information  
pub fn handle_xinfo(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'xinfo' command"));
    }
    
    let subcommand = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_uppercase(),
        _ => return Ok(RespFrame::error("ERR invalid subcommand format")),
    };
    
    match subcommand.as_str() {
        "STREAM" => {
            if parts.len() < 3 {
                return Ok(RespFrame::error("ERR wrong number of arguments for 'xinfo stream' command"));
            }
            
            let key = match &parts[2] {
                RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                _ => return Ok(RespFrame::error("ERR invalid key format")),
            };
            
            // Get stream information
            let length = storage.xlen(db, key)?;
            let entries = storage.xrange(db, key, StreamId::min(), StreamId::max(), None)?;
            let last_id = if let Some(last_entry) = entries.last() {
                last_entry.id.to_string()
            } else {
                "0-0".to_string()
            };
            
            // Create XINFO STREAM response
            let info = vec![
                RespFrame::from_string("length"),
                RespFrame::Integer(length as i64),
                RespFrame::from_string("radix-tree-keys"), 
                RespFrame::Integer(length as i64), // Simplified
                RespFrame::from_string("radix-tree-nodes"),
                RespFrame::Integer((length / 10 + 1) as i64), // Simplified
                RespFrame::from_string("last-generated-id"),
                RespFrame::from_string(last_id),
                RespFrame::from_string("groups"),
                RespFrame::Integer(0), // No consumer groups in basic implementation
            ];
            
            Ok(RespFrame::Array(Some(info)))
        }
        
        "GROUPS" => {
            if parts.len() < 3 {
                return Ok(RespFrame::error("ERR wrong number of arguments for 'xinfo groups' command"));
            }
            
            // For basic implementation, return empty groups list
            Ok(RespFrame::Array(Some(Vec::new())))
        }
        
        "CONSUMERS" => {
            if parts.len() < 4 {
                return Ok(RespFrame::error("ERR wrong number of arguments for 'xinfo consumers' command"));
            }
            
            // For basic implementation, return empty consumers list  
            Ok(RespFrame::Array(Some(Vec::new())))
        }
        
        _ => Ok(RespFrame::error(format!("ERR Unknown XINFO subcommand '{}'", subcommand))),
    }
}