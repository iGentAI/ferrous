import socket

def send_lua():
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.connect(('localhost', 6379))
    
    # RESP protocol command for EVAL 'return "hello"' 0
    cmd = b'*3\r\n\r\nEVAL\r\n3\r\nreturn "hello"\r\n\r\n0\r\n'
    print(f'Sending command: {cmd}')
    
    sock.send(cmd)
    
    response = b''
    sock.settimeout(3)
    try:
        while True:
            chunk = sock.recv(4096)
            if not chunk:
                print('Connection closed')
                break
            print(f'Received: {chunk}')
            response += chunk
            if response.endswith(b'\r\n'):
                break
    except socket.timeout:
        print('Socket timeout')
    
    sock.close()
    print(f'Response: {response}')

send_lua()
