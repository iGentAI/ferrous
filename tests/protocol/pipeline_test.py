#!/usr/bin/env python3

import redis
import time
import threading
from concurrent.futures import ThreadPoolExecutor

# PASSWORD = 'mysecretpassword'

def run_pipeline_test(pipeline_size=100):
    """Run simple pipeline test and report performance"""
    # Connect to our server without password authentication
    client = redis.Redis(host='127.0.0.1', port=6379)
    
    # Pipeline test
    pipeline = client.pipeline(transaction=False)
    start_time = time.time()
    
    # Add commands to the pipeline
    for i in range(pipeline_size):
        pipeline.ping()
        pipeline.set(f'key:{i}', f'value:{i}')
        pipeline.get(f'key:{i}')
        
    # Execute the pipeline
    results = pipeline.execute()
    end_time = time.time()
    
    # Calculate ops/sec
    total_ops = len(results)
    elapsed = end_time - start_time
    ops_per_sec = total_ops / elapsed
    
    print(f"Pipeline test results:")
    print(f"Total commands: {total_ops}")
    print(f"Time taken: {elapsed:.4f} seconds")
    print(f"Operations per second: {ops_per_sec:.2f}")
    print(f"All operations succeeded: {all(r is not None and r != '' for r in results)}")
    
    return ops_per_sec

def run_concurrent_test(num_clients=10, pipeline_size=10):
    """Run concurrent clients test with pipelining"""
    print(f"\nConcurrent clients test ({num_clients} clients, {pipeline_size} pipeline size):")
    
    def client_worker(client_id):
        try:
            client = redis.Redis(host='127.0.0.1', port=6379)
            
            # Pipeline test for this client
            pipeline = client.pipeline(transaction=False)
            
            # Add commands to the pipeline
            for i in range(pipeline_size):
                pipeline.set(f'client:{client_id}:key:{i}', f'value:{i}')
                pipeline.get(f'client:{client_id}:key:{i}')
            
            # Execute the pipeline
            results = pipeline.execute()
            client.close()
            
            return all(r is not None and r != '' for r in results)
        except Exception as e:
            print(f"Error in client {client_id}: {e}")
            return False
    
    # Use thread pool to run concurrent clients
    start_time = time.time()
    with ThreadPoolExecutor(max_workers=num_clients) as executor:
        futures = [executor.submit(client_worker, i) for i in range(num_clients)]
        results = [future.result() for future in futures]
    
    end_time = time.time()
    
    # Calculate stats
    elapsed = end_time - start_time
    total_ops = num_clients * pipeline_size * 2  # Each pipeline has SET/GET pairs
    ops_per_sec = total_ops / elapsed
    success_count = sum(results)
    
    print(f"Total commands: {total_ops}")
    print(f"Time taken: {elapsed:.4f} seconds")
    print(f"Operations per second: {ops_per_sec:.2f}")
    print(f"Success rate: {success_count}/{num_clients} clients ({success_count/num_clients*100:.1f}%)")
    
    return ops_per_sec

if __name__ == "__main__":
    # Run basic pipeline test
    pipeline_ops = run_pipeline_test(pipeline_size=100)
    
    # Try concurrent clients with increasing concurrency
    concurrent_5_ops = run_concurrent_test(num_clients=5, pipeline_size=20)
    concurrent_10_ops = run_concurrent_test(num_clients=10, pipeline_size=20)
    concurrent_25_ops = run_concurrent_test(num_clients=25, pipeline_size=20)
    concurrent_50_ops = run_concurrent_test(num_clients=50, pipeline_size=20)
    
    print("\nPerformance summary:")
    print(f"Pipeline: {pipeline_ops:.2f} ops/sec")
    print(f"5 concurrent clients: {concurrent_5_ops:.2f} ops/sec")
    print(f"10 concurrent clients: {concurrent_10_ops:.2f} ops/sec")
    print(f"25 concurrent clients: {concurrent_25_ops:.2f} ops/sec") 
    print(f"50 concurrent clients: {concurrent_50_ops:.2f} ops/sec")