import socket

def test_lua():
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.connect(('localhost', 6379))
    
    # First test PING
    ping_cmd = b'*1\r\n$4\r\nPING\r\n'
    print(f"Sending PING: {ping_cmd}")
    sock.send(ping_cmd)
    ping_resp = sock.recv(1024)
    print(f"PING Response: {ping_resp}\n")
    
    # Then test EVAL
    eval_cmd = b'*3\r\n$4\r\nEVAL\r\n$14\r\nreturn 123.45\r\n$1\r\n0\r\n'
    print(f"Sending EVAL: {eval_cmd}")
    sock.send(eval_cmd)
    eval_resp = sock.recv(1024)
    print(f"EVAL Response: {eval_resp}")
    
    sock.close()

test_lua()
