import redis
import sys

print('Debugging SCRIPT LOAD issue...')
r = redis.Redis(host='localhost', port=6379, decode_responses=True, socket_timeout=2)

try:
    print('1. Testing basic SCRIPT LOAD...')
    sha = r.script_load('return KEYS[1] .. ARGV[1]')
    print(f'SHA: {sha}')
    
    print('2. Testing SCRIPT EXISTS...')
    exists = r.script_exists(sha)
    print(f'EXISTS: {exists}')
    
    print('3. Testing EVALSHA with cached script...')
    result = r.evalsha(sha, 1, 'hello', 'world')
    print(f'EVALSHA result: {result}')
    
except redis.ResponseError as e:
    print(f'Redis error: {e}')
except redis.TimeoutError as e:
    print(f'Timeout error: {e}')
except Exception as e:
    print(f'Other error: {e}')
