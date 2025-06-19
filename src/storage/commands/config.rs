//! CONFIG command implementation
//! 
//! Provides Redis-compatible CONFIG command implementation for better compatibility
//! with Redis benchmarking tools.

use crate::error::Result;
use crate::protocol::RespFrame;

/// Handle CONFIG command
/// 
/// redis-benchmark uses this command to fetch server configuration.
/// If we don't handle it properly, benchmarks will error out with "Could not fetch server CONFIG".
pub fn handle_config(parts: &[RespFrame]) -> Result<RespFrame> {
    if parts.len() < 2 {
        return Ok(RespFrame::error("ERR wrong number of arguments for 'config' command"));
    }
    
    // Extract subcommand
    let subcommand = match &parts[1] {
        RespFrame::BulkString(Some(bytes)) => {
            String::from_utf8_lossy(bytes).to_uppercase()
        },
        _ => return Ok(RespFrame::error("ERR invalid subcommand format")),
    };
    
    match subcommand.as_str() {
        "GET" => {
            if parts.len() < 3 {
                return Ok(RespFrame::error("ERR wrong number of arguments for 'config get' command"));
            }
            
            // Extract parameter
            let param = match &parts[2] {
                RespFrame::BulkString(Some(bytes)) => {
                    String::from_utf8_lossy(bytes).to_string()
                },
                _ => return Ok(RespFrame::error("ERR invalid parameter format")),
            };
            
            handle_config_get(&param)
        },
        "SET" => Ok(RespFrame::error("ERR CONFIG SET not supported")),
        "RESETSTAT" => Ok(RespFrame::error("ERR CONFIG RESETSTAT not supported")),
        "REWRITE" => Ok(RespFrame::error("ERR CONFIG REWRITE not supported")),
        _ => Ok(RespFrame::error("ERR CONFIG command not supported")),
    }
}

/// Handle CONFIG GET parameter
fn handle_config_get(param: &str) -> Result<RespFrame> {
    // If parameter is "*" or "save", return all configs
    if param == "*" {
        return generate_all_configs();
    }
    
    // Handle common parameters that redis-benchmark needs
    match param.to_lowercase().as_str() {
        "save" => {
            let values = vec![
                RespFrame::from_string("save"),
                RespFrame::from_string("900 1 300 10 60 10000"),
            ];
            Ok(RespFrame::Array(Some(values)))
        },
        "appendonly" => {
            let values = vec![
                RespFrame::from_string("appendonly"),
                RespFrame::from_string("no"),
            ];
            Ok(RespFrame::Array(Some(values)))
        },
        "maxmemory" => {
            let values = vec![
                RespFrame::from_string("maxmemory"),
                RespFrame::from_string("0"),
            ];
            Ok(RespFrame::Array(Some(values)))
        },
        "maxmemory-policy" => {
            let values = vec![
                RespFrame::from_string("maxmemory-policy"),
                RespFrame::from_string("noeviction"),
            ];
            Ok(RespFrame::Array(Some(values)))
        },
        "timeout" => {
            let values = vec![
                RespFrame::from_string("timeout"),
                RespFrame::from_string("0"),
            ];
            Ok(RespFrame::Array(Some(values)))
        },
        "databases" => {
            let values = vec![
                RespFrame::from_string("databases"),
                RespFrame::from_string("16"),
            ];
            Ok(RespFrame::Array(Some(values)))
        },
        _ => {
            // For unknown parameters, return empty array
            Ok(RespFrame::Array(Some(vec![])))
        },
    }
}

/// Generate a response with all configs
fn generate_all_configs() -> Result<RespFrame> {
    let mut configs = Vec::new();
    
    // Standard Redis configuration parameters
    configs.push(RespFrame::from_string("save"));
    configs.push(RespFrame::from_string("900 1 300 10 60 10000"));
    
    configs.push(RespFrame::from_string("appendonly"));
    configs.push(RespFrame::from_string("no"));
    
    configs.push(RespFrame::from_string("maxmemory"));
    configs.push(RespFrame::from_string("0"));
    
    configs.push(RespFrame::from_string("maxmemory-policy"));
    configs.push(RespFrame::from_string("noeviction"));
    
    configs.push(RespFrame::from_string("timeout"));
    configs.push(RespFrame::from_string("0"));
    
    configs.push(RespFrame::from_string("tcp-keepalive"));
    configs.push(RespFrame::from_string("300"));
    
    configs.push(RespFrame::from_string("databases"));
    configs.push(RespFrame::from_string("16"));
    
    Ok(RespFrame::Array(Some(configs)))
}