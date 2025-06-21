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
    
    // Execute script
    ctx.script_executor.eval(&script, keys, argv, ctx.db)
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
    
    // Execute script
    ctx.script_executor.evalsha(&sha1, keys, argv, ctx.db)
}

/// Handle SCRIPT commands
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
            
            // Load script (using empty keys and argv to just compile and cache)
            match ctx.script_executor.eval(&script, vec![], vec![], ctx.db) {
                Ok(_) => {
                    // Get SHA1 of script
                    let sha1 = compute_sha1(&script);
                    
                    // Return SHA1
                    Ok(RespFrame::BulkString(Some(Arc::new(sha1.into_bytes()))))
                },
                Err(e) => Err(e),
            }
        },
        
        "EXISTS" => {
            // Check if we have at least one SHA1
            if args.len() < 3 {
                return Err(CommandError::WrongNumberOfArgs("script exists".to_string()).into());
            }
            
            // Not implemented in this simplified version
            // In a real implementation, we would check the script cache
            
            // Return 0 for simplicity
            let mut responses = Vec::new();
            for _ in 2..args.len() {
                responses.push(RespFrame::Integer(0));
            }
            
            Ok(RespFrame::Array(Some(responses)))
        },
        
        "FLUSH" => {
            // Not implemented in this simplified version
            // In a real implementation, we would clear the script cache
            
            // Return OK
            Ok(RespFrame::SimpleString(Arc::new(b"OK".to_vec())))
        },
        
        "KILL" => {
            // Not implemented in this simplified version
            // In a real implementation, we would kill the currently running script
            
            // Return error (no running script)
            Err(ScriptError::ExecutionError("No scripts in execution".to_string()).into())
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
    
    // Execute script
    ctx.script_executor.eval(&script, keys, argv, ctx.db)
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
    
    // Execute script
    ctx.script_executor.evalsha(&sha1, keys, argv, ctx.db)
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
            
            // Load script (using empty keys and argv to just compile and cache)
            match ctx.script_executor.eval(&script, vec![], vec![], ctx.db) {
                Ok(_) => {
                    // Get SHA1 of script
                    let sha1 = compute_sha1(&script);
                    
                    // Return SHA1
                    Ok(RespFrame::BulkString(Some(Arc::new(sha1.into_bytes()))))
                },
                Err(e) => Err(e),
            }
        },
        
        "EXISTS" => {
            // Check if we have at least one SHA1
            if args.len() < 3 {
                return Err(CommandError::WrongNumberOfArgs("script exists".to_string()).into());
            }
            
            // Not implemented in this simplified version
            // In a real implementation, we would check the script cache
            
            // Return 0 for simplicity
            let mut responses = Vec::new();
            for _ in 2..args.len() {
                responses.push(RespFrame::Integer(0));
            }
            
            Ok(RespFrame::Array(Some(responses)))
        },
        
        "FLUSH" => {
            // Not implemented in this simplified version
            // In a real implementation, we would clear the script cache
            
            // Return OK
            Ok(RespFrame::SimpleString(Arc::new(b"OK".to_vec())))
        },
        
        "KILL" => {
            // Not implemented in this simplified version
            // In a real implementation, we would kill the currently running script
            
            // Return error (no running script)
            Err(ScriptError::ExecutionError("No scripts in execution".to_string()).into())
        },
        
        _ => Err(CommandError::InvalidArgument.into()),
    }
}

/// Compute SHA1 hash (simplified implementation)
fn compute_sha1(script: &str) -> String {
    // In a real implementation, this would compute an actual SHA1 hash
    // For simplicity, we return a placeholder
    "da39a3ee5e6b4b0d3255bfef95601890afd80709".to_string()
}