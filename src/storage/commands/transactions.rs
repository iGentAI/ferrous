//! Transaction command implementations
//! 
//! Provides Redis-compatible transaction support with MULTI/EXEC/DISCARD/WATCH.

use crate::error::{FerrousError, Result, CommandError};
use crate::protocol::RespFrame;
use crate::storage::StorageEngine;
use crate::network::Connection;
use std::sync::Arc;
use std::collections::{HashMap, VecDeque};

/// Transaction state for a connection
#[derive(Debug, Default)]
pub struct TransactionState {
    /// Whether we're in a transaction
    pub in_transaction: bool,
    /// Queued commands
    pub queued_commands: VecDeque<Vec<RespFrame>>,
    /// Watched keys with their baseline modification counters (key -> counter when watched)
    pub watched_keys: HashMap<Vec<u8>, u64>,
    /// Whether the transaction is aborted due to watched key changes
    pub aborted: bool,
}

/// Handle MULTI command - Start a transaction
pub fn handle_multi(conn: &mut Connection) -> Result<RespFrame> {
    if conn.transaction_state.in_transaction {
        return Ok(RespFrame::error("ERR MULTI calls can not be nested"));
    }
    
    conn.transaction_state.in_transaction = true;
    conn.transaction_state.queued_commands.clear();
    conn.transaction_state.aborted = false;
    
    Ok(RespFrame::ok())
}

/// Handle EXEC command - Execute transaction
pub fn handle_exec(
    conn: &mut Connection, 
    storage: &Arc<StorageEngine>,
    process_command_fn: impl Fn(&Arc<StorageEngine>, &Vec<RespFrame>, &mut Connection) -> Result<RespFrame>
) -> Result<RespFrame> {
    if !conn.transaction_state.in_transaction {
        return Ok(RespFrame::error("ERR EXEC without MULTI"));
    }
    
    // Clear transaction state
    conn.transaction_state.in_transaction = false;
    let commands = std::mem::take(&mut conn.transaction_state.queued_commands);
    
    // Check if transaction was aborted
    if conn.transaction_state.aborted {
        conn.transaction_state.watched_keys.clear();
        return Ok(RespFrame::null_array());
    }
    
    // Check watched keys
    for (key, baseline_counter) in &conn.transaction_state.watched_keys {
        if storage.was_modified_since(conn.db_index, key, *baseline_counter)? {
            conn.transaction_state.watched_keys.clear();
            return Ok(RespFrame::null_array());
        }
    }
    
    // Clear watched keys
    conn.transaction_state.watched_keys.clear();
    
    // Execute all commands
    let mut results = Vec::new();
    for cmd in commands {
        match process_command_fn(storage, &cmd, conn) {
            Ok(response) => results.push(response),
            Err(e) => results.push(RespFrame::error(e.to_string())),
        }
    }
    
    Ok(RespFrame::Array(Some(results)))
}

/// Handle DISCARD command - Abort transaction
pub fn handle_discard(conn: &mut Connection) -> Result<RespFrame> {
    if !conn.transaction_state.in_transaction {
        return Ok(RespFrame::error("ERR DISCARD without MULTI"));
    }
    
    conn.transaction_state.in_transaction = false;
    conn.transaction_state.queued_commands.clear();
    conn.transaction_state.watched_keys.clear();
    conn.transaction_state.aborted = false;
    
    Ok(RespFrame::ok())
}

/// Handle WATCH command - Watch keys for changes
pub fn handle_watch(conn: &mut Connection, parts: &[RespFrame], storage: &Arc<StorageEngine>) -> Result<RespFrame> {
    if parts.len() < 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'watch' command"));
    }
    
    if conn.transaction_state.in_transaction {
        return Ok(RespFrame::error("ERR WATCH inside MULTI is not allowed"));
    }
    
    // Add keys to watch set with current modification counters
    for i in 1..parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => {
                let key = bytes.as_ref().clone();
                
                // Get current atomic modification counter for baseline
                match storage.get_modification_counter(conn.db_index, &key) {
                    Ok(baseline_counter) => {
                        // Store the atomic counter value as baseline
                        conn.transaction_state.watched_keys.insert(key, baseline_counter);
                    }
                    Err(_) => {
                        // If we can't get the counter, use 0 as fallback
                        conn.transaction_state.watched_keys.insert(key, 0);
                    }
                }
            }
            _ => {
                return Ok(RespFrame::error("ERR invalid key format"));
            }
        }
    }
    
    Ok(RespFrame::ok())
}

/// Handle UNWATCH command - Unwatch all keys
pub fn handle_unwatch(conn: &mut Connection) -> Result<RespFrame> {
    conn.transaction_state.watched_keys.clear();
    Ok(RespFrame::ok())
}

/// Check if we should queue a command (we're in a transaction)
pub fn should_queue_command(command: &str) -> bool {
    !matches!(command, "MULTI" | "EXEC" | "DISCARD" | "WATCH" | "UNWATCH")
}

/// Queue a command for later execution
pub fn queue_command(conn: &mut Connection, parts: Vec<RespFrame>) -> Result<RespFrame> {
    conn.transaction_state.queued_commands.push_back(parts);
    Ok(RespFrame::SimpleString(Arc::new(b"QUEUED".to_vec())))
}