//! MONITOR command handler

use crate::error::Result;
use crate::protocol::RespFrame;

/// Handle MONITOR command
/// 
/// The MONITOR command streams back every command processed by the Redis server.
/// It's useful for debugging what your application is doing.
pub fn handle_monitor(monitor_conn_id: u64) -> Result<RespFrame> {
    // Return OK to indicate monitoring has started
    // The actual subscription to monitoring happens in the server code
    Ok(RespFrame::ok())
}