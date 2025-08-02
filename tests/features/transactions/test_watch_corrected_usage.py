#!/usr/bin/env python3
"""
Corrected WATCH mechanism test using proper redis-py connection patterns
Tests WATCH/MULTI/EXEC with proper connection consistency as required by Redis specification
"""

import redis
import sys

def test_watch_with_proper_connection_management():
    """Test WATCH using proper redis-py connection consistency patterns"""
    print("Testing WATCH with proper connection management...")
    
    # Create connection pool for proper connection consistency
    pool = redis.ConnectionPool(host='localhost', port=6379, db=0)
    r1 = redis.Redis(connection_pool=pool, decode_responses=True)
    r2 = redis.Redis(host='localhost', port=6379, decode_responses=True)  # External modifier
    
    # Clean up
    r1.delete('watch:corrected')
    
    print("Step 1: Set initial key value")
    r1.set('watch:corrected', 'initial_value')
    
    print("Step 2: Use CORRECTED WATCH pattern with connection consistency")
    try:
        with r1.pipeline() as pipe:
            # WATCH and transaction on SAME pipeline/connection
            pipe.watch('watch:corrected')
            
            # External modification (should trigger violation)
            print("Step 3: External modification on different connection")
            r2.set('watch:corrected', 'external_modification')
            
            # Transaction on SAME connection as WATCH
            print("Step 4: Execute transaction on SAME connection as WATCH")
            pipe.multi()
            pipe.set('watch:corrected', 'transaction_value')
            pipe.set('watch_corrected_indicator', 'transaction_executed')
            result = pipe.execute()
            
            print(f"Transaction result: {result}")
            
            if result is None:
                print("✅ WATCH mechanism working correctly (transaction aborted)")
                return True
            else:
                print("❌ WATCH mechanism failed - transaction should have been aborted")
                return False
                
    except redis.WatchError:
        print("✅ WATCH mechanism working correctly (WatchError exception)")
        return True
        
    except Exception as e:
        print(f"❌ Unexpected error: {e}")
        return False
    finally:
        # Cleanup
        r1.delete('watch:corrected', 'watch_corrected_indicator')

def test_watch_with_stream_operations():
    """Test WATCH mechanism with Stream operations using corrected patterns"""
    print("Testing WATCH with Stream operations...")
    
    pool = redis.ConnectionPool(host='localhost', port=6379, db=0)
    r1 = redis.Redis(connection_pool=pool, decode_responses=True)
    r2 = redis.Redis(host='localhost', port=6379, decode_responses=True)
    
    stream_key = 'watch:stream:test'
    r1.delete(stream_key)
    
    print("Step 1: Create initial stream entry")
    r1.xadd(stream_key, {'init': 'data'})
    
    print("Step 2: Use corrected WATCH pattern with streams")
    try:
        with r1.pipeline() as pipe:
            # WATCH the stream on same connection
            pipe.watch(stream_key)
            
            # External stream modification
            print("Step 3: External stream modification")
            r2.xadd(stream_key, {'external': 'modification'})
            
            # Transaction on same connection
            print("Step 4: Execute transaction that should fail")
            pipe.multi()
            pipe.xadd(stream_key, {'transaction': 'should_fail'})
            pipe.set('stream_watch_indicator', 'executed')
            result = pipe.execute()
            
            if result is None:
                print("✅ Stream WATCH working correctly (transaction aborted)")
                success = True
            else:
                print("❌ Stream WATCH failed - transaction executed when it should abort")
                success = False
                
    except redis.WatchError:
        print("✅ Stream WATCH working correctly (WatchError exception)")
        success = True
        
    except Exception as e:
        print(f"❌ Error in Stream WATCH test: {e}")
        success = False
    
    # Cleanup
    r1.delete(stream_key, 'stream_watch_indicator')
    return success

def test_watch_cross_connection_violations():
    """Test that cross-connection modifications properly trigger WATCH violations"""
    print("Testing cross-connection WATCH violations...")
    
    pool = redis.ConnectionPool(host='localhost', port=6379, db=0)
    r1 = redis.Redis(connection_pool=pool, decode_responses=True)  # WATCH/EXEC connection
    r2 = redis.Redis(host='localhost', port=6379, decode_responses=True)  # External modifier
    
    test_key = 'cross:connection:test'
    r1.delete(test_key)
    
    print("Step 1: Set initial key value")
    r1.set(test_key, 'initial_value')
    
    print("Step 2: Establish WATCH on connection 1")
    print("Step 3: Modify key on connection 2") 
    print("Step 4: Execute transaction on connection 1 - should abort")
    
    try:
        with r1.pipeline() as pipe:
            # WATCH on pipeline connection
            pipe.watch(test_key)
            
            # External modification from different connection
            r2.set(test_key, 'external_modification')
            
            # Transaction on SAME connection as WATCH
            pipe.multi()
            pipe.set(test_key, 'transaction_value')
            pipe.set('cross_connection_indicator', 'transaction_executed')
            result = pipe.execute()
            
            # Validate proper violation detection
            if result is None:
                final_value = r1.get(test_key)
                expected_values = ['external_modification', b'external_modification']
                if final_value in expected_values:
                    print("✅ Cross-connection WATCH violations properly detected")
                    success = True
                else:
                    print(f"❌ Wrong final value: {final_value}")
                    success = False
            else:
                indicator_exists = r1.exists('cross_connection_indicator')
                print(f"❌ Transaction executed when should abort. Indicator: {indicator_exists}")
                success = False
                
    except redis.WatchError:
        print("✅ Cross-connection WATCH working via exception handling")
        final_value = r1.get(test_key)
        expected_values = ['external_modification', b'external_modification']
        success = (final_value in expected_values)
        
    except Exception as e:
        print(f"❌ Error in cross-connection test: {e}")
        success = False
    
    # Cleanup
    r1.delete(test_key, 'cross_connection_indicator')
    return success

def run_all_corrected_watch_tests():
    """Run all corrected WATCH tests with proper redis-py usage patterns"""
    print("=" * 70)
    print("FERROUS WATCH MECHANISM - CORRECTED USAGE TESTS")
    print("=" * 70)
    print()
    
    try:
        # Test connection
        r = redis.Redis(host='localhost', port=6379)
        r.ping()
        print("✅ Server connection verified\n")
    except Exception as e:
        print(f"❌ Cannot connect to server: {e}")
        return False
    
    tests_run = 0
    tests_passed = 0
    
    # Run all corrected test functions
    test_functions = [
        test_watch_with_proper_connection_management,
        test_watch_with_stream_operations,
        test_watch_cross_connection_violations,
    ]
    
    for test_func in test_functions:
        tests_run += 1
        try:
            if test_func():
                tests_passed += 1
            else:
                print(f"❌ {test_func.__name__} failed")
        except Exception as e:
            print(f"❌ {test_func.__name__} failed with exception: {e}")
            import traceback
            traceback.print_exc()
    
    print("\n" + "=" * 70)
    print(f"CORRECTED WATCH TEST RESULTS: {tests_passed}/{tests_run} PASSED")
    print("=" * 70)
    
    if tests_passed == tests_run:
        print("🎉 ALL CORRECTED WATCH TESTS PASSED!")
        print("✅ WATCH mechanism working correctly with proper redis-py usage!")
        print("✅ Connection consistency requirements properly implemented!")
        return True
    else:
        print(f"⚠️  {tests_run - tests_passed} corrected tests failed")
        return False

if __name__ == "__main__":
    success = run_all_corrected_watch_tests()
    sys.exit(0 if success else 1)