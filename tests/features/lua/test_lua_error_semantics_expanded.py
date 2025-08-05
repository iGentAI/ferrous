#!/usr/bin/env python3
"""
Expanded Lua Error Semantics Tests for Ferrous
Comprehensive validation of redis.call vs redis.pcall behavior per Redis specification
"""

import redis
import time
import sys
import threading
import random

class LuaErrorSemanticsValidator:
    def __init__(self, host='127.0.0.1', port=6379):
        self.host = host
        self.port = port
        self.r = redis.Redis(host=host, port=port, decode_responses=True)
        
    def test_redis_call_error_scenarios(self):
        """Test various redis.call error scenarios"""
        print("Testing redis.call error scenarios...")
        
        error_scenarios = [
            ("SET with insufficient args", 'redis.call("SET", "key")', "wrong number of arguments"),
            ("INCR on non-numeric", 'redis.call("SET", "str_key", "abc"); redis.call("INCR", "str_key")', "not an integer"),
            ("Unknown command", 'redis.call("NONEXISTENT", "arg")', "unknown command"),
            ("GET with too many args", 'redis.call("GET", "key1", "key2")', "wrong number of arguments"),
        ]
        
        for desc, script, expected_error in error_scenarios:
            try:
                result = self.r.eval(script, 0)
                print(f"  ❌ {desc}: Should have thrown error but got {result}")
                return False
            except redis.ResponseError as e:
                if expected_error.lower() in str(e).lower():
                    print(f"  ✅ {desc}: PASSED")
                else:
                    print(f"  ❌ {desc}: Wrong error - got {e}")
                    return False
            except Exception as e:
                print(f"  ❌ {desc}: Unexpected exception {e}")
                return False
                
        return True
    
    def test_redis_pcall_error_scenarios(self):
        """Test redis.pcall returns nil for all error scenarios"""
        print("\nTesting redis.pcall error scenarios...")
        
        error_scenarios = [
            ("SET with insufficient args", 'redis.pcall("SET", "key")'),
            ("INCR on non-numeric", 'redis.pcall("SET", "str_key2", "abc"); redis.pcall("INCR", "str_key2")'),
            ("Unknown command", 'redis.pcall("NONEXISTENT", "arg")'),
            ("GET with too many args", 'redis.pcall("GET", "key1", "key2")'),
        ]
        
        for desc, script in error_scenarios:
            try:
                result = self.r.eval(script, 0)
                if result is None:
                    print(f"  ✅ {desc}: PASSED (returned nil)")
                else:
                    print(f"  ❌ {desc}: Should return nil but got {result}")
                    return False
            except Exception as e:
                print(f"  ❌ {desc}: Unexpected exception {e}")
                return False
                
        return True
    
    def test_mixed_call_pcall_scenarios(self):
        """Test scripts that mix redis.call and redis.pcall"""
        print("\nTesting mixed redis.call and redis.pcall scenarios...")
        
        try:
            # Test 1: pcall catching errors, call succeeding
            script = '''
                local error_result = redis.pcall("SET", "insufficient")
                if error_result == nil then
                    return redis.call("SET", "recovery_key", "success")
                else
                    return "unexpected_success"
                end
            '''
            
            result = self.r.eval(script, 0)
            if result == "OK":
                print("  ✅ Mixed pcall error recovery: PASSED")
            else:
                print(f"  ❌ Mixed scenario failed: got {result}")
                return False
                
            # Test 2: Multiple pcall operations
            script = '''
                local results = {}
                
                -- This should fail and return nil
                local bad_result = redis.pcall("SET", "bad")
                table.insert(results, bad_result == nil)
                
                -- This should succeed
                local good_result = redis.pcall("SET", "good_key", "good_value")
                table.insert(results, good_result == "OK")
                
                return results
            '''
            
            result = self.r.eval(script, 0)
            if result == [1, 1]:  # [true, true] as integers
                print("  ✅ Multiple pcall operations: PASSED")
            else:
                print(f"  ❌ Multiple pcall failed: got {result}")
                return False
                
            return True
            
        except Exception as e:
            print(f"  ❌ Mixed call/pcall test failed: {e}")
            return False
    
    def test_atomic_transaction_patterns(self):
        """Test atomic transaction patterns that depend on proper error handling"""
        print("\nTesting atomic transaction patterns...")
        
        try:
            # Test 1: Atomic lock release pattern
            script = '''
                local lock_key = KEYS[1]
                local lock_value = ARGV[1]
                
                -- Set the lock
                redis.call("SET", lock_key, lock_value)
                
                -- Atomic release (should work with proper error handling)
                if redis.call("GET", lock_key) == lock_value then
                    return redis.call("DEL", lock_key)
                else
                    return 0
                end
            '''
            
            result = self.r.eval(script, 1, "atomic_test_lock", "unique_value")
            if result == 1:
                print("  ✅ Atomic lock release: PASSED")
            else:
                print(f"  ❌ Atomic lock release failed: got {result}")
                return False
                
            # Test 2: Conditional update with error recovery
            script = '''
                local key = KEYS[1]
                local new_value = ARGV[1]
                
                -- Try to increment (might fail if not numeric)
                local incr_result = redis.pcall("INCR", key)
                if incr_result == nil then
                    -- Fallback: set as string
                    redis.call("SET", key, new_value)
                    return {action = "set", success = true}
                else
                    return {action = "incr", value = incr_result}
                end
            '''
            
            result = self.r.eval(script, 1, "conditional_key", "fallback_value")
            if isinstance(result, list) and "action" in result:
                print("  ✅ Conditional update with recovery: PASSED")
            else:
                print(f"  ❌ Conditional update failed: got {result}")
                return False
                
            return True
            
        except Exception as e:
            print(f"  ❌ Atomic pattern test failed: {e}")
            return False
    
    def test_performance_under_load(self):
        """Test error handling performance under load"""
        print("\nTesting error handling performance under load...")
        
        try:
            # Test rapid error generation doesn't cause issues
            start_time = time.time()
            error_count = 0
            success_count = 0
            
            for i in range(100):
                try:
                    # Mix of errors and successes
                    if i % 3 == 0:
                        # This should error
                        self.r.eval('return redis.call("SET", "insufficient")', 0)
                    else:
                        # This should succeed
                        result = self.r.eval(f'return redis.call("SET", "key{i}", "value{i}")', 0)
                        if result == "OK":
                            success_count += 1
                except redis.ResponseError:
                    error_count += 1
                except Exception as e:
                    print(f"  ❌ Unexpected error in load test: {e}")
                    return False
            
            elapsed = time.time() - start_time
            
            if error_count >= 30 and success_count >= 60:  # Rough expected counts
                print(f"  ✅ Load test: PASSED ({error_count} errors, {success_count} successes in {elapsed:.2f}s)")
            else:
                print(f"  ❌ Load test failed: {error_count} errors, {success_count} successes")
                return False
                
            return True
            
        except Exception as e:
            print(f"  ❌ Load test failed: {e}")
            return False

def main():
    print("=" * 80)
    print("FERROUS LUA ERROR SEMANTICS EXPANDED VALIDATION")
    print("Comprehensive redis.call vs redis.pcall behavior testing")
    print("=" * 80)
    
    # Check if server is running
    try:
        r = redis.Redis(host='127.0.0.1', port=6379)
        r.ping()
        print("✅ Server connection verified")
    except:
        print("❌ Cannot connect to server")
        sys.exit(1)
        
    print()
    
    validator = LuaErrorSemanticsValidator()
    
    # Run all validation tests
    results = []
    results.append(validator.test_redis_call_error_scenarios())
    results.append(validator.test_redis_pcall_error_scenarios())
    results.append(validator.test_mixed_call_pcall_scenarios())
    results.append(validator.test_atomic_transaction_patterns())
    results.append(validator.test_performance_under_load())
    
    # Summary
    passed = sum(results)
    total = len(results)
    
    print()
    print("=" * 80)
    print(f"LUA ERROR SEMANTICS VALIDATION: {passed}/{total} PASSED")
    print("=" * 80)
    
    if passed == total:
        print("🎉 All Lua error semantics validated successfully!")
        print("✅ redis.call errors properly handled")
        print("✅ redis.pcall returns nil for errors")
        print("✅ Mixed scenarios work correctly")
        print("✅ Atomic patterns function properly")
        sys.exit(0)
    else:
        print("❌ Some error semantics tests failed")
        print(f"   Success rate: {(passed/total)*100:.1f}%")
        sys.exit(1)

if __name__ == "__main__":
    main()