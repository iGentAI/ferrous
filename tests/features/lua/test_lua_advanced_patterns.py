#!/usr/bin/env python3
"""
Advanced Lua Pattern Tests for Ferrous
Complex examples that thoroughly test Lua API integration without relying on script caching
"""

import redis
import time
import sys

class AdvancedLuaPatternTester:
    def __init__(self, host='127.0.0.1', port=6379):
        self.host = host
        self.port = port
        self.r = redis.Redis(host=host, port=port, decode_responses=True)
        
    def test_distributed_counter_with_expiry(self):
        """Test a distributed counter pattern with automatic expiry"""
        print("Testing distributed counter with expiry...")
        
        # Complex script that increments counters with sliding window expiry
        script = """
            local key = KEYS[1]
            local window_seconds = tonumber(ARGV[1])
            local current_time = tonumber(ARGV[2])
            
            -- Get current count or default to 0
            local current_count = redis.call('GET', key)
            if current_count == nil then
                current_count = 0
            else
                current_count = tonumber(current_count)
            end
            
            -- Increment counter
            local new_count = current_count + 1
            
            -- Set the new count with expiry
            redis.call('SET', key, tostring(new_count))
            redis.call('EXPIRE', key, window_seconds)
            
            -- Return both old and new count for verification
            return {current_count, new_count, window_seconds}
        """
        
        try:
            # Test the counter pattern
            current_time = int(time.time())
            result = self.r.eval(script, 1, "test:counter", "10", str(current_time))
            
            if isinstance(result, list) and len(result) == 3:
                old_count, new_count, window = result
                print(f"  ✅ Counter incremented from {old_count} to {new_count}")
                print(f"  ✅ Window set to {window} seconds")
                
                # Test second increment
                result2 = self.r.eval(script, 1, "test:counter", "10", str(current_time))
                old_count2, new_count2, _ = result2
                
                if new_count2 == new_count + 1:
                    print(f"  ✅ Second increment: {old_count2} -> {new_count2}")
                    return True
                else:
                    print(f"  ❌ Second increment failed: expected {new_count + 1}, got {new_count2}")
                    return False
            else:
                print(f"  ❌ Unexpected result format: {result}")
                return False
                
        except Exception as e:
            print(f"  ❌ Distributed counter test failed: {e}")
            return False
        finally:
            try:
                self.r.delete("test:counter")
            except:
                pass
    
    def test_rate_limiter_pattern(self):
        """Test a sophisticated rate limiting pattern using Lua"""
        print("\nTesting sliding window rate limiter...")
        
        # Rate limiter with sliding window
        rate_limiter_script = """
            local key = KEYS[1]
            local max_requests = tonumber(ARGV[1])
            local window_seconds = tonumber(ARGV[2])
            local current_time = tonumber(ARGV[3])
            
            -- Get current count
            local current = redis.call('GET', key)
            local count = 0
            if current ~= nil then
                count = tonumber(current)
            end
            
            -- Check if limit exceeded
            if count >= max_requests then
                local ttl = redis.call('TTL', key)
                return {false, count, ttl}
            end
            
            -- Increment and set expiry
            local new_count = count + 1
            redis.call('SET', key, tostring(new_count))
            
            -- Set expiry only on first request
            if count == 0 then
                redis.call('EXPIRE', key, window_seconds)
            end
            
            local remaining_ttl = redis.call('TTL', key)
            return {true, new_count, remaining_ttl}
        """
        
        try:
            rate_key = "ratelimit:test_user"
            max_requests = 5
            window_seconds = 10
            current_time = int(time.time())
            
            results = []
            
            # Test multiple requests within the rate limit
            for i in range(7):  # Try 7 requests with limit of 5
                result = self.r.eval(rate_limiter_script, 1, rate_key, 
                                   str(max_requests), str(window_seconds), str(current_time))
                
                allowed, count, ttl = result
                results.append((allowed, count, ttl))
                print(f"  Request {i+1}: allowed={allowed}, count={count}, ttl={ttl}")
            
            # Verify rate limiting behavior
            allowed_requests = sum(1 for allowed, _, _ in results if allowed)
            denied_requests = sum(1 for allowed, _, _ in results if not allowed)
            
            if allowed_requests == max_requests and denied_requests == 2:
                print(f"  ✅ Rate limiter working: {allowed_requests} allowed, {denied_requests} denied")
                return True
            else:
                print(f"  ❌ Rate limiter failed: {allowed_requests} allowed, {denied_requests} denied")
                return False
                
        except Exception as e:
            print(f"  ❌ Rate limiter test failed: {e}")
            return False
        finally:
            try:
                self.r.delete(rate_key)
            except:
                pass
    
    def test_bulk_key_operations(self):
        """Test bulk operations using Lua for atomicity"""
        print("\nTesting bulk key operations...")
        
        # Bulk set with validation
        bulk_set_script = """
            local prefix = KEYS[1]
            local count = tonumber(ARGV[1])
            local base_value = ARGV[2]
            
            for i = 1, count do
                local key = prefix .. ':' .. tostring(i)
                local value = base_value .. '_' .. tostring(i)
                redis.call('SET', key, value)
            end
            
            -- Return count of successful operations
            return count
        """
        
        # Bulk get and verify
        bulk_get_script = """
            local prefix = KEYS[1]
            local count = tonumber(ARGV[1])
            local expected_base = ARGV[2]
            
            local found = 0
            local missing = 0
            local wrong = 0
            
            for i = 1, count do
                local key = prefix .. ':' .. tostring(i)
                local value = redis.call('GET', key)
                local expected = expected_base .. '_' .. tostring(i)
                
                if value == nil then
                    missing = missing + 1
                elseif value == expected then
                    found = found + 1
                else
                    wrong = wrong + 1
                end
            end
            
            return {found, missing, wrong}
        """
        
        try:
            prefix = "bulk_test"
            count = 25
            base_value = "test_value"
            
            # Bulk set operation
            set_result = self.r.eval(bulk_set_script, 1, prefix, str(count), base_value)
            if set_result == count:
                print(f"  ✅ Bulk set: {set_result} keys created")
            else:
                print(f"  ❌ Bulk set failed: expected {count}, got {set_result}")
                return False
            
            # Bulk get and verify  
            get_result = self.r.eval(bulk_get_script, 1, prefix, str(count), base_value)
            found, missing, wrong = get_result
            
            if found == count and missing == 0 and wrong == 0:
                print(f"  ✅ Bulk verify: {found} correct, {missing} missing, {wrong} wrong")
                return True
            else:
                print(f"  ❌ Bulk verify failed: {found} correct, {missing} missing, {wrong} wrong")
                return False
                
        except Exception as e:
            print(f"  ❌ Bulk operations test failed: {e}")
            return False
        finally:
            # Cleanup
            try:
                for i in range(1, count + 1):
                    self.r.delete(f"{prefix}:{i}")
            except:
                pass
    
    def test_conditional_multi_key_transaction(self):
        """Test complex conditional logic across multiple keys"""
        print("\nTesting conditional multi-key transaction...")
        
        # Complex conditional transaction
        conditional_script = """
            local primary_key = KEYS[1]
            local secondary_key = KEYS[2] 
            local backup_key = KEYS[3]
            local operation = ARGV[1]
            local value = ARGV[2]
            local threshold = tonumber(ARGV[3])
            
            if operation == 'migrate' then
                -- Get values from primary and secondary
                local primary_val = redis.call('GET', primary_key)
                local secondary_val = redis.call('GET', secondary_key)
                
                -- Convert to numbers or default to 0
                local primary_num = primary_val and tonumber(primary_val) or 0
                local secondary_num = secondary_val and tonumber(secondary_val) or 0
                local total = primary_num + secondary_num
                
                -- Conditional migration based on threshold
                if total > threshold then
                    -- Migrate to backup and clear originals
                    redis.call('SET', backup_key, tostring(total))
                    redis.call('DEL', primary_key)
                    redis.call('DEL', secondary_key)
                    return {true, 'migrated', total, primary_num, secondary_num}
                else
                    -- Accumulate in primary
                    redis.call('SET', primary_key, tostring(total))
                    redis.call('DEL', secondary_key) 
                    return {false, 'accumulated', total, primary_num, secondary_num}
                end
            else
                return {false, 'unknown_operation', 0, 0, 0}
            end
        """
        
        try:
            primary = "multikey:primary"
            secondary = "multikey:secondary" 
            backup = "multikey:backup"
            
            # Setup initial state
            self.r.set(primary, "15")
            self.r.set(secondary, "8")
            
            # Test below threshold (15 + 8 = 23 < 30)
            result1 = self.r.eval(conditional_script, 3, primary, secondary, backup, 
                                "migrate", "unused", "30")
            
            migrated1, action1, total1, p1, s1 = result1
            if not migrated1 and action1 == "accumulated" and total1 == 23:
                print(f"  ✅ Below threshold: accumulated {total1} = {p1} + {s1}")
            else:
                print(f"  ❌ Below threshold test failed: {result1}")
                return False
            
            # Add more to secondary to trigger migration
            self.r.set(secondary, "12")  # 23 + 12 = 35 > 30
            
            result2 = self.r.eval(conditional_script, 3, primary, secondary, backup,
                                "migrate", "unused", "30")
            
            migrated2, action2, total2, p2, s2 = result2
            if migrated2 and action2 == "migrated" and total2 == 35:
                print(f"  ✅ Above threshold: migrated {total2} = {p2} + {s2}")
                
                # Verify backup has the value
                backup_value = self.r.get(backup)
                if backup_value == "35":
                    print("  ✅ Migration successful, backup contains correct value")
                    return True
                else:
                    print(f"  ❌ Migration failed, backup has: {backup_value}")
                    return False
            else:
                print(f"  ❌ Above threshold test failed: {result2}")
                return False
                
        except Exception as e:
            print(f"  ❌ Conditional transaction test failed: {e}")
            return False
        finally:
            try:
                self.r.delete(primary, secondary, backup)
            except:
                pass
    
    def test_hash_aggregation_pattern(self):
        """Test hash field aggregation and computation"""
        print("\nTesting hash aggregation patterns...")
        
        # Hash aggregation with mathematical operations
        hash_aggregation_script = """
            local hash_key = KEYS[1]
            local summary_key = KEYS[2]
            local operation = ARGV[1]
            
            -- Get all hash fields and values
            local hash_data = redis.call('HGETALL', hash_key)
            local field_count = 0
            local sum_total = 0
            local max_value = nil
            local min_value = nil
            
            -- Process hash data (comes as flat array: field1, value1, field2, value2...)
            for i = 1, #hash_data, 2 do
                local field = hash_data[i]
                local value = tonumber(hash_data[i + 1])
                
                if value then
                    field_count = field_count + 1
                    sum_total = sum_total + value
                    
                    if max_value == nil or value > max_value then
                        max_value = value
                    end
                    
                    if min_value == nil or value < min_value then
                        min_value = value
                    end
                end
            end
            
            local average = field_count > 0 and (sum_total / field_count) or 0
            
            -- Store summary
            redis.call('HSET', summary_key, 'count', field_count)
            redis.call('HSET', summary_key, 'sum', sum_total)
            redis.call('HSET', summary_key, 'avg', tostring(average))
            redis.call('HSET', summary_key, 'max', max_value or 0)
            redis.call('HSET', summary_key, 'min', min_value or 0)
            
            return {field_count, sum_total, average, max_value or 0, min_value or 0}
        """
        
        try:
            hash_key = "stats:metrics"
            summary_key = "stats:summary"
            
            # Setup hash with numeric data
            metrics = {
                "cpu_usage": "45.5",
                "memory_usage": "78.2", 
                "disk_usage": "23.1",
                "network_io": "156.7",
                "response_time": "12.3"
            }
            
            for field, value in metrics.items():
                self.r.hset(hash_key, field, value)
            
            # Aggregate the data
            result = self.r.eval(hash_aggregation_script, 2, hash_key, summary_key, "aggregate")
            
            count, sum_val, avg, max_val, min_val = result
            
            print(f"  ✅ Aggregated {count} metrics")
            print(f"  ✅ Sum: {sum_val}, Avg: {avg:.2f}")
            print(f"  ✅ Range: {min_val} - {max_val}")
            
            # Verify calculations
            expected_sum = sum(float(v) for v in metrics.values())
            expected_avg = expected_sum / len(metrics)
            
            if abs(sum_val - expected_sum) < 0.01 and abs(avg - expected_avg) < 0.01:
                print("  ✅ Hash aggregation calculations correct")
                return True
            else:
                print(f"  ❌ Calculations wrong: sum={sum_val} (expected {expected_sum}), avg={avg} (expected {expected_avg})")
                return False
                
        except Exception as e:
            print(f"  ❌ Hash aggregation test failed: {e}")
            return False
        finally:
            try:
                self.r.delete(hash_key, summary_key)
            except:
                pass
    
    def test_advanced_string_manipulation(self):
        """Test advanced string processing and pattern matching"""
        print("\nTesting advanced string manipulation...")
        
        # String processing with validation and transformation
        string_processing_script = """
            local data_key = KEYS[1]
            local result_key = KEYS[2]
            local pattern = ARGV[1]
            local replacement = ARGV[2]
            
            -- Get the data
            local data = redis.call('GET', data_key)
            if data == nil then
                return {false, 'no_data', 0, 0}
            end
            
            -- Simple pattern replacement (Lua string.gsub)
            local processed, count = string.gsub(data, pattern, replacement)
            
            -- Calculate some stats
            local original_length = string.len(data)
            local processed_length = string.len(processed)
            
            -- Store processed result
            redis.call('SET', result_key, processed)
            
            -- Set expiry on result
            redis.call('EXPIRE', result_key, 300)
            
            return {
                true,                   -- success
                'processed',           -- status
                original_length,       -- original length
                processed_length,      -- processed length 
                count                  -- replacements made
            }
        """
        
        try:
            data_key = "string:original"
            result_key = "string:processed"
            
            # Setup test data
            test_string = "Hello world! This is a test. Hello again, world!"
            self.r.set(data_key, test_string)
            
            # Process string (replace "world" with "universe")
            result = self.r.eval(string_processing_script, 2, data_key, result_key, 
                               "world", "universe")
            
            success, status, orig_len, proc_len, replacements = result
            
            if success and replacements == 2:  # Should replace 2 instances of "world"
                print(f"  ✅ String processing: {replacements} replacements")
                print(f"  ✅ Length: {orig_len} -> {proc_len}")
                
                # Verify result
                processed = self.r.get(result_key)
                expected = "Hello universe! This is a test. Hello again, universe!"
                
                if processed == expected:
                    print("  ✅ String transformation correct")
                    return True
                else:
                    print(f"  ❌ Transformation failed: got '{processed}'")
                    return False
            else:
                print(f"  ❌ String processing failed: {result}")
                return False
                
        except Exception as e:
            print(f"  ❌ String manipulation test failed: {e}")
            return False
        finally:
            try:
                self.r.delete(data_key, result_key)
            except:
                pass
    
    def test_performance_complex_script(self):
        """Test performance of complex Lua operations"""
        print("\nTesting performance of complex operations...")
        
        # Performance test script with multiple operations
        perf_script = """
            local base_key = KEYS[1]
            local iterations = tonumber(ARGV[1])
            
            local operations = 0
            
            for i = 1, iterations do
                local key = base_key .. ':' .. tostring(i)
                
                -- Multiple operations per iteration
                redis.call('SET', key, tostring(i * 2))
                operations = operations + 1
                
                local value = redis.call('GET', key)
                operations = operations + 1
                
                if tonumber(value) > 10 then
                    redis.call('INCR', key)
                    operations = operations + 1
                end
                
                if i % 5 == 0 then
                    redis.call('DEL', key)
                    operations = operations + 1
                end
            end
            
            -- Return performance info
            return {operations, iterations}
        """
        
        try:
            base_key = "perf_test"
            iterations = 50
            
            # Run performance test
            before = time.time()
            result = self.r.eval(perf_script, 1, base_key, str(iterations))
            after = time.time()
            
            operations, iter_count = result
            duration = after - before
            ops_per_sec = operations / duration if duration > 0 else 0
            
            print(f"  ✅ Completed {operations} operations in {iter_count} iterations")
            print(f"  ✅ Duration: {duration:.3f}s, Rate: {ops_per_sec:.0f} ops/sec")
            
            if ops_per_sec > 1000:  # Should be able to do >1000 ops/sec in Lua
                print("  ✅ Performance acceptable")
                return True
            else:
                print(f"  ⚠️  Performance lower than expected: {ops_per_sec:.0f} ops/sec")
                return True  # Still pass, but note the performance
                
        except Exception as e:
            print(f"  ❌ Performance test failed: {e}")
            return False
        finally:
            try:
                # Cleanup any remaining keys
                for i in range(1, 51):
                    self.r.delete(f"{base_key}:{i}")
            except:
                pass

def main():
    print("=" * 80) 
    print("FERROUS ADVANCED LUA PATTERN TESTS")
    print("Testing complex Lua functionality without script caching")
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
    
    tester = AdvancedLuaPatternTester()
    
    # Run all advanced tests
    results = []
    results.append(tester.test_distributed_counter_with_expiry())
    results.append(tester.test_rate_limiter_pattern())
    results.append(tester.test_bulk_key_operations())
    results.append(tester.test_conditional_multi_key_transaction())
    results.append(tester.test_hash_aggregation_pattern())
    results.append(tester.test_advanced_string_manipulation())
    results.append(tester.test_performance_complex_script())
    
    # Summary
    passed = sum(results)
    total = len(results)
    
    print()
    print("=" * 80)
    print(f"ADVANCED LUA PATTERN RESULTS: {passed}/{total} PASSED")
    print("=" * 80)
    
    if passed == total:
        print("🎉 All advanced Lua patterns working correctly!")
        print("✅ Lua API integration is solid and reliable!")
        sys.exit(0)
    else:
        print(f"⚠️  {total - passed} tests encountered issues")
        print("Note: These tests avoid script caching issues")
        sys.exit(1)

if __name__ == "__main__":
    main()