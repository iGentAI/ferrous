import socket

def send_lua_test():
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.connect(('localhost', 6379))
    
    script = 'return "hello"'
    
    command = '*3\r\n$4\r\nEVAL\r\n$13\r\nreturn "hello"\r\n$1\r\n0\r\n'
    print(f'Sending valid EVAL: {command}')
    
    sock.send(command.encode())
    
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
    except socket.timeout:
        print('Socket timeout')
    
    sock.close()
    print(f'Final response: {response}')

print('\nTesting Valid Lua Script\n')
send_lua_test()
