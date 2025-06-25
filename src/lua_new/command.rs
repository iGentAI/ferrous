//! Redis Lua command implementation
//!
//! This module implements the EVAL, EVALSHA, and SCRIPT commands
//! compatible with Redis behavior.

use std::sync::Arc;
use crate::protocol::resp::RespFrame;
use crate::storage::engine::StorageEngine;
use crate::error::{Result, FerrousError};
use crate::lua_new::executor::ScriptExecutor;

/// Command context for Lua script execution
pub struct CommandContext {
    /// Current database index
    pub db: usize,
    
    /// Storage engine reference
    pub storage: Arc<StorageEngine>,
    
    /// Script executor
    pub script_executor: Arc<ScriptExecutor>,
}

/// Handle EVAL command - EVAL script numkeys key [key ...] arg [arg ...]
pub fn handle_eval(ctx: &CommandContext, parts: &[RespFrame]) -> Result<RespFrame> {
    println!("[LUA_CMD] Processing EVAL command with {} parts", parts.len());
    
    if parts.len() < 3 {
        println!("[LUA_CMD] Error: wrong number of arguments, got {}", parts.len());
        return Ok(RespFrame::error("ERR wrong number of arguments for 'eval' command"));
    }
    
    // Debug print parts for troubleshooting
    for (i, part) in parts.iter().enumerate() {
        println!("[LUA_CMD] Part {}: {:?}", i, part);
    }
    
    // Extract script
    let script = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8(bytes.as_ref().to_vec()) {
                Ok(s) => {
                    println!("[LUA_CMD] Script: {}", s);
                    s
                },
                Err(e) => {
                    println!("[LUA_CMD] Error parsing script: {}", e);
                    return Ok(RespFrame::error("ERR invalid script encoding"));
                }
            }
        }
        other => {
            println!("[LUA_CMD] Error: expected BulkString for script, got {:?}", other);
            return Ok(RespFrame::error("ERR invalid script format"));
        }
    };
    
    // Extract numkeys
    let numkeys = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<usize>() {
                Ok(n) => {
                    println!("[LUA_CMD] Numkeys: {}", n);
                    n
                },
                Err(e) => {
                    println!("[LUA_CMD] Error parsing numkeys: {}", e);
                    return Ok(RespFrame::error("ERR value is not an integer or out of range"));
                }
            }
        }
        other => {
            println!("[LUA_CMD] Error: expected BulkString for numkeys, got {:?}", other);
            return Ok(RespFrame::error("ERR invalid numkeys format"));
        }
    };
    
    // Check if numkeys is valid
    let max_keys = parts.len() - 3;
    if numkeys > max_keys {
        println!("[LUA_CMD] Error: numkeys {} > max available keys {}", numkeys, max_keys);
        return Ok(RespFrame::error(format!("ERR Number of keys can't be greater than number of args - 3")));
    }
    
    // Extract keys and args
    let mut keys = Vec::with_capacity(numkeys);
    let mut args = Vec::new();
    
    for i in 3..parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => {
                let bytes_vec = bytes.as_ref().to_vec();
                if i - 3 < numkeys {
                    println!("[LUA_CMD] Key {}: {:?}", i-3, String::from_utf8_lossy(&bytes_vec));
                    keys.push(bytes_vec);
                } else {
                    println!("[LUA_CMD] Arg {}: {:?}", i-3-numkeys, String::from_utf8_lossy(&bytes_vec));
                    args.push(bytes_vec);
                }
            }
            other => {
                println!("[LUA_CMD] Error: expected BulkString for key/arg, got {:?}", other);
                return Ok(RespFrame::error("ERR invalid key/arg format"));
            }
        }
    }
    
    // Execute script
    println!("[LUA_CMD] Calling script_executor.eval with script '{}'", script);
    match ctx.script_executor.eval(&script, keys, args, ctx.db) {
        Ok(resp) => {
            println!("[LUA_CMD] Script executed successfully, response: {:?}", resp);
            Ok(resp)
        },
        Err(e) => {
            println!("[LUA_CMD] Script execution error: {}", e);
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}

/// Handle EVAL command synchronously
pub fn handle_eval_sync(ctx: &CommandContext, parts: &[RespFrame]) -> Result<RespFrame> {
    handle_eval(ctx, parts)
}

/// Handle EVALSHA command - EVALSHA sha1 numkeys key [key ...] arg [arg ...]
pub fn handle_evalsha(ctx: &CommandContext, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'evalsha' command"));
    }
    
    // Extract SHA1
    let sha1 = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8(bytes.as_ref().to_vec()) {
                Ok(s) => s,
                Err(_) => return Ok(RespFrame::error("ERR invalid sha1 encoding")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid sha1 format")),
    };
    
    // Extract numkeys
    let numkeys = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<usize>() {
                Ok(n) => n,
                Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid numkeys format")),
    };
    
    // Check if numkeys is valid
    let max_keys = parts.len() - 3;
    if numkeys > max_keys {
        return Ok(RespFrame::error(format!("ERR Number of keys can't be greater than number of args - 3")));
    }
    
    // Extract keys and args
    let mut keys = Vec::with_capacity(numkeys);
    let mut args = Vec::new();
    
    for i in 3..parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => {
                let bytes_vec = bytes.as_ref().to_vec();
                if i - 3 < numkeys {
                    keys.push(bytes_vec);
                } else {
                    args.push(bytes_vec);
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid key/arg format")),
        }
    }
    
    // Execute script
    match ctx.script_executor.evalsha(&sha1, keys, args, ctx.db) {
        Ok(resp) => Ok(resp),
        Err(e) => Ok(RespFrame::error(format!("ERR {}", e))),
    }
}

/// Handle EVALSHA command synchronously
pub fn handle_evalsha_sync(ctx: &CommandContext, parts: &[RespFrame]) -> Result<RespFrame> {
    handle_evalsha(ctx, parts)
}

/// Handle SCRIPT command - Multiple operations for script management
pub fn handle_script(ctx: &CommandContext, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'script' command"));
    }
    
    // Extract subcommand
    let subcommand = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8(bytes.as_ref().to_vec()) {
                Ok(s) => s.to_uppercase(),
                Err(_) => return Ok(RespFrame::error("ERR invalid subcommand encoding")),
            }
        }
        _ => return Ok(RespFrame::error("ERR invalid subcommand format")),
    };
    
    match subcommand.as_str() {
        "LOAD" => {
            if parts.len() != 3 {
                return Ok(RespFrame::error("ERR wrong number of arguments for 'script load' command"));
            }
            
            // Extract script
            let script = match &parts[2] {
                RespFrame::BulkString(Some(bytes)) => {
                    match String::from_utf8(bytes.as_ref().to_vec()) {
                        Ok(s) => s,
                        Err(_) => return Ok(RespFrame::error("ERR invalid script encoding")),
                    }
                }
                _ => return Ok(RespFrame::error("ERR invalid script format")),
            };
            
            // Load script
            match ctx.script_executor.load(&script) {
                Ok(sha1) => Ok(RespFrame::from_string(sha1)),
                Err(e) => Ok(RespFrame::error(format!("ERR {}", e))),
            }
        }
        "EXISTS" => {
            // Check if scripts exist
            let mut sha1s = Vec::new();
            
            for i in 2..parts.len() {
                match &parts[i] {
                    RespFrame::BulkString(Some(bytes)) => {
                        match String::from_utf8(bytes.as_ref().to_vec()) {
                            Ok(s) => sha1s.push(s),
                            Err(_) => return Ok(RespFrame::error("ERR invalid sha1 encoding")),
                        }
                    }
                    _ => return Ok(RespFrame::error("ERR invalid sha1 format")),
                }
            }
            
            // Check existence
            let exists = ctx.script_executor.exists(&sha1s);
            
            // Return array of 0/1 integers
            let mut results = Vec::with_capacity(exists.len());
            for e in exists {
                results.push(RespFrame::Integer(if e { 1 } else { 0 }));
            }
            
            Ok(RespFrame::Array(Some(results)))
        }
        "FLUSH" => {
            // Clear the script cache
            ctx.script_executor.flush();
            Ok(RespFrame::ok())
        }
        "KILL" => {
            // Kill the currently running script
            if ctx.script_executor.kill() {
                Ok(RespFrame::ok())
            } else {
                Ok(RespFrame::error("NOTBUSY No scripts in execution right now."))
            }
        }
        _ => {
            Ok(RespFrame::error("ERR Unknown SCRIPT subcommand"))
        }
    }
}

/// Handle SCRIPT command synchronously
pub fn handle_script_sync(ctx: &CommandContext, parts: &[RespFrame]) -> Result<RespFrame> {
    handle_script(ctx, parts)
}