import socket
import time

def send_eval(script):
    """Send an EVAL command with better error handling"""
    # Format command
    script_bytes = script.encode('utf-8')
    cmd = f"*3\r\n$4\r\nEVAL\r\n${len(script_bytes)}\r\n{script}\r\n$1\r\n0\r\n".encode('utf-8')
    
    # Send command
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(2)
        s.connect(('localhost', 6379))
        print(f"Sending EVAL: {script}")
        s.sendall(cmd)
        
        # Receive response with timeout
        resp = b''
        start = time.time()
        while time.time() - start < 1.0:  # 1 second timeout
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
    except Exception as e:
        print(f"Error: {e}")
        return b''

def run_tests():
    print("\n===== Testing Ferrous Lua Implementation =====\n")
    
    # Test 1: Basic return
    print("Test 1: Basic return value")
    resp = send_eval("return 42")
    print(f"Response: {resp}\n")
    
    # Test 2: Function call
    print("Test 2: Function call")
    resp = send_eval("local function f() return 'test' end; return f()")
    print(f"Response: {resp}\n")
    
    # Test 3: Table access
    print("Test 3: Table field access")
    resp = send_eval("local t = {a=1}; return t.a")
    print(f"Response: {resp}\n")
    
    # Test 4: Concatenation
    print("Test 4: String concatenation with table fields")
    resp = send_eval("local t = {foo='bar', baz=42}; return t.foo .. ' ' .. t.baz")
    print(f"Response: {resp}\n")
    
    # Test 5: cjson library
    print("Test 5: cjson.encode")
    resp = send_eval("local t = {name='test', value=123}; return cjson.encode(t)")
    print(f"Response: {resp}\n")

if __name__ == "__main__":
    run_tests()
