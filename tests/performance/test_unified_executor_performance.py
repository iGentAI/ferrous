#!/usr/bin/env python3
"""
Performance test suite for unified command executor
Validates that comprehensive command coverage maintains strong performance
"""

import redis
import time
import sys
from concurrent.futures import ThreadPoolExecutor

class UnifiedExecutorPerformanceTester:
    def __init__(self, host='127.0.0.1', port=6379):
        self.host = host
        self.port = port
        self.r = redis.Redis(host=host, port=port, decode_responses=True)
        
    def test_unified_executor_throughput(self):
        """Test unified command executor throughput for different operation types"""
        print("Testing unified command executor throughput...")
        
        operations = [
            ("SET operations", lambda i: self.r.eval(f'return redis.call("SET", "perf_test_{i}", "value_{i}")', 0)),
            ("GET operations", lambda i: self.r.eval(f'return redis.call("GET", "perf_test_{i}")', 0)),
            ("INCR operations", lambda i: self.r.eval(f'return redis.call("INCR", "counter_{i}")', 0)),
            ("LPUSH operations", lambda i: self.r.eval(f'return redis.call("LPUSH", "list_{i}", "item_{i}")', 0)),
            ("SADD operations", lambda i: self.r.eval(f'return redis.call("SADD", "set_{i}", "member_{i}")', 0)),
            ("HSET operations", lambda i: self.r.eval(f'return redis.call("HSET", "hash_{i}", "field_{i}", "value_{i}")', 0)),
            ("ZADD operations", lambda i: self.r.eval(f'return redis.call("ZADD", "zset_{i}", "{i}.0", "member_{i}")', 0)),
        ]
        
        results = []
        
        for op_name, op_func in operations:
            try:
                # Warm up
                for i in range(10):
                    op_func(i)
                
                # Performance test
                start_time = time.time()
                num_ops = 1000
                
                for i in range(num_ops):
                    op_func(i)
                
                elapsed = time.time() - start_time
                ops_per_sec = num_ops / elapsed
                
                print(f"  {op_name}: {ops_per_sec:.2f} ops/sec")
                results.append((op_name, ops_per_sec))
                
                if ops_per_sec < 1000:  # Minimum threshold
                    print(f"  ⚠️  {op_name} performance below threshold")
                    return False
                    
            except Exception as e:
                print(f"  ❌ {op_name} failed: {e}")
                return False
        
        avg_performance = sum(r[1] for r in results) / len(results)
        print(f"  Average performance: {avg_performance:.2f} ops/sec")
        
        if avg_performance >= 5000:  # Good performance threshold
            print("✅ Unified executor performance excellent")
            return True
        else:
            print("⚠️  Unified executor performance needs attention")
            return False
    
    def test_multi_command_script_performance(self):
        """Test performance of multi-command Lua scripts through unified executor"""
        print("Testing multi-command script performance...")
        
        try:
            # Complex multi-command script
            complex_script = """
                local prefix = ARGV[1]
                
                -- Multi-step operations
                redis.call("SET", prefix .. ":string", "test_value")
                redis.call("LPUSH", prefix .. ":list", "item1", "item2")
                redis.call("SADD", prefix .. ":set", "member1", "member2")
                redis.call("HSET", prefix .. ":hash", "field1", "value1", "field2", "value2")
                redis.call("ZADD", prefix .. ":zset", "1.0", "one", "2.0", "two")
                
                -- Retrieve values
                local string_val = redis.call("GET", prefix .. ":string")
                local list_len = redis.call("LLEN", prefix .. ":list")
                local set_card = redis.call("SCARD", prefix .. ":set")
                local hash_len = redis.call("HLEN", prefix .. ":hash")
                local zset_card = redis.call("ZCARD", prefix .. ":zset")
                
                return {string_val, list_len, set_card, hash_len, zset_card}
            """
            
            # Performance test
            start_time = time.time()
            num_scripts = 500
            
            for i in range(num_scripts):
                result = self.r.eval(complex_script, 0, f"test_{i}")
                
                # Validate result structure 
                if result != ["test_value", 2, 2, 2, 2]:
                    print(f"❌ Incorrect script result: {result}")
                    return False
            
            elapsed = time.time() - start_time
            scripts_per_sec = num_scripts / elapsed
            
            print(f"  Complex scripts: {scripts_per_sec:.2f} scripts/sec")
            print(f"  Operations per script: 10 commands")
            print(f"  Effective ops/sec: {scripts_per_sec * 10:.2f}")
            
            if scripts_per_sec >= 100:  # Good script performance
                print("✅ Multi-command script performance excellent")
                return True
            else:
                print("⚠️  Multi-command script performance needs attention")
                return False
                
        except Exception as e:
            print(f"❌ Multi-command script performance test failed: {e}")
            return False
    
    def test_atomicity_under_load(self):
        """Test atomicity guarantees under concurrent load"""
        print("Testing atomicity under concurrent load...")
        
        try:
            # Atomic increment test with multiple "clients"
            def atomic_increment_worker(worker_id):
                script = """
                    for i = 1, 10 do
                        redis.call("INCR", "shared_counter")
                        redis.call("LPUSH", "shared_list", ARGV[1])
                    end
                    return "done"
                """
                return self.r.eval(script, 0, f"worker_{worker_id}")
            
            # Clear test data
            self.r.delete("shared_counter", "shared_list")
            
            # Run multiple workers concurrently
            with ThreadPoolExecutor(max_workers=5) as executor:
                futures = [executor.submit(atomic_increment_worker, i) for i in range(5)]
                
                # Wait for all to complete
                for future in futures:
                    result = future.result(timeout=10)
                    if result != "done":
                        print(f"❌ Worker failed: {result}")
                        return False
            
            # Verify atomic results
            final_counter = int(self.r.get("shared_counter") or 0)
            list_length = self.r.llen("shared_list")
            
            expected_counter = 5 * 10  # 5 workers * 10 increments each
            expected_list_length = 5 * 10  # 5 workers * 10 pushes each
            
            if final_counter == expected_counter and list_length == expected_list_length:
                print(f"✅ Atomicity maintained under load: counter={final_counter}, list_len={list_length}")
                return True
            else:
                print(f"❌ Atomicity violated: expected counter={expected_counter}, list={expected_list_length}")
                print(f"    Actual: counter={final_counter}, list={list_length}")
                return False
                
        except Exception as e:
            print(f"❌ Atomicity under load test failed: {e}")
            return False

def main():
    print("=" * 80)
    print("UNIFIED COMMAND EXECUTOR PERFORMANCE VALIDATION")
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
    
    tester = UnifiedExecutorPerformanceTester()
    
    # Run performance tests
    results = []
    results.append(tester.test_unified_executor_throughput())
    results.append(tester.test_multi_command_script_performance())
    results.append(tester.test_atomicity_under_load())
    
    # Summary
    passed = sum(results)
    total = len(results)
    
    print()
    print("=" * 80)
    print(f"UNIFIED EXECUTOR PERFORMANCE RESULTS: {passed}/{total} PASSED")
    print("=" * 80)
    
    if passed == total:
        print("🎉 All performance tests passed!")
        print("✅ Unified command executor maintains excellent performance")
        print("✅ Atomicity guarantees preserved under load")
        print("✅ Multi-step script execution optimized")
        sys.exit(0)
    else:
        print("❌ Some performance tests failed")
        print("⚠️  Performance or atomicity issues detected")
        sys.exit(1)

if __name__ == "__main__":
    main()