import socket

def test_complex_eval():
    print("\nTesting complex Lua with function calls and cjson...")
    
    # Connect to the server
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(2)
    s.connect(('localhost', 6379))
    
    # 1. Function call test
    cmd1 = b'*3\r\n$4\r\nEVAL\r\n$41\r\nlocal function f() return \'test\' end; return f()\r\n$1\r\n0\r\n'
    print(f"Sending function call: {cmd1!r}")
    s.sendall(cmd1)
    resp1 = s.recv(1024)
    print(f"Response: {resp1!r}\n")
    
    # 2. Table field concatenation
    cmd2 = b'*3\r\n$4\r\nEVAL\r\n$50\r\nlocal t = {foo=\'bar\', baz=42}; return t.foo .. \' \' .. t.baz\r\n$1\r\n0\r\n'
    print(f"Sending table concatenation: {cmd2!r}")
    s.sendall(cmd2)
    resp2 = s.recv(1024)
    print(f"Response: {resp2!r}\n")
    
    # 3. cjson.encode test
    cmd3 = b'*3\r\n$4\r\nEVAL\r\n$52\r\nlocal t = {name=\'test\', value=123}; return cjson.encode(t)\r\n$1\r\n0\r\n'
    print(f"Sending cjson.encode: {cmd3!r}")
    s.sendall(cmd3)
    resp3 = s.recv(1024)
    print(f"Response: {resp3!r}\n")
    
    s.close()

if __name__ == "__main__":
    test_complex_eval()
