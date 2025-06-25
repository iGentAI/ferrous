import socket
import binascii

def send_lua():
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.connect(('localhost', 6379))
    
    # Properly formatted RESP command for EVAL 'return "hello"' 0
    # with correct $ before lengths
    cmd = b'*3\r\n\r\nEVAL\r\n3\r\nreturn "hello"\r\n\r\n0\r\n'
    
    print(f'Sending raw command: {cmd!r}')
    print(f'Sending hex encoded: {binascii.hexlify(cmd).decode()}')
    
    sock.send(cmd)
    
    response = b''
    sock.settimeout(5)
    try:
        while True:
            chunk = sock.recv(4096)
            if not chunk:
                print('Connection closed')
                break
            print(f'Received chunk: {chunk!r}')
            print(f'Received hex: {binascii.hexlify(chunk).decode()}')
            response += chunk
            if response.endswith(b'\r\n'):
                break
    except socket.timeout:
        print('Socket timeout')
    
    sock.close()
    print(f'Final response hex: {binascii.hexlify(response).decode()}')
    print(f'Final response repr: {response!r}')

send_lua()
