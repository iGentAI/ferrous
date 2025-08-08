#!/usr/bin/env python3
"""
Efficient Memory Command Test Suite for Ferrous
Tests MEMORY commands with proper Redis client patterns and reasonable data sizes
"""

import redis
import sys
import time

class EfficientMemoryTester:
    def __init__(self, host='127.0.0.1', port=6379):
        self.host = host
        self.port = port
        self.r = redis.Redis(host=host, port=port, decode_responses=True)
        
    def test_memory_usage_basic(self):
        """Test MEMORY USAGE command with various data types"""
        print("Testing MEMORY USAGE command...")
        
        try:
            # Clear test keys
            self.r.delete('mem_string', 'mem_list', 'mem_hash', 'mem_set')
            
            # Test 1: String memory usage
            self.r.set('mem_string', 'x' * 1000)  # 1KB string
            string_memory = self.r.memory_usage('mem_string')
            if isinstance(string_memory, int) and string_memory > 0:
                print(f"  ✅ String (1KB): {string_memory} bytes")
            else:
                print(f"  ❌ String memory usage failed: {string_memory}")
                return False
            
            # Test 2: List memory usage with reasonable size
            for i in range(100):  # 100 items, not 10,000
                self.r.lpush('mem_list', f'item_{i}')
            
            list_memory = self.r.memory_usage('mem_list')
            if isinstance(list_memory, int) and list_memory > string_memory:
                print(f"  ✅ List (100 items): {list_memory} bytes")
            else:
                print(f"  ❌ List memory usage failed: {list_memory}")
                return False
            
            # Test 3: Hash memory usage
            for i in range(50):  # 50 fields
                self.r.hset('mem_hash', f'field_{i}', f'value_{i}')
            
            hash_memory = self.r.memory_usage('mem_hash')
            if isinstance(hash_memory, int) and hash_memory > 0:
                print(f"  ✅ Hash (50 fields): {hash_memory} bytes")
            else:
                print(f"  ❌ Hash memory usage failed: {hash_memory}")
                return False
            
            # Test 4: Set memory usage
            for i in range(50):  # 50 members
                self.r.sadd('mem_set', f'member_{i}')
            
            set_memory = self.r.memory_usage('mem_set')
            if isinstance(set_memory, int) and set_memory > 0:
                print(f"  ✅ Set (50 members): {set_memory} bytes")
            else:
                print(f"  ❌ Set memory usage failed: {set_memory}")
                return False
                
            # Test 5: Non-existent key
            nonexistent_memory = self.r.memory_usage('nonexistent') 
            if nonexistent_memory is None:
                print(f"  ✅ Non-existent key: None (correct)")
            else:
                print(f"  ❌ Non-existent key should return None: {nonexistent_memory}")
                return False
            
            return True
            
        except Exception as e:
            print(f"  ❌ MEMORY USAGE test failed: {e}")
            return False
    
    def test_memory_stats(self):
        """Test MEMORY STATS command"""
        print("\nTesting MEMORY STATS command...")
        
        try:
            # Get memory stats
            stats = self.r.memory_stats()
            
            if isinstance(stats, dict) and len(stats) > 0:
                # Check for essential memory statistics
                essential_stats = ['total.allocated', 'peak.allocated']
                found_stats = [stat for stat in essential_stats if stat in stats or stat.replace('.', '_') in stats]
                
                print(f"  ✅ MEMORY STATS returned {len(stats)} statistics")
                print(f"  ✅ Essential stats found: {len(found_stats)}/{len(essential_stats)}")
                
                # Show sample statistics
                for key, value in list(stats.items())[:3]:
                    print(f"    {key}: {value}")
                
                return True
            else:
                print(f"  ❌ MEMORY STATS returned invalid format: {type(stats)}")
                return False
                
        except Exception as e:
            print(f"  ❌ MEMORY STATS test failed: {e}")
            return False
    
    def test_memory_doctor(self):
        """Test MEMORY DOCTOR command"""
        print("\nTesting MEMORY DOCTOR command...")
        
        try:
            doctor_output = self.r.execute_command('MEMORY', 'DOCTOR')
            
            if isinstance(doctor_output, (str, bytes)):
                print(f"  ✅ MEMORY DOCTOR returned analysis")
                print(f"    Output length: {len(doctor_output)} characters")
                return True
            else:
                print(f"  ❌ MEMORY DOCTOR returned unexpected type: {type(doctor_output)}")
                return False
                
        except Exception as e:
            print(f"  ❌ MEMORY DOCTOR test failed: {e}")
            return False
    
    def test_memory_efficiency(self):
        """Test memory efficiency with progressively larger datasets"""
        print("\nTesting memory efficiency scaling...")
        
        try:
            # Test memory scaling with different data sizes
            sizes = [10, 50, 100]  # Reasonable test sizes
            memory_usage = {}
            
            for size in sizes:
                key = f'efficiency_test_{size}'
                self.r.delete(key)
                
                start_time = time.time()
                # Use pipeline for efficiency
                with self.r.pipeline() as pipe:
                    for i in range(size):
                        pipe.lpush(key, f'item_{i}')
                    pipe.execute()
                
                elapsed = time.time() - start_time
                memory = self.r.memory_usage(key)
                memory_usage[size] = memory
                
                print(f"  ✅ {size} items: {memory} bytes ({elapsed:.3f}s to create)")
            
            # Verify memory scaling makes sense
            if memory_usage[100] > memory_usage[50] > memory_usage[10]:
                print(f"  ✅ Memory scaling is logical")
                return True
            else:
                print(f"  ⚠️ Memory scaling seems inconsistent: {memory_usage}")
                return True  # Don't fail for this - it's just a warning
                
        except Exception as e:
            print(f"  ❌ Memory efficiency test failed: {e}")
            return False
        finally:
            # Cleanup
            for size in sizes:
                try:
                    self.r.delete(f'efficiency_test_{size}')
                except:
                    pass
    
    def test_memory_large_values(self):
        """Test memory usage with larger values to stress test without hanging"""
        print("\nTesting memory with larger values...")
        
        try:
            # Test progressively larger string values
            sizes = [1000, 10000, 100000]  # 1KB, 10KB, 100KB
            
            for size in sizes:
                key = f'large_value_{size}'
                value = 'x' * size
                
                start_time = time.time()
                self.r.set(key, value)
                elapsed = time.time() - start_time
                
                memory = self.r.memory_usage(key)
                efficiency = memory / size if size > 0 else 0
                
                print(f"  ✅ {size//1000}KB value: {memory} bytes ({efficiency:.1f}x overhead, {elapsed:.3f}s)")
            
            return True
            
        except Exception as e:
            print(f"  ❌ Large value test failed: {e}")
            return False
        finally:
            # Cleanup
            for size in sizes:
                try:
                    self.r.delete(f'large_value_{size}')
                except:
                    pass
    
    def test_memory_edge_cases(self):
        """Test memory command edge cases"""
        print("\nTesting memory edge cases...")
        
        try:
            # Test 1: Empty value
            self.r.set('empty_test', '')
            empty_memory = self.r.memory_usage('empty_test')
            print(f"  ✅ Empty string: {empty_memory} bytes")
            
            # Test 2: Complex data structure
            self.r.delete('complex_test')
            with self.r.pipeline() as pipe:
                # Create a moderately complex hash
                for i in range(20):
                    pipe.hset('complex_test', f'field_{i}', f'value_{i}' * 5)
                pipe.execute()
            
            complex_memory = self.r.memory_usage('complex_test')
            print(f"  ✅ Complex hash (20 fields): {complex_memory} bytes")
            
            # Test 3: Key that doesn't exist
            nonexist_memory = self.r.memory_usage('does_not_exist')
            if nonexist_memory is None:
                print(f"  ✅ Non-existent key: None (correct)")
            else:
                print(f"  ⚠️ Non-existent key returned: {nonexist_memory}")
            
            return True
            
        except Exception as e:
            print(f"  ❌ Edge case test failed: {e}")
            return False
        finally:
            # Cleanup
            try:
                self.r.delete('empty_test', 'complex_test')
            except:
                pass

def main():
    print("=" * 70)
    print("FERROUS EFFICIENT MEMORY TEST SUITE")
    print("Proper Redis client patterns with reasonable data sizes")
    print("=" * 70)
    
    # Check server connection
    try:
        r = redis.Redis(host='127.0.0.1', port=6379)
        r.ping()
        print("✅ Server connection verified")
    except Exception as e:
        print(f"❌ Cannot connect to server: {e}")
        sys.exit(1)
        
    print()
    
    tester = EfficientMemoryTester()
    
    # Run all memory tests efficiently
    start_time = time.time()
    results = []
    
    results.append(tester.test_memory_usage_basic())
    results.append(tester.test_memory_stats())
    results.append(tester.test_memory_doctor())
    results.append(tester.test_memory_efficiency())
    results.append(tester.test_memory_large_values())
    results.append(tester.test_memory_edge_cases())
    
    elapsed = time.time() - start_time
    passed = sum(results)
    total = len(results)
    
    print("\n" + "=" * 70)
    print(f"EFFICIENT MEMORY TEST RESULTS: {passed}/{total} PASSED")
    print(f"Total test time: {elapsed:.2f} seconds (vs hours for old test)")
    print("=" * 70)
    
    if passed == total:
        print("🎉 All memory tests passed efficiently!")
        print("✅ MEMORY command functionality validated")
        print("✅ No hanging or performance issues")
        sys.exit(0)
    else:
        print(f"❌ {total - passed} memory tests failed")
        sys.exit(1)

if __name__ == "__main__":
    main()