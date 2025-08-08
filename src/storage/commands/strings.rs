//! String command implementations
//! 
//! Provides additional Redis-compatible string operations beyond basic SET/GET.

use crate::error::{FerrousError, Result, StorageError};
use crate::protocol::RespFrame;
use crate::storage::StorageEngine;
use std::sync::Arc;

/// Handle MGET command - Get multiple keys
pub fn handle_mget(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'mget' command"));
    }
    
    let mut values = Vec::new();
    
    for i in 1..parts.len() {
        let key = match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        match storage.get_string(db, key)? {
            Some(value) => values.push(RespFrame::from_bytes(value)),
            None => values.push(RespFrame::null_bulk()),
        }
    }
    
    Ok(RespFrame::Array(Some(values)))
}

/// Handle MSET command - Set multiple keys
pub fn handle_mset(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 3 || parts.len() % 2 == 0 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'mset' command"));
    }
    
    for i in (1..parts.len()).step_by(2) {
        let key = match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        let value = match &parts[i + 1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
            _ => return Ok(RespFrame::error("ERR invalid value format")),
        };
        
        storage.set_string(db, key, value)?;
    }
    
    Ok(RespFrame::ok())
}

/// Handle GETSET command - Set new value and return old value
pub fn handle_getset(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'getset' command"));
    }
    
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let new_value = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid value format")),
    };
    
    // Get old value first
    let old_value = storage.get_string(db, &key)?;
    
    // Set new value
    storage.set_string(db, key, new_value)?;
    
    // Return old value
    match old_value {
        Some(value) => Ok(RespFrame::from_bytes(value)),
        None => Ok(RespFrame::null_bulk()),
    }
}

/// Handle APPEND command - Append value to key
pub fn handle_append(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'append' command"));
    }
    
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let value = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid value format")),
    };
    
    // Append and handle WrongType errors properly
    match storage.append(db, key, value) {
        Ok(new_len) => Ok(RespFrame::Integer(new_len as i64)),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}

/// Handle STRLEN command - Get string length
pub fn handle_strlen(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'strlen' command"));
    }
    
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Get string length and handle WrongType errors properly
    match storage.strlen(db, key) {
        Ok(len) => Ok(RespFrame::Integer(len as i64)),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}

/// Handle GETRANGE command - Get substring of string
pub fn handle_getrange(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 4 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'getrange' command"));
    }
    
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let start = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<isize>() {
                Ok(n) => n,
                Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid start format")),
    };
    
    let end = match &parts[3] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<isize>() {
                Ok(n) => n,
                Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid end format")),
    };
    
    // Get substring and handle WrongType errors properly
    match storage.getrange(db, key, start, end) {
        Ok(substring) => Ok(RespFrame::from_bytes(substring)),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}

/// Handle SETRANGE command - Overwrite part of string
pub fn handle_setrange(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 4 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'setrange' command"));
    }
    
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let offset = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<usize>() {
                Ok(n) => n,
                Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid offset format")),
    };
    
    let value = match &parts[3] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid value format")),
    };
    
    // Set range and handle WrongType errors properly
    match storage.setrange(db, key, offset, value) {
        Ok(new_len) => Ok(RespFrame::Integer(new_len as i64)),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}

/// Handle TYPE command - Get key type
pub fn handle_type(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'type' command"));
    }
    
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let type_name = storage.key_type(db, key)?;
    Ok(RespFrame::SimpleString(Arc::new(type_name.into_bytes())))
}

/// Handle RENAME command - Rename a key
pub fn handle_rename(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'rename' command"));
    }
    
    let old_key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let new_key = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    storage.rename(db, old_key, new_key)?;
    Ok(RespFrame::ok())
}

/// Handle KEYS command - Find all keys matching pattern
pub fn handle_keys(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'keys' command"));
    }
    
    let pattern = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid pattern format")),
    };
    
    let matching_keys = storage.keys(db, pattern)?;
    let frames: Vec<RespFrame> = matching_keys.into_iter()
        .map(|k| RespFrame::from_bytes(k))
        .collect();
    
    Ok(RespFrame::Array(Some(frames)))
}

/// Handle PEXPIRE command - Set expiration in milliseconds
pub fn handle_pexpire(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'pexpire' command"));
    }
    
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let milliseconds = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<u64>() {
                Ok(n) => n,
                Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid milliseconds format")),
    };
    
    let result = storage.pexpire(db, key, milliseconds)?;
    Ok(RespFrame::Integer(if result { 1 } else { 0 }))
}

/// Handle PTTL command - Get TTL in milliseconds
pub fn handle_pttl(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'pttl' command"));
    }
    
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let ttl_millis = storage.pttl(db, key)?;
    Ok(RespFrame::Integer(ttl_millis))
}

/// Handle PERSIST command - Remove expiration
pub fn handle_persist(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'persist' command"));
    }
    
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Remove expiration and handle errors properly (PERSIST works on any key type)
    match storage.persist(db, key) {
        Ok(result) => Ok(RespFrame::Integer(if result { 1 } else { 0 })),
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}