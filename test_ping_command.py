import socket

def send_command(command, debug=True):
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.connect(('localhost', 6379))
    
    if debug:
        print(f'Sending: {command}')
        
    sock.send(command)
    
    response = b''
    sock.settimeout(5)  # Longer timeout for debugging
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
            # Check if we got a complete response
            if response.endswith(b'\r\n'):
                break
    except socket.timeout:
        if debug:
            print('Socket timeout')
        
    sock.close()
    return response

print('Testing direct PING command...')
ping_command = b'*1\r\n$4\r\nPING\r\n'
response = send_command(ping_command)
print(f'Direct PING Response: {response}\n')

print('Testing Lua redis.call with PING command...')
ping_script = b'*3\r\n$4\r\nEVAL\r\n$31\r\nreturn redis.call("PING")\r\n$1\r\n0\r\n'
response = send_command(ping_script)
print(f'EVAL PING Response: {response}\n')
