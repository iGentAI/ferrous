#!/usr/bin/env python3
"""
WRONGTYPE Protocol Compliance Test Suite for Ferrous
Validates that all command types return proper Redis WRONGTYPE error responses
instead of closing connections when type mismatches occur.
"""

import redis
import sys
import time

class WrongTypeComplianceTester:
    def __init__(self, host='127.0.0.1', port=6379):
        self.host = host
        self.port = port
        self.r = redis.Redis(host=host, port=port, decode_responses=True)
        
    def test_list_commands_wrongtype(self):
        """Test list commands return WRONGTYPE on non-list keys"""
        print("Testing list commands WRONGTYPE compliance...")
        
        # Setup different key types
        self.r.delete('string_key', 'set_key', 'hash_key')
        self.r.set('string_key', 'string_value')
        self.r.sadd('set_key', 'set_member')
        self.r.hset('hash_key', 'field', 'value')
        
        list_commands = [
            ('LPUSH on string', lambda: self.r.lpush('string_key', 'item')),
            ('RPUSH on string', lambda: self.r.rpush('string_key', 'item')),
            ('LPOP on set', lambda: self.r.lpop('set_key')),
            ('RPOP on set', lambda: self.r.rpop('set_key')),
            ('LLEN on hash', lambda: self.r.llen('hash_key')),
            ('LRANGE on string', lambda: self.r.lrange('string_key', 0, -1)),
            ('LINDEX on set', lambda: self.r.lindex('set_key', 0)),
        ]
        
        results = []
        for test_name, test_func in list_commands:
            try:
                test_func()
                print(f"  ❌ {test_name}: Allowed (should return WRONGTYPE)")
                results.append(False)
            except redis.ResponseError as e:
                if 'WRONGTYPE' in str(e) or 'wrong kind of value' in str(e):
                    print(f"  ✅ {test_name}: Proper WRONGTYPE error")
                    results.append(True)
                else:
                    print(f"  ⚠️ {test_name}: Wrong error type: {e}")
                    results.append(False)
            except Exception as e:
                print(f"  ❌ {test_name}: Connection error: {e}")
                results.append(False)
                
        # Verify connection is still alive
        try:
            self.r.ping()
            print("  ✅ Connection alive after list command errors")
            results.append(True)
        except Exception as e:
            print(f"  ❌ Connection dead after list errors: {e}")
            results.append(False)
            
        return all(results)
    
    def test_set_commands_wrongtype(self):
        """Test set commands return WRONGTYPE on non-set keys"""
        print("\nTesting set commands WRONGTYPE compliance...")
        
        # Setup different key types
        self.r.delete('string_key', 'list_key', 'hash_key')
        self.r.set('string_key', 'string_value')
        self.r.lpush('list_key', 'list_item')
        self.r.hset('hash_key', 'field', 'value')
        
        set_commands = [
            ('SADD on string', lambda: self.r.sadd('string_key', 'member')),
            ('SREM on list', lambda: self.r.srem('list_key', 'member')),
            ('SMEMBERS on hash', lambda: self.r.smembers('hash_key')),
            ('SISMEMBER on string', lambda: self.r.sismember('string_key', 'member')),
            ('SCARD on list', lambda: self.r.scard('list_key')),
            ('SPOP on hash', lambda: self.r.spop('hash_key')),
            ('SRANDMEMBER on string', lambda: self.r.srandmember('string_key')),
        ]
        
        results = []
        for test_name, test_func in set_commands:
            try:
                test_func()
                print(f"  ❌ {test_name}: Allowed (should return WRONGTYPE)")
                results.append(False)
            except redis.ResponseError as e:
                if 'WRONGTYPE' in str(e) or 'wrong kind of value' in str(e):
                    print(f"  ✅ {test_name}: Proper WRONGTYPE error")
                    results.append(True)
                else:
                    print(f"  ⚠️ {test_name}: Wrong error type: {e}")
                    results.append(False)
            except Exception as e:
                print(f"  ❌ {test_name}: Connection error: {e}")
                results.append(False)
                
        return all(results)
    
    def test_hash_commands_wrongtype(self):
        """Test hash commands return WRONGTYPE on non-hash keys"""
        print("\nTesting hash commands WRONGTYPE compliance...")
        
        # Setup different key types
        self.r.delete('string_key', 'list_key', 'set_key')
        self.r.set('string_key', 'string_value')
        self.r.lpush('list_key', 'list_item')
        self.r.sadd('set_key', 'set_member')
        
        hash_commands = [
            ('HSET on string', lambda: self.r.hset('string_key', 'field', 'value')),
            ('HGET on list', lambda: self.r.hget('list_key', 'field')),
            ('HGETALL on set', lambda: self.r.hgetall('set_key')),
            ('HDEL on string', lambda: self.r.hdel('string_key', 'field')),
            ('HLEN on list', lambda: self.r.hlen('list_key')),
            ('HEXISTS on set', lambda: self.r.hexists('set_key', 'field')),
            ('HKEYS on string', lambda: self.r.hkeys('string_key')),
            ('HVALS on list', lambda: self.r.hvals('list_key')),
        ]
        
        results = []
        for test_name, test_func in hash_commands:
            try:
                test_func()
                print(f"  ❌ {test_name}: Allowed (should return WRONGTYPE)")
                results.append(False)
            except redis.ResponseError as e:
                if 'WRONGTYPE' in str(e) or 'wrong kind of value' in str(e):
                    print(f"  ✅ {test_name}: Proper WRONGTYPE error")
                    results.append(True)
                else:
                    print(f"  ⚠️ {test_name}: Wrong error type: {e}")
                    results.append(False)
            except Exception as e:
                print(f"  ❌ {test_name}: Connection error: {e}")
                results.append(False)
                
        return all(results)
    
    def test_string_commands_wrongtype(self):
        """Test string commands return WRONGTYPE on non-string keys"""
        print("\nTesting string commands WRONGTYPE compliance...")
        
        # Setup different key types
        self.r.delete('list_key', 'set_key', 'hash_key')
        self.r.lpush('list_key', 'list_item')
        self.r.sadd('set_key', 'set_member')
        self.r.hset('hash_key', 'field', 'value')
        
        string_commands = [
            ('APPEND on list', lambda: self.r.append('list_key', 'text')),
            ('STRLEN on set', lambda: self.r.strlen('set_key')),
            ('GETRANGE on hash', lambda: self.r.getrange('hash_key', 0, 5)),
            ('SETRANGE on list', lambda: self.r.setrange('list_key', 0, 'text')),
        ]
        
        results = []
        for test_name, test_func in string_commands:
            try:
                test_func()
                print(f"  ❌ {test_name}: Allowed (should return WRONGTYPE)")
                results.append(False)
            except redis.ResponseError as e:
                if 'WRONGTYPE' in str(e) or 'wrong kind of value' in str(e):
                    print(f"  ✅ {test_name}: Proper WRONGTYPE error")
                    results.append(True)
                else:
                    print(f"  ⚠️ {test_name}: Wrong error type: {e}")
                    results.append(False)
            except Exception as e:
                print(f"  ❌ {test_name}: Connection error: {e}")
                results.append(False)
                
        return all(results)
    
    def test_connection_stability_after_errors(self):
        """Test that connections remain stable after multiple WRONGTYPE errors"""
        print("\nTesting connection stability after multiple WRONGTYPE errors...")
        
        error_count = 0
        connection_alive = True
        
        try:
            # Generate multiple WRONGTYPE errors in sequence
            self.r.delete('multi_error_test')
            self.r.set('multi_error_test', 'string_value')
            
            error_operations = [
                lambda: self.r.lpush('multi_error_test', 'item'),
                lambda: self.r.sadd('multi_error_test', 'member'),
                lambda: self.r.hset('multi_error_test', 'field', 'value'),
                lambda: self.r.rpush('multi_error_test', 'item'),
                lambda: self.r.smembers('multi_error_test'),
            ]
            
            for i, op in enumerate(error_operations):
                try:
                    op()
                    print(f"  ⚠️ Operation {i+1}: Succeeded (should have errored)")
                except redis.ResponseError as e:
                    if 'WRONGTYPE' in str(e):
                        error_count += 1
                    else:
                        print(f"  ⚠️ Operation {i+1}: Wrong error type: {e}")
                except Exception as e:
                    print(f"  ❌ Operation {i+1}: Connection error: {e}")
                    connection_alive = False
                    break
            
            # Test final connection health
            if connection_alive:
                try:
                    result = self.r.ping()
                    print(f"  ✅ Connection healthy after {error_count} WRONGTYPE errors")
                    return True
                except Exception as e:
                    print(f"  ❌ Connection failed final health check: {e}")
                    return False
            else:
                print(f"  ❌ Connection failed during error sequence")
                return False
                
        except Exception as e:
            print(f"  ❌ Test failed: {e}")
            return False
    
    def test_rapid_fire_wrongtype_errors(self):
        """Test rapid-fire WRONGTYPE errors don't destabilize the server"""
        print("\nTesting rapid-fire WRONGTYPE errors...")
        
        try:
            # Setup for rapid errors
            self.r.delete('rapid_test')
            self.r.set('rapid_test', 'string_value')
            
            error_count = 0
            start_time = time.time()
            
            # Generate 50 rapid WRONGTYPE errors
            for i in range(50):
                try:
                    self.r.lpush('rapid_test', f'item_{i}')
                except redis.ResponseError as e:
                    if 'WRONGTYPE' in str(e):
                        error_count += 1
                except Exception as e:
                    print(f"  ❌ Connection error during rapid test: {e}")
                    return False
            
            end_time = time.time()
            duration = end_time - start_time
            
            # Verify server stability
            try:
                self.r.ping()
                print(f"  ✅ Generated {error_count} WRONGTYPE errors in {duration:.2f}s")
                print("  ✅ Server stable after rapid error generation")
                return True
            except Exception as e:
                print(f"  ❌ Server unstable after rapid errors: {e}")
                return False
                
        except Exception as e:
            print(f"  ❌ Rapid error test failed: {e}")
            return False

def main():
    print("=" * 80)
    print("FERROUS WRONGTYPE PROTOCOL COMPLIANCE TEST SUITE")
    print("Dipstick validation for Redis protocol adherence")
    print("=" * 80)
    
    # Check server connection
    try:
        r = redis.Redis(host='127.0.0.1', port=6379)
        r.ping()
        print("✅ Server connection verified")
    except:
        print("❌ Cannot connect to server")
        sys.exit(1)
        
    print()
    
    tester = WrongTypeComplianceTester()
    
    # Run all WRONGTYPE compliance tests
    results = []
    results.append(tester.test_list_commands_wrongtype())
    results.append(tester.test_set_commands_wrongtype())  
    results.append(tester.test_hash_commands_wrongtype())
    results.append(tester.test_string_commands_wrongtype())
    results.append(tester.test_connection_stability_after_errors())
    results.append(tester.test_rapid_fire_wrongtype_errors())
    
    # Summary
    passed = sum(results)
    total = len(results)
    
    print("\n" + "=" * 80)
    print(f"WRONGTYPE COMPLIANCE RESULTS: {passed}/{total} PASSED")
    print("=" * 80)
    
    if passed == total:
        print("🎉 ALL WRONGTYPE PROTOCOL COMPLIANCE TESTS PASSED!")
        print("✅ Redis protocol adherence validated across all command types")
        print("✅ Connection stability maintained during error conditions")
        print("✅ No connection closures on type mismatches")
        sys.exit(0)
    else:
        print(f"❌ PROTOCOL COMPLIANCE ISSUES: {total - passed} failed tests")
        print("⚠️ WRONGTYPE error handling needs attention")
        sys.exit(1)

if __name__ == "__main__":
    main()