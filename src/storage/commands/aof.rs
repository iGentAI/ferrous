//! AOF (Append Only File) command implementations
//! 
//! Provides Redis-compatible AOF persistence commands.

use crate::error::Result;
use crate::protocol::RespFrame;
use crate::storage::aof::AofEngine;
use std::sync::Arc;

/// Handle BGREWRITEAOF command - Background AOF rewrite
pub fn handle_bgrewriteaof(aof_engine: Option<&Arc<AofEngine>>) -> Result<RespFrame> {
    if let Some(aof) = aof_engine {
        match aof.bgrewrite() {
            Ok(_) => Ok(RespFrame::SimpleString(Arc::new(b"Background append only file rewriting started".to_vec()))),
            Err(e) => Ok(RespFrame::error(format!("ERR {}", e))),
        }
    } else {
        Ok(RespFrame::error("ERR AOF is disabled"))
    }
}