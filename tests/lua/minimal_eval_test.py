import socket

def test_minimal_eval():
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(2)
    s.connect(('localhost', 6379))
    
    # Test simple PING first
    ping_cmd = b'*1\r\n$4\r\nPING\r\n'
    print(f"Sending PING: {ping_cmd!r}")
    s.sendall(ping_cmd)
    ping_resp = s.recv(1024)
    print(f"PING Response: {ping_resp!r}\n")
    
    # Test minimal EVAL with a numeric return
    eval_cmd = b'*3\r\n$4\r\nEVAL\r\n$9\r\nreturn 42\r\n$1\r\n0\r\n'
    print(f"Sending EVAL: {eval_cmd!r}")
    s.sendall(eval_cmd)
    eval_resp = s.recv(1024)
    print(f"EVAL Response: {eval_resp!r}\n")
    
    s.close()

if __name__ == "__main__":
    test_minimal_eval()
