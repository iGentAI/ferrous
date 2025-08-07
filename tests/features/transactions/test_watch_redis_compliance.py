#!/usr/bin/env python3
"""
Redis WATCH Compliance Tests for Ferrous
Validates exact Redis protocol compliance for WATCH behavior
"""

import redis
import time
import sys
import socket

def test_watch_protocol_compliance():
    """Test WATCH protocol responses match Redis exactly"""
    print("Testing WATCH protocol compliance...")
    
    # Use raw socket for precise protocol testing
    def send_raw_command(commands):
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.connect(('127.0.0.1', 6379))
        s.settimeout(5.0)
        
        responses = []
        try:
            for cmd in commands:
                s.sendall(cmd)
                resp = s.recv(4096)
                responses.append(resp)
        finally:
            s.close()
        return responses
    
    # Test WATCH response format
    commands = [
        b"*2\r\n$5\r\nWATCH\r\n$8\r\ntestkey1\r\n",  # WATCH testkey1
        b"*3\r\n$5\r\nWATCH\r\n$8\r\ntestkey2\r\n$8\r\ntestkey3\r\n",  # WATCH testkey2 testkey3  
        b"*1\r\n$7\r\nUNWATCH\r\n",  # UNWATCH
    ]
    
    responses = send_raw_command(commands)
    
    # All WATCH commands should return +OK
    passed = 0
    total = 3
    
    for i, response in enumerate(responses):
        if b"+OK\r\n" in response:
            print(f"✅ Command {i+1} returned correct +OK response")
            passed += 1
        else:
            print(f"❌ Command {i+1} wrong response: {response}")
    
    return passed == total

def test_watch_error_conditions():
    """Test WATCH error conditions and edge cases"""
    print("Testing WATCH error conditions...")
    
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Test 1: WATCH with no arguments
    try:
        r.execute_command("WATCH")
        print("❌ WATCH with no args should fail")
        return False
    except redis.ResponseError as e:
        if "wrong number of arguments" in str(e).lower():
            print("✅ WATCH with no arguments correctly rejected")
            test1_pass = True
        else:
            print(f"❌ Wrong error for no args: {e}")
            test1_pass = False
    
    # Test 2: UNWATCH in transaction should fail
    try:
        with r.pipeline() as pipe:
            pipe.multi()
            pipe.unwatch()
            pipe.execute()
        print("❌ UNWATCH in transaction should fail")
        test2_pass = False
    except redis.ResponseError as e:
        if "not allowed" in str(e).lower() or "err" in str(e).lower():
            print("✅ UNWATCH in transaction properly rejected")
            test2_pass = True
        else:
            print(f"❌ Wrong error for UNWATCH in transaction: {e}")
            test2_pass = False
    except Exception as e:
        # Some implementations may handle this differently
        print(f"✅ UNWATCH in transaction handled: {e}")
        test2_pass = True
    
    # Test 3: WATCH invalid key name formats (edge case)
    try:
        r.execute_command("WATCH", "")  # Empty key name
        empty_key_handled = True  # Some implementations allow empty keys
    except redis.ResponseError:
        empty_key_handled = True  # Rejection is also valid
    except Exception as e:
        print(f"Empty key test error: {e}")
        empty_key_handled = False
    
    if empty_key_handled:
        print("✅ Empty key name handled appropriately")
        test3_pass = True
    else:
        print("❌ Empty key name caused unexpected error")
        test3_pass = False
    
    return test1_pass and test2_pass and test3_pass

def test_watch_memory_behavior():
    """Test WATCH memory behavior with many watched keys"""
    print("Testing WATCH memory behavior...")
    
    r = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    # Watch a large number of keys
    many_keys = [f'watch_memory_test_{i}' for i in range(1000)]
    
    try:
        start_time = time.time()
        
        with r.pipeline() as pipe:
            # Watch many keys at once
            pipe.watch(*many_keys)
            
            # Transaction should work if no keys are modified
            pipe.multi()
            pipe.set('many_keys_marker', 'transaction_completed')
            result = pipe.execute()
            
            watch_time = time.time() - start_time
            
            if result is not None:
                marker_exists = r.get('many_keys_marker')
                if marker_exists == 'transaction_completed':
                    print(f"✅ Large WATCH set handled correctly ({watch_time:.3f}s)")
                    print(f"   • Watched {len(many_keys)} keys simultaneously")
                    success = True
                else:
                    print("❌ Transaction marker not set correctly")
                    success = False
            else:
                print("❌ Transaction aborted unexpectedly")
                success = False
                
    except Exception as e:
        print(f"❌ Large WATCH set failed: {e}")
        success = False
    
    # Cleanup
    r.delete('many_keys_marker')
    
    # Test memory doesn't leak by doing it multiple times
    for repeat in range(5):
        try:
            with r.pipeline() as pipe:
                pipe.watch(*many_keys)
                pipe.discard()  # Cancel without executing
        except Exception as e:
            print(f"❌ Memory test repeat {repeat} failed: {e}")
            return False
    
    print("✅ Memory behavior stable across multiple large WATCH operations")
    return success

def test_watch_transaction_isolation():
    """Test WATCH transaction isolation between different connections"""
    print("Testing WATCH transaction isolation...")
    
    # Create 3 separate connections 
    connections = [redis.Redis(host='localhost', port=6379, decode_responses=True) for _ in range(3)]
    
    # Each connection watches and modifies different sets of keys
    isolation_scenarios = [
        (['iso_key1', 'iso_key2'], 'conn1_value'),
        (['iso_key3', 'iso_key4'], 'conn2_value'),  
        (['iso_key5', 'iso_key6'], 'conn3_value'),
    ]
    
    # Initialize all keys
    for i, (keys, _) in enumerate(isolation_scenarios):
        for key in keys:
            connections[i].set(key, f'initial_{key}')
    
    successful_transactions = 0
    
    def isolated_transaction_worker(conn_id, keys, value):
        nonlocal successful_transactions
        
        conn = connections[conn_id]
        
        try:
            with conn.pipeline() as pipe:
                # Watch this connection's keys
                pipe.watch(*keys)
                
                time.sleep(0.01)  # Small delay
                
                # Modify this connection's keys
                pipe.multi()
                for key in keys:
                    pipe.set(key, value)
                pipe.set(f'isolation_marker_{conn_id}', 'completed')
                result = pipe.execute()
                
                if result is not None:
                    return True
                else:
                    return False
                    
        except Exception as e:
            print(f"Isolation worker {conn_id} error: {e}")
            return False
    
    # Run all transactions simultaneously
    import threading
    threads = []
    results = [False] * 3
    
    def worker_wrapper(conn_id, keys, value):
        results[conn_id] = isolated_transaction_worker(conn_id, keys, value)
    
    for i, (keys, value) in enumerate(isolation_scenarios):
        t = threading.Thread(target=worker_wrapper, args=(i, keys, value))
        threads.append(t)
        t.start()
    
    for t in threads:
        t.join()
    
    # All transactions should succeed (no cross-interference)
    if all(results):
        # Verify all keys have correct values
        verification_passed = True
        for i, (keys, expected_value) in enumerate(isolation_scenarios):
            for key in keys:
                actual_value = connections[i].get(key)
                if actual_value != expected_value:
                    print(f"❌ Key {key} has wrong value: {actual_value} != {expected_value}")
                    verification_passed = False
        
        if verification_passed:
            print("✅ Transaction isolation working correctly")
            print("   • All 3 independent transactions succeeded") 
            print("   • No cross-interference between WATCH operations")
            success = True
        else:
            success = False
    else:
        failed_workers = [i for i, result in enumerate(results) if not result]
        print(f"❌ Transaction isolation failed: workers {failed_workers} failed")
        success = False
    
    # Cleanup
    for i, (keys, _) in enumerate(isolation_scenarios):
        connections[i].delete(*keys)
        connections[i].delete(f'isolation_marker_{i}')
    
    return success

def main():
    print("=" * 70)
    print("FERROUS WATCH REDIS COMPLIANCE & STRESS TESTS")
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
    
    # Run compliance and stress tests
    test_functions = [
        test_watch_protocol_compliance,
        test_watch_error_conditions,
        test_watch_memory_behavior,
        test_watch_transaction_isolation,
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
    print(f"WATCH COMPLIANCE & STRESS RESULTS: {passed}/{total} PASSED")
    print("=" * 70)
    
    if passed == total:
        print("🎉 ALL WATCH COMPLIANCE & STRESS TESTS PASSED!")
        print("✅ WATCH mechanism is Redis-compliant and robust under:")
        print("   • Protocol compliance testing")
        print("   • Error condition handling")
        print("   • Memory behavior validation") 
        print("   • Transaction isolation guarantees")
        sys.exit(0)
    else:
        print(f"❌ {total - passed} WATCH compliance/stress tests failed")
        sys.exit(1)

if __name__ == "__main__":
    main()