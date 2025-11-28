//! Bit operation commands for Ferrous
//!
//! Implements Redis-compatible bit operations: SETBIT, GETBIT, BITCOUNT, BITOP, BITPOS
//! 
//! Bit addressing: Strings are treated as bit arrays where bit 0 is the MSB of byte 0.
//! Bit offset N maps to byte N/8, bit position 7-(N%8) within that byte.

use std::sync::Arc;
use crate::error::Result;
use crate::protocol::RespFrame;
use crate::storage::StorageEngine;

/// Calculate byte index and bit mask for a given bit offset
/// Redis uses MSB-first bit ordering within each byte
#[inline]
fn bit_position(offset: u64) -> (usize, u8) {
    let byte_index = (offset / 8) as usize;
    let bit_index = 7 - (offset % 8) as u8;
    let bit_mask = 1u8 << bit_index;
    (byte_index, bit_mask)
}

/// Handle SETBIT command
/// SETBIT key offset value
pub fn handle_setbit(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 4 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'setbit' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract offset
    let offset = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<i64>() {
                Ok(n) if n >= 0 && n <= 4294967295 => n as u64,
                Ok(_) => return Ok(RespFrame::error("ERR bit offset is not an integer or out of range")),
                Err(_) => return Ok(RespFrame::error("ERR bit offset is not an integer or out of range")),
            }
        }
        _ => return Ok(RespFrame::error("ERR bit offset is not an integer or out of range")),
    };
    
    // Extract bit value
    let bit_value = match &parts[3] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).trim() {
                "0" => false,
                "1" => true,
                _ => return Ok(RespFrame::error("ERR bit is not an integer or out of range")),
            }
        }
        _ => return Ok(RespFrame::error("ERR bit is not an integer or out of range")),
    };
    
    // Calculate byte position and bit mask
    let (byte_index, bit_mask) = bit_position(offset);
    
    // Get current string or create empty one
    let mut current_value = match storage.get_string(db, &key)? {
        Some(bytes) => bytes,
        None => Vec::new(),
    };
    
    // Extend string if necessary (fill with zeros)
    if current_value.len() <= byte_index {
        current_value.resize(byte_index + 1, 0);
    }
    
    // Get the original bit value
    let original_bit = if (current_value[byte_index] & bit_mask) != 0 { 1 } else { 0 };
    
    // Set or clear the bit
    if bit_value {
        current_value[byte_index] |= bit_mask;
    } else {
        current_value[byte_index] &= !bit_mask;
    }
    
    // Store the updated value
    storage.set_string(db, key, current_value)?;
    
    Ok(RespFrame::Integer(original_bit))
}

/// Handle GETBIT command
/// GETBIT key offset
pub fn handle_getbit(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() != 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'getbit' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract offset
    let offset = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<i64>() {
                Ok(n) if n >= 0 && n <= 4294967295 => n as u64,
                Ok(_) => return Ok(RespFrame::error("ERR bit offset is not an integer or out of range")),
                Err(_) => return Ok(RespFrame::error("ERR bit offset is not an integer or out of range")),
            }
        }
        _ => return Ok(RespFrame::error("ERR bit offset is not an integer or out of range")),
    };
    
    // Get current string
    let current_value = match storage.get_string(db, key)? {
        Some(bytes) => bytes,
        None => return Ok(RespFrame::Integer(0)), // Non-existent key returns 0
    };
    
    // Calculate byte position
    let (byte_index, bit_mask) = bit_position(offset);
    
    // Return 0 if offset is beyond string length
    if byte_index >= current_value.len() {
        return Ok(RespFrame::Integer(0));
    }
    
    // Get the bit value
    let bit_value = if (current_value[byte_index] & bit_mask) != 0 { 1 } else { 0 };
    
    Ok(RespFrame::Integer(bit_value))
}

/// Handle BITCOUNT command
/// BITCOUNT key [start end [BYTE|BIT]]
pub fn handle_bitcount(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 2 || parts.len() > 5 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'bitcount' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Get current string
    let current_value = match storage.get_string(db, key)? {
        Some(bytes) => bytes,
        None => return Ok(RespFrame::Integer(0)), // Non-existent key returns 0
    };
    
    if current_value.is_empty() {
        return Ok(RespFrame::Integer(0));
    }
    
    // Determine if we're using BIT or BYTE mode (default: BYTE)
    let bit_mode = if parts.len() == 5 {
        match &parts[4] {
            RespFrame::BulkString(Some(bytes)) => {
                let mode = String::from_utf8_lossy(bytes).to_uppercase();
                match mode.as_str() {
                    "BIT" => true,
                    "BYTE" => false,
                    _ => return Ok(RespFrame::error("ERR syntax error")),
                }
            }
            _ => false,
        }
    } else {
        false
    };
    
    // Parse start and end if provided
    let (start, end) = if parts.len() >= 4 {
        let start = match &parts[2] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<i64>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                }
            }
            _ => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
        };
        
        let end = match &parts[3] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<i64>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                }
            }
            _ => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
        };
        
        (start, end)
    } else {
        // Default: entire string
        (0i64, -1i64)
    };
    
    let count = if bit_mode {
        // BIT mode: start/end are bit offsets
        let total_bits = (current_value.len() * 8) as i64;
        
        // Convert negative indices
        let start_bit = if start < 0 { 
            (total_bits + start).max(0) as usize 
        } else { 
            start as usize 
        };
        let end_bit = if end < 0 { 
            (total_bits + end).max(0) as usize 
        } else { 
            (end as usize).min(total_bits as usize - 1)
        };
        
        if start_bit > end_bit || start_bit >= total_bits as usize {
            0
        } else {
            count_bits_in_range(&current_value, start_bit, end_bit)
        }
    } else {
        // BYTE mode: start/end are byte offsets
        let len = current_value.len() as i64;
        
        // Convert negative indices
        let start_byte = if start < 0 { 
            (len + start).max(0) as usize 
        } else { 
            start as usize 
        };
        let end_byte = if end < 0 { 
            (len + end).max(0) as usize 
        } else { 
            (end as usize).min(current_value.len() - 1)
        };
        
        if start_byte > end_byte || start_byte >= current_value.len() {
            0
        } else {
            // Count bits in byte range
            current_value[start_byte..=end_byte]
                .iter()
                .map(|b| b.count_ones() as i64)
                .sum()
        }
    };
    
    Ok(RespFrame::Integer(count))
}

/// Count set bits in a specific bit range
fn count_bits_in_range(data: &[u8], start_bit: usize, end_bit: usize) -> i64 {
    let mut count = 0i64;
    
    for bit_offset in start_bit..=end_bit {
        let (byte_index, bit_mask) = bit_position(bit_offset as u64);
        if byte_index < data.len() && (data[byte_index] & bit_mask) != 0 {
            count += 1;
        }
    }
    
    count
}

/// Handle BITOP command
/// BITOP operation destkey key [key ...]
pub fn handle_bitop(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 4 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'bitop' command"));
    }
    
    // Extract operation
    let operation = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            String::from_utf8_lossy(bytes).to_uppercase()
        }
        _ => return Ok(RespFrame::error("ERR syntax error")),
    };
    
    // Extract destination key
    let destkey = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
        _ => return Ok(RespFrame::error("ERR invalid destkey format")),
    };
    
    // Validate operation and key count
    match operation.as_str() {
        "NOT" => {
            if parts.len() != 4 {
                return Ok(RespFrame::error("ERR BITOP NOT requires one and only one key"));
            }
        }
        "AND" | "OR" | "XOR" => {
            if parts.len() < 4 {
                return Ok(RespFrame::error("ERR wrong number of arguments for 'bitop' command"));
            }
        }
        _ => return Ok(RespFrame::error("ERR syntax error")),
    }
    
    // Collect source strings
    let mut sources: Vec<Vec<u8>> = Vec::new();
    let mut max_len = 0usize;
    
    for i in 3..parts.len() {
        let key = match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        let value = storage.get_string(db, key)?.unwrap_or_default();
        max_len = max_len.max(value.len());
        sources.push(value);
    }
    
    // Perform the operation
    let result = match operation.as_str() {
        "NOT" => {
            // NOT operation on single key
            let mut result = sources[0].clone();
            result.resize(max_len, 0);
            for byte in result.iter_mut() {
                *byte = !*byte;
            }
            result
        }
        "AND" => {
            let mut result = vec![0xFFu8; max_len];
            for source in &sources {
                for (i, byte) in result.iter_mut().enumerate() {
                    let src_byte = source.get(i).copied().unwrap_or(0);
                    *byte &= src_byte;
                }
            }
            result
        }
        "OR" => {
            let mut result = vec![0u8; max_len];
            for source in &sources {
                for (i, byte) in result.iter_mut().enumerate() {
                    let src_byte = source.get(i).copied().unwrap_or(0);
                    *byte |= src_byte;
                }
            }
            result
        }
        "XOR" => {
            let mut result = vec![0u8; max_len];
            for source in &sources {
                for (i, byte) in result.iter_mut().enumerate() {
                    let src_byte = source.get(i).copied().unwrap_or(0);
                    *byte ^= src_byte;
                }
            }
            result
        }
        _ => unreachable!(),
    };
    
    let result_len = result.len() as i64;
    
    // Store the result
    if result.is_empty() {
        // If result is empty, delete the key
        storage.delete(db, &destkey)?;
    } else {
        storage.set_string(db, destkey, result)?;
    }
    
    Ok(RespFrame::Integer(result_len))
}

/// Handle BITPOS command
/// BITPOS key bit [start [end [BYTE|BIT]]]
pub fn handle_bitpos(storage: &Arc<StorageEngine>, db: usize, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 3 || parts.len() > 6 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'bitpos' command"));
    }
    
    // Extract key
    let key = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
        _ => return Ok(RespFrame::error("ERR invalid key format")),
    };
    
    // Extract bit to search for
    let search_bit = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).trim() {
                "0" => false,
                "1" => true,
                _ => return Ok(RespFrame::error("ERR bit is not an integer or out of range")),
            }
        }
        _ => return Ok(RespFrame::error("ERR bit is not an integer or out of range")),
    };
    
    // Get current string
    let current_value = match storage.get_string(db, key)? {
        Some(bytes) => bytes,
        None => {
            // Key doesn't exist
            return Ok(RespFrame::Integer(if search_bit { -1 } else { 0 }));
        }
    };
    
    if current_value.is_empty() {
        return Ok(RespFrame::Integer(-1));
    }
    
    // Determine mode (BIT or BYTE)
    let bit_mode = if parts.len() == 6 {
        match &parts[5] {
            RespFrame::BulkString(Some(bytes)) => {
                let mode = String::from_utf8_lossy(bytes).to_uppercase();
                match mode.as_str() {
                    "BIT" => true,
                    "BYTE" => false,
                    _ => return Ok(RespFrame::error("ERR syntax error")),
                }
            }
            _ => false,
        }
    } else {
        false
    };
    
    // Parse start and end
    let (start, end, has_end) = if parts.len() >= 4 {
        let start = match &parts[3] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<i64>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                }
            }
            _ => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
        };
        
        if parts.len() >= 5 {
            let end = match &parts[4] {
                RespFrame::BulkString(Some(bytes)) => {
                    match String::from_utf8_lossy(bytes).parse::<i64>() {
                        Ok(n) => n,
                        Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                    }
                }
                _ => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
            };
            (start, end, true)
        } else {
            (start, -1i64, false)
        }
    } else {
        (0i64, -1i64, false)
    };
    
    let position = if bit_mode {
        // BIT mode
        let total_bits = (current_value.len() * 8) as i64;
        
        let start_bit = if start < 0 { 
            (total_bits + start).max(0) as usize 
        } else { 
            start as usize 
        };
        let end_bit = if end < 0 { 
            (total_bits + end).max(0) as usize 
        } else { 
            (end as usize).min(total_bits as usize - 1)
        };
        
        find_bit_in_range(&current_value, search_bit, start_bit, end_bit)
    } else {
        // BYTE mode
        let len = current_value.len() as i64;
        
        let start_byte = if start < 0 { 
            (len + start).max(0) as usize 
        } else { 
            start as usize 
        };
        let end_byte = if end < 0 { 
            (len + end).max(0) as usize 
        } else { 
            (end as usize).min(current_value.len() - 1)
        };
        
        // Convert to bit range
        let start_bit = start_byte * 8;
        let end_bit = if has_end {
            (end_byte + 1) * 8 - 1
        } else {
            current_value.len() * 8 - 1
        };
        
        let result = find_bit_in_range(&current_value, search_bit, start_bit, end_bit);
        
        // Special handling for searching 0 without explicit end
        if !has_end && !search_bit && result == -1 {
            // When searching for 0 without end limit, and none found in string,
            // Redis returns the first bit position after the string
            return Ok(RespFrame::Integer((current_value.len() * 8) as i64));
        }
        
        result
    };
    
    Ok(RespFrame::Integer(position))
}

/// Find the first occurrence of a bit value in a range
fn find_bit_in_range(data: &[u8], search_bit: bool, start_bit: usize, end_bit: usize) -> i64 {
    for bit_offset in start_bit..=end_bit {
        let (byte_index, bit_mask) = bit_position(bit_offset as u64);
        if byte_index >= data.len() {
            break;
        }
        
        let is_set = (data[byte_index] & bit_mask) != 0;
        if is_set == search_bit {
            return bit_offset as i64;
        }
    }
    
    -1
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bit_position() {
        // Bit 0 should be MSB of byte 0
        let (byte, mask) = bit_position(0);
        assert_eq!(byte, 0);
        assert_eq!(mask, 0b10000000);
        
        // Bit 7 should be LSB of byte 0
        let (byte, mask) = bit_position(7);
        assert_eq!(byte, 0);
        assert_eq!(mask, 0b00000001);
        
        // Bit 8 should be MSB of byte 1
        let (byte, mask) = bit_position(8);
        assert_eq!(byte, 1);
        assert_eq!(mask, 0b10000000);
    }
    
    #[test]
    fn test_count_bits_in_range() {
        // 0xFF = 11111111
        let data = vec![0xFF];
        assert_eq!(count_bits_in_range(&data, 0, 7), 8);
        assert_eq!(count_bits_in_range(&data, 0, 3), 4);
        
        // 0x80 = 10000000
        let data = vec![0x80];
        assert_eq!(count_bits_in_range(&data, 0, 7), 1);
        assert_eq!(count_bits_in_range(&data, 0, 0), 1);
        assert_eq!(count_bits_in_range(&data, 1, 7), 0);
    }
}