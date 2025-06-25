import socket
import binascii

def send_lua():
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.connect(('localhost', 6379))
    
    # Use the exact bytes we need for the RESP protocol
    cmd = b'*3\r\n\r\nEVAL\r\n3\r\nreturn "hello"\r\n\r\n0\r\n'
    
    # Print hex to debug exact bytes being sent
    print(f'Sending hex: {binascii.hexlify(cmd)}')
    print(f'Sending command: {repr(cmd.decode("latin1"))}')
    
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
            print(f'Received: {repr(chunk.decode("latin1"))}')
            response += chunk
    except socket.timeout:
        print('Socket timeout')
    
    sock.close()
    print(f'Final response hex: {binascii.hexlify(response)}')
    print(f'Final response: {repr(response.decode("latin1", errors="replace"))}')

send_lua()
