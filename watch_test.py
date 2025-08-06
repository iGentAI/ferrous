import redis

# Test WATCH mechanism with proper connection handling
r = redis.Redis(host='127.0.0.1', port=6379, decode_responses=True)
print(f'Testing WATCH mechanism - Server PING: {r.ping()}')

# Test 1: Normal transaction (should succeed)
r.delete('watch_normal')
r.set('watch_normal', 'initial')
pipe = r.pipeline()
pipe.watch('watch_normal')
pipe.multi()
pipe.set('watch_normal', 'updated')
result1 = pipe.execute()
final_value1 = r.get('watch_normal')
print(f'Test 1 - Normal WATCH: result={result1}, final_value={final_value1}')

# Test 2: Violation case (should abort)
r.delete('watch_violation')
r.set('watch_violation', 'initial')
pipe = r.pipeline()
pipe.watch('watch_violation')
pipe.multi()
pipe.set('watch_violation', 'transaction_value')

# Cause violation from another connection
r2 = redis.Redis(host='127.0.0.1', port=6379, decode_responses=True)
r2.set('watch_violation', 'external_modification')

# Execute (should abort)
result2 = pipe.execute()
final_value2 = r.get('watch_violation')
print(f'Test 2 - WATCH violation: result={result2}, final_value={final_value2}')

# Summary
if result1 == [True] and final_value1 == 'updated':
    print('✅ Normal WATCH working correctly')
    test1_pass = True
else:
    print(f'❌ Normal WATCH failed: expected [True] and "updated", got {result1} and {final_value1}')
    test1_pass = False

if result2 is None and final_value2 == 'external_modification':
    print('✅ WATCH violation detection working correctly')  
    test2_pass = True
else:
    print(f'❌ WATCH violation failed: expected None and "external_modification", got {result2} and {final_value2}')
    test2_pass = False

if test1_pass and test2_pass:
    print('🎉 WATCH mechanism working correctly!')
    exit(0)
else:
    print('❌ WATCH mechanism has issues')
    exit(1)
