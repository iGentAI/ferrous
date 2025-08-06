import redis
import sys
import time

print('Testing redis-py Lua deadlock...')
r = redis.Redis(host='localhost', port=6379, decode_responses=True, socket_timeout=5)

try:
    print('1. Basic PING...')
    result = r.ping()
    print(f'PING result: {result}')
    
    print('2. Basic EVAL...')
    result = r.eval('return 123', 0)
    print(f'EVAL result: {result}')
    
    print('3. SCRIPT LOAD...')
    sha = r.script_load('return 456')
    print(f'SCRIPT LOAD SHA: {sha}')
    
    print('4. EVALSHA...')
    result = r.evalsha(sha, 0)
    print(f'EVALSHA result: {result}')
    
    print('All tests completed successfully!')
except Exception as e:
    print(f'Error: {e}')
    sys.exit(1)
