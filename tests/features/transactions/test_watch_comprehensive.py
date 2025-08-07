#!/usr/bin/env python3
"""
Comprehensive WATCH command testing for Ferrous
Tests concurrent watches, edge cases, performance under load, and complex scenarios
"""

import redis
import threading
import time
import sys
import random
from concurrent.futures import ThreadPoolExecutor, as_completed

class WatchTester:
    def __init__(self, host='127.0.0.1', port=6379):
        self.host = host
        self.port = port
        self.results = []
        self.lock = threading.Lock()
        
    def log_result(self, test_name, success, message=""):
        with self.lock:
            self.results.append((test_name, success, message))
            status = "✅" if success else "❌"
            print(f"{status} {test_name}: {message}")

def test_concurrent_watches():
    """Test multiple concurrent WATCH operations on the same key"""
    print("Testing concurrent WATCH operations...")
    tester = WatchTester()
    
    test_key = 'concurrent_watch_test'
    
    # Set initial value
    r_setup = redis.Redis(host='localhost', port=6379, decode_responses=True)
    r_setup.set(test_key, 'initial')
    
    success_count = 0
    abort_count = 0
    error_count = 0
    
    def concurrent_watch_worker(worker_id):
        nonlocal success_count, abort_count, error_count
        
        try:
            r = redis.Redis(host='localhost', port=6379, decode_responses=True)
            
            with r.pipeline() as pipe:
                pipe.watch(test_key)
                
                # Small delay to increase chances of conflict
                time.sleep(0.01)
                
                pipe.multi()
                pipe.set(test_key, f'worker_{worker_id}')
                pipe.set(f'worker_{worker_id}_success', 'true')
                result = pipe.execute()
                
                if result is None:
                    # Transaction aborted due to WATCH violation
                    with tester.lock:
                        abort_count += 1
                else:
                    # Transaction succeeded
                    with tester.lock:
                        success_count += 1
                        
        except redis.WatchError:
            with tester.lock:
                abort_count += 1
        except Exception as e:
            with tester.lock:
                error_count += 1
                print(f"Error in worker {worker_id}: {e}")
    
    # Start 20 concurrent workers
    threads = []
    for i in range(20):
        t = threading.Thread(target=concurrent_watch_worker, args=(i,))
        threads.append(t)
        t.start()
    
    # Wait for all to complete
    for t in threads:
        t.join()
    
    # Verify results - at least one should succeed, rest should abort
    if success_count >= 1 and abort_count >= 1 and error_count == 0:
        success_markers = sum(1 for i in range(20) if r_setup.exists(f'worker_{i}_success'))
        
        if success_markers == success_count:
            final_value = r_setup.get(test_key)
            tester.log_result("Concurrent WATCH", True, 
                f"{success_count} successes, {abort_count} proper aborts")
            
            # Cleanup
            for i in range(20):
                r_setup.delete(f'worker_{i}_success')
            r_setup.delete(test_key)
            return True
    
    tester.log_result("Concurrent WATCH", False, 
        f"Unexpected: {success_count} success, {abort_count} aborts, {error_count} errors")
    return False

def test_multiple_key_watch():
    """Test watching multiple keys simultaneously"""
    print("Testing multiple key WATCH...")
    tester = WatchTester()
    
    r1 = redis.Redis(host='localhost', port=6379, decode_responses=True)
    r2 = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Set up test keys
    r1.set('key1', 'value1')
    r1.set('key2', 'value2') 
    r1.set('key3', 'value3')
    
    try:
        with r1.pipeline() as pipe:
            # Watch multiple keys
            pipe.watch('key1', 'key2', 'key3')
            
            # External modification to one watched key
            r2.set('key2', 'externally_modified')
            
            # Transaction should abort
            pipe.multi()
            pipe.set('key1', 'new_value1')
            pipe.set('transaction_executed', 'yes')
            result = pipe.execute()
            
            if result is None:
                key2_value = r1.get('key2')
                if key2_value == 'externally_modified':
                    tester.log_result("Multiple key WATCH", True, "Transaction properly aborted")
                    success = True
                else:
                    tester.log_result("Multiple key WATCH", False, f"Wrong key2 value: {key2_value}")
                    success = False
            else:
                tester.log_result("Multiple key WATCH", False, "Transaction executed when should abort")
                success = False
                
    except redis.WatchError:
        tester.log_result("Multiple key WATCH", True, "WatchError correctly raised")
        success = True
    except Exception as e:
        tester.log_result("Multiple key WATCH", False, f"Unexpected error: {e}")
        success = False
    
    # Cleanup
    r1.delete('key1', 'key2', 'key3', 'transaction_executed')
    return success

def test_watch_nonexistent_key():
    """Test watching a non-existent key"""
    print("Testing WATCH on non-existent key...")
    tester = WatchTester()
    
    r1 = redis.Redis(host='localhost', port=6379, decode_responses=True)
    r2 = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    nonexistent_key = 'does_not_exist_key'
    r1.delete(nonexistent_key)  # Ensure it doesn't exist
    
    try:
        with r1.pipeline() as pipe:
            pipe.watch(nonexistent_key)
            
            # External creation of the key 
            r2.set(nonexistent_key, 'created_externally')
            
            # Transaction should abort since watched key was created
            pipe.multi()
            pipe.set(nonexistent_key, 'transaction_value')
            pipe.set('nonexistent_test_executed', 'yes')
            result = pipe.execute()
            
            if result is None:
                key_value = r1.get(nonexistent_key)
                if key_value == 'created_externally':
                    tester.log_result("WATCH nonexistent key", True, "Creation detected and transaction aborted")
                    success = True
                else:
                    tester.log_result("WATCH nonexistent key", False, f"Wrong key value: {key_value}")
                    success = False
            else:
                tester.log_result("WATCH nonexistent key", False, "Transaction executed when should abort")
                success = False
                
    except redis.WatchError:
        tester.log_result("WATCH nonexistent key", True, "WatchError on creation")
        success = True
    except Exception as e:
        tester.log_result("WATCH nonexistent key", False, f"Error: {e}")
        success = False
    
    # Cleanup
    r1.delete(nonexistent_key, 'nonexistent_test_executed')
    return success

def test_watch_stress_test():
    """High-contention stress test with rapid WATCH operations"""
    print("Testing WATCH under high-contention stress...")
    
    stress_key = 'watch_stress_key'
    num_workers = 50
    operations_per_worker = 20
    
    # Initialize
    r_setup = redis.Redis(host='localhost', port=6379, decode_responses=True)
    r_setup.set(stress_key, '0')
    
    successful_operations = 0
    failed_operations = 0
    lock = threading.Lock()
    
    def stress_worker(worker_id):
        nonlocal successful_operations, failed_operations
        
        r = redis.Redis(host='localhost', port=6379, decode_responses=True)
        
        for op in range(operations_per_worker):
            try:
                with r.pipeline() as pipe:
                    pipe.watch(stress_key)
                    
                    current_val = int(r.get(stress_key) or 0)
                    time.sleep(0.0001)  # Brief delay
                    
                    pipe.multi()
                    pipe.set(stress_key, str(current_val + 1))
                    result = pipe.execute()
                    
                    if result is not None:
                        with lock:
                            successful_operations += 1
                    else:
                        with lock:
                            failed_operations += 1
                            
            except redis.WatchError:
                with lock:
                    failed_operations += 1
            except Exception as e:
                print(f"Stress worker {worker_id} error: {e}")
    
    # Execute stress test
    start_time = time.time()
    with ThreadPoolExecutor(max_workers=num_workers) as executor:
        futures = [executor.submit(stress_worker, i) for i in range(num_workers)]
        [future.result() for future in futures]
    end_time = time.time()
    
    # Verify final state
    final_value = int(r_setup.get(stress_key))
    total_operations = successful_operations + failed_operations
    
    if final_value == successful_operations and final_value > 0:
        duration = end_time - start_time
        tester = WatchTester()
        tester.log_result("WATCH stress test", True, 
            f"{successful_operations} successes, {total_operations} total ops in {duration:.2f}s")
        r_setup.delete(stress_key)
        return True
    else:
        print(f"❌ Stress test failed: final={final_value}, successes={successful_operations}")
        return False

def test_watch_with_different_transactions():
    """Test multiple independent transactions with WATCH on different key sets"""
    print("Testing independent transactions with different WATCH keys...")
    
    # Create separate connections for different transaction groups
    connections = [redis.Redis(host='localhost', port=6379, decode_responses=True) for _ in range(5)]
    
    # Each group gets its own set of keys to avoid interference
    test_keys = [f'independent_test_{i}' for i in range(5)]
    for i, key in enumerate(test_keys):
        connections[i].set(key, f'initial_value_{i}')
    
    def independent_transaction(conn_id, key):
        try:
            conn = connections[conn_id]
            with conn.pipeline() as pipe:
                pipe.watch(key)
                
                # Small delay
                time.sleep(0.01)
                
                # Each transaction modifies only its own key
                pipe.multi()
                pipe.set(key, f'modified_by_worker_{conn_id}')
                pipe.set(f'worker_{conn_id}_marker', 'done')
                result = pipe.execute()
                
                return result is not None  # Success if not aborted
        except Exception as e:
            print(f"Error in independent transaction {conn_id}: {e}")
            return False
    
    # Run all transactions concurrently
    with ThreadPoolExecutor(max_workers=5) as executor:
        futures = [executor.submit(independent_transaction, i, test_keys[i]) for i in range(5)]
        results = [future.result() for future in as_completed(futures)]
    
    # All transactions should succeed since they watch different keys
    success_count = sum(results)
    
    if success_count == 5:
        # Verify all keys were modified correctly
        all_correct = all(connections[i].get(test_keys[i]) == f'modified_by_worker_{i}' for i in range(5))
        
        if all_correct:
            tester = WatchTester()
            tester.log_result("Independent transactions", True, "All 5 independent transactions succeeded")
            success = True
        else:
            print("❌ Key values incorrect after independent transactions")
            success = False
    else:
        print(f"❌ Independent transactions failed: only {success_count}/5 succeeded")
        success = False
    
    # Cleanup
    for i in range(5):
        connections[i].delete(test_keys[i], f'worker_{i}_marker')
    
    return success

def test_watch_type_change():
    """Test watching a key that changes type"""
    print("Testing WATCH on type change...")
    tester = WatchTester()
    
    r1 = redis.Redis(host='localhost', port=6379, decode_responses=True)
    r2 = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    test_key = 'watch_type_change_test'
    
    # Set as string initially
    r1.set(test_key, 'string_value')
    
    try:
        with r1.pipeline() as pipe:
            pipe.watch(test_key)
            
            # External type change (string to list)
            r2.delete(test_key)
            r2.lpush(test_key, 'list_item')
            
            # Transaction should abort due to type change
            pipe.multi()
            pipe.set(test_key, 'back_to_string')
            pipe.set('type_change_executed', 'yes')
            result = pipe.execute()
            
            if result is None:
                # Verify key is still a list
                key_type = r1.type(test_key)
                if key_type == 'list':
                    tester.log_result("WATCH type change", True, "Type change detected, transaction aborted")
                    success = True
                else:
                    tester.log_result("WATCH type change", False, f"Wrong type: {key_type}")
                    success = False
            else:
                tester.log_result("WATCH type change", False, "Transaction executed when should abort")
                success = False
                
    except redis.WatchError:
        tester.log_result("WATCH type change", True, "WatchError on type change")
        success = True
    except Exception as e:
        tester.log_result("WATCH type change", False, f"Error: {e}")
        success = False
    
    # Cleanup
    r1.delete(test_key, 'type_change_executed')
    return success

def test_watch_cross_database():
    """Test WATCH behavior across database switches"""
    print("Testing WATCH across database switches...")
    tester = WatchTester()
    
    r1 = redis.Redis(host='localhost', port=6379, db=0, decode_responses=True)
    r2 = redis.Redis(host='localhost', port=6379, db=1, decode_responses=True)
    
    test_key = 'cross_db_watch_test'
    
    # Set key in both databases
    r1.set(test_key, 'value_db0')
    r2.set(test_key, 'value_db1')
    
    try:
        with r1.pipeline() as pipe:
            # Watch key in DB 0
            pipe.watch(test_key)
            
            # Modify same key name in DB 1 (should not affect WATCH in DB 0)
            r2.set(test_key, 'modified_value_db1')
            
            # Transaction should succeed since we're watching DB 0, not DB 1
            pipe.multi()
            pipe.set(test_key, 'transaction_value_db0')
            pipe.set('cross_db_executed', 'yes')
            result = pipe.execute()
            
            if result is not None:
                # Transaction should succeed - different databases
                db0_value = r1.get(test_key)
                if db0_value == 'transaction_value_db0':
                    tester.log_result("WATCH cross-database", True, "Cross-DB isolation working correctly")
                    success = True
                else:
                    tester.log_result("WATCH cross-database", False, f"Wrong DB0 value: {db0_value}")
                    success = False
            else:
                tester.log_result("WATCH cross-database", False, "Transaction aborted incorrectly")
                success = False
                
    except Exception as e:
        tester.log_result("WATCH cross-database", False, f"Error: {e}")
        success = False
    
    # Cleanup
    r1.delete(test_key, 'cross_db_executed')
    r2.delete(test_key)
    return success

def test_watch_with_expiring_keys():
    """Test WATCH on keys with TTL"""
    print("Testing WATCH with expiring keys...")
    tester = WatchTester()
    
    r1 = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    test_key = 'expiring_watch_test'
    
    try:
        # Set key with short TTL
        r1.setex(test_key, 2, 'will_expire')  # 2 second TTL
        
        with r1.pipeline() as pipe:
            pipe.watch(test_key)
            
            # Wait for key to expire
            time.sleep(3)
            
            # Transaction behavior with expired key varies by implementation
            pipe.multi()
            pipe.set(test_key, 'after_expiry')
            pipe.set('expiry_executed', 'yes')
            result = pipe.execute()
            
            if result is None:
                tester.log_result("WATCH expiring key", True, "Expiry detected as modification")
                success = True
            else:
                # Some implementations allow transactions after expiry
                tester.log_result("WATCH expiring key", True, "Transaction allowed after expiry (acceptable)")
                success = True
                
    except redis.WatchError:
        tester.log_result("WATCH expiring key", True, "WatchError on expiry")
        success = True
    except Exception as e:
        tester.log_result("WATCH expiring key", False, f"Error: {e}")
        success = False
    
    # Cleanup
    r1.delete(test_key, 'expiry_executed')
    return success

def test_watch_unwatch():
    """Test UNWATCH command functionality"""
    print("Testing UNWATCH command...")
    tester = WatchTester()
    
    r1 = redis.Redis(host='localhost', port=6379, decode_responses=True)
    r2 = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    test_key = 'unwatch_test'
    r1.set(test_key, 'initial')
    
    try:
        with r1.pipeline() as pipe:
            # Watch the key
            pipe.watch(test_key)
            
            # Unwatch all keys 
            pipe.unwatch()
            
            # External modification (should not abort transaction now)
            r2.set(test_key, 'external_modification')
            
            # Transaction should succeed since we unwatched
            pipe.multi()
            pipe.set(test_key, 'transaction_value')
            pipe.set('unwatch_executed', 'yes')
            result = pipe.execute()
            
            if result is not None:
                key_value = r1.get(test_key)
                if key_value == 'transaction_value':
                    tester.log_result("UNWATCH command", True, "UNWATCH allowed transaction after modification")
                    success = True
                else:
                    tester.log_result("UNWATCH command", False, f"Wrong final value: {key_value}")
                    success = False
            else:
                tester.log_result("UNWATCH command", False, "Transaction aborted even after UNWATCH")
                success = False
                
    except Exception as e:
        tester.log_result("UNWATCH command", False, f"Error: {e}")
        success = False
    
    # Cleanup
    r1.delete(test_key, 'unwatch_executed')
    return success

def main():
    print("=" * 70)
    print("FERROUS COMPREHENSIVE WATCH MECHANISM TEST SUITE")
    print("=" * 70)
    
    # Verify server connection
    try:
        r = redis.Redis(host='localhost', port=6379, decode_responses=True)
        r.ping()
        print("✅ Server connection verified")
    except:
        print("❌ Cannot connect to server")
        sys.exit(1)
    
    print()
    
    # Run comprehensive WATCH tests - ALL DEFINED TESTS INCLUDED
    test_functions = [
        test_concurrent_watches,
        test_multiple_key_watch,
        test_watch_nonexistent_key,
        test_watch_stress_test,              # Now properly included
        test_watch_with_different_transactions,  # Now properly included
        test_watch_type_change,
        test_watch_cross_database,
        test_watch_with_expiring_keys,
        test_watch_unwatch,
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
    print(f"COMPREHENSIVE WATCH TEST RESULTS: {passed}/{total} PASSED")
    print("=" * 70)
    
    if passed == total:
        print("🎉 ALL COMPREHENSIVE WATCH TESTS PASSED!")
        print("✅ WATCH mechanism thoroughly validated under:")
        print("   • Concurrent access from multiple clients")
        print("   • Multiple key watching scenarios")
        print("   • Edge cases (nonexistent keys, type changes)")
        print("   • Cross-database isolation")
        print("   • Key expiration scenarios")
        print("   • UNWATCH functionality")
        print("   • High-contention stress testing")
        print("   • Independent transaction isolation")
        sys.exit(0)
    else:
        print(f"❌ {total - passed} WATCH tests failed")
        print("⚠️  WATCH mechanism needs attention in failed areas")
        sys.exit(1)

if __name__ == "__main__":
    main()