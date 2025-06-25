import socket
import binascii
import time

def send_raw_command(command_bytes):
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.connect(('localhost', 6379))
    
    print(f'Sending hex: {binascii.hexlify(command_bytes).decode()}')
    print(f'Sending decoded: {repr(command_bytes.decode("latin1"))}')
    
    sock.send(command_bytes)
    
    response = b''
    sock.settimeout(5)
    try:
        while True:
            chunk = sock.recv(4096)
            if not chunk:
                print('Connection closed')
                break
            print(f'Received chunk: {repr(chunk.decode("latin1"))}')
            print(f'Received hex: {binascii.hexlify(chunk).decode()}')
            response += chunk
            if response.endswith(b'\r\n'):
                break
    except socket.timeout:
        print('Socket timeout')
    
    sock.close()
    print(f'Final response: {repr(response.decode("latin1", errors="replace"))}')
    print(f'Final hex: {binascii.hexlify(response).decode()}')

# Try a known working command first - PING
ping_command = b'*1\r\n$4\r\nPING\r\n'
print("\n=== Testing PING ===\n")
send_raw_command(ping_command)

time.sleep(1)

# Now test a Lua script
lua_script = b'*3\r\n$4\r\nEVAL\r\n$13\r\nreturn "hello"\r\n$1\r\n0\r\n'
print("\n=== Testing Lua EVAL ===\n")
send_raw_command(lua_script)
