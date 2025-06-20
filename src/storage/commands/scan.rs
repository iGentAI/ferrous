//! SCAN command implementation

use std::sync::Arc;
use crate::protocol::RespFrame;
use crate::error::Result;
use crate::storage::StorageEngine;

/// Handle SCAN command
pub fn handle_scan(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    // SCAN cursor [MATCH pattern] [TYPE type] [COUNT count]
    if parts.len() < 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'scan' command"));
    }
    
    // Parse cursor
    let cursor = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<u64>() {
                Ok(c) => c,
                Err(_) => return Ok(RespFrame::error("ERR invalid cursor")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid cursor format")),
    };
    
    // Parse optional arguments
    let mut pattern = None;
    let mut type_filter = None;
    let mut count = 10; // Default count
    
    let mut i = 2;
    while i < parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(option)) => {
                let option_str = String::from_utf8_lossy(option).to_uppercase();
                match option_str.as_str() {
                    "MATCH" => {
                        if i + 1 < parts.len() {
                            if let RespFrame::BulkString(Some(p)) = &parts[i + 1] {
                                pattern = Some(p.as_ref());
                                i += 2;
                                continue;
                            }
                        }
                        return Ok(RespFrame::error("ERR syntax error"));
                    }
                    "TYPE" => {
                        if i + 1 < parts.len() {
                            if let RespFrame::BulkString(Some(t)) = &parts[i + 1] {
                                type_filter = Some(String::from_utf8_lossy(t).to_string());
                                i += 2;
                                continue;
                            }
                        }
                        return Ok(RespFrame::error("ERR syntax error"));
                    }
                    "COUNT" => {
                        if i + 1 < parts.len() {
                            if let RespFrame::BulkString(Some(c)) = &parts[i + 1] {
                                match String::from_utf8_lossy(c).parse::<usize>() {
                                    Ok(n) => {
                                        count = n;
                                        i += 2;
                                        continue;
                                    }
                                    Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                                }
                            }
                        }
                        return Ok(RespFrame::error("ERR syntax error"));
                    }
                    _ => return Ok(RespFrame::error("ERR syntax error")),
                }
            }
            _ => return Ok(RespFrame::error("ERR syntax error")),
        }
    }
    
    // Execute scan
    let (next_cursor, keys) = storage.scan(db, cursor, pattern.map(|p| &**p), type_filter.as_deref(), count)?;
    
    // Build response
    let cursor_str = next_cursor.to_string();
    let cursor_frame = RespFrame::from_string(cursor_str);
    
    let keys_frames: Vec<RespFrame> = keys.into_iter()
        .map(|k| RespFrame::from_bytes(k))
        .collect();
    
    Ok(RespFrame::Array(Some(vec![
        cursor_frame,
        RespFrame::Array(Some(keys_frames))
    ])))
}

/// Handle HSCAN command
pub fn handle_hscan(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    // HSCAN key cursor [MATCH pattern] [COUNT count] [NOVALUES]
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'hscan' command"));
    }
    
    // Parse key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Parse cursor
    let cursor = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<u64>() {
                Ok(c) => c,
                Err(_) => return Ok(RespFrame::error("ERR invalid cursor")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid cursor format")),
    };
    
    // Parse optional arguments
    let mut pattern = None;
    let mut count = 10; // Default count
    let mut no_values = false;
    
    let mut i = 3;
    while i < parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(option)) => {
                let option_str = String::from_utf8_lossy(option).to_uppercase();
                match option_str.as_str() {
                    "MATCH" => {
                        if i + 1 < parts.len() {
                            if let RespFrame::BulkString(Some(p)) = &parts[i + 1] {
                                pattern = Some(p.as_ref());
                                i += 2;
                                continue;
                            }
                        }
                        return Ok(RespFrame::error("ERR syntax error"));
                    }
                    "COUNT" => {
                        if i + 1 < parts.len() {
                            if let RespFrame::BulkString(Some(c)) = &parts[i + 1] {
                                match String::from_utf8_lossy(c).parse::<usize>() {
                                    Ok(n) => {
                                        count = n;
                                        i += 2;
                                        continue;
                                    }
                                    Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                                }
                            }
                        }
                        return Ok(RespFrame::error("ERR syntax error"));
                    }
                    "NOVALUES" => {
                        no_values = true;
                        i += 1;
                    }
                    _ => return Ok(RespFrame::error("ERR syntax error")),
                }
            }
            _ => return Ok(RespFrame::error("ERR syntax error")),
        }
    }
    
    // Execute hscan
    let (next_cursor, elements) = storage.hscan(db, key, cursor, pattern.map(|p| &**p), count, no_values)?;
    
    // Build response
    let cursor_str = next_cursor.to_string();
    let cursor_frame = RespFrame::from_string(cursor_str);
    
    let elements_frames: Vec<RespFrame> = elements.into_iter()
        .map(|e| RespFrame::from_bytes(e))
        .collect();
    
    Ok(RespFrame::Array(Some(vec![
        cursor_frame,
        RespFrame::Array(Some(elements_frames))
    ])))
}

/// Handle SSCAN command
pub fn handle_sscan(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    // SSCAN key cursor [MATCH pattern] [COUNT count]
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'sscan' command"));
    }
    
    // Parse key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Parse cursor
    let cursor = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<u64>() {
                Ok(c) => c,
                Err(_) => return Ok(RespFrame::error("ERR invalid cursor")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid cursor format")),
    };
    
    // Parse optional arguments
    let mut pattern = None;
    let mut count = 10; // Default count
    
    let mut i = 3;
    while i < parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(option)) => {
                let option_str = String::from_utf8_lossy(option).to_uppercase();
                match option_str.as_str() {
                    "MATCH" => {
                        if i + 1 < parts.len() {
                            if let RespFrame::BulkString(Some(p)) = &parts[i + 1] {
                                pattern = Some(p.as_ref());
                                i += 2;
                                continue;
                            }
                        }
                        return Ok(RespFrame::error("ERR syntax error"));
                    }
                    "COUNT" => {
                        if i + 1 < parts.len() {
                            if let RespFrame::BulkString(Some(c)) = &parts[i + 1] {
                                match String::from_utf8_lossy(c).parse::<usize>() {
                                    Ok(n) => {
                                        count = n;
                                        i += 2;
                                        continue;
                                    }
                                    Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                                }
                            }
                        }
                        return Ok(RespFrame::error("ERR syntax error"));
                    }
                    _ => return Ok(RespFrame::error("ERR syntax error")),
                }
            }
            _ => return Ok(RespFrame::error("ERR syntax error")),
        }
    }
    
    // Execute sscan
    let (next_cursor, members) = storage.sscan(db, key, cursor, pattern.map(|p| &**p), count)?;
    
    // Build response
    let cursor_str = next_cursor.to_string();
    let cursor_frame = RespFrame::from_string(cursor_str);
    
    let members_frames: Vec<RespFrame> = members.into_iter()
        .map(|m| RespFrame::from_bytes(m))
        .collect();
    
    Ok(RespFrame::Array(Some(vec![
        cursor_frame,
        RespFrame::Array(Some(members_frames))
    ])))
}

/// Handle ZSCAN command
pub fn handle_zscan(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    // ZSCAN key cursor [MATCH pattern] [COUNT count]
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'zscan' command"));
    }
    
    // Parse key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Parse cursor
    let cursor = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<u64>() {
                Ok(c) => c,
                Err(_) => return Ok(RespFrame::error("ERR invalid cursor")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid cursor format")),
    };
    
    // Parse optional arguments
    let mut pattern = None;
    let mut count = 10; // Default count
    
    let mut i = 3;
    while i < parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(option)) => {
                let option_str = String::from_utf8_lossy(option).to_uppercase();
                match option_str.as_str() {
                    "MATCH" => {
                        if i + 1 < parts.len() {
                            if let RespFrame::BulkString(Some(p)) = &parts[i + 1] {
                                pattern = Some(p.as_ref());
                                i += 2;
                                continue;
                            }
                        }
                        return Ok(RespFrame::error("ERR syntax error"));
                    }
                    "COUNT" => {
                        if i + 1 < parts.len() {
                            if let RespFrame::BulkString(Some(c)) = &parts[i + 1] {
                                match String::from_utf8_lossy(c).parse::<usize>() {
                                    Ok(n) => {
                                        count = n;
                                        i += 2;
                                        continue;
                                    }
                                    Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                                }
                            }
                        }
                        return Ok(RespFrame::error("ERR syntax error"));
                    }
                    _ => return Ok(RespFrame::error("ERR syntax error")),
                }
            }
            _ => return Ok(RespFrame::error("ERR syntax error")),
        }
    }
    
    // Execute zscan
    let (next_cursor, items) = storage.zscan(db, key, cursor, pattern.map(|p| &**p), count)?;
    
    // Build response
    let cursor_str = next_cursor.to_string();
    let cursor_frame = RespFrame::from_string(cursor_str);
    
    // Flatten member-score pairs into a single array
    let mut elements_frames = Vec::with_capacity(items.len() * 2);
    for (member, score) in items {
        elements_frames.push(RespFrame::from_bytes(member));
        elements_frames.push(RespFrame::from_string(score.to_string()));
    }
    
    Ok(RespFrame::Array(Some(vec![
        cursor_frame,
        RespFrame::Array(Some(elements_frames))
    ])))
}