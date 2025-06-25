#!/usr/bin/env python3
import redis
import time

# Connect to server
r = redis.StrictRedis(host='127.0.0.1', port=6379, password='mysecretpassword', decode_responses=True)

print("Testing SLOWLOG functionality...")

# Check initial slowlog
print("\n1. Initial SLOWLOG state:")
print(f"   SLOWLOG LEN: {r.slowlog_len()}")
print(f"   SLOWLOG GET: {r.slowlog_get()}")

# Get current threshold
config = r.config_get('slowlog-log-slower-than')
print(f"\n2. Current slowlog threshold: {config.get('slowlog-log-slower-than')} microseconds")

# Create a simple slow operation by doing many operations in pipeline
print("\n3. Creating slow operations...")

# Method 1: Large MSET operation
start = time.time()
large_data = {f'key{i}': f'value{i}' * 50 for i in range(500)}  # Large values
r.mset(large_data)
elapsed_ms = (time.time() - start) * 1000
print(f"   Large MSET took {elapsed_ms:.2f} ms")

# Method 2: Many individual operations without pipelining
start = time.time()
for i in range(100):
    r.set(f'slowkey{i}', 'x' * 1000)
elapsed_ms = (time.time() - start) * 1000
print(f"   100 individual SETs took {elapsed_ms:.2f} ms")

# Wait a moment for slowlog to update
time.sleep(0.1)

# Check slowlog now
print("\n4. SLOWLOG after operations:")
print(f"   SLOWLOG LEN: {r.slowlog_len()}")
slowlog_entries = r.slowlog_get(10)
print(f"   Found {len(slowlog_entries)} slow operations")

# Display entries
for i, entry in enumerate(slowlog_entries):
    print(f"\n   Entry {i + 1}:")
    print(f"     ID: {entry.get('id')}")
    print(f"     Duration: {entry.get('duration')} microseconds") 
    print(f"     Command: {' '.join(entry.get('command', []))[:100]}...")
    print(f"     Timestamp: {entry.get('start_time')}")

# Test SLOWLOG RESET
print("\n5. Testing SLOWLOG RESET...")
r.slowlog_reset()
print(f"   SLOWLOG LEN after reset: {r.slowlog_len()}")

print("\nSLOWLOG test completed!")
