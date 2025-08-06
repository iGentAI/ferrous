#!/usr/bin/env python3
"""
Comprehensive RESP and Lua 5.1 Conversion Validation Tests for Ferrous
Tests all the dragon fixes: boolean conversion, associative tables, float precision, etc.
"""

import redis
import time
import sys
import math

class RespLuaConversionValidator:
    def __init__(self, host='127.0.0.1', port=6379):
        self.host = host
        self.port = port
        self.r = redis.Redis(host=host, port=port, decode_responses=True)
        
    def test_boolean_conversions(self):
        """Test boolean conversion fixes - critical dragon fix"""
        print("Testing boolean conversions (CRITICAL DRAGON FIX)...")
        
        try:
            # Test 1: Boolean true should convert to 1
            result = self.r.eval("return true", 0)
            if result == 1:
                print("  ✅ Boolean true → Integer(1): PASSED")
            else:
                print(f"  ❌ Boolean true failed: got {result} (expected 1)")
                return False
                
            # Test 2: Boolean false should convert to 0 (not null!)
            result = self.r.eval("return false", 0)
            if result == 0:
                print("  ✅ Boolean false → Integer(0): PASSED (dragon fixed)")
            else:
                print(f"  ❌ Boolean false failed: got {result} (expected 0)")
                return False
                
            # Test 3: Boolean in conditional logic
            result = self.r.eval("local b = false; if b == false then return 'correct' else return 'wrong' end", 0)
            if result == "correct":
                print("  ✅ Boolean conditional logic: PASSED")
            else:
                print(f"  ❌ Boolean conditional failed: got {result}")
                return False
                
            return True
            
        except Exception as e:
            print(f"  ❌ Boolean conversion test failed: {e}")
            return False
    
    def test_associative_table_conversions(self):
        """Test associative table conversion fixes - critical data loss fix"""
        print("\nTesting associative table conversions (DATA LOSS DRAGON FIX)...")
        
        try:
            # Test 1: Simple associative table
            result = self.r.eval('local t = {}; t.name = "test"; t.value = 42; return t', 0)
            expected = ["name", "test", "value", 42]
            if result == expected:
                print("  ✅ Simple associative table: PASSED (data preserved)")
            else:
                print(f"  ❌ Simple associative table failed: got {result}")
                return False
                
            # Test 2: Complex associative table with mixed types
            result = self.r.eval('''
                local t = {}
                t.string_field = "hello"
                t.number_field = 3.14
                t.bool_field = true
                t.nil_field = nil
                return t
            ''', 0)
            
            # Should preserve all non-nil fields as key-value pairs
            if len(result) >= 6:  # At least 3 fields * 2 (key-value pairs)
                print("  ✅ Complex associative table: PASSED (all data preserved)")
            else:
                print(f"  ❌ Complex associative table failed: got {result}")
                return False
                
            # Test 3: Empty associative table
            result = self.r.eval('local t = {}; return t', 0)
            if result is None:  # Empty table should convert to null
                print("  ✅ Empty table: PASSED")
            else:
                print(f"  ❌ Empty table failed: got {result}")
                return False
                
            return True
            
        except Exception as e:
            print(f"  ❌ Associative table test failed: {e}")
            return False
    
    def test_sequential_vs_associative_tables(self):
        """Test intelligent table detection between sequential and associative"""
        print("\nTesting sequential vs associative table detection...")
        
        try:
            # Test 1: Pure sequential table (should be array)
            result = self.r.eval('return {1, 2, 3, "four"}', 0)
            expected = [1, 2, 3, "four"]
            if result == expected:
                print("  ✅ Sequential table (array): PASSED")
            else:
                print(f"  ❌ Sequential table failed: got {result}")
                return False
                
            # Test 2: Pure associative table (should be key-value pairs)
            result = self.r.eval('return {name="test", id=123}', 0)
            if "name" in result and "test" in result and "id" in result and 123 in result:
                print("  ✅ Pure associative table: PASSED")
            else:
                print(f"  ❌ Pure associative table failed: got {result}")
                return False
                
            # Test 3: Mixed table (should handle gracefully)
            result = self.r.eval('local t = {1, 2}; t.name = "mixed"; return t', 0)
            if isinstance(result, list) and len(result) > 0:
                print("  ✅ Mixed table: PASSED (handled gracefully)")
            else:
                print(f"  ❌ Mixed table failed: got {result}")
                return False
                
            return True
            
        except Exception as e:
            print(f"  ❌ Table detection test failed: {e}")
            return False
    
    def test_float_precision_and_edge_cases(self):
        """Test float precision improvements and IEEE 754 edge cases"""
        print("\nTesting float precision and IEEE 754 edge cases...")
        
        try:
            # Test 1: High precision float
            result = self.r.eval('return 3.141592653589793', 0)
            if isinstance(result, str) and "3.14159265358979" in result:
                print(f"  ✅ High precision float: PASSED ({result})")
            else:
                print(f"  ❌ High precision float failed: got {result}")
                return False
                
            # Test 2: Positive infinity
            result = self.r.eval('return 1/0', 0)
            if result == "inf":
                print("  ✅ Positive infinity: PASSED")
            else:
                print(f"  ❌ Positive infinity failed: got {result}")
                return False
                
            # Test 3: Negative infinity
            result = self.r.eval('return -1/0', 0)
            if result == "-inf":
                print("  ✅ Negative infinity: PASSED")
            else:
                print(f"  ❌ Negative infinity failed: got {result}")
                return False
                
            # Test 4: NaN handling
            result = self.r.eval('return 0/0', 0)
            if result is None:  # NaN should convert to null
                print("  ✅ NaN handling: PASSED (converts to null)")
            else:
                print(f"  ❌ NaN handling failed: got {result}")
                return False
                
            # Test 5: Integer-valued floats
            result = self.r.eval('return 42.0', 0)
            if result == 42:  # Should convert to integer
                print("  ✅ Integer-valued float: PASSED")
            else:
                print(f"  ❌ Integer-valued float failed: got {result}")
                return False
                
            return True
            
        except Exception as e:
            print(f"  ❌ Float precision test failed: {e}")
            return False
    
    def test_error_handling_robustness(self):
        """Test comprehensive error handling after dragon fixes"""
        print("\nTesting error handling robustness...")
        
        try:
            # Test 1: redis.call error handling
            try:
                result = self.r.eval('return redis.call("SET", "insufficient_args")', 0)
                print("  ❌ redis.call should have thrown error")
                return False
            except redis.ResponseError as e:
                if "wrong number of arguments" in str(e):
                    print("  ✅ redis.call error handling: PASSED")
                else:
                    print(f"  ❌ Unexpected error message: {e}")
                    return False
                    
            # Test 2: redis.pcall error handling
            result = self.r.eval('return redis.pcall("SET", "insufficient_args")', 0)
            if result is None:
                print("  ✅ redis.pcall error handling: PASSED")
            else:
                print(f"  ❌ redis.pcall should return nil for errors: got {result}")
                return False
                
            # Test 3: Error handling with boolean conversion
            result = self.r.eval('return redis.pcall("GET", "nonexistent") == nil', 0)
            if result == 0:  # false should be 0
                print("  ✅ Error handling with boolean conversion: PASSED")
            else:
                print(f"  ❌ Boolean comparison failed: got {result}")
                return False
                
            return True
            
        except Exception as e:
            print(f"  ❌ Error handling test failed: {e}")
            return False
    
    def test_complex_lua_patterns(self):
        """Test complex Lua patterns that use multiple fixed features"""
        print("\nTesting complex Lua patterns with multiple fixes...")
        
        try:
            # Test 1: Atomic operation with proper error handling and boolean logic
            script = '''
                local key = KEYS[1]
                local value = ARGV[1]
                
                -- Set initial value
                redis.call("SET", key, value)
                
                -- Test boolean logic (should work with fixed conversions)
                local exists = redis.call("EXISTS", key) == 1
                if not exists then
                    return {success = false, reason = "key_not_found"}
                end
                
                -- Test associative table return (should preserve data)
                return {
                    success = true,
                    value = redis.call("GET", key),
                    exists = exists,
                    timestamp = 1234567890
                }
            '''
            
            result = self.r.eval(script, 1, "test_complex", "test_value")
            
            if isinstance(result, list) and len(result) >= 6:  # Key-value pairs
                print("  ✅ Complex atomic operation: PASSED")
            else:
                print(f"  ❌ Complex operation failed: got {result}")
                return False
                
            # Test 2: Conditional logic with float calculations
            script = '''
                local pi = 3.141592653589793
                local calculation = pi * 2
                local is_positive = calculation > 0
                
                return {
                    pi = pi,
                    doubled = calculation, 
                    positive = is_positive,
                    infinity_test = 1/0
                }
            '''
            
            result = self.r.eval(script, 0)
            
            # Should get key-value pairs with proper float precision and infinity
            if isinstance(result, list) and "inf" in str(result):
                print("  ✅ Float calculations with infinity: PASSED")
            else:
                print(f"  ❌ Float calculation test failed: got {result}")
                return False
                
            return True
            
        except Exception as e:
            print(f"  ❌ Complex pattern test failed: {e}")
            return False
    
    def test_edge_cases_and_boundary_conditions(self):
        """Test edge cases and boundary conditions for all fixes"""
        print("\nTesting edge cases and boundary conditions...")
        
        try:
            # Test 1: Very large associative table
            script = '''
                local t = {}
                for i = 1, 50 do
                    t["key" .. tostring(i)] = "value" .. tostring(i)
                end
                return t
            '''
            
            result = self.r.eval(script, 0)
            if isinstance(result, list) and len(result) >= 50:  # Should have many key-value pairs
                print("  ✅ Large associative table: PASSED")
            else:
                print(f"  ❌ Large table failed: got length {len(result) if isinstance(result, list) else 'not list'}")
                return False
                
            # Test 2: Nested boolean logic
            result = self.r.eval('''
                local a = true
                local b = false
                local c = (a and not b)
                return c
            ''', 0)
            
            if result == 1:  # true should be 1
                print("  ✅ Nested boolean logic: PASSED")
            else:
                print(f"  ❌ Nested boolean failed: got {result}")
                return False
                
            # Test 3: Float edge cases with very small numbers
            result = self.r.eval('return 1e-10', 0)
            if isinstance(result, str) and ("0.0000000001" in result or "1e-10" in result):
                print(f"  ✅ Scientific notation: PASSED ({result})")
            else:
                print(f"  ❌ Scientific notation failed: got {result}")
                return False
                
            # Test 4: Boolean conversion in redis.call results
            result = self.r.eval('return redis.call("EXISTS", "nonexistent_key") == 0', 0)
            if result == 1:  # comparison result true should be 1
                print("  ✅ Boolean conversion in redis.call: PASSED")
            else:
                print(f"  ❌ Boolean redis.call comparison failed: got {result}")
                return False
                
            return True
            
        except Exception as e:
            print(f"  ❌ Edge case test failed: {e}")
            return False

def main():
    print("=" * 80)
    print("FERROUS RESP+LUA 5.1 CONVERSION VALIDATION TEST SUITE")
    print("Testing all dragon fixes: Boolean, Table, Float, Error handling")
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
    
    validator = RespLuaConversionValidator()
    
    # Run all validation tests
    results = []
    results.append(validator.test_boolean_conversions())
    results.append(validator.test_associative_table_conversions())
    results.append(validator.test_sequential_vs_associative_tables())
    results.append(validator.test_float_precision_and_edge_cases())
    results.append(validator.test_error_handling_robustness())
    results.append(validator.test_complex_lua_patterns())
    results.append(validator.test_edge_cases_and_boundary_conditions())
    
    # Summary
    passed = sum(results)
    total = len(results)
    
    print()
    print("=" * 80)
    print(f"RESP+LUA 5.1 VALIDATION RESULTS: {passed}/{total} PASSED")
    print("=" * 80)
    
    # Detailed assessment
    test_names = [
        "Boolean conversions (false→0 fix)",
        "Associative table conversions (data loss fix)",
        "Sequential vs associative table detection",
        "Float precision and IEEE 754 edge cases",
        "Error handling robustness",
        "Complex Lua patterns",
        "Edge cases and boundary conditions"
    ]
    
    print("\nDetailed Results:")
    print("-" * 50)
    for i, (test_name, passed) in enumerate(zip(test_names, results)):
        status = "✅ PASS" if passed else "❌ FAIL"
        print(f"{test_name:<45} {status}")
    
    print()
    
    if passed == total:
        print("🎉 All RESP+Lua 5.1 dragon fixes validated successfully!")
        print("✅ Boolean conversion working correctly")
        print("✅ Associative table data preserved")
        print("✅ Float precision maintained")
        print("✅ Error handling robust")
        sys.exit(0)
    else:
        print("❌ Some validation tests failed")
        print(f"   Success rate: {(passed/total)*100:.1f}%")
        sys.exit(1)

if __name__ == "__main__":
    main()