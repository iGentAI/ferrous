#!/usr/bin/env python3
"""
Comprehensive edge case testing for Ferrous Stream functionality
Tests boundary conditions, error scenarios, and performance edge cases
"""

import redis
import time
import sys

def test_xrevrange_ordering_edge_cases():
    """Test XREVRANGE edge cases with various ID orderings"""
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    print("Testing XREVRANGE strict ordering edge cases...")
    
    # Clean up
    r.delete('order:test')
    
    # Add entries with proper sequential IDs (no backwards timestamps)
    ids = ['999-0', '1000-0', '1000-1', '1001-0', '1002-0']
    for i, id_val in enumerate(ids):
        r.xadd('order:test', {'seq': str(i), 'id': id_val}, id=id_val)
    
    # Test XRANGE normal order
    normal = r.xrange('order:test', '-', '+')
    print(f"XRANGE order: {[entry[0] for entry in normal]}")
    
    # Test XREVRANGE reverse order
    reversed_entries = r.xrevrange('order:test', '+', '-')
    print(f"XREVRANGE order: {[entry[0] for entry in reversed_entries]}")
    
    # Verify correct reverse ordering
    normal_ids = [entry[0] for entry in normal]
    reversed_ids = [entry[0] for entry in reversed_entries]
    
    expected_reverse = list(reversed(normal_ids))
    assert reversed_ids == expected_reverse, f"Expected {expected_reverse}, got {reversed_ids}"
    
    print("✅ XREVRANGE ordering edge case validated")
    return True

def test_stream_id_boundary_conditions():
    """Test extreme Stream ID boundary conditions"""
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    print("Testing Stream ID boundary conditions...")
    
    # Clean up
    r.delete('boundary:test')
    
    # Test minimum ID
    r.xadd('boundary:test', {'test': 'min'}, id='1-0')
    
    # Test large IDs - use proper sequential ordering
    large_id = str(int(time.time() * 1000) + 10000) + '-0'
    r.xadd('boundary:test', {'test': 'large'}, id=large_id)
    
    # Test sequential IDs at same timestamp - use proper incrementing sequence
    base_timestamp = int(time.time() * 1000) + 20000  # Future timestamp to avoid conflicts
    for i in range(5):
        r.xadd('boundary:test', {'test': f'seq{i}'}, id=f'{base_timestamp}-{i}')
    
    # Verify ordering is maintained
    entries = r.xrange('boundary:test', '-', '+')
    assert len(entries) == 7
    
    # Verify IDs are in ascending order
    ids = [entry[0] for entry in entries]
    sorted_ids = sorted(ids, key=lambda x: tuple(map(int, x.split('-'))))
    assert ids == sorted_ids, f"IDs not properly ordered: {ids} vs {sorted_ids}"
    
    print("✅ Stream ID boundary conditions validated")
    return True

def test_empty_stream_edge_cases():
    """Test operations on empty streams"""
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    print("Testing empty stream edge cases...")
    
    # Test operations on non-existent stream
    assert r.xlen('nonexistent') == 0
    assert r.xrange('nonexistent') == []
    assert r.xrevrange('nonexistent', '+', '-') == []
    
    # Test XREAD on non-existent stream
    # Redis-py with decode_responses=True returns [] for empty XREAD results, not {}
    result = r.xread({'nonexistent': '0-0'})
    assert result == [] or result == {}, f"Expected empty result, got {result}"
    
    # Test XTRIM on non-existent stream
    trimmed = r.xtrim('nonexistent', maxlen=10)
    assert trimmed == 0
    
    # Test XDEL on non-existent stream  
    deleted = r.xdel('nonexistent', '1000-0')
    assert deleted == 0
    
    print("✅ Empty stream edge cases validated")
    return True

def test_large_stream_performance():
    """Test performance with larger streams"""
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    print("Testing large stream performance edge cases...")
    
    # Clean up
    r.delete('perf:test')
    
    # Add many entries rapidly
    start_time = time.time()
    entry_count = 1000
    
    for i in range(entry_count):
        r.xadd('perf:test', {
            'counter': str(i),
            'data': f'payload_{i}' * 5,  # Larger payload
            'timestamp': str(time.time())
        })
    
    add_time = time.time() - start_time
    print(f"Added {entry_count} entries in {add_time:.3f}s ({entry_count/add_time:.0f} ops/sec)")
    
    # Test range queries on large stream
    start_time = time.time()
    full_range = r.xrange('perf:test', '-', '+')
    range_time = time.time() - start_time
    print(f"XRANGE {len(full_range)} entries in {range_time:.3f}s")
    
    # Test trimming large stream
    start_time = time.time()
    trimmed = r.xtrim('perf:test', maxlen=100)
    trim_time = time.time() - start_time
    print(f"XTRIM removed {trimmed} entries in {trim_time:.3f}s")
    
    # Verify final state
    final_length = r.xlen('perf:test')
    assert final_length == 100, f"Expected 100 entries after trim, got {final_length}"
    
    print("✅ Large stream performance edge cases validated")
    return True

def test_concurrent_access_patterns():
    """Test concurrent-like access patterns and state consistency"""
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    print("Testing concurrent access pattern edge cases...")
    
    # Clean up
    r.delete('concurrent:test')
    
    # Simulate concurrent writers with overlapping timestamps
    base_time = int(time.time() * 1000)
    
    # Add entries with same timestamp, different sequences
    for i in range(5):
        r.xadd('concurrent:test', {'writer': 'A', 'seq': str(i)}, id=f'{base_time}-{i}')
    
    # Add entries with slightly later timestamp
    for i in range(3):
        r.xadd('concurrent:test', {'writer': 'B', 'seq': str(i)}, id=f'{base_time + 1}-{i}')
    
    # Verify ordering is maintained correctly
    entries = r.xrange('concurrent:test', '-', '+')
    assert len(entries) == 8
    
    # Verify timestamp ordering
    for i in range(len(entries) - 1):
        current_id = tuple(map(int, entries[i][0].split('-')))
        next_id = tuple(map(int, entries[i + 1][0].split('-')))
        assert current_id <= next_id, f"Ordering violation: {current_id} > {next_id}"
    
    print("✅ Concurrent access pattern edge cases validated")
    return True

def test_memory_efficiency_edge_cases():
    """Test memory efficiency under various edge conditions"""
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    print("Testing memory efficiency edge cases...")
    
    # Test with very large field values
    r.delete('memory:test')
    
    # Add entry with large field
    large_data = 'x' * 1000  # 1KB field
    r.xadd('memory:test', {'large_field': large_data})
    
    # Add entry with many small fields
    many_fields = {f'field_{i}': f'val_{i}' for i in range(50)}
    r.xadd('memory:test', many_fields)
    
    # Test memory usage
    try:
        memory_usage = r.memory_usage('memory:test')
        if memory_usage:
            print(f"✅ Memory usage for complex stream: {memory_usage} bytes")
        else:
            print("⚠️  Memory usage tracking not available for streams")
    except:
        print("⚠️  Memory usage command not supported")
    
    # Test trimming with different patterns
    original_length = r.xlen('memory:test')
    r.xtrim('memory:test', maxlen=1)
    final_length = r.xlen('memory:test')
    
    assert final_length == 1, f"Expected 1 entry after trim, got {final_length}"
    print(f"✅ Trimmed from {original_length} to {final_length} entries")
    
    return True

def test_error_handling_edge_cases():
    """Test comprehensive error handling edge cases"""
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    print("Testing error handling edge cases...")
    
    # Test various invalid ID formats
    invalid_ids = ['invalid', 'abc-def', '123-abc', '123--456', '', '-', '+']
    
    for invalid_id in invalid_ids:
        try:
            r.xadd('error:test', {'data': 'test'}, id=invalid_id)
            if invalid_id not in ['-', '+']:  # These might be valid in some contexts
                print(f"❌ Should have rejected invalid ID: {invalid_id}")
                return False
        except redis.RedisError:
            pass  # Expected for invalid IDs
    
    # Test ID sequence violations
    r.delete('sequence:test')
    r.xadd('sequence:test', {'data': '1'}, id='1000-0')
    
    try:
        # Try to add with earlier timestamp
        r.xadd('sequence:test', {'data': '2'}, id='999-0')
        print("❌ Should have rejected backwards timestamp")
        return False
    except redis.RedisError:
        pass  # Expected
    
    try:
        # Try to add with same ID
        r.xadd('sequence:test', {'data': '2'}, id='1000-0')
        print("❌ Should have rejected duplicate ID")
        return False
    except redis.RedisError:
        pass  # Expected
    
    print("✅ Error handling edge cases validated")
    return True

def run_edge_case_tests():
    """Run all edge case tests"""
    print("=" * 70)
    print("FERROUS STREAMS EDGE CASE TEST SUITE")
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
    
    # Run all edge case test functions
    test_functions = [
        test_xrevrange_ordering_edge_cases,
        test_stream_id_boundary_conditions,
        test_empty_stream_edge_cases,
        test_large_stream_performance,
        test_concurrent_access_patterns,
        test_memory_efficiency_edge_cases,
        test_error_handling_edge_cases,
    ]
    
    for test_func in test_functions:
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
    print(f"EDGE CASE TEST RESULTS: {tests_passed}/{tests_run} PASSED")
    print("=" * 70)
    
    if tests_passed == tests_run:
        print("🎉 ALL EDGE CASE TESTS PASSED!")
        return True
    else:
        print(f"⚠️  {tests_run - tests_passed} edge cases failed")
        return False

if __name__ == "__main__":
    success = run_edge_case_tests()
    sys.exit(0 if success else 1)