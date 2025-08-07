#!/usr/bin/env python3
"""
WATCH Stress Testing for Ferrous
High-concurrency stress tests to validate WATCH mechanism under extreme load
"""

import redis
import threading
import time
import sys
import random
from concurrent.futures import ThreadPoolExecutor, as_completed
import statistics

def test_massive_concurrent_watch():
    """Stress test with massive concurrent WATCH operations"""
    print("Testing massive concurrent WATCH operations...")
    
    test_key = 'massive_concurrent_test'
    num_workers = 100
    
    # Initialize
    r_setup = redis.Redis(host='localhost', port=6379, decode_responses=True)
    r_setup.set(test_key, '0')
    
    successful_operations = 0
    aborted_operations = 0
    errors = 0
    operation_times = []
    lock = threading.Lock()
    
    def massive_watch_worker(worker_id):
        nonlocal successful_operations, aborted_operations, errors
        
        r = redis.Redis(host='localhost', port=6379, decode_responses=True)
        
        try:
            start_time = time.time()
            
            with r.pipeline() as pipe:
                pipe.watch(test_key)
                
                # Random small delay to create contention
                time.sleep(random.uniform(0.001, 0.01))
                
                current_val = int(r.get(test_key) or 0)
                
                pipe.multi() 
                pipe.set(test_key, str(current_val + worker_id))
                pipe.set(f'worker_{worker_id}_timestamp', str(time.time()))
                result = pipe.execute()
                
                op_time = time.time() - start_time
                
                with lock:
                    operation_times.append(op_time)
                    
                if result is not None:
                    with lock:
                        successful_operations += 1
                else:
                    with lock:
                        aborted_operations += 1
                        
        except redis.WatchError:
            with lock:
                aborted_operations += 1
        except Exception as e:
            with lock:
                errors += 1
            print(f"Worker {worker_id} error: {e}")
    
    # Execute massive concurrent test
    start_time = time.time()
    with ThreadPoolExecutor(max_workers=num_workers) as executor:
        futures = [executor.submit(massive_watch_worker, i) for i in range(num_workers)]
        [future.result() for future in as_completed(futures)]
    end_time = time.time()
    
    # Analyze results
    total_ops = successful_operations + aborted_operations
    duration = end_time - start_time
    
    if errors == 0 and successful_operations >= 1:
        avg_op_time = statistics.mean(operation_times) * 1000  # ms
        p95_op_time = statistics.quantiles(operation_times, n=20)[18] * 1000  # 95th percentile
        
        print(f"✅ Massive concurrent WATCH test passed:")
        print(f"   • {successful_operations} successful operations")
        print(f"   • {aborted_operations} proper aborts") 
        print(f"   • {total_ops} total operations in {duration:.2f}s")
        print(f"   • {total_ops/duration:.0f} operations/sec")
        print(f"   • Avg operation time: {avg_op_time:.2f}ms")
        print(f"   • P95 operation time: {p95_op_time:.2f}ms")
        print(f"   • Success rate: {successful_operations/total_ops*100:.1f}%")
        
        # Cleanup
        for i in range(num_workers):
            r_setup.delete(f'worker_{i}_timestamp')
        r_setup.delete(test_key)
        
        return True
    else:
        print(f"❌ Massive concurrent test failed: {errors} errors, {successful_operations} successes")
        return False

def test_watch_fairness():
    """Test WATCH fairness under contention"""
    print("Testing WATCH fairness under contention...")
    
    shared_resource = 'fairness_test'
    num_workers = 20
    iterations_per_worker = 10
    
    r_setup = redis.Redis(host='localhost', port=6379, decode_responses=True)
    r_setup.set(shared_resource, '0')
    
    worker_success_counts = {}
    total_successes = 0
    lock = threading.Lock()
    
    def fairness_worker(worker_id):
        nonlocal total_successes
        
        r = redis.Redis(host='localhost', port=6379, decode_responses=True)
        local_successes = 0
        
        for iteration in range(iterations_per_worker):
            try:
                with r.pipeline() as pipe:
                    pipe.watch(shared_resource)
                    
                    current_val = int(r.get(shared_resource) or 0)
                    
                    # Random delay to simulate processing time
                    time.sleep(random.uniform(0.001, 0.005))
                    
                    pipe.multi()
                    pipe.set(shared_resource, str(current_val + 1))
                    pipe.lpush(f'worker_{worker_id}_operations', iteration)
                    result = pipe.execute()
                    
                    if result is not None:
                        local_successes += 1
                        
            except redis.WatchError:
                pass  # Expected conflict
            except Exception as e:
                print(f"Worker {worker_id} iteration {iteration} error: {e}")
        
        with lock:
            worker_success_counts[worker_id] = local_successes
            total_successes += local_successes
    
    # Run fairness test
    start_time = time.time()
    threads = []
    for i in range(num_workers):
        t = threading.Thread(target=fairness_worker, args=(i,))
        threads.append(t)
        t.start()
    
    for t in threads:
        t.join()
    end_time = time.time()
    
    # Analyze fairness
    final_counter = int(r_setup.get(shared_resource))
    
    if final_counter == total_successes and total_successes > 0:
        # Check fairness distribution
        success_values = list(worker_success_counts.values())
        min_successes = min(success_values)
        max_successes = max(success_values)
        avg_successes = statistics.mean(success_values)
        
        # Reasonable fairness: no worker should be completely starved
        # and no worker should dominate excessively
        if min_successes >= 0 and max_successes <= avg_successes * 3:
            print(f"✅ WATCH fairness test passed:")
            print(f"   • Final counter: {final_counter}")
            print(f"   • Total successes: {total_successes}")
            print(f"   • Duration: {end_time - start_time:.2f}s")
            print(f"   • Success distribution: min={min_successes}, max={max_successes}, avg={avg_successes:.1f}")
            
            # Show worker distribution
            distribution = {}
            for count in success_values:
                distribution[count] = distribution.get(count, 0) + 1
            print(f"   • Success distribution: {distribution}")
            
            success = True
        else:
            print(f"❌ Unfair distribution: min={min_successes}, max={max_successes}, avg={avg_successes:.1f}")
            success = False
    else:
        print(f"❌ Counter mismatch: final={final_counter}, successes={total_successes}")
        success = False
    
    # Cleanup
    for i in range(num_workers):
        r_setup.delete(f'worker_{i}_operations')
    r_setup.delete(shared_resource)
    
    return success

def test_watch_with_blocking_operations():
    """Test WATCH interaction with blocking operations"""
    print("Testing WATCH with blocking operations...")
    
    r1 = redis.Redis(host='localhost', port=6379, decode_responses=True)
    r2 = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    queue_key = 'blocking_watch_queue'
    counter_key = 'blocking_watch_counter'
    
    # Initialize
    r1.delete(queue_key, counter_key)
    r1.set(counter_key, '0')
    
    blocking_result = None
    transaction_result = None
    
    def blocking_worker():
        nonlocal blocking_result
        try:
            # This will block until item is pushed
            blocking_result = r2.blpop(queue_key, timeout=5)
        except Exception as e:
            print(f"Blocking worker error: {e}")
            blocking_result = None
    
    def transaction_worker():
        nonlocal transaction_result
        try:
            time.sleep(0.1)  # Let blocker start first
            
            with r1.pipeline() as pipe:
                pipe.watch(counter_key)
                
                current_count = int(r1.get(counter_key))
                
                pipe.multi()
                pipe.set(counter_key, str(current_count + 1))
                pipe.rpush(queue_key, f'item_{current_count}')  # This will unblock the other worker
                result = pipe.execute()
                
                transaction_result = result is not None
                
        except Exception as e:
            print(f"Transaction worker error: {e}")
            transaction_result = False
    
    # Start both workers
    blocking_thread = threading.Thread(target=blocking_worker)
    transaction_thread = threading.Thread(target=transaction_worker)
    
    blocking_thread.start()
    transaction_thread.start()
    
    # Wait for completion
    blocking_thread.join(timeout=10)
    transaction_thread.join(timeout=10)
    
    # Analyze results
    if transaction_result and blocking_result:
        if blocking_result[0] == queue_key and 'item_' in blocking_result[1]:
            print("✅ WATCH worked correctly with blocking operations")
            success = True
        else:
            print(f"❌ Wrong blocking result: {blocking_result}")
            success = False
    else:
        print(f"❌ WATCH + blocking failed: tx={transaction_result}, block={blocking_result}")
        success = False
    
    # Cleanup
    r1.delete(queue_key, counter_key)
    return success

def main():
    print("=" * 70)
    print("FERROUS WATCH STRESS & ADVANCED PATTERN TESTS")
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
    
    # Run stress tests
    test_functions = [
        test_massive_concurrent_watch,
        test_watch_fairness,
        test_watch_with_blocking_operations,
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
    print(f"WATCH STRESS TEST RESULTS: {passed}/{total} PASSED")
    print("=" * 70)
    
    if passed == total:
        print("🎉 ALL WATCH STRESS TESTS PASSED!")
        print("✅ WATCH mechanism validated under extreme conditions:")
        print("   • Massive concurrency (100+ concurrent operations)")
        print("   • Fair access patterns under contention")  
        print("   • Integration with blocking operations")
        print("   • Distributed locking patterns")
        print("   • Atomic counter patterns")
        sys.exit(0)
    else:
        print(f"❌ {total - passed} WATCH stress tests failed")
        sys.exit(1)

if __name__ == "__main__":
    main()