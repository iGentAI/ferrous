//! Main server implementation

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::thread;
use crate::error::{FerrousError, Result};
use crate::protocol::RespFrame;
use crate::storage::{StorageEngine, GetResult, RdbEngine, StorageMonitor, RdbConfig};
use crate::pubsub::{PubSubManager, format_message, format_pmessage, 
                    format_subscribe_response, format_psubscribe_response,
                    format_unsubscribe_response, format_punsubscribe_response};
use super::{Listener, Connection, NetworkConfig};

/// Connection ID generator
static CONN_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Main server struct
pub struct Server {
    listener: Listener,
    connections: Arc<Mutex<HashMap<u64, Connection>>>,
    config: NetworkConfig,
    /// Storage engine for Redis data
    storage: Arc<StorageEngine>,
    /// RDB persistence engine
    rdb_engine: Option<Arc<RdbEngine>>,
    /// Storage monitor for auto-save
    storage_monitor: Option<StorageMonitor>,
    /// Pub/sub manager
    pubsub: Arc<PubSubManager>,
}

impl Server {
    /// Create a new server with default config
    pub fn new() -> Result<Self> {
        Self::with_config(NetworkConfig::default())
    }
    
    /// Create a new server with custom config
    pub fn with_config(config: NetworkConfig) -> Result<Self> {
        let listener = Listener::bind(config.clone())?;
        let connections = Arc::new(Mutex::new(HashMap::new()));
        let storage = StorageEngine::new();
        
        // Create RDB engine with default config
        let rdb_config = RdbConfig::default();
        let rdb_engine = Arc::new(RdbEngine::new(rdb_config.clone()));
        
        // Create storage monitor
        let mut monitor = StorageMonitor::new();
        
        // Create pub/sub manager
        let pubsub = PubSubManager::new();
        
        // Load existing RDB if available
        if let Err(e) = rdb_engine.load(&storage) {
            eprintln!("Failed to load RDB file: {}", e);
        }
        
        // Start background monitoring if auto-save is enabled
        if rdb_config.auto_save {
            monitor.start(Arc::clone(&storage), Arc::clone(&rdb_engine), rdb_config);
        }
        
        Ok(Server {
            listener,
            connections,
            config,
            storage,
            rdb_engine: Some(rdb_engine),
            storage_monitor: Some(monitor),
            pubsub,
        })
    }
    
    /// Set RDB engine for persistence
    pub fn set_rdb_engine(&mut self, rdb_engine: Arc<RdbEngine>) {
        self.rdb_engine = Some(rdb_engine);
    }
    
    /// Set storage monitor
    pub fn set_storage_monitor(&mut self, monitor: StorageMonitor) {
        self.storage_monitor = Some(monitor);
    }
    
    /// Run the server
    pub fn run(&mut self) -> Result<()> {
        println!("Ferrous server v{} ready to accept connections", env!("CARGO_PKG_VERSION"));
        
        loop {
            // Accept new connections
            self.accept_connections()?;
            
            // Process existing connections
            self.process_connections()?;
            
            // Clean up closed connections
            self.cleanup_connections()?;
            
            // Small sleep to prevent busy waiting
            thread::sleep(Duration::from_micros(100));
        }
    }
    
    /// Accept new connections
    fn accept_connections(&mut self) -> Result<()> {
        while let Some((stream, addr)) = self.listener.accept()? {
            let id = CONN_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
            
            // Check max clients limit
            {
                let connections = self.connections.lock().unwrap();
                if connections.len() >= self.config.max_clients {
                    println!("Max clients reached, rejecting connection from {}", addr);
                    drop(stream); // Close connection
                    continue;
                }
            }
            
            // Create new connection
            match Connection::new(id, stream, addr) {
                Ok(conn) => {
                    println!("Client {} connected from {}", id, addr);
                    let mut connections = self.connections.lock().unwrap();
                    connections.insert(id, conn);
                }
                Err(e) => {
                    eprintln!("Failed to create connection: {}", e);
                }
            }
        }
        
        Ok(())
    }
    
    /// Process all connections
    fn process_connections(&mut self) -> Result<()> {
        let mut to_remove = Vec::new();
        
        // Clone the connections to avoid holding the lock too long
        let conn_ids: Vec<u64> = {
            let connections = self.connections.lock().unwrap();
            connections.keys().cloned().collect()
        };
        
        for id in conn_ids {
            // Process each connection
            match self.process_connection(id) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Error processing connection {}: {}", id, e);
                    to_remove.push(id);
                }
            }
        }
        
        // Remove failed connections
        if !to_remove.is_empty() {
            let mut connections = self.connections.lock().unwrap();
            for id in to_remove {
                if let Some(conn) = connections.remove(&id) {
                    println!("Client {} disconnected from {}", id, conn.addr);
                }
            }
        }
        
        Ok(())
    }
    
    /// Process a single connection
    fn process_connection(&mut self, id: u64) -> Result<()> {
        // Read and parse frames without holding the lock during processing
        let mut frames_to_process = Vec::new();
        let mut should_close = false;
        let mut timeout_check = false;
        
        // First phase: read and parse with the lock
        {
            let mut connections = self.connections.lock().unwrap();
            let conn = connections.get_mut(&id).ok_or_else(|| {
                FerrousError::Internal("Connection not found".into())
            })?;
            
            // Read data from connection
            match conn.read() {
                Ok(true) => {
                    // Data was read, try to parse frames
                    while let Some(frame) = conn.parse_frame()? {
                        frames_to_process.push(frame);
                    }
                }
                Ok(false) => {
                    // No data available (would block)
                }
                Err(e) => {
                    // Connection error
                    conn.close()?;
                    return Err(e);
                }
            }
            
            // Check for timeout
            if self.config.timeout > 0 {
                timeout_check = conn.idle_time() > Duration::from_secs(self.config.timeout);
            }
        }
        
        // Second phase: process frames without the lock
        let mut responses = Vec::new();
        for frame in frames_to_process {
            // We need to check if this is a QUIT command
            if let RespFrame::Array(Some(parts)) = &frame {
                if !parts.is_empty() {
                    if let RespFrame::BulkString(Some(bytes)) = &parts[0] {
                        if String::from_utf8_lossy(bytes).to_uppercase() == "QUIT" {
                            should_close = true;
                        }
                    }
                }
            }
            
            let response = self.process_frame(frame, id)?;
            responses.push(response);
        }
        
        // Third phase: send responses and handle state changes
        {
            let mut connections = self.connections.lock().unwrap();
            let conn = connections.get_mut(&id).ok_or_else(|| {
                FerrousError::Internal("Connection not found".into())
            })?;
            
            // Send all responses
            for response in responses {
                conn.send_frame(&response)?;
            }
            
            // Try to flush any pending writes
            conn.flush()?;
            
            // Handle QUIT command
            if should_close {
                conn.state = super::ConnectionState::Closing;
            }
            
            // Handle timeout
            if timeout_check {
                conn.close()?;
                return Err(FerrousError::Connection("Connection timed out".into()));
            }
        }
        
        Ok(())
    }
    
    /// Process a RESP frame and generate a response
    fn process_frame(&mut self, frame: RespFrame, conn_id: u64) -> Result<RespFrame> {
        match &frame {
            RespFrame::Array(Some(parts)) if !parts.is_empty() => {
                // Extract command name
                let cmd_frame = &parts[0];
                let command = match cmd_frame {
                    RespFrame::BulkString(Some(bytes)) => {
                        String::from_utf8_lossy(bytes).to_uppercase()
                    }
                    _ => return Ok(RespFrame::error("ERR invalid command format")),
                };
                
                // Route to command handler
                match command.as_str() {
                    "PING" => self.handle_ping(parts),
                    "ECHO" => self.handle_echo(parts),
                    "SET" => self.handle_set(parts),
                    "GET" => self.handle_get(parts),
                    "INCR" => self.handle_incr(parts),
                    "DECR" => self.handle_decr(parts),
                    "INCRBY" => self.handle_incrby(parts),
                    "DEL" => self.handle_del(parts),
                    "EXISTS" => self.handle_exists(parts),
                    "EXPIRE" => self.handle_expire(parts),
                    "TTL" => self.handle_ttl(parts),
                    // Sorted set commands
                    "ZADD" => self.handle_zadd(parts),
                    "ZREM" => self.handle_zrem(parts),
                    "ZSCORE" => self.handle_zscore(parts),
                    "ZRANK" => self.handle_zrank(parts),
                    "ZREVRANK" => self.handle_zrevrank(parts),
                    "ZRANGE" => self.handle_zrange(parts),
                    "ZREVRANGE" => self.handle_zrevrange(parts),
                    "ZRANGEBYSCORE" => self.handle_zrangebyscore(parts),
                    "ZREVRANGEBYSCORE" => self.handle_zrevrangebyscore(parts),
                    "ZCOUNT" => self.handle_zcount(parts),
                    "ZINCRBY" => self.handle_zincrby(parts),
                    // RDB commands
                    "SAVE" => self.handle_save(parts),
                    "BGSAVE" => self.handle_bgsave(parts),
                    "LASTSAVE" => self.handle_lastsave(parts),
                    // Pub/Sub commands
                    "PUBLISH" => self.handle_publish(parts),
                    "SUBSCRIBE" => self.handle_subscribe(parts, conn_id),
                    "UNSUBSCRIBE" => self.handle_unsubscribe(parts, conn_id),
                    "PSUBSCRIBE" => self.handle_psubscribe(parts, conn_id),
                    "PUNSUBSCRIBE" => self.handle_punsubscribe(parts, conn_id),
                    "QUIT" => Ok(RespFrame::ok()),
                    _ => Ok(RespFrame::error(format!("ERR unknown command '{}'", command))),
                }
            }
            _ => Ok(RespFrame::error("ERR invalid request format")),
        }
    }

    /// Record a data change for auto-save monitoring
    fn record_change(&self) {
        if let Some(monitor) = &self.storage_monitor {
            monitor.record_change();
        }
    }

    /// Handle PUBLISH command
    fn handle_publish(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        if parts.len() != 3 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'publish' command"));
        }
        
        let channel = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid channel format")),
        };
        
        let message = match &parts[2] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid message format")),
        };
        
        // Get all subscribers
        let receivers = self.pubsub.publish(channel, message)?;
        let num_receivers = receivers.len();
        
        // Send message to all subscribers
        if num_receivers > 0 {
            let mut connections = self.connections.lock().unwrap();
            
            for (conn_id, pattern) in receivers {
                if let Some(conn) = connections.get_mut(&conn_id) {
                    let frame = if let Some(pat) = pattern {
                        format_pmessage(&pat, channel, message)
                    } else {
                        format_message(channel, message)
                    };
                    
                    // Best effort delivery - ignore errors
                    let _ = conn.send_frame(&frame);
                }
            }
        }
        
        Ok(RespFrame::Integer(num_receivers as i64))
    }
    
    /// Handle SUBSCRIBE command
    fn handle_subscribe(&self, parts: &[RespFrame], conn_id: u64) -> Result<RespFrame> {
        if parts.len() < 2 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'subscribe' command"));
        }
        
        let mut channels = Vec::new();
        for i in 1..parts.len() {
            match &parts[i] {
                RespFrame::BulkString(Some(bytes)) => channels.push(bytes.as_ref().to_vec()),
                _ => return Ok(RespFrame::error("ERR invalid channel format")),
            }
        }
        
        let results = self.pubsub.subscribe(conn_id, channels)?;
        
        // Return array of subscription confirmations
        let responses: Vec<RespFrame> = results.into_iter()
            .map(|r| match r.subscription {
                crate::pubsub::Subscription::Channel(ch) => {
                    format_subscribe_response(&ch, r.num_subscriptions)
                }
                _ => unreachable!(),
            })
            .collect();
        
        Ok(RespFrame::Array(Some(responses)))
    }
    
    /// Handle UNSUBSCRIBE command
    fn handle_unsubscribe(&self, parts: &[RespFrame], conn_id: u64) -> Result<RespFrame> {
        let channels = if parts.len() > 1 {
            let mut chans = Vec::new();
            for i in 1..parts.len() {
                match &parts[i] {
                    RespFrame::BulkString(Some(bytes)) => chans.push(bytes.as_ref().to_vec()),
                    _ => return Ok(RespFrame::error("ERR invalid channel format")),
                }
            }
            Some(chans)
        } else {
            None // Unsubscribe from all
        };
        
        let results = self.pubsub.unsubscribe(conn_id, channels)?;
        
        // Return array of unsubscription confirmations
        let responses: Vec<RespFrame> = results.into_iter()
            .map(|r| match r.subscription {
                crate::pubsub::Subscription::Channel(ch) => {
                    format_unsubscribe_response(&ch, r.num_subscriptions)
                }
                _ => unreachable!(),
            })
            .collect();
        
        Ok(RespFrame::Array(Some(responses)))
    }
    
    /// Handle PSUBSCRIBE command
    fn handle_psubscribe(&self, parts: &[RespFrame], conn_id: u64) -> Result<RespFrame> {
        if parts.len() < 2 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'psubscribe' command"));
        }
        
        let mut patterns = Vec::new();
        for i in 1..parts.len() {
            match &parts[i] {
                RespFrame::BulkString(Some(bytes)) => patterns.push(bytes.as_ref().to_vec()),
                _ => return Ok(RespFrame::error("ERR invalid pattern format")),
            }
        }
        
        let results = self.pubsub.psubscribe(conn_id, patterns)?;
        
        // Return array of subscription confirmations
        let responses: Vec<RespFrame> = results.into_iter()
            .map(|r| match r.subscription {
                crate::pubsub::Subscription::Pattern(pat) => {
                    format_psubscribe_response(&pat, r.num_subscriptions)
                }
                _ => unreachable!(),
            })
            .collect();
        
        Ok(RespFrame::Array(Some(responses)))
    }
    
    /// Handle PUNSUBSCRIBE command
    fn handle_punsubscribe(&self, parts: &[RespFrame], conn_id: u64) -> Result<RespFrame> {
        let patterns = if parts.len() > 1 {
            let mut pats = Vec::new();
            for i in 1..parts.len() {
                match &parts[i] {
                    RespFrame::BulkString(Some(bytes)) => pats.push(bytes.as_ref().to_vec()),
                    _ => return Ok(RespFrame::error("ERR invalid pattern format")),
                }
            }
            Some(pats)
        } else {
            None // Unsubscribe from all patterns
        };
        
        let results = self.pubsub.punsubscribe(conn_id, patterns)?;
        
        // Return array of unsubscription confirmations
        let responses: Vec<RespFrame> = results.into_iter()
            .map(|r| match r.subscription {
                crate::pubsub::Subscription::Pattern(pat) => {
                    format_punsubscribe_response(&pat, r.num_subscriptions)
                }
                _ => unreachable!(),
            })
            .collect();
        
        Ok(RespFrame::Array(Some(responses)))
    }

    /// Handle SAVE command
    fn handle_save(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        if parts.len() != 1 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'save' command"));
        }
        
        if let Some(rdb_engine) = &self.rdb_engine {
            match rdb_engine.save(&self.storage) {
                Ok(_) => {
                    if let Some(monitor) = &self.storage_monitor {
                        monitor.reset_changes();
                    }
                    Ok(RespFrame::ok())
                },
                Err(e) => Ok(RespFrame::error(format!("ERR {}", e))),
            }
        } else {
            Ok(RespFrame::error("ERR RDB persistence not configured"))
        }
    }
    
    /// Handle BGSAVE command
    fn handle_bgsave(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        if parts.len() != 1 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'bgsave' command"));
        }
        
        if let Some(rdb_engine) = &self.rdb_engine {
            match rdb_engine.bgsave(Arc::clone(&self.storage)) {
                Ok(_) => Ok(RespFrame::SimpleString(Arc::new(b"Background saving started".to_vec()))),
                Err(e) => Ok(RespFrame::error(format!("ERR {}", e))),
            }
        } else {
            Ok(RespFrame::error("ERR RDB persistence not configured"))
        }
    }
    
    /// Handle LASTSAVE command
    fn handle_lastsave(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        if parts.len() != 1 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'lastsave' command"));
        }
        
        if let Some(rdb_engine) = &self.rdb_engine {
            if let Some(last_save) = rdb_engine.last_save_time() {
                let timestamp = last_save.duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                Ok(RespFrame::Integer(timestamp))
            } else {
                Ok(RespFrame::Integer(0)) // Never saved
            }
        } else {
            Ok(RespFrame::error("ERR RDB persistence not configured"))
        }
    }
    
    /// Handle ZADD command
    fn handle_zadd(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        // ZADD key score member [score member ...]
        if parts.len() < 4 || parts.len() % 2 != 0 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'zadd' command"));
        }
        
        // Extract key
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        let mut new_members = 0;
        
        // Process each score-member pair
        for i in (2..parts.len()).step_by(2) {
            let score = match &parts[i] {
                RespFrame::BulkString(Some(bytes)) => {
                    match String::from_utf8_lossy(bytes).parse::<f64>() {
                        Ok(n) => n,
                        Err(_) => return Ok(RespFrame::error("ERR value is not a valid float")),
                    }
                }
                _ => return Ok(RespFrame::error("ERR invalid score format")),
            };
            
            let member = match &parts[i+1] {
                RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
                _ => return Ok(RespFrame::error("ERR invalid member format")),
            };
            
            // Add to sorted set 
            if self.storage.zadd(0, key.clone(), member, score)? {
                new_members += 1;
                // Record change for auto-save
                self.record_change();
            }
        }
        
        Ok(RespFrame::Integer(new_members))
    }
    
    /// Handle ZREM command
    fn handle_zrem(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        // ZREM key member [member ...]
        if parts.len() < 3 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'zrem' command"));
        }
        
        // Extract key
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        let mut removed = 0;
        
        // Process each member
        for i in 2..parts.len() {
            let member = match &parts[i] {
                RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                _ => continue, // Skip invalid members
            };
            
            // Remove from sorted set
            if self.storage.zrem(0, key, member)? {
                removed += 1;
            }
        }
        
        Ok(RespFrame::Integer(removed))
    }
    
    /// Handle ZSCORE command
    fn handle_zscore(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        // ZSCORE key member
        if parts.len() != 3 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'zscore' command"));
        }
        
        // Extract key
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        // Extract member
        let member = match &parts[2] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid member format")),
        };
        
        // Get score
        match self.storage.zscore(0, key, member)? {
            Some(score) => {
                // Convert f64 to string with Redis protocol formatting
                let score_str = format!("{}", score);
                Ok(RespFrame::from_string(score_str))
            }
            None => Ok(RespFrame::null_bulk()), // Member not found or key doesn't exist
        }
    }
    
    /// Handle ZRANK command
    fn handle_zrank(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        // ZRANK key member
        if parts.len() != 3 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'zrank' command"));
        }
        
        // Extract key
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        // Extract member
        let member = match &parts[2] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid member format")),
        };
        
        // Get rank
        match self.storage.zrank(0, key, member, false)? {
            Some(rank) => Ok(RespFrame::Integer(rank as i64)),
            None => Ok(RespFrame::null_bulk()), // Member not found or key doesn't exist
        }
    }
    
    /// Handle ZREVRANK command
    fn handle_zrevrank(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        // ZREVRANK key member
        if parts.len() != 3 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'zrevrank' command"));
        }
        
        // Extract key
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        // Extract member
        let member = match &parts[2] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid member format")),
        };
        
        // Get rank (reversed)
        match self.storage.zrank(0, key, member, true)? {
            Some(rank) => Ok(RespFrame::Integer(rank as i64)),
            None => Ok(RespFrame::null_bulk()), // Member not found or key doesn't exist
        }
    }
    
    /// Handle ZRANGE command
    fn handle_zrange(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        // ZRANGE key start stop [WITHSCORES]
        if parts.len() < 4 || parts.len() > 5 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'zrange' command"));
        }
        
        // Extract key
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        // Extract start
        let start = match &parts[2] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<isize>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid start format")),
        };
        
        // Extract stop
        let stop = match &parts[3] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<isize>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid stop format")),
        };
        
        // Check if WITHSCORES option is present
        let with_scores = parts.len() == 5 && match &parts[4] {
            RespFrame::BulkString(Some(bytes)) => {
                String::from_utf8_lossy(bytes).to_uppercase() == "WITHSCORES"
            }
            _ => false,
        };
        
        // Get range
        let members = self.storage.zrange(0, key, start, stop, false)?;
        
        // Format response
        if with_scores {
            // Return as array of [member1, score1, member2, score2, ...]
            let mut response = Vec::with_capacity(members.len() * 2);
            for (member, score) in members {
                response.push(RespFrame::from_bytes(member));
                response.push(RespFrame::from_string(score.to_string()));
            }
            Ok(RespFrame::Array(Some(response)))
        } else {
            // Return as array of [member1, member2, ...]
            let response = members.into_iter()
                .map(|(member, _)| RespFrame::from_bytes(member))
                .collect();
            Ok(RespFrame::Array(Some(response)))
        }
    }
    
    /// Handle ZREVRANGE command
    fn handle_zrevrange(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        // ZREVRANGE key start stop [WITHSCORES]
        if parts.len() < 4 || parts.len() > 5 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'zrevrange' command"));
        }
        
        // Extract key
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        // Extract start
        let start = match &parts[2] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<isize>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid start format")),
        };
        
        // Extract stop
        let stop = match &parts[3] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<isize>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid stop format")),
        };
        
        // Check if WITHSCORES option is present
        let with_scores = parts.len() == 5 && match &parts[4] {
            RespFrame::BulkString(Some(bytes)) => {
                String::from_utf8_lossy(bytes).to_uppercase() == "WITHSCORES"
            }
            _ => false,
        };
        
        // Get range in reverse order
        let members = self.storage.zrange(0, key, start, stop, true)?;
        
        // Format response
        if with_scores {
            // Return as array of [member1, score1, member2, score2, ...]
            let mut response = Vec::with_capacity(members.len() * 2);
            for (member, score) in members {
                response.push(RespFrame::from_bytes(member));
                response.push(RespFrame::from_string(score.to_string()));
            }
            Ok(RespFrame::Array(Some(response)))
        } else {
            // Return as array of [member1, member2, ...]
            let response = members.into_iter()
                .map(|(member, _)| RespFrame::from_bytes(member))
                .collect();
            Ok(RespFrame::Array(Some(response)))
        }
    }
    
    /// Handle ZRANGEBYSCORE command
    fn handle_zrangebyscore(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        // ZRANGEBYSCORE key min max [WITHSCORES]
        if parts.len() < 4 || parts.len() > 5 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'zrangebyscore' command"));
        }
        
        // Extract key
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        // Extract min score
        let min_score = match &parts[2] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<f64>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR min or max is not a float")),
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid min score format")),
        };
        
        // Extract max score
        let max_score = match &parts[3] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<f64>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR min or max is not a float")),
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid max score format")),
        };
        
        // Check if WITHSCORES option is present
        let with_scores = parts.len() == 5 && match &parts[4] {
            RespFrame::BulkString(Some(bytes)) => {
                String::from_utf8_lossy(bytes).to_uppercase() == "WITHSCORES"
            }
            _ => false,
        };
        
        // Get range by score
        let members = self.storage.zrangebyscore(0, key, min_score, max_score, false)?;
        
        // Format response
        if with_scores {
            // Return as array of [member1, score1, member2, score2, ...]
            let mut response = Vec::with_capacity(members.len() * 2);
            for (member, score) in members {
                response.push(RespFrame::from_bytes(member));
                response.push(RespFrame::from_string(score.to_string()));
            }
            Ok(RespFrame::Array(Some(response)))
        } else {
            // Return as array of [member1, member2, ...]
            let response = members.into_iter()
                .map(|(member, _)| RespFrame::from_bytes(member))
                .collect();
            Ok(RespFrame::Array(Some(response)))
        }
    }
    
    /// Handle ZREVRANGEBYSCORE command
    fn handle_zrevrangebyscore(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        // ZREVRANGEBYSCORE key max min [WITHSCORES]
        if parts.len() < 4 || parts.len() > 5 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'zrevrangebyscore' command"));
        }
        
        // Extract key
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        // Extract max score
        let max_score = match &parts[2] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<f64>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR min or max is not a float")),
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid max score format")),
        };
        
        // Extract min score
        let min_score = match &parts[3] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<f64>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR min or max is not a float")),
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid min score format")),
        };
        
        // Check if WITHSCORES option is present
        let with_scores = parts.len() == 5 && match &parts[4] {
            RespFrame::BulkString(Some(bytes)) => {
                String::from_utf8_lossy(bytes).to_uppercase() == "WITHSCORES"
            }
            _ => false,
        };
        
        // Get range by score in reverse order
        let members = self.storage.zrangebyscore(0, key, min_score, max_score, true)?;
        
        // Format response
        if with_scores {
            // Return as array of [member1, score1, member2, score2, ...]
            let mut response = Vec::with_capacity(members.len() * 2);
            for (member, score) in members {
                response.push(RespFrame::from_bytes(member));
                response.push(RespFrame::from_string(score.to_string()));
            }
            Ok(RespFrame::Array(Some(response)))
        } else {
            // Return as array of [member1, member2, ...]
            let response = members.into_iter()
                .map(|(member, _)| RespFrame::from_bytes(member))
                .collect();
            Ok(RespFrame::Array(Some(response)))
        }
    }
    
    /// Handle ZCOUNT command
    fn handle_zcount(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        // ZCOUNT key min max
        if parts.len() != 4 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'zcount' command"));
        }
        
        // Extract key
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        // Extract min score
        let min_score = match &parts[2] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<f64>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR min or max is not a float")),
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid min score format")),
        };
        
        // Extract max score
        let max_score = match &parts[3] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<f64>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR min or max is not a float")),
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid max score format")),
        };
        
        // Get count
        let count = self.storage.zcount(0, key, min_score, max_score)?;
        
        Ok(RespFrame::Integer(count as i64))
    }
    
    /// Handle ZINCRBY command
    fn handle_zincrby(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        // ZINCRBY key increment member
        if parts.len() != 4 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'zincrby' command"));
        }
        
        // Extract key
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        // Extract increment
        let increment = match &parts[2] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<f64>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR value is not a valid float")),
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid increment format")),
        };
        
        // Extract member
        let member = match &parts[3] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
            _ => return Ok(RespFrame::error("ERR invalid member format")),
        };
        
        // Increment score
        let new_score = self.storage.zincrby(0, key, member, increment)?;
        
        // Return new score as bulk string (Redis protocol format)
        Ok(RespFrame::from_string(new_score.to_string()))
    }
    
    /// Handle PING command
    fn handle_ping(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        if parts.len() == 1 {
            Ok(RespFrame::SimpleString(Arc::new(b"PONG".to_vec())))
        } else if parts.len() == 2 {
            // PING with argument returns the argument
            Ok(parts[1].clone())
        } else {
            Ok(RespFrame::error("ERR wrong number of arguments for 'ping' command"))
        }
    }
    
    /// Handle ECHO command
    fn handle_echo(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        if parts.len() != 2 {
            Ok(RespFrame::error("ERR wrong number of arguments for 'echo' command"))
        } else {
            Ok(parts[1].clone())
        }
    }
    
    /// Handle SET command - now with actual storage
    fn handle_set(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        if parts.len() < 3 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'set' command"));
        }
        
        // Extract key and value
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        let value = match &parts[2] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
            _ => return Ok(RespFrame::error("ERR invalid value format")),
        };
        
        // Parse SET options (EX, PX, NX, XX)
        let mut expiration = None;
        let mut nx = false; // Only set if key doesn't exist
        let mut xx = false; // Only set if key exists
        
        let mut i = 3;
        while i < parts.len() {
            match &parts[i] {
                RespFrame::BulkString(Some(option)) => {
                    let option_str = String::from_utf8_lossy(option).to_uppercase();
                    match option_str.as_str() {
                        "EX" => {
                            if i + 1 >= parts.len() {
                                return Ok(RespFrame::error("ERR syntax error"));
                            }
                            if let RespFrame::BulkString(Some(seconds_bytes)) = &parts[i + 1] {
                                if let Ok(seconds_str) = String::from_utf8(seconds_bytes.as_ref().clone()) {
                                    if let Ok(seconds) = seconds_str.parse::<u64>() {
                                        expiration = Some(Duration::from_secs(seconds));
                                        i += 2;
                                        continue;
                                    }
                                }
                            }
                            return Ok(RespFrame::error("ERR invalid expire time"));
                        }
                        "PX" => {
                            if i + 1 >= parts.len() {
                                return Ok(RespFrame::error("ERR syntax error"));
                            }
                            if let RespFrame::BulkString(Some(millis_bytes)) = &parts[i + 1] {
                                if let Ok(millis_str) = String::from_utf8(millis_bytes.as_ref().clone()) {
                                    if let Ok(millis) = millis_str.parse::<u64>() {
                                        expiration = Some(Duration::from_millis(millis));
                                        i += 2;
                                        continue;
                                    }
                                }
                            }
                            return Ok(RespFrame::error("ERR invalid expire time"));
                        }
                        "NX" => {
                            nx = true;
                            i += 1;
                        }
                        "XX" => {
                            xx = true;
                            i += 1;
                        }
                        _ => return Ok(RespFrame::error("ERR syntax error")),
                    }
                }
                _ => return Ok(RespFrame::error("ERR syntax error")),
            }
        }
        
        // Check NX condition
        if nx {
            if self.storage.exists(0, &key)? {
                return Ok(RespFrame::null_bulk());
            }
        }
        
        // Check XX condition
        if xx {
            if !self.storage.exists(0, &key)? {
                return Ok(RespFrame::null_bulk());
            }
        }
        
        // Set the value
        match expiration {
            Some(expires_in) => {
                self.storage.set_string_ex(0, key, value, expires_in)?;
            }
            None => {
                self.storage.set_string(0, key, value)?;
            }
        }
        
        // Record change for auto-save
        self.record_change();
        
        Ok(RespFrame::ok())
    }
    
    /// Handle GET command - now with actual storage
    fn handle_get(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        if parts.len() != 2 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'get' command"));
        }
        
        // Extract key
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        // Get the value
        match self.storage.get_string(0, key)? {
            Some(value) => Ok(RespFrame::from_bytes(value)),
            None => Ok(RespFrame::null_bulk()),
        }
    }
    
    /// Handle INCR command
    fn handle_incr(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        if parts.len() != 2 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'incr' command"));
        }
        
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        match self.storage.incr(0, key) {
            Ok(new_value) => Ok(RespFrame::Integer(new_value)),
            Err(e) => Ok(RespFrame::error(e.to_string())),
        }
    }
    
    /// Handle DECR command
    fn handle_decr(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        if parts.len() != 2 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'decr' command"));
        }
        
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        match self.storage.incr_by(0, key, -1) {
            Ok(new_value) => Ok(RespFrame::Integer(new_value)),
            Err(e) => Ok(RespFrame::error(e.to_string())),
        }
    }
    
    /// Handle INCRBY command
    fn handle_incrby(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        if parts.len() != 3 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'incrby' command"));
        }
        
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        let increment = match &parts[2] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<i64>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid increment format")),
        };
        
        match self.storage.incr_by(0, key, increment) {
            Ok(new_value) => Ok(RespFrame::Integer(new_value)),
            Err(e) => Ok(RespFrame::error(e.to_string())),
        }
    }
    
    /// Handle DEL command
    fn handle_del(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        if parts.len() < 2 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'del' command"));
        }
        
        let mut deleted = 0;
        
        for i in 1..parts.len() {
            let key = match &parts[i] {
                RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                _ => continue, // Skip invalid keys
            };
            
            if self.storage.delete(0, key)? {
                deleted += 1;
                // Record change for auto-save
                self.record_change();
            }
        }
        
        Ok(RespFrame::Integer(deleted))
    }
    
    /// Handle EXISTS command
    fn handle_exists(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        if parts.len() < 2 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'exists' command"));
        }
        
        let mut count = 0;
        
        for i in 1..parts.len() {
            let key = match &parts[i] {
                RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                _ => continue, // Skip invalid keys
            };
            
            if self.storage.exists(0, key)? {
                count += 1;
            }
        }
        
        Ok(RespFrame::Integer(count))
    }
    
    /// Handle EXPIRE command
    fn handle_expire(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        if parts.len() != 3 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'expire' command"));
        }
        
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        let seconds = match &parts[2] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8_lossy(bytes).parse::<u64>() {
                    Ok(n) => n,
                    Err(_) => return Ok(RespFrame::error("ERR value is not an integer or out of range")),
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid seconds format")),
        };
        
        let result = self.storage.expire(0, key, Duration::from_secs(seconds))?;
        Ok(RespFrame::Integer(if result { 1 } else { 0 }))
    }
    
    /// Handle TTL command
    fn handle_ttl(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        if parts.len() != 2 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'ttl' command"));
        }
        
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        match self.storage.ttl(0, key)? {
            Some(duration) => {
                if duration.as_secs() == 0 && duration.subsec_millis() == 0 {
                    Ok(RespFrame::Integer(-2)) // Key expired
                } else {
                    Ok(RespFrame::Integer(duration.as_secs() as i64))
                }
            }
            None => {
                if self.storage.exists(0, key)? {
                    Ok(RespFrame::Integer(-1)) // Key exists but no expiration
                } else {
                    Ok(RespFrame::Integer(-2)) // Key doesn't exist
                }
            }
        }
    }
    
    /// Clean up closed connections
    fn cleanup_connections(&mut self) -> Result<()> {
        let mut to_remove = Vec::new();
        
        {
            let connections = self.connections.lock().unwrap();
            for (&id, conn) in connections.iter() {
                if conn.is_closing() {
                    to_remove.push(id);
                }
            }
        }
        
        if !to_remove.is_empty() {
            let mut connections = self.connections.lock().unwrap();
            for id in to_remove {
                if let Some(conn) = connections.remove(&id) {
                    println!("Client {} disconnected from {}", id, conn.addr);
                    
                    // Clean up any pub/sub subscriptions
                    if let Err(e) = self.pubsub.unsubscribe_all(id) {
                        eprintln!("Error cleaning up subscriptions for connection {}: {}", id, e);
                    }
                }
            }
        }
        
        Ok(())
    }
}