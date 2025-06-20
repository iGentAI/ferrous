//! Configuration module for Ferrous
//! 
//! Provides a centralized configuration system that supports both
//! configuration files and command-line arguments.

mod parser;
mod cli;

pub use parser::{parse_config_file, ConfigParseError};
pub use cli::{parse_cli_args, CliArgs};

use crate::network::NetworkConfig;
use crate::storage::{RdbConfig, AofConfig};
use crate::storage::memory::EvictionPolicy;
use crate::replication::ReplicationConfig;

use std::path::PathBuf;

/// Main configuration structure for Ferrous
#[derive(Debug, Clone)]
pub struct Config {
    /// Server configuration
    pub server: ServerConfig,
    
    /// Network configuration
    pub network: NetworkConfig,
    
    /// RDB persistence configuration
    pub rdb: RdbConfig,
    
    /// AOF persistence configuration
    pub aof: AofConfig,
    
    /// Replication configuration
    pub replication: ReplicationConfig,
    
    /// Memory management configuration
    pub memory: MemoryConfig,
}

/// Server-specific configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Server process title
    pub proc_title: String,
    
    /// Number of databases
    pub databases: usize,
    
    /// Daemonize server (run in background)
    pub daemonize: bool,
    
    /// Log level
    pub log_level: LogLevel,
    
    /// Log file path (empty for stdout)
    pub log_file: String,
    
    /// PID file path
    pub pid_file: Option<String>,
    
    /// Show logo on startup
    pub show_logo: bool,
}

/// Memory management configuration
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    /// Maximum memory to use (0 = unlimited)
    pub max_memory: usize,
    
    /// Eviction policy when memory limit is reached
    pub max_memory_policy: EvictionPolicy,
    
    /// Sample size for eviction (number of keys to sample)
    pub max_memory_samples: usize,
}

/// Log level configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// Debug level - most verbose
    Debug,
    
    /// Verbose level
    Verbose,
    
    /// Notice level - default
    Notice,
    
    /// Warning level
    Warning,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            server: ServerConfig::default(),
            network: NetworkConfig::default(),
            rdb: RdbConfig::default(),
            aof: AofConfig::default(),
            replication: ReplicationConfig::default(),
            memory: MemoryConfig::default(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            proc_title: "ferrous-server".to_string(),
            databases: 16,
            daemonize: false,
            log_level: LogLevel::Notice,
            log_file: "".to_string(),
            pid_file: None,
            show_logo: true,
        }
    }
}

impl Default for MemoryConfig {
    fn default() -> Self {
        MemoryConfig {
            max_memory: 0, // Unlimited
            max_memory_policy: EvictionPolicy::NoEviction,
            max_memory_samples: 5,
        }
    }
}

impl Config {
    /// Load configuration from a file
    pub fn from_file(path: impl Into<PathBuf>) -> Result<Self, ConfigParseError> {
        let path = path.into();
        parse_config_file(&path)
    }
    
    /// Apply command-line arguments to override config
    pub fn apply_cli_args(&mut self, args: CliArgs) {
        // Apply network overrides
        if let Some(port) = args.port {
            self.network.port = port;
        }
        if let Some(bind_addr) = args.bind {
            self.network.bind_addr = bind_addr;
        }
        if let Some(password) = args.password {
            self.network.password = Some(password);
        }
        
        // Apply replication overrides
        if let Some((host, port)) = args.replicaof {
            self.replication.master_host = Some(host);
            self.replication.master_port = Some(port);
        }
        
        // Apply server config overrides
        if let Some(log_file) = args.logfile {
            self.server.log_file = log_file;
        }
        if let Some(dir) = args.dir {
            self.rdb.dir = dir.clone();
            self.aof.dir = dir;
        }
        
        // Apply persistence overrides
        if let Some(dbfilename) = args.dbfilename {
            self.rdb.filename = dbfilename;
        }
        if args.appendonly {
            self.aof.enabled = true;
        }
    }
    
    /// Get a configuration parameter by name
    pub fn get(&self, param: &str) -> Option<String> {
        match param {
            "port" => Some(self.network.port.to_string()),
            "bind" => Some(self.network.bind_addr.clone()),
            "timeout" => Some(self.network.timeout.to_string()),
            "tcp-keepalive" => self.network.tcp_keepalive.map(|v| v.to_string()),
            "protected-mode" => Some("yes".to_string()), // Always enabled
            "databases" => Some(self.server.databases.to_string()),
            "dbfilename" => Some(self.rdb.filename.clone()),
            "dir" => Some(self.rdb.dir.clone()),
            "maxmemory" => Some(self.memory.max_memory.to_string()),
            "maxmemory-policy" => Some(self.memory_policy_str()),
            "appendonly" => Some(if self.aof.enabled { "yes" } else { "no" }.to_string()),
            "appendfilename" => Some(self.aof.filename.clone()),
            "appendfsync" => Some(self.fsync_policy_str()),
            "save" => Some(self.format_save_rules()),
            _ => None,
        }
    }
    
    /// Get all configuration parameters
    pub fn get_all(&self) -> Vec<(String, String)> {
        let mut params = Vec::new();
        
        // Network params
        params.push(("port".to_string(), self.network.port.to_string()));
        params.push(("bind".to_string(), self.network.bind_addr.clone()));
        params.push(("timeout".to_string(), self.network.timeout.to_string()));
        if let Some(keepalive) = self.network.tcp_keepalive {
            params.push(("tcp-keepalive".to_string(), keepalive.to_string()));
        }
        params.push(("protected-mode".to_string(), "yes".to_string()));
        
        // Server params
        params.push(("databases".to_string(), self.server.databases.to_string()));
        params.push(("daemonize".to_string(), if self.server.daemonize { "yes" } else { "no" }.to_string()));
        
        // RDB params
        params.push(("dbfilename".to_string(), self.rdb.filename.clone()));
        params.push(("dir".to_string(), self.rdb.dir.clone()));
        params.push(("save".to_string(), self.format_save_rules()));
        
        // AOF params
        params.push(("appendonly".to_string(), if self.aof.enabled { "yes" } else { "no" }.to_string()));
        params.push(("appendfilename".to_string(), self.aof.filename.clone()));
        params.push(("appendfsync".to_string(), self.fsync_policy_str()));
        
        // Memory params
        params.push(("maxmemory".to_string(), self.memory.max_memory.to_string()));
        params.push(("maxmemory-policy".to_string(), self.memory_policy_str()));
        
        params
    }
    
    // Helper methods for formatting
    
    fn format_save_rules(&self) -> String {
        let mut result = String::new();
        for (seconds, changes) in &self.rdb.save_rules {
            if !result.is_empty() {
                result.push(' ');
            }
            result.push_str(&format!("{} {}", seconds, changes));
        }
        result
    }
    
    fn memory_policy_str(&self) -> String {
        match self.memory.max_memory_policy {
            EvictionPolicy::NoEviction => "noeviction".to_string(),
            EvictionPolicy::AllKeysLru => "allkeys-lru".to_string(),
            EvictionPolicy::VolatileLru => "volatile-lru".to_string(),
            EvictionPolicy::AllKeysRandom => "allkeys-random".to_string(),
            EvictionPolicy::VolatileRandom => "volatile-random".to_string(),
            EvictionPolicy::VolatileTtl => "volatile-ttl".to_string(),
        }
    }
    
    fn fsync_policy_str(&self) -> String {
        match self.aof.fsync_policy {
            crate::storage::aof::FsyncPolicy::Always => "always".to_string(),
            crate::storage::aof::FsyncPolicy::EverySecond => "everysec".to_string(),
            crate::storage::aof::FsyncPolicy::No => "no".to_string(),
        }
    }
}

/// Errors that can occur during configuration
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Configuration file parse error
    #[error("Failed to parse config: {0}")]
    Parse(#[from] ConfigParseError),
    
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    /// Other errors
    #[error("Configuration error: {0}")]
    Other(String),
}