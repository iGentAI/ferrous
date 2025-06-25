import socket

def check_vm_impl():
    print("\nChecking VM implementation in Ferrous...\n")
    
    # Connect to the server
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(2)
    s.connect(('localhost', 6379))
    
    # Try three different EVAL versions to diagnose the issue
    # 1. Simplest possible Lua - just return 1
    cmd1 = b'*3\r\n$4\r\nEVAL\r\n$8\r\nreturn 1\r\n$1\r\n0\r\n'
    print(f"Sending simple return 1: {cmd1!r}")
    s.sendall(cmd1)
    resp1 = s.recv(1024)
    print(f"Response: {resp1!r}\n")
    
    # 2. Return nil - should work on any VM
    cmd2 = b'*3\r\n$4\r\nEVAL\r\n$10\r\nreturn nil\r\n$1\r\n0\r\n'
    print(f"Sending return nil: {cmd2!r}")
    s.sendall(cmd2)
    resp2 = s.recv(1024)
    print(f"Response: {resp2!r}\n")
    
    # 3. Get a table value - should work in complete implementation
    cmd3 = b'*3\r\n$4\r\nEVAL\r\n$28\r\nlocal t={a=1}; return t.a;\r\n$1\r\n0\r\n'
    print(f"Sending table get: {cmd3!r}")
    s.sendall(cmd3)
    resp3 = s.recv(1024)
    print(f"Response: {resp3!r}\n")
    
    s.close()

if __name__ == "__main__":
    check_vm_impl()
