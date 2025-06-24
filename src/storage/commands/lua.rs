//! Lua script command handlers

use crate::error::Result;
use crate::protocol::resp::RespFrame;
use crate::storage::engine::StorageEngine;
use crate::lua_new::executor::ScriptExecutor;
use crate::lua_new::command::{self, CommandContext};
use std::sync::Arc;

/// Handle EVAL command
pub fn handle_eval(
    storage: &Arc<StorageEngine>,
    script_executor: &Arc<ScriptExecutor>,
    db: usize, 
    parts: &[RespFrame]
) -> Result<RespFrame> {
    let ctx = CommandContext {
        db,
        storage: Arc::clone(storage),
        script_executor: Arc::clone(script_executor),
    };
    
    command::handle_eval_sync(&ctx, parts)
}

/// Handle EVALSHA command
pub fn handle_evalsha(
    storage: &Arc<StorageEngine>,
    script_executor: &Arc<ScriptExecutor>,
    db: usize, 
    parts: &[RespFrame]
) -> Result<RespFrame> {
    let ctx = CommandContext {
        db,
        storage: Arc::clone(storage),
        script_executor: Arc::clone(script_executor),
    };
    
    command::handle_evalsha_sync(&ctx, parts)
}

/// Handle SCRIPT command
pub fn handle_script(
    storage: &Arc<StorageEngine>,
    script_executor: &Arc<ScriptExecutor>,
    db: usize, 
    parts: &[RespFrame]
) -> Result<RespFrame> {
    let ctx = CommandContext {
        db,
        storage: Arc::clone(storage),
        script_executor: Arc::clone(script_executor),
    };
    
    command::handle_script_sync(&ctx, parts)
}