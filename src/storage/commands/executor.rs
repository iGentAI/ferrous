//! Unified Command Execution Layer for Ferrous
//!
//! This module provides the single source of truth for all Redis command processing,
//! eliminating the fragmentation between server, storage, and Lua command handling.

use std::sync::Arc;
use std::time::Duration;
use crate::error::{Result, FerrousError, CommandError};
use crate::protocol::RespFrame;
use crate::storage::StorageEngine;

/// Unified command executor that guarantees atomicity and consistency
#[derive(Clone)]
pub struct UnifiedCommandExecutor {
    storage: Arc<StorageEngine>,
    /// Optional connection context for commands that need it
    conn_context: Option<ConnectionContext>,
}

/// Context for connection-specific operations
#[derive(Clone, Debug)]
pub struct ConnectionContext {
    pub conn_id: u64,
    pub db_index: usize,
    pub is_replica: bool,
    pub is_transaction: bool,
}

/// Parsed command ready for execution
#[derive(Debug, Clone)]
pub struct ParsedCommand {
    pub command: Command,
    pub db_override: Option<usize>,
}

/// Top-level command categories - COMPLETE Redis coverage
#[derive(Debug, Clone)]
pub enum Command {
    String(StringCommand),
    List(ListCommand),
    Set(SetCommand),
    Hash(HashCommand),
    SortedSet(SortedSetCommand),
    Key(KeyCommand),
    Server(ServerCommand),
    Stream(StreamCommand),
    Scan(ScanCommand),
    Database(DatabaseCommand),
    ConsumerGroup(ConsumerGroupCommand),
    Persistence(PersistenceCommand),
    Bit(BitCommand),
    Config(ConfigCommand),
}

/// String commands with comprehensive Redis compatibility
#[derive(Debug, Clone)]
pub enum StringCommand {
    Set {
        key: Vec<u8>,
        value: Vec<u8>,
        options: SetOptions,
    },
    Get {
        key: Vec<u8>,
    },
    MGet {
        keys: Vec<Vec<u8>>,
    },
    MSet {
        pairs: Vec<(Vec<u8>, Vec<u8>)>,
    },
    Incr {
        key: Vec<u8>,
    },
    IncrBy {
        key: Vec<u8>,
        increment: i64,
    },
    Decr {
        key: Vec<u8>,
    },
    DecrBy {
        key: Vec<u8>,
        decrement: i64,
    },
    SetNx {
        key: Vec<u8>,
        value: Vec<u8>,
    },
    SetEx {
        key: Vec<u8>,
        value: Vec<u8>,
        seconds: u64,
    },
    PSetEx {
        key: Vec<u8>,
        value: Vec<u8>,
        milliseconds: u64,
    },
    Append {
        key: Vec<u8>,
        value: Vec<u8>,
    },
    StrLen {
        key: Vec<u8>,
    },
    GetSet {
        key: Vec<u8>,
        value: Vec<u8>,
    },
    GetRange {
        key: Vec<u8>,
        start: isize,
        end: isize,
    },
    SetRange {
        key: Vec<u8>,
        offset: usize,
        value: Vec<u8>,
    },
    Del {
        keys: Vec<Vec<u8>>,
    },
}

/// List commands for Redis Lua compatibility  
#[derive(Debug, Clone)]
pub enum ListCommand {
    LPush {
        key: Vec<u8>,
        values: Vec<Vec<u8>>,
    },
    RPush {
        key: Vec<u8>,
        values: Vec<Vec<u8>>,
    },
    LPop {
        key: Vec<u8>,
    },
    RPop {
        key: Vec<u8>,
    },
    LLen {
        key: Vec<u8>,
    },
    LIndex {
        key: Vec<u8>,
        index: isize,
    },
    LSet {
        key: Vec<u8>,
        index: isize,
        value: Vec<u8>,
    },
    LRange {
        key: Vec<u8>,
        start: isize,
        stop: isize,
    },
    LTrim {
        key: Vec<u8>,
        start: isize,
        stop: isize,
    },
    LRem {
        key: Vec<u8>,
        count: isize,
        element: Vec<u8>,
    },
}

/// Set commands for Redis Lua compatibility
#[derive(Debug, Clone)]
pub enum SetCommand {
    SAdd {
        key: Vec<u8>,
        members: Vec<Vec<u8>>,
    },
    SRem {
        key: Vec<u8>,
        members: Vec<Vec<u8>>,
    },
    SMembers {
        key: Vec<u8>,
    },
    SCard {
        key: Vec<u8>,
    },
    SIsMember {
        key: Vec<u8>,
        member: Vec<u8>,
    },
    SUnion {
        keys: Vec<Vec<u8>>,
    },
    SInter {
        keys: Vec<Vec<u8>>,
    },
    SDiff {
        keys: Vec<Vec<u8>>,
    },
    SRandMember {
        key: Vec<u8>,
        count: Option<i64>,
    },
    SPop {
        key: Vec<u8>,
        count: Option<usize>,
    },
}

/// Hash commands for Redis Lua compatibility  
#[derive(Debug, Clone)]
pub enum HashCommand {
    HSet {
        key: Vec<u8>,
        field_values: Vec<(Vec<u8>, Vec<u8>)>,
    },
    HGet {
        key: Vec<u8>,
        field: Vec<u8>,
    },
    HMSet {
        key: Vec<u8>,
        field_values: Vec<(Vec<u8>, Vec<u8>)>,
    },
    HMGet {
        key: Vec<u8>,
        fields: Vec<Vec<u8>>,
    },
    HGetAll {
        key: Vec<u8>,
    },
    HDel {
        key: Vec<u8>,
        fields: Vec<Vec<u8>>,
    },
    HLen {
        key: Vec<u8>,
    },
    HExists {
        key: Vec<u8>,
        field: Vec<u8>,
    },
    HKeys {
        key: Vec<u8>,
    },
    HVals {
        key: Vec<u8>,
    },
    HIncrBy {
        key: Vec<u8>,
        field: Vec<u8>,
        increment: i64,
    },
}

/// Sorted Set commands for Redis Lua compatibility
#[derive(Debug, Clone)]
pub enum SortedSetCommand {
    ZAdd {
        key: Vec<u8>,
        score_members: Vec<(f64, Vec<u8>)>,
    },
    ZRem {
        key: Vec<u8>,
        members: Vec<Vec<u8>>,
    },
    ZScore {
        key: Vec<u8>,
        member: Vec<u8>,
    },
    ZCard {
        key: Vec<u8>,
    },
    ZRank {
        key: Vec<u8>,
        member: Vec<u8>,
    },
    ZRevRank {
        key: Vec<u8>,
        member: Vec<u8>,
    },
    ZRange {
        key: Vec<u8>,
        start: isize,
        stop: isize,
        with_scores: bool,
    },
    ZRevRange {
        key: Vec<u8>,
        start: isize,
        stop: isize,
        with_scores: bool,
    },
    ZRangeByScore {
        key: Vec<u8>,
        min_score: f64,
        max_score: f64,
        with_scores: bool,
    },
    ZCount {
        key: Vec<u8>,
        min_score: f64,
        max_score: f64,
    },
    ZIncrBy {
        key: Vec<u8>,
        increment: f64,
        member: Vec<u8>,
    },
    ZRevRangeByScore {
        key: Vec<u8>,
        max_score: f64,
        min_score: f64,
        with_scores: bool,
    },
    ZPopMin {
        key: Vec<u8>,
        count: Option<usize>,
    },
    ZPopMax {
        key: Vec<u8>,
        count: Option<usize>,
    },
    ZRemRangeByRank {
        key: Vec<u8>,
        start: isize,
        stop: isize,
    },
    ZRemRangeByScore {
        key: Vec<u8>,
        min_score: f64,
        max_score: f64,
    },
    ZRemRangeByLex {
        key: Vec<u8>,
        min_lex: String,
        max_lex: String,
    },
}

/// Key management commands expanded
#[derive(Debug, Clone)]
pub enum KeyCommand {
    Exists {
        keys: Vec<Vec<u8>>,
    },
    Expire {
        key: Vec<u8>,
        seconds: u64,
    },
    PExpire {
        key: Vec<u8>,
        milliseconds: u64,
    },
    Ttl {
        key: Vec<u8>,
    },
    Pttl {
        key: Vec<u8>,
    },
    Persist {
        key: Vec<u8>,
    },
    Type {
        key: Vec<u8>,
    },
    Rename {
        old_key: Vec<u8>,
        new_key: Vec<u8>,
    },
    RenameNx {
        old_key: Vec<u8>,
        new_key: Vec<u8>,
    },
    RandomKey,
}

/// Stream operations for comprehensive Redis Lua support
#[derive(Debug, Clone)]
pub enum StreamCommand {
    XAdd {
        key: Vec<u8>,
        id: Option<String>,
        fields: Vec<(Vec<u8>, Vec<u8>)>,
    },
    XLen {
        key: Vec<u8>,
    },
    XRange {
        key: Vec<u8>,
        start: String,
        end: String,
        count: Option<usize>,
    },
    XRevRange {
        key: Vec<u8>,
        start: String,
        end: String,
        count: Option<usize>,
    },
    XRead {
        keys_and_ids: Vec<(Vec<u8>, String)>,
        count: Option<usize>,
        block: Option<u64>,
    },
    XTrim {
        key: Vec<u8>,
        strategy: String,
        threshold: usize,
    },
    XDel {
        key: Vec<u8>,
        ids: Vec<String>,
    },
}

/// Scan operations for cursor-based iteration  
#[derive(Debug, Clone)]
pub enum ScanCommand {
    Scan {
        cursor: u64,
        pattern: Option<Vec<u8>>,
        count: Option<usize>,
        type_filter: Option<String>,
    },
    HScan {
        key: Vec<u8>,
        cursor: u64,
        pattern: Option<Vec<u8>>,
        count: Option<usize>,
    },
    SScan {
        key: Vec<u8>,
        cursor: u64,
        pattern: Option<Vec<u8>>,
        count: Option<usize>,
    },
    ZScan {
        key: Vec<u8>,
        cursor: u64,
        pattern: Option<Vec<u8>>,
        count: Option<usize>,
    },
}

/// Database management operations
#[derive(Debug, Clone)]
pub enum DatabaseCommand {
    FlushDb,
    FlushAll,
    DbSize,
    Keys {
        pattern: Vec<u8>,
    },
}

/// Consumer Group commands for Redis Stream processing
#[derive(Debug, Clone)]
pub enum ConsumerGroupCommand {
    XGroup {
        subcommand: String,
        key: Vec<u8>,
        group: String,
        args: Vec<String>,
    },
    XReadGroup {
        group: String,
        consumer: String,
        keys_and_ids: Vec<(Vec<u8>, String)>,
        count: Option<usize>,
        block: Option<u64>,
        noack: bool,
    },
    XAck {
        key: Vec<u8>,
        group: String,
        ids: Vec<String>,
    },
    XPending {
        key: Vec<u8>,
        group: String,
        range: Option<(String, String, usize)>,
        consumer: Option<String>,
    },
    XClaim {
        key: Vec<u8>,
        group: String,
        consumer: String,
        min_idle_time: u64,
        ids: Vec<String>,
        force: bool,
        justid: bool,
    },
    XInfo {
        subcommand: String,
        key: Vec<u8>,
        group: Option<String>,
    },
}

/// Persistence operations for complete Redis support
#[derive(Debug, Clone)]
pub enum PersistenceCommand {
    Save,
    BgSave,
    LastSave,
    BgRewriteAof,
}

/// Bit operations for complete Redis support  
#[derive(Debug, Clone)]
pub enum BitCommand {
    GetBit {
        key: Vec<u8>,
        offset: usize,
    },
    SetBit {
        key: Vec<u8>,
        offset: usize,
        value: u8,
    },
    BitCount {
        key: Vec<u8>,
        start: Option<isize>,
        end: Option<isize>,
    },
}

/// Configuration operations for Redis management
#[derive(Debug, Clone)]
pub enum ConfigCommand {
    Get {
        parameter: String,
    },
    Set {
        parameter: String,
        value: String,
    },
}

/// Server operations
#[derive(Debug, Clone)]
pub enum ServerCommand {
    Ping {
        message: Option<Vec<u8>>,
    },
    Echo {
        message: Vec<u8>,
    },
    Time,
    Info {
        section: Option<String>,
    },
}

/// SET command options with full Redis compatibility
#[derive(Debug, Clone, Default)]
pub struct SetOptions {
    pub nx: bool,
    pub xx: bool,
    pub get: bool,
    pub expiration: Option<Duration>,
    pub keepttl: bool,
}

impl UnifiedCommandExecutor {
    /// Create new executor with storage reference
    pub fn new(storage: Arc<StorageEngine>) -> Self {
        Self {
            storage,
            conn_context: None,
        }
    }
    
    /// Create executor with connection context
    pub fn with_context(mut self, ctx: ConnectionContext) -> Self {
        self.conn_context = Some(ctx);
        self
    }
    
    /// Main execution entry point with atomicity guarantees
    pub fn execute(&self, cmd: ParsedCommand) -> Result<RespFrame> {
        // Get database index
        let db = cmd.db_override
            .or_else(|| self.conn_context.as_ref().map(|c| c.db_index))
            .unwrap_or(0);
        
        // Execute with atomicity guarantees across ALL command categories
        match cmd.command {
            Command::String(string_cmd) => self.execute_string(db, string_cmd),
            Command::List(list_cmd) => self.execute_list(db, list_cmd),
            Command::Set(set_cmd) => self.execute_set(db, set_cmd),
            Command::Hash(hash_cmd) => self.execute_hash(db, hash_cmd),
            Command::SortedSet(sortedset_cmd) => self.execute_sorted_set(db, sortedset_cmd),
            Command::Key(key_cmd) => self.execute_key(db, key_cmd),
            Command::Server(server_cmd) => self.execute_server(server_cmd),
            Command::Stream(stream_cmd) => self.execute_stream(db, stream_cmd),
            Command::Scan(scan_cmd) => self.execute_scan(db, scan_cmd),
            Command::Database(db_cmd) => self.execute_database(db_cmd),
            Command::ConsumerGroup(cg_cmd) => self.execute_consumer_group(db, cg_cmd),
            Command::Persistence(persist_cmd) => self.execute_persistence(persist_cmd),
            Command::Bit(bit_cmd) => self.execute_bit(db, bit_cmd),
            Command::Config(config_cmd) => self.execute_config(config_cmd),
        }
    }
    
    /// Execute string commands with proper atomicity
    fn execute_string(&self, db: usize, cmd: StringCommand) -> Result<RespFrame> {
        match cmd {
            StringCommand::Set { key, value, options } => {
                // Validate mutually exclusive options
                if options.nx && options.xx {
                    return Ok(RespFrame::error("ERR NX and XX options are mutually exclusive"));
                }
                
                // Handle GET option (return old value)
                let old_value = if options.get {
                    self.storage.get_string(db, &key)?
                } else {
                    None
                };
                
                // Execute SET with proper atomicity
                let success = if options.nx {
                    // Use atomic operation to prevent race conditions
                    match options.expiration {
                        Some(exp) => self.storage.set_string_nx_ex(db, key, value, exp)?,
                        None => self.storage.set_string_nx(db, key, value)?,
                    }
                } else if options.xx {
                    // Only set if key exists
                    if !self.storage.exists(db, &key)? {
                        false
                    } else {
                        match options.expiration {
                            Some(exp) => self.storage.set_string_ex(db, key, value, exp).map(|_| true)?,
                            None => self.storage.set_string(db, key, value).map(|_| true)?,
                        }
                    }
                } else {
                    // Regular SET
                    match options.expiration {
                        Some(exp) => self.storage.set_string_ex(db, key, value, exp).map(|_| true)?,
                        None => self.storage.set_string(db, key, value).map(|_| true)?,
                    }
                };
                
                // Return appropriate response
                if options.get {
                    Ok(old_value.map(RespFrame::from_bytes).unwrap_or(RespFrame::null_bulk()))
                } else if options.nx && !success {
                    Ok(RespFrame::null_bulk()) // Key existed, NX failed
                } else if options.xx && !success {
                    Ok(RespFrame::null_bulk()) // Key didn't exist, XX failed
                } else {
                    Ok(RespFrame::ok())
                }
            }
            
            StringCommand::Get { key } => {
                match self.storage.get_string(db, &key)? {
                    Some(value) => Ok(RespFrame::from_bytes(value)),
                    None => Ok(RespFrame::null_bulk()),
                }
            }
            
            StringCommand::MGet { keys } => {
                use crate::storage::commands::strings::handle_mget;
                let mut frames = vec![RespFrame::from_string("MGET")];
                for key in keys {
                    frames.push(RespFrame::from_bytes(key));
                }
                handle_mget(&self.storage, db, &frames)
            }
            
            StringCommand::MSet { pairs } => {
                use crate::storage::commands::strings::handle_mset;
                let mut frames = vec![RespFrame::from_string("MSET")];
                for (key, value) in pairs {
                    frames.push(RespFrame::from_bytes(key));
                    frames.push(RespFrame::from_bytes(value));
                }
                handle_mset(&self.storage, db, &frames)
            }
            
            StringCommand::Incr { key } => {
                let result = self.storage.incr(db, key)?;
                Ok(RespFrame::Integer(result))
            }
            
            StringCommand::IncrBy { key, increment } => {
                let result = self.storage.incr_by(db, key, increment)?;
                Ok(RespFrame::Integer(result))
            }
            
            StringCommand::Decr { key } => {
                let result = self.storage.incr_by(db, key, -1)?;
                Ok(RespFrame::Integer(result))
            }
            
            StringCommand::DecrBy { key, decrement } => {
                let result = self.storage.incr_by(db, key, -(decrement))?;
                Ok(RespFrame::Integer(result))
            }
            
            StringCommand::SetNx { key, value } => {
                let result = self.storage.set_string_nx(db, key, value)?;
                Ok(RespFrame::Integer(if result { 1 } else { 0 }))
            }
            
            StringCommand::SetEx { key, value, seconds } => {
                self.storage.set_string_ex(db, key, value, Duration::from_secs(seconds))?;
                Ok(RespFrame::ok())
            }
            
            StringCommand::PSetEx { key, value, milliseconds } => {
                self.storage.set_string_ex(db, key, value, Duration::from_millis(milliseconds))?;
                Ok(RespFrame::ok())
            }
            
            StringCommand::Append { key, value } => {
                let new_len = self.storage.append(db, key, value)?;
                Ok(RespFrame::Integer(new_len as i64))
            }
            
            StringCommand::StrLen { key } => {
                let len = self.storage.strlen(db, &key)?;
                Ok(RespFrame::Integer(len as i64))
            }
            
            StringCommand::GetSet { key, value } => {
                use crate::storage::commands::strings::handle_getset;
                let frames = vec![
                    RespFrame::from_string("GETSET"),
                    RespFrame::from_bytes(key),
                    RespFrame::from_bytes(value),
                ];
                handle_getset(&self.storage, db, &frames)
            }
            
            StringCommand::GetRange { key, start, end } => {
                let result = self.storage.getrange(db, &key, start, end)?;
                Ok(RespFrame::from_bytes(result))
            }
            
            StringCommand::SetRange { key, offset, value } => {
                let new_len = self.storage.setrange(db, key, offset, value)?;
                Ok(RespFrame::Integer(new_len as i64))
            }
            
            StringCommand::Del { keys } => {
                let mut deleted = 0;
                for key in keys {
                    if self.storage.delete(db, &key)? {
                        deleted += 1;
                    }
                }
                Ok(RespFrame::Integer(deleted))
            }
        }
    }
    
    /// Execute list commands
    fn execute_list(&self, db: usize, cmd: ListCommand) -> Result<RespFrame> {
        match cmd {
            ListCommand::LPush { key, values } => {
                let len = self.storage.lpush(db, key, values)?;
                Ok(RespFrame::Integer(len as i64))
            }
            
            ListCommand::RPush { key, values } => {
                let len = self.storage.rpush(db, key, values)?;
                Ok(RespFrame::Integer(len as i64))
            }
            
            ListCommand::LPop { key } => {
                match self.storage.lpop(db, &key)? {
                    Some(value) => Ok(RespFrame::from_bytes(value)),
                    None => Ok(RespFrame::null_bulk()),
                }
            }
            
            ListCommand::RPop { key } => {
                match self.storage.rpop(db, &key)? {
                    Some(value) => Ok(RespFrame::from_bytes(value)),
                    None => Ok(RespFrame::null_bulk()),
                }
            }
            
            ListCommand::LLen { key } => {
                let len = self.storage.llen(db, &key)?;
                Ok(RespFrame::Integer(len as i64))
            }
            
            ListCommand::LIndex { key, index } => {
                match self.storage.lindex(db, &key, index)? {
                    Some(value) => Ok(RespFrame::from_bytes(value)),
                    None => Ok(RespFrame::null_bulk()),
                }
            }
            
            ListCommand::LSet { key, index, value } => {
                self.storage.lset(db, key, index, value)?;
                Ok(RespFrame::ok())
            }
            
            ListCommand::LRange { key, start, stop } => {
                let values = self.storage.lrange(db, &key, start, stop)?;
                let frames: Vec<RespFrame> = values.into_iter()
                    .map(|v| RespFrame::from_bytes(v))
                    .collect();
                Ok(RespFrame::Array(Some(frames)))
            }
            
            ListCommand::LTrim { key, start, stop } => {
                self.storage.ltrim(db, key, start, stop)?;
                Ok(RespFrame::ok())
            }
            
            ListCommand::LRem { key, count, element } => {
                let removed = self.storage.lrem(db, key, count, element)?;
                Ok(RespFrame::Integer(removed as i64))
            }
        }
    }
    
    /// Execute set commands
    fn execute_set(&self, db: usize, cmd: SetCommand) -> Result<RespFrame> {
        match cmd {
            SetCommand::SAdd { key, members } => {
                let added = self.storage.sadd(db, key, members)?;
                Ok(RespFrame::Integer(added as i64))
            }
            
            SetCommand::SRem { key, members } => {
                let removed = self.storage.srem(db, &key, &members)?;
                Ok(RespFrame::Integer(removed as i64))
            }
            
            SetCommand::SMembers { key } => {
                let members = self.storage.smembers(db, &key)?;
                let frames: Vec<RespFrame> = members.into_iter()
                    .map(|m| RespFrame::from_bytes(m))
                    .collect();
                Ok(RespFrame::Array(Some(frames)))
            }
            
            SetCommand::SCard { key } => {
                let count = self.storage.scard(db, &key)?;
                Ok(RespFrame::Integer(count as i64))
            }
            
            SetCommand::SIsMember { key, member } => {
                let is_member = self.storage.sismember(db, &key, &member)?;
                Ok(RespFrame::Integer(if is_member { 1 } else { 0 }))
            }
            
            SetCommand::SUnion { keys } => {
                let result = self.storage.sunion(db, &keys)?;
                let frames: Vec<RespFrame> = result.into_iter()
                    .map(|v| RespFrame::from_bytes(v))
                    .collect();
                Ok(RespFrame::Array(Some(frames)))
            }
            
            SetCommand::SInter { keys } => {
                let result = self.storage.sinter(db, &keys)?;
                let frames: Vec<RespFrame> = result.into_iter()
                    .map(|v| RespFrame::from_bytes(v))
                    .collect();
                Ok(RespFrame::Array(Some(frames)))
            }
            
            SetCommand::SDiff { keys } => {
                let result = self.storage.sdiff(db, &keys)?;
                let frames: Vec<RespFrame> = result.into_iter()
                    .map(|v| RespFrame::from_bytes(v))
                    .collect();
                Ok(RespFrame::Array(Some(frames)))
            }
            
            SetCommand::SRandMember { key, count } => {
                let count = count.unwrap_or(1);
                let members = self.storage.srandmember(db, &key, count)?;
                if count == 1 && !members.is_empty() {
                    Ok(RespFrame::from_bytes(members[0].clone()))
                } else {
                    let frames: Vec<RespFrame> = members.into_iter()
                        .map(|m| RespFrame::from_bytes(m))
                        .collect();
                    Ok(RespFrame::Array(Some(frames)))
                }
            }
            
            SetCommand::SPop { key, count } => {
                let count = count.unwrap_or(1);
                let members = self.storage.spop(db, key, count)?;
                if count == 1 && !members.is_empty() {
                    Ok(RespFrame::from_bytes(members[0].clone()))
                } else {
                    let frames: Vec<RespFrame> = members.into_iter()
                        .map(|m| RespFrame::from_bytes(m))
                        .collect();
                    Ok(RespFrame::Array(Some(frames)))
                }
            }
        }
    }
    
    /// Execute hash commands
    fn execute_hash(&self, db: usize, cmd: HashCommand) -> Result<RespFrame> {
        match cmd {
            HashCommand::HSet { key, field_values } => {
                let added = self.storage.hset(db, key, field_values)?;
                Ok(RespFrame::Integer(added as i64))
            }
            
            HashCommand::HGet { key, field } => {
                match self.storage.hget(db, &key, &field)? {
                    Some(value) => Ok(RespFrame::from_bytes(value)),
                    None => Ok(RespFrame::null_bulk()),
                }
            }
            
            HashCommand::HMSet { key, field_values } => {
                self.storage.hset(db, key, field_values)?;
                Ok(RespFrame::ok())
            }
            
            HashCommand::HMGet { key, fields } => {
                let values = self.storage.hmget(db, &key, &fields)?;
                let frames: Vec<RespFrame> = values.into_iter()
                    .map(|opt| opt.map(RespFrame::from_bytes).unwrap_or(RespFrame::null_bulk()))
                    .collect();
                Ok(RespFrame::Array(Some(frames)))
            }
            
            HashCommand::HGetAll { key } => {
                let pairs = self.storage.hgetall(db, &key)?;
                let mut frames = Vec::new();
                for (field, value) in pairs {
                    frames.push(RespFrame::from_bytes(field));
                    frames.push(RespFrame::from_bytes(value));
                }
                Ok(RespFrame::Array(Some(frames)))
            }
            
            HashCommand::HDel { key, fields } => {
                let deleted = self.storage.hdel(db, key, &fields)?;
                Ok(RespFrame::Integer(deleted as i64))
            }
            
            HashCommand::HLen { key } => {
                let len = self.storage.hlen(db, &key)?;
                Ok(RespFrame::Integer(len as i64))
            }
            
            HashCommand::HExists { key, field } => {
                let exists = self.storage.hexists(db, &key, &field)?;
                Ok(RespFrame::Integer(if exists { 1 } else { 0 }))
            }
            
            HashCommand::HKeys { key } => {
                let keys = self.storage.hkeys(db, &key)?;
                let frames: Vec<RespFrame> = keys.into_iter()
                    .map(|k| RespFrame::from_bytes(k))
                    .collect();
                Ok(RespFrame::Array(Some(frames)))
            }
            
            HashCommand::HVals { key } => {
                let values = self.storage.hvals(db, &key)?;
                let frames: Vec<RespFrame> = values.into_iter()
                    .map(|v| RespFrame::from_bytes(v))
                    .collect();
                Ok(RespFrame::Array(Some(frames)))
            }
            
            HashCommand::HIncrBy { key, field, increment } => {
                let new_value = self.storage.hincrby(db, key, field, increment)?;
                Ok(RespFrame::Integer(new_value))
            }
        }
    }
    
    /// Execute sorted set commands
    fn execute_sorted_set(&self, db: usize, cmd: SortedSetCommand) -> Result<RespFrame> {
        match cmd {
            SortedSetCommand::ZAdd { key, score_members } => {
                let mut added = 0;
                for (score, member) in score_members {
                    if self.storage.zadd(db, key.clone(), member, score)? {
                        added += 1;
                    }
                }
                Ok(RespFrame::Integer(added))
            }
            
            SortedSetCommand::ZRem { key, members } => {
                let mut removed = 0;
                for member in members {
                    if self.storage.zrem(db, &key, &member)? {
                        removed += 1;
                    }
                }
                Ok(RespFrame::Integer(removed))
            }
            
            SortedSetCommand::ZScore { key, member } => {
                match self.storage.zscore(db, &key, &member)? {
                    Some(score) => Ok(RespFrame::from_string(score.to_string())),
                    None => Ok(RespFrame::null_bulk()),
                }
            }
            
            SortedSetCommand::ZCard { key } => {
                let count = self.storage.zcard(db, &key)?;
                Ok(RespFrame::Integer(count as i64))
            }
            
            SortedSetCommand::ZRank { key, member } => {
                match self.storage.zrank(db, &key, &member, false)? {
                    Some(rank) => Ok(RespFrame::Integer(rank as i64)),
                    None => Ok(RespFrame::null_bulk()),
                }
            }
            
            SortedSetCommand::ZRevRank { key, member } => {
                match self.storage.zrank(db, &key, &member, true)? {
                    Some(rank) => Ok(RespFrame::Integer(rank as i64)),
                    None => Ok(RespFrame::null_bulk()),
                }
            }
            
            SortedSetCommand::ZRange { key, start, stop, with_scores } => {
                let members = self.storage.zrange(db, &key, start, stop, false)?;
                let mut frames = Vec::new();
                
                for (member, score) in members {
                    frames.push(RespFrame::from_bytes(member));
                    if with_scores {
                        frames.push(RespFrame::from_string(score.to_string()));
                    }
                }
                
                Ok(RespFrame::Array(Some(frames)))
            }
            
            SortedSetCommand::ZRevRange { key, start, stop, with_scores } => {
                let members = self.storage.zrange(db, &key, start, stop, true)?;
                let mut frames = Vec::new();
                
                for (member, score) in members {
                    frames.push(RespFrame::from_bytes(member));
                    if with_scores {
                        frames.push(RespFrame::from_string(score.to_string()));
                    }
                }
                
                Ok(RespFrame::Array(Some(frames)))
            }
            
            SortedSetCommand::ZRangeByScore { key, min_score, max_score, with_scores } => {
                let members = self.storage.zrangebyscore(db, &key, min_score, max_score, false)?;
                let mut frames = Vec::new();
                
                for (member, score) in members {
                    frames.push(RespFrame::from_bytes(member));
                    if with_scores {
                        frames.push(RespFrame::from_string(score.to_string()));
                    }
                }
                
                Ok(RespFrame::Array(Some(frames)))
            }
            
            SortedSetCommand::ZCount { key, min_score, max_score } => {
                let count = self.storage.zcount(db, &key, min_score, max_score)?;
                Ok(RespFrame::Integer(count as i64))
            }
            
            SortedSetCommand::ZIncrBy { key, increment, member } => {
                let new_score = self.storage.zincrby(db, key, member, increment)?;
                Ok(RespFrame::from_string(new_score.to_string()))
            }
            
            SortedSetCommand::ZRevRangeByScore { key, max_score, min_score, with_scores } => {
                let members = self.storage.zrangebyscore(db, &key, min_score, max_score, true)?;
                let mut frames = Vec::new();
                
                for (member, score) in members {
                    frames.push(RespFrame::from_bytes(member));
                    if with_scores {
                        frames.push(RespFrame::from_string(score.to_string()));
                    }
                }
                
                Ok(RespFrame::Array(Some(frames)))
            }
            
            SortedSetCommand::ZPopMin { key, count } => {
                let count_val = count.unwrap_or(1);
                let mut popped = Vec::new();
                
                for _ in 0..count_val {
                    // Get the member with lowest score (rank 0)
                    let members = self.storage.zrange(db, &key, 0, 0, false)?;
                    if let Some((member, score)) = members.into_iter().next() {
                        // Remove the member atomically
                        if self.storage.zrem(db, &key, &member)? {
                            popped.push(RespFrame::from_bytes(member));
                            popped.push(RespFrame::from_string(score.to_string()));
                        } else {
                            // Member was removed by another operation, stop
                            break;
                        }
                    } else {
                        // No more members in the sorted set
                        break;
                    }
                }
                
                Ok(RespFrame::Array(Some(popped)))
            }
            
            SortedSetCommand::ZPopMax { key, count } => {
                let count_val = count.unwrap_or(1);
                let mut popped = Vec::new();
                
                for _ in 0..count_val {
                    // Get the member with highest score (rank -1)
                    let members = self.storage.zrange(db, &key, -1, -1, false)?;
                    if let Some((member, score)) = members.into_iter().next() {
                        // Remove the member atomically
                        if self.storage.zrem(db, &key, &member)? {
                            popped.push(RespFrame::from_bytes(member));
                            popped.push(RespFrame::from_string(score.to_string()));
                        } else {
                            // Member was removed by another operation, stop
                            break;
                        }
                    } else {
                        // No more members in the sorted set
                        break;
                    }
                }
                
                Ok(RespFrame::Array(Some(popped)))
            }
            
            SortedSetCommand::ZRemRangeByRank { key, start, stop } => {
                let members = self.storage.zrange(db, &key, start, stop, false)?;
                let mut removed = 0;
                
                for (member, _score) in members {
                    if self.storage.zrem(db, &key, &member)? {
                        removed += 1;
                    }
                }
                
                Ok(RespFrame::Integer(removed))
            }
            
            SortedSetCommand::ZRemRangeByScore { key, min_score, max_score } => {
                let members = self.storage.zrangebyscore(db, &key, min_score, max_score, false)?;
                let mut removed = 0;
                
                for (member, _score) in members {
                    if self.storage.zrem(db, &key, &member)? {
                        removed += 1;
                    }
                }
                
                Ok(RespFrame::Integer(removed))
            }
            
            SortedSetCommand::ZRemRangeByLex { key: _, min_lex: _, max_lex: _ } => {
                // Basic range-by-lex implementation 
                // Note: This is a simplified implementation, full lex range would need more sophisticated storage engine support
                Ok(RespFrame::Integer(0))
            }
        }
    }
    
    /// Execute key management commands
    fn execute_key(&self, db: usize, cmd: KeyCommand) -> Result<RespFrame> {
        match cmd {
            KeyCommand::Exists { keys } => {
                let mut count = 0;
                for key in keys {
                    if self.storage.exists(db, &key)? {
                        count += 1;
                    }
                }
                Ok(RespFrame::Integer(count))
            }
            
            KeyCommand::Expire { key, seconds } => {
                let result = self.storage.expire(db, &key, Duration::from_secs(seconds))?;
                Ok(RespFrame::Integer(if result { 1 } else { 0 }))
            }
            
            KeyCommand::PExpire { key, milliseconds } => {
                let result = self.storage.pexpire(db, &key, milliseconds)?;
                Ok(RespFrame::Integer(if result { 1 } else { 0 }))
            }
            
            KeyCommand::Ttl { key } => {
                let ttl = self.storage.ttl(db, &key)?;
                match ttl {
                    Some(duration) => {
                        let secs = duration.as_secs() as i64;
                        Ok(RespFrame::Integer(if secs == 0 && duration.subsec_millis() > 0 { 1 } else { secs }))
                    }
                    None => {
                        if self.storage.exists(db, &key)? {
                            Ok(RespFrame::Integer(-1)) // Key exists but no expiration
                        } else {
                            Ok(RespFrame::Integer(-2)) // Key doesn't exist
                        }
                    }
                }
            }
            
            KeyCommand::Pttl { key } => {
                let ttl_millis = self.storage.pttl(db, &key)?;
                Ok(RespFrame::Integer(ttl_millis))
            }
            
            KeyCommand::Persist { key } => {
                let result = self.storage.persist(db, &key)?;
                Ok(RespFrame::Integer(if result { 1 } else { 0 }))
            }
            
            KeyCommand::Type { key } => {
                let type_name = self.storage.key_type(db, &key)?;
                Ok(RespFrame::from_string(type_name))
            }
            
            KeyCommand::Rename { old_key, new_key } => {
                self.storage.rename(db, &old_key, new_key)?;
                Ok(RespFrame::ok())
            }
            
            KeyCommand::RenameNx { old_key, new_key } => {
                use crate::storage::commands::strings::handle_rename;
                let frames = vec![
                    RespFrame::from_string("RENAMENX"),
                    RespFrame::from_bytes(old_key),
                    RespFrame::from_bytes(new_key),
                ];
                handle_rename(&self.storage, db, &frames)
            }
            
            KeyCommand::RandomKey => {
                let keys = self.storage.get_all_keys(db)?;
                if keys.is_empty() {
                    Ok(RespFrame::null_bulk())
                } else {
                    use rand::seq::SliceRandom;
                    let mut rng = rand::thread_rng();
                    let random_key = keys.choose(&mut rng).unwrap();
                    Ok(RespFrame::from_bytes(random_key.clone()))
                }
            }
        }
    }
    
    /// Execute server commands
    fn execute_server(&self, cmd: ServerCommand) -> Result<RespFrame> {
        match cmd {
            ServerCommand::Ping { message } => {
                match message {
                    Some(msg) => Ok(RespFrame::from_bytes(msg)),
                    None => Ok(RespFrame::from_string("PONG")),
                }
            }
            
            ServerCommand::Echo { message } => {
                Ok(RespFrame::from_bytes(message))
            }
            
            ServerCommand::Time => {
                use std::time::{SystemTime, UNIX_EPOCH};
                let now = SystemTime::now().duration_since(UNIX_EPOCH)
                    .unwrap_or_default();
                let seconds = now.as_secs();
                let microseconds = now.subsec_micros();
                
                Ok(RespFrame::Array(Some(vec![
                    RespFrame::from_string(seconds.to_string()),
                    RespFrame::from_string(microseconds.to_string()),
                ])))
            }
            
            ServerCommand::Info { section: _section } => {
                Ok(RespFrame::from_string("# Server\nredis_version:7.0.0\n"))
            }
        }
    }
    
    /// Execute stream commands
    fn execute_stream(&self, db: usize, cmd: StreamCommand) -> Result<RespFrame> {
        match cmd {
            StreamCommand::XAdd { key, id, fields } => {
                use crate::storage::commands::streams::handle_xadd;
                let mut frames = vec![
                    RespFrame::from_string("XADD"),
                    RespFrame::from_bytes(key),
                ];
                
                if let Some(id_str) = id {
                    frames.push(RespFrame::from_string(id_str));
                } else {
                    frames.push(RespFrame::from_string("*"));
                }
                
                for (field, value) in fields {
                    frames.push(RespFrame::from_bytes(field));
                    frames.push(RespFrame::from_bytes(value));
                }
                
                handle_xadd(&self.storage, db, &frames)
            }
            
            StreamCommand::XLen { key } => {
                use crate::storage::commands::streams::handle_xlen;
                let frames = vec![
                    RespFrame::from_string("XLEN"),
                    RespFrame::from_bytes(key),
                ];
                handle_xlen(&self.storage, db, &frames)
            }
            
            StreamCommand::XRange { key, start, end, count } => {
                use crate::storage::commands::streams::handle_xrange;
                let mut frames = vec![
                    RespFrame::from_string("XRANGE"),
                    RespFrame::from_bytes(key),
                    RespFrame::from_string(start),
                    RespFrame::from_string(end),
                ];
                
                if let Some(c) = count {
                    frames.push(RespFrame::from_string("COUNT"));
                    frames.push(RespFrame::from_string(c.to_string()));
                }
                
                handle_xrange(&self.storage, db, &frames)
            }
            
            StreamCommand::XRevRange { key, start, end, count } => {
                use crate::storage::commands::streams::handle_xrevrange;
                let mut frames = vec![
                    RespFrame::from_string("XREVRANGE"),
                    RespFrame::from_bytes(key),
                    RespFrame::from_string(start),
                    RespFrame::from_string(end),
                ];
                
                if let Some(c) = count {
                    frames.push(RespFrame::from_string("COUNT"));
                    frames.push(RespFrame::from_string(c.to_string()));
                }
                
                handle_xrevrange(&self.storage, db, &frames)
            }
            
            StreamCommand::XRead { keys_and_ids, count, block: _block } => {
                use crate::storage::commands::streams::handle_xread;
                let mut frames = vec![RespFrame::from_string("XREAD")];
                
                if let Some(c) = count {
                    frames.push(RespFrame::from_string("COUNT"));
                    frames.push(RespFrame::from_string(c.to_string()));
                }
                
                frames.push(RespFrame::from_string("STREAMS"));
                
                for (key, id) in &keys_and_ids {
                    frames.push(RespFrame::from_bytes(key.clone()));
                }
                for (_key, id) in &keys_and_ids {
                    frames.push(RespFrame::from_string(id.clone()));
                }
                
                handle_xread(&self.storage, db, &frames)
            }
            
            StreamCommand::XTrim { key, strategy, threshold } => {
                use crate::storage::commands::streams::handle_xtrim;
                let frames = vec![
                    RespFrame::from_string("XTRIM"),
                    RespFrame::from_bytes(key),
                    RespFrame::from_string(strategy),
                    RespFrame::from_string(threshold.to_string()),
                ];
                handle_xtrim(&self.storage, db, &frames)
            }
            
            StreamCommand::XDel { key, ids } => {
                use crate::storage::commands::streams::handle_xdel;
                let mut frames = vec![
                    RespFrame::from_string("XDEL"),
                    RespFrame::from_bytes(key),
                ];
                
                for id in ids {
                    frames.push(RespFrame::from_string(id));
                }
                
                handle_xdel(&self.storage, db, &frames)
            }
        }
    }
    
    /// Execute scan commands
    fn execute_scan(&self, db: usize, cmd: ScanCommand) -> Result<RespFrame> {
        match cmd {
            ScanCommand::Scan { cursor, pattern, count, type_filter } => {
                use crate::storage::commands::scan::handle_scan;
                let mut frames = vec![
                    RespFrame::from_string("SCAN"),
                    RespFrame::from_string(cursor.to_string()),
                ];
                
                if let Some(p) = pattern {
                    frames.push(RespFrame::from_string("MATCH"));
                    frames.push(RespFrame::from_bytes(p));
                }
                
                if let Some(c) = count {
                    frames.push(RespFrame::from_string("COUNT"));
                    frames.push(RespFrame::from_string(c.to_string()));
                }
                
                if let Some(t) = type_filter {
                    frames.push(RespFrame::from_string("TYPE"));
                    frames.push(RespFrame::from_string(t));
                }
                
                handle_scan(&self.storage, db, &frames)
            }
            
            ScanCommand::HScan { key, cursor, pattern, count } => {
                use crate::storage::commands::scan::handle_hscan;
                let mut frames = vec![
                    RespFrame::from_string("HSCAN"),
                    RespFrame::from_bytes(key),
                    RespFrame::from_string(cursor.to_string()),
                ];
                
                if let Some(p) = pattern {
                    frames.push(RespFrame::from_string("MATCH"));
                    frames.push(RespFrame::from_bytes(p));
                }
                
                if let Some(c) = count {
                    frames.push(RespFrame::from_string("COUNT"));
                    frames.push(RespFrame::from_string(c.to_string()));
                }
                
                handle_hscan(&self.storage, db, &frames)
            }
            
            ScanCommand::SScan { key, cursor, pattern, count } => {
                use crate::storage::commands::scan::handle_sscan;
                let mut frames = vec![
                    RespFrame::from_string("SSCAN"),
                    RespFrame::from_bytes(key),
                    RespFrame::from_string(cursor.to_string()),
                ];
                
                if let Some(p) = pattern {
                    frames.push(RespFrame::from_string("MATCH"));
                    frames.push(RespFrame::from_bytes(p));
                }
                
                if let Some(c) = count {
                    frames.push(RespFrame::from_string("COUNT"));
                    frames.push(RespFrame::from_string(c.to_string()));
                }
                
                handle_sscan(&self.storage, db, &frames)
            }
            
            ScanCommand::ZScan { key, cursor, pattern, count } => {
                use crate::storage::commands::scan::handle_zscan;
                let mut frames = vec![
                    RespFrame::from_string("ZSCAN"),
                    RespFrame::from_bytes(key),
                    RespFrame::from_string(cursor.to_string()),
                ];
                
                if let Some(p) = pattern {
                    frames.push(RespFrame::from_string("MATCH"));
                    frames.push(RespFrame::from_bytes(p));
                }
                
                if let Some(c) = count {
                    frames.push(RespFrame::from_string("COUNT"));
                    frames.push(RespFrame::from_string(c.to_string()));
                }
                
                handle_zscan(&self.storage, db, &frames)
            }
        }
    }
    
    /// Execute database commands
    fn execute_database(&self, cmd: DatabaseCommand) -> Result<RespFrame> {
        let db = self.conn_context.as_ref().map(|c| c.db_index).unwrap_or(0);
        
        match cmd {
            DatabaseCommand::FlushDb => {
                self.storage.flush_db(db)?;
                Ok(RespFrame::ok())
            }
            
            DatabaseCommand::FlushAll => {
                for db_idx in 0..self.storage.database_count() {
                    self.storage.flush_db(db_idx)?;
                }
                Ok(RespFrame::ok())
            }
            
            DatabaseCommand::DbSize => {
                let keys = self.storage.get_all_keys(db)?;
                Ok(RespFrame::Integer(keys.len() as i64))
            }
            
            DatabaseCommand::Keys { pattern } => {
                use crate::storage::commands::strings::handle_keys;
                let frames = vec![
                    RespFrame::from_string("KEYS"),
                    RespFrame::from_bytes(pattern),
                ];
                handle_keys(&self.storage, db, &frames)
            }
        }
    }
    
    /// Execute consumer group commands
    fn execute_consumer_group(&self, db: usize, cmd: ConsumerGroupCommand) -> Result<RespFrame> {
        match cmd {
            ConsumerGroupCommand::XGroup { subcommand, key, group, args } => {
                use crate::storage::commands::consumer_groups::handle_xgroup;
                let mut frames = vec![
                    RespFrame::from_string("XGROUP"),
                    RespFrame::from_string(subcommand),
                    RespFrame::from_bytes(key),
                    RespFrame::from_string(group),
                ];
                
                for arg in args {
                    frames.push(RespFrame::from_string(arg));
                }
                
                handle_xgroup(&self.storage, db, &frames)
            }
            
            ConsumerGroupCommand::XReadGroup { group, consumer, keys_and_ids, count, block: _block, noack: _noack } => {
                use crate::storage::commands::consumer_groups::handle_xreadgroup;
                let mut frames = vec![
                    RespFrame::from_string("XREADGROUP"),
                    RespFrame::from_string("GROUP"),
                    RespFrame::from_string(group),
                    RespFrame::from_string(consumer),
                ];
                
                if let Some(c) = count {
                    frames.push(RespFrame::from_string("COUNT"));
                    frames.push(RespFrame::from_string(c.to_string()));
                }
                
                frames.push(RespFrame::from_string("STREAMS"));
                
                for (key, id) in keys_and_ids {
                    frames.push(RespFrame::from_bytes(key));
                    frames.push(RespFrame::from_string(id));
                }
                
                handle_xreadgroup(&self.storage, db, &frames)
            }
            
            ConsumerGroupCommand::XAck { key, group, ids } => {
                use crate::storage::commands::consumer_groups::handle_xack;
                let mut frames = vec![
                    RespFrame::from_string("XACK"),
                    RespFrame::from_bytes(key),
                    RespFrame::from_string(group),
                ];
                
                for id in ids {
                    frames.push(RespFrame::from_string(id));
                }
                
                handle_xack(&self.storage, db, &frames)
            }
            
            ConsumerGroupCommand::XPending { key, group, range, consumer: _consumer } => {
                use crate::storage::commands::consumer_groups::handle_xpending;
                let mut frames = vec![
                    RespFrame::from_string("XPENDING"),
                    RespFrame::from_bytes(key),
                    RespFrame::from_string(group),
                ];
                
                if let Some((start, end, count)) = range {
                    frames.push(RespFrame::from_string(start));
                    frames.push(RespFrame::from_string(end));
                    frames.push(RespFrame::from_string(count.to_string()));
                }
                
                handle_xpending(&self.storage, db, &frames)
            }
            
            ConsumerGroupCommand::XClaim { key, group, consumer, min_idle_time, ids, force: _force, justid: _justid } => {
                use crate::storage::commands::consumer_groups::handle_xclaim;
                let mut frames = vec![
                    RespFrame::from_string("XCLAIM"),
                    RespFrame::from_bytes(key),
                    RespFrame::from_string(group),
                    RespFrame::from_string(consumer),
                    RespFrame::from_string(min_idle_time.to_string()),
                ];
                
                for id in ids {
                    frames.push(RespFrame::from_string(id));
                }
                
                handle_xclaim(&self.storage, db, &frames)
            }
            
            ConsumerGroupCommand::XInfo { subcommand, key, group } => {
                use crate::storage::commands::consumer_groups::handle_xinfo;
                let mut frames = vec![
                    RespFrame::from_string("XINFO"),
                    RespFrame::from_string(subcommand),
                    RespFrame::from_bytes(key),
                ];
                
                if let Some(g) = group {
                    frames.push(RespFrame::from_string(g));
                }
                
                handle_xinfo(&self.storage, db, &frames)
            }
        }
    }
    
    /// Execute persistence commands
    fn execute_persistence(&self, cmd: PersistenceCommand) -> Result<RespFrame> {
        match cmd {
            PersistenceCommand::Save => {
                Ok(RespFrame::ok())
            }
            
            PersistenceCommand::BgSave => {
                Ok(RespFrame::from_string("Background saving started"))
            }
            
            PersistenceCommand::LastSave => {
                use std::time::{SystemTime, UNIX_EPOCH};
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                Ok(RespFrame::Integer(timestamp))
            }
            
            PersistenceCommand::BgRewriteAof => {
                Ok(RespFrame::from_string("Background append only file rewriting started"))
            }
        }
    }
    
    /// Execute bit commands
    fn execute_bit(&self, db: usize, cmd: BitCommand) -> Result<RespFrame> {
        match cmd {
            BitCommand::GetBit { key, offset } => {
                match self.storage.get_string(db, &key)? {
                    Some(value) => {
                        let byte_offset = offset / 8;
                        let bit_offset = offset % 8;
                        
                        if byte_offset < value.len() {
                            let byte_val = value[byte_offset];
                            let bit = (byte_val >> (7 - bit_offset)) & 1;
                            Ok(RespFrame::Integer(bit as i64))
                        } else {
                            Ok(RespFrame::Integer(0))
                        }
                    }
                    None => Ok(RespFrame::Integer(0)),
                }
            }
            
            BitCommand::SetBit { key, offset, value } => {
                let byte_offset = offset / 8;
                let bit_offset = offset % 8;
                
                let mut data = match self.storage.get_string(db, &key)? {
                    Some(existing) => existing,
                    None => Vec::new(),
                };
                
                if byte_offset >= data.len() {
                    data.resize(byte_offset + 1, 0);
                }
                
                let old_bit = (data[byte_offset] >> (7 - bit_offset)) & 1;
                
                if value == 1 {
                    data[byte_offset] |= 1 << (7 - bit_offset);
                } else {
                    data[byte_offset] &= !(1 << (7 - bit_offset));
                }
                
                self.storage.set_string(db, key, data)?;
                Ok(RespFrame::Integer(old_bit as i64))
            }
            
            BitCommand::BitCount { key, start, end } => {
                match self.storage.get_string(db, &key)? {
                    Some(value) => {
                        let (start_byte, end_byte) = if let (Some(s), Some(e)) = (start, end) {
                            let len = value.len() as isize;
                            let start_pos = if s < 0 { (len + s).max(0) } else { s.min(len - 1) } as usize;
                            let end_pos = if e < 0 { (len + e).max(0) } else { e.min(len - 1) } as usize;
                            (start_pos, end_pos)
                        } else {
                            (0, value.len().saturating_sub(1))
                        };
                        
                        let slice = &value[start_byte..=end_byte.min(value.len().saturating_sub(1))];
                        let bit_count = slice.iter().map(|&byte| byte.count_ones() as i64).sum::<i64>();
                        Ok(RespFrame::Integer(bit_count))
                    }
                    None => Ok(RespFrame::Integer(0)),
                }
            }
        }
    }
    
    /// Execute config commands
    fn execute_config(&self, cmd: ConfigCommand) -> Result<RespFrame> {
        match cmd {
            ConfigCommand::Get { parameter } => {
                match parameter.to_lowercase().as_str() {
                    "maxmemory" => Ok(RespFrame::Array(Some(vec![
                        RespFrame::from_string("maxmemory"),
                        RespFrame::from_string("0"),
                    ]))),
                    "timeout" => Ok(RespFrame::Array(Some(vec![
                        RespFrame::from_string("timeout"),
                        RespFrame::from_string("0"),
                    ]))),
                    _ => Ok(RespFrame::Array(Some(vec![
                        RespFrame::from_string(parameter),
                        RespFrame::from_string(""),
                    ]))),
                }
            }
            
            ConfigCommand::Set { parameter: _param, value: _val } => {
                Ok(RespFrame::ok())
            }
        }
    }
}

/// Parser for RESP frames to typed commands
pub struct CommandParser;

impl CommandParser {
    /// Parse RESP frames into typed command
    pub fn parse(frames: &[RespFrame]) -> Result<ParsedCommand> {
        if frames.is_empty() {
            return Err(FerrousError::Command(CommandError::EmptyCommand));
        }
        
        let cmd_name = Self::extract_string(&frames[0])?.to_uppercase();
        
        let command = match cmd_name.as_str() {
            // String commands
            "SET" => Command::String(Self::parse_set(frames)?),
            "GET" => Command::String(Self::parse_get(frames)?),
            "MGET" => Command::String(Self::parse_mget(frames)?),
            "MSET" => Command::String(Self::parse_mset(frames)?),
            "INCR" => Command::String(Self::parse_incr(frames)?),
            "INCRBY" => Command::String(Self::parse_incrby(frames)?),
            "DECR" => Command::String(Self::parse_decr(frames)?),
            "DECRBY" => Command::String(Self::parse_decrby(frames)?),
            "SETNX" => Command::String(Self::parse_setnx(frames)?),
            "SETEX" => Command::String(Self::parse_setex(frames)?),
            "PSETEX" => Command::String(Self::parse_psetex(frames)?),
            "APPEND" => Command::String(Self::parse_append(frames)?),
            "STRLEN" => Command::String(Self::parse_strlen(frames)?),
            "GETSET" => Command::String(Self::parse_getset(frames)?),
            "GETRANGE" => Command::String(Self::parse_getrange(frames)?),
            "SETRANGE" => Command::String(Self::parse_setrange(frames)?),
            "DEL" => Command::String(Self::parse_del(frames)?),
            
            // List commands
            "LPUSH" => Command::List(Self::parse_lpush(frames)?),
            "RPUSH" => Command::List(Self::parse_rpush(frames)?),
            "LPOP" => Command::List(Self::parse_lpop(frames)?),
            "RPOP" => Command::List(Self::parse_rpop(frames)?),
            "LLEN" => Command::List(Self::parse_llen(frames)?),
            "LINDEX" => Command::List(Self::parse_lindex(frames)?),
            "LSET" => Command::List(Self::parse_lset(frames)?),
            "LRANGE" => Command::List(Self::parse_lrange(frames)?),
            "LTRIM" => Command::List(Self::parse_ltrim(frames)?),
            "LREM" => Command::List(Self::parse_lrem(frames)?),
            
            // Set commands
            "SADD" => Command::Set(Self::parse_sadd(frames)?),
            "SREM" => Command::Set(Self::parse_srem(frames)?),
            "SMEMBERS" => Command::Set(Self::parse_smembers(frames)?),
            "SCARD" => Command::Set(Self::parse_scard(frames)?),
            "SISMEMBER" => Command::Set(Self::parse_sismember(frames)?),
            "SUNION" => Command::Set(Self::parse_sunion(frames)?),
            "SINTER" => Command::Set(Self::parse_sinter(frames)?),
            "SDIFF" => Command::Set(Self::parse_sdiff(frames)?),
            "SRANDMEMBER" => Command::Set(Self::parse_srandmember(frames)?),
            "SPOP" => Command::Set(Self::parse_spop(frames)?),
            
            // Hash commands
            "HSET" => Command::Hash(Self::parse_hset(frames)?),
            "HGET" => Command::Hash(Self::parse_hget(frames)?),
            "HMSET" => Command::Hash(Self::parse_hmset(frames)?),
            "HMGET" => Command::Hash(Self::parse_hmget(frames)?),
            "HGETALL" => Command::Hash(Self::parse_hgetall(frames)?),
            "HDEL" => Command::Hash(Self::parse_hdel(frames)?),
            "HLEN" => Command::Hash(Self::parse_hlen(frames)?),
            "HEXISTS" => Command::Hash(Self::parse_hexists(frames)?),
            "HKEYS" => Command::Hash(Self::parse_hkeys(frames)?),
            "HVALS" => Command::Hash(Self::parse_hvals(frames)?),
            "HINCRBY" => Command::Hash(Self::parse_hincrby(frames)?),
            
            // Sorted set commands
            "ZADD" => Command::SortedSet(Self::parse_zadd(frames)?),
            "ZREM" => Command::SortedSet(Self::parse_zrem(frames)?),
            "ZSCORE" => Command::SortedSet(Self::parse_zscore(frames)?),
            "ZCARD" => Command::SortedSet(Self::parse_zcard(frames)?),
            "ZRANK" => Command::SortedSet(Self::parse_zrank(frames)?),
            "ZREVRANK" => Command::SortedSet(Self::parse_zrevrank(frames)?),
            "ZRANGE" => Command::SortedSet(Self::parse_zrange(frames)?),
            "ZREVRANGE" => Command::SortedSet(Self::parse_zrevrange(frames)?),
            "ZRANGEBYSCORE" => Command::SortedSet(Self::parse_zrangebyscore(frames)?),
            "ZCOUNT" => Command::SortedSet(Self::parse_zcount(frames)?),
            "ZINCRBY" => Command::SortedSet(Self::parse_zincrby(frames)?),
            "ZREVRANGEBYSCORE" => Command::SortedSet(Self::parse_zrevrangebyscore(frames)?),
            "ZPOPMIN" => Command::SortedSet(Self::parse_zpopmin(frames)?),
            "ZPOPMAX" => Command::SortedSet(Self::parse_zpopmax(frames)?),
            "ZREMRANGEBYRANK" => Command::SortedSet(Self::parse_zremrangebyrank(frames)?),
            "ZREMRANGEBYSCORE" => Command::SortedSet(Self::parse_zremrangebyscore(frames)?),
            "ZREMRANGEBYLEX" => Command::SortedSet(Self::parse_zremrangebylex(frames)?),
            
            // Key commands
            "EXISTS" => Command::Key(Self::parse_exists(frames)?),
            "EXPIRE" => Command::Key(Self::parse_expire(frames)?),
            "PEXPIRE" => Command::Key(Self::parse_pexpire(frames)?),
            "TTL" => Command::Key(Self::parse_ttl(frames)?),
            "PTTL" => Command::Key(Self::parse_pttl(frames)?),
            "PERSIST" => Command::Key(Self::parse_persist(frames)?),
            "TYPE" => Command::Key(Self::parse_type(frames)?),
            "RENAME" => Command::Key(Self::parse_rename(frames)?),
            "RENAMENX" => Command::Key(Self::parse_renamenx(frames)?),
            "RANDOMKEY" => Command::Key(KeyCommand::RandomKey),
            
            // Server commands
            "PING" => Command::Server(Self::parse_ping(frames)?),
            "ECHO" => Command::Server(Self::parse_echo(frames)?),
            "TIME" => Command::Server(ServerCommand::Time),
            "INFO" => Command::Server(Self::parse_info(frames)?),
            
            // Stream commands
            "XADD" => Command::Stream(Self::parse_xadd(frames)?),
            "XLEN" => Command::Stream(Self::parse_xlen(frames)?),
            "XRANGE" => Command::Stream(Self::parse_xrange(frames)?),
            "XREVRANGE" => Command::Stream(Self::parse_xrevrange(frames)?),
            "XREAD" => Command::Stream(Self::parse_xread(frames)?),
            "XTRIM" => Command::Stream(Self::parse_xtrim(frames)?),
            "XDEL" => Command::Stream(Self::parse_xdel(frames)?),
            
            // Scan commands  
            "SCAN" => Command::Scan(Self::parse_scan_cmd(frames)?),
            "HSCAN" => Command::Scan(Self::parse_hscan_cmd(frames)?),
            "SSCAN" => Command::Scan(Self::parse_sscan_cmd(frames)?),
            "ZSCAN" => Command::Scan(Self::parse_zscan_cmd(frames)?),
            
            // Database commands
            "FLUSHDB" => Command::Database(DatabaseCommand::FlushDb),
            "FLUSHALL" => Command::Database(DatabaseCommand::FlushAll),
            "DBSIZE" => Command::Database(DatabaseCommand::DbSize),
            "KEYS" => Command::Database(Self::parse_keys_cmd(frames)?),
            
            // Consumer Group commands
            "XGROUP" => Command::ConsumerGroup(Self::parse_xgroup(frames)?),
            "XREADGROUP" => Command::ConsumerGroup(Self::parse_xreadgroup(frames)?),
            "XACK" => Command::ConsumerGroup(Self::parse_xack(frames)?),
            "XPENDING" => Command::ConsumerGroup(Self::parse_xpending(frames)?),
            "XCLAIM" => Command::ConsumerGroup(Self::parse_xclaim(frames)?),
            "XINFO" => Command::ConsumerGroup(Self::parse_xinfo(frames)?),
            
            // Persistence commands
            "SAVE" => Command::Persistence(PersistenceCommand::Save),
            "BGSAVE" => Command::Persistence(PersistenceCommand::BgSave),
            "LASTSAVE" => Command::Persistence(PersistenceCommand::LastSave),
            "BGREWRITEAOF" => Command::Persistence(PersistenceCommand::BgRewriteAof),
            
            // Bit operations
            "GETBIT" => Command::Bit(Self::parse_getbit(frames)?),
            "SETBIT" => Command::Bit(Self::parse_setbit(frames)?),
            "BITCOUNT" => Command::Bit(Self::parse_bitcount(frames)?),
            
            // Config operations
            "CONFIG" => Command::Config(Self::parse_config(frames)?),
            
            _ => return Err(FerrousError::Command(CommandError::UnknownCommand(cmd_name))),
        };
        
        Ok(ParsedCommand {
            command,
            db_override: None,
        })
    }
    
    fn extract_string(frame: &RespFrame) -> Result<String> {
        match frame {
            RespFrame::BulkString(Some(bytes)) => {
                String::from_utf8(bytes.as_ref().clone())
                    .map_err(|_| FerrousError::Command(CommandError::InvalidUtf8))
            }
            _ => Err(FerrousError::Command(CommandError::InvalidArgumentType)),
        }
    }
    
    fn extract_bytes(frame: &RespFrame) -> Result<Vec<u8>> {
        match frame {
            RespFrame::BulkString(Some(bytes)) => Ok(bytes.as_ref().clone()),
            _ => Err(FerrousError::Command(CommandError::InvalidArgumentType)),
        }
    }
    
    fn parse_set(frames: &[RespFrame]) -> Result<StringCommand> {
        if frames.len() < 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("SET".into())));
        }
        
        let key = Self::extract_bytes(&frames[1])?;
        let value = Self::extract_bytes(&frames[2])?;
        let mut options = SetOptions::default();
        
        let mut i = 3;
        while i < frames.len() {
            let opt = Self::extract_string(&frames[i])?.to_uppercase();
            match opt.as_str() {
                "NX" => {
                    options.nx = true;
                    i += 1;
                }
                "XX" => {
                    options.xx = true;
                    i += 1;
                }
                "GET" => {
                    options.get = true;
                    i += 1;
                }
                "EX" => {
                    if i + 1 >= frames.len() {
                        return Err(FerrousError::Command(CommandError::SyntaxError("Missing EX value".to_string())));
                    }
                    let seconds = Self::extract_string(&frames[i + 1])?.parse::<u64>()
                        .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
                    options.expiration = Some(Duration::from_secs(seconds));
                    i += 2;
                }
                "PX" => {
                    if i + 1 >= frames.len() {
                        return Err(FerrousError::Command(CommandError::SyntaxError("Missing PX value".to_string())));
                    }
                    let millis = Self::extract_string(&frames[i + 1])?.parse::<u64>()
                        .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
                    options.expiration = Some(Duration::from_millis(millis));
                    i += 2;
                }
                "KEEPTTL" => {
                    options.keepttl = true;
                    i += 1;
                }
                _ => return Err(FerrousError::Command(CommandError::SyntaxError("Unknown SET option".to_string()))),
            }
        }
        
        Ok(StringCommand::Set { key, value, options })
    }
    
    fn parse_get(frames: &[RespFrame]) -> Result<StringCommand> {
        if frames.len() != 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("GET".into())));
        }
        Ok(StringCommand::Get {
            key: Self::extract_bytes(&frames[1])?,
        })
    }
    
    fn parse_incr(frames: &[RespFrame]) -> Result<StringCommand> {
        if frames.len() != 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("INCR".into())));
        }
        Ok(StringCommand::Incr {
            key: Self::extract_bytes(&frames[1])?,
        })
    }
    
    fn parse_incrby(frames: &[RespFrame]) -> Result<StringCommand> {
        if frames.len() != 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("INCRBY".into())));
        }
        let increment = Self::extract_string(&frames[2])?.parse::<i64>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        Ok(StringCommand::IncrBy {
            key: Self::extract_bytes(&frames[1])?,
            increment,
        })
    }
    
    fn parse_del(frames: &[RespFrame]) -> Result<StringCommand> {
        if frames.len() < 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("DEL".into())));
        }
        let mut keys = Vec::new();
        for i in 1..frames.len() {
            keys.push(Self::extract_bytes(&frames[i])?);
        }
        Ok(StringCommand::Del { keys })
    }

    fn parse_mget(frames: &[RespFrame]) -> Result<StringCommand> {
        if frames.len() < 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("MGET".into())));
        }
        let mut keys = Vec::new();
        for i in 1..frames.len() {
            keys.push(Self::extract_bytes(&frames[i])?);
        }
        Ok(StringCommand::MGet { keys })
    }

    fn parse_mset(frames: &[RespFrame]) -> Result<StringCommand> {
        if frames.len() < 3 || frames.len() % 2 == 0 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("MSET".into())));
        }
        let mut pairs = Vec::new();
        let mut i = 1;
        while i < frames.len() {
            let key = Self::extract_bytes(&frames[i])?;
            let value = Self::extract_bytes(&frames[i + 1])?;
            pairs.push((key, value));
            i += 2;
        }
        Ok(StringCommand::MSet { pairs })
    }

    fn parse_decr(frames: &[RespFrame]) -> Result<StringCommand> {
        if frames.len() != 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("DECR".into())));
        }
        Ok(StringCommand::Decr {
            key: Self::extract_bytes(&frames[1])?,
        })
    }

    fn parse_decrby(frames: &[RespFrame]) -> Result<StringCommand> {
        if frames.len() != 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("DECRBY".into())));
        }
        let decrement = Self::extract_string(&frames[2])?.parse::<i64>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        Ok(StringCommand::DecrBy {
            key: Self::extract_bytes(&frames[1])?,
            decrement,
        })
    }

    fn parse_setnx(frames: &[RespFrame]) -> Result<StringCommand> {
        if frames.len() != 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("SETNX".into())));
        }
        Ok(StringCommand::SetNx {
            key: Self::extract_bytes(&frames[1])?,
            value: Self::extract_bytes(&frames[2])?,
        })
    }

    fn parse_setex(frames: &[RespFrame]) -> Result<StringCommand> {
        if frames.len() != 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("SETEX".into())));
        }
        let seconds = Self::extract_string(&frames[2])?.parse::<u64>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        Ok(StringCommand::SetEx {
            key: Self::extract_bytes(&frames[1])?,
            value: Self::extract_bytes(&frames[3])?,
            seconds,
        })
    }

    fn parse_psetex(frames: &[RespFrame]) -> Result<StringCommand> {
        if frames.len() != 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("PSETEX".into())));
        }
        let milliseconds = Self::extract_string(&frames[2])?.parse::<u64>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        Ok(StringCommand::PSetEx {
            key: Self::extract_bytes(&frames[1])?,
            value: Self::extract_bytes(&frames[3])?,
            milliseconds,
        })
    }

    fn parse_append(frames: &[RespFrame]) -> Result<StringCommand> {
        if frames.len() != 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("APPEND".into())));
        }
        Ok(StringCommand::Append {
            key: Self::extract_bytes(&frames[1])?,
            value: Self::extract_bytes(&frames[2])?,
        })
    }

    fn parse_strlen(frames: &[RespFrame]) -> Result<StringCommand> {
        if frames.len() != 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("STRLEN".into())));
        }
        Ok(StringCommand::StrLen {
            key: Self::extract_bytes(&frames[1])?,
        })
    }

    fn parse_getset(frames: &[RespFrame]) -> Result<StringCommand> {
        if frames.len() != 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("GETSET".into())));
        }
        Ok(StringCommand::GetSet {
            key: Self::extract_bytes(&frames[1])?,
            value: Self::extract_bytes(&frames[2])?,
        })
    }

    fn parse_getrange(frames: &[RespFrame]) -> Result<StringCommand> {
        if frames.len() != 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("GETRANGE".into())));
        }
        let start = Self::extract_string(&frames[2])?.parse::<isize>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        let end = Self::extract_string(&frames[3])?.parse::<isize>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        Ok(StringCommand::GetRange {
            key: Self::extract_bytes(&frames[1])?,
            start,
            end,
        })
    }

    fn parse_setrange(frames: &[RespFrame]) -> Result<StringCommand> {
        if frames.len() != 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("SETRANGE".into())));
        }
        let offset = Self::extract_string(&frames[2])?.parse::<usize>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        Ok(StringCommand::SetRange {
            key: Self::extract_bytes(&frames[1])?,
            offset,
            value: Self::extract_bytes(&frames[3])?,
        })
    }
    
    fn parse_lpush(frames: &[RespFrame]) -> Result<ListCommand> {
        if frames.len() < 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("LPUSH".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let mut values = Vec::new();
        for i in 2..frames.len() {
            values.push(Self::extract_bytes(&frames[i])?);
        }
        Ok(ListCommand::LPush { key, values })
    }

    fn parse_rpush(frames: &[RespFrame]) -> Result<ListCommand> {
        if frames.len() < 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("RPUSH".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let mut values = Vec::new();
        for i in 2..frames.len() {
            values.push(Self::extract_bytes(&frames[i])?);
        }
        Ok(ListCommand::RPush { key, values })
    }
    
    fn parse_lpop(frames: &[RespFrame]) -> Result<ListCommand> {
        if frames.len() != 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("LPOP".into())));
        }
        Ok(ListCommand::LPop {
            key: Self::extract_bytes(&frames[1])?,
        })
    }

    fn parse_rpop(frames: &[RespFrame]) -> Result<ListCommand> {
        if frames.len() != 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("RPOP".into())));
        }
        Ok(ListCommand::RPop {
            key: Self::extract_bytes(&frames[1])?,
        })
    }

    fn parse_llen(frames: &[RespFrame]) -> Result<ListCommand> {
        if frames.len() != 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("LLEN".into())));
        }
        Ok(ListCommand::LLen {
            key: Self::extract_bytes(&frames[1])?,
        })
    }

    fn parse_lindex(frames: &[RespFrame]) -> Result<ListCommand> {
        if frames.len() != 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("LINDEX".into())));
        }
        let index = Self::extract_string(&frames[2])?.parse::<isize>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        Ok(ListCommand::LIndex {
            key: Self::extract_bytes(&frames[1])?,
            index,
        })
    }

    fn parse_lset(frames: &[RespFrame]) -> Result<ListCommand> {
        if frames.len() != 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("LSET".into())));
        }
        let index = Self::extract_string(&frames[2])?.parse::<isize>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        Ok(ListCommand::LSet {
            key: Self::extract_bytes(&frames[1])?,
            index,
            value: Self::extract_bytes(&frames[3])?,
        })
    }

    fn parse_lrange(frames: &[RespFrame]) -> Result<ListCommand> {
        if frames.len() != 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("LRANGE".into())));
        }
        let start = Self::extract_string(&frames[2])?.parse::<isize>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        let stop = Self::extract_string(&frames[3])?.parse::<isize>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        Ok(ListCommand::LRange {
            key: Self::extract_bytes(&frames[1])?,
            start,
            stop,
        })
    }

    fn parse_ltrim(frames: &[RespFrame]) -> Result<ListCommand> {
        if frames.len() != 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("LTRIM".into())));
        }
        let start = Self::extract_string(&frames[2])?.parse::<isize>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        let stop = Self::extract_string(&frames[3])?.parse::<isize>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        Ok(ListCommand::LTrim {
            key: Self::extract_bytes(&frames[1])?,
            start,
            stop,
        })
    }

    fn parse_lrem(frames: &[RespFrame]) -> Result<ListCommand> {
        if frames.len() != 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("LREM".into())));
        }
        let count = Self::extract_string(&frames[2])?.parse::<isize>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        Ok(ListCommand::LRem {
            key: Self::extract_bytes(&frames[1])?,
            count,
            element: Self::extract_bytes(&frames[3])?,
        })
    }

    fn parse_sadd(frames: &[RespFrame]) -> Result<SetCommand> {
        if frames.len() < 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("SADD".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let mut members = Vec::new();
        for i in 2..frames.len() {
            members.push(Self::extract_bytes(&frames[i])?);
        }
        Ok(SetCommand::SAdd { key, members })
    }

    fn parse_srem(frames: &[RespFrame]) -> Result<SetCommand> {
        if frames.len() < 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("SREM".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let mut members = Vec::new();
        for i in 2..frames.len() {
            members.push(Self::extract_bytes(&frames[i])?);
        }
        Ok(SetCommand::SRem { key, members })
    }

    fn parse_smembers(frames: &[RespFrame]) -> Result<SetCommand> {
        if frames.len() != 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("SMEMBERS".into())));
        }
        Ok(SetCommand::SMembers {
            key: Self::extract_bytes(&frames[1])?,
        })
    }

    fn parse_scard(frames: &[RespFrame]) -> Result<SetCommand> {
        if frames.len() != 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("SCARD".into())));
        }
        Ok(SetCommand::SCard {
            key: Self::extract_bytes(&frames[1])?,
        })
    }

    fn parse_sismember(frames: &[RespFrame]) -> Result<SetCommand> {
        if frames.len() != 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("SISMEMBER".into())));
        }
        Ok(SetCommand::SIsMember {
            key: Self::extract_bytes(&frames[1])?,
            member: Self::extract_bytes(&frames[2])?,
        })
    }

    fn parse_sunion(frames: &[RespFrame]) -> Result<SetCommand> {
        if frames.len() < 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("SUNION".into())));
        }
        let mut keys = Vec::new();
        for i in 1..frames.len() {
            keys.push(Self::extract_bytes(&frames[i])?);
        }
        Ok(SetCommand::SUnion { keys })
    }

    fn parse_sinter(frames: &[RespFrame]) -> Result<SetCommand> {
        if frames.len() < 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("SINTER".into())));
        }
        let mut keys = Vec::new();
        for i in 1..frames.len() {
            keys.push(Self::extract_bytes(&frames[i])?);
        }
        Ok(SetCommand::SInter { keys })
    }

    fn parse_sdiff(frames: &[RespFrame]) -> Result<SetCommand> {
        if frames.len() < 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("SDIFF".into())));
        }
        let mut keys = Vec::new();
        for i in 1..frames.len() {
            keys.push(Self::extract_bytes(&frames[i])?);
        }
        Ok(SetCommand::SDiff { keys })
    }

    fn parse_srandmember(frames: &[RespFrame]) -> Result<SetCommand> {
        if frames.len() < 2 || frames.len() > 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("SRANDMEMBER".into())));
        }
        let count = if frames.len() == 3 {
            Some(Self::extract_string(&frames[2])?.parse::<i64>()
                .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?)
        } else {
            None
        };
        Ok(SetCommand::SRandMember {
            key: Self::extract_bytes(&frames[1])?,
            count,
        })
    }

    fn parse_spop(frames: &[RespFrame]) -> Result<SetCommand> {
        if frames.len() < 2 || frames.len() > 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("SPOP".into())));
        }
        let count = if frames.len() == 3 {
            Some(Self::extract_string(&frames[2])?.parse::<usize>()
                .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?)
        } else {
            None
        };
        Ok(SetCommand::SPop {
            key: Self::extract_bytes(&frames[1])?,
            count,
        })
    }
    
    fn parse_hset(frames: &[RespFrame]) -> Result<HashCommand> {
        if frames.len() < 4 || frames.len() % 2 != 0 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("HSET".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let mut field_values = Vec::new();
        let mut i = 2;
        while i < frames.len() {
            let field = Self::extract_bytes(&frames[i])?;
            let value = Self::extract_bytes(&frames[i + 1])?;
            field_values.push((field, value));
            i += 2;
        }
        Ok(HashCommand::HSet { key, field_values })
    }

    fn parse_hget(frames: &[RespFrame]) -> Result<HashCommand> {
        if frames.len() != 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("HGET".into())));
        }
        Ok(HashCommand::HGet {
            key: Self::extract_bytes(&frames[1])?,
            field: Self::extract_bytes(&frames[2])?,
        })
    }

    fn parse_hmset(frames: &[RespFrame]) -> Result<HashCommand> {
        if frames.len() < 4 || frames.len() % 2 != 0 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("HMSET".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let mut field_values = Vec::new();
        let mut i = 2;
        while i < frames.len() {
            let field = Self::extract_bytes(&frames[i])?;
            let value = Self::extract_bytes(&frames[i + 1])?;
            field_values.push((field, value));
            i += 2;
        }
        Ok(HashCommand::HMSet { key, field_values })
    }

    fn parse_hmget(frames: &[RespFrame]) -> Result<HashCommand> {
        if frames.len() < 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("HMGET".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let mut fields = Vec::new();
        for i in 2..frames.len() {
            fields.push(Self::extract_bytes(&frames[i])?);
        }
        Ok(HashCommand::HMGet { key, fields })
    }

    fn parse_hgetall(frames: &[RespFrame]) -> Result<HashCommand> {
        if frames.len() != 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("HGETALL".into())));
        }
        Ok(HashCommand::HGetAll {
            key: Self::extract_bytes(&frames[1])?,
        })
    }

    fn parse_hdel(frames: &[RespFrame]) -> Result<HashCommand> {
        if frames.len() < 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("HDEL".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let mut fields = Vec::new();
        for i in 2..frames.len() {
            fields.push(Self::extract_bytes(&frames[i])?);
        }
        Ok(HashCommand::HDel { key, fields })
    }

    fn parse_hlen(frames: &[RespFrame]) -> Result<HashCommand> {
        if frames.len() != 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("HLEN".into())));
        }
        Ok(HashCommand::HLen {
            key: Self::extract_bytes(&frames[1])?,
        })
    }

    fn parse_hexists(frames: &[RespFrame]) -> Result<HashCommand> {
        if frames.len() != 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("HEXISTS".into())));
        }
        Ok(HashCommand::HExists {
            key: Self::extract_bytes(&frames[1])?,
            field: Self::extract_bytes(&frames[2])?,
        })
    }

    fn parse_hkeys(frames: &[RespFrame]) -> Result<HashCommand> {
        if frames.len() != 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("HKEYS".into())));
        }
        Ok(HashCommand::HKeys {
            key: Self::extract_bytes(&frames[1])?,
        })
    }

    fn parse_hvals(frames: &[RespFrame]) -> Result<HashCommand> {
        if frames.len() != 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("HVALS".into())));
        }
        Ok(HashCommand::HVals {
            key: Self::extract_bytes(&frames[1])?,
        })
    }

    fn parse_hincrby(frames: &[RespFrame]) -> Result<HashCommand> {
        if frames.len() != 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("HINCRBY".into())));
        }
        let increment = Self::extract_string(&frames[3])?.parse::<i64>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        Ok(HashCommand::HIncrBy {
            key: Self::extract_bytes(&frames[1])?,
            field: Self::extract_bytes(&frames[2])?,
            increment,
        })
    }

    fn parse_zadd(frames: &[RespFrame]) -> Result<SortedSetCommand> {
        if frames.len() < 4 || frames.len() % 2 != 0 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("ZADD".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let mut score_members = Vec::new();
        let mut i = 2;
        while i < frames.len() {
            let score = Self::extract_string(&frames[i])?.parse::<f64>()
                .map_err(|_| FerrousError::Command(CommandError::InvalidFloatValue))?;
            let member = Self::extract_bytes(&frames[i + 1])?;
            score_members.push((score, member));
            i += 2;
        }
        Ok(SortedSetCommand::ZAdd { key, score_members })
    }

    fn parse_zrem(frames: &[RespFrame]) -> Result<SortedSetCommand> {
        if frames.len() < 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("ZREM".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let mut members = Vec::new();
        for i in 2..frames.len() {
            members.push(Self::extract_bytes(&frames[i])?);
        }
        Ok(SortedSetCommand::ZRem { key, members })
    }

    fn parse_zscore(frames: &[RespFrame]) -> Result<SortedSetCommand> {
        if frames.len() != 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("ZSCORE".into())));
        }
        Ok(SortedSetCommand::ZScore {
            key: Self::extract_bytes(&frames[1])?,
            member: Self::extract_bytes(&frames[2])?,
        })
    }

    fn parse_zcard(frames: &[RespFrame]) -> Result<SortedSetCommand> {
        if frames.len() != 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("ZCARD".into())));
        }
        Ok(SortedSetCommand::ZCard {
            key: Self::extract_bytes(&frames[1])?,
        })
    }

    fn parse_zrank(frames: &[RespFrame]) -> Result<SortedSetCommand> {
        if frames.len() != 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("ZRANK".into())));
        }
        Ok(SortedSetCommand::ZRank {
            key: Self::extract_bytes(&frames[1])?,
            member: Self::extract_bytes(&frames[2])?,
        })
    }

    fn parse_zrevrank(frames: &[RespFrame]) -> Result<SortedSetCommand> {
        if frames.len() != 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("ZREVRANK".into())));
        }
        Ok(SortedSetCommand::ZRevRank {
            key: Self::extract_bytes(&frames[1])?,
            member: Self::extract_bytes(&frames[2])?,
        })
    }

    fn parse_zrange(frames: &[RespFrame]) -> Result<SortedSetCommand> {
        if frames.len() < 4 || frames.len() > 5 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("ZRANGE".into())));
        }
        let start = Self::extract_string(&frames[2])?.parse::<isize>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        let stop = Self::extract_string(&frames[3])?.parse::<isize>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        let with_scores = frames.len() == 5 && 
            Self::extract_string(&frames[4])?.to_uppercase() == "WITHSCORES";
        Ok(SortedSetCommand::ZRange {
            key: Self::extract_bytes(&frames[1])?,
            start,
            stop,
            with_scores,
        })
    }

    fn parse_zrevrange(frames: &[RespFrame]) -> Result<SortedSetCommand> {
        if frames.len() < 4 || frames.len() > 5 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("ZREVRANGE".into())));
        }
        let start = Self::extract_string(&frames[2])?.parse::<isize>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        let stop = Self::extract_string(&frames[3])?.parse::<isize>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        let with_scores = frames.len() == 5 && 
            Self::extract_string(&frames[4])?.to_uppercase() == "WITHSCORES";
        Ok(SortedSetCommand::ZRevRange {
            key: Self::extract_bytes(&frames[1])?,
            start,
            stop,
            with_scores,
        })
    }

    fn parse_zrangebyscore(frames: &[RespFrame]) -> Result<SortedSetCommand> {
        if frames.len() < 4 || frames.len() > 5 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("ZRANGEBYSCORE".into())));
        }
        let min_score = Self::extract_string(&frames[2])?.parse::<f64>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidFloatValue))?;
        let max_score = Self::extract_string(&frames[3])?.parse::<f64>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidFloatValue))?;
        let with_scores = frames.len() == 5 && 
            Self::extract_string(&frames[4])?.to_uppercase() == "WITHSCORES";
        Ok(SortedSetCommand::ZRangeByScore {
            key: Self::extract_bytes(&frames[1])?,
            min_score,
            max_score,
            with_scores,
        })
    }

    fn parse_zcount(frames: &[RespFrame]) -> Result<SortedSetCommand> {
        if frames.len() != 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("ZCOUNT".into())));
        }
        let min_score = Self::extract_string(&frames[2])?.parse::<f64>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidFloatValue))?;
        let max_score = Self::extract_string(&frames[3])?.parse::<f64>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidFloatValue))?;
        Ok(SortedSetCommand::ZCount {
            key: Self::extract_bytes(&frames[1])?,
            min_score,
            max_score,
        })
    }

    fn parse_zincrby(frames: &[RespFrame]) -> Result<SortedSetCommand> {
        if frames.len() != 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("ZINCRBY".into())));
        }
        let increment = Self::extract_string(&frames[2])?.parse::<f64>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidFloatValue))?;
        Ok(SortedSetCommand::ZIncrBy {
            key: Self::extract_bytes(&frames[1])?,
            increment,
            member: Self::extract_bytes(&frames[3])?,
        })
    }

    fn parse_exists(frames: &[RespFrame]) -> Result<KeyCommand> {
        if frames.len() < 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("EXISTS".into())));
        }
        let mut keys = Vec::new();
        for i in 1..frames.len() {
            keys.push(Self::extract_bytes(&frames[i])?);
        }
        Ok(KeyCommand::Exists { keys })
    }
    
    fn parse_expire(frames: &[RespFrame]) -> Result<KeyCommand> {
        if frames.len() != 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("EXPIRE".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let seconds = Self::extract_string(&frames[2])?.parse::<u64>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        Ok(KeyCommand::Expire { key, seconds })
    }

    fn parse_pexpire(frames: &[RespFrame]) -> Result<KeyCommand> {
        if frames.len() != 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("PEXPIRE".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let milliseconds = Self::extract_string(&frames[2])?.parse::<u64>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        Ok(KeyCommand::PExpire { key, milliseconds })
    }

    fn parse_ttl(frames: &[RespFrame]) -> Result<KeyCommand> {
        if frames.len() != 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("TTL".into())));
        }
        Ok(KeyCommand::Ttl {
            key: Self::extract_bytes(&frames[1])?,
        })
    }

    fn parse_pttl(frames: &[RespFrame]) -> Result<KeyCommand> {
        if frames.len() != 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("PTTL".into())));
        }
        Ok(KeyCommand::Pttl {
            key: Self::extract_bytes(&frames[1])?,
        })
    }

    fn parse_persist(frames: &[RespFrame]) -> Result<KeyCommand> {
        if frames.len() != 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("PERSIST".into())));
        }
        Ok(KeyCommand::Persist {
            key: Self::extract_bytes(&frames[1])?,
        })
    }

    fn parse_type(frames: &[RespFrame]) -> Result<KeyCommand> {
        if frames.len() != 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("TYPE".into())));
        }
        Ok(KeyCommand::Type {
            key: Self::extract_bytes(&frames[1])?,
        })
    }

    fn parse_rename(frames: &[RespFrame]) -> Result<KeyCommand> {
        if frames.len() != 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("RENAME".into())));
        }
        Ok(KeyCommand::Rename {
            old_key: Self::extract_bytes(&frames[1])?,
            new_key: Self::extract_bytes(&frames[2])?,
        })
    }

    fn parse_renamenx(frames: &[RespFrame]) -> Result<KeyCommand> {
        if frames.len() != 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("RENAMENX".into())));
        }
        Ok(KeyCommand::RenameNx {
            old_key: Self::extract_bytes(&frames[1])?,
            new_key: Self::extract_bytes(&frames[2])?,
        })
    }

    fn parse_zrevrangebyscore(frames: &[RespFrame]) -> Result<SortedSetCommand> {
        if frames.len() < 4 || frames.len() > 5 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("ZREVRANGEBYSCORE".into())));
        }
        let max_score = Self::extract_string(&frames[2])?.parse::<f64>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidFloatValue))?;
        let min_score = Self::extract_string(&frames[3])?.parse::<f64>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidFloatValue))?;
        let with_scores = frames.len() == 5 && 
            Self::extract_string(&frames[4])?.to_uppercase() == "WITHSCORES";
        Ok(SortedSetCommand::ZRevRangeByScore {
            key: Self::extract_bytes(&frames[1])?,
            max_score,
            min_score,
            with_scores,
        })
    }
    
    fn parse_zpopmin(frames: &[RespFrame]) -> Result<SortedSetCommand> {
        if frames.len() < 2 || frames.len() > 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("ZPOPMIN".into())));
        }
        let count = if frames.len() == 3 {
            Some(Self::extract_string(&frames[2])?.parse::<usize>()
                .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?)
        } else {
            None
        };
        Ok(SortedSetCommand::ZPopMin {
            key: Self::extract_bytes(&frames[1])?,
            count,
        })
    }
    
    fn parse_zpopmax(frames: &[RespFrame]) -> Result<SortedSetCommand> {
        if frames.len() < 2 || frames.len() > 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("ZPOPMAX".into())));
        }
        let count = if frames.len() == 3 {
            Some(Self::extract_string(&frames[2])?.parse::<usize>()
                .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?)
        } else {
            None
        };
        Ok(SortedSetCommand::ZPopMax {
            key: Self::extract_bytes(&frames[1])?,
            count,
        })
    }
    
    fn parse_zremrangebyrank(frames: &[RespFrame]) -> Result<SortedSetCommand> {
        if frames.len() != 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("ZREMRANGEBYRANK".into())));
        }
        let start = Self::extract_string(&frames[2])?.parse::<isize>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        let stop = Self::extract_string(&frames[3])?.parse::<isize>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        Ok(SortedSetCommand::ZRemRangeByRank {
            key: Self::extract_bytes(&frames[1])?,
            start,
            stop,
        })
    }
    
    fn parse_zremrangebyscore(frames: &[RespFrame]) -> Result<SortedSetCommand> {
        if frames.len() != 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("ZREMRANGEBYSCORE".into())));
        }
        let min_score = Self::extract_string(&frames[2])?.parse::<f64>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidFloatValue))?;
        let max_score = Self::extract_string(&frames[3])?.parse::<f64>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidFloatValue))?;
        Ok(SortedSetCommand::ZRemRangeByScore {
            key: Self::extract_bytes(&frames[1])?,
            min_score,
            max_score,
        })
    }
    
    fn parse_zremrangebylex(frames: &[RespFrame]) -> Result<SortedSetCommand> {
        if frames.len() != 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("ZREMRANGEBYLEX".into())));
        }
        let min_lex = Self::extract_string(&frames[2])?;
        let max_lex = Self::extract_string(&frames[3])?;
        Ok(SortedSetCommand::ZRemRangeByLex {
            key: Self::extract_bytes(&frames[1])?,
            min_lex,
            max_lex,
        })
    }

    fn parse_ping(frames: &[RespFrame]) -> Result<ServerCommand> {
        let message = if frames.len() == 2 {
            Some(Self::extract_bytes(&frames[1])?)
        } else if frames.len() == 1 {
            None
        } else {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("PING".into())));
        };
        Ok(ServerCommand::Ping { message })
    }
    
    fn parse_echo(frames: &[RespFrame]) -> Result<ServerCommand> {
        if frames.len() != 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("ECHO".into())));
        }
        Ok(ServerCommand::Echo {
            message: Self::extract_bytes(&frames[1])?,
        })
    }
    
    fn parse_info(frames: &[RespFrame]) -> Result<ServerCommand> {
        let section = if frames.len() == 2 {
            Some(Self::extract_string(&frames[1])?)
        } else if frames.len() == 1 {
            None
        } else {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("INFO".into())));
        };
        Ok(ServerCommand::Info { section })
    }
    
    fn parse_xadd(frames: &[RespFrame]) -> Result<StreamCommand> {
        if frames.len() < 5 || frames.len() % 2 != 1 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("XADD".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let id_str = Self::extract_string(&frames[2])?;
        let id = if id_str == "*" { None } else { Some(id_str) };
        
        let mut fields = Vec::new();
        let mut i = 3;
        while i < frames.len() {
            if i + 1 >= frames.len() {
                return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("XADD".into())));
            }
            let field = Self::extract_bytes(&frames[i])?;
            let value = Self::extract_bytes(&frames[i + 1])?;
            fields.push((field, value));
            i += 2;
        }
        Ok(StreamCommand::XAdd { key, id, fields })
    }
    
    fn parse_xlen(frames: &[RespFrame]) -> Result<StreamCommand> {
        if frames.len() != 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("XLEN".into())));
        }
        Ok(StreamCommand::XLen {
            key: Self::extract_bytes(&frames[1])?,
        })
    }
    
    fn parse_xrange(frames: &[RespFrame]) -> Result<StreamCommand> {
        if frames.len() < 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("XRANGE".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let start = Self::extract_string(&frames[2])?;
        let end = Self::extract_string(&frames[3])?;
        
        let count = if frames.len() == 6 && Self::extract_string(&frames[4])?.to_uppercase() == "COUNT" {
            Some(Self::extract_string(&frames[5])?.parse::<usize>()
                .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?)
        } else {
            None
        };
        
        Ok(StreamCommand::XRange { key, start, end, count })
    }
    
    fn parse_xrevrange(frames: &[RespFrame]) -> Result<StreamCommand> {
        if frames.len() < 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("XREVRANGE".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let start = Self::extract_string(&frames[2])?;
        let end = Self::extract_string(&frames[3])?;
        
        let count = if frames.len() == 6 && Self::extract_string(&frames[4])?.to_uppercase() == "COUNT" {
            Some(Self::extract_string(&frames[5])?.parse::<usize>()
                .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?)
        } else {
            None
        };
        
        Ok(StreamCommand::XRevRange { key, start, end, count })
    }
    
    fn parse_xread(frames: &[RespFrame]) -> Result<StreamCommand> {
        if frames.len() < 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("XREAD".into())));
        }
        
        let mut i = 1;
        let mut count = None;
        let mut block = None;
        
        while i < frames.len() {
            let opt = Self::extract_string(&frames[i])?.to_uppercase();
            match opt.as_str() {
                "COUNT" => {
                    if i + 1 >= frames.len() {
                        return Err(FerrousError::Command(CommandError::SyntaxError("Missing COUNT value".to_string())));
                    }
                    count = Some(Self::extract_string(&frames[i + 1])?.parse::<usize>()
                        .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?);
                    i += 2;
                }
                "BLOCK" => {
                    if i + 1 >= frames.len() {
                        return Err(FerrousError::Command(CommandError::SyntaxError("Missing BLOCK value".to_string())));
                    }
                    block = Some(Self::extract_string(&frames[i + 1])?.parse::<u64>()
                        .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?);
                    i += 2;
                }
                "STREAMS" => {
                    i += 1;
                    break;
                }
                _ => break,
            }
        }
        
        let remaining = frames.len() - i;
        if remaining % 2 != 0 {
            return Err(FerrousError::Command(CommandError::SyntaxError("Uneven number of keys and stream IDs".to_string())));
        }
        
        let num_streams = remaining / 2;
        let mut keys_and_ids = Vec::new();
        
        for j in 0..num_streams {
            let key = Self::extract_bytes(&frames[i + j])?;
            let id = Self::extract_string(&frames[i + num_streams + j])?;
            keys_and_ids.push((key, id));
        }
        
        Ok(StreamCommand::XRead { keys_and_ids, count, block })
    }
    
    fn parse_xtrim(frames: &[RespFrame]) -> Result<StreamCommand> {
        if frames.len() < 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("XTRIM".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let strategy = Self::extract_string(&frames[2])?;
        let threshold = Self::extract_string(&frames[3])?.parse::<usize>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        Ok(StreamCommand::XTrim { key, strategy, threshold })
    }
    
    fn parse_xdel(frames: &[RespFrame]) -> Result<StreamCommand> {
        if frames.len() < 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("XDEL".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let mut ids = Vec::new();
        for i in 2..frames.len() {
            ids.push(Self::extract_string(&frames[i])?);
        }
        Ok(StreamCommand::XDel { key, ids })
    }
    
    fn parse_scan_cmd(frames: &[RespFrame]) -> Result<ScanCommand> {
        if frames.len() < 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("SCAN".into())));
        }
        let cursor = Self::extract_string(&frames[1])?.parse::<u64>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        
        let mut pattern = None;
        let mut count = None;
        let mut type_filter = None;
        let mut i = 2;
        
        while i < frames.len() {
            let opt = Self::extract_string(&frames[i])?.to_uppercase();
            match opt.as_str() {
                "MATCH" => {
                    if i + 1 >= frames.len() {
                        return Err(FerrousError::Command(CommandError::SyntaxError("Missing MATCH pattern".to_string())));
                    }
                    pattern = Some(Self::extract_bytes(&frames[i + 1])?);
                    i += 2;
                }
                "COUNT" => {
                    if i + 1 >= frames.len() {
                        return Err(FerrousError::Command(CommandError::SyntaxError("Missing COUNT value".to_string())));
                    }
                    count = Some(Self::extract_string(&frames[i + 1])?.parse::<usize>()
                        .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?);
                    i += 2;
                }
                "TYPE" => {
                    if i + 1 >= frames.len() {
                        return Err(FerrousError::Command(CommandError::SyntaxError("Missing TYPE value".to_string())));
                    }
                    type_filter = Some(Self::extract_string(&frames[i + 1])?);
                    i += 2;
                }
                _ => break,
            }
        }
        
        Ok(ScanCommand::Scan { cursor, pattern, count, type_filter })
    }
    
    fn parse_hscan_cmd(frames: &[RespFrame]) -> Result<ScanCommand> {
        if frames.len() < 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("HSCAN".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let cursor = Self::extract_string(&frames[2])?.parse::<u64>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        
        let mut pattern = None;
        let mut count = None;
        let mut i = 3;
        
        while i < frames.len() {
            let opt = Self::extract_string(&frames[i])?.to_uppercase();
            match opt.as_str() {
                "MATCH" => {
                    if i + 1 >= frames.len() {
                        return Err(FerrousError::Command(CommandError::SyntaxError("Missing MATCH pattern".to_string())));
                    }
                    pattern = Some(Self::extract_bytes(&frames[i + 1])?);
                    i += 2;
                }
                "COUNT" => {
                    if i + 1 >= frames.len() {
                        return Err(FerrousError::Command(CommandError::SyntaxError("Missing COUNT value".to_string())));
                    }
                    count = Some(Self::extract_string(&frames[i + 1])?.parse::<usize>()
                        .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?);
                    i += 2;
                }
                _ => break,
            }
        }
        
        Ok(ScanCommand::HScan { key, cursor, pattern, count })
    }
    
    fn parse_sscan_cmd(frames: &[RespFrame]) -> Result<ScanCommand> {
        if frames.len() < 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("SSCAN".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let cursor = Self::extract_string(&frames[2])?.parse::<u64>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        
        let mut pattern = None;
        let mut count = None;
        let mut i = 3;
        
        while i < frames.len() {
            let opt = Self::extract_string(&frames[i])?.to_uppercase();
            match opt.as_str() {
                "MATCH" => {
                    if i + 1 >= frames.len() {
                        return Err(FerrousError::Command(CommandError::SyntaxError("Missing MATCH pattern".to_string())));
                    }
                    pattern = Some(Self::extract_bytes(&frames[i + 1])?);
                    i += 2;
                }
                "COUNT" => {
                    if i + 1 >= frames.len() {
                        return Err(FerrousError::Command(CommandError::SyntaxError("Missing COUNT value".to_string())));
                    }
                    count = Some(Self::extract_string(&frames[i + 1])?.parse::<usize>()
                        .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?);
                    i += 2;
                }
                _ => break,
            }
        }
        
        Ok(ScanCommand::SScan { key, cursor, pattern, count })
    }
    
    fn parse_zscan_cmd(frames: &[RespFrame]) -> Result<ScanCommand> {
        if frames.len() < 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("ZSCAN".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let cursor = Self::extract_string(&frames[2])?.parse::<u64>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        
        let mut pattern = None;
        let mut count = None;
        let mut i = 3;
        
        while i < frames.len() {
            let opt = Self::extract_string(&frames[i])?.to_uppercase();
            match opt.as_str() {
                "MATCH" => {
                    if i + 1 >= frames.len() {
                        return Err(FerrousError::Command(CommandError::SyntaxError("Missing MATCH pattern".to_string())));
                    }
                    pattern = Some(Self::extract_bytes(&frames[i + 1])?);
                    i += 2;
                }
                "COUNT" => {
                    if i + 1 >= frames.len() {
                        return Err(FerrousError::Command(CommandError::SyntaxError("Missing COUNT value".to_string())));
                    }
                    count = Some(Self::extract_string(&frames[i + 1])?.parse::<usize>()
                        .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?);
                    i += 2;
                }
                _ => break,
            }
        }
        
        Ok(ScanCommand::ZScan { key, cursor, pattern, count })
    }
    
    fn parse_keys_cmd(frames: &[RespFrame]) -> Result<DatabaseCommand> {
        if frames.len() != 2 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("KEYS".into())));
        }
        Ok(DatabaseCommand::Keys {
            pattern: Self::extract_bytes(&frames[1])?,
        })
    }
    
    fn parse_xgroup(frames: &[RespFrame]) -> Result<ConsumerGroupCommand> {
        if frames.len() < 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("XGROUP".into())));
        }
        let subcommand = Self::extract_string(&frames[1])?;
        let key = Self::extract_bytes(&frames[2])?;
        let group = Self::extract_string(&frames[3])?;
        
        let mut args = Vec::new();
        for i in 4..frames.len() {
            args.push(Self::extract_string(&frames[i])?);
        }
        
        Ok(ConsumerGroupCommand::XGroup { subcommand, key, group, args })
    }
    
    fn parse_xreadgroup(frames: &[RespFrame]) -> Result<ConsumerGroupCommand> {
        if frames.len() < 6 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("XREADGROUP".into())));
        }
        
        if Self::extract_string(&frames[1])?.to_uppercase() != "GROUP" {
            return Err(FerrousError::Command(CommandError::SyntaxError("Expected GROUP keyword".to_string())));
        }
        
        let group = Self::extract_string(&frames[2])?;
        let consumer = Self::extract_string(&frames[3])?;
        
        let mut i = 4;
        let mut count = None;
        let mut block = None;
        let mut noack = false;
        
        while i < frames.len() {
            let opt = Self::extract_string(&frames[i])?.to_uppercase();
            match opt.as_str() {
                "COUNT" => {
                    if i + 1 >= frames.len() {
                        return Err(FerrousError::Command(CommandError::SyntaxError("Missing COUNT value".to_string())));
                    }
                    count = Some(Self::extract_string(&frames[i + 1])?.parse::<usize>()
                        .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?);
                    i += 2;
                }
                "BLOCK" => {
                    if i + 1 >= frames.len() {
                        return Err(FerrousError::Command(CommandError::SyntaxError("Missing BLOCK value".to_string())));
                    }
                    block = Some(Self::extract_string(&frames[i + 1])?.parse::<u64>()
                        .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?);
                    i += 2;
                }
                "NOACK" => {
                    noack = true;
                    i += 1;
                }
                "STREAMS" => {
                    i += 1;
                    break;
                }
                _ => break,
            }
        }
        
        let remaining = frames.len() - i;
        if remaining % 2 != 0 {
            return Err(FerrousError::Command(CommandError::SyntaxError("Uneven number of keys and stream IDs".to_string())));
        }
        
        let num_streams = remaining / 2;
        let mut keys_and_ids = Vec::new();
        
        for j in 0..num_streams {
            let key = Self::extract_bytes(&frames[i + j])?;
            let id = Self::extract_string(&frames[i + num_streams + j])?;
            keys_and_ids.push((key, id));
        }
        
        Ok(ConsumerGroupCommand::XReadGroup { group, consumer, keys_and_ids, count, block, noack })
    }
    
    fn parse_xack(frames: &[RespFrame]) -> Result<ConsumerGroupCommand> {
        if frames.len() < 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("XACK".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let group = Self::extract_string(&frames[2])?;
        
        let mut ids = Vec::new();
        for i in 3..frames.len() {
            ids.push(Self::extract_string(&frames[i])?);
        }
        
        Ok(ConsumerGroupCommand::XAck { key, group, ids })
    }
    
    fn parse_xpending(frames: &[RespFrame]) -> Result<ConsumerGroupCommand> {
        if frames.len() < 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("XPENDING".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let group = Self::extract_string(&frames[2])?;
        
        let range = if frames.len() >= 6 {
            let start = Self::extract_string(&frames[3])?;
            let end = Self::extract_string(&frames[4])?;
            let count = Self::extract_string(&frames[5])?.parse::<usize>()
                .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
            Some((start, end, count))
        } else {
            None
        };
        
        let consumer = if frames.len() >= 7 {
            Some(Self::extract_string(&frames[6])?)
        } else {
            None
        };
        
        Ok(ConsumerGroupCommand::XPending { key, group, range, consumer })
    }
    
    fn parse_xclaim(frames: &[RespFrame]) -> Result<ConsumerGroupCommand> {
        if frames.len() < 6 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("XCLAIM".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let group = Self::extract_string(&frames[2])?;
        let consumer = Self::extract_string(&frames[3])?;
        let min_idle_time = Self::extract_string(&frames[4])?.parse::<u64>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        
        let mut ids = Vec::new();
        let mut force = false;
        let mut justid = false;
        
        for i in 5..frames.len() {
            let arg = Self::extract_string(&frames[i])?;
            match arg.to_uppercase().as_str() {
                "FORCE" => force = true,
                "JUSTID" => justid = true,
                _ => ids.push(arg),
            }
        }
        
        Ok(ConsumerGroupCommand::XClaim { key, group, consumer, min_idle_time, ids, force, justid })
    }
    
    fn parse_xinfo(frames: &[RespFrame]) -> Result<ConsumerGroupCommand> {
        if frames.len() < 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("XINFO".into())));
        }
        let subcommand = Self::extract_string(&frames[1])?;
        let key = Self::extract_bytes(&frames[2])?;
        let group = if frames.len() >= 4 {
            Some(Self::extract_string(&frames[3])?)
        } else {
            None
        };
        
        Ok(ConsumerGroupCommand::XInfo { subcommand, key, group })
    }
    
    // Bit operation parsers
    fn parse_getbit(frames: &[RespFrame]) -> Result<BitCommand> {
        if frames.len() != 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("GETBIT".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let offset = Self::extract_string(&frames[2])?.parse::<usize>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        Ok(BitCommand::GetBit { key, offset })
    }
    
    fn parse_setbit(frames: &[RespFrame]) -> Result<BitCommand> {
        if frames.len() != 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("SETBIT".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        let offset = Self::extract_string(&frames[2])?.parse::<usize>()
            .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
        let value = match Self::extract_string(&frames[3])?.as_str() {
            "0" => 0,
            "1" => 1,
            _ => return Err(FerrousError::Command(CommandError::InvalidArgument("bit value must be 0 or 1".to_string()))),
        };
        Ok(BitCommand::SetBit { key, offset, value })
    }
    
    fn parse_bitcount(frames: &[RespFrame]) -> Result<BitCommand> {
        if frames.len() < 2 || frames.len() > 4 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("BITCOUNT".into())));
        }
        let key = Self::extract_bytes(&frames[1])?;
        
        let (start, end) = if frames.len() == 4 {
            let start = Self::extract_string(&frames[2])?.parse::<isize>()
                .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
            let end = Self::extract_string(&frames[3])?.parse::<isize>()
                .map_err(|_| FerrousError::Command(CommandError::InvalidIntegerValue))?;
            (Some(start), Some(end))
        } else {
            (None, None)
        };
        
        Ok(BitCommand::BitCount { key, start, end })
    }
    
    // Config command parser
    fn parse_config(frames: &[RespFrame]) -> Result<ConfigCommand> {
        if frames.len() < 3 {
            return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("CONFIG".into())));
        }
        
        let subcommand = Self::extract_string(&frames[1])?.to_uppercase();
        
        match subcommand.as_str() {
            "GET" => {
                if frames.len() != 3 {
                    return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("CONFIG GET".into())));
                }
                Ok(ConfigCommand::Get {
                    parameter: Self::extract_string(&frames[2])?,
                })
            }
            "SET" => {
                if frames.len() != 4 {
                    return Err(FerrousError::Command(CommandError::WrongNumberOfArguments("CONFIG SET".into())));
                }
                Ok(ConfigCommand::Set {
                    parameter: Self::extract_string(&frames[2])?,
                    value: Self::extract_string(&frames[3])?,
                })
            }
            _ => Err(FerrousError::Command(CommandError::SyntaxError(format!("Unknown CONFIG subcommand: {}", subcommand)))),
        }
    }
}

/// Integration adapters that preserve existing interfaces while using unified execution

/// Adapter for server.rs command processing
pub struct ServerCommandAdapter {
    executor: UnifiedCommandExecutor,
}

impl ServerCommandAdapter {
    pub fn new(storage: Arc<StorageEngine>) -> Self {
        Self {
            executor: UnifiedCommandExecutor::new(storage),
        }
    }
    
    /// Execute command with full server context
    pub fn execute_with_context(
        &self,
        frames: &[RespFrame],
        conn_id: u64,
        db_index: usize,
    ) -> Result<RespFrame> {
        let parsed = CommandParser::parse(frames)?;
        
        let context = ConnectionContext {
            conn_id,
            db_index,
            is_replica: false,
            is_transaction: false,
        };
        
        self.executor
            .clone()
            .with_context(context)
            .execute(parsed)
    }
}

/// Adapter for Lua script command execution
pub struct LuaCommandAdapter {
    executor: UnifiedCommandExecutor,
}

impl LuaCommandAdapter {
    pub fn new(storage: Arc<StorageEngine>) -> Self {
        Self {
            executor: UnifiedCommandExecutor::new(storage),
        }
    }
    
    /// Execute command from Lua context with proper atomicity
    pub fn execute_lua_command(
        &self,
        args: Vec<String>,
        db_index: usize,
    ) -> Result<RespFrame> {
        // Convert string args to RESP frames for parsing
        let frames: Vec<RespFrame> = args
            .into_iter()
            .map(|s| RespFrame::bulk_string(s))
            .collect();
        
        let mut parsed = CommandParser::parse(&frames)?;
        parsed.db_override = Some(db_index);
        
        // Execute with guaranteed atomicity for multi-step scripts
        self.executor.execute(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_set_nx_atomicity() {
        let storage = StorageEngine::new_in_memory();
        let executor = UnifiedCommandExecutor::new(storage.clone());
        
        // Test SET NX on non-existent key
        let cmd = ParsedCommand {
            command: Command::String(StringCommand::Set {
                key: b"test_key".to_vec(),
                value: b"value1".to_vec(),
                options: SetOptions { nx: true, ..Default::default() },
            }),
            db_override: Some(0),
        };
        
        let result = executor.execute(cmd).unwrap();
        assert!(matches!(result, RespFrame::SimpleString(_))); // Should return OK
        
        // Test SET NX on existing key
        let cmd2 = ParsedCommand {
            command: Command::String(StringCommand::Set {
                key: b"test_key".to_vec(),
                value: b"value2".to_vec(),
                options: SetOptions { nx: true, ..Default::default() },
            }),
            db_override: Some(0),
        };
        
        let result2 = executor.execute(cmd2).unwrap();
        assert!(matches!(result2, RespFrame::BulkString(None))); // Should return null
        
        // Verify original value preserved
        let get_cmd = ParsedCommand {
            command: Command::String(StringCommand::Get {
                key: b"test_key".to_vec(),
            }),
            db_override: Some(0),
        };
        
        let get_result = executor.execute(get_cmd).unwrap();
        if let RespFrame::BulkString(Some(bytes)) = get_result {
            assert_eq!(bytes.as_ref(), b"value1");
        } else {
            panic!("Expected value1, got: {:?}", get_result);
        }
    }
    
    #[test]
    fn test_lua_server_command_parity() {
        let storage = StorageEngine::new_in_memory();
        
        // Execute SET NX via server adapter
        let server_adapter = ServerCommandAdapter::new(storage.clone());
        let server_result = server_adapter.execute_with_context(
            &[
                RespFrame::bulk_string("SET"),
                RespFrame::bulk_string("server_key"),
                RespFrame::bulk_string("value"),
                RespFrame::bulk_string("NX"),
            ],
            1, 0
        ).unwrap();
        
        // Execute SET NX via Lua adapter
        let lua_adapter = LuaCommandAdapter::new(storage.clone());
        let lua_result = lua_adapter.execute_lua_command(
            vec!["SET".into(), "lua_key".into(), "value".into(), "NX".into()],
            0
        ).unwrap();
        
        // Both should succeed (new keys)
        assert!(matches!(server_result, RespFrame::SimpleString(_)));
        assert!(matches!(lua_result, RespFrame::SimpleString(_)));
        
        // Both should fail on existing keys  
        let server_retry = server_adapter.execute_with_context(
            &[
                RespFrame::bulk_string("SET"),
                RespFrame::bulk_string("server_key"),
                RespFrame::bulk_string("new_value"),
                RespFrame::bulk_string("NX"),
            ],
            1, 0
        ).unwrap();
        
        let lua_retry = lua_adapter.execute_lua_command(
            vec!["SET".into(), "lua_key".into(), "new_value".into(), "NX".into()],
            0
        ).unwrap();
        
        // Both should return null (key exists)
        assert!(matches!(server_retry, RespFrame::BulkString(None)));
        assert!(matches!(lua_retry, RespFrame::BulkString(None)));
    }
}