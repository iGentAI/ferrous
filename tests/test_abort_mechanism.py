import redis

try:
    r = redis.Redis(host='localhost', port=6379, decode_responses=True, socket_timeout=5)
    print('Testing REDIS_CALL_ABORT mechanism...')
    print('1. PING:', r.ping())
    
    print('2. Testing multi-statement script (should abort on error):')
    try:
        result = r.eval('redis.call("SET", "test", "abc"); redis.call("INCR", "test")', 0)
        print('ERROR: Should have aborted but got:', result)
    except Exception as e:
        print('SUCCESS: Multi-statement script aborted correctly')
        print('Exception type:', type(e).__name__)
        print('Error message:', str(e)[:100])
        
except Exception as e:
    print('Connection failed:', e)
