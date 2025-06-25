import socket
import binascii

def send_lua():
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.connect(('localhost', 6379))
    
    # Properly formatted RESP command for EVAL 'return "hello"' 0
    # Format: *3\r\n\r\nEVAL\r\n3\r\nreturn "hello"\r\n\r\n0\r\n
    cmd = b'*3\r\n\r\nEVAL\r\n3\r\nreturn "hello"\r\n\r\n0\r\n'
    
    print(f'Sending hex: {binascii.hexlify(cmd)}')
    print(f'Sending command: {cmd}')
    
    sock.send(cmd)
    
    response = b''
    sock.settimeout(5)
    try:
        while True:
            chunk = sock.recv(4096)
            if not chunk:
                print('Connection closed')
                break
            print(f'Received hex: {binascii.hexlify(chunk)}')
            print(f'Received: {chunk}')
            response += chunk
            if response.endswith(b'\r\n'):
                break
    except socket.timeout:
        print('Socket timeout')
    
    sock.close()
    print(f'Final response hex: {binascii.hexlify(response)}')
    print(f'Final response: {response}')

send_lua()
