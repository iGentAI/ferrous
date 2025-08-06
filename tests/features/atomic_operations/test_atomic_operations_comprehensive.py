#!/usr/bin/env python3
"""
Comprehensive atomic operations test suite for Ferrous
Tests all conditional and atomic operations with timeout detection
"""

import redis
import time
import sys
import threading
from concurrent.futures import ThreadPoolExecutor, TimeoutError as FuturesTimeoutError

class AtomicOperationsTester:
    def __init__(self, host='127.0.0.1', port=6379):
        self.host = host
        self.port = port
        self.r = redis.Redis(host=host, port=port, decode_responses=True, socket_timeout=5)
        
    def test_set_nx_operations(self):
        """Test SET with NX option comprehensively"""
        print("Testing SET NX operations...")
        
        try:
            # Clean up
            self.r.delete("test_nx_key")
            
            # Test 1: SET NX on non-existent key (should succeed)
            result = self.r.set("test_nx_key", "value1", nx=True)
            if result != True:
                print(f"❌ SET NX on new key failed: {result}")
                return False
            
            # Test 2: SET NX on existing key (should fail gracefully, not hang)
            start_time = time.time()
            result = self.r.set("test_nx_key", "value2", nx=True)
            elapsed = time.time() - start_time
            
            if result is not None:
                print(f"❌ SET NX on existing key should return None, got: {result}")
                return False
                
            if elapsed > 1.0:
                print(f"❌ SET NX took too long ({elapsed:.2f}s), possible hanging issue")
                return False
                
            # Test 3: Verify original value unchanged
            value = self.r.get("test_nx_key")
            if value != "value1":
                print(f"❌ Original value modified by failed SET NX: {value}")
                return False
                
            print("✅ SET NX operations working correctly")
            return True
            
        except Exception as e:
            print(f"❌ SET NX test failed: {e}")
            return False
            
    def test_set_xx_operations(self):
        """Test SET with XX option"""
        print("\nTesting SET XX operations...")
        
        try:
            # Clean up
            self.r.delete("test_xx_key")
            
            # Test 1: SET XX on non-existent key (should fail)
            start_time = time.time()
            result = self.r.set("test_xx_key", "value1", xx=True)
            elapsed = time.time() - start_time
            
            if result is not None:
                print(f"❌ SET XX on non-existent key should return None, got: {result}")
                return False
                
            if elapsed > 1.0:
                print(f"❌ SET XX took too long ({elapsed:.2f}s), possible hanging issue")
                return False
            
            # Test 2: SET normal value first
            self.r.set("test_xx_key", "original")
            
            # Test 3: SET XX on existing key (should succeed)
            result = self.r.set("test_xx_key", "updated", xx=True)
            if result != True:
                print(f"❌ SET XX on existing key failed: {result}")
                return False
                
            value = self.r.get("test_xx_key")
            if value != "updated":
                print(f"❌ SET XX didn't update value: {value}")
                return False
                
            print("✅ SET XX operations working correctly")
            return True
            
        except Exception as e:
            print(f"❌ SET XX test failed: {e}")
            return False
            
    def test_set_ex_operations(self):
        """Test SET with expiration options"""
        print("\nTesting SET EX/PX operations...")
        
        try:
            # Test SET EX (seconds)
            start_time = time.time()
            result = self.r.set("test_ex", "value", ex=1)
            elapsed = time.time() - start_time
            
            if result != True:
                print(f"❌ SET EX failed: {result}")
                return False
                
            if elapsed > 0.5:
                print(f"❌ SET EX took too long ({elapsed:.2f}s)")
                return False
                
            # Test SET PX (milliseconds)
            result = self.r.set("test_px", "value", px=1000)
            if result != True:
                print(f"❌ SET PX failed: {result}")
                return False
                
            print("✅ SET expiration options working correctly")
            return True
            
        except Exception as e:
            print(f"❌ SET expiration test failed: {e}")
            return False
            
    def test_conditional_operations_stress(self):
        """Stress test conditional operations with concurrent clients"""
        print("\nTesting conditional operations under stress...")
        
        def worker_set_nx(thread_id):
            try:
                r = redis.Redis(host=self.host, port=self.port, decode_responses=True, socket_timeout=2)
                
                # Each thread tries to SET NX, only one should succeed
                key = f"stress_nx_key"
                value = f"thread_{thread_id}_value"
                
                start_time = time.time()
                result = r.set(key, value, nx=True)
                elapsed = time.time() - start_time
                
                if elapsed > 1.0:
                    return False, f"Thread {thread_id}: SET NX took too long ({elapsed:.2f}s)"
                    
                return True, f"Thread {thread_id}: {result}"
                
            except Exception as e:
                return False, f"Thread {thread_id}: Exception - {e}"
        
        try:
            # Clean up
            self.r.delete("stress_nx_key")
            
            # Run 5 concurrent SET NX operations
            with ThreadPoolExecutor(max_workers=5) as executor:
                futures = [executor.submit(worker_set_nx, i) for i in range(5)]
                
                success_count = 0
                for future in futures:
                    try:
                        success, message = future.result(timeout=5.0)
                        if success:
                            success_count += 1
                        # Don't print individual thread results to keep output clean
                    except FuturesTimeoutError:
                        print(f"❌ Stress test thread timed out - hanging detected")
                        return False
                    except Exception as e:
                        print(f"❌ Stress test thread failed: {e}")
                        return False
                        
            if success_count != 5:  # All threads should complete, but only one should set the key
                print(f"❌ Stress test: {success_count}/5 threads completed successfully")
                return False
                
            print("✅ Conditional operations stress test passed")
            return True
            
        except Exception as e:
            print(f"❌ Stress test failed: {e}")
            return False
            
    def test_atomic_increment_operations(self):
        """Test atomic increment operations for comparison"""
        print("\nTesting atomic increment operations...")
        
        try:
            # Clean up
            self.r.delete("test_incr")
            
            # Test INCR on non-existent key
            start_time = time.time()
            result = self.r.incr("test_incr")
            elapsed = time.time() - start_time
            
            if result != 1:
                print(f"❌ INCR on new key failed: {result}")
                return False
                
            if elapsed > 0.5:
                print(f"❌ INCR took too long ({elapsed:.2f}s)")
                return False
                
            # Test INCRBY
            result = self.r.incrby("test_incr", 5)
            if result != 6:
                print(f"❌ INCRBY failed: {result}")
                return False
                
            print("✅ Atomic increment operations working correctly")
            return True
            
        except Exception as e:
            print(f"❌ Atomic increment test failed: {e}")
            return False
            
    def test_watch_mechanism(self):
        """Test WATCH mechanism for transaction isolation"""
        print("\nTesting WATCH mechanism...")
        
        try:
            # Clean up
            self.r.delete("watch_key")
            self.r.set("watch_key", "initial")
            
            # Start watching
            pipe = self.r.pipeline()
            pipe.watch("watch_key")
            pipe.multi()
            pipe.set("watch_key", "updated") 
            
            # Modify key externally to trigger WATCH violation
            r2 = redis.Redis(host=self.host, port=self.port, decode_responses=True)
            r2.set("watch_key", "external_change")
            
            # Execute transaction (should fail due to WATCH violation)
            start_time = time.time()
            
            try:
                result = pipe.execute()
                # If we get here without exception, WATCH violation wasn't detected
                print(f"❌ WATCH violation not detected: {result}")
                return False
            except redis.exceptions.WatchError:
                # This is the CORRECT behavior for WATCH violations
                elapsed = time.time() - start_time
                if elapsed > 0.5:
                    print(f"❌ WATCH transaction took too long ({elapsed:.2f}s)")
                    return False
                print("✅ WATCH mechanism working correctly")
                return True
            
        except Exception as e:
            print(f"❌ WATCH test failed: {e}")
            return False
            
    def test_null_response_semantics(self):
        """Test that null responses are handled consistently"""
        print("\nTesting null response semantics...")
        
        try:
            # Test various commands that return null
            null_tests = [
                ("GET non-existent", lambda: self.r.get("nonexistent_key_12345")),
                ("LPOP empty list", lambda: self.r.lpop("empty_list_12345")),
                ("RPOP empty list", lambda: self.r.rpop("empty_list_12345")),
                ("HGET non-existent", lambda: self.r.hget("nonexistent_hash", "field")),
            ]
            
            for test_name, test_func in null_tests:
                start_time = time.time()
                result = test_func()
                elapsed = time.time() - start_time
                
                if result is not None:
                    print(f"❌ {test_name} should return None, got: {result}")
                    return False
                    
                if elapsed > 0.5:
                    print(f"❌ {test_name} took too long ({elapsed:.2f}s)")
                    return False
            
            print("✅ Null response semantics working correctly")
            return True
            
        except Exception as e:
            print(f"❌ Null response test failed: {e}")
            return False

def main():
    print("=" * 70)
    print("FERROUS ATOMIC OPERATIONS COMPREHENSIVE TEST SUITE")
    print("=" * 70)
    
    # Check if server is running
    try:
        r = redis.Redis(host='127.0.0.1', port=6379, socket_timeout=1)
        r.ping()
        print("✅ Server connection verified")
    except:
        print("❌ Cannot connect to server")
        sys.exit(1)
        
    print()
    
    tester = AtomicOperationsTester()
    
    # Run all tests
    results = []
    results.append(tester.test_set_nx_operations())
    results.append(tester.test_set_xx_operations())
    results.append(tester.test_set_ex_operations())
    results.append(tester.test_conditional_operations_stress())
    results.append(tester.test_atomic_increment_operations())
    results.append(tester.test_watch_mechanism())
    results.append(tester.test_null_response_semantics())
    
    # Summary
    passed = sum(results)
    total = len(results)
    
    print()
    print("=" * 70)
    print(f"ATOMIC OPERATIONS TEST RESULTS: {passed}/{total} PASSED")
    print("=" * 70)
    
    if passed == total:
        print("🎉 All atomic operations tests passed!")
        print("✅ SET NX hanging bug prevention: WORKING")
        print("✅ Response handling regression prevention: WORKING")
        sys.exit(0)
    else:
        print("❌ Some atomic operations tests failed")
        print("⚠️  Critical atomic operation bugs may still exist")
        sys.exit(1)

if __name__ == "__main__":
    main()