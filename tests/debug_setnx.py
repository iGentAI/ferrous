#!/usr/bin/env python3
"""
Debug script to test SETNX and SET NX operations
"""

import redis
import time
import sys
import threading

def test_setnx():
    """Test SETNX command"""
    print("Testing SETNX command...")
    
    try:
        r = redis.Redis(host='127.0.0.1', port=6379, decode_responses=True,
                       socket_timeout=3, socket_connect_timeout=3)
        
        # Clean up
        r.delete("test_setnx_key")
        
        # Test 1: SETNX on non-existent key (should set)
        print("  Test 1: SETNX on non-existent key")
        result = r.setnx("test_setnx_key", "value1")
        print(f"    Result: {result} (expected: True/1)")
        
        # Verify it was set
        value = r.get("test_setnx_key")
        print(f"    Value: {value} (expected: 'value1')")
        
        if result and value == "value1":
            print("    ✅ SETNX on new key works")
        else:
            print("    ❌ SETNX on new key failed")
            return False
        
        # Test 2: SETNX on existing key (should not set)
        print("  Test 2: SETNX on existing key")
        result = r.setnx("test_setnx_key", "value2")
        print(f"    Result: {result} (expected: False/0)")
        
        # Verify value unchanged
        value = r.get("test_setnx_key")
        print(f"    Value: {value} (expected: 'value1')")
        
        if not result and value == "value1":
            print("    ✅ SETNX on existing key correctly rejected")
        else:
            print("    ❌ SETNX on existing key failed")
            return False
            
        return True
        
    except redis.TimeoutError as e:
        print(f"  ❌ TIMEOUT: {e}")
        print("    This confirms the SETNX hanging issue!")
        return False
    except Exception as e:
        print(f"  ❌ ERROR: {e}")
        return False

def test_set_nx():
    """Test SET with NX option"""
    print("\nTesting SET with NX option...")
    
    try:
        r = redis.Redis(host='127.0.0.1', port=6379, decode_responses=True,
                       socket_timeout=3, socket_connect_timeout=3)
        
        # Clean up
        r.delete("test_set_nx_key")
        
        # Test 1: SET NX on non-existent key (should set)
        print("  Test 1: SET NX on non-existent key")
        result = r.set("test_set_nx_key", "value1", nx=True)
        print(f"    Result: {result} (expected: True)")
        
        # Verify it was set
        value = r.get("test_set_nx_key")
        print(f"    Value: {value} (expected: 'value1')")
        
        if result and value == "value1":
            print("    ✅ SET NX on new key works")
        else:
            print("    ❌ SET NX on new key failed")
            return False
        
        # Test 2: SET NX on existing key (should not set)
        print("  Test 2: SET NX on existing key")
        result = r.set("test_set_nx_key", "value2", nx=True)
        print(f"    Result: {result} (expected: None/False)")
        
        # Verify value unchanged
        value = r.get("test_set_nx_key")
        print(f"    Value: {value} (expected: 'value1')")
        
        if result is None and value == "value1":
            print("    ✅ SET NX on existing key correctly rejected")
        else:
            print(f"    ❌ SET NX on existing key failed (result={result})")
            return False
            
        return True
        
    except redis.TimeoutError as e:
        print(f"  ❌ TIMEOUT: {e}")
        print("    This confirms the SET NX hanging issue!")
        return False
    except Exception as e:
        print(f"  ❌ ERROR: {e}")
        return False

def test_blocking_operations():
    """Test BLPOP/BRPOP blocking behavior"""
    print("\nTesting blocking operations...")
    
    try:
        r = redis.Redis(host='127.0.0.1', port=6379, decode_responses=True,
                       socket_timeout=5, socket_connect_timeout=3)
        
        # Clean up
        r.delete("test_list")
        
        # Test 1: Non-blocking case (data exists)
        print("  Test 1: BLPOP with data available")
        r.lpush("test_list", "value1")
        result = r.blpop("test_list", timeout=1)
        print(f"    Result: {result} (expected: ('test_list', 'value1'))")
        
        if result == ("test_list", "value1"):
            print("    ✅ BLPOP with data works")
        else:
            print("    ❌ BLPOP with data failed")
            return False
        
        # Test 2: Blocking case (no data, should timeout)
        print("  Test 2: BLPOP without data (should block then timeout)")
        start_time = time.time()
        result = r.blpop("test_list", timeout=2)
        elapsed = time.time() - start_time
        print(f"    Result: {result} (expected: None)")
        print(f"    Elapsed time: {elapsed:.1f}s (expected: ~2s)")
        
        if result is None and elapsed >= 1.5:
            print("    ✅ BLPOP blocking works correctly")
        else:
            print(f"    ❌ BLPOP blocking failed (elapsed={elapsed:.1f}s)")
            return False
            
        return True
        
    except redis.TimeoutError as e:
        print(f"  ❌ TIMEOUT: {e}")
        return False
    except Exception as e:
        print(f"  ❌ ERROR: {e}")
        return False

def main():
    print("=" * 60)
    print("SETNX AND BLOCKING OPERATIONS DEBUG TEST")
    print("=" * 60)
    
    # Check server is running
    try:
        r = redis.Redis(host='127.0.0.1', port=6379, socket_timeout=1)
        r.ping()
        print("✅ Server connection verified\n")
    except:
        print("❌ Cannot connect to server")
        sys.exit(1)
    
    # Run tests
    success = True
    success &= test_setnx()
    success &= test_set_nx()
    success &= test_blocking_operations()
    
    print("\n" + "=" * 60)
    if success:
        print("✅ All operations work correctly")
    else:
        print("❌ Issues detected - see above for details")
    print("=" * 60)
    
    sys.exit(0 if success else 1)

if __name__ == "__main__":
    main()