//! RDB persistence implementation
//! 
//! Provides Redis Database (RDB) format persistence for durability.
//! Supports both blocking (SAVE) and background (BGSAVE) operations.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write, BufWriter, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use std::thread;

use crate::error::{FerrousError, Result};
use crate::storage::{StorageEngine, Value, GetResult, DatabaseIndex, Key};

/// RDB file version (Redis 9 compatible)
const RDB_VERSION: u16 = 9;

/// RDB magic string
const RDB_MAGIC: &[u8] = b"REDIS";

/// RDB opcodes
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
enum RdbOpcode {
    /// Indicates end of database
    Eof = 0xFF,
    /// Select database
    SelectDb = 0xFE,
    /// Key with expiry time in seconds
    ExpireTimeS = 0xFD,
    /// Key with expiry time in milliseconds
    ExpireTimeMs = 0xFC,
    /// Resize database hint
    ResizeDb = 0xFB,
    /// Auxiliary field
    Aux = 0xFA,
    
    /// String encoding
    String = 0x00,
    /// List encoding
    List = 0x01,
    /// Set encoding
    Set = 0x02,
    /// Sorted set encoding
    ZSet = 0x03,
    /// Hash encoding
    Hash = 0x04,
    /// Sorted set with ziplist encoding
    ZSet2 = 0x05,
}

/// RDB persistence engine
pub struct RdbEngine {
    /// Path to RDB file
    file_path: PathBuf,
    
    /// Is background save in progress?
    bgsave_in_progress: Arc<Mutex<bool>>,
    
    /// Last save time
    last_save_time: Arc<RwLock<Option<SystemTime>>>,
    
    /// Configuration
    config: RdbConfig,
}

/// RDB configuration
#[derive(Debug, Clone)]
pub struct RdbConfig {
    /// Enable automatic saving
    pub auto_save: bool,
    
    /// Save after N seconds and M changes
    pub save_rules: Vec<(u64, u64)>, // (seconds, changes)
    
    /// Compress string values in RDB
    pub compress_strings: bool,
    
    /// RDB filename
    pub filename: String,
    
    /// Working directory
    pub dir: String,
}

impl Default for RdbConfig {
    fn default() -> Self {
        Self {
            auto_save: true,
            save_rules: vec![
                (900, 1),    // After 900 sec (15 min) if at least 1 key changed
                (300, 10),   // After 300 sec (5 min) if at least 10 keys changed
                (60, 10000), // After 60 sec if at least 10000 keys changed
            ],
            compress_strings: false,
            filename: "dump.rdb".to_string(),
            dir: "./".to_string(),
        }
    }
}

impl RdbEngine {
    /// Create a new RDB engine
    pub fn new(config: RdbConfig) -> Self {
        let mut file_path = PathBuf::from(&config.dir);
        file_path.push(&config.filename);
        
        Self {
            file_path,
            bgsave_in_progress: Arc::new(Mutex::new(false)),
            last_save_time: Arc::new(RwLock::new(None)),
            config,
        }
    }
    
    /// Load RDB file into storage engine
    pub fn load(&self, storage: &Arc<StorageEngine>) -> Result<()> {
        if !self.file_path.exists() {
            println!("RDB: No dump file found at {}", self.file_path.display());
            return Ok(()); // No RDB file to load
        }
        
        let file = File::open(&self.file_path)
            .map_err(|e| FerrousError::Io(format!("Failed to open RDB file: {}", e)))?;
        
        let mut reader = RdbReader::new(BufReader::new(file));
        reader.load_into(storage)?;
        
        println!("RDB: Loaded data from {}", self.file_path.display());
        Ok(())
    }
    
    /// Perform blocking save
    pub fn save(&self, storage: &Arc<StorageEngine>) -> Result<()> {
        // Check if background save is in progress
        {
            let bgsave = self.bgsave_in_progress.lock().unwrap();
            if *bgsave {
                return Err(FerrousError::Internal(
                    "Background save already in progress".into()
                ));
            }
        }
        
        // Create temporary file
        let temp_path = self.file_path.with_extension("tmp");
        
        println!("RDB: Starting dump to {}", temp_path.display());
        
        // Write to temporary file
        self.write_snapshot(storage, &temp_path)?;
        
        // Atomic rename
        std::fs::rename(&temp_path, &self.file_path)
            .map_err(|e| FerrousError::Io(format!("Failed to rename RDB file: {}", e)))?;
        
        // Update last save time
        {
            let mut last_save = self.last_save_time.write().unwrap();
            *last_save = Some(SystemTime::now());
        }
        
        println!("RDB: Dump completed successfully");
        Ok(())
    }
    
    /// Perform background save
    pub fn bgsave(&self, storage: Arc<StorageEngine>) -> Result<()> {
        // Check if background save is already in progress
        {
            let mut bgsave = self.bgsave_in_progress.lock().unwrap();
            if *bgsave {
                return Err(FerrousError::Internal(
                    "Background save already in progress".into()
                ));
            }
            *bgsave = true;
        }
        
        let engine = self.clone();
        
        // Spawn background thread
        thread::spawn(move || {
            println!("RDB: Background saving started");
            
            match engine.save(&storage) {
                Ok(_) => println!("RDB: Background saving terminated with success"),
                Err(e) => eprintln!("RDB: Background saving error: {}", e),
            }
            
            // Clear in-progress flag
            let mut bgsave = engine.bgsave_in_progress.lock().unwrap();
            *bgsave = false;
        });
        
        Ok(())
    }
    
    /// Check if background save is in progress
    pub fn is_bgsave_in_progress(&self) -> bool {
        let bgsave = self.bgsave_in_progress.lock().unwrap();
        *bgsave
    }
    
    /// Get last save time
    pub fn last_save_time(&self) -> Option<SystemTime> {
        let last_save = self.last_save_time.read().unwrap();
        *last_save
    }
    
    /// Generate RDB bytes for replication
    pub fn generate_rdb_bytes(&self, storage: &Arc<StorageEngine>) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        
        // Write magic and version
        buffer.extend_from_slice(RDB_MAGIC);
        buffer.extend_from_slice(format!("{:04}", RDB_VERSION).as_bytes());
        
        // Write metadata
        self.write_aux_field(&mut buffer, "redis-ver", env!("CARGO_PKG_VERSION"))?;
        self.write_aux_field(&mut buffer, "redis-bits", "64")?;
        self.write_aux_field(&mut buffer, "ctime", &SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string())?;
        self.write_aux_field(&mut buffer, "used-mem", &storage.memory_usage().to_string())?;
        
        // Write databases
        for db in 0..storage.database_count() {
            let keys = storage.get_all_keys(db)?;
            if keys.is_empty() {
                continue;
            }
            
            // Select DB opcode
            buffer.push(RdbOpcode::SelectDb as u8);
            self.write_length(&mut buffer, db)?;
            
            // Resize DB opcode
            buffer.push(RdbOpcode::ResizeDb as u8);
            self.write_length(&mut buffer, keys.len())?;
            self.write_length(&mut buffer, 0)?; // No separate expires hash
            
            // Write all key-value pairs
            for key in keys {
                if let GetResult::Found(value) = storage.get(db, &key)? {
                    // Check for expiration
                    let expire_time = storage.ttl(db, &key)?
                        .map(|ttl| SystemTime::now() + ttl);
                    
                    // Write expiration if present
                    if let Some(expire) = expire_time {
                        buffer.push(RdbOpcode::ExpireTimeMs as u8);
                        let timestamp = expire.duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
                        buffer.extend_from_slice(&timestamp.to_le_bytes());
                    }
                    
                    // Write value type
                    match value {
                        Value::String(_) => buffer.push(RdbOpcode::String as u8),
                        Value::List(_) => buffer.push(RdbOpcode::List as u8),
                        Value::Set(_) => buffer.push(RdbOpcode::Set as u8),
                        Value::Hash(_) => buffer.push(RdbOpcode::Hash as u8),
                        Value::SortedSet(_) => buffer.push(RdbOpcode::ZSet as u8),
                    }
                    
                    // Write key
                    self.write_length(&mut buffer, key.len())?;
                    buffer.extend_from_slice(&key);
                    
                    // Write value
                    match value {
                        Value::String(bytes) => {
                            self.write_length(&mut buffer, bytes.len())?;
                            buffer.extend_from_slice(bytes.as_ref());
                        }
                        _ => {
                            // Simplified handling - just write a placeholder for now
                            // For production code, you'd want to properly encode each type
                            buffer.push(0); // Empty value for now
                        }
                    }
                }
            }
        }
        
        // Write EOF
        buffer.push(RdbOpcode::Eof as u8);
        
        // Write checksum (simplified)
        let checksum: u64 = 0; // Real implementation would calculate CRC64
        buffer.extend_from_slice(&checksum.to_le_bytes());
        
        println!("RDB: Generated {} bytes for replication", buffer.len());
        
        Ok(buffer)
    }
    
    /// Write auxiliary field to buffer
    fn write_aux_field(&self, buffer: &mut Vec<u8>, key: &str, value: &str) -> Result<()> {
        buffer.push(RdbOpcode::Aux as u8);
        
        // Write key
        self.write_length(buffer, key.len())?;
        buffer.extend_from_slice(key.as_bytes());
        
        // Write value
        self.write_length(buffer, value.len())?;
        buffer.extend_from_slice(value.as_bytes());
        
        Ok(())
    }
    
    /// Write length to buffer
    fn write_length(&self, buffer: &mut Vec<u8>, len: usize) -> Result<()> {
        match len {
            0..=63 => {
                // 6-bit length
                buffer.push(len as u8);
            }
            64..=16383 => {
                // 14-bit length
                let high = ((len >> 8) & 0x3F) as u8 | 0x40;
                let low = (len & 0xFF) as u8;
                buffer.push(high);
                buffer.push(low);
            }
            _ => {
                // 32-bit length
                buffer.push(0x80);
                buffer.extend_from_slice(&(len as u32).to_be_bytes());
            }
        }
        Ok(())
    }
    
    /// Write snapshot to file
    fn write_snapshot(&self, storage: &Arc<StorageEngine>, path: &Path) -> Result<()> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(|e| FerrousError::Io(format!("Failed to create RDB file: {}", e)))?;
        
        let mut writer = RdbWriter::new(BufWriter::new(file));
        
        // Write header
        writer.write_header()?;
        
        // Write metadata
        writer.write_metadata()?;
        
        // Write databases
        for db_idx in 0..storage.database_count() {
            // Get all keys from database
            let keys = storage.get_all_keys(db_idx)?;
            
            if !keys.is_empty() {
                // Write database selector
                writer.write_db_selector(db_idx)?;
                
                // Write resize hint
                writer.write_resize_db(keys.len(), keys.len())?;
                
                // Write each key-value pair
                for key in keys {
                    // Get value
                    match storage.get(db_idx, &key)? {
                        GetResult::Found(value) => {
                            // Get TTL if any
                            let ttl = storage.ttl(db_idx, &key)?;
                            
                            // Write key-value pair
                            writer.write_key_value(&key, &value, ttl)?;
                        }
                        _ => {
                            // Key doesn't exist or expired, skip
                        }
                    }
                }
            }
        }
        
        // Write EOF
        writer.write_eof()?;
        
        // Write CRC64 checksum
        writer.write_checksum()?;
        
        // Ensure all data is flushed
        writer.flush()?;
        
        Ok(())
    }
}

impl Clone for RdbEngine {
    fn clone(&self) -> Self {
        Self {
            file_path: self.file_path.clone(),
            bgsave_in_progress: Arc::clone(&self.bgsave_in_progress),
            last_save_time: Arc::clone(&self.last_save_time),
            config: self.config.clone(),
        }
    }
}

/// RDB file writer
struct RdbWriter<W: Write> {
    writer: W,
    bytes_written: u64,
    crc: u64,
}

impl<W: Write> RdbWriter<W> {
    fn new(writer: W) -> Self {
        Self {
            writer,
            bytes_written: 0,
            crc: 0,
        }
    }
    
    /// Write RDB header
    fn write_header(&mut self) -> io::Result<()> {
        // Magic string
        self.write_raw(RDB_MAGIC)?;
        
        // Version (4 ASCII digits)
        let version_str = format!("{:04}", RDB_VERSION);
        self.write_raw(version_str.as_bytes())?;
        
        Ok(())
    }
    
    /// Write metadata
    fn write_metadata(&mut self) -> io::Result<()> {
        // Redis version
        self.write_aux("redis-ver", env!("CARGO_PKG_VERSION"))?;
        
        // Creation time
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.write_aux("ctime", &now.to_string())?;
        
        Ok(())
    }
    
    /// Write auxiliary field
    fn write_aux(&mut self, key: &str, value: &str) -> io::Result<()> {
        self.write_byte(RdbOpcode::Aux as u8)?;
        self.write_string(key.as_bytes())?;
        self.write_string(value.as_bytes())?;
        Ok(())
    }
    
    /// Write database selector
    fn write_db_selector(&mut self, db: usize) -> io::Result<()> {
        self.write_byte(RdbOpcode::SelectDb as u8)?;
        self.write_length(db)?;
        Ok(())
    }
    
    /// Write database resize hint
    fn write_resize_db(&mut self, db_size: usize, expires_size: usize) -> io::Result<()> {
        self.write_byte(RdbOpcode::ResizeDb as u8)?;
        self.write_length(db_size)?;
        self.write_length(expires_size)?;
        Ok(())
    }
    
    /// Write key-value pair
    fn write_key_value(&mut self, key: &[u8], value: &Value, ttl: Option<Duration>) -> io::Result<()> {
        // Write expiry if present
        if let Some(ttl) = ttl {
            let expiry_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64 + ttl.as_millis() as u64;
            
            self.write_byte(RdbOpcode::ExpireTimeMs as u8)?;
            self.write_u64_le(expiry_ms)?;
        }
        
        // Write value type and key
        match value {
            Value::String(bytes) => {
                self.write_byte(RdbOpcode::String as u8)?;
                self.write_string(key)?;
                self.write_string(bytes)?;
            }
            Value::SortedSet(skiplist) => {
                self.write_byte(RdbOpcode::ZSet as u8)?;
                self.write_string(key)?;
                
                // Get all items and write them
                let len = skiplist.len();
                self.write_length(len)?;
                
                // Note: This is a suboptimal approach since we need to materialize
                // all members in memory. A better approach would be to have a streaming
                // iterator in the SkipList implementation.
                let items = skiplist.range_by_rank(0, len - 1).items;
                
                for (member, score) in items {
                    self.write_string(&member)?;
                    self.write_f64(score)?;
                }
            }
            Value::List(_) => {
                // TODO: Implement list serialization
                return Ok(());
            }
            Value::Set(_) => {
                // TODO: Implement set serialization
                return Ok(());
            }
            Value::Hash(_) => {
                // TODO: Implement hash serialization
                return Ok(());
            }
        }
        
        Ok(())
    }
    
    /// Write EOF marker
    fn write_eof(&mut self) -> io::Result<()> {
        self.write_byte(RdbOpcode::Eof as u8)?;
        Ok(())
    }
    
    /// Write CRC64 checksum
    fn write_checksum(&mut self) -> io::Result<()> {
        // For now, write a dummy checksum
        self.write_u64_le(self.crc)?;
        Ok(())
    }
    
    /// Write a single byte
    fn write_byte(&mut self, byte: u8) -> io::Result<()> {
        self.write_raw(&[byte])
    }
    
    /// Write raw bytes
    fn write_raw(&mut self, data: &[u8]) -> io::Result<()> {
        self.writer.write_all(data)?;
        self.bytes_written += data.len() as u64;
        // Update CRC (simplified - real implementation would use CRC64)
        for &byte in data {
            self.crc = self.crc.wrapping_add(byte as u64);
        }
        Ok(())
    }
    
    /// Write length-prefixed string
    fn write_string(&mut self, s: &[u8]) -> io::Result<()> {
        self.write_length(s.len())?;
        self.write_raw(s)?;
        Ok(())
    }
    
    /// Write length encoding
    fn write_length(&mut self, len: usize) -> io::Result<()> {
        match len {
            0..=63 => {
                // 6-bit length
                self.write_byte(len as u8)?;
            }
            64..=16383 => {
                // 14-bit length
                let high = ((len >> 8) & 0x3F) as u8 | 0x40;
                let low = (len & 0xFF) as u8;
                self.write_byte(high)?;
                self.write_byte(low)?;
            }
            _ => {
                // 32-bit length
                self.write_byte(0x80)?;
                self.write_u32_be(len as u32)?;
            }
        }
        Ok(())
    }
    
    /// Write 32-bit big-endian integer
    fn write_u32_be(&mut self, n: u32) -> io::Result<()> {
        self.write_raw(&n.to_be_bytes())
    }
    
    /// Write 64-bit little-endian integer
    fn write_u64_le(&mut self, n: u64) -> io::Result<()> {
        self.write_raw(&n.to_le_bytes())
    }
    
    /// Write f64 as binary
    fn write_f64(&mut self, n: f64) -> io::Result<()> {
        self.write_raw(&n.to_le_bytes())
    }
    
    /// Flush the writer
    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

/// RDB file reader
struct RdbReader<R: Read> {
    reader: R,
}

impl<R: Read> RdbReader<R> {
    fn new(reader: R) -> Self {
        Self { reader }
    }
    
    /// Load RDB file into storage engine
    fn load_into(&mut self, storage: &Arc<StorageEngine>) -> Result<()> {
        // Read and verify header
        self.read_header()?;
        
        let mut current_db = 0;
        
        loop {
            let opcode = self.read_byte()?;
            
            match opcode {
                op if op == RdbOpcode::Eof as u8 => {
                    // Read checksum and we're done
                    let _checksum = self.read_u64_le()?;
                    break;
                }
                op if op == RdbOpcode::SelectDb as u8 => {
                    current_db = self.read_length()?;
                }
                op if op == RdbOpcode::ResizeDb as u8 => {
                    // Read resize hints (we can ignore these)
                    let _db_size = self.read_length()?;
                    let _expires_size = self.read_length()?;
                }
                op if op == RdbOpcode::Aux as u8 => {
                    // Read auxiliary field (we can ignore these)
                    let _key = self.read_string()?;
                    let _value = self.read_string()?;
                }
                op if op == RdbOpcode::ExpireTimeMs as u8 => {
                    // Read expiry time and then the key-value
                    let expiry_ms = self.read_u64_le()?;
                    self.read_key_value_with_expiry(storage, current_db, expiry_ms)?;
                }
                op if op == RdbOpcode::ExpireTimeS as u8 => {
                    // Read expiry time in seconds and then the key-value
                    let expiry_s = self.read_u32_le()? as u64;
                    self.read_key_value_with_expiry(storage, current_db, expiry_s * 1000)?;
                }
                _ => {
                    // This is a value type opcode
                    self.read_key_value_with_type(storage, current_db, opcode, None)?;
                }
            }
        }
        
        Ok(())
    }
    
    /// Read and verify header
    fn read_header(&mut self) -> Result<()> {
        let mut magic = [0u8; 5];
        self.read_exact(&mut magic)?;
        
        if &magic != RDB_MAGIC {
            return Err(FerrousError::Io("Invalid RDB file format".to_string()));
        }
        
        let mut version = [0u8; 4];
        self.read_exact(&mut version)?;
        
        // Parse version
        let version_str = String::from_utf8_lossy(&version);
        let _version_num = version_str.parse::<u16>()
            .map_err(|_| FerrousError::Io("Invalid RDB version".to_string()))?;
        
        Ok(())
    }
    
    /// Read key-value with expiry
    fn read_key_value_with_expiry(&mut self, storage: &Arc<StorageEngine>, db: usize, expiry_ms: u64) -> Result<()> {
        let value_type = self.read_byte()?;
        
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        let ttl = if expiry_ms > now_ms {
            Some(Duration::from_millis(expiry_ms - now_ms))
        } else {
            None // Already expired
        };
        
        self.read_key_value_with_type(storage, db, value_type, ttl)
    }
    
    /// Read key-value with known type
    fn read_key_value_with_type(&mut self, storage: &Arc<StorageEngine>, db: usize, value_type: u8, ttl: Option<Duration>) -> Result<()> {
        match value_type {
            op if op == RdbOpcode::String as u8 => {
                let key = self.read_string()?;
                let value = self.read_string()?;
                
                if let Some(ttl) = ttl {
                    storage.set_string_ex(db, key, value, ttl)?;
                } else {
                    storage.set_string(db, key, value)?;
                }
            }
            op if op == RdbOpcode::ZSet as u8 || op == RdbOpcode::ZSet2 as u8 => {
                let key = self.read_string()?;
                let count = self.read_length()?;
                
                for _ in 0..count {
                    let member = self.read_string()?;
                    let score = self.read_f64()?;
                    storage.zadd(db, key.clone(), member, score)?;
                }
                
                if let Some(ttl) = ttl {
                    storage.expire(db, &key, ttl)?;
                }
            }
            _ => {
                // Skip unknown types for now
                return Err(FerrousError::Io(format!("Unknown value type: {}", value_type)));
            }
        }
        
        Ok(())
    }
    
    /// Read a single byte
    fn read_byte(&mut self) -> Result<u8> {
        let mut buf = [0u8; 1];
        self.read_exact(&mut buf)?;
        Ok(buf[0])
    }
    
    /// Read exact number of bytes
    fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        self.reader.read_exact(buf)
            .map_err(|e| FerrousError::Io(e.to_string()))
    }
    
    /// Read length encoding
    fn read_length(&mut self) -> Result<usize> {
        let first = self.read_byte()?;
        
        match first >> 6 {
            0 => Ok(first as usize),
            1 => {
                let second = self.read_byte()?;
                Ok((((first & 0x3F) as usize) << 8) | (second as usize))
            }
            2 => {
                let len = self.read_u32_be()?;
                Ok(len as usize)
            }
            _ => Err(FerrousError::Io("Invalid length encoding".to_string())),
        }
    }
    
    /// Read string
    fn read_string(&mut self) -> Result<Vec<u8>> {
        let len = self.read_length()?;
        let mut buf = vec![0u8; len];
        self.read_exact(&mut buf)?;
        Ok(buf)
    }
    
    /// Read 32-bit little-endian integer
    fn read_u32_le(&mut self) -> Result<u32> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }
    
    /// Read 32-bit big-endian integer
    fn read_u32_be(&mut self) -> Result<u32> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf)?;
        Ok(u32::from_be_bytes(buf))
    }
    
    /// Read 64-bit little-endian integer
    fn read_u64_le(&mut self) -> Result<u64> {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }
    
    /// Read f64
    fn read_f64(&mut self) -> Result<f64> {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf)?;
        Ok(f64::from_le_bytes(buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_rdb_save_load() {
        let config = RdbConfig {
            filename: "test.rdb".to_string(),
            ..Default::default()
        };
        
        let engine = RdbEngine::new(config);
        let storage = StorageEngine::new();
        
        // Add some test data
        storage.set_string(0, b"key1".to_vec(), b"value1".to_vec()).unwrap();
        storage.set_string(0, b"key2".to_vec(), b"value2".to_vec()).unwrap();
        storage.zadd(0, b"zset".to_vec(), b"member1".to_vec(), 1.0).unwrap();
        storage.zadd(0, b"zset".to_vec(), b"member2".to_vec(), 2.0).unwrap();
        
        // Save
        engine.save(&storage).unwrap();
        
        // Clear storage
        storage.flush_db(0).unwrap();
        
        // Load
        engine.load(&storage).unwrap();
        
        // Verify data
        assert_eq!(storage.get_string(0, b"key1").unwrap(), Some(b"value1".to_vec()));
        assert_eq!(storage.get_string(0, b"key2").unwrap(), Some(b"value2".to_vec()));
        assert_eq!(storage.zscore(0, b"zset", b"member1").unwrap(), Some(1.0));
        assert_eq!(storage.zscore(0, b"zset", b"member2").unwrap(), Some(2.0));
        
        // Cleanup
        std::fs::remove_file("test.rdb").ok();
    }
}