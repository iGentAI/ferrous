#!/bin/bash
# Benchmark script for Ferrous using redis-benchmark

echo "======================================"
echo "FERROUS BENCHMARK TEST"
echo "======================================"

# Check if Ferrous is running
if ! pgrep -f "ferrous" > /dev/null; then
    echo "⚠️  WARNING: Ferrous server is not running!"
    echo ""
    echo "For accurate benchmark results, start Ferrous with logging redirected:"
    echo ""
    echo "  ./target/release/ferrous master.conf > /dev/null 2>&1 &"
    echo ""
    echo "This prevents console logging from impacting performance."
    echo "Without this, SET operations may show ~34k ops/sec instead of ~72k ops/sec."
    echo ""
    read -p "Press Enter to continue anyway, or Ctrl+C to exit and start the server properly..."
fi

# Check if redis-benchmark is available
if ! command -v redis-benchmark &> /dev/null; then
    echo "redis-benchmark not found, installing..."
    sudo dnf install -y redis
fi

# Basic benchmark - only testing implemented commands
echo -e "\n1. PING Command Benchmark (10k requests)"
redis-benchmark -p 6379 -t ping -n 10000 -q

echo -e "\n2. SET Command Benchmark (10k requests)"
redis-benchmark -p 6379 -t set -n 10000 -q

echo -e "\n3. GET Command Benchmark (10k requests)"
redis-benchmark -p 6379 -t get -n 10000 -q

echo -e "\n4. Pipeline Test - PING (10k requests, pipeline of 10)"
redis-benchmark -p 6379 -t ping -n 10000 -P 10 -q

echo -e "\n5. Concurrent Clients Test (50 clients, 10k requests)"
redis-benchmark -p 6379 -t ping -n 10000 -c 50 -q

echo -e "\n6. INCR Command Benchmark"
redis-benchmark -p 6379 -t incr -n 10000 -q

echo -e "\n7. LPUSH/LPOP Commands"
redis-benchmark -p 6379 -t lpush -n 10000 -q
redis-benchmark -p 6379 -t lpop -n 10000 -q

echo -e "\n8. SADD Command"
redis-benchmark -p 6379 -t sadd -n 10000 -q

echo -e "\n9. HSET Command"
redis-benchmark -p 6379 -t hset -n 10000 -q

echo -e "\n10. Latency Test (5 second sample)"
timeout 5 redis-cli -p 6379 --latency-history -i 1 || echo "Latency test completed (5 second sample)"

echo -e "\n======================================"
echo "BENCHMARK COMPLETE"
echo ""
echo "Expected performance with logging redirected:"
echo "- PING: ~85k ops/sec"
echo "- SET:  ~72k ops/sec (vs ~34k with console logging)"
echo "- GET:  ~81k ops/sec"
echo "======================================"