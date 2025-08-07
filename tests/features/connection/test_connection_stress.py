#!/usr/bin/env python3
"""
Connection Management Stress Test Suite for Ferrous
Tests connection limits, recovery, and production stress scenarios
"""

import redis
import socket
import threading
import time
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed

def test_multiple_connection_limits():
    """Test server behavior with many concurrent connections"""
    print("Testing multiple connection limits...")
    
    max_connections = 100  # Test reasonable concurrent connection limit
    active_connections = []
    
    try:
        # Create many connections
        for i in range(max_connections):
            try:
                r = redis.Redis(host='localhost', port=6379, decode_responses=True, 
                              socket_timeout=5, socket_connect_timeout=5)
                result = r.ping()
                if result == True:
                    active_connections.append(r)
                else:
                    print(f"❌ Connection {i} failed PING with result: {result}")
                    break
            except Exception as e:
                print(f"❌ Connection {i} failed: {e}")
                break
        
        if len(active_connections) >= max_connections * 0.9:  # Allow 10% tolerance
            print(f"✅ Connection limits: {len(active_connections)}/{max_connections} connections established")
            
            # Test that all connections can still execute commands
            success_count = 0
            for i, conn in enumerate(active_connections[:10]):  # Test first 10 for speed
                try:
                    conn.set(f'conn_test_{i}', f'value_{i}')
                    result = conn.get(f'conn_test_{i}')
                    if result == f'value_{i}':
                        success_count += 1
                    conn.delete(f'conn_test_{i}')
                except Exception as e:
                    print(f"❌ Connection {i} operation failed: {e}")
            
            if success_count == 10:
                print("✅ All active connections functional")
                return_value = True
            else:
                print(f"❌ Only {success_count}/10 connections functional")
                return_value = False
        else:
            print(f"❌ Too few connections established: {len(active_connections)}")
            return_value = False
            
    finally:
        # Cleanup connections
        for conn in active_connections:
            try:
                conn.close()
            except:
                pass
    
    return return_value

def test_connection_recovery():
    """Test connection recovery after various failure scenarios"""
    print("Testing connection recovery scenarios...")
    
    # Test 1: Reconnection after timeout
    try:
        r = redis.Redis(host='localhost', port=6379, decode_responses=True,
                       socket_timeout=1)  # Short timeout
        
        # Normal operation
        r.ping()
        
        # Force timeout by blocking operation with short timeout
        try:
            r.blpop('nonexistent_key', 2)  # This will timeout
        except redis.TimeoutError:
            pass  # Expected
        
        # Should be able to reconnect and operate normally
        r.ping()  # This should work after timeout
        r.set('recovery_test', 'recovered')
        result = r.get('recovery_test')
        
        if result == 'recovered':
            print("✅ Connection recovery: Timeout recovery working")
            success_1 = True
        else:
            print("❌ Connection recovery: Failed after timeout")
            success_1 = False
        
        r.delete('recovery_test')
        
    except Exception as e:
        print(f"❌ Connection recovery test failed: {e}")
        success_1 = False
    
    # Test 2: Rapid connect/disconnect cycles
    rapid_test_success = True
    for i in range(50):
        try:
            r = redis.Redis(host='localhost', port=6379, decode_responses=True)
            r.ping()
            r.set(f'rapid_{i}', f'value_{i}')
            result = r.get(f'rapid_{i}')
            r.delete(f'rapid_{i}')
            r.close()
            
            if result != f'value_{i}':
                rapid_test_success = False
                break
                
        except Exception as e:
            print(f"❌ Rapid connection test failed at iteration {i}: {e}")
            rapid_test_success = False
            break
    
    if rapid_test_success:
        print("✅ Rapid connection cycling: 50 cycles successful")
        success_2 = True
    else:
        print("❌ Rapid connection cycling failed")
        success_2 = False
    
    return success_1 and success_2

def test_malformed_connection_handling():
    """Test server resilience to malformed connections and data"""
    print("Testing malformed connection handling...")
    
    test_cases = [
        # (description, data, expect_disconnect)
        ("Invalid RESP type", b"@invalid\r\n", True),
        ("Negative array length", b"*-5\r\n", True), 
        ("Incomplete bulk string", b"$10\r\nhello", True),
        ("Invalid bulk string length", b"$abc\r\n", True),
        ("Oversized length claim", b"$999999999\r\nshort", True),
        ("Binary garbage", b"\x00\x01\x02\x03\x04", True),
    ]
    
    success_count = 0
    for desc, data, expect_disconnect in test_cases:
        try:
            s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            s.connect(('127.0.0.1', 6379))
            s.settimeout(2.0)
            
            s.sendall(data)
            
            try:
                response = s.recv(1024)
                if expect_disconnect and len(response) == 0:
                    print(f"✅ {desc}: Server correctly disconnected")
                    success_count += 1
                elif not expect_disconnect and len(response) > 0:
                    print(f"✅ {desc}: Server handled gracefully")
                    success_count += 1
                else:
                    print(f"❌ {desc}: Unexpected behavior")
            except socket.timeout:
                if expect_disconnect:
                    print(f"✅ {desc}: Server timeout (acceptable)")
                    success_count += 1
                else:
                    print(f"❌ {desc}: Unexpected timeout")
                    
        except Exception as e:
            print(f"❌ {desc}: Connection error - {e}")
        finally:
            try:
                s.close()
            except:
                pass
            
        # Small delay between tests
        time.sleep(0.1)
    
    return success_count == len(test_cases)

def main():
    print("=" * 70)
    print("CONNECTION STRESS TESTS")
    print("=" * 70)
    
    # Verify server connection
    try:
        r = redis.Redis(host='localhost', port=6379, decode_responses=True)
        r.ping()
        print("✅ Server connection verified")
    except Exception as e:
        print(f"❌ Cannot connect to server: {e}")
        sys.exit(1)
    
    print()
    
    # Run connection tests
    test_functions = [
        test_multiple_connection_limits,
        test_connection_recovery,
        test_malformed_connection_handling,
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
    print(f"CONNECTION TEST RESULTS: {passed}/{total} PASSED")
    print("=" * 70)
    
    if passed == total:
        print("All connection stress tests passed")
        sys.exit(0)
    else:
        print(f"{total - passed} connection tests failed")
        sys.exit(1)

if __name__ == "__main__":
    main()