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

# Test cjson.encode
print("\n=== Testing cjson.encode ===\n")

script1 = """
local t = {a="hello", b=42, c=true}
return cjson.encode(t)
"""
print(f"Script: {script1}")
resp1 = test_lua(script1)
print(f"Response: {resp1}\n")

script2 = """
local arr = {"apple", "banana", "cherry"}
return cjson.encode(arr)
"""
print(f"Script: {script2}")
resp2 = test_lua(script2)
print(f"Response: {resp2}\n")

# Test cjson.decode
print("\n=== Testing cjson.decode ===\n")

script3 = """
local json = '{"a":"hello", "b":42, "c":true}'
local t = cjson.decode(json)
return t.a .. " " .. t.b
"""
print(f"Script: {script3}")
resp3 = test_lua(script3)
print(f"Response: {resp3}\n")

script4 = """
local json = '["apple", "banana", "cherry"]'
local arr = cjson.decode(json)
return arr[1] .. ", " .. arr[2] .. ", " .. arr[3]
"""
print(f"Script: {script4}")
resp4 = test_lua(script4)
print(f"Response: {resp4}\n")
