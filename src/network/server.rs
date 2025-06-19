//! Main server implementation

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::thread;
use crate::error::{FerrousError, Result};
use crate::protocol::RespFrame;
use crate::storage::{StorageEngine, GetResult, RdbEngine, StorageMonitor, RdbConfig};
use crate::storage::commands::{transactions, aof};
use crate::storage::{aof::AofEngine, AofConfig};
use crate::pubsub::{PubSubManager, format_message, format_pmessage, 
                    format_subscribe_response, format_psubscribe_response,
                    format_unsubscribe_response, format_punsubscribe_response};
use super::{Listener, Connection, ConnectionState, NetworkConfig};

/// Connection ID generator
static CONN_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Number of shards for connection storage
const CONNECTION_SHARDS: usize = 16;

/// Sharded connection storage for better concurrency
struct ShardedConnections {
    shards: Vec<Arc<Mutex<HashMap<u64, Connection>>>>,
}

impl ShardedConnections {
    fn new() -> Self {
        let mut shards = Vec::with_capacity(CONNECTION_SHARDS);
        for _ in 0..CONNECTION_SHARDS {
            shards.push(Arc::new(Mutex::new(HashMap::new())));
        }
        ShardedConnections { shards }
    }
    
    fn get_shard(&self, id: u64) -> &Arc<Mutex<HashMap<u64, Connection>>> {
        let shard_idx = (id as usize) % CONNECTION_SHARDS;
        &self.shards[shard_idx]
    }
    
    fn insert(&self, id: u64, connection: Connection) {
        let shard = self.get_shard(id);
        let mut connections = shard.lock().unwrap();
        connections.insert(id, connection);
    }
    
    fn remove(&self, id: u64) -> Option<Connection> {
        let shard = self.get_shard(id);
        let mut connections = shard.lock().unwrap();
        connections.remove(&id)
    }
    
    fn with_connection<F, R>(&self, id: u64, f: F) -> Option<R>
    where
        F: FnOnce(&mut Connection) -> R,
    {
        let shard = self.get_shard(id);
        
        // Try to acquire lock with a timeout to prevent deadlocks
        match shard.try_lock() {
            Ok(mut connections) => connections.get_mut(&id).map(f),
            Err(_) => {
                // If we can't get the lock immediately, use regular lock
                let mut connections = shard.lock().unwrap();
                connections.get_mut(&id).map(f)
            }
        }
    }
    
    fn total_connections(&self) -> usize {
        self.shards.iter()
            .filter_map(|shard| shard.try_lock().ok())
            .map(|connections| connections.len())
            .sum()
    }
    
    fn all_connection_ids(&self) -> Vec<u64> {
        let mut ids = Vec::new();
        for shard in &self.shards {
            if let Ok(connections) = shard.try_lock() {
                ids.extend(connections.keys().copied());
            }
            // Skip locked shards to prevent blocking
        }
        ids
    }
    
    /// Count active shards
    fn active_shards(&self) -> usize {
        self.shards.iter()
            .filter_map(|shard| shard.try_lock().ok())
            .filter(|connections| !connections.is_empty())
            .count()
    }
}

/// Server statistics for monitoring
pub struct ServerStats {
    /// Total number of connections received
    pub total_connections_received: AtomicU64,
    /// Total number of commands processed
    pub total_commands_processed: AtomicU64,
    /// Total number of keyspace hits
    pub keyspace_hits: AtomicU64,
    /// Total number of keyspace misses
    pub keyspace_misses: AtomicU64,
    /// Peak memory usage
    pub peak_memory: AtomicUsize,
    /// Number of blocked clients
    pub blocked_clients: AtomicU64,
    /// Total number of successful authentications
    pub auth_successes: AtomicU64,
    /// Total number of failed authentication attempts
    pub auth_failures: AtomicU64,
    /// Number of pending writes
    pub pending_writes: AtomicU64,
}

impl ServerStats {
    fn new() -> Self {
        Self {
            total_connections_received: AtomicU64::new(0),
            total_commands_processed: AtomicU64::new(0),
            keyspace_hits: AtomicU64::new(0),
            keyspace_misses: AtomicU64::new(0),
            peak_memory: AtomicUsize::new(0),
            blocked_clients: AtomicU64::new(0),
            auth_successes: AtomicU64::new(0),
            auth_failures: AtomicU64::new(0),
            pending_writes: AtomicU64::new(0),
        }
    }
}

/// Main server struct
pub struct Server {
    listener: Listener,
    connections: Arc<ShardedConnections>,
    config: NetworkConfig,
    /// Storage engine for Redis data
    storage: Arc<StorageEngine>,
    /// RDB persistence engine
    rdb_engine: Option<Arc<RdbEngine>>,
    /// Storage monitor for auto-save
    storage_monitor: Option<StorageMonitor>,
    /// Pub/sub manager
    pubsub: Arc<PubSubManager>,
    /// AOF persistence engine
    aof_engine: Option<Arc<AofEngine>>,
    /// Connections with pending writes
    pending_writes: Arc<Mutex<Vec<u64>>>,
    /// Server statistics
    stats: Arc<ServerStats>,
    /// Server start time
    start_time: SystemTime,
}

impl Server {
    /// Create a new server with default config
    pub fn new() -> Result<Self> {
        Self::with_config(NetworkConfig::default())
    }
    
    /// Create a new server with custom config
    pub fn with_config(config: NetworkConfig) -> Result<Self> {
        let listener = Listener::bind(config.clone())?;
        let connections = Arc::new(ShardedConnections::new());
        let storage = StorageEngine::new();
        
        // Create RDB engine with default config
        let rdb_config = RdbConfig::default();
        let rdb_engine = Arc::new(RdbEngine::new(rdb_config.clone()));
        
        // Create AOF engine with default config
        let aof_config = AofConfig::default();
        let aof_engine = if aof_config.enabled {
            let engine = Arc::new(AofEngine::new(aof_config));
            engine.init()?;
            engine.load(&storage)?;
            Some(engine)
        } else {
            None
        };
        
        // Create storage monitor
        let mut monitor = StorageMonitor::new();
        
        // Create pub/sub manager
        let pubsub = PubSubManager::new();
        
        // Create server stats
        let stats = Arc::new(ServerStats::new());
        
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
            aof_engine,
            pending_writes: Arc::new(Mutex::new(Vec::new())),
            stats,
            start_time: SystemTime::now(),
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
        
        // Track adaptive sleep intervals
        let mut cycles_without_work = 0;
        
        loop {
            let mut did_work = false;
            
            // Accept new connections with a limit per iteration
            for _ in 0..10 { // Process up to 10 new connections per iteration
                if self.accept_single_connection()? {
                    did_work = true;
                } else {
                    break; // No more pending connections
                }
            }
            
            // Process existing connections
            if self.process_connections()? {
                did_work = true;
            }
            
            // Process connections with pending writes
            if self.process_pending_writes()? {
                did_work = true;
            }
            
            // Clean up closed connections
            self.cleanup_connections()?;
            
            // Adaptive sleep to balance CPU usage and responsiveness
            if did_work {
                cycles_without_work = 0;
                // Yield to other threads without sleeping
                thread::yield_now();
            } else {
                cycles_without_work += 1;
                // Progressive backoff when idle
                let sleep_duration = match cycles_without_work {
                    0..=10 => Duration::from_micros(10),
                    11..=100 => Duration::from_micros(100),
                    _ => Duration::from_millis(1),
                };
                thread::sleep(sleep_duration);
            }
        }
    }
    
    /// Accept a single new connection
    /// Returns true if connection was accepted, false if would block
    fn accept_single_connection(&mut self) -> Result<bool> {
        if let Some((stream, addr)) = self.listener.accept()? {
            let id = CONN_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
            
            // Update statistics
            self.stats.total_connections_received.fetch_add(1, Ordering::Relaxed);
            
            // Check max clients limit
            if self.connections.total_connections() >= self.config.max_clients {
                println!("Max clients reached, rejecting connection from {}", addr);
                drop(stream); // Close connection
                return Ok(true);
            }
            
            // Create new connection
            match Connection::new(id, stream, addr) {
                Ok(mut conn) => {
                    // Set initial state based on auth requirement
                    if self.config.password.is_some() {
                        conn.state = ConnectionState::Connected; // Requires auth
                    } else {
                        conn.state = ConnectionState::Authenticated; // No auth required
                    }
                    
                    println!("Client {} connected from {}", id, addr);
                    self.connections.insert(id, conn);
                }
                Err(e) => {
                    eprintln!("Failed to create connection: {}", e);
                }
            }
            
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    /// Accept new connections
    fn accept_connections(&mut self) -> Result<()> {
        while self.accept_single_connection()? {}
        Ok(())
    }
    
    /// Process all connections
    /// Returns true if any work was done
    fn process_connections(&mut self) -> Result<bool> {
        let mut to_remove = Vec::new();
        let mut connections_with_writes = Vec::new();
        let mut did_work = false;
        
        // Get all connection IDs
        let conn_ids = self.connections.all_connection_ids();
        
        for id in conn_ids {
            // Process each connection
            match self.process_connection(id) {
                Ok(has_pending_writes) => {
                    did_work = true;
                    if has_pending_writes {
                        connections_with_writes.push(id);
                    }
                }
                Err(e) => {
                    // Error processing connection - mark for removal
                    eprintln!("Error processing connection {}: {}", id, e);
                    to_remove.push(id);
                }
            }
        }
        
        // Update pending writes list
        if !connections_with_writes.is_empty() {
            let mut pending = self.pending_writes.lock().unwrap();
            pending.extend(connections_with_writes);
            self.stats.pending_writes.store(pending.len() as u64, Ordering::Relaxed);
        }
        
        // Remove failed connections
        for id in to_remove {
            if let Some(conn) = self.connections.remove(id) {
                println!("Client {} disconnected from {}", id, conn.addr);
                
                // Clean up any pub/sub subscriptions
                if let Err(e) = self.pubsub.unsubscribe_all(id) {
                    eprintln!("Error cleaning up subscriptions for connection {}: {}", id, e);
                }
            }
        }
        
        Ok(did_work)
    }
    
    /// Process a single connection
    /// Returns Ok(true) if connection has pending writes
    fn process_connection(&mut self, id: u64) -> Result<bool> {
        // Read and parse frames without holding the lock during processing
        let mut frames_to_process = Vec::new();
        let mut should_close = false;
        let mut timeout_check = false;
        let mut conn_closed = false;
        
        // First phase: read and parse with the lock
        let read_result = self.connections.with_connection(id, |conn| -> Result<()> {
            // Try to flush any pending writes first to avoid buffer buildup
            if conn.has_pending_writes() {
                match conn.flush() {
                    Ok(_) => {},
                    Err(e) if matches!(e, FerrousError::Connection(_)) => {
                        conn_closed = true;
                        return Err(e);
                    }
                    Err(e) => return Err(e),
                }
            }
            
            // Read data from connection
            match conn.read() {
                Ok(true) => {
                    // Data was read, try to parse all available frames
                    loop {
                        match conn.parse_frame() {
                            Ok(Some(frame)) => frames_to_process.push(frame),
                            Ok(None) => break, // No more complete frames
                            Err(e) => {
                                conn.close()?;
                                return Err(e);
                            }
                        }
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
            
            Ok(())
        });
        
        if let Some(Err(e)) = read_result {
            if conn_closed {
                // Connection was closed, don't try to process further
                return Err(e);
            }
            return Err(e);
        }
        
        // Second phase: process frames without the lock
        let mut responses = Vec::new();
        for frame in frames_to_process {
            // Process each frame and increment command counter
            self.stats.total_commands_processed.fetch_add(1, Ordering::Relaxed);
            
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
        let has_pending_writes = self.connections.with_connection(id, |conn| -> Result<bool> {
            // Send all responses without flushing between them (for pipelining)
            for response in responses {
                conn.send_frame(&response)?;
            }
            
            // Now try to flush all at once
            match conn.flush() {
                Ok(_) => {},
                Err(e) => {
                    // Connection error during flush - don't abort processing for other connections
                    if matches!(e, FerrousError::Connection(_)) {
                        conn.state = ConnectionState::Closing;
                    }
                    return Err(e);
                }
            }
            
            // Handle QUIT command
            if should_close {
                conn.state = ConnectionState::Closing;
            }
            
            // Handle timeout
            if timeout_check {
                conn.close()?;
                return Err(FerrousError::Connection("Connection timed out".into()));
            }
            
            Ok(conn.has_pending_writes())
        }).unwrap_or(Ok(false))?;
        
        Ok(has_pending_writes)
    }
    
    /// Process connections with pending writes
    /// Returns true if any work was done
    fn process_pending_writes(&mut self) -> Result<bool> {
        let pending_ids: Vec<u64> = {
            let mut pending = self.pending_writes.lock().unwrap();
            let ids = std::mem::take(&mut *pending);
            self.stats.pending_writes.store(0, Ordering::Relaxed);
            ids
        };
        
        if pending_ids.is_empty() {
            return Ok(false);
        }
        
        let mut still_pending = Vec::new();
        let mut did_work = !pending_ids.is_empty();
        
        for id in &pending_ids {
            let result = self.connections.with_connection(*id, |conn| -> Result<bool> {
                match conn.flush() {
                    Ok(_) => Ok(conn.has_pending_writes()),
                    Err(e) if matches!(e, FerrousError::Connection(_)) => {
                        // Connection error - mark for closing
                        conn.state = ConnectionState::Closing;
                        Err(e)
                    }
                    Err(e) => Err(e),
                }
            });
            
            match result {
                Some(Ok(true)) => still_pending.push(*id), // Still has pending writes
                Some(Err(e)) => {
                    // Connection error - will be cleaned up in cleanup phase
                    eprintln!("Error flushing connection {}: {}", id, e);
                }
                _ => {} // Connection gone or all data flushed
            }
        }
        
        // Put back connections that still have pending writes
        if !still_pending.is_empty() {
            let mut pending = self.pending_writes.lock().unwrap();
            pending.extend(still_pending);
            self.stats.pending_writes.store(pending.len() as u64, Ordering::Relaxed);
        }
        
        Ok(did_work)
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
                
                // Get connection state
                let conn_state = self.connections.with_connection(conn_id, |conn| {
                    (conn.db_index, conn.transaction_state.in_transaction, conn.state)
                }).unwrap_or((0, false, ConnectionState::Closing));
                
                let (db_index, in_transaction, conn_status) = conn_state;
                
                // Check if authentication is required
                if self.config.password.is_some() && conn_status != ConnectionState::Authenticated {
                    // Only AUTH, PING and QUIT commands are allowed when not authenticated
                    match command.as_str() {
                        "AUTH" => return self.handle_auth(parts, conn_id),
                        "PING" => return self.handle_ping(parts), // Allow PING for monitoring
                        "QUIT" => return Ok(RespFrame::ok()),
                        _ => return Ok(RespFrame::error("NOAUTH Authentication required")),
                    }
                }
                
                // Handle transaction control commands and connection-specific commands
                match command.as_str() {
                    "MULTI" => {
                        return self.connections.with_connection(conn_id, |conn| {
                            transactions::handle_multi(conn)
                        }).unwrap_or_else(|| Ok(RespFrame::error("ERR connection not found")));
                    }
                    "EXEC" => {
                        return self.handle_exec(conn_id);
                    }
                    "DISCARD" => {
                        return self.connections.with_connection(conn_id, |conn| {
                            transactions::handle_discard(conn)
                        }).unwrap_or_else(|| Ok(RespFrame::error("ERR connection not found")));
                    }
                    "WATCH" => {
                        return self.connections.with_connection(conn_id, |conn| {
                            transactions::handle_watch(conn, parts)
                        }).unwrap_or_else(|| Ok(RespFrame::error("ERR connection not found")));
                    }
                    "UNWATCH" => {
                        return self.connections.with_connection(conn_id, |conn| {
                            transactions::handle_unwatch(conn)
                        }).unwrap_or_else(|| Ok(RespFrame::error("ERR connection not found")));
                    }
                    "PUBLISH" => return self.handle_publish(parts),
                    "SUBSCRIBE" => return self.handle_subscribe(parts, conn_id),
                    "UNSUBSCRIBE" => return self.handle_unsubscribe(parts, conn_id),
                    "PSUBSCRIBE" => return self.handle_psubscribe(parts, conn_id),
                    "PUNSUBSCRIBE" => return self.handle_punsubscribe(parts, conn_id),
                    "AUTH" => return self.handle_auth(parts, conn_id), // Handle AUTH after authentication too
                    _ => {}
                }
                
                // Check if we should queue the command
                if in_transaction && transactions::should_queue_command(&command) {
                    return self.connections.with_connection(conn_id, |conn| {
                        transactions::queue_command(conn, parts.to_vec())
                    }).unwrap_or_else(|| Ok(RespFrame::error("ERR connection not found")));
                }
                
                // Process normal command
                self.process_normal_command(parts, db_index)
            }
            _ => Ok(RespFrame::error("ERR invalid request format")),
        }
    }
    
    /// Handle EXEC command - execute queued transaction commands
    fn handle_exec(&mut self, conn_id: u64) -> Result<RespFrame> {
        // Get queued commands and clear transaction state
        let (commands, db_index, aborted) = {
            let result = self.connections.with_connection(conn_id, |conn| {
                if !conn.transaction_state.in_transaction {
                    return None;
                }
                
                let commands = std::mem::take(&mut conn.transaction_state.queued_commands);
                let db_index = conn.db_index;
                let aborted = conn.transaction_state.aborted;
                
                conn.transaction_state.in_transaction = false;
                conn.transaction_state.watched_keys.clear();
                conn.transaction_state.aborted = false;
                
                Some((commands, db_index, aborted))
            });
            
            match result {
                Some(Some(data)) => data,
                _ => return Ok(RespFrame::error("ERR EXEC without MULTI")),
            }
        };
        
        if aborted {
            return Ok(RespFrame::null_array());
        }
        
        // Execute all commands
        let mut results = Vec::new();
        for cmd_parts in commands {
            match self.process_command_parts(&cmd_parts, db_index) {
                Ok(response) => results.push(response),
                Err(e) => results.push(RespFrame::error(e.to_string())),
            }
        }
        
        Ok(RespFrame::Array(Some(results)))
    }
    
    /// Helper method to process a Vec<RespFrame> in a transaction
    fn process_command_parts(&mut self, parts: &Vec<RespFrame>, db: usize) -> Result<RespFrame> {
        self.process_normal_command(parts, db)
    }

    /// Process a normal (non-transaction) command
    fn process_normal_command(&mut self, parts: &[RespFrame], db: usize) -> Result<RespFrame> {
        // Extract command name
        let cmd_frame = &parts[0];
        let command = match cmd_frame {
            RespFrame::BulkString(Some(bytes)) => {
                String::from_utf8_lossy(bytes).to_uppercase()
            }
            _ => return Ok(RespFrame::error("ERR invalid command format")),
        };
        
        // Log to AOF for write commands
        if let Some(aof) = &self.aof_engine {
            if self.is_write_command(&command) {
                if let Err(e) = aof.append_command(parts) {
                    eprintln!("Failed to append to AOF: {}", e);
                }
            }
        }
        
        // Route to command handler
        match command.as_str() {
            "PING" => self.handle_ping(parts),
            "ECHO" => self.handle_echo(parts),
            "SET" => self.handle_set(parts, db),
            "GET" => self.handle_get(parts, db),
            "INCR" => self.handle_incr(parts, db),
            "DECR" => self.handle_decr(parts, db),
            "INCRBY" => self.handle_incrby(parts, db),
            "DEL" => self.handle_del(parts, db),
            "EXISTS" => self.handle_exists(parts, db),
            "EXPIRE" => self.handle_expire(parts, db),
            "TTL" => self.handle_ttl(parts, db),
            // Handle CONFIG command - key for benchmarking compatibility
            "CONFIG" => crate::storage::commands::config::handle_config(parts),
            // Additional string commands
            "MGET" => crate::storage::commands::strings::handle_mget(&self.storage, db, parts),
            "MSET" => crate::storage::commands::strings::handle_mset(&self.storage, db, parts),
            "GETSET" => crate::storage::commands::strings::handle_getset(&self.storage, db, parts),
            "APPEND" => crate::storage::commands::strings::handle_append(&self.storage, db, parts),
            "STRLEN" => crate::storage::commands::strings::handle_strlen(&self.storage, db, parts),
            "GETRANGE" => crate::storage::commands::strings::handle_getrange(&self.storage, db, parts),
            "SETRANGE" => crate::storage::commands::strings::handle_setrange(&self.storage, db, parts),
            "TYPE" => crate::storage::commands::strings::handle_type(&self.storage, db, parts),
            "RENAME" => crate::storage::commands::strings::handle_rename(&self.storage, db, parts),
            "KEYS" => crate::storage::commands::strings::handle_keys(&self.storage, db, parts),
            "PEXPIRE" => crate::storage::commands::strings::handle_pexpire(&self.storage, db, parts),
            "PTTL" => crate::storage::commands::strings::handle_pttl(&self.storage, db, parts),
            "PERSIST" => crate::storage::commands::strings::handle_persist(&self.storage, db, parts),
            // List commands
            "LPUSH" => crate::storage::commands::lists::handle_lpush(&self.storage, db, parts),
            "RPUSH" => crate::storage::commands::lists::handle_rpush(&self.storage, db, parts),
            "LPOP" => crate::storage::commands::lists::handle_lpop(&self.storage, db, parts),
            "RPOP" => crate::storage::commands::lists::handle_rpop(&self.storage, db, parts),
            "LLEN" => crate::storage::commands::lists::handle_llen(&self.storage, db, parts),
            "LRANGE" => crate::storage::commands::lists::handle_lrange(&self.storage, db, parts),
            "LINDEX" => crate::storage::commands::lists::handle_lindex(&self.storage, db, parts),
            "LSET" => crate::storage::commands::lists::handle_lset(&self.storage, db, parts),
            "LTRIM" => crate::storage::commands::lists::handle_ltrim(&self.storage, db, parts),
            "LREM" => crate::storage::commands::lists::handle_lrem(&self.storage, db, parts),
            // Set commands
            "SADD" => crate::storage::commands::sets::handle_sadd(&self.storage, db, parts),
            "SREM" => crate::storage::commands::sets::handle_srem(&self.storage, db, parts),
            "SMEMBERS" => crate::storage::commands::sets::handle_smembers(&self.storage, db, parts),
            "SISMEMBER" => crate::storage::commands::sets::handle_sismember(&self.storage, db, parts),
            "SCARD" => crate::storage::commands::sets::handle_scard(&self.storage, db, parts),
            "SUNION" => crate::storage::commands::sets::handle_sunion(&self.storage, db, parts),
            "SINTER" => crate::storage::commands::sets::handle_sinter(&self.storage, db, parts),
            "SDIFF" => crate::storage::commands::sets::handle_sdiff(&self.storage, db, parts),
            "SRANDMEMBER" => crate::storage::commands::sets::handle_srandmember(&self.storage, db, parts),
            "SPOP" => crate::storage::commands::sets::handle_spop(&self.storage, db, parts),
            // Hash commands
            "HSET" => crate::storage::commands::hashes::handle_hset(&self.storage, db, parts),
            "HGET" => crate::storage::commands::hashes::handle_hget(&self.storage, db, parts),
            "HMSET" => crate::storage::commands::hashes::handle_hmset(&self.storage, db, parts),
            "HMGET" => crate::storage::commands::hashes::handle_hmget(&self.storage, db, parts),
            "HGETALL" => crate::storage::commands::hashes::handle_hgetall(&self.storage, db, parts),
            "HDEL" => crate::storage::commands::hashes::handle_hdel(&self.storage, db, parts),
            "HLEN" => crate::storage::commands::hashes::handle_hlen(&self.storage, db, parts),
            "HEXISTS" => crate::storage::commands::hashes::handle_hexists(&self.storage, db, parts),
            "HKEYS" => crate::storage::commands::hashes::handle_hkeys(&self.storage, db, parts),
            "HVALS" => crate::storage::commands::hashes::handle_hvals(&self.storage, db, parts),
            "HINCRBY" => crate::storage::commands::hashes::handle_hincrby(&self.storage, db, parts),
            // Sorted set commands
            "ZADD" => self.handle_zadd(parts, db),
            "ZREM" => self.handle_zrem(parts, db),
            "ZSCORE" => self.handle_zscore(parts, db),
            "ZRANK" => self.handle_zrank(parts, db),
            "ZREVRANK" => self.handle_zrevrank(parts, db),
            "ZRANGE" => self.handle_zrange(parts, db),
            "ZREVRANGE" => self.handle_zrevrange(parts, db),
            "ZRANGEBYSCORE" => self.handle_zrangebyscore(parts, db),
            "ZREVRANGEBYSCORE" => self.handle_zrevrangebyscore(parts, db),
            "ZCOUNT" => self.handle_zcount(parts, db),
            "ZINCRBY" => self.handle_zincrby(parts, db),
            // RDB commands
            "SAVE" => self.handle_save(parts),
            "BGSAVE" => self.handle_bgsave(parts),
            "LASTSAVE" => self.handle_lastsave(parts),
            // AOF commands
            "BGREWRITEAOF" => aof::handle_bgrewriteaof(self.aof_engine.as_ref()),
            // Monitoring commands
            "INFO" => crate::storage::commands::monitor::handle_info(
                &self.storage,
                &self.stats,
                self.start_time,
                self.connections.total_connections(),
                self.config.max_clients,
                parts
            ),
            // Auth command  
            "AUTH" => self.handle_auth(parts, 0), // Special handling for AUTH in process_frame
            "QUIT" => Ok(RespFrame::ok()),
            _ => Ok(RespFrame::error(format!("ERR unknown command '{}'", command))),
        }
    }
    
    /// Handle AUTH command
    fn handle_auth(&self, parts: &[RespFrame], conn_id: u64) -> Result<RespFrame> {
        // AUTH password
        if parts.len() != 2 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'auth' command"));
        }
        
        // Extract password
        let provided_password = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => {
                match String::from_utf8(bytes.to_vec()) {
                    Ok(s) => s,
                    Err(_) => return Ok(RespFrame::error("ERR invalid password format")),
                }
            }
            _ => return Ok(RespFrame::error("ERR invalid password format")),
        };
        
        // Check if server requires authentication
        match &self.config.password {
            Some(server_password) => {
                if provided_password == *server_password {
                    // Authentication successful
                    self.stats.auth_successes.fetch_add(1, Ordering::Relaxed);
                    
                    // Update connection state
                    self.connections.with_connection(conn_id, |conn| {
                        conn.state = ConnectionState::Authenticated;
                    });
                    
                    Ok(RespFrame::ok())
                } else {
                    // Authentication failed
                    self.stats.auth_failures.fetch_add(1, Ordering::Relaxed);
                    Ok(RespFrame::error("ERR invalid password"))
                }
            }
            None => {
                // No password set on server
                Ok(RespFrame::error("ERR Client sent AUTH, but no password is set"))
            }
        }
    }
    
    /// Check if a command is a write command that should be logged to AOF
    fn is_write_command(&self, command: &str) -> bool {
        matches!(command,
            "SET" | "DEL" | "EXPIRE" | "INCR" | "DECR" | "INCRBY" |
            "LPUSH" | "RPUSH" | "LPOP" | "RPOP" | "LSET" | "LREM" | "LTRIM" |
            "SADD" | "SREM" | "SPOP" | 
            "HSET" | "HDEL" | "HINCRBY" |
            "ZADD" | "ZREM" | "ZINCRBY" |
            "MSET" | "APPEND" | "SETRANGE" | "RENAME" | "PERSIST"
        )
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
            for (conn_id, pattern) in receivers {
                let frame = if let Some(pat) = pattern {
                    format_pmessage(&pat, channel, message)
                } else {
                    format_message(channel, message)
                };
                
                // Best effort delivery - ignore errors
                let _ = self.connections.with_connection(conn_id, |conn| {
                    conn.send_frame(&frame)
                });
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
    fn handle_zadd(&self, parts: &[RespFrame], db: usize) -> Result<RespFrame> {
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
            if self.storage.zadd(db, key.clone(), member, score)? {
                new_members += 1;
                // Record change for auto-save
                self.record_change();
            }
        }
        
        Ok(RespFrame::Integer(new_members))
    }
    
    /// Handle ZREM command
    fn handle_zrem(&self, parts: &[RespFrame], db: usize) -> Result<RespFrame> {
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
            if self.storage.zrem(db, key, member)? {
                removed += 1;
            }
        }
        
        Ok(RespFrame::Integer(removed))
    }
    
    /// Handle ZSCORE command
    fn handle_zscore(&self, parts: &[RespFrame], db: usize) -> Result<RespFrame> {
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
        match self.storage.zscore(db, key, member)? {
            Some(score) => {
                // Convert f64 to string with Redis protocol formatting
                let score_str = format!("{}", score);
                Ok(RespFrame::from_string(score_str))
            }
            None => Ok(RespFrame::null_bulk()), // Member not found or key doesn't exist
        }
    }
    
    /// Handle ZRANK command
    fn handle_zrank(&self, parts: &[RespFrame], db: usize) -> Result<RespFrame> {
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
        match self.storage.zrank(db, key, member, false)? {
            Some(rank) => Ok(RespFrame::Integer(rank as i64)),
            None => Ok(RespFrame::null_bulk()), // Member not found or key doesn't exist
        }
    }
    
    /// Handle ZREVRANK command
    fn handle_zrevrank(&self, parts: &[RespFrame], db: usize) -> Result<RespFrame> {
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
        match self.storage.zrank(db, key, member, true)? {
            Some(rank) => Ok(RespFrame::Integer(rank as i64)),
            None => Ok(RespFrame::null_bulk()), // Member not found or key doesn't exist
        }
    }
    
    /// Handle ZRANGE command
    fn handle_zrange(&self, parts: &[RespFrame], db: usize) -> Result<RespFrame> {
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
        let members = self.storage.zrange(db, key, start, stop, false)?;
        
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
    fn handle_zrevrange(&self, parts: &[RespFrame], db: usize) -> Result<RespFrame> {
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
        let members = self.storage.zrange(db, key, start, stop, true)?;
        
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
    fn handle_zrangebyscore(&self, parts: &[RespFrame], db: usize) -> Result<RespFrame> {
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
        let members = self.storage.zrangebyscore(db, key, min_score, max_score, false)?;
        
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
    fn handle_zrevrangebyscore(&self, parts: &[RespFrame], db: usize) -> Result<RespFrame> {
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
        let members = self.storage.zrangebyscore(db, key, min_score, max_score, true)?;
        
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
    fn handle_zcount(&self, parts: &[RespFrame], db: usize) -> Result<RespFrame> {
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
                    Ok(n)

 => n,
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
        let count = self.storage.zcount(db, key, min_score, max_score)?;
        
        Ok(RespFrame::Integer(count as i64))
    }
    
    /// Handle ZINCRBY command
    fn handle_zincrby(&self, parts: &[RespFrame], db: usize) -> Result<RespFrame> {
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
        let new_score = self.storage.zincrby(db, key, member, increment)?;
        
        // Return new score as bulk string (Redis protocol format)
        Ok(RespFrame::from_string(new_score.to_string()))
    }
    
    /// Handle PING command
    /// This implementation has been enhanced for better compatibility with
    /// redis-benchmark and other Redis clients.
    fn handle_ping(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        // If PING has an argument, return that argument
        if parts.len() > 1 {
            return Ok(parts[1].clone());
        }
        
        // Otherwise return PONG
        // Note: We use SimpleString instead of BulkString for better compatibility
        // with redis-benchmark and other clients that may expect this format
        Ok(RespFrame::SimpleString(Arc::new(b"PONG".to_vec())))
    }
    
    /// Handle ECHO command
    fn handle_echo(&self, parts: &[RespFrame]) -> Result<RespFrame> {
        if parts.len() != 2 {
            Ok(RespFrame::error("ERR wrong number of arguments for 'echo' command"))
        } else {
            Ok(parts[1].clone())
        }
    }
    
    /// Handle SET command
    fn handle_set(&self, parts: &[RespFrame], db: usize) -> Result<RespFrame> {
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
            if self.storage.exists(db, &key)? {
                return Ok(RespFrame::null_bulk());
            }
        }
        
        // Check XX condition
        if xx {
            if !self.storage.exists(db, &key)? {
                return Ok(RespFrame::null_bulk());
            }
        }
        
        // Set the value
        match expiration {
            Some(expires_in) => {
                self.storage.set_string_ex(db, key, value, expires_in)?;
            }
            None => {
                self.storage.set_string(db, key, value)?;
            }
        }
        
        // Record change for auto-save
        self.record_change();
        
        Ok(RespFrame::ok())
    }
    
    /// Handle GET command
    fn handle_get(&self, parts: &[RespFrame], db: usize) -> Result<RespFrame> {
        if parts.len() != 2 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'get' command"));
        }
        
        // Extract key
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        // Get the value
        match self.storage.get_string(db, key)? {
            Some(value) => {
                // Record a cache hit
                self.stats.keyspace_hits.fetch_add(1, Ordering::Relaxed);
                Ok(RespFrame::from_bytes(value))
            }
            None => {
                // Record a cache miss
                self.stats.keyspace_misses.fetch_add(1, Ordering::Relaxed);
                Ok(RespFrame::null_bulk())
            }
        }
    }
    
    /// Handle INCR command
    fn handle_incr(&self, parts: &[RespFrame], db: usize) -> Result<RespFrame> {
        if parts.len() != 2 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'incr' command"));
        }
        
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        match self.storage.incr(db, key) {
            Ok(new_value) => Ok(RespFrame::Integer(new_value)),
            Err(e) => Ok(RespFrame::error(e.to_string())),
        }
    }
    
    /// Handle DECR command
    fn handle_decr(&self, parts: &[RespFrame], db: usize) -> Result<RespFrame> {
        if parts.len() != 2 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'decr' command"));
        }
        
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref().clone(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        match self.storage.incr_by(db, key, -1) {
            Ok(new_value) => Ok(RespFrame::Integer(new_value)),
            Err(e) => Ok(RespFrame::error(e.to_string())),
        }
    }
    
    /// Handle INCRBY command
    fn handle_incrby(&self, parts: &[RespFrame], db: usize) -> Result<RespFrame> {
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
        
        match self.storage.incr_by(db, key, increment) {
            Ok(new_value) => Ok(RespFrame::Integer(new_value)),
            Err(e) => Ok(RespFrame::error(e.to_string())),
        }
    }
    
    /// Handle DEL command
    fn handle_del(&self, parts: &[RespFrame], db: usize) -> Result<RespFrame> {
        if parts.len() < 2 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'del' command"));
        }
        
        let mut deleted = 0;
        
        for i in 1..parts.len() {
            let key = match &parts[i] {
                RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                _ => continue, // Skip invalid keys
            };
            
            if self.storage.delete(db, key)? {
                deleted += 1;
                // Record change for auto-save
                self.record_change();
            }
        }
        
        Ok(RespFrame::Integer(deleted))
    }
    
    /// Handle EXISTS command
    fn handle_exists(&self, parts: &[RespFrame], db: usize) -> Result<RespFrame> {
        if parts.len() < 2 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'exists' command"));
        }
        
        let mut count = 0;
        
        for i in 1..parts.len() {
            let key = match &parts[i] {
                RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
                _ => continue, // Skip invalid keys
            };
            
            if self.storage.exists(db, key)? {
                count += 1;
                // Record a cache hit
                self.stats.keyspace_hits.fetch_add(1, Ordering::Relaxed);
            } else {
                // Record a cache miss
                self.stats.keyspace_misses.fetch_add(1, Ordering::Relaxed);
            }
        }
        
        Ok(RespFrame::Integer(count))
    }
    
    /// Handle EXPIRE command
    fn handle_expire(&self, parts: &[RespFrame], db: usize) -> Result<RespFrame> {
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
        
        let result = self.storage.expire(db, key, Duration::from_secs(seconds))?;
        Ok(RespFrame::Integer(if result { 1 } else { 0 }))
    }
    
    /// Handle TTL command
    fn handle_ttl(&self, parts: &[RespFrame], db: usize) -> Result<RespFrame> {
        if parts.len() != 2 {
            return Ok(RespFrame::error("ERR wrong number of arguments for 'ttl' command"));
        }
        
        let key = match &parts[1] {
            RespFrame::BulkString(Some(bytes)) => bytes.as_ref(),
            _ => return Ok(RespFrame::error("ERR invalid key format")),
        };
        
        match self.storage.ttl(db, key)? {
            Some(duration) => {
                if duration.as_secs() == 0 && duration.subsec_millis() == 0 {
                    Ok(RespFrame::Integer(-2)) // Key expired
                } else {
                    Ok(RespFrame::Integer(duration.as_secs() as i64))
                }
            }
            None => {
                if self.storage.exists(db, key)? {
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
        
        // Check all connections for closing state
        for id in self.connections.all_connection_ids() {
            let should_remove = self.connections.with_connection(id, |conn| {
                conn.is_closing()
            }).unwrap_or(false);
            
            if should_remove {
                to_remove.push(id);
            }
        }
        
        // Remove closed connections
        for id in to_remove {
            if let Some(conn) = self.connections.remove(id) {
                println!("Client {} disconnected from {}", id, conn.addr);
                
                // Clean up any pub/sub subscriptions
                if let Err(e) = self.pubsub.unsubscribe_all(id) {
                    eprintln!("Error cleaning up subscriptions for connection {}: {}", id, e);
                }
            }
        }
        
        Ok(())
    }
}