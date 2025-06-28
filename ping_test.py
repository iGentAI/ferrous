import socket

s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.connect(('localhost', 6379))
s.sendall(b'*1\r\n\r\nPING\r\n')
resp = s.recv(1024)
print(f'Response: {resp}')
s.close()
