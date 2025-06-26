#!/usr/bin/env python3
"""
Test script for the GIL-based Lua VM implementation in Ferrous.
This script tests KEYS/ARGV access and redis.call/pcall which were previously failing.
"""

import socket
import time
import sys

def send_command(command, debug=True):
    """Send a command to Ferrous and return the response."""
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.settimeout(5)
    
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
                    if debug:
                        print('[Connection closed after response]')
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
    if resp and resp.startswith(b'+PONG\r\n'):
        print("✓ Server is responsive")
        return True
    else:
        print(f"✗ Server not responding properly: {resp}")
        return False

def run_lua_test(title, script, keys=None, args=None):
    """Run a Lua test with the given script, keys, and args."""
    print(f"\n=== {title} ===")
    
    # Prepare EVAL command
    if keys is None:
        keys = []
    if args is None:
        args = []
    
    numkeys = len(keys)
    
    # Encode script
    script_bytes = script.encode('utf-8')
    script_len = len(script_bytes)
    
    # Build command parts
    cmd_parts = [
        b'*' + str(3 + numkeys + len(args)).encode('utf-8') + b'\r\n',
        b'$4\r\nEVAL\r\n',
        b'$' + str(script_len).encode('utf-8') + b'\r\n',
        script_bytes + b'\r\n',
        b'$' + str(len(str(numkeys))).encode('utf-8') + b'\r\n',
        str(numkeys).encode('utf-8') + b'\r\n'
    ]
    
    # Add keys
    for key in keys:
        key_bytes = key.encode('utf-8')
        cmd_parts.extend([
            b'$' + str(len(key_bytes)).encode('utf-8') + b'\r\n',
            key_bytes + b'\r\n'
        ])
    
    # Add args
    for arg in args:
        arg_bytes = arg.encode('utf-8')
        cmd_parts.extend([
            b'$' + str(len(arg_bytes)).encode('utf-8') + b'\r\n',
            arg_bytes + b'\r\n'
        ])
    
    # Send command
    cmd = b''.join(cmd_parts)
    resp = send_command(cmd)
    
    # Check response
    if resp:
        print(f"✓ Success: Got response {resp}")
        return resp
    else:
        print(f"✗ Failed: No response")
        return None

def main():
    """Run the Lua GIL tests."""
    print("=== Ferrous Lua GIL Tests ===")
    print("This script tests the GIL-based Lua VM implementation focusing on")
    print("KEYS/ARGV access and redis.call/pcall which were previously failing.")
    
    # Check if server is up
    if not test_server_alive():
        print("ERROR: Server is not running on localhost:6379")
        print("Please start Ferrous first: ./target/release/ferrous --port 6379")
        return 1
    
    # Set a test key
    set_cmd = b'*3\r\n$3\r\nSET\r\n$8\r\ntestkey1\r\n$10\r\ntestvalue1\r\n'
    resp = send_command(set_cmd)
    print(f"\nSetting test key: {resp}")
    
    # Run basic tests first
    run_lua_test("Basic Lua Test", "return 'GIL test'")
    run_lua_test("Table Test", "local t = {a=123}; return t.a")
    
    # Test KEYS access
    run_lua_test("KEYS Access Test", "return KEYS[1]", ["testkey1"])
    
    # Test ARGV access
    run_lua_test("ARGV Access Test", "return ARGV[1]", [], ["testarg1"])
    
    # Test redis.call
    run_lua_test("redis.call Test", "return redis.call('PING')")
    
    # Test redis.call with GET
    run_lua_test("redis.call GET Test", "return redis.call('GET', KEYS[1])", ["testkey1"])
    
    # Test redis.pcall
    run_lua_test("redis.pcall Test", "return redis.pcall('PING')")
    
    # Test error handling in pcall
    run_lua_test("redis.pcall Error Handling", 
                "local result = redis.pcall('UNKNOWN_CMD'); return type(result)")
    
    # Test transaction semantics
    transaction_test = """
    redis.call('SET', KEYS[1], 'transaction-test')
    local current = redis.call('GET', KEYS[1])
    assert(current == 'transaction-test', 'Transaction failed - wrong value')
    if ARGV[1] == 'fail' then
        error('Simulated failure')
    end
    return 'Transaction successful'
    """
    print("\n=== Testing Transaction Success ===")
    run_lua_test("Transaction Success", transaction_test, ["testkey1"], ["success"])
    
    # Check value after success
    get_cmd = b'*2\r\n$3\r\nGET\r\n$8\r\ntestkey1\r\n'
    resp = send_command(get_cmd)
    print(f"\nValue after successful transaction: {resp}")
    
    print("\n=== Testing Transaction Rollback ===")
    run_lua_test("Transaction Rollback", transaction_test, ["testkey1"], ["fail"])
    
    # Check value after rollback
    resp = send_command(get_cmd)
    print(f"\nValue after failed transaction (should be rolled back): {resp}")
    
    print("\n=== Test Summary ===")
    print("All tests have been completed. Check the responses to see if the")
    print("GIL implementation has resolved the issues with KEYS/ARGV access")
    print("and redis.call/pcall functionality.")
    
    return 0

if __name__ == "__main__":
    sys.exit(main())