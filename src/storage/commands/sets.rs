//! Set command implementations
//! 
//! Provides Redis-compatible set operations including add, remove, membership testing,
//! and set operations like union, intersection, and difference.

use crate::error::{FerrousError, Result, StorageError};
use crate::protocol::RespFrame;
use crate::storage::StorageEngine;
use std::sync::Arc;

/// Handle SADD command - Add members to a set
pub fn handle_sadd(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'sadd' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract members
    let mut members = Vec::new();
    for i in 2..parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => members.push(bytes.as_ref().clone()),
            _ => return Ok(RespFrame::error("ERR invalid member format")),
        }
    }
    
    // Add members and handle WrongType errors properly
    match storage.sadd(db, key, members) {
        Ok(added) => Ok(RespFrame::Integer(added as i64)),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}

/// Handle SREM command - Remove members from a set
pub fn handle_srem(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'srem' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract members
    let mut members = Vec::new();
    for i in 2..parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => members.push(bytes.as_ref()),
            _ => continue,
        }
    }
    
    // Remove members and handle WrongType errors properly
    match storage.srem(db, key, &members) {
        Ok(removed) => Ok(RespFrame::Integer(removed as i64)),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}

/// Handle SMEMBERS command - Get all members of a set
pub fn handle_smembers(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'smembers' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Get all members and handle WrongType errors properly
    match storage.smembers(db, key) {
        Ok(members) => {
            let frames: Vec<RespFrame> = members.into_iter()
                .map(|m| RespFrame::from_bytes(m))
                .collect();
            Ok(RespFrame::Array(Some(frames)))
        },
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}

/// Handle SISMEMBER command - Check if a member exists in a set
pub fn handle_sismember(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'sismember' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract member
    let member = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid member format")),
    };
    
    // Check membership and handle WrongType errors properly
    match storage.sismember(db, key, member) {
        Ok(exists) => Ok(RespFrame::Integer(if exists { 1 } else { 0 })),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}

/// Handle SCARD command - Get the number of members in a set
pub fn handle_scard(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'scard' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Get cardinality and handle WrongType errors properly
    match storage.scard(db, key) {
        Ok(count) => Ok(RespFrame::Integer(count as i64)),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}

/// Handle SUNION command - Get union of multiple sets
pub fn handle_sunion(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'sunion' command"));
    }
    
    // Extract keys
    let mut keys = Vec::new();
    for i in 1..parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => keys.push(bytes.as_ref()),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        }
    }
    
    // Get union and handle WrongType errors properly  
    match storage.sunion(db, &keys) {
        Ok(union) => {
            let frames: Vec<RespFrame> = union.into_iter()
                .map(|m| RespFrame::from_bytes(m))
                .collect();
            Ok(RespFrame::Array(Some(frames)))
        },
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}

/// Handle SINTER command - Get intersection of multiple sets
pub fn handle_sinter(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'sinter' command"));
    }
    
    // Extract keys
    let mut keys = Vec::new();
    for i in 1..parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => keys.push(bytes.as_ref()),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        }
    }
    
    // Get intersection and handle WrongType errors properly
    match storage.sinter(db, &keys) {
        Ok(intersection) => {
            let frames: Vec<RespFrame> = intersection.into_iter()
                .map(|m| RespFrame::from_bytes(m))
                .collect();
            Ok(RespFrame::Array(Some(frames)))
        },
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}

/// Handle SDIFF command - Get difference of multiple sets
pub fn handle_sdiff(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'sdiff' command"));
    }
    
    // Extract keys
    let mut keys = Vec::new();
    for i in 1..parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => keys.push(bytes.as_ref()),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        }
    }
    
    // Get difference and handle WrongType errors properly
    match storage.sdiff(db, &keys) {
        Ok(diff) => {
            let frames: Vec<RespFrame> = diff.into_iter()
                .map(|m| RespFrame::from_bytes(m))
                .collect();
            Ok(RespFrame::Array(Some(frames)))
        },
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}

/// Handle SRANDMEMBER command - Get random members from a set
pub fn handle_srandmember(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 2 || parts.len() > 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'srandmember' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract count (optional)
    let count = if parts.len() == 3 {
        match &parts[2] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<i64>() {
                    Ok(n) => Some(n),
                    Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid count format")),
        }
    } else {
        None
    };
    
    // Get random members and handle WrongType errors properly  
    match count {
        None => {
            match storage.srandmember(db, key, 1) {
                Ok(members) if members.is_empty() => Ok(RespFrame::null_bulk()),
                Ok(mut members) => Ok(RespFrame::from_bytes(members.pop().unwrap())),
                Err(FerrousError::Storage(StorageError::WrongType)) => {
                    Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
                },
                Err(e) => {
                    Ok(RespFrame::error(format!("ERR {}", e)))
                }
            }
        }
        Some(n) => {
            match storage.srandmember(db, key, n) {
                Ok(members) => {
                    let frames: Vec<RespFrame> = members.into_iter()
                        .map(|m| RespFrame::from_bytes(m))
                        .collect();
                    Ok(RespFrame::Array(Some(frames)))
                },
                Err(FerrousError::Storage(StorageError::WrongType)) => {
                    Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
                },
                Err(e) => {
                    Ok(RespFrame::error(format!("ERR {}", e)))
                }
            }
        }
    }
}

/// Handle SPOP command - Remove and return random members from a set
pub fn handle_spop(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 2 || parts.len() > 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'spop' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract count (optional)
    let count = if parts.len() == 3 {
        match &parts[2] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<usize>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid count format")),
        }
    } else {
        1
    };
    
    // Pop members and handle WrongType errors properly
    match storage.spop(db, key, count) {
        Ok(members) => {
            if parts.len() == 2 && count == 1 {
                // Single member response
                match members.into_iter().next() {
                    Some(member) => Ok(RespFrame::from_bytes(member)),
                    None => Ok(RespFrame::null_bulk()),
                }
            } else {
                // Array response
                let frames: Vec<RespFrame> = members.into_iter()
                    .map(|m| RespFrame::from_bytes(m))
                    .collect();
                Ok(RespFrame::Array(Some(frames)))
            }
        },
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}