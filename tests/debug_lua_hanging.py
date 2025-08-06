#!/usr/bin/env python3
"""
Debug script to identify why Lua tests are hanging
"""

import redis
import time
import sys
import socket

def test_simple_eval():
    """Test the simplest possible EVAL"""
    print("Testing simple EVAL...")
    
    try:
        # Use a very short timeout to detect hanging
        r = redis.Redis(host='127.0.0.1', port=6379, decode_responses=True, 
                       socket_timeout=3, socket_connect_timeout=3)
        
        # Test 1: Simplest possible script
        print("  Test 1: return 42")
        result = r.eval("return 42", 0)
        print(f"    Result: {result}")
        
        # Test 2: Simple string return
        print("  Test 2: return 'hello'")
        result = r.eval("return 'hello'", 0)
        print(f"    Result: {result}")
        
        # Test 3: Using KEYS
        print("  Test 3: return KEYS[1]")
        result = r.eval("return KEYS[1]", 1, "mykey")
        print(f"    Result: {result}")
        
        # Test 4: Using redis.call GET
        print("  Test 4: redis.call GET")
        r.set("test_key", "test_value")
        result = r.eval("return redis.call('GET', KEYS[1])", 1, "test_key")
        print(f"    Result: {result}")
        
        # Test 5: Boolean false (reported as hanging)
        print("  Test 5: return false")
        result = r.eval("return false", 0)
        print(f"    Result: {result}")
        
        print("\n✅ All tests passed without hanging!")
        return True
        
    except redis.TimeoutError as e:
        print(f"\n❌ TIMEOUT: {e}")
        return False
    except Exception as e:
        print(f"\n❌ ERROR: {e}")
        return False

def test_atomic_lock_pattern():
    """Test the specific atomic lock release pattern"""
    print("\nTesting atomic lock release pattern...")
    
    lock_script = """
        if redis.call("GET", KEYS[1]) == ARGV[1] then
            return redis.call("DEL", KEYS[1])
        else
            return 0
        end
    """
    
    try:
        r = redis.Redis(host='127.0.0.1', port=6379, decode_responses=True,
                       socket_timeout=3, socket_connect_timeout=3)
        
        # Set up the lock
        lock_key = "test_lock"
        lock_value = "unique_123"
        r.set(lock_key, lock_value)
        print(f"  Set lock: {lock_key} = {lock_value}")
        
        # Test releasing with correct value
        print("  Releasing with correct value...")
        result = r.eval(lock_script, 1, lock_key, lock_value)
        print(f"    Result: {result} (expected: 1)")
        
        # Verify lock is gone
        value = r.get(lock_key)
        print(f"    Lock after release: {value} (expected: None)")
        
        if result == 1 and value is None:
            print("  ✅ Lock release with correct value works!")
        else:
            print("  ❌ Lock release failed!")
            return False
            
        # Set lock again for wrong value test
        r.set(lock_key, lock_value)
        
        # Test with wrong value
        print("  Testing with wrong value...")
        result = r.eval(lock_script, 1, lock_key, "wrong_value")
        print(f"    Result: {result} (expected: 0)")
        
        # Verify lock still exists
        value = r.get(lock_key)
        print(f"    Lock after failed release: {value} (expected: {lock_value})")
        
        if result == 0 and value == lock_value:
            print("  ✅ Lock rejection with wrong value works!")
            return True
        else:
            print("  ❌ Lock rejection failed!")
            return False
            
    except redis.TimeoutError as e:
        print(f"  ❌ TIMEOUT: {e}")
        return False
    except Exception as e:
        print(f"  ❌ ERROR: {e}")
        return False

def main():
    print("=" * 60)
    print("LUA HANGING DEBUG TEST")
    print("=" * 60)
    
    # Check server is running
    try:
        r = redis.Redis(host='127.0.0.1', port=6379, socket_timeout=1)
        r.ping()
        print("✅ Server connection verified")
    except:
        print("❌ Cannot connect to server")
        sys.exit(1)
    
    # Run tests
    success = True
    success &= test_simple_eval()
    success &= test_atomic_lock_pattern()
    
    print("\n" + "=" * 60)
    if success:
        print("✅ No hanging detected - issue may be in test framework")
    else:
        print("❌ Hanging or errors detected")
    print("=" * 60)
    
    sys.exit(0 if success else 1)

if __name__ == "__main__":
    main()