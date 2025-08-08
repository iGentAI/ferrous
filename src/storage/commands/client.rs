//! CLIENT command implementations
//!
//! This module provides Redis-compatible CLIENT commands for
//! examining and managing client connections.

use std::time::{Duration, SystemTime, UNIX_EPOCH};
use crate::error::Result;
use crate::protocol::RespFrame;
use crate::network::Connection;

/// Handle the CLIENT command and its various subcommands
pub fn handle_client(
    parts: &[RespFrame], 
    connections: &impl ConnectionProvider, 
    this_conn_id: u64,
    paused_until: Option<&mut SystemTime>
) -> Result<RespFrame> {
    if parts.len() < 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'client' command"));
    }
    
    let subcommand = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            String::from_utf8_lossy(bytes).to_uppercase()
        }
        _ => return Ok(RespFrame::error("ERR syntax error")),
    };
    
    match subcommand.as_str() {
        "LIST" => handle_client_list(parts, connections),
        "KILL" => handle_client_kill(parts, connections, this_conn_id),
        "SETNAME" => handle_client_setname(parts, connections, this_conn_id),
        "GETNAME" => handle_client_getname(parts, connections, this_conn_id),
        "ID" => handle_client_id(parts, this_conn_id),
        "PAUSE" => handle_client_pause(parts, paused_until),
        "UNPAUSE" => handle_client_unpause(parts, paused_until),
        _ => Ok(RespFrame::error(format!("ERR Unknown subcommand or wrong number of arguments for '{}'", subcommand))),
    }
}

/// Connection provider trait for testing and dependency inversion
pub trait ConnectionProvider {
    /// Execute a function on a specific connection
    fn with_connection<F, R>(&self, id: u64, f: F) -> Option<R>
    where
        F: FnOnce(&mut Connection) -> R;
    
    /// Get all connection IDs
    fn all_connection_ids(&self) -> Vec<u64>;
    
    /// Close a connection by ID
    fn close_connection(&self, id: u64) -> bool;
}

/// Handle the CLIENT LIST command
fn handle_client_list(parts: &[RespFrame], connections: &impl ConnectionProvider) -> Result<RespFrame> {
    if parts.len() != 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'client list' command"));
    }
    
    let mut result = String::new();
    let conn_ids = connections.all_connection_ids();
    
    for id in conn_ids {
        let conn_info = connections.with_connection(id, |conn| {
            // Instant.elapsed() returns a Duration directly, no need for Result handling
            let age_secs = conn.created_at.elapsed().as_secs();
            
            format!(
                "id={} addr={} fd={} name={} age={} idle={} flags={} db={} cmd={}\n",
                id,
                conn.addr,
                id, // Using ID as a proxy for file descriptor
                conn.name.as_deref().unwrap_or(""),
                age_secs,
                conn.idle_time().as_secs(),
                get_client_flags(conn),
                conn.db_index,
                "unknown" // We're not tracking last command yet
            )
        });
        
        if let Some(info) = conn_info {
            result.push_str(&info);
        }
    }
    
    // Return as bulk string
    Ok(RespFrame::from_string(result))
}

/// Handle the CLIENT KILL command
fn handle_client_kill(parts: &[RespFrame], connections: &impl ConnectionProvider, this_conn_id: u64) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'client kill' command"));
    }
    
    // Simplified CLIENT KILL implementation - just by ID or ADDRESS
    // Redis has more complex filtering options
    
    let arg_type = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_uppercase(),
        _ => return Ok(RespFrame::error("ERR syntax error")),
    };
    
    match arg_type.as_str() {
        "ID" => {
            if parts.len() != 4 {
                return Ok(RespFrame::error("ERR syntax error"));
            }
            
            let id_str = match &parts[3] {
                RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
                _ => return Ok(RespFrame::error("ERR syntax error")),
            };
            
            let id = match id_str.parse::<u64>() {
                Ok(id) => id,
                Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
            };
            
            // Don't allow killing self
            if id == this_conn_id {
                return Ok(RespFrame::error("ERR cannot kill self"));
            }
            
            if connections.close_connection(id) {
                Ok(RespFrame::Integer(1)) // 1 client killed
            } else {
                Ok(RespFrame::Integer(0)) // No clients killed
            }
        },
        "ADDR" => {
            if parts.len() != 4 {
                return Ok(RespFrame::error("ERR syntax error"));
            }
            
            let addr_str = match &parts[3] {
                RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
                _ => return Ok(RespFrame::error("ERR syntax error")),
            };
            
            let mut killed = 0;
            
            for id in connections.all_connection_ids() {
                // Skip self
                if id == this_conn_id {
                    continue;
                }
                
                let should_kill = connections.with_connection(id, |conn| {
                    conn.addr.to_string() == addr_str
                }).unwrap_or(false);
                
                if should_kill && connections.close_connection(id) {
                    killed += 1;
                }
            }
            
            Ok(RespFrame::Integer(killed))
        },
        _ => Ok(RespFrame::error("ERR syntax error")),
    }
}

/// Handle the CLIENT SETNAME command
fn handle_client_setname(parts: &[RespFrame], connections: &impl ConnectionProvider, conn_id: u64) -> Result<RespFrame> {
    if parts.len() != 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'client setname' command"));
    }
    
    let name = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
        _ => return Ok(RespFrame::error("ERR syntax error")),
    };
    
    // Set the name on the connection
    let result = connections.with_connection(conn_id, |conn| {
        conn.name = Some(name);
        true
    });
    
    if result.is_some() {
        Ok(RespFrame::ok())
    } else {
        Ok(RespFrame::error("ERR connection not found"))
    }
}

/// Handle the CLIENT GETNAME command
fn handle_client_getname(parts: &[RespFrame], connections: &impl ConnectionProvider, conn_id: u64) -> Result<RespFrame> {
    if parts.len() != 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'client getname' command"));
    }
    
    let name = connections.with_connection(conn_id, |conn| {
        conn.name.clone()
    });
    
    match name {
        Some(Some(name)) => Ok(RespFrame::from_string(name)),
        Some(None) => Ok(RespFrame::null_bulk()),
        None => Ok(RespFrame::error("ERR connection not found")),
    }
}

/// Handle the CLIENT ID command
fn handle_client_id(parts: &[RespFrame], conn_id: u64) -> Result<RespFrame> {
    if parts.len() != 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'client id' command"));
    }
    
    Ok(RespFrame::Integer(conn_id as i64))
}

/// Handle the CLIENT PAUSE command
fn handle_client_pause(parts: &[RespFrame], paused_until: Option<&mut SystemTime>) -> Result<RespFrame> {
    if parts.len() != 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'client pause' command"));
    }
    
    // Extract timeout in milliseconds
    let timeout_ms = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<u64>() {
                Ok(ms) => ms,
                Err(_) => return Ok(RespFrame::error("ERR timeout is not an integer or out of range")),
            }
        },
        _ => return Ok(RespFrame::error("ERR syntax error")),
    };
    
    // Validate timeout isn't too large (prevent overflow)
    if timeout_ms > 2_147_483_647 { // Max i32 value
        return Ok(RespFrame::error("ERR timeout is out of range"));
    }
    
    // Set pause until timestamp
    if let Some(paused) = paused_until {
        let now = SystemTime::now();
        *paused = now + Duration::from_millis(timeout_ms);
        Ok(RespFrame::ok())
    } else {
        // If no pause_until field is available
        Ok(RespFrame::error("ERR client pause not supported in this context"))
    }
}

/// Handle the CLIENT UNPAUSE command
fn handle_client_unpause(parts: &[RespFrame], paused_until: Option<&mut SystemTime>) -> Result<RespFrame> {
    if parts.len() != 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'client unpause' command"));
    }
    
    if let Some(paused) = paused_until {
        // Set to UNIX_EPOCH to effectively disable the pause
        *paused = UNIX_EPOCH;
        Ok(RespFrame::ok())
    } else {
        // If no pause_until field is available
        Ok(RespFrame::error("ERR client pause not supported in this context"))
    }
}

/// Helper to format client flags
fn get_client_flags(conn: &Connection) -> String {
    let mut flags = Vec::new();
    
    if conn.is_monitoring {
        flags.push("M"); // Monitoring client
    }
    
    if conn.transaction_state.in_transaction {
        flags.push("x"); // In MULTI/EXEC transaction
    }
    
    // Add more flags as needed
    
    // Default to 'N' (normal) if no special flags
    if flags.is_empty() {
        flags.push("N");
    }
    
    flags.join("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::cell::RefCell;
    
    struct MockConnectionProvider {
        connections: Arc<Mutex<HashMap<u64, RefCell<Connection>>>>,
        killed: Arc<Mutex<Vec<u64>>>,
    }
    
    impl MockConnectionProvider {
        fn new() -> Self {
            Self {
                connections: Arc::new(Mutex::new(HashMap::new())),
                killed: Arc::new(Mutex::new(Vec::new())),
            }
        }
        
        fn add_connection(&mut self, id: u64, conn: Connection) {
            let mut connections = self.connections.lock().unwrap();
            connections.insert(id, RefCell::new(conn));
        }
    }
    
    impl ConnectionProvider for MockConnectionProvider {
        fn with_connection<F, R>(&self, id: u64, f: F) -> Option<R>
        where
            F: FnOnce(&mut Connection) -> R,
        {
            let connections = self.connections.lock().unwrap();
            connections.get(&id).map(|conn_ref| {
                let mut conn = conn_ref.borrow_mut();
                f(&mut *conn)
            })
        }
        
        fn all_connection_ids(&self) -> Vec<u64> {
            let connections = self.connections.lock().unwrap();
            connections.keys().copied().collect()
        }
        
        fn close_connection(&self, id: u64) -> bool {
            let connections = self.connections.lock().unwrap();
            let exists = connections.contains_key(&id);
            if exists {
                let mut killed = self.killed.lock().unwrap();
                killed.push(id);
            }
            exists
        }
    }
    
    // Tests would go here
}