import socket, time

def test_concat():
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect(('localhost', 6379))
    
    test_cases = [
        # Simple table field
        b'*3\r\n\r\nEVAL\r\n6\r\nlocal t = {foo=\'bar\'}; return t.foo\r\n\r\n0\r\n',
        
        # Simple concatenation
        b'*3\r\n\r\nEVAL\r\n1\r\nlocal t = {foo=\'bar\'}; return t.foo .. \' test\'\r\n\r\n0\r\n',
        
        # String + table field
        b'*3\r\n\r\nEVAL\r\n9\r\nlocal str=\'test \'; local t = {foo=\'bar\'}; return str .. t.foo\r\n\r\n0\r\n',
        
        # Table field + number
        b'*3\r\n\r\nEVAL\r\n2\r\nlocal t = {num=42}; return \'Value: \' .. t.num\r\n\r\n0\r\n',
        
        # Double table fields
        b'*3\r\n\r\nEVAL\r\n9\r\nlocal t = {foo=\'bar\', baz=42}; return t.foo .. \' \' .. t.baz\r\n\r\n0\r\n'
    ]
    
    for i, cmd in enumerate(test_cases):
        print(f"Test {i+1}:")
        s.sendall(cmd)
        
        # Collect response
        resp = b''
        s.settimeout(2)
        try:
            while True:
                chunk = s.recv(1024)
                if not chunk:
                    break
                resp += chunk
                if resp.endswith(b'\r\n'):
                    break
        except socket.timeout:
            print('Socket timeout')
        
    
    s.close()

test_concat()
