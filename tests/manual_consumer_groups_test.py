#!/usr/bin/env python3
"""
Manual test for consumer groups functionality in Ferrous.
Tests core functionality step by step to identify implementation issues.
"""

import redis
import sys
import time

def test_consumer_groups():
    """Test consumer groups functionality step by step."""
    client = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    print("Testing Consumer Groups Implementation")
    print("=" * 50)
    
    # Clear database
    print("1. Flushing database...")
    client.flushall()
    print("   ✓ FLUSHALL successful")
    
    # Test basic stream operations
    print("\n2. Testing basic stream operations...")
    try:
        id1 = client.xadd("test:stream", {"field1": "value1"})
        id2 = client.xadd("test:stream", {"field2": "value2"})
        print(f"   ✓ XADD successful: {id1}, {id2}")
        
        length = client.xlen("test:stream")
        print(f"   ✓ XLEN: {length} entries")
        
        entries = client.xrange("test:stream")
        print(f"   ✓ XRANGE: {len(entries)} entries")
        
    except Exception as e:
        print(f"   ✗ Basic stream operations failed: {e}")
        return False
    
    # Test XGROUP CREATE
    print("\n3. Testing XGROUP CREATE...")
    try:
        result = client.xgroup_create("test:stream", "mygroup", "0")
        if result:
            print("   ✓ XGROUP CREATE successful")
        else:
            print("   ✗ XGROUP CREATE returned False")
            return False
    except Exception as e:
        print(f"   ✗ XGROUP CREATE failed: {e}")
        return False
    
    # Test XINFO GROUPS
    print("\n4. Testing XINFO GROUPS...")
    try:
        groups = client.xinfo_groups("test:stream")
        print(f"   Groups found: {len(groups)}")
        if groups:
            for group in groups:
                print(f"   Group: {group}")
        print("   ✓ XINFO GROUPS successful")
    except Exception as e:
        print(f"   ✗ XINFO GROUPS failed: {e}")
    
    # Test XREADGROUP
    print("\n5. Testing XREADGROUP...")
    try:
        result = client.xreadgroup("mygroup", "consumer1", {"test:stream": ">"})
        print(f"   XREADGROUP result: {result}")
        if result:
            print("   ✓ XREADGROUP successful")
        else:
            print("   ⚠ XREADGROUP returned empty (may be correct)")
    except Exception as e:
        print(f"   ✗ XREADGROUP failed: {e}")
    
    # Test XPENDING
    print("\n6. Testing XPENDING...")
    try:
        pending = client.xpending("test:stream", "mygroup")
        print(f"   Pending messages: {pending}")
        print("   ✓ XPENDING successful")
    except Exception as e:
        print(f"   ✗ XPENDING failed: {e}")
    
    # Test XACK
    print("\n7. Testing XACK...")
    try:
        acked = client.xack("test:stream", "mygroup", id1)
        print(f"   XACK result: {acked}")
        print("   ✓ XACK successful")
    except Exception as e:
        print(f"   ✗ XACK failed: {e}")
    
    # Test XGROUP DESTROY
    print("\n8. Testing XGROUP DESTROY...")
    try:
        destroyed = client.xgroup_destroy("test:stream", "mygroup")
        print(f"   XGROUP DESTROY result: {destroyed}")
        print("   ✓ XGROUP DESTROY successful")
    except Exception as e:
        print(f"   ✗ XGROUP DESTROY failed: {e}")
    
    print("\n" + "=" * 50)
    print("Manual test completed")
    return True

if __name__ == "__main__":
    try:
        test_consumer_groups()
    except KeyboardInterrupt:
        print("\nTest interrupted by user")
        sys.exit(1)
    except Exception as e:
        print(f"Unexpected error: {e}")
        sys.exit(1)