import socket

def test_concat(script):
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect(('localhost', 6379))
    
    script_bytes = script.encode('utf-8')
    cmd = f"*3\r\n$4\r\nEVAL\r\n${len(script_bytes)}\r\n{script}\r\n$1\r\n0\r\n"
    s.sendall(cmd.encode('utf-8'))
    
    resp = s.recv(1024)
    s.close()
    return resp

# Test specific failing case
script = "local t = {a='hello'}; return t.a"
print(f"Test: Get table field only")
print(f"Script: {script}")
resp = test_concat(script)
print(f"Response: {resp}")
print()

# Now test concat
script2 = "local t = {a='hello'}; return t.a .. ' world'"
print(f"Test: Simple concat after table field")
print(f"Script: {script2}")
resp2 = test_concat(script2)
print(f"Response: {resp2}")
print()

# Test double field access
script3 = "local t = {a='hello', b='world'}; local x = t.a; local y = t.b; return x .. ' ' .. y"
print(f"Test: Use intermediate variables")
print(f"Script: {script3}")
resp3 = test_concat(script3)
print(f"Response: {resp3}")
