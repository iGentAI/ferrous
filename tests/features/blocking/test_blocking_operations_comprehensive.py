#!/usr/bin/env python3
"""
Comprehensive Blocking Operations Test Suite for Ferrous
Tests BLPOP/BRPOP for production queue patterns and Redis compliance
"""

import redis
import threading
import time
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed

def test_response_format():
    """Test blocking operations return correct Redis-compliant format"""
    print("Testing response format compliance...")
    
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Test BLPOP format
    r.delete('format_test')
    r.lpush('format_test', 'test_item')
    result = r.blpop('format_test', 1)
    
    if isinstance(result, tuple) and len(result) == 2 and result[0] == 'format_test' and result[1] == 'test_item':
        print("✅ BLPOP response format: Redis-compliant tuple")
        success_1 = True
    else:
        print(f"❌ BLPOP response format issue: {result}")
        success_1 = False
    
    # Test BRPOP format
    r.lpush('format_test', 'test_item2')
    result = r.brpop('format_test', 1)
    
    if isinstance(result, tuple) and len(result) == 2 and result[0] == 'format_test' and result[1] == 'test_item2':
        print("✅ BRPOP response format: Redis-compliant tuple")
        success_2 = True
    else:
        print(f"❌ BRPOP response format issue: {result}")
        success_2 = False
    
    r.delete('format_test')
    return success_1 and success_2

def test_concurrent_blocking():
    """Test multiple concurrent clients blocking on same queue"""
    print("Testing concurrent blocking operations...")
    
    r_setup = redis.Redis(host='localhost', port=6379, decode_responses=True)
    r_setup.delete('concurrent_queue')
    
    results = []
    num_workers = 5
    
    def blocking_worker(worker_id):
        r_worker = redis.Redis(host='localhost', port=6379, decode_responses=True)
        try:
            start = time.time()
            result = r_worker.blpop('concurrent_queue', 10)  # 10 second timeout
            duration = time.time() - start
            return (worker_id, result, duration)
        except Exception as e:
            return (worker_id, None, f"Error: {e}")
    
    # Start concurrent blockers
    with ThreadPoolExecutor(max_workers=num_workers) as executor:
        # Start all workers
        futures = [executor.submit(blocking_worker, i) for i in range(num_workers)]
        
        # Give workers time to start blocking
        time.sleep(0.5)
        
        # Push items to wake them up
        for i in range(num_workers):
            r_setup.lpush('concurrent_queue', f'item_{i}')
            time.sleep(0.1)
        
        # Collect results
        results = [future.result() for future in as_completed(futures, timeout=15)]
    
    successful_blocks = sum(1 for _, result, _ in results if result is not None)
    
    if successful_blocks == num_workers:
        print(f"✅ Concurrent blocking: {successful_blocks}/{num_workers} workers received data")
        success = True
    else:
        print(f"❌ Concurrent blocking failed: only {successful_blocks}/{num_workers} workers succeeded")
        success = False
    
    for worker_id, result, duration in results:
        if result:
            print(f"   Worker {worker_id}: {result[1]} ({duration:.2f}s)")
    
    r_setup.delete('concurrent_queue')
    return success

def test_timeout_precision():
    """Test timeout precision and behavior"""
    print("Testing timeout precision...")
    
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Test 2-second timeout
    start = time.time()
    result = r.blpop('nonexistent_queue', 2)
    duration = time.time() - start
    
    if result is None and 1.8 <= duration <= 2.5:  # Allow some tolerance
        print(f"✅ Timeout precision: {duration:.2f}s (within tolerance)")
        success_1 = True
    else:
        print(f"❌ Timeout precision issue: {duration:.2f}s, result: {result}")
        success_1 = False
    
    # Test zero timeout (should return immediately)
    start = time.time()
    result = r.blpop('nonexistent_queue_2', 0.1)  # Very short timeout
    duration = time.time() - start
    
    if result is None and duration < 0.5:
        print(f"✅ Short timeout: {duration:.2f}s (immediate return)")
        success_2 = True
    else:
        print(f"❌ Short timeout issue: {duration:.2f}s")
        success_2 = False
    
    return success_1 and success_2

def test_fifo_ordering():
    """Test FIFO ordering is preserved in blocking operations"""
    print("Testing FIFO ordering preservation...")
    
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    r.delete('fifo_test')
    
    # Push items in order using RPUSH for proper FIFO queue behavior
    for i in range(5):
        r.rpush('fifo_test', f'item_{i}')  # RPUSH adds to tail for FIFO order
    
    # Pop items and verify order (RPUSH to tail, BLPOP from head = FIFO)
    expected_order = ['item_0', 'item_1', 'item_2', 'item_3', 'item_4']
    actual_order = []
    
    for i in range(5):
        result = r.blpop('fifo_test', 1)
        if result:
            actual_order.append(result[1])
    
    if actual_order == expected_order:
        print(f"✅ FIFO ordering: {actual_order}")
        success = True
    else:
        print(f"❌ FIFO ordering broken: expected {expected_order}, got {actual_order}")
        success = False
    
    r.delete('fifo_test')
    return success

def test_multiple_queue_priority():
    """Test multiple queue scanning order"""
    print("Testing multiple queue priority order...")
    
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Clean slate
    r.delete('queue_a', 'queue_b', 'queue_c')
    
    # Put item only in queue_c (last in list)
    r.lpush('queue_c', 'item_from_c')
    
    # BLPOP should find it and return queue_c
    result = r.blpop(['queue_a', 'queue_b', 'queue_c'], 1)
    
    if result and result[0] == 'queue_c' and result[1] == 'item_from_c':
        print("✅ Multiple queue priority: Correctly scans queues in order")
        success_1 = True
    else:
        print(f"❌ Multiple queue priority failed: {result}")
        success_1 = False
    
    # Test priority - put items in multiple queues
    r.lpush('queue_a', 'item_a')
    r.lpush('queue_b', 'item_b')
    r.lpush('queue_c', 'item_c_2')
    
    # Should get item from first queue that has data
    result = r.blpop(['queue_a', 'queue_b', 'queue_c'], 1)
    
    if result and result[0] == 'queue_a' and result[1] == 'item_a':
        print("✅ Queue priority: Returns from first available queue")
        success_2 = True
    else:
        print(f"❌ Queue priority failed: {result}")
        success_2 = False
    
    r.delete('queue_a', 'queue_b', 'queue_c')
    return success_1 and success_2

def test_blocking_with_transactions():
    """Test blocking operations interaction with transactions"""
    print("Testing blocking operations with transactions...")
    
    r1 = redis.Redis(host='localhost', port=6379, decode_responses=True)
    r2 = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Test that blocking operations work correctly with transactions
    r1.delete('tx_block_queue')
    
    # Use transaction to atomically push multiple items
    with r1.pipeline() as pipe:
        pipe.multi()
        pipe.lpush('tx_block_queue', 'tx_item1', 'tx_item2')
        pipe.execute()
    
    # Verify blocking pop works on transactionally added items
    result = r2.blpop('tx_block_queue', 1)
    
    if result and 'tx_item' in result[1]:
        print("✅ Blocking with transactions: Working correctly")
        success = True
    else:
        print(f"❌ Blocking with transactions failed: {result}")
        success = False
    
    r1.delete('tx_block_queue')
    return success

def test_error_conditions():
    """Test comprehensive error handling"""
    print("Testing error conditions...")
    
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    test_cases = [
        # (command, expected_error_fragment)
        (lambda: r.execute_command("BLPOP"), "wrong number of arguments"),
        (lambda: r.execute_command("BLPOP", "key"), "wrong number of arguments"),
        (lambda: r.execute_command("BLPOP", "key", "invalid_timeout"), "timeout is not"),
        (lambda: r.execute_command("BLPOP", "key", "-1"), "timeout is not"),
        (lambda: r.execute_command("BRPOP"), "wrong number of arguments"),
        (lambda: r.execute_command("BRPOP", "key"), "wrong number of arguments"),
        (lambda: r.execute_command("BRPOP", "key", "invalid_timeout"), "timeout is not"),
    ]
    
    passed = 0
    total = len(test_cases)
    
    for i, (command, expected_error) in enumerate(test_cases):
        try:
            command()
            print(f"❌ Test {i+1}: Should have failed but didn't")
        except redis.ResponseError as e:
            if expected_error.lower() in str(e).lower():
                print(f"✅ Test {i+1}: Correct error - {expected_error}")
                passed += 1
            else:
                print(f"❌ Test {i+1}: Wrong error - {e}")
        except Exception as e:
            print(f"❌ Test {i+1}: Unexpected error - {e}")
    
    return passed == total

def main():
    print("=" * 70)
    print("BLOCKING OPERATIONS COMPREHENSIVE TESTS")
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
    
    # Run comprehensive blocking tests
    test_functions = [
        test_response_format,
        test_concurrent_blocking,
        test_timeout_precision,
        test_fifo_ordering,
        test_multiple_queue_priority,
        test_blocking_with_transactions,
        test_error_conditions,
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
    print(f"BLOCKING OPERATIONS TEST RESULTS: {passed}/{total} PASSED")
    print("=" * 70)
    
    if passed == total:
        print("All blocking operations tests passed")
        sys.exit(0)
    else:
        print(f"{total - passed} blocking operations tests failed")
        sys.exit(1)

if __name__ == "__main__":
    main()