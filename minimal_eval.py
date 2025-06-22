import socket

def send_minimal_eval():
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.connect(('localhost', 6379))
    
    # A truly minimal Lua script that just returns a string literal
    # Format carefully to ensure proper RESP protocol
    command = b'*3\r\n$4\r\nEVAL\r\n$6\r\nreturn\r\n$1\r\n0\r\n'
    print(f'Sending minimal EVAL: {command}')
    
    sock.send(command)
    
    response = b''
    sock.settimeout(3)
    try:
        while True:
            chunk = sock.recv(4096)
            if not chunk:
                print('Connection closed')
                break
            print(f'Received chunk: {chunk}')
            response += chunk
            if response.endswith(b'\r\n'):
                break
    except socket.timeout:
        print('Socket timeout')
    
    sock.close()
    print(f'Final response: {response}')

print('\nTesting Minimal EVAL\n')
send_minimal_eval()
