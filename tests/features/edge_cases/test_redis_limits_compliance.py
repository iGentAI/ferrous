#!/usr/bin/env python3
"""
Redis Limits Compliance Test Suite for Ferrous
Tests edge cases, limits, and boundary conditions for production safety
"""

import redis
import sys
import time
import random
import string

def test_key_size_limits():
    """Test Redis key size limits and edge cases"""
    print("Testing key size limits and edge cases...")
    
    r = redis.Redis(host='localhost', port=6379, decode_responses=False)
    
    # Test 1: Empty key rejection (should be fixed now)
    try:
        r.set('', 'value')
        print("❌ Empty key should be rejected")
        return False
    except redis.ResponseError as e:
        if "empty string keys are not allowed" in str(e):
            print("✅ Empty key properly rejected")
            success_1 = True
        else:
            print(f"❌ Wrong error for empty key: {e}")
            success_1 = False
    
    # Test 2: Very long keys (Redis allows up to 512MB but test reasonable limits)
    long_key = 'k' * 10000  # 10KB key
    try:
        r.set(long_key, 'value')
        result = r.get(long_key)
        if result == b'value':
            print("✅ Long key (10KB) handled correctly")
            success_2 = True
        else:
            print("❌ Long key retrieval failed")
            success_2 = False
        r.delete(long_key)
    except Exception as e:
        print(f"❌ Long key test failed: {e}")
        success_2 = False
    
    # Test 3: Keys with special characters and binary data
    special_keys = [
        b'\x00\x01\x02\x03',  # Binary data
        b'\xff\xfe\xfd',      # High bytes
        'key\nwith\nnewlines',  # Newlines
        'key\twith\ttabs',     # Tabs
        '키한글',               # Unicode
        'key with spaces',     # Spaces
        'UPPERCASE',           # Case sensitivity
        'lowercase',
    ]
    
    success_3 = True
    for i, key in enumerate(special_keys):
        try:
            r.set(key, f'value_{i}')
            result = r.get(key)
            if result == f'value_{i}'.encode('utf-8'):
                print(f"✅ Special key test {i+1}: {repr(key)}")
            else:
                print(f"❌ Special key test {i+1} failed: {repr(key)}")
                success_3 = False
            r.delete(key)
        except Exception as e:
            print(f"❌ Special key test {i+1} error: {e}")
            success_3 = False
    
    return success_1 and success_2 and success_3

def test_value_size_limits():
    """Test Redis value size limits"""
    print("Testing value size limits...")
    
    r = redis.Redis(host='localhost', port=6379, decode_responses=False)
    
    # Test 1: Large values (1MB)
    large_value = b'x' * (1024 * 1024)  # 1MB
    try:
        r.set('large_value_test', large_value)
        result = r.get('large_value_test')
        if result == large_value:
            print("✅ Large value (1MB) handled correctly")
            success_1 = True
        else:
            print("❌ Large value retrieval failed")
            success_1 = False
        r.delete('large_value_test')
    except Exception as e:
        print(f"❌ Large value test failed: {e}")
        success_1 = False
    
    # Test 2: Binary values with all byte values
    binary_value = bytes(range(256))  # All possible byte values
    try:
        r.set('binary_test', binary_value)
        result = r.get('binary_test')
        if result == binary_value:
            print("✅ Binary value (all bytes 0-255) handled correctly")
            success_2 = True
        else:
            print("❌ Binary value retrieval failed")
            success_2 = False
        r.delete('binary_test')
    except Exception as e:
        print(f"❌ Binary value test failed: {e}")
        success_2 = False
    
    # Test 3: Empty values
    try:
        r.set('empty_value_test', '')
        result = r.get('empty_value_test')
        if result == b'':
            print("✅ Empty value handled correctly")
            success_3 = True
        else:
            print(f"❌ Empty value retrieval failed: {result}")
            success_3 = False
        r.delete('empty_value_test')
    except Exception as e:
        print(f"❌ Empty value test failed: {e}")
        success_3 = False
    
    return success_1 and success_2 and success_3

def test_collection_size_limits():
    """Test collection size limits for lists, sets, hashes, sorted sets"""
    print("Testing collection size limits...")
    
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Test large list (10K items)
    large_list_key = 'large_list_test'
    r.delete(large_list_key)
    
    try:
        # Push 10K items
        for i in range(0, 10000, 100):  # Batch by 100 for efficiency
            items = [f'item_{j}' for j in range(i, min(i + 100, 10000))]
            r.lpush(large_list_key, *items)
        
        list_len = r.llen(large_list_key)
        if list_len == 10000:
            # Test random access
            random_idx = random.randint(0, 9999)
            item = r.lindex(large_list_key, random_idx)
            print(f"✅ Large list (10K items) handled correctly, random access works")
            success_1 = True
        else:
            print(f"❌ Large list length wrong: {list_len}")
            success_1 = False
        
        r.delete(large_list_key)
    except Exception as e:
        print(f"❌ Large list test failed: {e}")
        success_1 = False
    
    # Test large hash (5K fields)
    large_hash_key = 'large_hash_test'
    try:
        # Use individual HSET calls instead of deprecated HMSET
        for i in range(5000):
            r.hset(large_hash_key, f'field_{i}', f'value_{i}')
        
        hash_len = r.hlen(large_hash_key)
        if hash_len == 5000:
            # Test random field access
            random_field = f'field_{random.randint(0, 4999)}'
            value = r.hget(large_hash_key, random_field)
            print(f"✅ Large hash (5K fields) handled correctly")
            success_2 = True
        else:
            print(f"❌ Large hash length wrong: {hash_len}")
            success_2 = False
        
        r.delete(large_hash_key)
    except Exception as e:
        print(f"❌ Large hash test failed: {e}")
        success_2 = False
    
    # Test large sorted set (5K members)
    large_zset_key = 'large_zset_test'
    try:
        # Add 5K members with scores
        for i in range(0, 5000, 100):  # Batch for efficiency
            score_members = {}
            for j in range(i, min(i + 100, 5000)):
                score_members[f'member_{j}'] = j * 1.5
            r.zadd(large_zset_key, score_members)
        
        zset_card = r.zcard(large_zset_key)
        if zset_card == 5000:
            # Test range access
            members = r.zrange(large_zset_key, 0, 10)
            print(f"✅ Large sorted set (5K members) handled correctly")
            success_3 = True
        else:
            print(f"❌ Large sorted set cardinality wrong: {zset_card}")
            success_3 = False
        
        r.delete(large_zset_key)
    except Exception as e:
        print(f"❌ Large sorted set test failed: {e}")
        success_3 = False
    
    return success_1 and success_2 and success_3

def test_numeric_edge_cases():
    """Test numeric edge cases for INCR, scores, etc."""
    print("Testing numeric edge cases...")
    
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Test 1: Integer overflow boundaries - Redis should prevent overflow
    overflow_test_cases = [
        ('max_int', str(2**63 - 1), True),     # Max int64 - should overflow
        ('min_int', str(-2**63), False),       # Min int64 - can be incremented
        ('zero', '0', False),                  # Zero - can be incremented
        ('negative', '-12345', False),         # Negative - can be incremented
    ]
    
    success_1 = True
    for key, initial_value, expect_overflow in overflow_test_cases:
        try:
            r.set(key, initial_value)
            # Test INCR
            try:
                result = r.incr(key)
                if expect_overflow:
                    print(f"❌ INCR edge case failed: {key} should have overflowed but got {result}")
                    success_1 = False
                else:
                    expected = int(initial_value) + 1
                    if result == expected:
                        print(f"✅ INCR edge case: {key} = {initial_value} -> {result}")
                    else:
                        print(f"❌ INCR edge case failed: {key}, expected {expected}, got {result}")
                        success_1 = False
            except redis.ResponseError as e:
                if expect_overflow and "overflow" in str(e):
                    print(f"✅ INCR edge case: {key} correctly rejected overflow")
                else:
                    print(f"❌ INCR edge case error for {key}: {e}")
                    success_1 = False
            
            r.delete(key)
        except Exception as e:
            print(f"❌ INCR edge case setup error for {key}: {e}")
            success_1 = False
    
    # Test 2: Float edge cases for sorted sets
    float_test_cases = [
        ('inf', float('inf')),
        ('neg_inf', float('-inf')),
        ('very_small', 1e-10),
        ('very_large', 1e10),
        ('precise', 3.141592653589793),
    ]
    
    success_2 = True
    zset_key = 'float_edge_test'
    r.delete(zset_key)
    
    for member, score in float_test_cases:
        try:
            r.zadd(zset_key, {member: score})
            retrieved_score = r.zscore(zset_key, member)
            
            # Handle infinity comparison
            if score == float('inf') and retrieved_score == float('inf'):
                print(f"✅ Float edge case: {member} = +inf")
            elif score == float('-inf') and retrieved_score == float('-inf'):
                print(f"✅ Float edge case: {member} = -inf")
            elif abs(float(retrieved_score) - score) < 1e-9:
                print(f"✅ Float edge case: {member} = {score}")
            else:
                print(f"❌ Float edge case failed: {member}, expected {score}, got {retrieved_score}")
                success_2 = False
                
        except Exception as e:
            print(f"❌ Float edge case error for {member}: {e}")
            success_2 = False
    
    r.delete(zset_key)
    return success_1 and success_2

def test_concurrent_data_safety():
    """Test data corruption prevention under concurrent access"""
    print("Testing concurrent data safety...")
    
    r_setup = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Test concurrent modifications to same key
    def concurrent_modifier(worker_id, shared_key):
        r_worker = redis.Redis(host='localhost', port=6379, decode_responses=True)
        errors = []
        
        for i in range(100):
            try:
                # Mix of different operations on same key
                operations = [
                    lambda: r_worker.set(shared_key, f'worker_{worker_id}_op_{i}'),
                    lambda: r_worker.append(shared_key, f'_append_{worker_id}'),
                    lambda: r_worker.get(shared_key),
                    lambda: r_worker.exists(shared_key),
                ]
                
                op = random.choice(operations)
                op()
                
            except Exception as e:
                errors.append(f"Worker {worker_id}, op {i}: {e}")
        
        return len(errors)
    
    # Run concurrent operations
    import threading
    shared_test_key = 'concurrent_safety_test'
    r_setup.delete(shared_test_key)
    r_setup.set(shared_test_key, 'initial_value')
    
    threads = []
    error_counts = [0] * 10
    
    def worker_wrapper(worker_id):
        error_counts[worker_id] = concurrent_modifier(worker_id, shared_test_key)
    
    for i in range(10):
        t = threading.Thread(target=worker_wrapper, args=(i,))
        threads.append(t)
        t.start()
    
    for t in threads:
        t.join()
    
    total_errors = sum(error_counts)
    if total_errors == 0:
        print("✅ Concurrent data safety: No errors in 1000 concurrent operations")
        success = True
    else:
        print(f"❌ Concurrent data safety: {total_errors} errors detected")
        success = False
    
    r_setup.delete(shared_test_key)
    return success

def test_memory_pressure_handling():
    """Test behavior under simulated memory pressure"""
    print("Testing memory pressure handling...")
    
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Create many keys to simulate memory pressure
    keys_created = 0
    max_keys = 50000  # Reasonable test limit
    
    try:
        for i in range(max_keys):
            key = f'memory_test_{i}'
            value = f'value_{i}_{"x" * 100}'  # ~100 byte values
            r.set(key, value)
            keys_created += 1
            
            # Test every 5000 keys that basic operations still work
            if i % 5000 == 0 and i > 0:
                test_key = f'memory_test_{i//2}'
                if r.exists(test_key):
                    retrieved = r.get(test_key)
                    expected = f'value_{i//2}_{"x" * 100}'
                    if retrieved != expected:
                        print(f"❌ Data corruption detected at key count {i}")
                        return False
    
        print(f"✅ Memory pressure test: Created {keys_created} keys without corruption")
        
        # Cleanup with batched deletes for efficiency
        for i in range(0, keys_created, 1000):
            batch_keys = [f'memory_test_{j}' for j in range(i, min(i + 1000, keys_created))]
            r.delete(*batch_keys)
        
        return True
        
    except Exception as e:
        print(f"❌ Memory pressure test failed at {keys_created} keys: {e}")
        # Cleanup
        for i in range(0, keys_created, 1000):
            try:
                batch_keys = [f'memory_test_{j}' for j in range(i, min(i + 1000, keys_created))]
                r.delete(*batch_keys)
            except:
                pass
        return False

def test_protocol_edge_cases():
    """Test RESP protocol edge cases and malformed input handling"""
    print("Testing RESP protocol edge cases...")
    
    import socket
    
    def test_raw_protocol(commands_and_expectations):
        results = []
        for cmd, expect_error in commands_and_expectations:
            s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            try:
                s.connect(('127.0.0.1', 6379))
                s.settimeout(2.0)
                s.sendall(cmd)
                response = s.recv(1024)
                
                if expect_error:
                    if b'-ERR' in response or len(response) == 0:
                        results.append(True)  # Expected error or disconnect
                    else:
                        results.append(False)  # Should have errored but didn't
                else:
                    if b'+OK' in response or b'PONG' in response:
                        results.append(True)  # Expected success
                    else:
                        results.append(False)  # Should have succeeded but didn't
            except Exception:
                results.append(expect_error)  # Exception is okay if error expected
            finally:
                try:
                    s.close()
                except:
                    pass
            
            # Small delay between tests
            time.sleep(0.1)
        
        return results
    
    # Protocol edge cases
    test_cases = [
        # (command_bytes, expect_error)
        (b'*1\r\n$4\r\nPING\r\n', False),  # Valid PING
        (b'*1\r\n$4\r\nPING\r\n\r\n', False),  # Extra CRLF (should be tolerant)
        (b'*0\r\n', True),  # Empty array (should error)
        (b'*1\r\n$0\r\n\r\n', True),  # Empty command (should error)
        (b'*-1\r\n', True),  # Null array (invalid)
        (b'$-1\r\n', True),  # Standalone null string (invalid)
        (b'*2\r\n$3\r\nSET\r\n$-1\r\n', True),  # SET with null value (should error)
    ]
    
    results = test_raw_protocol(test_cases)
    passed = sum(results)
    total = len(results)
    
    if passed == total:
        print(f"✅ Protocol edge cases: {passed}/{total} handled correctly")
        return True
    else:
        print(f"❌ Protocol edge cases: only {passed}/{total} handled correctly")
        return False

def main():
    print("=" * 70)
    print("REDIS LIMITS COMPLIANCE & EDGE CASE TESTS")
    print("=" * 70)
    
    # Verify server connection
    try:
        r = redis.Redis(host='localhost', port=6379, decode_responses=True)
        r.ping()
        print("✅ Server connection verified")
    except Exception as e:
        print(f"❌ Cannot connect to server: {e}")
        sys.exit(1)
    
    print()
    
    # Run edge case and limits tests
    test_functions = [
        test_key_size_limits,
        test_value_size_limits,
        test_collection_size_limits,
        test_numeric_edge_cases,
        test_concurrent_data_safety,
        test_memory_pressure_handling,
        test_protocol_edge_cases,
    ]
    
    results = []
    for test_func in test_functions:
        try:
            print(f"\n{'='*50}")
            result = test_func()
            results.append(result)
        except Exception as e:
            print(f"❌ Test {test_func.__name__} crashed: {e}")
            results.append(False)
    
    # Summary
    passed = sum(results)
    total = len(results)
    
    print("\n" + "=" * 70)
    print(f"TEST RESULTS: {passed}/{total} PASSED")
    print("=" * 70)
    
    if passed == total:
        print("All edge case and limits tests passed")
        sys.exit(0)
    else:
        print(f"{total - passed} tests failed")
        sys.exit(1)

if __name__ == "__main__":
    main()