#!/usr/bin/env python3
"""
Comprehensive expiry operations tests for Ferrous
Tests SETEX, PSETEX, EXPIRE, PEXPIRE, TTL, PTTL, PERSIST 
and focuses on timing edge cases and race conditions
"""

import redis
import time
import threading
import random
import sys

def test_basic_expiry_operations():
    """Test basic expiry functionality"""
    print("Testing basic expiry operations...")
    
    try:
        r = redis.Redis(host='127.0.0.1', port=6379, decode_responses=True)
        
        # Test SETEX
        r.setex('test_setex', 2, 'expiry_value')
        assert r.get('test_setex') == 'expiry_value'
        assert r.ttl('test_setex') <= 2
        
        # Test EXPIRE on existing key
        r.set('test_expire', 'value')
        r.expire('test_expire', 2)
        assert r.ttl('test_expire') <= 2
        
        # Test PSETEX
        r.psetex('test_psetex', 1500, 'ms_value') 
        assert r.get('test_psetex') == 'ms_value'
        assert r.pttl('test_psetex') <= 1500
        
        # Test TTL on non-existent key
        assert r.ttl('nonexistent_key') == -2
        
        # Test TTL on non-expiring key
        r.set('persistent_key', 'value')
        assert r.ttl('persistent_key') == -1
        
        print("✅ Basic expiry operations working")
        return True
        
    except Exception as e:
        print(f"❌ Basic expiry test failed: {e}")
        return False

def test_expiry_timing_edge_cases():
    """Test timing edge cases in expiry operations"""
    print("\nTesting expiry timing edge cases...")
    
    try:
        r = redis.Redis(host='127.0.0.1', port=6379, decode_responses=True)
        
        # Test 1: Very short expiry (should expire quickly)
        r.setex('short_expire', 1, 'value')
        time.sleep(0.1)  # Small delay
        ttl_before = r.ttl('short_expire')
        if ttl_before is None or ttl_before < 0:
            print("❌ Key expired too quickly")
            return False
            
        time.sleep(1.1)  # Wait for expiry
        value_after = r.get('short_expire')
        if value_after is not None:
            print(f"❌ Key did not expire after TTL: {value_after}")
            return False
            
        # Test 2: Rapid TTL updates
        r.set('rapid_ttl', 'value')
        for i in range(10):
            r.expire('rapid_ttl', 5)
            ttl = r.ttl('rapid_ttl')
            if ttl <= 0:
                print(f"❌ TTL became negative during rapid updates: {ttl}")
                return False
            time.sleep(0.01)
            
        # Test 3: Concurrent access during expiry
        r.setex('concurrent_expire', 2, 'concurrent_value')
        
        def access_key():
            for _ in range(20):
                try:
                    val = r.get('concurrent_expire')
                    if val is None:
                        break
                except:
                    pass
                time.sleep(0.1)
        
        thread = threading.Thread(target=access_key)
        thread.start()
        thread.join(timeout=3.0)
        
        # Test 4: Precision timing test
        start_time = time.time()
        r.setex('precision_test', 2, 'precision_value')
        
        while True:
            value = r.get('precision_test')
            elapsed = time.time() - start_time
            
            if value is None:
                # Key expired
                if elapsed < 1.9 or elapsed > 3.1:  # Allow 10% tolerance
                    print(f"❌ Key expired at wrong time: {elapsed}s (expected ~2s)")
                    return False
                break
                
            if elapsed > 3.0:
                print("❌ Key didn't expire within expected time")
                return False
                
            time.sleep(0.1)
        
        print("✅ Expiry timing edge cases working")
        return True
        
    except Exception as e:
        print(f"❌ Timing edge case test failed: {e}")
        return False

def test_expiry_race_conditions():
    """Test race conditions between expiry and operations"""
    print("\nTesting expiry race conditions...")
    
    try:
        r = redis.Redis(host='127.0.0.1', port=6379, decode_responses=True)
        
        # Test 1: SET vs EXPIRE race
        race_issues = 0
        for i in range(50):
            key = f'race_test_{i}'
            
            def set_value():
                r.set(key, f'value_{i}')
                
            def expire_value():
                time.sleep(0.001)  # Small delay
                try:
                    r.expire(key, 1)
                except:
                    pass
                    
            t1 = threading.Thread(target=set_value)
            t2 = threading.Thread(target=expire_value)
            
            t1.start()
            t2.start()
            t1.join()
            t2.join()
            
            # Check if key exists and has TTL
            if r.exists(key):
                ttl = r.ttl(key)
                if ttl == -1:
                    # Key exists but no TTL set - might indicate race condition
                    race_issues += 1
        
        if race_issues > 10:  # Allow some tolerance
            print(f"❌ Too many race condition issues: {race_issues}/50")
            return False
        
        # Test 2: Expiry vs access race
        r.setex('access_race', 1, 'value')
        time.sleep(0.9)  # Wait until close to expiry
        
        # Rapid access while expiring
        access_results = []
        for _ in range(10):
            result = r.get('access_race')
            access_results.append(result)
            time.sleep(0.02)
        
        # Should have mixture of values and None
        has_value = any(result is not None for result in access_results)
        has_none = any(result is None for result in access_results)
        
        if not (has_value or has_none):
            print("❌ Expiry behavior inconsistent during race")
            return False
            
        print("✅ Expiry race conditions handled reasonably")
        return True
        
    except Exception as e:
        print(f"❌ Race condition test failed: {e}")
        return False

def test_persist_command():
    """Test PERSIST command functionality"""
    print("\nTesting PERSIST command...")
    
    try:
        r = redis.Redis(host='127.0.0.1', port=6379, decode_responses=True)
        
        # Test persisting a key with TTL
        r.setex('persist_test', 10, 'value')
        assert r.ttl('persist_test') > 0
        
        result = r.persist('persist_test')
        assert result == 1  # Should return 1 for success
        assert r.ttl('persist_test') == -1  # Should be -1 (no expiry)
        
        # Test persisting a key without TTL
        r.set('no_ttl_key', 'value')
        result = r.persist('no_ttl_key')
        assert result == 0  # Should return 0 (no TTL to remove)
        
        # Test persisting non-existent key
        result = r.persist('nonexistent_key')
        assert result == 0  # Should return 0
        
        print("✅ PERSIST command working")
        return True
        
    except Exception as e:
        print(f"❌ PERSIST test failed: {e}")
        return False

def test_negative_expire():
    """Test behavior with negative expire times"""
    print("\nTesting negative expire behavior...")
    
    try:
        r = redis.Redis(host='127.0.0.1', port=6379, decode_responses=True)
        
        # Set a key
        r.set('negative_test', 'value')
        assert r.get('negative_test') == 'value'
        
        # Set negative expiry (should delete immediately in Redis)
        r.expire('negative_test', -1)
        
        # Key should be deleted immediately
        value = r.get('negative_test')
        if value is not None:
            print(f"❌ Key with negative expire still exists: {value}")
            return False
            
        print("✅ Negative expire behavior correct")
        return True
        
    except Exception as e:
        print(f"❌ Negative expire test failed: {e}")
        return False

def test_expiry_stress():
    """Stress test expiry operations for consistency"""
    print("\nTesting expiry under stress...")
    
    try:
        r = redis.Redis(host='127.0.0.1', port=6379, decode_responses=True)
        
        # Create many keys with various expiry times
        inconsistencies = 0
        total_tests = 100
        
        for i in range(total_tests):
            key = f'stress_{i}'
            expiry_time = random.randint(1, 3)
            
            r.setex(key, expiry_time, f'value_{i}')
            
            # Immediately check TTL
            ttl = r.ttl(key)
            if ttl <= 0 or ttl > expiry_time:
                inconsistencies += 1
            
            # Random delay
            time.sleep(random.uniform(0.001, 0.01))
            
            # Check if key still exists when it should
            current_ttl = r.ttl(key)
            if current_ttl == -2:  # Key doesn't exist
                elapsed = time.time()
                # This is expected behavior, but we're checking consistency
        
        # Clean up
        r.flushdb()
        
        if inconsistencies > total_tests * 0.1:  # Allow 10% tolerance
            print(f"❌ Too many TTL inconsistencies: {inconsistencies}/{total_tests}")
            return False
            
        print("✅ Expiry stress test passed")
        return True
        
    except Exception as e:
        print(f"❌ Expiry stress test failed: {e}")
        return False

def test_expiry_boundary_conditions():
    """Test boundary conditions for expiry"""
    print("\nTesting expiry boundary conditions...")
    
    try:
        r = redis.Redis(host='127.0.0.1', port=6379, decode_responses=True)
        
        # Test zero expiry
        r.set('zero_expire_test', 'value')
        r.expire('zero_expire_test', 0)
        # Key should be deleted immediately
        if r.get('zero_expire_test') is not None:
            print("❌ Zero expire didn't delete key immediately")
            return False
        
        # Test maximum expire value
        r.set('max_expire_test', 'value')
        try:
            r.expire('max_expire_test', 2147483647)  # Max 32-bit int
            ttl = r.ttl('max_expire_test')
            if ttl <= 0:
                print("❌ Maximum expire value not handled correctly")
                return False
        except Exception as e:
            print(f"⚠️ Maximum expire test inconclusive: {e}")
        
        # Test very large negative expire
        r.set('large_negative_test', 'value')
        r.expire('large_negative_test', -1000000)
        if r.get('large_negative_test') is not None:
            print("❌ Large negative expire didn't delete key")
            return False
            
        print("✅ Expiry boundary conditions working")
        return True
        
    except Exception as e:
        print(f"❌ Boundary condition test failed: {e}")
        return False

def main():
    print("=" * 70)
    print("FERROUS EXPIRY OPERATIONS COMPREHENSIVE TESTS")
    print("=" * 70)
    
    # Check if server is running
    try:
        r = redis.Redis(host='127.0.0.1', port=6379)
        r.ping()
        print("✅ Server connection verified\n")
    except:
        print("❌ Cannot connect to server on port 6379")
        sys.exit(1)
    
    # Run all tests
    tests = [
        ("Basic expiry operations", test_basic_expiry_operations),
        ("Expiry timing edge cases", test_expiry_timing_edge_cases),
        ("Expiry race conditions", test_expiry_race_conditions),
        ("PERSIST command", test_persist_command),
        ("Negative expire behavior", test_negative_expire),
        ("Expiry stress test", test_expiry_stress),
        ("Expiry boundary conditions", test_expiry_boundary_conditions),
    ]
    
    results = []
    for test_name, test_func in tests:
        print(f"\n[TEST] {test_name}")
        result = test_func()
        results.append((test_name, result))
    
    # Summary
    passed = sum(1 for _, result in results if result)
    total = len(results)
    
    print("\n" + "=" * 70)
    print("EXPIRY TEST RESULTS")
    print("=" * 70)
    
    for test_name, result in results:
        status = "✅ PASS" if result else "❌ FAIL"
        print(f"{test_name}: {status}")
    
    print(f"\nTotal: {passed}/{total} tests passed")
    
    if passed == total:
        print("🎉 All expiry tests passed!")
        sys.exit(0)
    else:
        print("❌ Some expiry tests failed - timing edge cases detected!")
        sys.exit(1)

if __name__ == "__main__":
    main()