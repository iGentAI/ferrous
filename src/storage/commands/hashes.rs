//! Hash command implementations
//! 
//! Provides Redis-compatible hash operations for field-value pairs within a key.

use crate::error::{FerrousError, Result, CommandError};
use crate::protocol::RespFrame;
use crate::storage::{StorageEngine, Value};
use std::collections::HashMap;
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
    
    let fields_added = storage.hset(db, key, field_values)?;
    Ok(RespFrame::Integer(fields_added as i64))
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
    
    match storage.hget(db, key, field)? {
        Some(value) => Ok(RespFrame::from_bytes(value)),
        None => Ok(RespFrame::null_bulk()),
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
    
    storage.hset(db, key, field_values)?;
    Ok(RespFrame::ok())
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
    
    let values = storage.hmget(db, key, &fields)?;
    let frames: Vec<RespFrame> = values.into_iter()
        .map(|opt| match opt {
            Some(value) => RespFrame::from_bytes(value),
            None => RespFrame::null_bulk(),
        })
        .collect();
    
    Ok(RespFrame::Array(Some(frames)))
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
    
    let field_values = storage.hgetall(db, key)?;
    let mut frames = Vec::new();
    
    for (field, value) in field_values {
        frames.push(RespFrame::from_bytes(field));
        frames.push(RespFrame::from_bytes(value));
    }
    
    Ok(RespFrame::Array(Some(frames)))
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
    
    let deleted = storage.hdel(db, key, &fields)?;
    Ok(RespFrame::Integer(deleted as i64))
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
    
    let len = storage.hlen(db, key)?;
    Ok(RespFrame::Integer(len as i64))
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
    
    let exists = storage.hexists(db, key, field)?;
    Ok(RespFrame::Integer(if exists { 1 } else { 0 }))
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
    
    let fields = storage.hkeys(db, key)?;
    let frames: Vec<RespFrame> = fields.into_iter()
        .map(|f| RespFrame::from_bytes(f))
        .collect();
    
    Ok(RespFrame::Array(Some(frames)))
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
    
    let values = storage.hvals(db, key)?;
    let frames: Vec<RespFrame> = values.into_iter()
        .map(|v| RespFrame::from_bytes(v))
        .collect();
    
    Ok(RespFrame::Array(Some(frames)))
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
    
    let new_value = storage.hincrby(db, key, field, increment)?;
    Ok(RespFrame::Integer(new_value))
}