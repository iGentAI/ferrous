import redis
import sys

try:
    r = redis.Redis(host='localhost', port=6379, decode_responses=True, socket_timeout=5)
    
    print('Testing the exact script that causes compilation errors:')
    print('Script: redis.pcall("SET", "str_key2", "abc"); redis.pcall("INCR", "str_key2")')
    
    try:
        result = r.eval('redis.pcall("SET", "str_key2", "abc"); redis.pcall("INCR", "str_key2")', 0)
        print(f'Success! Result: {result}')
    except Exception as e:
        print(f'COMPILATION ERROR: {type(e).__name__}: {e}')
        print('This proves the string processing issue!')
        
except Exception as e:
    print(f'Connection failed: {e}')
