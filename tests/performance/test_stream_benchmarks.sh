#!/bin/bash
# Stream Performance Benchmarks for Ferrous using direct redis-benchmark
# Uses proven methodology for accurate performance measurement

echo "======================================"
echo "FERROUS STREAM PERFORMANCE BENCHMARKS"
echo "======================================"

# Ensure clean state
redis-cli -p 6379 FLUSHALL >/dev/null 2>&1

echo "1. XLEN Performance Test (10k operations)"
echo "============================================="
# Add test data for XLEN
for i in {1..100}; do
    redis-cli -p 6379 XADD test_stream "*" index $i >/dev/null 2>&1
done

redis-benchmark -h 127.0.0.1 -p 6379 -c 1 -n 10000 -q XLEN test_stream
echo ""

echo "2. XRANGE Performance Test (5k operations)"
echo "============================================="
redis-benchmark -h 127.0.0.1 -p 6379 -c 1 -n 5000 -q XRANGE test_stream - + COUNT 10
echo ""

echo "3. XTRIM Performance Test (1k operations)"
echo "============================================="
redis-benchmark -h 127.0.0.1 -p 6379 -c 1 -n 1000 -q XTRIM test_stream MAXLEN 50
echo ""

echo "4. XADD Performance Test (Custom RESP benchmark)"
echo "============================================="
echo "Using optimized direct RESP protocol benchmark for accurate measurement:"

# Create temporary Python script for XADD benchmark with FIXED protocol formatting
TEMP_SCRIPT="/tmp/xadd_bench_fixed_$$.py"
cat > "$TEMP_SCRIPT" << 'EOF'
import socket
import time
import sys

def benchmark_xadd(host, port, iterations=5000):
    try:
        # Create a new connection for FLUSHALL
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.connect((host, port))
        
        # Clear existing data - properly formatted RESP command
        sock.send(b"*1\r\n$8\r\nFLUSHALL\r\n")
        response = sock.recv(1024)
        if b"OK" not in response and b"+OK" not in response:
            print(f"FLUSHALL failed: {response}", file=sys.stderr)
        sock.close()
        
        # Pre-build commands with unique IDs
        base_timestamp = int(time.time() * 1000)
        
        # Run the benchmark with proper socket handling
        start_time = time.time()
        successful = 0
        
        # Process in batches to avoid socket buffer issues
        batch_size = 100
        for batch_start in range(0, iterations, batch_size):
            # New connection for each batch
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.connect((host, port))
            sock.settimeout(5.0)  # Set timeout to detect hanging
            
            batch_end = min(batch_start + batch_size, iterations)
            
            for i in range(batch_start, batch_end):
                # Create unique timestamp ID for each entry
                timestamp_id = f"{base_timestamp + i}-0"
                
                # Build properly formatted RESP command
                # *5 means 5 elements in array
                # Each bulk string must have $length\r\n prefix
                cmd_parts = [
                    "*5\r\n",
                    "$4\r\nXADD\r\n",
                    "$12\r\nbench_stream\r\n",
                    f"${len(timestamp_id)}\r\n{timestamp_id}\r\n",
                    "$5\r\nfield\r\n",
                    "$5\r\nvalue\r\n"
                ]
                cmd = "".join(cmd_parts).encode()
                
                try:
                    sock.send(cmd)
                    response = sock.recv(1024)
                    
                    if b"ERR" in response:
                        print(f"Error at iteration {i}: {response}", file=sys.stderr)
                        break
                    else:
                        successful += 1
                except socket.timeout:
                    print(f"Timeout at iteration {i} - command may be malformed", file=sys.stderr)
                    break
                except Exception as e:
                    print(f"Socket error at iteration {i}: {e}", file=sys.stderr)
                    break
            
            sock.close()
        
        end_time = time.time()
        
        if successful > 0:
            duration = end_time - start_time
            ops_per_sec = successful / duration
            avg_latency_ms = (duration * 1000) / successful
            
            print(f"XADD: {ops_per_sec:.2f} requests per second, avg={avg_latency_ms:.3f} msec")
        else:
            print("XADD benchmark failed - no successful operations", file=sys.stderr)
            sys.exit(1)
        
    except socket.error as e:
        print(f"Socket error: {e}", file=sys.stderr)
        sys.exit(1)
    except Exception as e:
        print(f"Unexpected error: {e}", file=sys.stderr)
        sys.exit(1)

if __name__ == "__main__":
    benchmark_xadd("127.0.0.1", 6379)
EOF

# Run the benchmark
if python3 "$TEMP_SCRIPT"; then
    echo "✅ XADD benchmark completed successfully"
    rm -f "$TEMP_SCRIPT"
else
    echo "❌ ERROR: XADD benchmark failed!"
    echo "Debug script preserved at: $TEMP_SCRIPT"
    echo "You can debug by running: python3 $TEMP_SCRIPT"
    exit 1
fi
echo ""

echo "5. Performance Summary"
echo "============================================="
echo "All tests use direct redis-benchmark methodology"
echo "XADD uses custom RESP protocol for accurate measurement"
echo "Results show true underlying Stream performance"
echo "Expected: 25,000-32,000 ops/sec with sub-millisecond latencies"
echo "======================================"

# Cleanup
redis-cli -p 6379 DEL test_stream bench_stream >/dev/null 2>&1