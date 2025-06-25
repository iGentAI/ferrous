#!/usr/bin/env python3
import socket
import time

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
        
    if not resp.startswith(b'*'):
        print(f"Unexpected response format: {resp}")
        return None
        
    try:
        # Very basic array parsing - just count the elements
        parts = resp.split(b'\r\n')
        if len(parts) > 1 and parts[0].startswith(b'*'):
            count = int(parts[0][1:])
            if count > 0:
                return count
    except Exception as e:
        print(f"Error parsing response: {e}")
        return None
        
    return []

print("Testing SLOWLOG with SLEEP command\n")

# Clear any existing slowlog
print("1. Clearing existing slowlog entries...")
redis_command("*2\r\n$7\r\nSLOWLOG\r\n$5\r\nRESET\r\n")

# Check current threshold
print("\n2. Checking current slowlog threshold...")
cmd = "*3\r\n$6\r\nCONFIG\r\n$3\r\nGET\r\n$22\r\nslowlog-log-slower-than\r\n"
resp = redis_command(cmd)
print(f"Response: {resp}")

# Set threshold to 5ms (5000 microseconds)
print("\n3. Setting slowlog-log-slower-than to 5000 microseconds (5ms)...")
cmd = "*3\r\n$6\r\nCONFIG\r\n$3\r\nSET\r\n$22\r\nslowlog-log-slower-than\r\n$4\r\n5000\r\n"
resp = redis_command(cmd)
print(f"Response: {resp}")

# Check SLOWLOG is empty
print("\n4. Checking SLOWLOG is empty...")
cmd = "*2\r\n$7\r\nSLOWLOG\r\n$3\r\nLEN\r\n"
resp = redis_command(cmd)
len_result = parse_int_response(resp)
print(f"SLOWLOG LEN: {len_result}")

# Execute fast command
print("\n5. Executing a normal (fast) SET command...")
cmd = "*3\r\n$3\r\nSET\r\n$4\r\nfast\r\n$4\r\ntest\r\n"
resp = redis_command(cmd)
print(f"Response: {resp}")

# Check if the fast command was logged
print("\n6. Checking if normal command was logged...")
cmd = "*2\r\n$7\r\nSLOWLOG\r\n$3\r\nLEN\r\n"
resp = redis_command(cmd)
len_result = parse_int_response(resp)
print(f"SLOWLOG LEN: {len_result} (should still be 0)")

# Execute SLEEP command that should be slow
print("\n7. Executing SLEEP command with 20ms delay (should be logged)...")
cmd = "*2\r\n$5\r\nSLEEP\r\n$2\r\n20\r\n"
resp = redis_command(cmd)
print(f"Response: {resp}")

# Check if the SLEEP command was logged
print("\n8. Checking if SLEEP command was logged...")
cmd = "*2\r\n$7\r\nSLOWLOG\r\n$3\r\nLEN\r\n"
resp = redis_command(cmd)
len_result = parse_int_response(resp)
print(f"SLOWLOG LEN: {len_result} (should be 1)")

# Get the slowlog entry details
print("\n9. Getting SLOWLOG entry details...")
cmd = "*2\r\n$7\r\nSLOWLOG\r\n$3\r\nGET\r\n"
resp = redis_command(cmd)
entry_count = parse_array_response(resp)
print(f"Response contains {entry_count} entries: {resp[:200]}...")

# Execute second SLEEP command with longer delay
print("\n10. Executing SLEEP command with 50ms delay (should be logged)...")
cmd = "*2\r\n$5\r\nSLEEP\r\n$2\r\n50\r\n"
resp = redis_command(cmd)
print(f"Response: {resp}")

# Check updated slowlog length
print("\n11. Checking updated SLOWLOG length...")
cmd = "*2\r\n$7\r\nSLOWLOG\r\n$3\r\nLEN\r\n"
resp = redis_command(cmd)
len_result = parse_int_response(resp)
print(f"SLOWLOG LEN: {len_result} (should be 2)")

# Reset the threshold back to default
print("\n12. Resetting slowlog-log-slower-than back to 10000 microseconds...")
cmd = "*3\r\n$6\r\nCONFIG\r\n$3\r\nSET\r\n$22\r\nslowlog-log-slower-than\r\n$5\r\n10000\r\n"
resp = redis_command(cmd)
print(f"Response: {resp}")

# Reset the SLOWLOG
print("\n13. Clearing the SLOWLOG...")
cmd = "*2\r\n$7\r\nSLOWLOG\r\n$5\r\nRESET\r\n"
resp = redis_command(cmd)
print(f"Response: {resp}")

print("\nSLOWLOG test with SLEEP command completed!")
