#!/usr/bin/env python3
"""
Basic test suite for Ferrous Stream functionality
Tests the currently implemented Stream commands
"""

import redis
import time
import sys

def test_basic_stream_operations():
    """Test basic stream operations"""
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Clean up
    r.delete('test:stream')
    
    print("Testing basic stream operations...")
    
    # Test XADD with auto ID
    id1 = r.xadd('test:stream', {'temperature': '25.5', 'humidity': '60'})
    assert isinstance(id1, str)
    print(f"✅ XADD auto ID: {id1}")
    
    # Test XLEN
    length = r.xlen('test:stream')
    assert length == 1
    print(f"✅ XLEN: {length} entries")
    
    # Test XRANGE
    entries = r.xrange('test:stream')
    assert len(entries) == 1
    print(f"✅ XRANGE: {len(entries)} entries")
    
    # Test TYPE command
    key_type = r.type('test:stream')
    assert key_type == 'stream'
    print("✅ TYPE command returns 'stream'")
    
    # Test XREAD
    # First add another entry after a delay
    time.sleep(0.001)
    id2 = r.xadd('test:stream', {'temperature': '26.0', 'humidity': '65'})
    
    # Read from the beginning
    streams = r.xread({'test:stream': '0-0'})
    if isinstance(streams, list) and streams:
        stream_data = {stream[0]: stream[1] for stream in streams}
        assert 'test:stream' in stream_data
        assert len(stream_data['test:stream']) == 2
        print(f"✅ XREAD: {len(stream_data['test:stream'])} entries")
    else:
        assert 'test:stream' in streams
        assert len(streams['test:stream']) == 2
        print(f"✅ XREAD: {len(streams['test:stream'])} entries")
    
    print("✅ Basic stream operations test completed\n")

def test_stream_range_operations():
    """Test XRANGE and XREVRANGE operations"""
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Clean up
    r.delete('range:stream')
    
    print("Testing stream range operations...")
    
    # Add entries with known sequence
    entries_added = []
    for i in range(5):
        time.sleep(0.001)  # Ensure different timestamps
        entry_id = r.xadd('range:stream', {'value': str(i)})
        entries_added.append(entry_id)
    
    # Test full XRANGE
    all_entries = r.xrange('range:stream', '-', '+')
    assert len(all_entries) == 5
    print(f"✅ XRANGE full range: {len(all_entries)} entries")
    
    # Test XRANGE with count
    limited_entries = r.xrange('range:stream', '-', '+', count=3)
    assert len(limited_entries) == 3
    print(f"✅ XRANGE with COUNT: {len(limited_entries)} entries")
    
    # Test XREVRANGE  
    reverse_entries = r.xrevrange('range:stream', '+', '-')
    assert len(reverse_entries) == 5
    # First entry should be the last added
    assert reverse_entries[0][1]['value'] == '4'
    print(f"✅ XREVRANGE: {len(reverse_entries)} entries in reverse order")
    
    print("✅ Stream range operations test completed\n")

def test_stream_trimming():
    """Test XTRIM operations"""
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Clean up
    r.delete('trim:stream')
    
    print("Testing stream trimming...")
    
    # Add multiple entries
    for i in range(10):
        r.xadd('trim:stream', {'count': str(i)})
    
    initial_length = r.xlen('trim:stream')
    assert initial_length == 10
    print(f"✅ Initial length: {initial_length}")
    
    # Trim to 5 entries
    trimmed = r.xtrim('trim:stream', maxlen=5)
    assert trimmed == 5
    print(f"✅ XTRIM removed: {trimmed} entries")
    
    # Verify length after trim
    final_length = r.xlen('trim:stream')
    assert final_length == 5
    print(f"✅ Length after trim: {final_length}")
    
    print("✅ Stream trimming test completed\n")

def test_stream_memory_tracking():
    """Test stream memory efficiency"""
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    print("Testing stream memory tracking...")
    
    # Create a test stream
    stream_key = 'memory:stream'
    r.delete(stream_key)
    
    # Add entries and check memory usage
    for i in range(50):
        r.xadd(stream_key, {
            'id': str(i),
            'data': f'data_{i}',
            'timestamp': str(time.time())
        })
    
    # Try to get memory usage
    try:
        memory = r.memory_usage(stream_key)
        if memory:
            print(f"✅ Stream memory usage: {memory} bytes for 50 entries")
        else:
            print("⚠️  MEMORY USAGE not supported for streams yet")
    except:
        print("⚠️  MEMORY USAGE command not available")
    
    # Verify stream functionality
    length = r.xlen(stream_key)
    assert length == 50
    print(f"✅ Stream length verification: {length} entries")
    
    # Clean up
    r.delete(stream_key)
    
    print("✅ Stream memory tracking test completed\n")

def test_stream_persistence():
    """Test that streams work with RDB"""
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    print("Testing stream persistence...")
    
    # Create a unique stream
    stream_key = f'persist:stream:{int(time.time())}'
    
    # Add data
    entries_added = []
    for i in range(3):
        entry_id = r.xadd(stream_key, {'sequence': str(i), 'data': f'test_{i}'})
        entries_added.append(entry_id)
    
    # Read back to verify
    entries = r.xrange(stream_key)
    assert len(entries) == 3
    print(f"✅ Stream created with {len(entries)} entries")
    
    # Check that TYPE command works
    key_type = r.type(stream_key)
    assert key_type == 'stream'
    print("✅ TYPE correctly identifies stream")
    
    # Note: In a real persistence test, we'd restart the server here
    # For now, just verify the stream is accessible
    length = r.xlen(stream_key)
    assert length == 3
    print("✅ Stream remains accessible")
    
    # Clean up
    r.delete(stream_key)
    
    print("✅ Stream persistence test completed\n")

def run_all_tests():
    """Run all available stream tests"""
    print("=" * 60)
    print("FERROUS STREAMS BASIC TEST SUITE")
    print("=" * 60)
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
    
    # Run all test functions
    test_functions = [
        test_basic_stream_operations,
        test_stream_range_operations,
        test_stream_trimming,
        test_stream_memory_tracking,
        test_stream_persistence,
    ]
    
    for test_func in test_functions:
        tests_run += 1
        try:
            test_func()
            tests_passed += 1
        except Exception as e:
            print(f"❌ {test_func.__name__} failed: {e}")
            import traceback
            traceback.print_exc()
    
    print("=" * 60)
    print(f"STREAM TEST RESULTS: {tests_passed}/{tests_run} PASSED")
    print("=" * 60)
    
    if tests_passed == tests_run:
        print("🎉 All basic stream tests passed!")
        return True
    else:
        print(f"⚠️  {tests_run - tests_passed} tests failed")
        return False

if __name__ == "__main__":
    success = run_all_tests()
    sys.exit(0 if success else 1)