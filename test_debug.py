import socket

def test_debug(script):
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect(('localhost', 6379))
    
    # Enable debug in VM later, for now just test simple case
    script_bytes = script.encode('utf-8')
    cmd = f"*3\r\n$4\r\nEVAL\r\n${len(script_bytes)}\r\n{script}\r\n$1\r\n0\r\n"
    s.sendall(cmd.encode('utf-8'))
    
    resp = s.recv(1024)
    s.close()
    return resp

# Test the failing case with intermediate variables  
script = "local t = {a='hello'}; local x = t.a; return x"
print(f"Test: Store table field in local variable")
print(f"Script: {script}")
resp = test_debug(script)
print(f"Response: {resp}")
print()

# Now test the problem case more directly
script2 = "local t = {a='hello', b='world'}; local x = t.a; return x"
print(f"Test: Store one field with multiple fields in table")
print(f"Script: {script2}")
resp2 = test_debug(script2)
print(f"Response: {resp2}")
