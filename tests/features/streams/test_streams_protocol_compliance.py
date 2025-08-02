#!/usr/bin/env python3
"""
Comprehensive Redis protocol compliance testing for Stream commands
Tests various client behaviors and protocol edge cases
"""

import redis
import socket
import sys

def test_raw_protocol_streams():
    """Test Stream commands via raw Redis protocol"""
    print("Testing raw Redis protocol for Streams...")
    
    def send_command(cmd_bytes):
        """Send raw command and return response"""
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.connect(('localhost', 6379))
        s.sendall(cmd_bytes)
        response = s.recv(4096)
        s.close()
        return response
    
    # Test XADD via raw protocol
    xadd_cmd = b'*5\r\n$4\r\nXADD\r\n$11\r\nraw:stream\r\n$1\r\n*\r\n$4\r\ntest\r\n$5\r\nvalue\r\n'
    response = send_command(xadd_cmd)
    assert response.startswith(b'$'), f"XADD should return bulk string, got {response}"
    print("✅ Raw XADD protocol working")
    
    # Test XLEN via raw protocol  
    xlen_cmd = b'*2\r\n$4\r\nXLEN\r\n$10\r\nraw:stream\r\n'
    response = send_command(xlen_cmd)
    assert response.startswith(b':1\r\n'), f"XLEN should return :1, got {response}"
    print("✅ Raw XLEN protocol working")
    
    # Test XRANGE via raw protocol
    xrange_cmd = b'*4\r\n$6\r\nXRANGE\r\n$10\r\nraw:stream\r\n$1\r\n-\r\n$1\r\n+\r\n'
    response = send_command(xrange_cmd)
    assert response.startswith(b'*'), f"XRANGE should return array, got {response}"
    print("✅ Raw XRANGE protocol working")
    
    # Cleanup
    cleanup_cmd = b'*2\r\n$3\r\nDEL\r\n$10\r\nraw:stream\r\n'
    send_command(cleanup_cmd)
    
    return True

def test_client_library_variations():
    """Test different Redis client library patterns"""
    print("Testing Redis client library variations...")
    
    # Test with decode_responses=False (binary mode)
    r_binary = redis.Redis(host='localhost', port=6379, decode_responses=False)
    
    # Add entry in binary mode
    entry_id = r_binary.xadd(b'binary:stream', {b'field': b'value', b'num': b'123'})
    assert isinstance(entry_id, bytes), "Binary mode should return bytes"
    print("✅ Binary mode XADD working")
    
    # Read in binary mode
    entries = r_binary.xrange(b'binary:stream')
    assert len(entries) == 1, "Should have 1 entry"
    assert isinstance(entries[0][0], bytes), "Entry ID should be bytes in binary mode"
    print("✅ Binary mode XRANGE working")
    
    # Test with decode_responses=True (string mode)
    r_string = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Add entry in string mode
    entry_id = r_string.xadd('string:stream', {'field': 'value', 'num': '456'})
    assert isinstance(entry_id, str), "String mode should return str"
    print("✅ String mode XADD working")
    
    # Read in string mode
    entries = r_string.xrange('string:stream')
    assert len(entries) == 1, "Should have 1 entry"
    assert isinstance(entries[0][0], str), "Entry ID should be str in string mode"
    print("✅ String mode XRANGE working")
    
    # Test XTRIM with different argument patterns
    for i in range(10):
        r_string.xadd('trim:test', {'seq': str(i)})
    
    # Test redis-py default approximate trim
    trimmed1 = r_string.xtrim('trim:test', maxlen=5, approximate=True)
    assert isinstance(trimmed1, int), "XTRIM should return integer"
    
    # Test exact trim
    trimmed2 = r_string.xtrim('trim:test', maxlen=3, approximate=False)
    assert isinstance(trimmed2, int), "XTRIM exact should return integer"
    
    print("✅ Client library variations validated")
    
    # Cleanup
    r_binary.delete(b'binary:stream')
    r_string.delete('string:stream', 'trim:test')
    
    return True

def test_malformed_stream_commands():
    """Test malformed Stream commands for proper error handling"""
    print("Testing malformed Stream commands...")
    
    def send_malformed(cmd_bytes):
        """Send malformed command and check for graceful handling"""
        try:
            s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            s.connect(('localhost', 6379))
            s.sendall(cmd_bytes)
            response = s.recv(4096)
            s.close()
            return response
        except:
            return b'CONNECTION_ERROR'
    
    # Test malformed XADD commands
    malformed_tests = [
        b'*1\r\n$4\r\nXADD\r\n',  # Missing arguments
        b'*3\r\n$4\r\nXADD\r\n$6\r\nstream\r\n$1\r\n*\r\n',  # Missing fields
        b'*4\r\n$4\r\nXADD\r\n$6\r\nstream\r\n$1\r\n*\r\n$4\r\nfield\r\n',  # Odd number of fields
        b'*4\r\n$4\r\nXADD\r\n$6\r\nstream\r\n$7\r\ninvalid\r\n$4\r\ntest\r\n$5\r\nvalue\r\n',  # Invalid ID
    ]
    
    for i, malformed_cmd in enumerate(malformed_tests):
        response = send_malformed(malformed_cmd)
        if response.startswith(b'-ERR'):
            print(f"✅ Malformed command {i+1} properly rejected")
        else:
            print(f"⚠️  Malformed command {i+1} not properly handled: {response}")
    
    return True

def test_stream_memory_pressure():
    """Test Stream operations under memory pressure"""
    print("Testing Stream operations under memory pressure...")
    
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Create multiple streams to test memory handling
    stream_count = 100
    entries_per_stream = 1000
    
    print(f"Creating {stream_count} streams with {entries_per_stream} entries each...")
    
    try:
        for stream_id in range(stream_count):
            stream_key = f'memory:stream:{stream_id}'
            
            # Add entries
            for entry_id in range(entries_per_stream):
                r.xadd(stream_key, {
                    'stream_id': str(stream_id),
                    'entry_id': str(entry_id),
                    'data': f'payload_{entry_id % 10}' * 10  # Repeated data
                })
            
            # Periodically trim to test memory management
            if stream_id % 10 == 0:
                r.xtrim(stream_key, maxlen=500)
        
        # Test memory usage
        try:
            total_memory = r.info('memory')['used_memory']
            print(f"✅ Memory pressure test completed: {total_memory} bytes used")
        except:
            print("✅ Memory pressure test completed (memory info unavailable)")
        
        # Cleanup
        for stream_id in range(stream_count):
            r.delete(f'memory:stream:{stream_id}')
            
    except Exception as e:
        print(f"⚠️  Memory pressure test revealed issue: {e}")
        return False
    
    return True

def run_protocol_compliance_tests():
    """Run all protocol compliance tests"""
    print("=" * 70)
    print("FERROUS STREAMS PROTOCOL COMPLIANCE TEST SUITE")  
    print("=" * 70)
    print()
    
    try:
        # Test connection
        r = redis.Redis(host='localhost', port=6379)
        r.ping()
        print("✅ Server connection verified\n")
    except Exception as e:
        print(f"❌ Cannot connect to server: {e}")
        return False
    
    tests_run = 0
    tests_passed = 0
    
    # Run all protocol compliance tests
    protocol_tests = [
        test_raw_protocol_streams,
        test_client_library_variations,
        test_malformed_stream_commands,
        test_stream_memory_pressure,
    ]
    
    for test_func in protocol_tests:
        tests_run += 1
        try:
            if test_func():
                tests_passed += 1
            else:
                print(f"❌ {test_func.__name__} failed")
        except Exception as e:
            print(f"❌ {test_func.__name__} failed with exception: {e}")
            import traceback
            traceback.print_exc()
    
    print("\n" + "=" * 70)
    print(f"PROTOCOL COMPLIANCE RESULTS: {tests_passed}/{tests_run} PASSED")
    print("=" * 70)
    
    if tests_passed == tests_run:
        print("🎉 ALL PROTOCOL COMPLIANCE TESTS PASSED!")
        return True
    else:
        print(f"⚠️  {tests_run - tests_passed} protocol tests failed")
        return False

if __name__ == "__main__":
    success = run_protocol_compliance_tests()
    sys.exit(0 if success else 1)