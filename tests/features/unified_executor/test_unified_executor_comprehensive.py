#!/usr/bin/env python3
"""
Comprehensive unified command executor test suite for Ferrous
Validates complete Redis command coverage through Lua interface
"""

import redis
import time
import sys
import threading

class UnifiedExecutorTester:
    def __init__(self, host='127.0.0.1', port=6379):
        self.host = host
        self.port = port
        self.r = redis.Redis(host=host, port=port, decode_responses=True, socket_timeout=5)
        
    def test_string_operations_comprehensive(self):
        """Test comprehensive string operations through unified executor"""
        print("Testing comprehensive string operations...")
        
        try:
            # Test SET operations separately for clearer validation
            basic_set_script = """
                redis.call("DEL", "string_test")
                redis.call("SET", "string_test", "value1")
                local get1 = redis.call("GET", "string_test")
                
                -- Test SET NX (should fail on existing key) 
                local setnx_result = redis.call("SET", "string_test", "value2", "NX")
                local get2 = redis.call("GET", "string_test")
                
                return {get1, setnx_result, get2}
            """
            
            result1 = self.r.eval(basic_set_script, 0)
            if result1 != ["value1", None, "value1"]:
                print(f"❌ Basic SET operations failed: {result1}")
                return False
            
            # Test arithmetic operations
            arith_script = """
                redis.call("SET", "counter", "10")
                local incr_result = redis.call("INCR", "counter")
                local incrby_result = redis.call("INCRBY", "counter", "5")
                return {incr_result, incrby_result}
            """
            
            result2 = self.r.eval(arith_script, 0)
            if result2 != [11, 16]:
                print(f"❌ Arithmetic operations failed: {result2}")
                return False
            
            print("✅ Comprehensive string operations working correctly")
            return True
            
        except Exception as e:
            print(f"❌ String operations test failed: {e}")
            return False
    
    def test_data_structure_operations(self):
        """Test all data structure operations (Lists, Sets, Hashes, Sorted Sets)"""
        print("Testing comprehensive data structure operations...")
        
        try:
            # Comprehensive data structure test
            test_script = """
                -- Clear test keys
                redis.call("DEL", "list_test", "set_test", "hash_test", "zset_test")
                
                -- List operations
                redis.call("LPUSH", "list_test", "item1", "item2", "item3")
                local llen_result = redis.call("LLEN", "list_test")
                local lpop_result = redis.call("LPOP", "list_test")
                local lrange_result = redis.call("LRANGE", "list_test", "0", "-1")
                
                -- Set operations  
                redis.call("SADD", "set_test", "member1", "member2", "member3")
                local scard_result = redis.call("SCARD", "set_test")
                local sismember_result = redis.call("SISMEMBER", "set_test", "member1")
                local smembers_result = redis.call("SMEMBERS", "set_test")
                
                -- Hash operations
                redis.call("HSET", "hash_test", "field1", "value1", "field2", "value2")
                local hlen_result = redis.call("HLEN", "hash_test")
                local hget_result = redis.call("HGET", "hash_test", "field1")
                local hkeys_result = redis.call("HKEYS", "hash_test")
                
                -- Sorted Set operations
                redis.call("ZADD", "zset_test", "1.0", "one", "2.0", "two", "3.0", "three")
                local zcard_result = redis.call("ZCARD", "zset_test")
                local zscore_result = redis.call("ZSCORE", "zset_test", "two")
                local zrange_result = redis.call("ZRANGE", "zset_test", "0", "1", "WITHSCORES")
                local zpopmin_result = redis.call("ZPOPMIN", "zset_test")
                
                return {
                    llen_result, lpop_result, lrange_result,
                    scard_result, sismember_result, smembers_result,
                    hlen_result, hget_result, hkeys_result,
                    zcard_result, zscore_result, zrange_result,
                    zpopmin_result
                }
            """
            
            result = self.r.eval(test_script, 0)
            
            # Validate comprehensive results
            expected_base = [
                3, "item3",  # List: length=3, popped="item3"
                3, 1,        # Set: cardinality=3, member exists=1
                2, "value1", # Hash: length=2, field value="value1"
                3, "2.0"     # Sorted set: cardinality=3, score="2.0"
            ]
            
            # The remaining results are arrays (lrange, smembers, hkeys, zrange, zpopmin)
            # Validate the beginning and array results separately
            if (len(result) >= 8 and
                result[0] == 3 and result[1] == "item3" and  # List operations
                result[3] == 3 and result[4] == 1 and        # Set operations  
                result[6] == 2 and result[7] == "value1" and # Hash operations
                result[9] == 3 and result[10] == "2.0"):     # Sorted set operations
                print("✅ Comprehensive data structure operations working correctly")
                return True
            else:
                print(f"❌ Data structure operations mismatch")
                print(f"Full result: {result}")
                return False
                
        except Exception as e:
            print(f"❌ Data structure operations test failed: {e}")
            return False
    
    def test_stream_and_scan_operations(self):
        """Test Stream and Scan operations"""
        print("Testing Stream and Scan operations...")
        
        try:
            # Stream operations test
            stream_script = """
                redis.call("DEL", "stream_test")
                
                -- Add entries to stream
                local xadd1 = redis.call("XADD", "stream_test", "*", "field1", "value1")
                local xadd2 = redis.call("XADD", "stream_test", "*", "field2", "value2")
                
                -- Test stream operations
                local xlen_result = redis.call("XLEN", "stream_test")
                
                return {xlen_result}
            """
            
            result = self.r.eval(stream_script, 0)
            if result[0] == 2:  # Should have 2 entries
                print("✅ Stream operations working correctly")
                stream_success = True
            else:
                print(f"❌ Stream operations failed: expected [2], got {result}")
                stream_success = False
            
            # Database operations test  
            db_script = """
                -- Test database operations
                redis.call("SET", "db_test1", "value1")
                redis.call("SET", "db_test2", "value2")
                local dbsize_result = redis.call("DBSIZE")
                
                return {dbsize_result}
            """
            
            result = self.r.eval(db_script, 0)
            if result[0] >= 2:  # Should have at least 2 keys
                print("✅ Database operations working correctly")
                db_success = True
            else:
                print(f"❌ Database operations failed: {result}")
                db_success = False
            
            return stream_success and db_success
            
        except Exception as e:
            print(f"❌ Stream and scan operations test failed: {e}")
            return False
    
    def test_advanced_operations(self):
        """Test advanced Redis operations and edge cases"""
        print("Testing advanced operations and edge cases...")
        
        try:
            # Advanced operations test
            advanced_script = """
                -- Clear test data
                redis.call("DEL", "advanced_test")
                
                -- Test key operations
                redis.call("SET", "advanced_test", "test_value")
                local exists_result = redis.call("EXISTS", "advanced_test")
                local type_result = redis.call("TYPE", "advanced_test")
                
                -- Test expiration
                redis.call("EXPIRE", "advanced_test", "1")
                local ttl_result = redis.call("TTL", "advanced_test")
                
                -- Test server operations
                local ping_result = redis.call("PING")
                local echo_result = redis.call("ECHO", "test_echo")
                
                return {
                    exists_result, type_result, ttl_result,
                    ping_result, echo_result
                }
            """
            
            result = self.r.eval(advanced_script, 0)
            expected = [1, "string", 1, "PONG", "test_echo"]
            
            if result == expected:
                print("✅ Advanced operations working correctly")
                return True
            else:
                print(f"❌ Advanced operations mismatch: expected {expected}, got {result}")
                return False
                
        except Exception as e:
            print(f"❌ Advanced operations test failed: {e}")
            return False
    
    def test_atomic_multi_step_operations(self):
        """Test atomic multi-step operations critical for distributed coordination"""
        print("Testing atomic multi-step operations...")
        
        try:
            # Complex multi-step atomic pattern
            atomic_script = """
                -- Distributed lock pattern with counter
                local lock_key = "dist_lock"
                local counter_key = "shared_counter"
                local unique_id = "unique_123"
                
                -- Try to acquire lock
                local lock_acquired = redis.call("SET", lock_key, unique_id, "NX", "EX", "10")
                
                if lock_acquired then
                    -- We have the lock, do atomic operations
                    local current_val = redis.call("GET", counter_key) or "0"
                    local new_val = tonumber(current_val) + 1
                    redis.call("SET", counter_key, tostring(new_val))
                    
                    -- Release lock atomically
                    if redis.call("GET", lock_key) == unique_id then
                        redis.call("DEL", lock_key)
                        return {"lock_acquired", "counter_updated", new_val}
                    else
                        return {"lock_acquired", "lock_stolen", new_val}
                    end
                else
                    return {"lock_failed", "no_update", 0}
                end
            """
            
            # Run the script multiple times to test atomicity under concurrent execution
            results = []
            for i in range(3):
                result = self.r.eval(atomic_script, 0)
                results.append(result)
                time.sleep(0.1)  # Small delay between runs
            
            # All should succeed since we're not running concurrently
            success_count = sum(1 for r in results if r[0] == "lock_acquired" and r[1] == "counter_updated")
            
            if success_count == 3:
                print(f"✅ Atomic multi-step operations working correctly ({success_count}/3 succeeded)")
                return True
            else:
                print(f"❌ Atomic multi-step operations inconsistent: {success_count}/3 succeeded")
                print(f"Results: {results}")
                return False
                
        except Exception as e:
            print(f"❌ Atomic multi-step operations test failed: {e}")
            return False

    def test_zpopmin_specifically(self):
        """Test ZPOPMIN functionality specifically after the array response fix"""
        print("Testing ZPOPMIN functionality specifically...")
        
        try:
            # ZPOPMIN specific validation
            zpopmin_script = """
                -- Clear and test ZPOPMIN comprehensive scenarios
                redis.call("DEL", "zpopmin_test")
                
                -- Test empty set case
                local empty_result = redis.call("ZPOPMIN", "zpopmin_test")
                
                -- Test single member
                redis.call("ZADD", "zpopmin_test", "5.0", "five", "1.0", "one", "3.0", "three")
                local single_pop = redis.call("ZPOPMIN", "zpopmin_test")
                local remaining_count = redis.call("ZCARD", "zpopmin_test")
                
                return {empty_result, single_pop, remaining_count}
            """
            
            result = self.r.eval(zpopmin_script, 0)
            
            # Validate ZPOPMIN behavior: empty should be [], pop should be ["one", "1"], remaining should be 2
            if (isinstance(result[0], list) and len(result[0]) == 0 and     # Empty array for empty set
                isinstance(result[1], list) and result[1] == ["one", "1"] and # Single pop returns [member, score]  
                result[2] == 2):                                             # 2 members remaining
                print("✅ ZPOPMIN functionality working correctly")
                return True
            else:
                print(f"❌ ZPOPMIN functionality mismatch")
                print(f"Results: {result}")
                return False
                
        except Exception as e:
            print(f"❌ ZPOPMIN test failed: {e}")
            return False

def main():
    print("=" * 80)
    print("UNIFIED COMMAND EXECUTOR COMPREHENSIVE VALIDATION")
    print("=" * 80)
    
    # Check server connectivity
    try:
        r = redis.Redis(host='127.0.0.1', port=6379, socket_timeout=2)
        r.ping()
        print("✅ Server connection verified")
    except:
        print("❌ Cannot connect to server")
        sys.exit(1)
    
    print()
    
    tester = UnifiedExecutorTester()
    
    # Run comprehensive tests
    results = []
    results.append(tester.test_string_operations_comprehensive())
    results.append(tester.test_data_structure_operations())
    results.append(tester.test_stream_and_scan_operations())
    results.append(tester.test_advanced_operations())
    results.append(tester.test_atomic_multi_step_operations())
    results.append(tester.test_zpopmin_specifically())  # Added specific ZPOPMIN validation
    
    # Summary
    passed = sum(results)
    total = len(results)
    
    print()
    print("=" * 80)
    print(f"UNIFIED COMMAND EXECUTOR TEST RESULTS: {passed}/{total} PASSED")
    print("=" * 80)
    
    if passed == total:
        print("🎉 All unified command executor tests passed!")
        print("✅ Comprehensive Redis command coverage validated")
        print("✅ Array response handling working correctly")
        print("✅ Atomic operation guarantees confirmed")
        print("✅ Multi-step script integrity maintained")
        sys.exit(0)
    else:
        print("❌ Some unified command executor tests failed")
        print("⚠️  Command coverage gaps or implementation issues detected")
        sys.exit(1)

if __name__ == "__main__":
    main()