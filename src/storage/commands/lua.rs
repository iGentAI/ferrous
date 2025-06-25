//! Lua script command handlers

use crate::error::Result;
use crate::protocol::resp::RespFrame;
use crate::storage::engine::StorageEngine;
use crate::lua_new::executor::ScriptExecutor;
use crate::lua_new::command::{self, CommandContext};
use std::sync::Arc;

/// Handle EVAL command
pub fn handle_eval(
    _storage: &Arc<StorageEngine>,
    script_executor: &Arc<ScriptExecutor>,
    db: usize, 
    parts: &[RespFrame]
) -> Result<RespFrame> {
    println!("[LUA_GIL_CMD] Processing EVAL command with {} parts", parts.len());
    
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'eval' command"));
    }
    
    // Extract script
    let script = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8(bytes.as_ref().to_vec()) {
                Ok(s) => {
                    println!("[LUA_GIL_CMD] Script: {}", s);
                    s
                },
                Err(e) => {
                    println!("[LUA_GIL_CMD] Error parsing script: {}", e);
                    return Ok(RespFrame::error("ERR invalid script encoding"));
                }
            }
        }
        other => {
            println!("[LUA_GIL_CMD] Error: expected BulkString for script, got {:?}", other);
            return Ok(RespFrame::error("ERR invalid script format"));
        }
    };
    
    // Extract numkeys
    let numkeys = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            match String::from_utf8_lossy(bytes).parse::<usize>() {
                Ok(n) => {
                    println!("[LUA_GIL_CMD] Numkeys: {}", n);
                    n
                },
                Err(e) => {
                    println!("[LUA_GIL_CMD] Error parsing numkeys: {}", e);
                    return Ok(RespFrame::error("ERR value is not an integer or out of range"));
                }
            }
        }
        other => {
            println!("[LUA_GIL_CMD] Error: expected BulkString for numkeys, got {:?}", other);
            return Ok(RespFrame::error("ERR invalid numkeys format"));
        }
    };
    
    // Check if numkeys is valid
    let max_keys = parts.len() - 3;
    if numkeys > max_keys {
        println!("[LUA_GIL_CMD] Error: numkeys {} > max available keys {}", numkeys, max_keys);
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
                    println!("[LUA_GIL_CMD] Key {}: {:?}", i-3, String::from_utf8_lossy(&bytes_vec));
                    keys.push(bytes_vec);
                } else {
                    println!("[LUA_GIL_CMD] Arg {}: {:?}", i-3-numkeys, String::from_utf8_lossy(&bytes_vec));
                    args.push(bytes_vec);
                }
            }
            other => {
                println!("[LUA_GIL_CMD] Error: expected BulkString for key/arg, got {:?}", other);
                return Ok(RespFrame::error("ERR invalid key/arg format"));
            }
        }
    }
    
    // Execute script with GIL
    println!("[LUA_GIL_CMD] Calling script_executor.eval with script '{}'", script);
    match script_executor.eval(&script, keys, args, db) {
        Ok(resp) => {
            println!("[LUA_GIL_CMD] Script executed successfully, response: {:?}", resp);
            Ok(resp)
        },
        Err(e) => {
            println!("[LUA_GIL_CMD] Script execution error: {}", e);
            Ok(RespFrame::error(format!("ERR {}", e)))
        }
    }
}

/// Handle EVALSHA command 
pub fn handle_evalsha(
    _storage: &Arc<StorageEngine>,
    script_executor: &Arc<ScriptExecutor>,
    db: usize, 
    parts: &[RespFrame]
) -> Result<RespFrame> {
    println!("[LUA_GIL_CMD] Processing EVALSHA command");
    
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
    
    // Execute script with GIL
    println!("[LUA_GIL_CMD] Executing script with SHA1 {}", sha1);
    script_executor.evalsha(&sha1, keys, args, db)
}

/// Handle SCRIPT command
pub fn handle_script(
    _storage: &Arc<StorageEngine>,
    script_executor: &Arc<ScriptExecutor>,
    _db: usize, 
    parts: &[RespFrame]
) -> Result<RespFrame> {
    println!("[LUA_GIL_CMD] Processing SCRIPT command");
    
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
            println!("[LUA_GIL_CMD] Loading script");
            match script_executor.load(&script) {
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
            println!("[LUA_GIL_CMD] Checking existence of {} scripts", sha1s.len());
            let exists = script_executor.exists(&sha1s);
            
            // Return array of 0/1 integers
            let mut results = Vec::with_capacity(exists.len());
            for e in exists {
                results.push(RespFrame::Integer(if e { 1 } else { 0 }));
            }
            
            Ok(RespFrame::Array(Some(results)))
        }
        "FLUSH" => {
            // Clear the script cache
            println!("[LUA_GIL_CMD] Flushing script cache");
            script_executor.flush();
            Ok(RespFrame::ok())
        }
        "KILL" => {
            // Kill the currently running script
            println!("[LUA_GIL_CMD] Killing running script");
            if script_executor.kill() {
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