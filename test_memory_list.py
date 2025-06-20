#!/usr/bin/env python3
import socket
import time
import sys

def send_command(command, host='127.0.0.1', port=6379, password='mysecretpassword'):
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

print("List Memory Usage Test\n")

# Start with empty list memory check
cmd = "*2\r\n$3\r\nDEL\r\n$9\r\ntest-list\r\n"
send_command(cmd)

# Create a simple list with one element and check
cmd = "*3\r\n$5\r\nLPUSH\r\n$9\r\ntest-list\r\n$5\r\nvalue\r\n"
resp = send_command(cmd)
print(f"LPUSH result: {decode_response(resp)}")

cmd = "*3\r\n$6\r\nMEMORY\r\n$5\r\nUSAGE\r\n$9\r\ntest-list\r\n"
resp = send_command(cmd)
print(f"MEMORY USAGE with 1 element: {decode_response(resp)}")

# Add another element and check again
cmd = "*3\r\n$5\r\nLPUSH\r\n$9\r\ntest-list\r\n$6\r\nvalue2\r\n"
resp = send_command(cmd)
print(f"LPUSH result: {decode_response(resp)}")

cmd = "*3\r\n$6\r\nMEMORY\r\n$5\r\nUSAGE\r\n$9\r\ntest-list\r\n"
resp = send_command(cmd)
print(f"MEMORY USAGE with 2 elements: {decode_response(resp)}")

# Add 100 more elements and check again
for i in range(100):
    cmd = f"*3\r\n$5\r\nLPUSH\r\n$9\r\ntest-list\r\n$7\r\nvalue{i}\r\n"
    send_command(cmd)

cmd = "*3\r\n$6\r\nMEMORY\r\n$5\r\nUSAGE\r\n$9\r\ntest-list\r\n"
resp = send_command(cmd)
print(f"MEMORY USAGE with 102 elements: {decode_response(resp)}")

# Get list length to confirm
cmd = "*2\r\n$4\r\nLLEN\r\n$9\r\ntest-list\r\n"
resp = send_command(cmd)
print(f"LLEN result: {decode_response(resp)}")

# Clean up
cmd = "*2\r\n$3\r\nDEL\r\n$9\r\ntest-list\r\n"
resp = send_command(cmd)
print(f"DEL result: {decode_response(resp)}")

print("\nList Memory Usage Test Completed")
