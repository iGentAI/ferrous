//! Administrative command implementations
//! 
//! This module provides handlers for server administration commands
//! like COMMAND (introspection) and SHUTDOWN (graceful termination).

use crate::error::Result;
use crate::protocol::resp::RespFrame;
use std::sync::Arc;

/// Handle COMMAND command - provides Redis command introspection for client compatibility
pub fn handle_command(parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() == 1 {
        // COMMAND with no arguments - return basic command metadata for client compatibility
        Ok(build_essential_commands_response())
    } else if let RespFrame::BulkString(Some(bytes)) = &parts[1] {
        if let Ok(subcommand) = std::str::from_utf8(bytes) {
            match subcommand.to_lowercase().as_str() {
                "count" => {
                    // COMMAND COUNT - return number of supported commands
                    Ok(RespFrame::Integer(114)) // Based on Ferrous documentation
                },
                "info" => {
                    // COMMAND INFO - return info for specific commands
                    if parts.len() < 3 {
                        return Ok(RespFrame::error("ERR wrong number of arguments for 'command info' command"));
                    }
                    
                    let mut results = Vec::new();
                    for i in 2..parts.len() {
                        if let RespFrame::BulkString(Some(cmd_bytes)) = &parts[i] {
                            if let Ok(cmd_name) = std::str::from_utf8(cmd_bytes) {
                                results.push(get_command_info(cmd_name));
                            } else {
                                results.push(RespFrame::null_bulk());
                            }
                        } else {
                            results.push(RespFrame::null_bulk());
                        }
                    }
                    Ok(RespFrame::Array(Some(results)))
                },
                _ => Ok(RespFrame::error(format!("ERR unknown subcommand '{}'", subcommand)))
            }
        } else {
            Ok(RespFrame::error("ERR invalid subcommand"))
        }
    } else {
        Ok(RespFrame::error("ERR invalid subcommand format"))
    }
}

/// Handle SHUTDOWN command - graceful server shutdown
pub fn handle_shutdown(parts: &[RespFrame], storage: &Arc<crate::storage::StorageEngine>, rdb_engine: Option<&Arc<crate::storage::RdbEngine>>) -> Result<RespFrame> {
    let save_before_shutdown = if parts.len() == 1 {
        true // Default: save on shutdown
    } else if parts.len() == 2 {
        match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => {
                match std::str::from_utf8(bytes).unwrap_or("").to_uppercase().as_str() {
                    "SAVE" => true,
                    "NOSAVE" => false,
                    _ => return Ok(RespFrame::error("ERR Invalid option. Valid values: SAVE, NOSAVE")),
                }
            }
            _ => return Ok(RespFrame::error("ERR Invalid option format")),
        }
    } else {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'shutdown' command"));
    };
    
    // Perform save if requested and RDB engine is available
    if save_before_shutdown {
        if let Some(rdb) = rdb_engine {
            println!("SHUTDOWN: Performing RDB save...");
            if let Err(e) = rdb.save(storage) {
                eprintln!("Warning: RDB save failed during shutdown: {}", e);
            } else {
                println!("SHUTDOWN: RDB save completed successfully");
            }
        } else {
            println!("SHUTDOWN: RDB engine not available, skipping save");
        }
    } else {
        println!("SHUTDOWN: Skipping save (NOSAVE specified)");
    }
    
    println!("SHUTDOWN: Initiating graceful server termination");
    
    // Send OK response then exit after brief delay
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(100));
        std::process::exit(0);
    });
    
    Ok(RespFrame::ok())
}

/// Build essential commands response for COMMAND command
fn build_essential_commands_response() -> RespFrame {
    // Return a minimal set of commands for client compatibility
    // Format: [name, arity, flags, first_key_pos, last_key_pos, step]
    let commands = vec![
        // Core commands that Redis clients expect
        cmd_info("ping", -1, &["stale", "fast"], 0, 0, 1),
        cmd_info("command", 0, &["loading", "stale"], 0, 0, 1), 
        cmd_info("info", -1, &["loading", "stale"], 0, 0, 1),
        cmd_info("set", -3, &["write", "denyoom"], 1, 1, 1),
        cmd_info("get", 2, &["readonly", "fast"], 1, 1, 1),
        cmd_info("del", -2, &["write"], 1, -1, 1),
        cmd_info("exists", -2, &["readonly", "fast"], 1, -1, 1),
        cmd_info("expire", 3, &["write", "fast"], 1, 1, 1),
        cmd_info("ttl", 2, &["readonly", "fast"], 1, 1, 1),
        cmd_info("type", 2, &["readonly", "fast"], 1, 1, 1),
        cmd_info("shutdown", -1, &["admin", "noscript", "no_async_loading"], 0, 0, 1),
        // List commands
        cmd_info("lpush", -3, &["write", "denyoom", "fast"], 1, 1, 1),
        cmd_info("rpush", -3, &["write", "denyoom", "fast"], 1, 1, 1),
        cmd_info("lpop", -2, &["write", "fast"], 1, 1, 1),
        cmd_info("rpop", -2, &["write", "fast"], 1, 1, 1),
        cmd_info("llen", 2, &["readonly", "fast"], 1, 1, 1),
        cmd_info("lrange", 4, &["readonly"], 1, 1, 1),
        // Scripting
        cmd_info("eval", -3, &["noscript", "movablekeys"], 0, 0, 1),
        cmd_info("evalsha", -3, &["noscript", "movablekeys"], 0, 0, 1),
        cmd_info("script", -2, &["noscript"], 0, 0, 1),
    ];
    
    RespFrame::Array(Some(commands))
}

/// Helper to build command info array
fn cmd_info(name: &str, arity: i64, flags: &[&str], first: i64, last: i64, step: i64) -> RespFrame {
    let flag_frames: Vec<RespFrame> = flags.iter()
        .map(|&f| RespFrame::from_string(f))
        .collect();
        
    RespFrame::Array(Some(vec![
        RespFrame::from_string(name),
        RespFrame::Integer(arity),
        RespFrame::Array(Some(flag_frames)),
        RespFrame::Integer(first),
        RespFrame::Integer(last), 
        RespFrame::Integer(step),
    ]))
}

/// Get command info for a specific command
fn get_command_info(cmd_name: &str) -> RespFrame {
    match cmd_name.to_lowercase().as_str() {
        "ping" => cmd_info("ping", -1, &["stale", "fast"], 0, 0, 1),
        "command" => cmd_info("command", 0, &["loading", "stale"], 0, 0, 1),
        "set" => cmd_info("set", -3, &["write", "denyoom"], 1, 1, 1),
        "get" => cmd_info("get", 2, &["readonly", "fast"], 1, 1, 1),
        "del" => cmd_info("del", -2, &["write"], 1, -1, 1),
        "exists" => cmd_info("exists", -2, &["readonly", "fast"], 1, -1, 1),
        "lpush" => cmd_info("lpush", -3, &["write", "denyoom", "fast"], 1, 1, 1),
        "lpop" => cmd_info("lpop", -2, &["write", "fast"], 1, 1, 1),
        "lrange" => cmd_info("lrange", 4, &["readonly"], 1, 1, 1),
        "eval" => cmd_info("eval", -3, &["noscript", "movablekeys"], 0, 0, 1),
        "shutdown" => cmd_info("shutdown", -1, &["admin", "noscript"], 0, 0, 1),
        _ => RespFrame::null_bulk(),
    }
}