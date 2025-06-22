//! Lua script command handlers for Redis compatibility
//!
//! This module implements the EVAL and EVALSHA commands.

use std::sync::Arc;

use super::executor::ScriptExecutor;
use crate::error::{FerrousError, CommandError, ScriptError};
use crate::protocol::{RespFrame, extract_bytes, extract_integer, extract_string};
use crate::storage::{StorageEngine, DatabaseIndex};

/// Context for executing commands
pub struct CommandContext {
    /// Current database
    pub db: DatabaseIndex,
    /// Storage engine
    pub storage: Arc<StorageEngine>,
    /// Script executor
    pub script_executor: Arc<ScriptExecutor>,
}

/// Handle EVAL command
/// 
/// EVAL script numkeys [key ...] [arg ...]
pub async fn handle_eval(ctx: &CommandContext, args: &[RespFrame]) -> Result<RespFrame, FerrousError> {
    // Check argument count
    if args.len() < 3 {
        return Err(CommandError::WrongNumberOfArgs("eval".to_string()).into());
    }
    
    // Extract script
    let script = extract_string(&args[1])?;
    
    // Extract number of keys
    let num_keys = extract_integer(&args[2])? as usize;
    
    // Validate number of keys
    if args.len() < num_keys + 3 {
        return Err(CommandError::InvalidArgument.into());
    }
    
    // Extract keys
    let mut keys = Vec::with_capacity(num_keys);
    for i in 0..num_keys {
        keys.push(extract_bytes(&args[3 + i])?);
    }
    
    // Extract arguments
    let mut argv = Vec::with_capacity(args.len() - num_keys - 3);
    for i in num_keys + 3..args.len() {
        argv.push(extract_bytes(&args[i])?);
    }
    
    println!("[SERVER DEBUG] EVAL: Running script with {} keys, {} args", keys.len(), argv.len());
    
    // Execute script
    match ctx.script_executor.eval(&script, keys, argv, ctx.db) {
        Ok(resp) => {
            println!("[SERVER DEBUG] EVAL executed successfully");
            Ok(resp)
        },
        Err(e) => {
            // Convert Lua errors to proper Redis errors with appropriate format
            match e {
                FerrousError::Script(ScriptError::CompilationError(msg)) => {
                    Ok(RespFrame::error(format!("ERR Error compiling script (new function): {}", msg)))
                },
                FerrousError::Script(ScriptError::ExecutionError(msg)) => {
                    Ok(RespFrame::error(format!("ERR Error running script (call to f_...): {}", msg)))
                },
                _ => {
                    let error_msg = format!("ERR {}", e);
                    println!("[SERVER ERROR] EVAL error: {}", error_msg);
                    Ok(RespFrame::error(error_msg))
                }
            }
        }
    }
}

/// Handle EVALSHA command
/// 
/// EVALSHA sha1 numkeys [key ...] [arg ...]
pub async fn handle_evalsha(ctx: &CommandContext, args: &[RespFrame]) -> Result<RespFrame, FerrousError> {
    // Check argument count
    if args.len() < 3 {
        return Err(CommandError::WrongNumberOfArgs("evalsha".to_string()).into());
    }
    
    // Extract SHA1
    let sha1 = extract_string(&args[1])?;
    
    // Extract number of keys
    let num_keys = extract_integer(&args[2])? as usize;
    
    // Validate number of keys
    if args.len() < num_keys + 3 {
        return Err(CommandError::InvalidArgument.into());
    }
    
    // Extract keys
    let mut keys = Vec::with_capacity(num_keys);
    for i in 0..num_keys {
        keys.push(extract_bytes(&args[3 + i])?);
    }
    
    // Extract arguments
    let mut argv = Vec::with_capacity(args.len() - num_keys - 3);
    for i in num_keys + 3..args.len() {
        argv.push(extract_bytes(&args[i])?);
    }
    
    // Execute script from cache
    match ctx.script_executor.evalsha(&sha1, keys, argv, ctx.db) {
        Ok(resp) => Ok(resp),
        Err(e) => {
            if let FerrousError::Script(ScriptError::NotFound) = &e {
                Ok(RespFrame::error("NOSCRIPT No matching script. Please use EVAL."))
            } else {
                Ok(RespFrame::error(format!("ERR Error running script: {}", e)))
            }
        }
    }
}

/// Handle SCRIPT command
/// 
/// SCRIPT LOAD script
/// SCRIPT EXISTS sha1 [sha1 ...]
/// SCRIPT FLUSH
/// SCRIPT KILL
pub async fn handle_script(ctx: &CommandContext, args: &[RespFrame]) -> Result<RespFrame, FerrousError> {
    // Check argument count
    if args.len() < 2 {
        return Err(CommandError::WrongNumberOfArgs("script".to_string()).into());
    }
    
    // Extract subcommand
    let subcommand = extract_string(&args[1])?.to_uppercase();
    
    match subcommand.as_str() {
        "LOAD" => {
            // Check argument count
            if args.len() != 3 {
                return Err(CommandError::WrongNumberOfArgs("script load".to_string()).into());
            }
            
            // Extract script
            let script = extract_string(&args[2])?;
            
            // Load script
            match ctx.script_executor.load_script(&script) {
                Ok(sha) => Ok(RespFrame::BulkString(Some(Arc::new(sha.into_bytes())))),
                Err(e) => Err(e),
            }
        },
        
        "EXISTS" => {
            // Check if we have at least one SHA1
            if args.len() < 3 {
                return Err(CommandError::WrongNumberOfArgs("script exists".to_string()).into());
            }
            
            // Check if scripts exist
            let mut response = Vec::with_capacity(args.len() - 2);
            for i in 2..args.len() {
                let sha = extract_string(&args[i])?;
                let exists = ctx.script_executor.script_exists(&sha);
                response.push(RespFrame::Integer(if exists { 1 } else { 0 }));
            }
            
            Ok(RespFrame::Array(Some(response)))
        },
        
        "FLUSH" => {
            // Flush script cache
            ctx.script_executor.flush_scripts();
            
            Ok(RespFrame::SimpleString(Arc::new(b"OK".to_vec())))
        },
        
        "KILL" => {
            // Kill running script
            if ctx.script_executor.kill_running_script() {
                Ok(RespFrame::SimpleString(Arc::new(b"OK".to_vec())))
            } else {
                Err(ScriptError::ExecutionError("No scripts in execution".to_string()).into())
            }
        },
        
        _ => Err(CommandError::InvalidArgument.into()),
    }
}

/// Synchronous version of handle_eval
pub fn handle_eval_sync(ctx: &CommandContext, args: &[RespFrame]) -> Result<RespFrame, FerrousError> {
    // Check argument count
    if args.len() < 3 {
        return Err(CommandError::WrongNumberOfArgs("eval".to_string()).into());
    }
    
    // Extract script
    let script = extract_string(&args[1])?;
    
    // Extract number of keys
    let num_keys = extract_integer(&args[2])? as usize;
    
    // Validate number of keys
    if args.len() < num_keys + 3 {
        return Err(CommandError::InvalidArgument.into());
    }
    
    // Extract keys
    let mut keys = Vec::with_capacity(num_keys);
    for i in 0..num_keys {
        keys.push(extract_bytes(&args[3 + i])?);
    }
    
    // Extract arguments
    let mut argv = Vec::with_capacity(args.len() - num_keys - 3);
    for i in num_keys + 3..args.len() {
        argv.push(extract_bytes(&args[i])?);
    }
    
    println!("[SERVER DEBUG] EVAL: Running script with {} keys, {} args", keys.len(), argv.len());
    
    // Execute script
    match ctx.script_executor.eval(&script, keys, argv, ctx.db) {
        Ok(resp) => {
            println!("[SERVER DEBUG] EVAL executed successfully");
            Ok(resp)
        },
        Err(e) => {
            // Convert Lua errors to proper Redis errors with appropriate format
            match e {
                FerrousError::Script(ScriptError::CompilationError(msg)) => {
                    Ok(RespFrame::error(format!("ERR Error compiling script (new function): {}", msg)))
                },
                FerrousError::Script(ScriptError::ExecutionError(msg)) => {
                    Ok(RespFrame::error(format!("ERR Error running script (call to f_...): {}", msg)))
                },
                _ => {
                    let error_msg = format!("ERR {}", e);
                    println!("[SERVER ERROR] EVAL error: {}", error_msg);
                    Ok(RespFrame::error(error_msg))
                }
            }
        }
    }
}

/// Synchronous version of handle_evalsha
pub fn handle_evalsha_sync(ctx: &CommandContext, args: &[RespFrame]) -> Result<RespFrame, FerrousError> {
    // Check argument count
    if args.len() < 3 {
        return Err(CommandError::WrongNumberOfArgs("evalsha".to_string()).into());
    }
    
    // Extract SHA1
    let sha1 = extract_string(&args[1])?;
    
    // Extract number of keys
    let num_keys = extract_integer(&args[2])? as usize;
    
    // Validate number of keys
    if args.len() < num_keys + 3 {
        return Err(CommandError::InvalidArgument.into());
    }
    
    // Extract keys
    let mut keys = Vec::with_capacity(num_keys);
    for i in 0..num_keys {
        keys.push(extract_bytes(&args[3 + i])?);
    }
    
    // Extract arguments
    let mut argv = Vec::with_capacity(args.len() - num_keys - 3);
    for i in num_keys + 3..args.len() {
        argv.push(extract_bytes(&args[i])?);
    }
    
    // Execute script from cache
    match ctx.script_executor.evalsha(&sha1, keys, argv, ctx.db) {
        Ok(resp) => Ok(resp),
        Err(e) => {
            if let FerrousError::Script(ScriptError::NotFound) = &e {
                Ok(RespFrame::error("NOSCRIPT No matching script. Please use EVAL."))
            } else {
                Ok(RespFrame::error(format!("ERR Error running script: {}", e)))
            }
        }
    }
}

/// Synchronous version of handle_script
pub fn handle_script_sync(ctx: &CommandContext, args: &[RespFrame]) -> Result<RespFrame, FerrousError> {
    // Check argument count
    if args.len() < 2 {
        return Err(CommandError::WrongNumberOfArgs("script".to_string()).into());
    }
    
    // Extract subcommand
    let subcommand = extract_string(&args[1])?.to_uppercase();
    
    match subcommand.as_str() {
        "LOAD" => {
            // Check argument count
            if args.len() != 3 {
                return Err(CommandError::WrongNumberOfArgs("script load".to_string()).into());
            }
            
            // Extract script
            let script = extract_string(&args[2])?;
            
            // Load script
            match ctx.script_executor.load_script(&script) {
                Ok(sha) => Ok(RespFrame::BulkString(Some(Arc::new(sha.into_bytes())))),
                Err(e) => Err(e),
            }
        },
        
        "EXISTS" => {
            // Check if we have at least one SHA1
            if args.len() < 3 {
                return Err(CommandError::WrongNumberOfArgs("script exists".to_string()).into());
            }
            
            // Check if scripts exist
            let mut response = Vec::with_capacity(args.len() - 2);
            for i in 2..args.len() {
                let sha = extract_string(&args[i])?;
                let exists = ctx.script_executor.script_exists(&sha);
                response.push(RespFrame::Integer(if exists { 1 } else { 0 }));
            }
            
            Ok(RespFrame::Array(Some(response)))
        },
        
        "FLUSH" => {
            // Flush script cache
            ctx.script_executor.flush_scripts();
            
            Ok(RespFrame::SimpleString(Arc::new(b"OK".to_vec())))
        },
        
        "KILL" => {
            // Kill running script
            if ctx.script_executor.kill_running_script() {
                Ok(RespFrame::SimpleString(Arc::new(b"OK".to_vec())))
            } else {
                Err(ScriptError::ExecutionError("No scripts in execution".to_string()).into())
            }
        },
        
        _ => Err(CommandError::InvalidArgument.into()),
    }
}