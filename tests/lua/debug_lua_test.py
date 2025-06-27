import socket
import time

def send_command(command, debug=True):
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.connect(('localhost', 6379))
    
    if debug:
        print(f'Sending: {command}')
        
    sock.send(command)
    
    response = b''
    sock.settimeout(2)
    try:
        while True:
            chunk = sock.recv(1024)
            if not chunk:
                if debug:
                    print('Connection closed')
                break
            if debug:
                print(f'Received chunk: {chunk}')
            response += chunk
    except socket.timeout:
        if debug:
            print('Socket timeout')
    except socket.error as e:
        if debug:
            print(f'Socket error: {e}')
    finally:
        sock.close()
        
    return response

def run_test(test_name, test_script, numkeys=0, keys=None, args=None):
    print(f'\n=== Running Test: {test_name} ===')
    
    # Build command
    command_parts = [
        b'*',
        str(3 + (numkeys or 0) + (len(args) if args else 0)).encode(),
        b'\r\n',
        b'$4\r\n',
        b'EVAL\r\n',
        b'$',
        str(len(test_script)).encode(),
        b'\r\n',
        test_script.encode(),
        b'\r\n',
        b'$',
        str(len(str(numkeys))).encode(),
        b'\r\n',
        str(numkeys).encode(),
        b'\r\n'
    ]
    
    # Add keys
    if keys:
        for key in keys:
            command_parts.extend([
                b'$',
                str(len(key)).encode(),
                b'\r\n',
                key.encode(),
                b'\r\n'
            ])
    
    # Add args
    if args:
        for arg in args:
            command_parts.extend([
                b'$',
                str(len(arg)).encode(),
                b'\r\n',
                arg.encode(),
                b'\r\n'
            ])
    
    command = b''.join(command_parts)
    
    # Send the command
    response = send_command(command)
    
    print(f'Response: {response}\n')
    return response

# First set a test key
print('Setting a test value directly...')
set_command = b'*3\r\n$3\r\nSET\r\n$7\r\ntestkey\r\n$9\r\ntestvalue\r\n'
response = send_command(set_command)
print(f'Response: {response}\n')

# Test 1: Simple string return
run_test(
    "Simple string return",
    "return 'success'"
)

# Test 2: Debug KEYS table structure
run_test(
    "Debug KEYS table",
    "return {KEYS=KEYS}"
)

# Test 3: Access KEYS[1] with better error handling
run_test(
    "KEYS[1] access with pcall",
    "local status, result = pcall(function() return KEYS[1] end); return {status=status, result=result}",
    numkeys=1,
    keys=["testkey"]
)

# Test 4: Debug redis table structure
run_test(
    "Debug redis table",
    "local result = {}; for k, v in pairs(redis) do result[k] = type(v) end; return result"
)

# Test 5: Attempt redis.call with pcall for error handling
run_test(
    "redis.call with pcall",
    "local status, result = pcall(function() return redis.call('PING') end); return {status=status, result=result}"
)

# Test 6: Table field concatenation test
run_test(
    "Table field concatenation",
    "local t = {str='hello'}; local result = pcall(function() return t.str .. ' world' end); return {status=result}"
)

# Test 7: Complex table concatenation test
run_test(
    "Complex table concatenation",
    "local t = {first='hello', second='world'}; local status, result = pcall(function() return t.first .. ' ' .. t.second end); return {status=status, result=result}"
)

print("All tests completed")