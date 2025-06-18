#!/bin/bash
# Benchmark script for Ferrous using redis-benchmark

echo "======================================"
echo "FERROUS BENCHMARK TEST"
echo "======================================"

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

echo -e "\n6. Latency Test"
redis-cli -p 6379 --latency-history

echo -e "\n======================================"
echo "BENCHMARK COMPLETE"
echo "======================================"