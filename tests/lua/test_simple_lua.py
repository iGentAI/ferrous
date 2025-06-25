import socket

def send_command(command, debug=True):
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.connect(('localhost', 6379))
    
    if debug:
        print(f'Sending: {command}')
        
    sock.send(command)
    
    response = b''
    sock.settimeout(2)
    try:
        while True:
            chunk = sock.recv(1024)
            if not chunk:
                if debug:
                    print('Connection closed')
                break
            if debug:
                print(f'Received chunk: {chunk}')
            response += chunk
    except socket.timeout:
        if debug:
            print('Socket timeout')
        
    sock.close()
    return response

# Test 1: Set a value directly
print('Setting a test value directly...')
set_command = b'*3\r\n$3\r\nSET\r\n$7\r\ntestkey\r\n$9\r\ntestvalue\r\n'
response = send_command(set_command)
print(f'Response: {response}\n')

# Test 2: Run the simplest possible Lua script
print('Test 1: Simple string Lua script...')
simple_script = b'*3\r\n$4\r\nEVAL\r\n$16\r\nreturn "success"\r\n$1\r\n0\r\n'
response = send_command(simple_script)
print(f'Response: {response}\n')

# Test 3: Try to print KEYS without GET
print('Test 2: Just print KEYS[1] directly...')
keys_script = b'*4\r\n$4\r\nEVAL\r\n$15\r\nreturn KEYS[1]\r\n$1\r\n1\r\n$7\r\ntestkey\r\n'
response = send_command(keys_script)
print(f'Response: {response}\n')

# Test 4: Super simple redis.call to just return OK
print('Test 3: Minimal redis.call with PING command...')
ping_script = b'*3\r\n$4\r\nEVAL\r\n$43\r\nlocal r = redis.call("PING"); return r;\r\n$1\r\n0\r\n'
response = send_command(ping_script)
print(f'Response: {response}\n')

