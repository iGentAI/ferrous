//! Replication command handlers

use std::sync::Arc;
use std::net::{SocketAddr, ToSocketAddrs};
use crate::error::{FerrousError, Result, CommandError};
use crate::protocol::RespFrame;
use crate::storage::{StorageEngine, RdbEngine};
use crate::network::Connection;
use super::ReplicationManager;

/// Handle REPLICAOF command (modern version of SLAVEOF)
pub fn handle_replicaof(
    parts: &[RespFrame],
    manager: &Arc<ReplicationManager>,
    storage: &Arc<StorageEngine>
) -> Result<RespFrame> {
    // REPLICAOF host port | REPLICAOF NO ONE
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'replicaof' command"));
    }
    
    // Extract host
    let host = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            String::from_utf8(bytes.as_ref().clone())
                .map_err(|_| CommandError::Generic("invalid host format".into()))?
        }
        _ => return Ok(RespFrame::error("ERR invalid host format")),
    };
    
    // Check for NO ONE
    if host.to_uppercase() == "NO" {
        if parts.len() != 3 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'replicaof' command"));
        }
        
        match &parts[2] {
            RespFrame::BulkString(Some(bytes)) => {
                let arg = String::from_utf8(bytes.as_ref().clone())
                    .map_err(|_| CommandError::Generic("invalid argument format".into()))?;
                
                if arg.to_uppercase() != "ONE" {
                    return Ok(RespFrame::error("ERR syntax error"));
                }
            }
            _ => return Ok(RespFrame::error("ERR syntax error")),
        }
        
        // Set as master
        manager.set_master(None, Arc::clone(storage))?;
        Ok(RespFrame::ok())
    } else {
        // Extract port
        let port = match &parts[2] {
            RespFrame::BulkString(Some(bytes)) => {
                let port_str = String::from_utf8(bytes.as_ref().clone())
                    .map_err(|_| CommandError::Generic("invalid port format".into()))?;
                
                port_str.parse::<u16>()
                    .map_err(|_| CommandError::Generic("invalid port number".into()))?
            }
            _ => return Ok(RespFrame::error("ERR invalid port format")),
        };
        
        // Parse address
        let addr_str = format!("{}:{}", host, port);
        let addr: SocketAddr = addr_str.to_socket_addrs()
            .map_err(|_| CommandError::Generic("invalid address".into()))?
            .next()
            .ok_or(CommandError::Generic("failed to resolve address".into()))?;
        
        // Set as replica
        manager.set_master(Some(addr), Arc::clone(storage))?;
        
        // TODO: Start background task to connect to master
        
        Ok(RespFrame::ok())
    }
}

/// Handle REPLCONF command
pub fn handle_replconf(
    parts: &[RespFrame],
    conn: &mut Connection,
    manager: &Arc<ReplicationManager>,
) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'replconf' command"));
    }
    
    // Extract subcommand
    let subcommand = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            String::from_utf8_lossy(bytes).to_uppercase()
        }
        _ => return Ok(RespFrame::error("ERR invalid subcommand format")),
    };
    
    match subcommand.as_str() {
        "LISTENING-PORT" => {
            // Replica is telling us its listening port
            // For now, just acknowledge
            Ok(RespFrame::ok())
        }
        
        "CAPA" => {
            // Replica is telling us its capabilities
            // For now, just acknowledge
            Ok(RespFrame::ok())
        }
        
        "ACK" => {
            // Replica is acknowledging received offset
            if parts.len() != 3 {
                return Ok(RespFrame::error("ERR wrong number of arguments"));
            }
            
            let offset = match &parts[2] {
                RespFrame::BulkString(Some(bytes)) => {
                    String::from_utf8_lossy(bytes).parse::<u64>()
                        .map_err(|_| FerrousError::Command(CommandError::Generic("invalid offset".to_string())))?
                }
                _ => return Ok(RespFrame::error("ERR invalid offset format")),
            };
            
            // Update replica's acknowledged offset
            if let Some(replica) = manager.get_replicas().iter()
                .find(|r| r.conn_id == conn.id) {
                replica.update_ack_offset(offset);
                replica.touch();
            }
            
            Ok(RespFrame::ok())
        }
        
        "GETACK" => {
            // Master is asking for our current offset
            if !manager.is_replica() {
                return Ok(RespFrame::error("ERR wrong role"));
            }
            
            let offset = manager.get_repl_offset();
            
            // Return REPLCONF ACK <offset>
            Ok(RespFrame::Array(Some(vec![
                RespFrame::BulkString(Some(Arc::new(b"REPLCONF".to_vec()))),
                RespFrame::BulkString(Some(Arc::new(b"ACK".to_vec()))),
                RespFrame::BulkString(Some(Arc::new(offset.to_string().into_bytes()))),
            ])))
        }
        
        _ => Ok(RespFrame::error("ERR unknown REPLCONF subcommand")),
    }
}

/// Handle SYNC command (deprecated, prefer PSYNC)
pub fn handle_sync(
    manager: &Arc<ReplicationManager>,
    storage: &Arc<StorageEngine>,
    rdb_engine: &Arc<RdbEngine>,
) -> Result<RespFrame> {
    use super::sync::SyncProtocol;
    SyncProtocol::handle_sync(manager, storage, rdb_engine)
}

/// Handle PSYNC command
pub fn handle_psync(
    parts: &[RespFrame],
    manager: &Arc<ReplicationManager>,
    storage: &Arc<StorageEngine>,
    rdb_engine: &Arc<RdbEngine>,
) -> Result<RespFrame> {
    if parts.len() != 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'psync' command"));
    }
    
    // Extract replication ID
    let repl_id = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            let id = String::from_utf8_lossy(bytes);
            if id == "?" {
                None
            } else {
                Some(id.to_string())
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid replication ID format")),
    };
    
    // Extract offset
    let offset = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            let offset_str = String::from_utf8_lossy(bytes);
            if offset_str == "-1" {
                None
            } else {
                match offset_str.parse::<u64>() {
                    Ok(o) => Some(o),
                    Err(_) => return Ok(RespFrame::error("ERR invalid offset format")),
                }
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid offset format")),
    };
    
    use super::sync::{SyncProtocol, PsyncResult};
    
    match SyncProtocol::handle_psync(manager, storage, rdb_engine, repl_id, offset)? {
        PsyncResult::FullResync { repl_id, offset } => {
            // Return +FULLRESYNC <replid> <offset>
            let response = format!("FULLRESYNC {} {}", repl_id, offset);
            Ok(RespFrame::SimpleString(Arc::new(response.into_bytes())))
        }
        PsyncResult::PartialResync { .. } => {
            // Return +CONTINUE
            Ok(RespFrame::SimpleString(Arc::new(b"CONTINUE".to_vec())))
        }
    }
}

/// Handle SLAVEOF command (deprecated, mapped to REPLICAOF)
pub fn handle_slaveof(
    parts: &[RespFrame],
    manager: &Arc<ReplicationManager>,
    storage: &Arc<StorageEngine>
) -> Result<RespFrame> {
    handle_replicaof(parts, manager, storage)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_replicaof_no_one() {
        let parts = vec![
            RespFrame::bulk_string(b"REPLICAOF"),
            RespFrame::bulk_string(b"NO"),
            RespFrame::bulk_string(b"ONE"),
        ];
        
        let config = super::super::ReplicationConfig::default();
        let manager = ReplicationManager::new(config);
        let storage = Arc::new(StorageEngine::new());
        
        let result = handle_replicaof(&parts, &manager, &storage).unwrap();
        assert!(matches!(result, RespFrame::SimpleString(_)));
        assert!(manager.is_master());
    }
}