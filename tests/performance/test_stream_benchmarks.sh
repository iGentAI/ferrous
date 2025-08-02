#!/bin/bash
# Stream Performance Benchmarks for Ferrous

echo "======================================"
echo "FERROUS STREAM PERFORMANCE BENCHMARKS"
echo "======================================"

# Ensure clean state
redis-cli -p 6379 DEL perfstream >/dev/null 2>&1

echo "1. XADD Performance Test (10k operations)"
redis-benchmark -h 127.0.0.1 -p 6379 -n 10000 -t xadd -d 100 -q
echo ""

echo "2. XLEN Performance Test (10k operations)"
# First add some entries for XLEN to measure
for i in {1..100}; do
    redis-cli -p 6379 XADD perfstream "*" index $i >/dev/null
done

time_start=$(date +%s.%N)
for i in {1..1000}; do
    redis-cli -p 6379 XLEN perfstream >/dev/null
done
time_end=$(date +%s.%N)

duration=$(echo "$time_end - $time_start" | bc)
ops_per_sec=$(echo "1000 / $duration" | bc)
echo "XLEN: ${ops_per_sec} ops/sec (1000 operations in ${duration}s)"
echo ""

echo "3. XRANGE Performance Test" 
time_start=$(date +%s.%N)
for i in {1..1000}; do
    redis-cli -p 6379 XRANGE perfstream - + COUNT 10 >/dev/null
done
time_end=$(date +%s.%N)

duration=$(echo "$time_end - $time_start" | bc)
ops_per_sec=$(echo "1000 / $duration" | bc)
echo "XRANGE (with COUNT): ${ops_per_sec} ops/sec"
echo ""

echo "4. XREVRANGE Performance Test"
time_start=$(date +%s.%N)
for i in {1..1000}; do
    redis-cli -p 6379 XREVRANGE perfstream + - COUNT 10 >/dev/null
done
time_end=$(date +%s.%N)

duration=$(echo "$time_end - $time_start" | bc)
ops_per_sec=$(echo "1000 / $duration" | bc) 
echo "XREVRANGE (with COUNT): ${ops_per_sec} ops/sec"
echo ""

echo "5. XTRIM Performance Test"
# Create a larger stream for trimming
for i in {1..1000}; do
    redis-cli -p 6379 XADD trimstream "*" data $i >/dev/null
done

time_start=$(date +%s.%N)
for i in {1..100}; do
    redis-cli -p 6379 XTRIM trimstream MAXLEN 500 >/dev/null
done
time_end=$(date +%s.%N)

duration=$(echo "$time_end - $time_start" | bc)
ops_per_sec=$(echo "100 / $duration" | bc)
echo "XTRIM: ${ops_per_sec} ops/sec"
echo ""

echo "6. XREAD Performance Test"
time_start=$(date +%s.%N)
for i in {1..500}; do
    redis-cli -p 6379 XREAD STREAMS perfstream 0-0 >/dev/null
done
time_end=$(date +%s.%N)

duration=$(echo "$time_end - $time_start" | bc)
ops_per_sec=$(echo "500 / $duration" | bc)
echo "XREAD: ${ops_per_sec} ops/sec"
echo ""

echo "7. Consumer Group Performance Test"
redis-cli -p 6379 XGROUP CREATE perfstream testgroup 0-0 >/dev/null 2>&1

time_start=$(date +%s.%N)
for i in {1..100}; do
    redis-cli -p 6379 XGROUP CREATE perfstream "group$i" 0-0 >/dev/null 2>&1
    redis-cli -p 6379 XGROUP DESTROY perfstream "group$i" >/dev/null 2>&1
done
time_end=$(date +%s.%N)

duration=$(echo "$time_end - $time_start" | bc)
ops_per_sec=$(echo "200 / $duration" | bc)  # 100 CREATE + 100 DESTROY = 200 ops
echo "XGROUP CREATE/DESTROY: ${ops_per_sec} ops/sec"
echo ""

echo "8. Mixed Workload Performance (Stream Operations)"
time_start=$(date +%s.%N)
for i in {1..200}; do
    redis-cli -p 6379 XADD mixedstream "*" op "add$i" >/dev/null
    redis-cli -p 6379 XLEN mixedstream >/dev/null
    redis-cli -p 6379 XRANGE mixedstream - + COUNT 5 >/dev/null
    if [ $((i % 50)) -eq 0 ]; then
        redis-cli -p 6379 XTRIM mixedstream MAXLEN 150 >/dev/null
    fi
done
time_end=$(date +%s.%N)

duration=$(echo "$time_end - $time_start" | bc)
total_ops=$(echo "200 * 3 + 4" | bc)  # 200*(ADD+LEN+RANGE) + 4*TRIM
ops_per_sec=$(echo "$total_ops / $duration" | bc)
echo "Mixed workload: ${ops_per_sec} ops/sec ($total_ops operations)"
echo ""

# Cleanup
redis-cli -p 6379 DEL perfstream trimstream mixedstream >/dev/null 2>&1

echo "======================================"
echo "STREAM PERFORMANCE BENCHMARKS COMPLETE"
echo ""
echo "Expected Performance Targets:"
echo "- XADD: >50k ops/sec"
echo "- XLEN/XRANGE: >30k ops/sec"
echo "- XTRIM: >10k ops/sec"
echo "- Consumer Groups: >5k ops/sec"
echo "======================================"