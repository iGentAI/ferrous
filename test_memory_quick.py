#!/usr/bin/env python3
import socket
import time

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

print("Quick Memory Test\n")

# Test list memory
print("Testing List Memory:\n")

# Start clean
cmd = "*2\r\n$3\r\nDEL\r\n$9\r\ntest-list\r\n"
send_command(cmd)

# Add 1 element
cmd = "*3\r\n$5\r\nLPUSH\r\n$9\r\ntest-list\r\n$5\r\nvalue\r\n"
send_command(cmd)

# Check memory usage with 1 element
cmd = "*3\r\n$6\r\nMEMORY\r\n$5\r\nUSAGE\r\n$9\r\ntest-list\r\n"
resp = send_command(cmd)
print(f"Memory usage with 1 element: {int(resp[1:].strip())} bytes")

# Add 9 more elements (total 10)
for i in range(9):
    cmd = f"*3\r\n$5\r\nLPUSH\r\n$9\r\ntest-list\r\n$6\r\nvalue{i}\r\n"
    send_command(cmd)

# Check memory usage with 10 elements
cmd = "*3\r\n$6\r\nMEMORY\r\n$5\r\nUSAGE\r\n$9\r\ntest-list\r\n"
resp = send_command(cmd)
print(f"Memory usage with 10 elements: {int(resp[1:].strip())} bytes")

# Add 40 more elements (total 50)
for i in range(40):
    cmd = f"*3\r\n$5\r\nLPUSH\r\n$9\r\ntest-list\r\n$6\r\nvalue{i}\r\n"
    send_command(cmd)

# Check memory usage with 50 elements
cmd = "*3\r\n$6\r\nMEMORY\r\n$5\r\nUSAGE\r\n$9\r\ntest-list\r\n"
resp = send_command(cmd)
print(f"Memory usage with 50 elements: {int(resp[1:].strip())} bytes")

# Confirm list length
cmd = "*2\r\n$4\r\nLLEN\r\n$9\r\ntest-list\r\n"
resp = send_command(cmd)
print(f"List length: {int(resp[1:].strip())} elements")

# Test memory calculation for large string vs hash
print("\nComparing memory usage of different data types:\n")

# Clean up previous keys
cmd = "*2\r\n$3\r\nDEL\r\n$11\r\ntest-string\r\n"
send_command(cmd)
cmd = "*2\r\n$3\r\nDEL\r\n$9\r\ntest-hash\r\n"
send_command(cmd)

# Create String - 1000 bytes
value = "x" * 1000
cmd = f"*3\r\n$3\r\nSET\r\n$11\r\ntest-string\r\n${len(value)}\r\n{value}\r\n"
send_command(cmd)

# Create Hash with 10 fields of 100 bytes each
for i in range(10):
    field = f"field{i}"
    val = f"value{i}" * 10  # ~100 bytes
    cmd = f"*4\r\n$4\r\nHSET\r\n$9\r\ntest-hash\r\n${len(field)}\r\n{field}\r\n${len(val)}\r\n{val}\r\n"
    send_command(cmd)

# Check memory usage
cmd = "*3\r\n$6\r\nMEMORY\r\n$5\r\nUSAGE\r\n$11\r\ntest-string\r\n"
resp = send_command(cmd)
print(f"String (1000 bytes) memory: {int(resp[1:].strip())} bytes")

cmd = "*3\r\n$6\r\nMEMORY\r\n$5\r\nUSAGE\r\n$9\r\ntest-hash\r\n"
resp = send_command(cmd)
print(f"Hash (10 fields × ~100 bytes) memory: {int(resp[1:].strip())} bytes")

# Clean up
cmd = "*4\r\n$3\r\nDEL\r\n$9\r\ntest-list\r\n$11\r\ntest-string\r\n$9\r\ntest-hash\r\n"
send_command(cmd)

print("\nMemory Test Completed")
