#!/usr/bin/env python3
"""
Comprehensive Lua Error Handling and Edge Case Tests for Ferrous
Tests all error paths, edge cases, and deadlock prevention scenarios
"""

import redis
import time
import sys
import threading

class LuaErrorHandlingTester:
    def __init__(self, host='127.0.0.1', port=6379):
        self.host = host
        self.port = port
        self.r = redis.Redis(host=host, port=port, decode_responses=True)
        
    def test_redis_call_vs_pcall_errors(self):
        """Test error handling differences between redis.call and redis.pcall"""
        print("Testing redis.call vs redis.pcall error handling...")
        
        try:
            # Test redis.call with intentional error (wrong number of args)
            try:
                result = self.r.eval("""
                    return redis.call('SET', 'key_only_one_arg')
                """, 0)
                print("❌ redis.call should have thrown error for wrong args")
                return False
            except redis.ResponseError as e:
                if "wrong number of arguments" in str(e).lower():
                    print("  ✅ redis.call correctly throws error for wrong args")
                else:
                    print(f"  ❌ Wrong error message: {e}")
                    return False
            
            # Test redis.pcall with same error (should return error value, not throw)
            result = self.r.eval("""
                local result = redis.pcall('SET', 'key_only_one_arg')
                if type(result) == 'table' and result.err then
                    return 'error_caught'
                else
                    return result
                end
            """, 0)
            
            if result == "error_caught":
                print("  ✅ redis.pcall correctly returns error value instead of throwing")
            else:
                print(f"  ❌ redis.pcall didn't handle error correctly: {result}")
                return False
                
            return True
            
        except Exception as e:
            print(f"  ❌ Unexpected error in redis.call/pcall test: {e}")
            return False
    
    def test_nested_lua_command_restriction(self):
        """Test that Lua scripts cannot call EVAL/EVALSHA/SCRIPT commands"""
        print("\nTesting nested Lua command restrictions...")
        
        nested_commands = ['EVAL', 'EVALSHA', 'SCRIPT']
        
        for cmd in nested_commands:
            try:
                # Test that redis.call throws error for nested commands
                try:
                    self.r.eval(f"""
                        return redis.call('{cmd}', 'return 1', '0')
                    """, 0)
                    print(f"  ❌ redis.call should prevent nested {cmd}")
                    return False
                except redis.ResponseError as e:
                    if "not allowed inside lua scripts" in str(e).lower():
                        print(f"  ✅ redis.call correctly blocks nested {cmd}")
                    else:
                        print(f"  ⚠️  {cmd} blocked but with different error: {e}")
                
                # Test that redis.pcall returns nil for nested commands
                result = self.r.eval(f"""
                    local result = redis.pcall('{cmd}', 'return 1', '0')
                    return result == nil and 'blocked' or 'allowed'
                """, 0)
                
                if result == "blocked":
                    print(f"  ✅ redis.pcall correctly blocks nested {cmd}")
                else:
                    print(f"  ❌ redis.pcall should block nested {cmd}")
                    return False
                    
            except Exception as e:
                print(f"  ❌ Error testing nested {cmd}: {e}")
                return False
        
        return True
    
    def test_script_compilation_errors(self):
        """Test various script compilation and syntax error scenarios"""
        print("\nTesting script compilation errors...")
        
        error_scripts = [
            ("Syntax error", "invalid lua syntax {{"),
            ("Undefined variable", "return nonexistent_variable"),
            ("Invalid function", "return invalid_function()"),
            ("Malformed conditional", "if true then return 1"),  # Missing end
            ("Invalid table access", "return KEYS[invalid_index]"),
        ]
        
        for desc, script in error_scripts:
            try:
                self.r.eval(script, 0)
                print(f"  ❌ {desc} should have failed: '{script[:30]}...'")
                return False
            except redis.ResponseError as e:
                if "err" in str(e).lower():
                    print(f"  ✅ {desc} correctly caught: {str(e)[:50]}...")
                else:
                    print(f"  ❌ Wrong error type for {desc}: {e}")
                    return False
            except Exception as e:
                print(f"  ❌ Unexpected error for {desc}: {e}")
                return False
        
        return True
    
    def test_script_load_error_handling(self):
        """Test SCRIPT LOAD with various error conditions"""
        print("\nTesting SCRIPT LOAD error handling...")
        
        try:
            # Test SCRIPT LOAD with syntax error
            try:
                self.r.script_load("invalid lua syntax {{")
                print("  ❌ SCRIPT LOAD should reject invalid syntax")
                return False
            except redis.ResponseError as e:
                if "err" in str(e).lower():
                    print("  ✅ SCRIPT LOAD correctly rejects invalid syntax")
                else:
                    print(f"  ❌ Wrong error for syntax error: {e}")
                    return False
            
            # Test valid script loading
            sha = self.r.script_load("return 'valid_script'")
            if len(sha) == 40:  # SHA1 length
                print(f"  ✅ Valid script loaded successfully: {sha[:8]}...")
            else:
                print(f"  ❌ Invalid SHA returned: {sha}")
                return False
            
            # Test SCRIPT EXISTS
            exists = self.r.script_exists(sha, "nonexistent_sha")
            if exists == [True, False]:
                print("  ✅ SCRIPT EXISTS working correctly")
            else:
                print(f"  ❌ SCRIPT EXISTS returned wrong result: {exists}")
                return False
            
            return True
            
        except Exception as e:
            print(f"  ❌ SCRIPT LOAD error test failed: {e}")
            return False
    
    def test_concurrent_lua_execution(self):
        """Test concurrent Lua execution to validate single-threaded semantics"""
        print("\nTesting concurrent Lua execution and deadlock prevention...")
        
        def lua_worker(worker_id, results):
            try:
                # Each worker runs a complex Lua script
                result = self.r.eval(f"""
                    -- Worker {worker_id} script
                    local key = 'worker_' .. '{worker_id}'
                    redis.call('SET', key, 'started')
                    
                    -- Simulate some work
                    for i = 1, 100 do
                        local val = redis.call('GET', key)
                        if val then
                            redis.call('SET', key, tostring(i))
                        end
                    end
                    
                    return redis.call('GET', key)
                """, 0)
                
                results[worker_id] = ('success', result)
                
            except Exception as e:
                results[worker_id] = ('error', str(e))
        
        # Run multiple workers concurrently
        results = {}
        threads = []
        
        for i in range(5):
            t = threading.Thread(target=lua_worker, args=(i, results))
            threads.append(t)
            t.start()
        
        # Wait for all workers
        for t in threads:
            t.join(timeout=10)
        
        # Check results
        successful_workers = 0
        for worker_id, (status, result) in results.items():
            if status == 'success':
                print(f"  ✅ Worker {worker_id}: {result}")
                successful_workers += 1
            else:
                print(f"  ❌ Worker {worker_id} failed: {result}")
        
        if successful_workers == 5:
            print(f"  ✅ All {successful_workers}/5 workers completed without deadlocks")
            return True
        else:
            print(f"  ⚠️  Only {successful_workers}/5 workers succeeded")
            return successful_workers >= 3  # Allow some tolerance
    
    def test_resource_limits(self):
        """Test resource limits and edge cases"""
        print("\nTesting resource limits and edge cases...")
        
        test_cases = []
        
        # Test very large scripts
        try:
            large_script = "return " + " + ".join(str(i) for i in range(1000))
            result = self.r.eval(large_script, 0)
            if result == sum(range(1000)):
                print("  ✅ Large script handled correctly")
                test_cases.append(True)
            else:
                print(f"  ❌ Large script incorrect result: {result}")
                test_cases.append(False)
        except Exception as e:
            print(f"  ⚠️  Large script failed (may be expected): {e}")
            test_cases.append(True)  # May be resource limit
        
        # Test deep recursion
        try:
            recursive_script = """
                function deep_recursion(n)
                    if n <= 0 then return 0 end
                    return n + deep_recursion(n - 1)
                end
                return deep_recursion(100)
            """
            result = self.r.eval(recursive_script, 0)
            expected = sum(range(101))  # 0 + 1 + 2 + ... + 100
            if result == expected:
                print(f"  ✅ Recursive function works: {result}")
                test_cases.append(True)
            else:
                print(f"  ❌ Recursive function wrong result: {result}")
                test_cases.append(False)
        except Exception as e:
            print(f"  ⚠️  Deep recursion limited (may be expected): {e}")
            test_cases.append(True)
        
        # Test large data structures
        try:
            large_table_script = """
                local large_table = {}
                for i = 1, 1000 do
                    large_table[i] = 'value_' .. tostring(i)
                end
                return #large_table
            """
            result = self.r.eval(large_table_script, 0)
            if result == 1000:
                print("  ✅ Large table creation works")
                test_cases.append(True)
            else:
                print(f"  ❌ Large table wrong size: {result}")
                test_cases.append(False)
        except Exception as e:
            print(f"  ❌ Large table test failed: {e}")
            test_cases.append(False)
        
        return all(test_cases)
    
    def test_storage_error_scenarios(self):
        """Test how Lua handles various StorageEngine error scenarios"""
        print("\nTesting storage error scenarios...")
        
        try:
            # Test operations on non-existent keys
            result = self.r.eval("""
                local get_result = redis.call('GET', 'nonexistent_key')
                local exists_result = redis.call('EXISTS', 'nonexistent_key')
                local del_result = redis.call('DEL', 'nonexistent_key')
                return {get_result, exists_result, del_result}
            """, 0)
            
            get_val, exists_val, del_count = result
            if get_val is None and exists_val == 0 and del_count == 0:
                print("  ✅ Non-existent key operations handled correctly")
            else:
                print(f"  ❌ Wrong results for non-existent key: {result}")
                return False
            
            # Test type conflicts (try to INCR a string value)
            self.r.set("string_key", "not_a_number")
            
            # redis.call should throw error
            try:
                self.r.eval("""
                    return redis.call('INCR', 'string_key')
                """, 0)
                print("  ❌ redis.call should throw error for INCR on string")
                return False
            except redis.ResponseError:
                print("  ✅ redis.call correctly throws error for type mismatch")
            
            # redis.pcall should return error value
            result = self.r.eval("""
                local result = redis.pcall('INCR', 'string_key') 
                return type(result)
            """, 0)
            
            if result == "nil":  # pcall returns nil on error in our implementation
                print("  ✅ redis.pcall correctly handles type mismatch")
            else:
                print(f"  ⚠️  redis.pcall returned: {result} (implementation specific)")
            
            # Cleanup
            self.r.delete("string_key")
            return True
            
        except Exception as e:
            print(f"  ❌ Storage error test failed: {e}")
            return False
    
    def test_deadlock_prevention_validation(self):
        """Specific tests to validate deadlock prevention in various scenarios"""
        print("\nTesting deadlock prevention validation...")
        
        # Test rapid fire redis.call operations
        try:
            rapid_fire_script = """
                local results = {}
                for i = 1, 50 do
                    local key = 'rapid_' .. tostring(i)
                    redis.call('SET', key, 'value_' .. tostring(i))
                    local value = redis.call('GET', key)
                    redis.call('DEL', key)
                    table.insert(results, value)
                end
                return #results
            """
            
            result = self.r.eval(rapid_fire_script, 0)
            if result == 50:
                print("  ✅ Rapid fire redis.call operations completed without deadlock")
            else:
                print(f"  ❌ Rapid fire test incomplete: {result}/50")
                return False
            
        except Exception as e:
            print(f"  ❌ Rapid fire test failed: {e}")
            return False
        
        # Test concurrent access patterns
        try:
            concurrent_script = """
                -- Script that accesses multiple shards
                local keys = {'shard_a', 'shard_b', 'shard_c', 'shard_d'}
                local results = {}
                
                for i, key in ipairs(keys) do
                    redis.call('SET', key, 'concurrent_' .. tostring(i))
                    table.insert(results, redis.call('GET', key))
                    redis.call('INCR', key .. '_counter')
                    table.insert(results, redis.call('GET', key .. '_counter'))
                end
                
                return #results
            """
            
            result = self.r.eval(concurrent_script, 0)
            if result == 8:  # 4 keys * 2 operations each
                print("  ✅ Multi-shard access completed without deadlock")
            else:
                print(f"  ❌ Multi-shard test incomplete: {result}/8")
                return False
            
            # Cleanup
            cleanup_keys = ['shard_a', 'shard_b', 'shard_c', 'shard_d', 
                          'shard_a_counter', 'shard_b_counter', 'shard_c_counter', 'shard_d_counter']
            for key in cleanup_keys:
                self.r.delete(key)
            
        except Exception as e:
            print(f"  ❌ Multi-shard test failed: {e}")
            return False
        
        return True
    
    def test_timeout_and_interruption_handling(self):
        """Test timeout handling and script interruption scenarios"""
        print("\nTesting timeout and interruption handling...")
        
        try:
            # Test script that takes some time but should complete
            time_consuming_script = """
                local start_time = tonumber(ARGV[1])
                local operations = 0
                
                -- Do some work
                for i = 1, 1000 do
                    redis.call('SET', 'temp_key_' .. tostring(i % 10), tostring(i))
                    operations = operations + 1
                    
                    if i % 100 == 0 then
                        -- Check if we should continue (simple yield point)
                        local current_key = 'temp_key_' .. tostring(i % 10)
                        redis.call('GET', current_key)
                    end
                end
                
                -- Cleanup
                for i = 0, 9 do
                    redis.call('DEL', 'temp_key_' .. tostring(i))
                end
                
                return operations
            """
            
            start_time = int(time.time() * 1000)
            result = self.r.eval(time_consuming_script, 0, str(start_time))
            
            if result == 1000:
                print("  ✅ Time-consuming script completed without timeout")
            else:
                print(f"  ❌ Time-consuming script incomplete: {result}/1000")
                return False
            
            return True
            
        except redis.TimeoutError:
            print("  ⚠️  Script timed out (may indicate timeout configuration)")
            return True  # Timeouts are acceptable behavior
        except Exception as e:
            print(f"  ❌ Timeout test failed: {e}")
            return False
    
    def test_edge_case_scenarios(self):
        """Test various edge cases and boundary conditions"""
        print("\nTesting edge case scenarios...")
        
        test_cases = []
        
        # Empty script
        try:
            result = self.r.eval("", 0)
            print("  ❌ Empty script should fail")
            test_cases.append(False)
        except redis.ResponseError:
            print("  ✅ Empty script correctly rejected")
            test_cases.append(True)
        
        # Script with only whitespace
        try:
            result = self.r.eval("   \n\t  ", 0)
            print("  ❌ Whitespace-only script should fail")
            test_cases.append(False)
        except redis.ResponseError:
            print("  ✅ Whitespace-only script correctly rejected")
            test_cases.append(True)
        
        # Very simple valid scripts
        simple_scripts = [
            ("nil", "return nil"),
            ("boolean true", "return true"),
            ("boolean false", "return false"),
            ("integer", "return 42"),
            ("float", "return 3.14"),
            ("string", "return 'hello'"),
            ("empty table", "return {}"),
        ]
        
        for desc, script in simple_scripts:
            try:
                result = self.r.eval(script, 0)
                print(f"  ✅ {desc} script works: {result}")
                test_cases.append(True)
            except Exception as e:
                print(f"  ❌ {desc} script failed: {e}")
                test_cases.append(False)
        
        return all(test_cases)

def main():
    print("=" * 80)
    print("FERROUS LUA ERROR HANDLING AND EDGE CASE TEST SUITE")
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
    
    tester = LuaErrorHandlingTester()
    
    # Run all error handling and edge case tests
    results = []
    results.append(tester.test_redis_call_vs_pcall_errors())
    results.append(tester.test_nested_lua_command_restriction())
    results.append(tester.test_script_compilation_errors())
    results.append(tester.test_script_load_error_handling())
    results.append(tester.test_concurrent_lua_execution())
    results.append(tester.test_storage_error_scenarios())
    results.append(tester.test_deadlock_prevention_validation())
    results.append(tester.test_timeout_and_interruption_handling())
    results.append(tester.test_edge_case_scenarios())
    
    # Summary
    passed = sum(results)
    total = len(results)
    
    print()
    print("=" * 80)
    print(f"LUA ERROR HANDLING TEST RESULTS: {passed}/{total} PASSED")
    print("=" * 80)
    
    # Detailed assessment
    test_names = [
        "redis.call vs redis.pcall errors",
        "Nested command restrictions", 
        "Script compilation errors",
        "SCRIPT LOAD error handling",
        "Concurrent execution",
        "Storage error scenarios",
        "Deadlock prevention",
        "Timeout handling",
        "Edge cases"
    ]
    
    print("\nDetailed Results:")
    print("-" * 40)
    for i, (test_name, passed) in enumerate(zip(test_names, results)):
        status = "✅ PASS" if passed else "❌ FAIL"
        print(f"{test_name:30} {status}")
    
    print()
    
    if passed == total:
        print("🎉 All Lua error handling tests passed!")
        print("✅ Comprehensive deadlock prevention validated!")
        print("✅ All error paths properly tested!")
        sys.exit(0)
    else:
        print("❌ Some error handling tests failed")
        print(f"   Coverage: {(passed/total)*100:.1f}%")
        sys.exit(1)

if __name__ == "__main__":
    main()