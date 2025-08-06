#!/usr/bin/env python3
"""
Tests for commands that might be missing in Ferrous implementation
Based on reported compatibility issues
"""

import redis
import time
import sys

class MissingCommandsTester:
    def __init__(self, host='127.0.0.1', port=6379):
        self.host = host
        self.port = port
        self.r = redis.Redis(host=host, port=port, decode_responses=True)
        
    def test_zcard(self):
        """Test ZCARD (sorted set cardinality) command"""
        print("Testing ZCARD command...")
        
        try:
            # Create a sorted set
            self.r.zadd("test_zset", {"member1": 1.0, "member2": 2.0, "member3": 3.0})
            
            # Test ZCARD
            count = self.r.zcard("test_zset")
            if count == 3:
                print("✅ ZCARD returned correct count")
                return True
            else:
                print(f"❌ ZCARD returned wrong count: {count} (expected 3)")
                return False
                
        except redis.ResponseError as e:
            if "unknown command" in str(e).lower():
                print("❌ ZCARD command not implemented")
                return False
            else:
                print(f"❌ ZCARD failed with error: {e}")
                return False
        except Exception as e:
            print(f"❌ ZCARD test failed: {e}")
            return False
        finally:
            try:
                self.r.delete("test_zset")
            except:
                pass
                
    def test_expiry_edge_cases(self):
        """Test expiry timing edge cases"""
        print("\nTesting expiry edge cases...")
        
        test_cases = []
        
        # Test 1: Immediate expiry (0 seconds)
        try:
            self.r.setex("expire_test_1", 0, "value")
            val = self.r.get("expire_test_1")
            if val is None:
                print("  ✅ Zero-second expiry worked correctly")
                test_cases.append(True)
            else:
                print(f"  ❌ Zero-second expiry failed: key still exists with value '{val}'")
                test_cases.append(False)
        except Exception as e:
            print(f"  ❌ Zero-second expiry test failed: {e}")
            test_cases.append(False)
            
        # Test 2: Very short expiry (1 second)
        try:
            self.r.setex("expire_test_2", 1, "value")
            # Check immediately
            val1 = self.r.get("expire_test_2")
            # Wait for expiry
            time.sleep(1.1)
            val2 = self.r.get("expire_test_2")
            
            if val1 == "value" and val2 is None:
                print("  ✅ One-second expiry worked correctly")
                test_cases.append(True)
            else:
                print(f"  ❌ One-second expiry failed: before={val1}, after={val2}")
                test_cases.append(False)
        except Exception as e:
            print(f"  ❌ One-second expiry test failed: {e}")
            test_cases.append(False)
            
        # Test 3: EXPIRE on existing key
        try:
            self.r.set("expire_test_3", "value")
            self.r.expire("expire_test_3", 1)
            # Check TTL
            ttl = self.r.ttl("expire_test_3")
            if 0 < ttl <= 1:
                print(f"  ✅ EXPIRE set correct TTL: {ttl}")
                test_cases.append(True)
            else:
                print(f"  ❌ EXPIRE set wrong TTL: {ttl}")
                test_cases.append(False)
        except Exception as e:
            print(f"  ❌ EXPIRE test failed: {e}")
            test_cases.append(False)
            
        # Test 4: Negative expiry
        try:
            self.r.set("expire_test_4", "value")
            self.r.expire("expire_test_4", -1)
            val = self.r.get("expire_test_4")
            if val is None:
                print("  ✅ Negative expiry immediately deleted key")
                test_cases.append(True)
            else:
                print(f"  ❌ Negative expiry failed: key still exists")
                test_cases.append(False)
        except Exception as e:
            print(f"  ❌ Negative expiry test failed: {e}")
            test_cases.append(False)
            
        # Cleanup
        for i in range(1, 5):
            try:
                self.r.delete(f"expire_test_{i}")
            except:
                pass
                
        return all(test_cases)
        
    def test_distributed_lock_commands(self):
        """Test distributed locking pattern commands"""
        print("\nTesting distributed lock commands...")
        
        lock_key = "distributed_lock_test"
        lock_value = "unique_lock_id_456"
        
        try:
            # Test SET with NX (set if not exists)
            result = self.r.set(lock_key, lock_value, nx=True)
            if result:
                print("  ✅ SET NX acquired lock")
            else:
                print("  ❌ SET NX failed to acquire lock")
                return False
                
            # Try to acquire again (should fail)
            result2 = self.r.set(lock_key, "another_id", nx=True)
            if not result2:
                print("  ✅ SET NX correctly rejected second acquisition")
            else:
                print("  ❌ SET NX allowed double acquisition")
                return False
                
            # Delete and test SET NX EX (atomic lock with expiry)
            self.r.delete(lock_key)
            result3 = self.r.set(lock_key, lock_value, nx=True, ex=2)
            if result3:
                print("  ✅ SET NX EX acquired lock with expiry")
                
                # Check TTL
                ttl = self.r.ttl(lock_key)
                if 1 <= ttl <= 2:
                    print(f"  ✅ Lock has correct TTL: {ttl}")
                else:
                    print(f"  ❌ Lock has wrong TTL: {ttl}")
                    return False
            else:
                print("  ❌ SET NX EX failed")
                return False
                
            return True
            
        except Exception as e:
            print(f"  ❌ Distributed lock test failed: {e}")
            return False
        finally:
            try:
                self.r.delete(lock_key)
            except:
                pass
                
    def test_all_reported_commands(self):
        """Test all commands reported in the compatibility report"""
        print("\nTesting reported command coverage...")
        
        commands_to_test = {
            # Basic operations (reported as working)
            "PING": lambda: self.r.ping(),
            "SET": lambda: self.r.set("test", "value"),
            "GET": lambda: self.r.get("test"),
            "DEL": lambda: self.r.delete("test"),
            "EXISTS": lambda: self.r.exists("test"),
            
            # Set operations (reported as working)
            "SADD": lambda: self.r.sadd("test_set", "member"),
            "SREM": lambda: self.r.srem("test_set", "member"),
            "SMEMBERS": lambda: self.r.smembers("test_set"),
            "SCARD": lambda: self.r.scard("test_set"),
            
            # Expiry operations
            "SETEX": lambda: self.r.setex("test_ex", 10, "value"),
            "EXPIRE": lambda: self.r.expire("test", 10),
            "TTL": lambda: self.r.ttl("test"),
            
            # ZCARD specifically
            "ZCARD": lambda: self.r.zcard("test_zset"),
        }
        
        results = {}
        
        for cmd, test_func in commands_to_test.items():
            try:
                test_func()
                results[cmd] = "✅"
            except redis.ResponseError as e:
                if "unknown command" in str(e).lower():
                    results[cmd] = "❌ NOT IMPLEMENTED"
                else:
                    results[cmd] = f"⚠️  ERROR: {str(e)[:30]}"
            except Exception as e:
                results[cmd] = f"⚠️  EXCEPTION: {str(e)[:30]}"
                
        # Cleanup
        for key in ["test", "test_set", "test_ex", "test_zset"]:
            try:
                self.r.delete(key)
            except:
                pass
                
        # Print results
        print("\nCommand Coverage Results:")
        print("-" * 40)
        all_working = True
        for cmd, status in sorted(results.items()):
            print(f"{cmd:15} {status}")
            if "❌" in status:
                all_working = False
                
        return all_working

def main():
    print("=" * 70)
    print("FERROUS MISSING COMMANDS TEST SUITE")
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
    
    tester = MissingCommandsTester()
    
    # Run tests
    results = []
    results.append(tester.test_zcard())
    results.append(tester.test_expiry_edge_cases())
    results.append(tester.test_distributed_lock_commands())
    results.append(tester.test_all_reported_commands())
    
    # Summary
    passed = sum(results)
    total = len(results)
    
    print()
    print("=" * 70)
    print(f"MISSING COMMANDS TEST RESULTS: {passed}/{total} PASSED")
    print("=" * 70)
    
    if passed == total:
        print("🎉 All command tests passed!")
        sys.exit(0)
    else:
        print("❌ Some command tests failed")
        print("   This may confirm missing commands like ZCARD")
        sys.exit(1)

if __name__ == "__main__":
    main()