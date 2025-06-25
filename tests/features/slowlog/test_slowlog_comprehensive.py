#!/usr/bin/env python3
import socket
import time
import sys

# Function to send Redis command and get response
def redis_command(cmd, host='127.0.0.1', port=6379, password='mysecretpassword'):
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect((host, port))
    
    # First authenticate
    auth_cmd = f"*2\r\n$4\r\nAUTH\r\n${len(password)}\r\n{password}\r\n"
    s.sendall(auth_cmd.encode())
    auth_resp = s.recv(1024)
    
    # Send command
    s.sendall(cmd.encode())
    resp = s.recv(4096)
    s.close()
    return resp

def parse_int_response(resp):
    # Simple parser for Redis integer responses
    try:
        if resp.startswith(b':'):
            return int(resp.strip()[1:])
        return None
    except:
        return None

def parse_array_response(resp):
    # Extremely simplified RESP parser just to handle SLOWLOG GET
    # Real code would use a proper RESP parser
    if resp.startswith(b'*0\r\n'):
        return []
        
    try:
        parts = resp.split(b'\r\n', 2)[1:]
        if not parts:
            return []
            
        if parts[0].startswith(b'*'):
            count = int(parts[0][1:])
            entries = []
            # Just assume we got something and return a count
            return 'ENTRIES_FOUND'
    except:
        return []
        
    return []

print("Comprehensive SLOWLOG Testing Script\n")

# Clear any existing slowlog
print("1. Clearing existing slowlog entries...")
redis_command("*2\r\n$7\r\nSLOWLOG\r\n$5\r\nRESET\r\n")

# Check current threshold
print("\n2. Checking current slowlog threshold...")
cmd = "*3\r\n$6\r\nCONFIG\r\n$3\r\nGET\r\n$22\r\nslowlog-log-slower-than\r\n"
resp = redis_command(cmd)
print(f"Response: {resp}")

# Set threshold to 1ms (1000 microseconds)
print("\n3. Setting slowlog-log-slower-than to 1000 microseconds (1ms)...")
cmd = "*3\r\n$6\r\nCONFIG\r\n$3\r\nSET\r\n$22\r\nslowlog-log-slower-than\r\n$4\r\n1000\r\n"
resp = redis_command(cmd)
print(f"Response: {resp}")

# Perform a SET command with artificial delay to ensure it's slow
print("\n4. Performing a SLOW SET command (sleeping for 100ms)...")
time.sleep(0.1)  # Sleep 100ms to ensure it's slow
cmd = "*3\r\n$3\r\nSET\r\n$8\r\ntestkey1\r\n$9\r\ntestvalue\r\n"
resp = redis_command(cmd)
print(f"Response: {resp}")

# Perform another slow command
print("\n5. Performing a SLOW GET command (sleeping for 50ms)...")
time.sleep(0.05)  # Sleep 50ms to ensure it's slow
cmd = "*2\r\n$3\r\nGET\r\n$8\r\ntestkey1\r\n"
resp = redis_command(cmd)
print(f"Response: {resp}")

# Sleep a moment to ensure commands are processed
print("\n6. Sleeping 1 second to ensure commands are processed...")
time.sleep(1)

# Check the SLOWLOG length
print("\n7. Checking SLOWLOG LEN...")
cmd = "*2\r\n$7\r\nSLOWLOG\r\n$3\r\nLEN\r\n"
resp = redis_command(cmd)
len_result = parse_int_response(resp)
print(f"Response: {resp}")
print(f"Parsed length: {len_result if len_result is not None else 'unknown'}")

# Check the SLOWLOG entries
print("\n8. Checking SLOWLOG GET...")
cmd = "*2\r\n$7\r\nSLOWLOG\r\n$3\r\nGET\r\n"
resp = redis_command(cmd)
entries = parse_array_response(resp)
print(f"Got response: {resp[:100]}{'...' if len(resp) > 100 else ''}")
print(f"Parsed entries: {entries}")

# Reset the threshold back to default
print("\n9. Resetting slowlog-log-slower-than back to 10000 microseconds...")
cmd = "*3\r\n$6\r\nCONFIG\r\n$3\r\nSET\r\n$22\r\nslowlog-log-slower-than\r\n$5\r\n10000\r\n"
resp = redis_command(cmd)
print(f"Response: {resp}")

print("\nTest completed!")
