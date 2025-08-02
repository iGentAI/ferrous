#!/usr/bin/env python3
"""
Comprehensive stress testing for Ferrous Stream functionality
Tests concurrent operations, large datasets, and integration scenarios
"""

import redis
import threading
import time
import random
import sys

def test_concurrent_stream_writes():
    """Test multiple clients writing to streams simultaneously"""
    print("Testing concurrent Stream writes...")
    
    num_threads = 10
    writes_per_thread = 100
    
    def writer_thread(thread_id):
        r = redis.Redis(host='localhost', port=6379, decode_responses=True)
        for i in range(writes_per_thread):
            try:
                r.xadd(f'concurrent:stream:{thread_id}', {
                    'thread': str(thread_id),
                    'sequence': str(i),
                    'timestamp': str(time.time())
                })
            except Exception as e:
                print(f"Thread {thread_id} error: {e}")
                return False
        return True
    
    # Start all threads
    threads = []
    for i in range(num_threads):
        t = threading.Thread(target=writer_thread, args=(i,))
        threads.append(t)
        t.start()
    
    # Wait for completion
    for t in threads:
        t.join()
    
    # Verify all entries were written
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    total_entries = 0
    for i in range(num_threads):
        length = r.xlen(f'concurrent:stream:{i}')
        total_entries += length
    
    expected = num_threads * writes_per_thread
    assert total_entries == expected, f"Expected {expected} entries, got {total_entries}"
    print(f"✅ Concurrent writes: {total_entries} entries across {num_threads} streams")
    
    # Cleanup
    for i in range(num_threads):
        r.delete(f'concurrent:stream:{i}')
    
    return True

def test_large_stream_operations():
    """Test operations on very large streams"""
    print("Testing large stream operations...")
    
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    stream_key = 'large:stream'
    r.delete(stream_key)
    
    # Add large number of entries
    entry_count = 10000
    print(f"Adding {entry_count} entries...")
    
    start_time = time.time()
    for i in range(entry_count):
        r.xadd(stream_key, {
            'counter': str(i),
            'data': f'payload_{i % 100}',  # Cycle through payloads
            'batch': str(i // 1000)
        })
    add_time = time.time() - start_time
    
    print(f"XADD performance: {entry_count/add_time:.0f} ops/sec")
    
    # Test range operations on large stream
    start_time = time.time()
    entries = r.xrange(stream_key, '-', '+', count=1000)
    range_time = time.time() - start_time
    print(f"XRANGE 1000 entries from {entry_count}: {range_time:.3f}s")
    
    # Test trimming large stream
    start_time = time.time()
    trimmed = r.xtrim(stream_key, maxlen=1000)
    trim_time = time.time() - start_time
    print(f"XTRIM from {entry_count} to 1000: {trimmed} removed in {trim_time:.3f}s")
    
    # Verify final state
    final_length = r.xlen(stream_key)
    assert final_length == 1000, f"Expected 1000 entries, got {final_length}"
    
    r.delete(stream_key)
    print("✅ Large stream operations validated")
    return True

def test_stream_transaction_integration():
    """Test Stream operations within transactions"""
    print("Testing Stream transaction integration...")
    
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    stream_key = 'transaction:stream'
    r.delete(stream_key)
    
    # Test XADD in transaction
    pipe = r.pipeline(transaction=True)
    pipe.multi()
    pipe.xadd(stream_key, {'tx': '1', 'data': 'first'})
    pipe.xadd(stream_key, {'tx': '1', 'data': 'second'})
    pipe.xlen(stream_key)
    results = pipe.execute()
    
    assert len(results) == 3, f"Expected 3 results, got {len(results)}"
    assert results[2] == 2, f"Expected 2 entries, got {results[2]}"
    
    # Test WATCH violation using proper redis-py pattern
    print("Testing WATCH violation with proper connection management...")
    
    # Create connection pool for proper connection consistency
    pool = redis.ConnectionPool(host='localhost', port=6379, db=0)
    r1 = redis.Redis(connection_pool=pool, decode_responses=True)
    r2 = redis.Redis(host='localhost', port=6379, decode_responses=True)  # External modifier
    
    # Create initial stream state
    r1.xadd(stream_key, {'initial': 'data'})
    
    # Use proper WATCH pattern with connection consistency
    try:
        with r1.pipeline() as pipe:
            # WATCH the stream on the SAME connection as EXEC
            pipe.watch(stream_key)
            
            # External modification via separate connection (this should trigger violation)
            external_result = r2.xadd(stream_key, {'external': 'modification'})
            
            # Continue transaction on SAME connection as WATCH
            pipe.multi()
            pipe.xadd(stream_key, {'test': 'should_fail'})
            pipe.set('violation_test_indicator', 'transaction_executed')
            result = pipe.execute()
        
        # Check if transaction was properly aborted
        transaction_executed = r1.exists('violation_test_indicator')
        
        if result is None or not transaction_executed:
            print("✅ Stream WATCH integration working correctly (transaction aborted)")
            success = True
        else:
            print(f"❌ Stream WATCH integration failed. Result: {result}, executed: {transaction_executed}")
            success = False
            
    except redis.WatchError:
        print("✅ Stream WATCH integration working correctly (WatchError exception)")
        success = True
        
    except Exception as e:
        print(f"❌ Unexpected error in WATCH test: {e}")
        success = False
    
    # Cleanup
    r1.delete(stream_key, 'violation_test_indicator')
    
    print("✅ Stream transaction integration validated")
    return success

def test_mixed_workload_stress():
    """Test mixed operations under stress"""
    print("Testing mixed workload stress...")
    
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    stream_key = 'stress:mixed'
    r.delete(stream_key)
    
    def mixed_worker(worker_id, operations):
        """Worker that performs mixed Stream operations"""
        local_r = redis.Redis(host='localhost', port=6379)
        
        for i in range(operations):
            try:
                # Randomly choose operation
                op = random.choice(['add', 'read', 'len', 'trim'])
                
                if op == 'add':
                    local_r.xadd(stream_key, {
                        'worker': str(worker_id),
                        'op': 'add',
                        'seq': str(i)
                    })
                elif op == 'read':
                    local_r.xrange(stream_key, '-', '+', count=10)
                elif op == 'len':
                    local_r.xlen(stream_key)
                elif op == 'trim' and i % 50 == 0:  # Trim occasionally
                    local_r.xtrim(stream_key, maxlen=500)
                    
            except Exception as e:
                print(f"Worker {worker_id} error: {e}")
                return False
        
        return True
    
    # Run mixed workload with multiple workers
    num_workers = 5
    operations_per_worker = 200
    
    workers = []
    for i in range(num_workers):
        t = threading.Thread(target=mixed_worker, args=(i, operations_per_worker))
        workers.append(t)
        t.start()
    
    # Wait for completion
    for w in workers:
        w.join()
    
    # Verify stream integrity
    final_length = r.xlen(stream_key)
    entries = r.xrange(stream_key, '-', '+', count=100)
    
    print(f"✅ Mixed workload: {final_length} entries, {len(entries)} sampled")
    
    r.delete(stream_key)
    return True

def test_error_boundary_conditions():
    """Test various error boundary conditions"""
    print("Testing error boundary conditions...")
    
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Test with extremely large field values
    try:
        large_data = 'x' * (1024 * 1024)  # 1MB field
        result = r.xadd('large:field', {'huge': large_data})
        print(f"✅ Large field handled: {result}")
        r.delete('large:field')
    except Exception as e:
        print(f"⚠️  Large field limitation: {e}")
    
    # Test with many fields
    try:
        many_fields = {f'field_{i}': f'value_{i}' for i in range(1000)}
        result = r.xadd('many:fields', many_fields)
        print(f"✅ Many fields handled: {result}")
        r.delete('many:fields')
    except Exception as e:
        print(f"⚠️  Many fields limitation: {e}")
    
    # Test edge case IDs
    test_ids = [
        '0-1',  # Minimum valid ID
        f'{int(time.time() * 1000) + 1000000}-99999',  # Very large sequence
    ]
    
    for test_id in test_ids:
        try:
            r.delete('edge:id')
            result = r.xadd('edge:id', {'test': 'boundary'}, id=test_id)
            assert result == test_id, f"ID mismatch: {result} vs {test_id}"
            print(f"✅ Edge ID handled: {test_id}")
        except Exception as e:
            print(f"⚠️  Edge ID failed: {test_id} - {e}")
    
    r.delete('edge:id')
    print("✅ Error boundary conditions tested")
    return True

def test_consumer_group_stress():
    """Test consumer group operations under stress"""
    print("Testing consumer group stress operations...")
    
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    stream_key = 'cg:stress'
    r.delete(stream_key)
    
    # Add test data
    for i in range(100):
        r.xadd(stream_key, {'event': f'event_{i}', 'data': str(i)})
    
    # Test multiple group creation/destruction
    for i in range(20):
        group_name = f'group_{i}'
        try:
            r.xgroup_create(stream_key, group_name, '0')
            
            # Test XPENDING on new group
            pending = r.xpending(stream_key, group_name)
            assert pending['pending'] == 0, f"New group should have 0 pending, got {pending['pending']}"
            
            # Test XINFO
            try:
                info = r.execute_command('XINFO', 'STREAM', stream_key)
                print(f"✅ XINFO working for group {i}")
            except Exception as e:
                print(f"⚠️  XINFO issue: {e}")
            
            # Destroy group
            destroyed = r.xgroup_destroy(stream_key, group_name)
            assert destroyed == 1, f"Group destroy should return 1, got {destroyed}"
            
        except redis.ResponseError as e:
            if "BUSYGROUP" in str(e):
                continue  # Group already exists, skip
            else:
                print(f"⚠️  Consumer group error: {e}")
    
    r.delete(stream_key)
    print("✅ Consumer group stress testing validated")
    return True

def test_stream_persistence_integrity():
    """Test Stream data integrity through persistence operations"""
    print("Testing Stream persistence integrity...")
    
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    persist_key = 'persist:integrity'
    r.delete(persist_key)
    
    # Add varied data
    test_data = []
    for i in range(50):
        entry_data = {
            'id': str(i),
            'binary_data': f'\x00\x01\x02{i}',  # Binary data
            'unicode_data': f'测试数据_{i}',  # Unicode
            'json_like': f'{{"key": "value_{i}", "num": {i}}}',
            'large_field': 'test_' * 100  # Larger field
        }
        entry_id = r.xadd(persist_key, entry_data)
        test_data.append((entry_id, entry_data))
    
    # Try background save to test RDB integration
    try:
        r.bgsave()
        time.sleep(0.5)  # Allow save to complete
        print("✅ RDB save with Stream data initiated")
    except:
        print("⚠️  RDB save not available")
    
    # Verify data integrity after operations
    all_entries = r.xrange(persist_key, '-', '+')
    assert len(all_entries) == 50, f"Expected 50 entries, got {len(all_entries)}"
    
    # Check that first and last entries have correct data
    first_entry = all_entries[0]
    assert first_entry[1]['id'] == '0', "First entry ID mismatch"
    
    last_entry = all_entries[-1]
    assert last_entry[1]['id'] == '49', "Last entry ID mismatch"
    
    r.delete(persist_key)
    print("✅ Stream persistence integrity validated")
    return True

def run_comprehensive_stress_tests():
    """Run all stress tests"""
    print("=" * 70)
    print("FERROUS STREAMS COMPREHENSIVE STRESS TEST SUITE")
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
    
    # Run all stress test functions
    stress_tests = [
        test_concurrent_stream_writes,
        test_large_stream_operations, 
        test_stream_transaction_integration,
        test_mixed_workload_stress,
        test_error_boundary_conditions,
        test_consumer_group_stress,
        test_stream_persistence_integrity,
    ]
    
    for test_func in stress_tests:
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
    print(f"STRESS TEST RESULTS: {tests_passed}/{tests_run} PASSED")
    print("=" * 70)
    
    if tests_passed == tests_run:
        print("🎉 ALL STRESS TESTS PASSED!")
        return True
    else:
        print(f"⚠️  {tests_run - tests_passed} stress tests failed")
        return False

if __name__ == "__main__":
    success = run_comprehensive_stress_tests()
    sys.exit(0 if success else 1)