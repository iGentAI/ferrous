#!/usr/bin/env python3
import socket
import time
import threading
import sys

# Function to send Redis command and get response
def redis_command(cmd, host='127.0.0.1', port=6379):
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect((host, port))
    
    # Send command directly without auth
    s.sendall(cmd.encode())
    resp = s.recv(4096)
    s.close()
    return resp

# Function to monitor Redis commands in a separate thread
def monitor_redis():
    print("Starting MONITOR...")
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect(('127.0.0.1', 6379))
    
    # Start monitoring directly
    s.sendall(b"*1\r\n$7\r\nMONITOR\r\n")
    monitor_resp = s.recv(1024)
    print(f"MONITOR response: {monitor_resp}")
    
    # Read monitor output continuously
    print("\nMonitoring Redis commands (waiting for other clients to execute commands):\n")
    s.settimeout(10) # 10 second timeout
    
    try:
        while True:
            try:
                data = s.recv(4096)
                if not data:
                    print("MONITOR connection closed by server")
                    break
                print(f"MONITOR received: {data}")
            except socket.timeout:
                print("MONITOR timeout - no commands received for 10 seconds")
                break
    except Exception as e:
        print(f"MONITOR error: {e}")
    finally:
        s.close()
        print("MONITOR connection closed")

print("MONITOR Command Test\n")

# Start monitoring in a background thread
monitor_thread = threading.Thread(target=monitor_redis)
monitor_thread.daemon = True
monitor_thread.start()

# Wait a moment for the monitor to start
print("Waiting for monitor to start...")
time.sleep(2)

# Execute some test commands that should show up in the monitor
print("\nExecuting test commands that should appear in monitor output:\n")

# Test command 1: SET
cmd = "*3\r\n$3\r\nSET\r\n$9\r\nmonitorkey\r\n$5\r\nvalue\r\n"
resp = redis_command(cmd)
print(f"SET response: {resp}")
time.sleep(0.5)

# Test command 2: GET
cmd = "*2\r\n$3\r\nGET\r\n$9\r\nmonitorkey\r\n"
resp = redis_command(cmd)
print(f"GET response: {resp}")
time.sleep(0.5)

# Test command 3: DEL
cmd = "*2\r\n$3\r\nDEL\r\n$9\r\nmonitorkey\r\n"
resp = redis_command(cmd)
print(f"DEL response: {resp}")
time.sleep(0.5)

# Test command 4: Slow command to see it in both monitor and slowlog
cmd = "*2\r\n$5\r\nSLEEP\r\n$2\r\n20\r\n"
resp = redis_command(cmd)
print(f"SLEEP response: {resp}")

# Wait for the monitor thread to complete
print("\nWaiting for monitor to process all commands...")
monitor_thread.join(6)  # Wait maximum 6 seconds

print("\nMONITOR test complete!")