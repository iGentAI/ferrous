//! List command implementations
//! 
//! Provides Redis-compatible list operations including push, pop, range, and more.

use crate::error::{FerrousError, Result, CommandError};
use crate::protocol::RespFrame;
use crate::storage::{StorageEngine, Value};
use std::collections::VecDeque;
use std::sync::Arc;

/// Handle LPUSH command - Insert elements at the head of the list
pub fn handle_lpush(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'lpush' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract elements to push
    let mut elements = Vec::new();
    for i in 2..parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => elements.push(bytes.as_ref().clone()),
            _ => return Ok(RespFrame::error("ERR invalid element format")),
        }
    }
    
    // Push elements and get new length
    let new_len = storage.lpush(db, key, elements)?;
    Ok(RespFrame::Integer(new_len as i64))
}

/// Handle RPUSH command - Insert elements at the tail of the list
pub fn handle_rpush(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'rpush' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract elements to push
    let mut elements = Vec::new();
    for i in 2..parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => elements.push(bytes.as_ref().clone()),
            _ => return Ok(RespFrame::error("ERR invalid element format")),
        }
    }
    
    // Push elements and get new length
    let new_len = storage.rpush(db, key, elements)?;
    Ok(RespFrame::Integer(new_len as i64))
}

/// Handle LPOP command - Remove and return element from head
pub fn handle_lpop(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'lpop' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    match storage.lpop(db, key)? {
        Some(element) => Ok(RespFrame::from_bytes(element)),
        None => Ok(RespFrame::null_bulk()),
    }
}

/// Handle RPOP command - Remove and return element from tail
pub fn handle_rpop(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'rpop' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    match storage.rpop(db, key)? {
        Some(element) => Ok(RespFrame::from_bytes(element)),
        None => Ok(RespFrame::null_bulk()),
    }
}

/// Handle LLEN command - Get list length
pub fn handle_llen(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'llen' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    let len = storage.llen(db, key)?;
    Ok(RespFrame::Integer(len as i64))
}

/// Handle LRANGE command - Get range of elements
pub fn handle_lrange(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 4 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'lrange' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract start index
    let start = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<isize>() {
                Ok(n) => n,
                Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid start format")),
    };
    
    // Extract stop index
    let stop = match &parts[3] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<isize>() {
                Ok(n) => n,
                Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid stop format")),
    };
    
    let elements = storage.lrange(db, key, start, stop)?;
    let frames: Vec<RespFrame> = elements.into_iter()
        .map(|e| RespFrame::from_bytes(e))
        .collect();
    
    Ok(RespFrame::Array(Some(frames)))
}

/// Handle LINDEX command - Get element at index
pub fn handle_lindex(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'lindex' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract index
    let index = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<isize>() {
                Ok(n) => n,
                Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid index format")),
    };
    
    match storage.lindex(db, key, index)? {
        Some(element) => Ok(RespFrame::from_bytes(element)),
        None => Ok(RespFrame::null_bulk()),
    }
}

/// Handle LSET command - Set element at index
pub fn handle_lset(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 4 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'lset' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract index
    let index = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<isize>() {
                Ok(n) => n,
                Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid index format")),
    };
    
    // Extract value
    let value = match &parts[3] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid value format")),
    };
    
    storage.lset(db, key, index, value)?;
    Ok(RespFrame::ok())
}

/// Handle LTRIM command - Trim list to specified range
pub fn handle_ltrim(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 4 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'ltrim' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract start index
    let start = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<isize>() {
                Ok(n) => n,
                Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid start format")),
    };
    
    // Extract stop index
    let stop = match &parts[3] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<isize>() {
                Ok(n) => n,
                Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid stop format")),
    };
    
    storage.ltrim(db, key, start, stop)?;
    Ok(RespFrame::ok())
}

/// Handle LREM command - Remove elements from list
pub fn handle_lrem(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 4 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'lrem' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract count
    let count = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<isize>() {
                Ok(n) => n,
                Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid count format")),
    };
    
    // Extract element
    let element = match &parts[3] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid element format")),
    };
    
    let removed = storage.lrem(db, key, count, element)?;
    Ok(RespFrame::Integer(removed as i64))
}