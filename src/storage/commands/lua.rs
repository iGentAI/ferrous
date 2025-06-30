//! Lua script command handlers

use std::sync::Arc;
use std::time::Duration;

use crate::error::Result;
use crate::protocol::resp::RespFrame;
use crate::storage::StorageEngine;
use crate::lua;

/// Handle EVAL command
pub fn handle_eval(storage: &Arc<StorageEngine>, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'eval' command"));
    }
    
    // Get script string
    let script = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            String::from_utf8_lossy(bytes).to_string()
        }
        _ => {
            return Ok(RespFrame::error("ERR invalid script"));
        }
    };
    
    // Get number of keys
    let num_keys = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            let num_str = String::from_utf8_lossy(bytes).to_string();
            match num_str.parse::<usize>() {
                Ok(n) => n,
                Err(_) => return Ok(RespFrame::error("ERR invalid number of keys"))
            }
        }
        RespFrame::Integer(n) => {
            *n as usize
        }
        _ => {
            return Ok(RespFrame::error("ERR invalid number of keys"));
        }
    };
    
    // Check if we have enough arguments
    if parts.len() < 3 + num_keys {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'eval' command"));
    }
    
    // Extract keys
    let mut keys = Vec::with_capacity(num_keys);
    for i in 0..num_keys {
        match &parts[3 + i] {
            RespFrame::BulkString(Some(bytes)) => {
                keys.push(bytes.to_vec());
            }
            _ => {
                return Ok(RespFrame::error("ERR invalid key"));
            }
        }
    }
    
    // Extract args
    let mut args = Vec::new();
    for i in 3 + num_keys..parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => {
                args.push(bytes.to_vec());
            }
            _ => {
                return Ok(RespFrame::error("ERR invalid argument"));
            }
        }
    }
    
    // Get the current DB
    let db = 0;  // For now, just use DB 0
    
    // Create execution context
    let context = lua::ScriptContext {
        storage: storage.clone(),
        db,
        keys,
        args,
        timeout: Duration::from_secs(5), // Default timeout from config
    };
    
    // Get Lua GIL
    let gil = match lua::LuaGIL::new() {
        Ok(gil) => gil,
        Err(e) => return Ok(RespFrame::error(format!("ERR {}", e))),
    };
    
    // Execute script
    match gil.eval(&script, context) {
        Ok(resp) => Ok(resp),
        Err(e) => Ok(RespFrame::error(format!("ERR {}", e))),
    }
}

/// Handle EVALSHA command
pub fn handle_evalsha(storage: &Arc<StorageEngine>, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 3 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'evalsha' command"));
    }
    
    // Get sha1
    let sha1 = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            String::from_utf8_lossy(bytes).to_string()
        }
        _ => {
            return Ok(RespFrame::error("ERR invalid sha1"));
        }
    };
    
    // Get number of keys
    let num_keys = match &parts[2] {
        RespFrame::BulkString(Some(bytes)) => {
            let num_str = String::from_utf8_lossy(bytes).to_string();
            match num_str.parse::<usize>() {
                Ok(n) => n,
                Err(_) => return Ok(RespFrame::error("ERR invalid number of keys"))
            }
        }
        RespFrame::Integer(n) => {
            *n as usize
        }
        _ => {
            return Ok(RespFrame::error("ERR invalid number of keys"));
        }
    };
    
    // Check if we have enough arguments
    if parts.len() < 3 + num_keys {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'evalsha' command"));
    }
    
    // Extract keys
    let mut keys = Vec::with_capacity(num_keys);
    for i in 0..num_keys {
        match &parts[3 + i] {
            RespFrame::BulkString(Some(bytes)) => {
                keys.push(bytes.to_vec());
            }
            _ => {
                return Ok(RespFrame::error("ERR invalid key"));
            }
        }
    }
    
    // Extract args
    let mut args = Vec::new();
    for i in 3 + num_keys..parts.len() {
        match &parts[i] {
            RespFrame::BulkString(Some(bytes)) => {
                args.push(bytes.to_vec());
            }
            _ => {
                return Ok(RespFrame::error("ERR invalid argument"));
            }
        }
    }
    
    // Get the current DB
    let db = 0;  // For now, just use DB 0
    
    // Create execution context
    let context = lua::ScriptContext {
        storage: storage.clone(),
        db,
        keys,
        args,
        timeout: Duration::from_secs(5), // Default timeout from config
    };
    
    // Get Lua GIL
    let gil = match lua::LuaGIL::new() {
        Ok(gil) => gil,
        Err(e) => return Ok(RespFrame::error(format!("ERR {}", e))),
    };
    
    // Execute script
    match gil.evalsha(&sha1, context) {
        Ok(resp) => Ok(resp),
        Err(e) => {
            match e {
                crate::error::FerrousError::ScriptNotFound(_) => {
                    Ok(RespFrame::error("NOSCRIPT No matching script. Please use EVAL."))
                }
                _ => Ok(RespFrame::error(format!("ERR {}", e))),
            }
        }
    }
}

/// Handle SCRIPT command
pub fn handle_script(storage: &Arc<StorageEngine>, parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'script' command"));
    }
    
    // Get subcommand
    let subcommand = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            String::from_utf8_lossy(bytes).to_uppercase()
        },
        _ => {
            return Ok(RespFrame::error("ERR invalid subcommand format"));
        }
    };
    
    // Get Lua GIL
    let gil = match lua::LuaGIL::new() {
        Ok(gil) => gil,
        Err(e) => return Ok(RespFrame::error(format!("ERR {}", e))),
    };
    
    match subcommand.as_str() {
        "LOAD" => {
            if parts.len() != 3 {
                return Ok(RespFrame::error("ERR wrong number of arguments for 'script load' command"));
            }
            
            // Get script string
            let script = match &parts[2] {
                RespFrame::BulkString(Some(bytes)) => {
                    String::from_utf8_lossy(bytes).to_string()
                },
                _ => {
                    return Ok(RespFrame::error("ERR invalid script"));
                }
            };
            
            // Load script
            match gil.script_load(&script) {
                Ok(sha1) => Ok(RespFrame::from_string(sha1)),
                Err(e) => Ok(RespFrame::error(format!("ERR {}", e))),
            }
        },
        "EXISTS" => {
            if parts.len() < 3 {
                return Ok(RespFrame::error("ERR wrong number of arguments for 'script exists' command"));
            }
            
            // Check each SHA1
            let mut results = Vec::new();
            for i in 2..parts.len() {
                match &parts[i] {
                    RespFrame::BulkString(Some(bytes)) => {
                        let sha1 = String::from_utf8_lossy(bytes).to_string();
                        if gil.script_exists(&sha1) {
                            results.push(RespFrame::Integer(1));
                        } else {
                            results.push(RespFrame::Integer(0));
                        }
                    },
                    _ => {
                        results.push(RespFrame::Integer(0));
                    }
                }
            }
            
            Ok(RespFrame::Array(Some(results)))
        },
        "FLUSH" => {
            gil.script_flush();
            Ok(RespFrame::ok())
        },
        "KILL" => {
            match gil.kill_script() {
                Ok(_) => Ok(RespFrame::ok()),
                Err(e) => Ok(RespFrame::error(format!("ERR {}", e))),
            }
        },
        _ => {
            Ok(RespFrame::error("ERR Unknown SCRIPT subcommand"))
        }
    }
}