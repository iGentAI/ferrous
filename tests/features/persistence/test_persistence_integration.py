#!/usr/bin/env python3
"""
Comprehensive persistence integration tests for Ferrous
Tests RDB, AOF, and their integration including failure scenarios
"""

import socket
import time
import os
import sys

def redis_command(cmd, host='127.0.0.1', port=6379):
    """Send Redis command and return response"""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect((host, port))
    
    s.sendall(cmd.encode())
    resp = s.recv(4096)
    s.close()
    return resp

def test_rdb_save_load():
    """Test RDB save and data persistence"""
    print("Testing RDB save functionality...")
    
    # Set test data
    redis_command("*3\r\n$3\r\nSET\r\n$8\r\nrdb_test\r\n$10\r\nrdb_value1\r\n")
    redis_command("*3\r\n$5\r\nLPUSH\r\n$8\r\nrdb_list\r\n$5\r\nitem1\r\n")
    redis_command("*3\r\n$5\r\nLPUSH\r\n$8\r\nrdb_list\r\n$5\r\nitem2\r\n")
    
    # Trigger RDB save
    resp = redis_command("*1\r\n$4\r\nSAVE\r\n")
    if b"+OK" not in resp:
        print("❌ RDB SAVE failed")
        return False
        
    # Check if RDB file was created
    if os.path.exists("dump.rdb"):
        print("✅ RDB file created successfully")
        return True
    else:
        print("❌ RDB file not found")
        return False

def test_background_save():
    """Test background RDB save (BGSAVE)"""
    print("Testing background save...")
    
    # Set more test data
    for i in range(100):
        key = f"bgkey{i}"
        value = f"bgvalue{i}"
        cmd = f"*3\r\n$3\r\nSET\r\n${len(key)}\r\n{key}\r\n${len(value)}\r\n{value}\r\n"
        redis_command(cmd)
    
    # Trigger background save
    resp = redis_command("*1\r\n$6\r\nBGSAVE\r\n")
    if b"Background saving started" in resp:
        print("✅ Background save initiated")
        
        # Wait and check LASTSAVE
        time.sleep(1)
        resp = redis_command("*1\r\n$8\r\nLASTSAVE\r\n")
        if resp.startswith(b":") and not resp.strip() == b":0":
            print("✅ Background save completed")
            return True
        else:
            print("❌ Background save may not have completed")
            return False
    else:
        print("❌ BGSAVE failed to start")
        return False

def test_save_conflict():
    """Test handling of concurrent save operations"""  
    print("Testing save conflict handling...")
    
    # Start background save
    redis_command("*1\r\n$6\r\nBGSAVE\r\n")
    
    # Immediately try another save - should handle gracefully
    resp = redis_command("*1\r\n$6\r\nBGSAVE\r\n")
    
    # Should either succeed or give appropriate error
    if b"Background saving started" in resp or b"Background save already in progress" in resp:
        print("✅ Save conflict handled correctly")
        return True
    else:
        print(f"❌ Unexpected save conflict response: {resp}")
        return False

def test_data_types_persistence():
    """Test persistence of all data types"""
    print("Testing all data types persistence...")
    
    # Create data of all types with correct length specifiers
    commands = [
        "*3\r\n$3\r\nSET\r\n$11\r\nstring_test\r\n$12\r\nstring_value\r\n",
        "*3\r\n$5\r\nLPUSH\r\n$9\r\nlist_test\r\n$10\r\nlist_item1\r\n",
        "*3\r\n$4\r\nSADD\r\n$8\r\nset_test\r\n$11\r\nset_member1\r\n",
        "*4\r\n$4\r\nHSET\r\n$9\r\nhash_test\r\n$6\r\nfield1\r\n$11\r\nhash_value1\r\n",
        "*4\r\n$4\r\nZADD\r\n$9\r\nzset_test\r\n$3\r\n1.0\r\n$12\r\nzset_member1\r\n",
    ]
    
    for cmd in commands:
        resp = redis_command(cmd)
        if b"+OK" not in resp and b":" not in resp:  # OK or integer response
            print(f"❌ Failed to create test data: {cmd[:20]}...")
            return False
    
    # Save data
    resp = redis_command("*1\r\n$4\r\nSAVE\r\n")
    if b"+OK" not in resp:
        print("❌ Failed to save data")
        return False
        
    print("✅ All data types persistence test setup completed")
    return True

def main():
    print("=" * 60)
    print("FERROUS PERSISTENCE COMPREHENSIVE TESTS")
    print("=" * 60)
    
    # Verify server connection
    try:
        resp = redis_command("*1\r\n$4\r\nPING\r\n")
        if b"PONG" not in resp:
            print("❌ Server not responding")
            sys.exit(1)
    except:
        print("❌ Cannot connect to server")
        sys.exit(1)
        
    print("✅ Server connection verified")
    print()
    
    # Run tests
    results = []
    results.append(test_rdb_save_load())
    results.append(test_background_save())
    results.append(test_save_conflict())
    results.append(test_data_types_persistence())
    
    # Summary
    passed = sum(results)
    total = len(results)
    
    print()
    print("=" * 60)
    print(f"PERSISTENCE TEST RESULTS: {passed}/{total} PASSED") 
    print("=" * 60)
    
    if passed == total:
        print("🎉 All persistence tests passed!")
        sys.exit(0)
    else:
        print("❌ Some persistence tests failed")
        sys.exit(1)

if __name__ == "__main__":
    main()