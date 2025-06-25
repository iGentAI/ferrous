#!/usr/bin/env python3

import socket
import time

def test_simple_commands():
    # First test basic commands
    print("\nTesting basic Redis commands...")
    cmds = [
        (b"PING", b"+PONG\r\n"),
        (b"SET testkey testvalue", b"+OK\r\n"),
        (b"GET testkey", b"$9\r\ntestvalue\r\n"),
    ]
    
    for cmd, expected in cmds:
        response = send_command(cmd)
        print(f"Command: {cmd}")
        print(f"Response: {response}")
        if expected in response:
            print("✅ PASSED\n")
        else:
            print(f"❌ FAILED - Expected: {expected}\n")

def test_lua_commands():
    # Now test Lua commands
    print("\nTesting Lua EVAL commands...")
    
    # We need to format EVAL commands in raw RESP format to prevent splitting
    # Format: *<num_parts>\r\n$4\r\nEVAL\r\n$<script_len>\r\n<script>\r\n$1\r\n0\r\n
    cmds = [
        # Simple return value
        (b"*3\r\n$4\r\nEVAL\r\n$11\r\nreturn 42\r\n$1\r\n0\r\n", b":42\r\n"),
        
        # Function call
        (b"*3\r\n$4\r\nEVAL\r\n$41\r\nlocal function f() return 'test' end; return f()\r\n$1\r\n0\r\n", b"$4\r\ntest\r\n"),
        
        # String concatenation
        (b"*3\r\n$4\r\nEVAL\r\n$50\r\nlocal t = {foo='bar', baz=42}; return t.foo .. ' ' .. t.baz\r\n$1\r\n0\r\n", b"$6\r\nbar 42\r\n"),
        
        # Use of cjson library
        (b"*3\r\n$4\r\nEVAL\r\n$52\r\nlocal t = {name='test', value=123}; return cjson.encode(t)\r\n$1\r\n0\r\n", b"$"),  # Just check it starts with bulk string marker
    ]
    
    for cmd, expected in cmds:
        response = send_command_raw(cmd)  # Use raw sending for EVAL
        print(f"Command: {cmd}")
        print(f"Response: {response}")
        if expected in response:
            print("✅ PASSED\n")
        else:
            print(f"❌ FAILED - Expected: {expected}\n")

def send_command(cmd):
    """Send a redis command and get the response"""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(2)
    s.connect(('localhost', 6379))
    
    # Convert command to RESP protocol if not already in that format
    if not cmd.startswith(b"*"):
        parts = cmd.split()
        cmd = b"*" + str(len(parts)).encode() + b"\r\n"
        for part in parts:
            cmd += b"$" + str(len(part)).encode() + b"\r\n" + part + b"\r\n"
    
    s.sendall(cmd)
    response = b""
    
    # Read response with timeout protection
    start_time = time.time()
    while time.time() - start_time < 2:
        try:
            chunk = s.recv(4096)
            if not chunk:
                break
            response += chunk
            
            # Check if response is complete (simplistic check)
            if response.endswith(b"\r\n"):
                break
        except socket.timeout:
            break
    
    s.close()
    return response

def send_command_raw(cmd):
    """Send a raw RESP command without any processing"""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(2)
    s.connect(('localhost', 6379))
    
    s.sendall(cmd)
    response = b""
    
    # Read response with timeout protection
    start_time = time.time()
    while time.time() - start_time < 2:
        try:
            chunk = s.recv(4096)
            if not chunk:
                break
            response += chunk
            
            # Check if response is complete (simplistic check)
            if response.endswith(b"\r\n"):
                break
        except socket.timeout:
            break
    
    s.close()
    return response

if __name__ == "__main__":
    # Wait for server to start
    time.sleep(1)
    test_simple_commands()
    test_lua_commands()
