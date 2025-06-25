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

# Function to monitor Redis commands in a separate thread
def monitor_redis():
    print("Starting MONITOR in background thread...")
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect(('127.0.0.1', 6379))
    
    # Authenticate
    auth_cmd = f"*2\r\n$4\r\nAUTH\r\n${len('mysecretpassword')}\r\nmysecretpassword\r\n"
    s.sendall(auth_cmd.encode())
    auth_resp = s.recv(1024)
    
    # Start monitoring
    s.sendall(b"*1\r\n$7\r\nMONITOR\r\n")
    monitor_resp = s.recv(1024)
    print(f"MONITOR started successfully: {monitor_resp.strip().decode('utf-8', errors='replace')}")
    
    # Read monitor output continuously
    print("Ready to receive command broadcasts:")
    s.settimeout(15) # 15 second timeout
    
    commands_received = 0
    try:
        while commands_received < 5:  # Exit after receiving 5 commands
            try:
                data = s.recv(4096)
                if not data:
                    print("MONITOR connection closed by server")
                    break
                    
                data_str = data.decode('utf-8', errors='replace')
                print(f"MONITOR broadcast #{commands_received + 1}: {data_str.strip()}")
                commands_received += 1
                
            except socket.timeout:
                print("MONITOR timeout - no commands received for 15 seconds")
                break
    except Exception as e:
        print(f"MONITOR error: {e}")
    finally:
        s.close()
        print("MONITOR connection closed")

print("Enhanced MONITOR Test - Multiple Commands\n")

# Start monitoring in a background thread
monitor_thread = threading.Thread(target=monitor_redis)
monitor_thread.daemon = True
monitor_thread.start()

# Wait longer for the monitor to start
print("Waiting for monitor to initialize...")
time.sleep(3)

print("\nExecuting sequence of commands:\n")

# Execute multiple commands with proper delay between them
commands = [
    ("*3\r\n$3\r\nSET\r\n$6\r\ntest:1\r\n$6\r\nvalue1\r\n", "SET test:1"),
    ("*3\r\n$3\r\nSET\r\n$6\r\ntest:2\r\n$6\r\nvalue2\r\n", "SET test:2"),
    ("*2\r\n$4\r\nMGET\r\n$6\r\ntest:1\r\n", "MGET test:1"),
    ("*2\r\n$4\r\nMGET\r\n$6\r\ntest:2\r\n", "MGET test:2"),
    ("*2\r\n$5\r\nSLEEP\r\n$1\r\n5\r\n", "SLEEP 5")
]

# Execute each command with delay
for i, (cmd, desc) in enumerate(commands):
    print(f"Executing command {i+1}: {desc}")
    resp = redis_command(cmd)
    print(f"Response: {resp}")
    time.sleep(1)  # Longer delay to ensure proper ordering in MONITOR output

# Wait a bit longer for the monitor to process all commands
print("\nWaiting for monitor to finish capturing commands...")
monitor_thread.join(10)

print("\nMONITOR test complete!")
