#!/usr/bin/env python3
"""
Test script for Ferrous MEMORY command functionality.
"""

import socket
import time
import sys

# Use a persistent connection for efficient testing
class PersistentRedisConnection:
    def __init__(self, host='127.0.0.1', port=6379):
        self.host = host
        self.port = port
        self.socket = None
        self.connect()
    
    def connect(self):
        """Establish persistent connection"""
        self.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.socket.connect((self.host, self.port))
    
    def send_command(self, command):
        """Send command on persistent connection"""
        self.socket.sendall(command.encode())
        return self.socket.recv(4096)
    
    def close(self):
        """Close the connection"""
        if self.socket:
            self.socket.close()
            self.socket = None

def decode_response(resp):
    """Basic RESP protocol decoding for display"""
    if resp.startswith(b'+'):
        # Simple string
        return f"String: {resp[1:].strip().decode('utf-8')}"
    elif resp.startswith(b'-'):
        # Error
        return f"Error: {resp[1:].strip().decode('utf-8')}"
    elif resp.startswith(b':'):
        # Integer
        return f"Integer: {int(resp[1:].strip())}"
    elif resp.startswith(b'$'):
        # Bulk string
        parts = resp.split(b'\r\n', 2)
        if len(parts) >= 3:
            if parts[0] == b'$-1':
                return "Nil"
            else:
                return f"Bulk String: {parts[1].decode('utf-8')}"
    elif resp.startswith(b'*'):
        # Array
        return f"Array Response ({resp[:50]}... {len(resp)} bytes)"
    
    # Fallback for more complex responses
    return f"Raw Response: {resp[:100]}... ({len(resp)} bytes)"

def print_section(title):
    """Print a section title"""
    print(f"\n{'=' * 60}")
    print(f"    {title}")
    print(f"{'=' * 60}\n")

# Start the test with persistent connection
print("Testing MEMORY commands in Ferrous\n")

# Create persistent connection
conn = PersistentRedisConnection()

try:
    # Create some data to track memory usage
    print_section("1. Creating test data to track memory usage")

    # Create a large string
    large_value = "x" * 1000000  # 1MB string
    cmd = f"*3\r\n$3\r\nSET\r\n$9\r\nlarge-key\r\n${len(large_value)}\r\n{large_value}\r\n"
    resp = conn.send_command(cmd)
    print(f"SET large-key: {decode_response(resp)}")

    # Create a large list efficiently with persistent connection
    print("\nCreating large list efficiently...")
    
    # First clear any existing list
    cmd = "*2\r\n$3\r\nDEL\r\n$10\r\nlarge-list\r\n"
    resp = conn.send_command(cmd)
    
    # Add items efficiently with batching for progress display
    start_time = time.time()
    for i in range(1000):
        cmd = f"*3\r\n$5\r\nLPUSH\r\n$10\r\nlarge-list\r\n$6\r\nvalue{i}\r\n"
        resp = conn.send_command(cmd)
        if i % 200 == 0:
            print(f"Added {i} items...")
    
    elapsed = time.time() - start_time
    print(f"LPUSH completed: 1000 items in {elapsed:.2f}s ({1000/elapsed:.1f} ops/sec)")

    # Create a hash efficiently
    print("\nCreating hash efficiently...")
    for i in range(100):
        field = f"field{i}"
        value = f"value{i}" * 10
        cmd = f"*4\r\n$4\r\nHSET\r\n$9\r\ntest-hash\r\n${len(field)}\r\n{field}\r\n${len(value)}\r\n{value}\r\n"
        resp = conn.send_command(cmd)

    print(f"HSET result: {decode_response(resp)}")

    # Test MEMORY USAGE command
    print_section("2. Testing MEMORY USAGE command")

    keys = ["large-key", "large-list", "test-hash"]
    for key in keys:
        cmd = f"*3\r\n$6\r\nMEMORY\r\n$5\r\nUSAGE\r\n${len(key)}\r\n{key}\r\n"
        resp = conn.send_command(cmd)
        print(f"MEMORY USAGE {key}: {decode_response(resp)}")

    # Test MEMORY STATS command
    print_section("3. Testing MEMORY STATS command")

    cmd = "*2\r\n$6\r\nMEMORY\r\n$5\r\nSTATS\r\n"
    resp = conn.send_command(cmd)
    print(f"MEMORY STATS: {decode_response(resp)}")

    # Test MEMORY DOCTOR command
    print_section("4. Testing MEMORY DOCTOR command")

    cmd = "*2\r\n$6\r\nMEMORY\r\n$6\r\nDOCTOR\r\n"
    resp = conn.send_command(cmd)
    print(f"MEMORY DOCTOR: {decode_response(resp)}")

    # Test MEMORY command via INFO
    print_section("5. Testing memory section in INFO command")

    cmd = "*2\r\n$4\r\nINFO\r\n$6\r\nMEMORY\r\n"
    resp = conn.send_command(cmd)
    print(f"INFO MEMORY: {decode_response(resp)}")

    # Cleanup
    print_section("6. Cleaning up test data")

    for key in keys:
        cmd = f"*2\r\n$3\r\nDEL\r\n${len(key)}\r\n{key}\r\n"
        resp = conn.send_command(cmd)
        print(f"DEL {key}: {decode_response(resp)}")

finally:
    conn.close()

print("\nMemory usage testing complete!")