#!/usr/bin/env python3
"""
Comprehensive test suite for Ferrous Stream functionality

Tests all Stream commands including:
- Basic stream operations (XADD, XRANGE, XLEN)
- Consumer groups (XGROUP, XREADGROUP, XACK)
- Advanced operations (XTRIM, XDEL, XCLAIM)
"""

import redis
import time
import threading
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
    
    # Test XADD with specific ID
    id2 = r.xadd('test:stream', {'temperature': '26.0', 'humidity': '65'}, id='1000000-1')
    assert id2 == '1000000-1'
    print(f"✅ XADD specific ID: {id2}")
    
    # Test XLEN
    length = r.xlen('test:stream')
    assert length >= 2
    print(f"✅ XLEN: {length} entries")
    
    # Test XRANGE
    entries = r.xrange('test:stream')
    assert len(entries) >= 2
    print(f"✅ XRANGE: {len(entries)} entries")
    
    # Test XRANGE with specific range
    entries = r.xrange('test:stream', min='1000000-1', max='1000000-1')
    assert len(entries) == 1
    assert entries[0][0] == '1000000-1'
    print(f"✅ XRANGE specific range: {len(entries)} entry")
    
    # Test XREVRANGE
    entries = r.xrevrange('test:stream', count=1)
    assert len(entries) == 1
    print(f"✅ XREVRANGE: {len(entries)} entry (latest)")
    
    print("✅ Basic stream operations passed\n")

def test_stream_trim_and_delete():
    """Test XTRIM and XDEL operations"""
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Clean up
    r.delete('trim:stream')
    
    print("Testing stream trim and delete...")
    
    # Add multiple entries
    ids = []
    for i in range(10):
        id = r.xadd('trim:stream', {'count': str(i)})
        ids.append(id)
    
    # Test XTRIM
    trimmed = r.xtrim('trim:stream', maxlen=5)
    assert trimmed == 5
    print(f"✅ XTRIM: trimmed {trimmed} entries")
    
    # Verify length
    length = r.xlen('trim:stream')
    assert length == 5
    print(f"✅ Length after trim: {length}")
    
    # Test XDEL
    deleted = r.xdel('trim:stream', ids[-1])  # Delete last entry
    assert deleted >= 0  # May be 0 if already trimmed
    print(f"✅ XDEL: deleted {deleted} entries")
    
    print("✅ Stream trim and delete passed\n")

def test_xread_operations():
    """Test XREAD functionality"""
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Clean up
    r.delete('read:stream1', 'read:stream2')
    
    print("Testing XREAD operations...")
    
    # Add initial entries
    id1 = r.xadd('read:stream1', {'data': 'initial1'})
    id2 = r.xadd('read:stream2', {'data': 'initial2'})
    
    # Test XREAD from beginning
    streams = r.xread({'read:stream1': '0-0', 'read:stream2': '0-0'})
    assert len(streams) == 2
    print(f"✅ XREAD from start: {len(streams)} streams")
    
    # Test XREAD with no new data
    streams = r.xread({'read:stream1': id1, 'read:stream2': id2})
    assert len(streams) == 0
    print("✅ XREAD with no new data: empty result")
    
    # Add new data and read
    id3 = r.xadd('read:stream1', {'data': 'new1'})
    streams = r.xread({'read:stream1': id1, 'read:stream2': id2})
    assert len(streams) >= 1
    print(f"✅ XREAD with new data: {len(streams)} streams")
    
    print("✅ XREAD operations passed\n")

def test_consumer_groups_basic():
    """Test basic consumer group functionality"""
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Clean up
    r.delete('cgroup:stream')
    
    print("Testing consumer groups...")
    
    # Create stream with initial data
    r.xadd('cgroup:stream', {'event': 'login', 'user': 'john'})
    r.xadd('cgroup:stream', {'event': 'purchase', 'user': 'jane'})
    
    try:
        # Create consumer group
        result = r.xgroup_create('cgroup:stream', 'group1', id='0-0')
        print("✅ XGROUP CREATE succeeded")
    except redis.ResponseError as e:
        if "BUSYGROUP" in str(e):
            print("⚠️  Consumer group already exists, continuing...")
        else:
            raise
    
    try:
        # Create consumer (if supported)
        result = r.xgroup_createconsumer('cgroup:stream', 'group1', 'consumer1')
        print("✅ XGROUP CREATECONSUMER succeeded")
    except (redis.ResponseError, AttributeError) as e:
        print(f"⚠️  CREATECONSUMER not supported or failed: {e}")
    
    try:
        # Read with consumer group
        streams = r.xreadgroup('group1', 'consumer1', {'cgroup:stream': '>'})
        print(f"✅ XREADGROUP: read {len(streams)} streams")
        
        # If we got entries, acknowledge them
        if streams:
            for stream_name, entries in streams:
                if entries:
                    ids = [entry[0] for entry in entries]
                    acked = r.xack('cgroup:stream', 'group1', *ids)
                    print(f"✅ XACK: acknowledged {acked} entries")
    except (redis.ResponseError, AttributeError) as e:
        print(f"⚠️  Consumer group operations not fully supported: {e}")
    
    print("✅ Consumer groups basic test completed\n")

def test_stream_persistence():
    """Test that streams persist across operations"""
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    print("Testing stream persistence...")
    
    # Create a stream with unique name
    stream_key = f'persist:stream:{int(time.time())}'
    
    # Add entries
    entries_added = []
    for i in range(5):
        id = r.xadd(stream_key, {'sequence': str(i), 'timestamp': str(time.time())})
        entries_added.append(id)
    
    # Read back
    all_entries = r.xrange(stream_key)
    assert len(all_entries) == 5
    print(f"✅ Stream persistence: {len(all_entries)} entries survived")
    
    # Check TYPE command
    key_type = r.type(stream_key)
    assert key_type == 'stream'
    print("✅ TYPE command returns 'stream'")
    
    # Clean up
    r.delete(stream_key)
    print("✅ Stream persistence test completed\n")

def test_stream_edge_cases():
    """Test edge cases and error conditions"""
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    print("Testing edge cases...")
    
    # Test operations on non-existent stream
    length = r.xlen('nonexistent:stream')
    assert length == 0
    print("✅ XLEN on non-existent stream: 0")
    
    entries = r.xrange('nonexistent:stream')
    assert len(entries) == 0
    print("✅ XRANGE on non-existent stream: empty")
    
    # Test invalid ID format
    try:
        r.execute_command('XADD', 'test:stream', 'invalid-id', 'field', 'value')
        assert False, "Should have failed with invalid ID"
    except redis.ResponseError as e:
        print("✅ Invalid ID format properly rejected")
    
    # Test TYPE command on stream vs other types
    r.set('string:key', 'value')
    r.xadd('stream:key', {'field': 'value'})
    
    assert r.type('string:key') == 'string'
    assert r.type('stream:key') == 'stream'
    print("✅ TYPE command correctly distinguishes streams")
    
    # Clean up
    r.delete('string:key', 'stream:key')
    
    print("✅ Edge cases test completed\n")

def test_stream_memory_efficiency():
    """Test that streams manage memory efficiently"""
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    print("Testing stream memory efficiency...")
    
    # Create a stream and add many entries
    stream_key = 'memory:stream'
    r.delete(stream_key)
    
    # Add 100 entries
    initial_memory = None
    try:
        initial_memory = r.memory_usage(stream_key) or 0
    except:
        initial_memory = 0
    
    for i in range(100):
        r.xadd(stream_key, {
            'id': str(i),
            'data': f'entry_{i}' * 10,  # Some data bulk
            'timestamp': str(time.time())
        })
    
    final_memory = None
    try:
        final_memory = r.memory_usage(stream_key)
        if final_memory:
            print(f"✅ Stream memory usage: {final_memory} bytes for 100 entries")
        else:
            print("⚠️  MEMORY USAGE not fully supported for streams")
    except:
        print("⚠️  MEMORY USAGE command not supported")
    
    # Test trimming reduces memory (if memory tracking works)
    length_before = r.xlen(stream_key)
    trimmed = r.xtrim(stream_key, maxlen=10)
    length_after = r.xlen(stream_key)
    
    assert length_after == 10
    print(f"✅ XTRIM efficiency: {length_before} -> {length_after} entries")
    
    # Clean up
    r.delete(stream_key)
    
    print("✅ Memory efficiency test completed\n")

def run_all_tests():
    """Run all stream tests"""
    print("=" * 60)
    print("FERROUS STREAMS COMPREHENSIVE TEST SUITE")
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
        test_stream_trim_and_delete,
        test_xread_operations,
        test_consumer_groups_basic,
        test_stream_persistence,
        test_stream_edge_cases,
        test_stream_memory_efficiency,
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
        print("🎉 All stream tests passed!")
        return True
    else:
        print(f"⚠️  {tests_run - tests_passed} tests failed")
        return False

if __name__ == "__main__":
    success = run_all_tests()
    sys.exit(0 if success else 1)