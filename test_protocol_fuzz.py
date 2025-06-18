#!/usr/bin/env python3
"""
Protocol fuzzing test for Ferrous
Tests robustness against random and malformed input
"""

import socket
import random
import string
import time

def generate_random_bytes(min_len=1, max_len=100):
    """Generate random bytes"""
    length = random.randint(min_len, max_len)
    return bytes(random.randint(0, 255) for _ in range(length))

def generate_random_resp():
    """Generate random RESP-like data"""
    types = [b'+', b'-', b':', b'$', b'*']
    resp_type = random.choice(types)
    
    if resp_type == b'+':
        # Simple string
        text = ''.join(random.choices(string.printable, k=random.randint(1, 50)))
        return resp_type + text.encode() + b'\r\n'
    elif resp_type == b'-':
        # Error
        text = ''.join(random.choices(string.printable, k=random.randint(1, 50)))
        return resp_type + text.encode() + b'\r\n'
    elif resp_type == b':':
        # Integer
        num = random.randint(-1000000, 1000000)
        return resp_type + str(num).encode() + b'\r\n'
    elif resp_type == b'$':
        # Bulk string
        if random.random() < 0.1:
            # Null bulk string
            return b'$-1\r\n'
        else:
            content = generate_random_bytes(0, 100)
            return b'$' + str(len(content)).encode() + b'\r\n' + content + b'\r\n'
    else:
        # Array
        if random.random() < 0.1:
            # Null array
            return b'*-1\r\n'
        else:
            count = random.randint(0, 5)
            result = b'*' + str(count).encode() + b'\r\n'
            for _ in range(count):
                result += generate_random_resp()
            return result

def fuzz_test(rounds=1000):
    """Run fuzzing test"""
    print(f"Running {rounds} fuzzing tests...")
    
    crashes = 0
    errors = 0
    successes = 0
    
    for i in range(rounds):
        if i % 100 == 0:
            print(f"Progress: {i}/{rounds}")
        
        try:
            s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            s.settimeout(0.5)
            s.connect(('127.0.0.1', 6379))
            
            # Choose fuzzing strategy
            strategy = random.choice(['random_bytes', 'malformed_resp', 'valid_resp'])
            
            if strategy == 'random_bytes':
                # Pure random bytes
                data = generate_random_bytes(1, 200)
            elif strategy == 'malformed_resp':
                # RESP-like but malformed
                data = generate_random_resp()
                # Corrupt it
                if len(data) > 5:
                    corruption = random.choice([
                        lambda d: d[:-2],  # Remove CRLF
                        lambda d: d[:-1],  # Remove just LF
                        lambda d: d[:len(d)//2],  # Cut in half
                        lambda d: d.replace(b'\r\n', b'\n'),  # Wrong line ending
                        lambda d: b'\x00' + d,  # Null byte prefix
                    ])
                    data = corruption(data)
            else:
                # Valid RESP (should work)
                data = generate_random_resp()
            
            s.sendall(data)
            
            # Try to read response
            try:
                response = s.recv(1024)
                if response:
                    successes += 1
                else:
                    errors += 1
            except socket.timeout:
                errors += 1
            
            s.close()
            
        except socket.error:
            # Connection refused or reset - server might have crashed
            try:
                # Check if server is still running
                time.sleep(0.1)
                test_sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                test_sock.settimeout(1)
                test_sock.connect(('127.0.0.1', 6379))
                test_sock.close()
                errors += 1  # Server is running, just rejected our connection
            except:
                crashes += 1
                print(f"\n❌ Server appears to have crashed at iteration {i}!")
                return False
        except Exception as e:
            errors += 1
    
    print(f"\nFuzzing complete!")
    print(f"  Successes: {successes}")
    print(f"  Errors/Rejections: {errors}")
    print(f"  Server crashes: {crashes}")
    
    if crashes > 0:
        print("\n❌ FAILED - Server crashed during fuzzing")
        return False
    else:
        print("\n✅ PASSED - Server remained stable")
        return True

def main():
    print("=" * 60)
    print("FERROUS PROTOCOL FUZZING TEST")
    print("=" * 60)
    
    # Check server is running
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(1)
        s.connect(('127.0.0.1', 6379))
        s.close()
        print("✅ Server is running on port 6379")
    except:
        print("❌ Server is not running on port 6379")
        return
    
    # Run fuzzing
    result = fuzz_test(1000)
    
    if result:
        print("\n🎉 Fuzzing test PASSED!")
    else:
        print("\n❌ Fuzzing test FAILED!")

if __name__ == "__main__":
    main()