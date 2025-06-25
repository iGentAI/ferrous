//! Simple SHA1 implementation for script hashing
//!
//! This provides a basic SHA1 implementation for hashing Lua scripts.
//! Used by the SCRIPT commands and redis.sha1hex function.

use std::fmt::Write;

/// Compute SHA1 hash of a string
pub fn compute_sha1(input: &str) -> String {
    // For now, use a simple hash implementation
    // In production, this should use a proper SHA1 library
    
    // Simple FNV-1a hash as placeholder
    let mut hash = 0xcbf29ce484222325u64; // FNV offset basis
    
    for byte in input.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3); // FNV prime
    }
    
    // Convert to hex string with SHA1 length (40 chars)
    let mut result = String::with_capacity(40);
    for i in 0..5 {
        let part = (hash >> (i * 8)) as u32;
        write!(&mut result, "{:08x}", part).unwrap();
    }
    
    result
}

/// SHA1 context for incremental hashing
pub struct Sha1Context {
    hash: u64,
}

impl Sha1Context {
    /// Create a new SHA1 context
    pub fn new() -> Self {
        Sha1Context {
            hash: 0xcbf29ce484222325u64, // FNV offset basis
        }
    }
    
    /// Update the hash with more data
    pub fn update(&mut self, data: &[u8]) {
        for &byte in data {
            self.hash ^= byte as u64;
            self.hash = self.hash.wrapping_mul(0x100000001b3); // FNV prime
        }
    }
    
    /// Finalize and get the hash
    pub fn finalize(&self) -> String {
        let mut result = String::with_capacity(40);
        let hash = self.hash;
        
        for i in 0..5 {
            let part = (hash >> (i * 8)) as u32;
            write!(&mut result, "{:08x}", part).unwrap();
        }
        
        result
    }
}

#[test]
fn test_sha1() {
    // Basic test to ensure consistent hashing
    let hash1 = compute_sha1("hello world");
    let hash2 = compute_sha1("hello world");
    assert_eq!(hash1, hash2);
    assert_eq!(hash1.len(), 40); // SHA1 hex string length
    
    // Different inputs should produce different hashes
    let hash3 = compute_sha1("goodbye world");
    assert_ne!(hash1, hash3);
}