import socket

def test_lua(script):
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect(('localhost', 6379))
    
    script_bytes = script.encode('utf-8')
    cmd = f"*3\r\n$4\r\nEVAL\r\n${len(script_bytes)}\r\n{script}\r\n$1\r\n0\r\n"
    s.sendall(cmd.encode('utf-8'))
    
    resp = s.recv(1024)
    s.close()
    return resp

# Test the global 'type' directly
script1 = "local f = type; local t = {}; return f(t)"
print(f"Testing direct reference to type function: {script1}")
resp1 = test_lua(script1)
print(f"Response: {resp1}\n")

# Test with cjson.decode
script2 = """local json = '{"a":"hello"}'\nlocal t = cjson.decode(json)\nlocal ty = type(t)\nreturn ty"""
print(f"Testing type of decoded JSON: {script2}")
resp2 = test_lua(script2)
print(f"Response: {resp2}\n")
