//! Configuration file parser
//!
//! Parses Redis-compatible configuration files for Ferrous.

use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::network::NetworkConfig;
use crate::storage::{RdbConfig, AofConfig};
use crate::storage::memory::EvictionPolicy;
use crate::storage::aof::FsyncPolicy;
use crate::replication::ReplicationConfig;

use super::{Config, ServerConfig, MemoryConfig, LogLevel};

/// Error type for configuration parsing
#[derive(Debug, thiserror::Error)]
pub enum ConfigParseError {
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    
    /// Invalid line format
    #[error("Invalid line format at line {0}: {1}")]
    Format(usize, String),
    
    /// Invalid parameter value
    #[error("Invalid value for parameter '{0}' at line {1}: {2}")]
    Value(String, usize, String),
    
    /// Unknown parameter
    #[error("Unknown parameter '{0}' at line {1}")]
    UnknownParam(String, usize),
}

/// Parse a Redis-compatible configuration file
pub fn parse_config_file(path: &Path) -> Result<Config, ConfigParseError> {
    let file = File::open(path)
        .map_err(ConfigParseError::Io)?;
    
    let reader = BufReader::new(file);
    let mut config = Config::default();
    
    // Update dir paths based on config file location
    if let Some(parent) = path.parent() {
        config.rdb.dir = parent.to_string_lossy().to_string();
        // Also update any other path-relative configs
    }
    
    for (line_num, line_result) in reader.lines().enumerate() {
        let line = line_result?;
        let line = line.trim();
        
        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        
        // Split the line into parameter and value
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() != 2 {
            return Err(ConfigParseError::Format(line_num + 1, line.to_string()));
        }
        
        let param = parts[0].trim().to_lowercase();
        let value = parts[1].trim();
        
        // Apply configuration parameter
        apply_config_param(&mut config, &param, value, line_num + 1)?;
    }
    
    Ok(config)
}

/// Apply a configuration parameter to the config
fn apply_config_param(config: &mut Config, param: &str, value: &str, line_num: usize) -> Result<(), ConfigParseError> {
    match param {
        // Network settings
        "bind" => {
            config.network.bind_addr = value.to_string();
        }
        "port" => {
            config.network.port = parse_value(param, value, line_num)?;
        }
        "tcp-backlog" => {
            config.network.tcp_backlog = parse_value(param, value, line_num)?;
        }
        "timeout" => {
            config.network.timeout = parse_value(param, value, line_num)?;
        }
        "tcp-keepalive" => {
            let keepalive: u64 = parse_value(param, value, line_num)?;
            config.network.tcp_keepalive = if keepalive == 0 { None } else { Some(keepalive) };
        }
        "requirepass" => {
            config.network.password = Some(value.to_string());
        }
        "protected-mode" => {
            // We only support protected mode on
            // Just consume the parameter but don't change behavior
            let _enabled: bool = parse_yes_no(param, value, line_num)?;
        }
        
        // Server settings
        "daemonize" => {
            config.server.daemonize = parse_yes_no(param, value, line_num)?;
        }
        "databases" => {
            config.server.databases = parse_value(param, value, line_num)?;
        }
        "pidfile" => {
            config.server.pid_file = if value.is_empty() { None } else { Some(value.to_string()) };
        }
        "loglevel" => {
            config.server.log_level = match value.to_lowercase().as_str() {
                "debug" => LogLevel::Debug,
                "verbose" => LogLevel::Verbose,
                "notice" => LogLevel::Notice,
                "warning" => LogLevel::Warning,
                _ => return Err(ConfigParseError::Value(param.to_string(), line_num, value.to_string())),
            };
        }
        "logfile" => {
            config.server.log_file = value.to_string();
        }
        "always-show-logo" => {
            config.server.show_logo = parse_yes_no(param, value, line_num)?;
        }
        
        // RDB settings
        "dbfilename" => {
            config.rdb.filename = value.to_string();
        }
        "dir" => {
            // Both RDB and AOF use this
            config.rdb.dir = value.to_string();
            config.aof.dir = value.to_string();
        }
        "save" => {
            if value.is_empty() || value == "\"\"" {
                // Empty save rule means disable RDB
                config.rdb.auto_save = false;
                config.rdb.save_rules.clear();
            } else {
                // Parse save rule format: "seconds changes"
                let parts: Vec<&str> = value.split_whitespace().collect();
                if parts.len() != 2 {
                    return Err(ConfigParseError::Value(param.to_string(), line_num, value.to_string()));
                }
                
                let seconds: u64 = parts[0].parse()
                    .map_err(|_| ConfigParseError::Value(param.to_string(), line_num, value.to_string()))?;
                
                let changes: u64 = parts[1].parse()
                    .map_err(|_| ConfigParseError::Value(param.to_string(), line_num, value.to_string()))?;
                
                // Add save rule
                config.rdb.auto_save = true;
                config.rdb.save_rules.push((seconds, changes));
            }
        }
        "rdbcompression" => {
            config.rdb.compress_strings = parse_yes_no(param, value, line_num)?;
        }
        
        // AOF settings
        "appendonly" => {
            config.aof.enabled = parse_yes_no(param, value, line_num)?;
        }
        "appendfilename" => {
            config.aof.filename = value.trim_matches('"').to_string();
        }
        "appendfsync" => {
            config.aof.fsync_policy = match value {
                "always" => FsyncPolicy::Always,
                "everysec" => FsyncPolicy::EverySecond,
                "no" => FsyncPolicy::No,
                _ => return Err(ConfigParseError::Value(param.to_string(), line_num, value.to_string())),
            };
        }
        "auto-aof-rewrite-percentage" => {
            config.aof.auto_rewrite_percentage = parse_value(param, value, line_num)?;
        }
        "auto-aof-rewrite-min-size" => {
            // Parse size formats like "64mb"
            config.aof.auto_rewrite_min_size = parse_size(param, value, line_num)?;
        }

        // Replication settings
        "replicaof" | "slaveof" => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            if parts.len() != 2 {
                return Err(ConfigParseError::Value(param.to_string(), line_num, value.to_string()));
            }
            
            let host = parts[0].to_string();
            let port: u16 = parts[1].parse()
                .map_err(|_| ConfigParseError::Value(param.to_string(), line_num, value.to_string()))?;
            
            if host.to_lowercase() == "no" && port == 1 { // "NO ONE" parsed as "no" + "1"
                config.replication.master_host = None;
                config.replication.master_port = None;
            } else {
                config.replication.master_host = Some(host);
                config.replication.master_port = Some(port);
            }
        }
        "repl-backlog-size" => {
            config.replication.backlog_size = parse_size(param, value, line_num)? as usize;
        }
        "repl-timeout" => {
            config.replication.timeout = parse_value(param, value, line_num)?;
        }
        "repl-ping-replica-period" | "repl-ping-slave-period" => {
            config.replication.ping_replica_period = parse_value(param, value, line_num)?;
        }
        "repl-diskless-sync" => {
            config.replication.diskless_sync = parse_yes_no(param, value, line_num)?;
        }
        
        // Memory settings
        "maxmemory" => {
            config.memory.max_memory = parse_size(param, value, line_num)? as usize;
        }
        "maxmemory-policy" => {
            config.memory.max_memory_policy = match value.to_lowercase().as_str() {
                "noeviction" => EvictionPolicy::NoEviction,
                "allkeys-lru" => EvictionPolicy::AllKeysLru,
                "volatile-lru" => EvictionPolicy::VolatileLru,
                "allkeys-random" => EvictionPolicy::AllKeysRandom,
                "volatile-random" => EvictionPolicy::VolatileRandom,
                "volatile-ttl" => EvictionPolicy::VolatileTtl,
                _ => return Err(ConfigParseError::Value(param.to_string(), line_num, value.to_string())),
            };
        }
        
        // Ignore other parameters
        _ => {
            // Just skip unknown parameters instead of erroring
            println!("Warning: unknown configuration parameter '{}' at line {} - skipping", param, line_num);
        }
    }
    
    Ok(())
}

/// Parse a value that implements FromStr
fn parse_value<T: FromStr>(param: &str, value: &str, line_num: usize) -> Result<T, ConfigParseError> {
    value.parse::<T>()
        .map_err(|_| ConfigParseError::Value(param.to_string(), line_num, value.to_string()))
}

/// Parse a yes/no value
fn parse_yes_no(param: &str, value: &str, line_num: usize) -> Result<bool, ConfigParseError> {
    match value.to_lowercase().as_str() {
        "yes" | "1" => Ok(true),
        "no" | "0" => Ok(false),
        _ => Err(ConfigParseError::Value(param.to_string(), line_num, value.to_string())),
    }
}

/// Parse a size value (e.g., 64mb, 2gb)
fn parse_size(param: &str, value: &str, line_num: usize) -> Result<u64, ConfigParseError> {
    let value = value.trim().to_lowercase();
    let mut chars = value.chars();
    
    // Find the end of the numeric part
    let mut idx = 0;
    while let Some(c) = chars.next() {
        if !c.is_ascii_digit() {
            break;
        }
        idx += 1;
    }
    
    // Parse the numeric part
    if idx == 0 {
        return Err(ConfigParseError::Value(param.to_string(), line_num, value.clone()));
    }
    let num_part = &value[..idx];
    let num: u64 = num_part.parse()
        .map_err(|_| ConfigParseError::Value(param.to_string(), line_num, value.clone()))?;
    
    // Parse the unit part
    let unit_part = &value[idx..];
    let multiplier = match unit_part {
        "" => 1, // No unit, just bytes
        "b" => 1,
        "kb" => 1024,
        "mb" => 1024 * 1024,
        "gb" => 1024 * 1024 * 1024,
        _ => return Err(ConfigParseError::Value(param.to_string(), line_num, value)),
    };
    
    Ok(num * multiplier)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::write;
    use tempfile::NamedTempFile;
    
    #[test]
    fn test_parse_basic_config() {
        let config_content = r#"
# This is a comment
bind 192.168.1.1
port 9999
requirepass secretpassword

# Persistence
save 900 1
save 300 10
save 60 10000
dir ./data
dbfilename dump.ferrous.rdb

# Replication
replicaof 192.168.1.100 6379
"#;
        
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();
        write(path, config_content).unwrap();
        
        let config = parse_config_file(path).unwrap();
        
        assert_eq!(config.network.bind_addr, "192.168.1.1");
        assert_eq!(config.network.port, 9999);
        assert_eq!(config.network.password, Some("secretpassword".to_string()));
        
        assert_eq!(config.rdb.filename, "dump.ferrous.rdb");
        assert_eq!(config.rdb.dir, path.parent().unwrap().to_string_lossy().to_string());
        
        assert_eq!(config.replication.master_host, Some("192.168.1.100".to_string()));
        assert_eq!(config.replication.master_port, Some(6379));
    }
    
    #[test]
    fn test_parse_yes_no() {
        assert_eq!(parse_yes_no("test", "yes", 1).unwrap(), true);
        assert_eq!(parse_yes_no("test", "no", 1).unwrap(), false);
        assert_eq!(parse_yes_no("test", "1", 1).unwrap(), true);
        assert_eq!(parse_yes_no("test", "0", 1).unwrap(), false);
        assert!(parse_yes_no("test", "invalid", 1).is_err());
    }
    
    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("test", "1024", 1).unwrap(), 1024);
        assert_eq!(parse_size("test", "1kb", 1).unwrap(), 1024);
        assert_eq!(parse_size("test", "1mb", 1).unwrap(), 1024 * 1024);
        assert_eq!(parse_size("test", "1gb", 1).unwrap(), 1024 * 1024 * 1024);
        assert!(parse_size("test", "invalid", 1).is_err());
    }
}