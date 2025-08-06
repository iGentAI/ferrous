import redis
import time

try:
    r = redis.Redis(host='localhost', port=6379, decode_responses=True, socket_timeout=3)
    print("1. Testing PING:", r.ping())
    
    print("2. Testing redis.call with error (this should hang):")
    try:
        result = r.eval('return redis.call("SET", "only_one_arg")', 0)
        print("ERROR: redis.call should have thrown error but returned:", result)
    except Exception as e:
        print("Correctly caught error:", type(e).__name__, str(e)[:100])
    
    print("3. Testing redis.pcall with error:")
    result = r.eval('return redis.pcall("SET", "only_one_arg")', 0)
    print("redis.pcall result:", result)
    
    print("All tests completed")
    
except Exception as e:
    print("Test failed with exception:", e)
