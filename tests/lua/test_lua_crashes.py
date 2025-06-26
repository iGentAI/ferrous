#!/usr/bin/env python3
"""
Test script to reproduce all known Lua VM crashes in Ferrous.

This script tests the specific functionality that causes the server to crash:
1. KEYS array access
2. ARGV array access 
3. redis.call() function
4. redis.pcall() function
"""

import socket
import time
import sys

def send_command(command, debug=True):
    """Send a command to Ferrous and return the response."""
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.settimeout(1) # Use a shorter timeout for responsiveness
    
    try:
        sock.connect(('localhost', 6379))
    except Exception as e:
        print(f"Failed to connect: {e}")
        return None
    
    if debug:
        print(f'[Sending]: {command}')
        
    try:
        sock.send(command)
        
        # Get the response with proper timeout handling
        response = b''
        try:
            while True:
                chunk = sock.recv(1024)
                if not chunk:
                    if debug and response:
                        print('[Connection closed after response]')
                    elif debug:
                        print('[Connection closed immediately]')
                    break
                
                if debug:
                    print(f'[Received]: {chunk}')
                response += chunk
                
                # If we got a complete response, don't wait for more
                if response.endswith(b'\r\n'):
                    break
        except socket.timeout:
            if debug:
                if response:
                    print('[Timeout after partial response]')
                else:
                    print('[Timeout - no response]')
    except Exception as e:
        if debug:
            print(f'[Error]: {e}')
        response = b''
    
    sock.close()
    return response

def test_server_alive():
    """Test if server is alive with a simple PING."""
    print("\n=== Testing server connectivity ===")
    ping = b'*1\r\n$4\r\nPING\r\n'
    resp = send_command(ping)
    if resp and resp == b'+PONG\r\n':
        print("✓ Server is responsive")
        return True
    else:
        print(f"✗ Server not responding properly: {resp}")
        return False

def test_basic_lua():
    """Test basic Lua functionality that should work."""
    print("\n=== Testing basic Lua (should work) ===")
    
    tests = [
        ("Simple return", b'*3\r\n$4\r\nEVAL\r\n$8\r\nreturn 1\r\n$1\r\n0\r\n'),
        ("String return", b'*3\r\n$4\r\nEVAL\r\n$13\r\nreturn "test"\r\n$1\r\n0\r\n'),
        ("Table access", b'*3\r\n$4\r\nEVAL\r\n$25\r\nlocal t={a=5}; return t.a\r\n$1\r\n0\r\n'),
    ]
    
    for name, cmd in tests:
        print(f"\nTest: {name}")
        resp = send_command(cmd)
        if resp:
            print(f"✓ Success: Got response {resp}")
        else:
            print(f"✗ Failed: No response")
        time.sleep(0.5)  # Give server time to recover if needed

def test_keys_access():
    """Test KEYS array access - known to crash."""
    print("\n=== Testing KEYS array access (known to crash) ===")
    
    # First, set a value
    set_cmd = b'*3\r\n$3\r\nSET\r\n$4\r\nkey1\r\n$6\r\nvalue1\r\n'
    print("\nSetting up test key...")
    resp = send_command(set_cmd)
    print(f"SET response: {resp}")
    
    time.sleep(0.5)
    
    # Test 1: Simple KEYS[1] access
    print("\nTest: Accessing KEYS[1]")
    keys_cmd = b'*4\r\n$4\r\nEVAL\r\n$15\r\nreturn KEYS[1]\r\n$1\r\n1\r\n$4\r\nkey1\r\n'
    resp = send_command(keys_cmd)
    if not resp:
        print("✗ Server likely crashed on KEYS[1] access")
    else:
        print(f"✓ Got response: {resp}")

def test_argv_access():
    """Test ARGV array access."""
    print("\n=== Testing ARGV array access ===")
    
    # Test server is still alive
    if not test_server_alive():
        print("Server is down, skipping ARGV tests")
        return
    
    # Test 1: Simple ARGV[1] access
    print("\nTest: Accessing ARGV[1]")
    argv_cmd = b'*4\r\n$4\r\nEVAL\r\n$15\r\nreturn ARGV[1]\r\n$1\r\n0\r\n$4\r\narg1\r\n'
    resp = send_command(argv_cmd)
    if not resp:
        print("✗ Server likely crashed on ARGV[1] access")
    else:
        print(f"✓ Got response: {resp}")

def test_redis_call():
    """Test redis.call() function - known to crash."""
    print("\n=== Testing redis.call() (known to crash) ===")
    
    # Test server is still alive
    if not test_server_alive():
        print("Server is down, skipping redis.call tests")
        return
    
    # Test 1: Simple PING via redis.call
    print("\nTest: redis.call('PING')")
    call_cmd = b'*3\r\n$4\r\nEVAL\r\n$24\r\nreturn redis.call("PING")\r\n$1\r\n0\r\n'
    resp = send_command(call_cmd)
    if not resp:
        print("✗ Server likely crashed on redis.call('PING')")
    else:
        print(f"✓ Got response: {resp}")
    
    time.sleep(0.5)
    
    # Test 2: GET via redis.call
    if test_server_alive():
        print("\nTest: redis.call('GET', KEYS[1])")
        get_cmd = b'*4\r\n$4\r\nEVAL\r\n$32\r\nreturn redis.call("GET", KEYS[1])\r\n$1\r\n1\r\n$4\r\nkey1\r\n'
        resp = send_command(get_cmd)
        if not resp:
            print("✗ Server likely crashed on redis.call('GET', KEYS[1])")
        else:
            print(f"✓ Got response: {resp}")

def test_redis_pcall():
    """Test redis.pcall() function."""
    print("\n=== Testing redis.pcall() ===")
    
    # Test server is still alive
    if not test_server_alive():
        print("Server is down, skipping redis.pcall tests")
        return
    
    # Test: Simple PING via redis.pcall
    print("\nTest: redis.pcall('PING')")
    pcall_cmd = b'*3\r\n$4\r\nEVAL\r\n$25\r\nreturn redis.pcall("PING")\r\n$1\r\n0\r\n'
    resp = send_command(pcall_cmd)
    if not resp:
        print("✗ Server likely crashed on redis.pcall('PING')")
    else:
        print(f"✓ Got response: {resp}")

def main():
    """Run all tests."""
    print("=== Ferrous Lua VM Crash Tests ===")
    print("This script tests known scenarios that crash the server.")
    print("The server may need to be restarted between tests.\n")
    
    # Check if server is up
    if not test_server_alive():
        print("ERROR: Server is not running on localhost:6379")
        print("Please start Ferrous first: ./target/release/ferrous --port 6379")
        return 1
    
    # Run tests in order of likelihood to crash
    test_basic_lua()      # Should work
    test_keys_access()    # Known to crash
    test_argv_access()    # May crash
    test_redis_call()     # Known to crash
    test_redis_pcall()    # May crash
    
    print("\n=== Test Summary ===")
    print("Check the server logs to see which operations caused crashes.")
    print("The server will need to be restarted after crashes.")
    
    return 0

if __name__ == "__main__":
    sys.exit(main())