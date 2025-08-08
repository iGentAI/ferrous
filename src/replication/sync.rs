//! Synchronization protocol implementation (SYNC/PSYNC)

use std::sync::Arc;
use std::io::Write;
use std::net::TcpStream;
use crate::error::{FerrousError, Result};
use crate::protocol::{RespFrame, serialize_resp_frame};
use crate::storage::{StorageEngine, RdbEngine};
use crate::network::Connection;
use super::{ReplicationManager, ReplicationRole};

/// Result of PSYNC command
#[derive(Debug)]
pub enum PsyncResult {
    /// Full resynchronization needed
    FullResync {
        repl_id: String,
        offset: u64,
    },
    
    /// Partial resynchronization possible
    PartialResync {
        backlog_data: Vec<u8>,
    },
}

/// Synchronization protocol handler
pub struct SyncProtocol;

impl SyncProtocol {
    /// Handle SYNC command from replica (deprecated, use PSYNC)
    pub fn handle_sync(
        manager: &Arc<ReplicationManager>,
        storage: &Arc<StorageEngine>,
        rdb_engine: &Arc<RdbEngine>,
    ) -> Result<RespFrame> {
        // Check if we're a master
        if !manager.is_master() {
            return Ok(RespFrame::error("ERR wrong role"));
        }
        
        // For SYNC, always do full resynchronization
        Self::perform_full_sync(manager, storage, rdb_engine)
    }
    
    /// Handle PSYNC command from replica
    pub fn handle_psync(
        manager: &Arc<ReplicationManager>,
        storage: &Arc<StorageEngine>,
        rdb_engine: &Arc<RdbEngine>,
        repl_id: Option<String>,
        offset: Option<u64>,
    ) -> Result<PsyncResult> {
        // Check if we're a master
        if !manager.is_master() {
            return Err(FerrousError::Command(
                crate::error::CommandError::Generic("wrong role".into())
            ));
        }
        
        let current_role = manager.role();
        
        match current_role {
            ReplicationRole::Master { repl_id: master_repl_id, repl_offset, .. } => {
                let current_offset = repl_offset.load(std::sync::atomic::Ordering::SeqCst);
                
                // Check if partial resync is possible
                if let (Some(replica_repl_id), Some(replica_offset)) = (repl_id, offset) {
                    if replica_repl_id == master_repl_id || replica_repl_id == "?" {
                        // Check if we have the data in backlog
                        if let Ok(backlog_data) = manager.get_backlog_data(replica_offset) {
                            return Ok(PsyncResult::PartialResync { backlog_data });
                        }
                    }
                }
                
                // Full resync needed
                Ok(PsyncResult::FullResync {
                    repl_id: master_repl_id,
                    offset: current_offset,
                })
            }
            _ => unreachable!(), // Already checked we're a master
        }
    }
    
    /// Perform full synchronization
    pub fn perform_full_sync(
        manager: &Arc<ReplicationManager>,
        storage: &Arc<StorageEngine>,
        rdb_engine: &Arc<RdbEngine>,
    ) -> Result<RespFrame> {
        // Get current replication info
        let (repl_id, offset) = match manager.role() {
            ReplicationRole::Master { repl_id, repl_offset, .. } => {
                (repl_id.clone(), repl_offset.load(std::sync::atomic::Ordering::SeqCst))
            }
            _ => return Ok(RespFrame::error("ERR wrong role")),
        };
        
        // Generate RDB data
        println!("SyncProtocol: Generating RDB data for replication");
        let rdb_data = match rdb_engine.generate_rdb_bytes(storage) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Error generating RDB: {}", e);
                return Ok(RespFrame::error(format!("ERR RDB generation failed: {}", e)));
            }
        };
        
        println!("SyncProtocol: Generated {} bytes of RDB data", rdb_data.len());
        
        // Response format: +FULLRESYNC <replid> <offset>\r\n
        let response = format!("FULLRESYNC {} {}", repl_id, offset);
        println!("SyncProtocol: Responding with: {}", response);
        
        Ok(RespFrame::SimpleString(Arc::new(response.into_bytes())))
    }
    
    /// Send RDB file to replica
    pub fn send_rdb_to_replica(
        connection: &mut Connection,
        storage: &Arc<StorageEngine>,
        rdb_engine: &Arc<RdbEngine>,
    ) -> Result<()> {
        println!("SyncProtocol: Sending RDB file to replica");
        
        // Generate RDB data
        let rdb_data = rdb_engine.generate_rdb_bytes(storage)?;
        println!("SyncProtocol: Generated {} bytes of RDB data", rdb_data.len());
        
        // Send as bulk string
        // Format: $<length>\r\n<rdb_data>\r\n
        let header = format!("${}\r\n", rdb_data.len());
        connection.send_raw(header.as_bytes())?;
        connection.send_raw(&rdb_data)?;
        connection.send_raw(b"\r\n")?;
        connection.flush()?;
        
        println!("SyncProtocol: RDB transfer complete");
        
        Ok(())
    }
    
    /// Connect to master and perform synchronization (for replica)
    pub fn sync_with_master(
        master_addr: std::net::SocketAddr,
        manager: &Arc<ReplicationManager>,
        storage: &Arc<StorageEngine>,
    ) -> Result<TcpStream> {
        // Connect to master
        let mut stream = TcpStream::connect(master_addr)?;
        stream.set_nodelay(true)?;
        
        // Send PING to check connection
        let ping = RespFrame::Array(Some(vec![
            RespFrame::BulkString(Some(Arc::new(b"PING".to_vec()))),
        ]));
        let mut write_buf = Vec::new();
        serialize_resp_frame(&ping, &mut write_buf)?;
        stream.write_all(&write_buf)?;
        
        // TODO: Read PONG response
        
        // Send REPLCONF listening-port
        let replconf = RespFrame::Array(Some(vec![
            RespFrame::BulkString(Some(Arc::new(b"REPLCONF".to_vec()))),
            RespFrame::BulkString(Some(Arc::new(b"listening-port".to_vec()))),
            RespFrame::BulkString(Some(Arc::new(b"6379".to_vec()))), // TODO: Get actual port
        ]));
        write_buf.clear();
        serialize_resp_frame(&replconf, &mut write_buf)?;
        stream.write_all(&write_buf)?;
        
        // TODO: Read +OK response
        
        // Send REPLCONF capa
        let replconf_capa = RespFrame::Array(Some(vec![
            RespFrame::BulkString(Some(Arc::new(b"REPLCONF".to_vec()))),
            RespFrame::BulkString(Some(Arc::new(b"capa".to_vec()))),
            RespFrame::BulkString(Some(Arc::new(b"psync2".to_vec()))),
        ]));
        write_buf.clear();
        serialize_resp_frame(&replconf_capa, &mut write_buf)?;
        stream.write_all(&write_buf)?;
        
        // TODO: Read +OK response
        
        // Send PSYNC
        let psync = RespFrame::Array(Some(vec![
            RespFrame::BulkString(Some(Arc::new(b"PSYNC".to_vec()))),
            RespFrame::BulkString(Some(Arc::new(b"?".to_vec()))),
            RespFrame::BulkString(Some(Arc::new(b"-1".to_vec()))),
        ]));
        write_buf.clear();
        serialize_resp_frame(&psync, &mut write_buf)?;
        stream.write_all(&write_buf)?;
        
        // TODO: Read FULLRESYNC response and RDB data
        
        // Update link status
        manager.update_master_link_status(super::MasterLinkStatus::Up)?;
        
        Ok(stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_psync_result() {
        // Test FullResync
        let result = PsyncResult::FullResync {
            repl_id: "abc123".to_string(),
            offset: 100,
        };
        
        match result {
            PsyncResult::FullResync { repl_id, offset } => {
                assert_eq!(repl_id, "abc123");
                assert_eq!(offset, 100);
            }
            _ => panic!("Wrong result type"),
        }
    }
}