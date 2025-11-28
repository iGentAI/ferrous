#!/usr/bin/env python3
"""
Comprehensive tests for Ferrous bit operations.
Tests SETBIT, GETBIT, BITCOUNT, BITOP, and BITPOS commands.
"""

import redis
import pytest
import time
import subprocess
import signal
import os
import sys

# Test configuration
FERROUS_HOST = "127.0.0.1"
FERROUS_PORT = 6399  # Use non-standard port to avoid conflicts

@pytest.fixture(scope="module")
def ferrous_server():
    """Start Ferrous server for testing."""
    # Find the ferrous binary
    ferrous_path = "./target/release/ferrous"
    if not os.path.exists(ferrous_path):
        ferrous_path = "./target/debug/ferrous"
    
    if not os.path.exists(ferrous_path):
        pytest.skip("Ferrous binary not found. Run 'cargo build --release' first.")
    
    # Start the server
    proc = subprocess.Popen(
        [ferrous_path, "--port", str(FERROUS_PORT)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        cwd=os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
    )
    
    # Wait for server to start
    time.sleep(1)
    
    yield proc
    
    # Cleanup
    proc.send_signal(signal.SIGTERM)
    proc.wait(timeout=5)


@pytest.fixture
def client(ferrous_server):
    """Create a Redis client connected to Ferrous."""
    r = redis.Redis(host=FERROUS_HOST, port=FERROUS_PORT, decode_responses=False)
    r.flushdb()
    yield r
    r.close()


class TestSetBit:
    """Tests for SETBIT command."""
    
    def test_setbit_basic(self, client):
        """Test basic SETBIT operation."""
        # Set bit 7 (LSB of byte 0) to 1
        result = client.setbit("mykey", 7, 1)
        assert result == 0  # Original value was 0
        
        # Verify the value
        value = client.get("mykey")
        assert value == b"\x01"  # Binary: 00000001
    
    def test_setbit_msb(self, client):
        """Test SETBIT on MSB (bit 0)."""
        result = client.setbit("mykey", 0, 1)
        assert result == 0
        
        value = client.get("mykey")
        assert value == b"\x80"  # Binary: 10000000
    
    def test_setbit_multiple_bytes(self, client):
        """Test SETBIT that extends string to multiple bytes."""
        # Set bit 15 (LSB of byte 1)
        result = client.setbit("mykey", 15, 1)
        assert result == 0
        
        value = client.get("mykey")
        assert value == b"\x00\x01"  # Two bytes, second byte is 00000001
    
    def test_setbit_returns_previous_value(self, client):
        """Test that SETBIT returns the previous bit value."""
        # Set bit to 1
        result1 = client.setbit("mykey", 7, 1)
        assert result1 == 0
        
        # Set same bit to 1 again
        result2 = client.setbit("mykey", 7, 1)
        assert result2 == 1  # Previous value was 1
        
        # Set bit to 0
        result3 = client.setbit("mykey", 7, 0)
        assert result3 == 1  # Previous value was 1
        
        # Set same bit to 0 again
        result4 = client.setbit("mykey", 7, 0)
        assert result4 == 0  # Previous value was 0
    
    def test_setbit_large_offset(self, client):
        """Test SETBIT with large offset."""
        # Set bit at offset 1000 (byte 125)
        result = client.setbit("mykey", 1000, 1)
        assert result == 0
        
        # Verify string length
        value = client.get("mykey")
        assert len(value) == 126  # Bytes 0-125
    
    def test_setbit_on_existing_string(self, client):
        """Test SETBIT on existing string value."""
        client.set("mykey", b"\xff")  # 11111111
        
        # Clear bit 0 (MSB)
        result = client.setbit("mykey", 0, 0)
        assert result == 1  # Was 1
        
        value = client.get("mykey")
        assert value == b"\x7f"  # 01111111


class TestGetBit:
    """Tests for GETBIT command."""
    
    def test_getbit_nonexistent_key(self, client):
        """Test GETBIT on non-existent key returns 0."""
        result = client.getbit("nonexistent", 0)
        assert result == 0
    
    def test_getbit_beyond_string(self, client):
        """Test GETBIT beyond string length returns 0."""
        client.set("mykey", b"\xff")
        result = client.getbit("mykey", 100)
        assert result == 0
    
    def test_getbit_basic(self, client):
        """Test basic GETBIT operations."""
        client.set("mykey", b"\x80")  # 10000000
        
        assert client.getbit("mykey", 0) == 1
        assert client.getbit("mykey", 1) == 0
        assert client.getbit("mykey", 7) == 0
    
    def test_getbit_all_ones(self, client):
        """Test GETBIT on byte with all bits set."""
        client.set("mykey", b"\xff")  # 11111111
        
        for i in range(8):
            assert client.getbit("mykey", i) == 1


class TestBitCount:
    """Tests for BITCOUNT command."""
    
    def test_bitcount_nonexistent_key(self, client):
        """Test BITCOUNT on non-existent key returns 0."""
        result = client.bitcount("nonexistent")
        assert result == 0
    
    def test_bitcount_empty_string(self, client):
        """Test BITCOUNT on empty string returns 0."""
        client.set("mykey", b"")
        result = client.bitcount("mykey")
        assert result == 0
    
    def test_bitcount_all_zeros(self, client):
        """Test BITCOUNT on all-zero bytes."""
        client.set("mykey", b"\x00\x00\x00")
        result = client.bitcount("mykey")
        assert result == 0
    
    def test_bitcount_all_ones(self, client):
        """Test BITCOUNT on all-one bytes."""
        client.set("mykey", b"\xff\xff\xff")  # 24 bits set
        result = client.bitcount("mykey")
        assert result == 24
    
    def test_bitcount_mixed(self, client):
        """Test BITCOUNT on mixed bytes."""
        client.set("mykey", b"\xff\xf0\x00")  # 8 + 4 + 0 = 12 bits
        result = client.bitcount("mykey")
        assert result == 12
    
    def test_bitcount_with_byte_range(self, client):
        """Test BITCOUNT with byte range."""
        client.set("mykey", b"\xff\xf0\x00")  # byte0=8, byte1=4, byte2=0
        
        # First byte only
        result = client.bitcount("mykey", 0, 0)
        assert result == 8
        
        # Second byte only
        result = client.bitcount("mykey", 1, 1)
        assert result == 4
        
        # Third byte only
        result = client.bitcount("mykey", 2, 2)
        assert result == 0
        
        # First two bytes
        result = client.bitcount("mykey", 0, 1)
        assert result == 12
    
    def test_bitcount_negative_indices(self, client):
        """Test BITCOUNT with negative byte indices."""
        client.set("mykey", b"\xff\xf0\x0f")  # byte0=8, byte1=4, byte2=4
        
        # Last byte
        result = client.bitcount("mykey", -1, -1)
        assert result == 4
        
        # Last two bytes
        result = client.bitcount("mykey", -2, -1)
        assert result == 8


class TestBitOp:
    """Tests for BITOP command."""
    
    def test_bitop_and(self, client):
        """Test BITOP AND operation."""
        client.set("key1", b"\xff\x0f")  # 11111111 00001111
        client.set("key2", b"\x0f\xff")  # 00001111 11111111
        
        result = client.bitop("AND", "destkey", "key1", "key2")
        assert result == 2  # Result length
        
        value = client.get("destkey")
        assert value == b"\x0f\x0f"  # 00001111 00001111
    
    def test_bitop_or(self, client):
        """Test BITOP OR operation."""
        client.set("key1", b"\xf0\x0f")  # 11110000 00001111
        client.set("key2", b"\x0f\xf0")  # 00001111 11110000
        
        result = client.bitop("OR", "destkey", "key1", "key2")
        assert result == 2
        
        value = client.get("destkey")
        assert value == b"\xff\xff"  # 11111111 11111111
    
    def test_bitop_xor(self, client):
        """Test BITOP XOR operation."""
        client.set("key1", b"\xff\x00")  # 11111111 00000000
        client.set("key2", b"\x0f\x0f")  # 00001111 00001111
        
        result = client.bitop("XOR", "destkey", "key1", "key2")
        assert result == 2
        
        value = client.get("destkey")
        assert value == b"\xf0\x0f"  # 11110000 00001111
    
    def test_bitop_not(self, client):
        """Test BITOP NOT operation."""
        client.set("key1", b"\x00\xff")  # 00000000 11111111
        
        result = client.bitop("NOT", "destkey", "key1")
        assert result == 2
        
        value = client.get("destkey")
        assert value == b"\xff\x00"  # 11111111 00000000
    
    def test_bitop_different_lengths(self, client):
        """Test BITOP with different length strings."""
        client.set("key1", b"\xff")      # 1 byte
        client.set("key2", b"\x0f\x0f")  # 2 bytes
        
        # OR should zero-pad shorter string
        result = client.bitop("OR", "destkey", "key1", "key2")
        assert result == 2
        
        value = client.get("destkey")
        assert value == b"\xff\x0f"  # key1 padded with 0x00
    
    def test_bitop_nonexistent_key(self, client):
        """Test BITOP with non-existent key (treated as zeros)."""
        client.set("key1", b"\xff")
        
        result = client.bitop("AND", "destkey", "key1", "nonexistent")
        assert result == 1
        
        value = client.get("destkey")
        assert value == b"\x00"  # AND with zeros = zeros
    
    def test_bitop_multiple_keys(self, client):
        """Test BITOP with multiple source keys."""
        client.set("key1", b"\xff")
        client.set("key2", b"\x0f")
        client.set("key3", b"\xf0")
        
        result = client.bitop("AND", "destkey", "key1", "key2", "key3")
        assert result == 1
        
        value = client.get("destkey")
        assert value == b"\x00"  # 0xff & 0x0f & 0xf0 = 0x00


class TestBitPos:
    """Tests for BITPOS command."""
    
    def test_bitpos_find_first_one(self, client):
        """Test BITPOS finding first 1 bit."""
        client.set("mykey", b"\x00\xff")  # First 1 is at bit 8
        result = client.bitpos("mykey", 1)
        assert result == 8
    
    def test_bitpos_find_first_zero(self, client):
        """Test BITPOS finding first 0 bit."""
        client.set("mykey", b"\xff\x7f")  # First 0 is at bit 8
        result = client.bitpos("mykey", 0)
        assert result == 8
    
    def test_bitpos_nonexistent_key(self, client):
        """Test BITPOS on non-existent key."""
        # Searching for 1 in non-existent key returns -1
        result = client.bitpos("nonexistent", 1)
        assert result == -1
        
        # Searching for 0 in non-existent key returns 0
        result = client.bitpos("nonexistent", 0)
        assert result == 0
    
    def test_bitpos_all_zeros(self, client):
        """Test BITPOS on all-zero string."""
        client.set("mykey", b"\x00\x00\x00")
        
        # First 0 is at position 0
        result = client.bitpos("mykey", 0)
        assert result == 0
        
        # No 1 bits exist
        result = client.bitpos("mykey", 1)
        assert result == -1
    
    def test_bitpos_all_ones(self, client):
        """Test BITPOS on all-ones string."""
        client.set("mykey", b"\xff\xff\xff")
        
        # First 1 is at position 0
        result = client.bitpos("mykey", 1)
        assert result == 0
        
        # No 0 bits in string, returns position after string
        result = client.bitpos("mykey", 0)
        assert result == 24  # 3 bytes * 8 bits
    
    def test_bitpos_with_byte_range(self, client):
        """Test BITPOS with byte range."""
        client.set("mykey", b"\x00\xff\x00")  # bit 8-15 are 1s
        
        # Search for 1 starting from byte 1
        result = client.bitpos("mykey", 1, 1)
        assert result == 8
        
        # Search for 1 in byte 0 only (none exist)
        result = client.bitpos("mykey", 1, 0, 0)
        assert result == -1


class TestBitEdgeCases:
    """Edge case tests for bit operations."""
    
    def test_setbit_zero_offset(self, client):
        """Test SETBIT at offset 0."""
        client.setbit("mykey", 0, 1)
        assert client.getbit("mykey", 0) == 1
    
    def test_setbit_creates_key(self, client):
        """Test that SETBIT creates a key if it doesn't exist."""
        assert not client.exists("newkey")
        client.setbit("newkey", 0, 1)
        assert client.exists("newkey")
    
    def test_bit_operations_on_string(self, client):
        """Test bit operations work on regular string values."""
        client.set("mykey", "hello")  # ASCII string
        
        # Get bits from 'h' (0x68 = 01101000)
        assert client.getbit("mykey", 0) == 0  # MSB
        assert client.getbit("mykey", 1) == 1
        assert client.getbit("mykey", 2) == 1
        assert client.getbit("mykey", 3) == 0
    
    def test_bitcount_string_value(self, client):
        """Test BITCOUNT on ASCII string."""
        client.set("mykey", "foobar")  # Known string
        result = client.bitcount("mykey")
        
        # Count bits in "foobar"
        expected = sum(bin(ord(c)).count('1') for c in "foobar")
        assert result == expected


class TestBitIntegration:
    """Integration tests combining multiple bit operations."""
    
    def test_user_activity_tracking(self, client):
        """Simulate user activity tracking use case."""
        # Track active users (user IDs 0, 7, 15, 100)
        client.setbit("active_users", 0, 1)
        client.setbit("active_users", 7, 1)
        client.setbit("active_users", 15, 1)
        client.setbit("active_users", 100, 1)
        
        # Count active users
        count = client.bitcount("active_users")
        assert count == 4
        
        # Check specific users
        assert client.getbit("active_users", 0) == 1
        assert client.getbit("active_users", 1) == 0
        assert client.getbit("active_users", 7) == 1
        assert client.getbit("active_users", 100) == 1
    
    def test_bloom_filter_simulation(self, client):
        """Simulate a simple bloom filter pattern."""
        # Add items to "bloom filter" by setting bits at hash positions
        hash_positions = [3, 17, 42, 99, 128]
        for pos in hash_positions:
            client.setbit("bloom", pos, 1)
        
        # Check membership (all should be present)
        for pos in hash_positions:
            assert client.getbit("bloom", pos) == 1
        
        # Check non-member (probably not present)
        assert client.getbit("bloom", 50) == 0
    
    def test_daily_active_users_intersection(self, client):
        """Test using BITOP to find users active on multiple days."""
        # Day 1 active users: 0, 1, 2
        for uid in [0, 1, 2]:
            client.setbit("day1", uid, 1)
        
        # Day 2 active users: 1, 2, 3
        for uid in [1, 2, 3]:
            client.setbit("day2", uid, 1)
        
        # Find users active on both days
        client.bitop("AND", "both_days", "day1", "day2")
        
        # Users 1 and 2 were active both days
        assert client.getbit("both_days", 0) == 0
        assert client.getbit("both_days", 1) == 1
        assert client.getbit("both_days", 2) == 1
        assert client.getbit("both_days", 3) == 0
        
        # Count users active both days
        assert client.bitcount("both_days") == 2


if __name__ == "__main__":
    pytest.main([__file__, "-v"])