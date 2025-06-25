import socket
import time

def test_table_concats():
    tests = [
        ("Simple concat", "local t = {foo='bar'}; return t.foo .. ' test'"),
        ("Reverse concat", "local str='test '; local t = {foo='bar'}; return str .. t.foo"),
        ("Number concat", "local t = {num=42}; return 'Value: ' .. t.num"),
        ("Double table", "local t = {a='hello', b='world'}; return t.a .. ' ' .. t.b"),
        ("Problem case", "local t = {foo='bar', baz=42}; return t.foo .. ' ' .. t.baz")
    ]
    
    print("===== Testing Table Concatenation Scenarios =====\n")
    
    for name, script in tests:
        # Format EVAL command
        command = encode_eval(script)
        
        # Send to server
        resp = send_command(command)
        
        print(f"Test: {name}")
        print(f"Script: {script}")
        print(f"Response: {resp}\n")

def encode_eval(script):
    """Encode a Lua script as a proper RESP EVAL command"""
    script_bytes = script.encode('utf-8')
    command = f"*3\r\n$4\r\nEVAL\r\n${len(script_bytes)}\r\n{script}\r\n$1\r\n0\r\n"
    return command.encode('utf-8')

def send_command(command):
    """Send a command to the Redis server"""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(2)
    s.connect(('localhost', 6379))
    
    s.sendall(command)
    
    resp = b''
    start = time.time()
    while time.time() - start < 1.0:
        try:
            chunk = s.recv(1024)
            if not chunk:
                break
            resp += chunk
            
            if resp.endswith(b'\r\n'):
                break
        except socket.timeout:
            break
    
    s.close()
    return resp

if __name__ == "__main__":
    test_table_concats()
