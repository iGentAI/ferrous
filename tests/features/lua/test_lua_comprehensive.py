#!/usr/bin/env python3
"""
Comprehensive Lua scripting tests for Ferrous
Tests EVAL, EVALSHA, SCRIPT commands, and specific patterns like atomic lock release
"""

import redis
import time
import sys
import threading
import socket

class LuaScriptTester:
    def __init__(self, host='127.0.0.1', port=6379):
        self.host = host
        self.port = port
        self.r = redis.Redis(host=host, port=port, decode_responses=True)
        
    def test_basic_eval(self):
        """Test basic EVAL functionality"""
        print("Testing basic EVAL...")
        
        try:
            # Simple script that returns a value
            result = self.r.eval("return 42", 0)
            if result == 42:
                print("✅ Basic EVAL working")
                return True
            else:
                print(f"❌ Basic EVAL returned wrong value: {result}")
                return False
        except Exception as e:
            print(f"❌ Basic EVAL failed: {e}")
            return False
            
    def test_eval_with_timeout(self):
        """Test if EVAL hangs as reported"""
        print("\nTesting EVAL with timeout...")
        
        def run_eval():
            try:
                # Try a simple eval with short socket timeout
                r_timeout = redis.Redis(host=self.host, port=self.port, 
                                      decode_responses=True, socket_timeout=5)
                result = r_timeout.eval("return 'not hanging'", 0)
                return True, result
            except redis.TimeoutError:
                return False, "TIMEOUT"
            except Exception as e:
                return False, str(e)
                
        # Run in thread to detect hanging
        thread = threading.Thread(target=lambda: setattr(thread, 'result', run_eval()))
        thread.start()
        thread.join(timeout=10)
        
        if thread.is_alive():
            print("❌ EVAL command hung indefinitely!")
            return False
        else:
            success, result = thread.result
            if success:
                print(f"✅ EVAL completed without hanging: {result}")
                return True
            else:
                print(f"❌ EVAL failed: {result}")
                return False
                
    def test_atomic_lock_release(self):
        """Test the specific atomic lock release pattern from the report"""
        print("\nTesting atomic lock release pattern...")
        
        # The exact script from the report
        lock_release_script = """
            if redis.call("get", KEYS[1]) == ARGV[1] then
                return redis.call("del", KEYS[1])
            else
                return 0
            end
        """
        
        try:
            # Set up a lock
            lock_key = "test_lock"
            lock_value = "unique_id_123"
            
            # Acquire the lock
            self.r.set(lock_key, lock_value)
            
            # Test releasing with correct value
            result = self.r.eval(lock_release_script, 1, lock_key, lock_value)
            if result == 1:
                print("✅ Lock released successfully with correct value")
            else:
                print(f"❌ Lock release failed with correct value: returned {result}")
                return False
                
            # Verify lock is gone
            if self.r.get(lock_key) is None:
                print("✅ Lock was properly deleted")
            else:
                print("❌ Lock still exists after release")
                return False
                
            # Set lock again for wrong value test
            self.r.set(lock_key, lock_value)
            
            # Test releasing with wrong value
            result = self.r.eval(lock_release_script, 1, lock_key, "wrong_value")
            if result == 0:
                print("✅ Lock release correctly rejected wrong value")
            else:
                print(f"❌ Lock release with wrong value returned {result} (expected 0)")
                return False
                
            # Verify lock is still there
            if self.r.get(lock_key) == lock_value:
                print("✅ Lock preserved when wrong value provided")
                return True
            else:
                print("❌ Lock was incorrectly modified")
                return False
                
        except Exception as e:
            print(f"❌ Atomic lock release test failed: {e}")
            return False
        finally:
            # Cleanup
            try:
                self.r.delete(lock_key)
            except:
                pass
                
    def test_script_load(self):
        """Test SCRIPT LOAD functionality"""
        print("\nTesting SCRIPT LOAD...")
        
        try:
            # Load a simple script
            script = "return KEYS[1] .. ARGV[1]"
            sha = self.r.script_load(script)
            
            if sha and len(sha) == 40:  # SHA1 is 40 chars
                print(f"✅ SCRIPT LOAD returned SHA: {sha}")
            else:
                print(f"❌ SCRIPT LOAD returned invalid SHA: {sha}")
                return False
                
            # Test EVALSHA with loaded script
            result = self.r.evalsha(sha, 1, "hello", "world")
            if result == "helloworld":
                print("✅ EVALSHA executed successfully")
                return True
            else:
                print(f"❌ EVALSHA returned wrong result: {result}")
                return False
                
        except redis.ResponseError as e:
            if "NOSCRIPT" in str(e):
                print("❌ Script was not properly cached")
                return False
            else:
                print(f"❌ SCRIPT LOAD test failed with Redis error: {e}")
                return False
        except redis.TimeoutError:
            print("❌ SCRIPT LOAD timed out - this confirms the hanging issue!")
            return False
        except Exception as e:
            print(f"❌ SCRIPT LOAD test failed: {e}")
            return False
            
    def test_eval_with_redis_calls(self):
        """Test EVAL with various redis.call operations"""
        print("\nTesting EVAL with redis.call operations...")
        
        test_scripts = [
            # Test SET/GET
            ("return redis.call('SET', KEYS[1], ARGV[1])", 
             lambda r: r == "OK", "SET operation"),
             
            # Test GET after SET
            ("redis.call('SET', KEYS[1], ARGV[1]); return redis.call('GET', KEYS[1])",
             lambda r: r == "test_value", "SET then GET"),
             
            # Test INCR
            ("redis.call('SET', KEYS[1], '10'); return redis.call('INCR', KEYS[1])",
             lambda r: r == 11, "INCR operation"),
             
            # Test EXISTS
            ("redis.call('SET', KEYS[1], 'value'); return redis.call('EXISTS', KEYS[1])",
             lambda r: r == 1, "EXISTS operation"),
        ]
        
        all_passed = True
        for script, validator, desc in test_scripts:
            try:
                result = self.r.eval(script, 1, "lua_test_key", "test_value")
                if validator(result):
                    print(f"  ✅ {desc}: passed")
                else:
                    print(f"  ❌ {desc}: failed (result: {result})")
                    all_passed = False
            except Exception as e:
                print(f"  ❌ {desc}: exception - {e}")
                all_passed = False
                
        # Cleanup
        try:
            self.r.delete("lua_test_key")
        except:
            pass
            
        return all_passed
        
    def test_script_error_handling(self):
        """Test Lua script error handling"""
        print("\nTesting Lua script error handling...")
        
        try:
            # Script with syntax error
            try:
                self.r.eval("invalid lua syntax {{", 0)
                print("❌ Syntax error not caught")
                return False
            except redis.ResponseError as e:
                if "ERR" in str(e):
                    print("✅ Syntax errors properly reported")
                else:
                    print(f"❌ Unexpected error format: {e}")
                    return False
                    
            # Script with runtime error
            try:
                self.r.eval("return nonexistent_variable", 0)
                print("❌ Runtime error not caught")
                return False
            except redis.ResponseError as e:
                if "ERR" in str(e):
                    print("✅ Runtime errors properly reported")
                else:
                    print(f"❌ Unexpected error format: {e}")
                    return False
                    
            return True
            
        except Exception as e:
            print(f"❌ Error handling test failed: {e}")
            return False
            
    def test_keys_and_argv(self):
        """Test KEYS and ARGV array handling"""
        print("\nTesting KEYS and ARGV handling...")
        
        try:
            # Test multiple KEYS and ARGV
            script = """
                local result = {}
                for i, key in ipairs(KEYS) do
                    table.insert(result, key)
                end
                for i, arg in ipairs(ARGV) do
                    table.insert(result, arg)
                end
                return result
            """
            
            result = self.r.eval(script, 3, "key1", "key2", "key3", "arg1", "arg2")
            expected = ["key1", "key2", "key3", "arg1", "arg2"]
            
            if result == expected:
                print("✅ KEYS and ARGV arrays handled correctly")
                return True
            else:
                print(f"❌ KEYS/ARGV handling failed: {result} != {expected}")
                return False
                
        except Exception as e:
            print(f"❌ KEYS/ARGV test failed: {e}")
            return False

def main():
    print("=" * 70)
    print("FERROUS LUA SCRIPTING COMPREHENSIVE TEST SUITE")
    print("=" * 70)
    
    # Check if server is running
    try:
        r = redis.Redis(host='127.0.0.1', port=6379)
        r.ping()
        print("✅ Server connection verified")
    except:
        print("❌ Cannot connect to server")
        sys.exit(1)
        
    print()
    
    tester = LuaScriptTester()
    
    # Run tests
    results = []
    results.append(tester.test_basic_eval())
    results.append(tester.test_eval_with_timeout())
    results.append(tester.test_atomic_lock_release())
    results.append(tester.test_script_load())
    results.append(tester.test_eval_with_redis_calls())
    results.append(tester.test_script_error_handling())
    results.append(tester.test_keys_and_argv())
    
    # Summary
    passed = sum(results)
    total = len(results)
    
    print()
    print("=" * 70)
    print(f"LUA SCRIPTING TEST RESULTS: {passed}/{total} PASSED")
    print("=" * 70)
    
    if passed == total:
        print("🎉 All Lua scripting tests passed!")
        sys.exit(0)
    else:
        print("❌ Some Lua scripting tests failed")
        print("   This may confirm the reported Lua issues")
        sys.exit(1)

if __name__ == "__main__":
    main()