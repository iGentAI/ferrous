#!/usr/bin/env python3
import socket
import time
import threading

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

def decode_resp(resp):
    """Basic RESP decoding for human-readable output"""
    try:
        if resp.startswith(b'$'):
            # Bulk string
            parts = resp.split(b'\r\n', 2)
            if len(parts) >= 3:
                return f"BULK: {parts[1].decode('utf-8', errors='replace')}"
        elif resp.startswith(b'+'):
            # Simple string
            return f"STRING: {resp[1:].strip().decode('utf-8', errors='replace')}"
        elif resp.startswith(b':'):
            # Integer
            return f"INT: {resp[1:].strip().decode('utf-8', errors='replace')}"
        elif resp.startswith(b'*'):
            # Array - simplified
            if resp == b'*0\r\n':
                return "ARRAY: (empty)"
            else:
                return f"ARRAY with {resp[1:resp.find(b'\r')].decode()} elements"
        
        # Fall back to showing raw bytes for complex responses
        return f"RAW: {resp[:100]}{'...' if len(resp) > 100 else ''}"
    except Exception as e:
        return f"Error decoding response: {e} - Raw: {resp}"

print("\n===== TESTING CLIENT COMMANDS =====\n")

# Test CLIENT LIST
print("1. Testing CLIENT LIST command\n")
resp = redis_command("*2\r\n$6\r\nCLIENT\r\n$4\r\nLIST\r\n")
print(f"Response: {decode_resp(resp)}\n")
print(f"Raw: {resp}\n")

# Test CLIENT ID
print("2. Testing CLIENT ID command\n")
resp = redis_command("*2\r\n$6\r\nCLIENT\r\n$2\r\nID\r\n")
print(f"Response: {decode_resp(resp)}\n")

# Test CLIENT SETNAME
print("3. Testing CLIENT SETNAME command\n")
resp = redis_command("*3\r\n$6\r\nCLIENT\r\n$7\r\nSETNAME\r\n$9\r\ntest-name\r\n")
print(f"Response: {decode_resp(resp)}\n")

# Test CLIENT GETNAME
print("4. Testing CLIENT GETNAME command\n")
resp = redis_command("*2\r\n$6\r\nCLIENT\r\n$7\r\nGETNAME\r\n")
print(f"Response: {decode_resp(resp)}\n")

# Test CLIENT LIST again to see if name appears
print("5. Testing CLIENT LIST again to see named client\n")
resp = redis_command("*2\r\n$6\r\nCLIENT\r\n$4\r\nLIST\r\n")
print(f"Response: {decode_resp(resp)}\n")
print(f"Raw: {resp}\n")

print("6. Testing CLIENT KILL BY ID\n")
# Create a connection that we'll keep open, then kill from another connection
def keep_connection_open():
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect(('127.0.0.1', 6379))
    
    # Authenticate
    auth_cmd = f"*2\r\n$4\r\nAUTH\r\n${len('mysecretpassword')}\r\nmysecretpassword\r\n"
    s.sendall(auth_cmd.encode())
    auth_resp = s.recv(1024)
    
    # Set a name on this connection for easy identification
    name_cmd = "*3\r\n$6\r\nCLIENT\r\n$7\r\nSETNAME\r\n$10\r\ntarget-conn\r\n"
    s.sendall(name_cmd.encode())
    name_resp = s.recv(1024)
    
    # Get the client ID
    id_cmd = "*2\r\n$6\r\nCLIENT\r\n$2\r\nID\r\n"
    s.sendall(id_cmd.encode())
    id_resp = s.recv(1024)
    
    print(f"Created target connection with ID: {id_resp.strip().decode('utf-8', errors='replace')}")
    
    # Keep connection open for 10 seconds
    try:
        s.settimeout(10)
        data = s.recv(1024)  # This will block until data received or timeout
        print(f"Target connection received unexpected data: {data}")
    except socket.timeout:
        print("Target connection timed out as expected")
    except Exception as e:
        print(f"Target connection closed: {e}")
    finally:
        s.close()
        print("Target connection closed")

# Start the target connection in a thread
target_thread = threading.Thread(target=keep_connection_open)
target_thread.daemon = True
target_thread.start()

# Wait for it to initialize
time.sleep(1)  

# Get list of client IDs
resp = redis_command("*2\r\n$6\r\nCLIENT\r\n$4\r\nLIST\r\n")
client_list = resp.decode('utf-8', errors='replace')
print("Client list:\n" + client_list)

# Find a client ID with 'target-conn' name
target_id = None
for line in client_list.split('\n'):
    if 'target-conn' in line:
        parts = line.split(' ')
        for part in parts:
            if part.startswith('id='):
                target_id = part.split('=')[1]
                break
    if target_id:
        break

if target_id:
    print(f"\nFound target connection with ID: {target_id}\n")
    # Kill the target connection
    resp = redis_command(f"*4\r\n$6\r\nCLIENT\r\n$4\r\nKILL\r\n$2\r\nID\r\n${len(target_id)}\r\n{target_id}\r\n")
    print(f"KILL response: {decode_resp(resp)}\n")
    
    # Check if the client is still in the list
    time.sleep(0.5)  # Give server time to process the kill
    resp = redis_command("*2\r\n$6\r\nCLIENT\r\n$4\r\nLIST\r\n")
    after_kill_list = resp.decode('utf-8', errors='replace')
    print("Client list after KILL:\n" + after_kill_list)
    
    # Check if target client is gone
    if f"id={target_id}" in after_kill_list:
        print(f"\nKILL FAILED: target client id={target_id} still in list\n")
    else:
        print(f"\nKILL SUCCEEDED: target client id={target_id} removed from list\n")
else:
    print("Could not find the target connection ID")

# Test CLIENT PAUSE
print("\n7. Testing CLIENT PAUSE command\n")

# Start a background thread that will try to execute commands during pause
def send_commands_during_pause():
    # Wait a moment for pause to take effect
    time.sleep(1)
    
    # Try a command that should be rejected during pause
    print("Sending GET command during PAUSE (should be rejected)...")
    resp = redis_command("*2\r\n$3\r\nGET\r\n$4\r\ntest\r\n")
    print(f"Response: {decode_resp(resp)}")
    
    # Wait until after pause should expire
    time.sleep(3)
    
    # Try again - should work now
    print("\nSending GET command after PAUSE (should succeed)...")
    resp = redis_command("*2\r\n$3\r\nGET\r\n$4\r\ntest\r\n")
    print(f"Response: {decode_resp(resp)}")

# First set a key for testing
redis_command("*3\r\n$3\r\nSET\r\n$4\r\ntest\r\n$5\r\nvalue\r\n")

# Pause the server for 3 seconds
print("Pausing all clients for 3 seconds...")
resp = redis_command("*3\r\n$6\r\nCLIENT\r\n$5\r\nPAUSE\r\n$4\r\n3000\r\n")
print(f"CLIENT PAUSE response: {decode_resp(resp)}")

# Start thread to test during pause
pause_thread = threading.Thread(target=send_commands_during_pause)
pause_thread.daemon = True
pause_thread.start()

# Wait for the pause thread to complete
pause_thread.join(7)  # Maximum 7 seconds wait

# Make sure we can execute commands after pause
resp = redis_command("*2\r\n$3\r\nGET\r\n$4\r\ntest\r\n")
print(f"\nFinal GET after PAUSE: {decode_resp(resp)}")

# Wait for target thread to complete
print("\nWaiting for all test threads to complete...")
target_thread.join(2)

print("\n===== CLIENT COMMANDS TESTING COMPLETE =====")
