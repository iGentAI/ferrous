#!/usr/bin/env python3
"""
Test script for Ferrous MEMORY command functionality.
"""

import socket
import time
import sys

def send_command(command, host='127.0.0.1', port=6379, password='mysecretpassword'):
    """Send a Redis command and return the response"""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect((host, port))
    
    # Authenticate
    auth_cmd = f"*2\r\n$4\r\nAUTH\r\n${len(password)}\r\n{password}\r\n"
    s.sendall(auth_cmd.encode())
    auth_resp = s.recv(1024)
    
    # Send command
    s.sendall(command.encode())
    resp = s.recv(4096)
    
    s.close()
    return resp

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

# Start the test
print("Testing MEMORY commands in Ferrous\n")

# Create some data to track memory usage
print_section("1. Creating test data to track memory usage")

# Create a large string
large_value = "x" * 1000000  # 1MB string
cmd = f"*3\r\n$3\r\nSET\r\n$9\r\nlarge-key\r\n${len(large_value)}\r\n{large_value}\r\n"
resp = send_command(cmd)
print(f"SET large-key: {decode_response(resp)}")

# Create a large list
print("\nCreating a large list...")
cmd = "*3\r\n$5\r\nLPUSH\r\n$10\r\nlarge-list\r\n$6\r\nvalue1\r\n"
resp = send_command(cmd)
for i in range(10000):
    cmd = f"*3\r\n$5\r\nLPUSH\r\n$10\r\nlarge-list\r\n$6\r\nvalue{i}\r\n"
    resp = send_command(cmd)
    if i % 1000 == 0:
        print(f"Added {i} items...")
print(f"LPUSH result: {decode_response(resp)}")

# Create a moderate sized hash
print("\nCreating a hash...")
for i in range(1000):
    field = f"field{i}"
    value = f"value{i}" * 10
    cmd = f"*4\r\n$4\r\nHSET\r\n$9\r\ntest-hash\r\n${len(field)}\r\n{field}\r\n${len(value)}\r\n{value}\r\n"
    resp = send_command(cmd)

print(f"HSET result: {decode_response(resp)}")

# Test MEMORY USAGE command
print_section("2. Testing MEMORY USAGE command")

keys = ["large-key", "large-list", "test-hash"]
for key in keys:
    cmd = f"*3\r\n$6\r\nMEMORY\r\n$5\r\nUSAGE\r\n${len(key)}\r\n{key}\r\n"
    resp = send_command(cmd)
    print(f"MEMORY USAGE {key}: {decode_response(resp)}")

# Test MEMORY STATS command
print_section("3. Testing MEMORY STATS command")

cmd = "*2\r\n$6\r\nMEMORY\r\n$5\r\nSTATS\r\n"
resp = send_command(cmd)
print(f"MEMORY STATS: {decode_response(resp)}")

# Test MEMORY DOCTOR command
print_section("4. Testing MEMORY DOCTOR command")

cmd = "*2\r\n$6\r\nMEMORY\r\n$6\r\nDOCTOR\r\n"
resp = send_command(cmd)
print(f"MEMORY DOCTOR: {decode_response(resp)}")

# Test MEMORY command via INFO
print_section("5. Testing memory section in INFO command")

cmd = "*2\r\n$4\r\nINFO\r\n$6\r\nMEMORY\r\n"
resp = send_command(cmd)
print(f"INFO MEMORY: {decode_response(resp)}")

# Cleanup
print_section("6. Cleaning up test data")

for key in keys:
    cmd = f"*2\r\n$3\r\nDEL\r\n${len(key)}\r\n{key}\r\n"
    resp = send_command(cmd)
    print(f"DEL {key}: {decode_response(resp)}")

print("\nMemory usage testing complete!")