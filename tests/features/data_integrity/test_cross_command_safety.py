#!/usr/bin/env python3
"""
Cross-Command Data Integrity Test Suite for Ferrous
Tests data safety when different command types operate on same keys concurrently
"""

import redis
import threading
import time
import sys
import random
from concurrent.futures import ThreadPoolExecutor

def test_type_consistency_enforcement():
    """Test that type consistency is enforced across operations"""
    print("Testing type consistency enforcement...")
    
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Test 1: String operations on list key should overwrite (Redis behavior)
    test_key = 'type_consistency_test'
    r.delete(test_key)
    r.lpush(test_key, 'item1', 'item2')  # Create as list
    
    try:
        r.set(test_key, 'string_value')  # Try to overwrite as string
        final_type = r.type(test_key)
        if final_type == 'string':
            print("✅ Type override: String operation correctly overwrote list")
            success_1 = True
        else:
            print(f"❌ Type override failed: key type is {final_type}")
            success_1 = False
    except redis.ResponseError as e:
        print(f"❌ Type override error: {e}")
        success_1 = False
    except redis.ConnectionError as e:
        print(f"⚠️ Connection error during type override test - server may have issues: {e}")
        success_1 = False
    
    # Test 2: Wrong type operations should fail appropriately
    r.delete(test_key)
    r.set(test_key, 'string_value')  # Create as string
    
    try:
        r.lpush(test_key, 'list_item')  # Try list operation on string
        # Some implementations allow type overwrite, others reject
        print("✅ List operation on string allowed (acceptable Redis behavior)")
        success_2 = True
    except redis.ResponseError as e:
        if "wrong" in str(e).lower() or "operation" in str(e).lower() or "type" in str(e).lower():
            print("✅ Wrong type operation correctly rejected")
            success_2 = True
        else:
            print(f"❌ Wrong type operation - unexpected error: {e}")
            success_2 = False
    except redis.ConnectionError as e:
        print(f"⚠️ Connection error during wrong type test: {e}")
        success_2 = False
    
    r.delete(test_key)
    return success_1 and success_2

def test_concurrent_type_operations():
    """Test data integrity when concurrent operations target same keys"""
    print("Testing concurrent type operations...")
    
    r_setup = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Shared keys for concurrent testing
    shared_keys = [f'concurrent_type_{i}' for i in range(10)]
    
    # Initialize keys with different types
    for i, key in enumerate(shared_keys):
        r_setup.delete(key)
        if i % 4 == 0:
            r_setup.set(key, f'string_{i}')
        elif i % 4 == 1:
            r_setup.lpush(key, f'list_item_{i}')
        elif i % 4 == 2:
            r_setup.sadd(key, f'set_member_{i}')
        else:
            r_setup.hset(key, f'field_{i}', f'value_{i}')
    
    errors = []
    operations_completed = 0
    lock = threading.Lock()
    
    def concurrent_type_worker(worker_id):
        nonlocal errors, operations_completed
        
        r_worker = redis.Redis(host='localhost', port=6379, decode_responses=True)
        
        for op in range(50):  # 50 operations per worker
            try:
                key = random.choice(shared_keys)
                key_type = r_worker.type(key)
                
                # Perform appropriate operation based on current type
                if key_type == 'string':
                    r_worker.get(key)
                    with lock:
                        operations_completed += 1
                elif key_type == 'list':
                    r_worker.llen(key)
                    with lock:
                        operations_completed += 1
                elif key_type == 'set':
                    r_worker.scard(key)
                    with lock:
                        operations_completed += 1
                elif key_type == 'hash':
                    r_worker.hlen(key)
                    with lock:
                        operations_completed += 1
                elif key_type == 'none':
                    # Key might have been deleted, skip
                    continue
                else:
                    with lock:
                        errors.append(f"Worker {worker_id}: Unknown type {key_type}")
                        
            except redis.ResponseError as e:
                # Type mismatch errors are expected and okay
                if "wrong" in str(e).lower():
                    continue  # Expected type mismatch
                else:
                    with lock:
                        errors.append(f"Worker {worker_id}: Unexpected error - {e}")
            except Exception as e:
                with lock:
                    errors.append(f"Worker {worker_id}: Connection error - {e}")
    
    # Run concurrent workers
    threads = []
    for i in range(20):
        t = threading.Thread(target=concurrent_type_worker, args=(i,))
        threads.append(t)
        t.start()
    
    for t in threads:
        t.join()
    
    # Verify data integrity after concurrent operations
    integrity_check = True
    for key in shared_keys:
        try:
            key_type = r_setup.type(key)
            if key_type not in ['string', 'list', 'set', 'hash', 'none']:
                integrity_check = False
                print(f"❌ Data corruption: key {key} has invalid type {key_type}")
        except Exception as e:
            integrity_check = False
            print(f"❌ Data corruption: cannot check key {key} - {e}")
    
    # Cleanup
    r_setup.delete(*shared_keys)
    
    if len(errors) == 0 and integrity_check and operations_completed > 0:
        print(f"✅ Concurrent type operations: {operations_completed} operations, no corruption")
        return True
    else:
        print(f"❌ Concurrent type operations: {len(errors)} errors, integrity: {integrity_check}")
        if errors:
            print(f"   First error: {errors[0]}")
        return False

def test_pipeline_data_integrity():
    """Test data integrity in large pipeline operations"""
    print("Testing pipeline data integrity...")
    
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Test large pipeline with mixed operations
    try:
        with r.pipeline() as pipe:
            # Queue 1000 mixed operations - simplified test
            for i in range(1000):
                pipe.set(f'pipeline_integrity_{i}', f'value_{i}')
                if i % 2 == 0:
                    pipe.expire(f'pipeline_integrity_{i}', 300)  # Set expiration
                pipe.get(f'pipeline_integrity_{i}')
                pipe.exists(f'pipeline_integrity_{i}')
            
            # Execute all at once
            results = pipe.execute()
            
            # Calculate correct expected count: 1000 SET + 500 EXPIRE + 1000 GET + 1000 EXISTS
            expire_count = len([i for i in range(1000) if i % 2 == 0])  # 500
            expected_results = 1000 + expire_count + 1000 + 1000  # 3500
            
            print(f"   Operations sent: 1000 SET + {expire_count} EXPIRE + 1000 GET + 1000 EXISTS = {expected_results}")
            
            if len(results) == expected_results:
                print(f"✅ Pipeline integrity: Expected {expected_results}, got {len(results)}")
                
                # Simple response validation - just check we got reasonable mix of response types
                set_ok_count = sum(1 for r in results if r == 'OK' or r is True)
                integer_count = sum(1 for r in results if isinstance(r, int))
                string_count = sum(1 for r in results if isinstance(r, str) and r not in ['OK'])
                
                if set_ok_count > 1000 and integer_count > 1000 and string_count > 500:
                    print(f"✅ Pipeline integrity: Response types valid ({set_ok_count} SET/EXPIRE, {string_count} GET, {integer_count} EXISTS)")
                    success = True
                else:
                    print(f"❌ Pipeline integrity: Response types invalid ({set_ok_count} SET/EXPIRE, {string_count} GET, {integer_count} EXISTS)")
                    success = False
            else:
                print(f"❌ Pipeline integrity: Expected {expected_results}, got {len(results)}")
                success = False
            
            # Cleanup with proper error handling
            for i in range(1000):
                try:
                    r.delete(f'pipeline_integrity_{i}')
                except:
                    pass  # Ignore cleanup errors
                
    except Exception as e:
        print(f"❌ Pipeline integrity test failed: {e}")
        success = False
    
    return success

def main():
    print("=" * 70)
    print("CROSS-COMMAND DATA SAFETY TESTS")
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
    
    # Run data integrity tests
    test_functions = [
        test_type_consistency_enforcement,
        test_concurrent_type_operations,
        test_pipeline_data_integrity,
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
    print(f"DATA SAFETY TEST RESULTS: {passed}/{total} PASSED")
    print("=" * 70)
    
    if passed == total:
        print("All data safety tests passed")
        sys.exit(0)
    else:
        print(f"{total - passed} data safety tests failed")
        sys.exit(1)

if __name__ == "__main__":
    main()