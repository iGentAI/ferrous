//! List command implementations
//! 
//! Provides Redis-compatible list operations including push, pop, range, and more.

use crate::error::{FerrousError, Result, CommandError, StorageError};
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
    match storage.lpush(db, key, elements) {
        Ok(new_len) => Ok(RespFrame::Integer(new_len as i64)),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
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
    match storage.rpush(db, key, elements) {
        Ok(new_len) => Ok(RespFrame::Integer(new_len as i64)),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
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
    
    // Pop element from head
    match storage.lpop(db, key) {
        Ok(Some(element)) => Ok(RespFrame::from_bytes(element)),
        Ok(None) => Ok(RespFrame::null_bulk()),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
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
    
    // Pop element from tail
    match storage.rpop(db, key) {
        Ok(Some(element)) => Ok(RespFrame::from_bytes(element)),
        Ok(None) => Ok(RespFrame::null_bulk()),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
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
    
    // Get list length
    match storage.llen(db, key) {
        Ok(len) => Ok(RespFrame::Integer(len as i64)),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
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
    
    // Get range of elements
    match storage.lrange(db, key, start, stop) {
        Ok(elements) => {
            let frames: Vec<RespFrame> = elements.into_iter()
                .map(|e| RespFrame::from_bytes(e))
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
    
    // Get element at index
    match storage.lindex(db, key, index) {
        Ok(Some(element)) => Ok(RespFrame::from_bytes(element)),
        Ok(None) => Ok(RespFrame::null_bulk()),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
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
    
    // Set element at index
    match storage.lset(db, key, index, value) {
        Ok(()) => Ok(RespFrame::ok()),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
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
    
    // Trim list to specified range
    match storage.ltrim(db, key, start, stop) {
        Ok(()) => Ok(RespFrame::ok()),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
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
    
    // Remove elements from list
    match storage.lrem(db, key, count, element) {
        Ok(removed) => Ok(RespFrame::Integer(removed as i64)),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}