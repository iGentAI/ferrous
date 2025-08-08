//! Hash command implementations
//! 
//! Provides Redis-compatible hash operations for field-value pairs within a key.

use crate::error::{FerrousError, Result, StorageError};
use crate::protocol::RespFrame;
use crate::storage::StorageEngine;
use std::sync::Arc;

/// Handle HSET command - Set hash field(s)
pub fn handle_hset(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 4 || (parts.len() - 2) % 2 != 0 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'hset' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract field-value pairs
    let mut field_values = Vec::new();
    for i in (2..parts.len()).step_by(2) {
        let field = match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
            _ => return Ok(RespFrame::error("ERR invalid field format")),
        };
        
        let value = match &parts[i+1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
            _ => return Ok(RespFrame::error("ERR invalid value format")),
        };
        
        field_values.push((field, value));
    }
    
    // Set hash fields and handle WrongType errors properly
    match storage.hset(db, key, field_values) {
        Ok(fields_added) => Ok(RespFrame::Integer(fields_added as i64)),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}

/// Handle HGET command - Get hash field value
pub fn handle_hget(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'hget' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract field
    let field = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid field format")),
    };
    
    // Get field value and handle WrongType errors properly
    match storage.hget(db, key, field) {
        Ok(Some(value)) => Ok(RespFrame::from_bytes(value)),
        Ok(None) => Ok(RespFrame::null_bulk()),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}

/// Handle HMSET command - Set multiple hash fields (deprecated but supported)
pub fn handle_hmset(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 4 || (parts.len() - 2) % 2 != 0 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'hmset' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract field-value pairs
    let mut field_values = Vec::new();
    for i in (2..parts.len()).step_by(2) {
        let field = match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
            _ => return Ok(RespFrame::error("ERR invalid field format")),
        };
        
        let value = match &parts[i+1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
            _ => return Ok(RespFrame::error("ERR invalid value format")),
        };
        
        field_values.push((field, value));
    }
    
    // Set hash fields and handle WrongType errors properly
    match storage.hset(db, key, field_values) {
        Ok(_) => Ok(RespFrame::ok()),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}

/// Handle HMGET command - Get multiple hash field values
pub fn handle_hmget(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'hmget' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract fields
    let mut fields = Vec::new();
    for i in 2..parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => fields.push(bytes.as_ref()),
            _ => return Ok(RespFrame::error("ERR invalid field format")),
        }
    }
    
    // Get field values and handle WrongType errors properly
    match storage.hmget(db, key, &fields) {
        Ok(values) => {
            let frames: Vec<RespFrame> = values.into_iter()
                .map(|opt| match opt {
                    Some(value) => RespFrame::from_bytes(value),
                    None => RespFrame::null_bulk(),
                })
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

/// Handle HGETALL command - Get all fields and values
pub fn handle_hgetall(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'hgetall' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Get all field-value pairs and handle WrongType errors properly
    match storage.hgetall(db, key) {
        Ok(field_values) => {
            let mut frames = Vec::new();
            for (field, value) in field_values {
                frames.push(RespFrame::from_bytes(field));
                frames.push(RespFrame::from_bytes(value));
            }
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

/// Handle HDEL command - Delete hash fields
pub fn handle_hdel(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'hdel' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract fields
    let mut fields = Vec::new();
    for i in 2..parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => fields.push(bytes.as_ref()),
            _ => continue,
        }
    }
    
    // Delete fields and handle WrongType errors properly
    match storage.hdel(db, key, &fields) {
        Ok(deleted) => Ok(RespFrame::Integer(deleted as i64)),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}

/// Handle HLEN command - Get number of fields in hash
pub fn handle_hlen(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'hlen' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Get hash length and handle WrongType errors properly
    match storage.hlen(db, key) {
        Ok(len) => Ok(RespFrame::Integer(len as i64)),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}

/// Handle HEXISTS command - Check if field exists in hash
pub fn handle_hexists(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'hexists' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract field
    let field = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid field format")),
    };
    
    // Check field existence and handle WrongType errors properly
    match storage.hexists(db, key, field) {
        Ok(exists) => Ok(RespFrame::Integer(if exists { 1 } else { 0 })),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}

/// Handle HKEYS command - Get all field names
pub fn handle_hkeys(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'hkeys' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Get field names and handle WrongType errors properly
    match storage.hkeys(db, key) {
        Ok(fields) => {
            let frames: Vec<RespFrame> = fields.into_iter()
                .map(|f| RespFrame::from_bytes(f))
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

/// Handle HVALS command - Get all values
pub fn handle_hvals(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'hvals' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Get all values and handle WrongType errors properly
    match storage.hvals(db, key) {
        Ok(values) => {
            let frames: Vec<RespFrame> = values.into_iter()
                .map(|v| RespFrame::from_bytes(v))
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

/// Handle HINCRBY command - Increment integer field value
pub fn handle_hincrby(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 4 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'hincrby' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract field
    let field = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid field format")),
    };
    
    // Extract increment
    let increment = match &parts[3] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<i64>() {
                Ok(n) => n,
                Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid increment format")),
    };
    
    // Increment field and handle WrongType errors properly
    match storage.hincrby(db, key, field, increment) {
        Ok(new_value) => Ok(RespFrame::Integer(new_value)),
        Err(FerrousError::Storage(StorageError::WrongType)) => {
            Ok(RespFrame::error("WRONGTYPE Operation against a key holding the wrong kind of value"))
        },
        Err(e) => {
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}