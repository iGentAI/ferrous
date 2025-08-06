#!/usr/bin/env python3
"""
Regression prevention test suite for Ferrous
Specifically tests for merge failure artifacts and critical bugs
"""

import redis
import time
import sys
import socket

class RegressionTester:
    def __init__(self, host='127.0.0.1', port=6379):
        self.host = host
        self.port = port
        self.r = redis.Redis(host=host, port=port, decode_responses=True, socket_timeout=3)
        
    def test_response_handling_regressions(self):
        """Test for response handling bugs that cause hanging"""
        print("Testing response handling regressions...")
        
        hanging_tests = [
            # SET NX variants that previously hung
            ("SET NX on existing", lambda: self._test_set_nx_existing()),
            ("SET XX on missing", lambda: self._test_set_xx_missing()),
            # Other operations that return null bulk strings
            ("GET missing key", lambda: self.r.get("missing_key_test_12345")),
            ("LPOP empty list", lambda: self.r.lpop("empty_list_test_12345")),
        ]
        
        for test_name, test_func in hanging_tests:
            start_time = time.time()
            try:
                result = test_func()
                elapsed = time.time() - start_time
                
                # Any response handling should be very fast
                if elapsed > 1.0:
                    print(f"❌ {test_name}: Response took {elapsed:.2f}s (possible hang)")
                    return False
                    
                print(f"✅ {test_name}: Responded in {elapsed:.3f}s")
                
            except redis.TimeoutError:
                print(f"❌ {test_name}: TIMEOUT - hanging detected!")
                return False
            except Exception as e:
                print(f"❌ {test_name}: Error - {e}")
                return False
        
        return True
        
    def _test_set_nx_existing(self):
        """Helper to test SET NX on existing key"""
        self.r.set("nx_existing_test", "original")
        return self.r.set("nx_existing_test", "new", nx=True)
        
    def _test_set_xx_missing(self):
        """Helper to test SET XX on missing key""" 
        self.r.delete("xx_missing_test")
        return self.r.set("xx_missing_test", "value", xx=True)
        
    def test_lua_socket_handling(self):
        """Test Lua scripts don't cause socket issues"""
        print("\nTesting Lua socket handling...")
        
        lua_tests = [
            # Basic operations
            ("Simple return", "return 'hello'", []),
            ("redis.call GET", "return redis.call('GET', 'nonexistent')", []),
            ("redis.call SET", "redis.call('SET', 'lua_test', 'value'); return 'OK'", []),
            # The atomic lock pattern that previously caused socket timeouts
            ("Atomic lock pattern", """
                if redis.call("GET", KEYS[1]) == ARGV[1] then
                    return redis.call("DEL", KEYS[1])
                else
                    return 0
                end
            """, ["lock_test_key", "lock_value"]),
        ]
        
        for test_name, script, args in lua_tests:
            try:
                # Set up for lock pattern test
                if "lock" in test_name:
                    self.r.set("lock_test_key", "lock_value")
                    
                start_time = time.time()
                
                if args:
                    result = self.r.eval(script, 1, *args)
                else:
                    result = self.r.eval(script, 0)
                    
                elapsed = time.time() - start_time
                
                # Lua operations should be fast
                if elapsed > 2.0:
                    print(f"❌ {test_name}: Lua took {elapsed:.2f}s (possible hang)")
                    return False
                    
                # For lock pattern, verify subsequent operations work
                if "lock" in test_name and result == 1:
                    # This GET should not timeout
                    verify_start = time.time()
                    check_result = self.r.get("lock_test_key")
                    verify_elapsed = time.time() - verify_start
                    
                    if verify_elapsed > 1.0:
                        print(f"❌ {test_name}: Post-script GET took {verify_elapsed:.2f}s")
                        return False
                        
                    if check_result is not None:
                        print(f"❌ {test_name}: Lock not properly deleted")
                        return False
                        
                print(f"✅ {test_name}: Completed in {elapsed:.3f}s")
                
            except redis.TimeoutError:
                print(f"❌ {test_name}: TIMEOUT - socket issue detected!")
                return False
            except Exception as e:
                print(f"❌ {test_name}: Error - {e}")
                return False
        
        return True
        
    def test_blocking_operations_reliability(self):
        """Test blocking operations work reliably"""
        print("\nTesting blocking operations reliability...")
        
        try:
            # Clean up
            self.r.delete("block_test_list")
            
            # Test 1: BLPOP with immediate data
            self.r.lpush("block_test_list", "immediate")
            start_time = time.time()
            result = self.r.blpop("block_test_list", timeout=1)
            elapsed = time.time() - start_time
            
            if result != ("block_test_list", "immediate"):
                print(f"❌ BLPOP immediate failed: {result}")
                return False
                
            if elapsed > 0.5:
                print(f"❌ BLPOP immediate took too long: {elapsed:.2f}s")
                return False
                
            # Test 2: BLPOP with timeout (should block then timeout)
            start_time = time.time()
            result = self.r.blpop("block_test_list", timeout=1)
            elapsed = time.time() - start_time
            
            if result is not None:
                print(f"❌ BLPOP timeout should return None, got: {result}")
                return False
                
            if elapsed < 0.8 or elapsed > 1.5:
                print(f"❌ BLPOP timeout took {elapsed:.2f}s (expected ~1s)")
                return False
                
            print("✅ Blocking operations reliability confirmed")
            return True
            
        except Exception as e:
            print(f"❌ Blocking operations test failed: {e}")
            return False
            
    def test_edge_cases_coverage(self):
        """Test edge cases that could reveal merge artifacts"""
        print("\nTesting edge cases...")
        
        edge_tests = [
            # Empty string operations 
            ("SET empty string", lambda: self.r.set("empty_test", "")),
            ("GET empty string", lambda: self.r.get("empty_test")),
            # Zero values
            ("INCR from non-existent", lambda: self.r.incr("incr_test_new")),
            ("DECR from zero", lambda: self.r.decr("incr_test_new")),
            # Multiple operations in quick succession
            ("Rapid SET operations", lambda: self._rapid_set_test()),
        ]
        
        for test_name, test_func in edge_tests:
            try:
                start_time = time.time()
                result = test_func()
                elapsed = time.time() - start_time
                
                if elapsed > 1.0:
                    print(f"❌ {test_name}: Too slow ({elapsed:.2f}s)")
                    return False
                    
                print(f"✅ {test_name}: OK ({elapsed:.3f}s)")
                
            except Exception as e:
                print(f"❌ {test_name}: {e}")
                return False
        
        return True
        
    def _rapid_set_test(self):
        """Helper for rapid SET operations"""
        for i in range(10):
            key = f"rapid_{i}"
            self.r.set(key, f"value_{i}")
            # Mix in some conditional operations
            if i % 2 == 0:
                self.r.set(key, f"nx_value_{i}", nx=True)  # Should fail
        return True

def main():
    print("=" * 70)
    print("FERROUS MERGE FAILURE REGRESSION PREVENTION TESTS")
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
    
    tester = RegressionTester()
    
    # Run all regression tests
    results = []
    results.append(tester.test_response_handling_regressions())
    results.append(tester.test_lua_socket_handling())
    results.append(tester.test_blocking_operations_reliability())
    results.append(tester.test_edge_cases_coverage())
    
    # Summary
    passed = sum(results)
    total = len(results)
    
    print()
    print("=" * 70)
    print(f"REGRESSION PREVENTION RESULTS: {passed}/{total} PASSED")
    print("=" * 70)
    
    if passed == total:
        print("🎉 All regression prevention tests passed!")
        print("✅ Ferrous is protected against known merge failure patterns")
        print("✅ Critical hanging bugs are prevented")
        print("✅ Socket handling is robust")
        sys.exit(0)
    else:
        print("❌ Some regression tests failed")
        print("⚠️  Ferrous may be vulnerable to similar issues")
        sys.exit(1)

if __name__ == "__main__":
    main()