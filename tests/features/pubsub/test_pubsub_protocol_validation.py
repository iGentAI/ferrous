#!/usr/bin/env python3
"""
Pub/Sub protocol validation test for Ferrous
Tests RESP2 protocol compliance for pub/sub operations
"""

import redis
import socket
import time
import threading

def test_subscribe_protocol_with_redis_py():
    """Test SUBSCRIBE protocol compliance using redis-py client"""
    print("Testing SUBSCRIBE protocol with redis-py...")
    
    try:
        # Create redis-py client
        r = redis.Redis(host='127.0.0.1', port=6379, decode_responses=False)
        
        # Create pubsub object
        pubsub = r.pubsub()
        
        # Try to subscribe to a channel
        try:
            pubsub.subscribe('test_channel')
            print("✅ Subscribe call succeeded")
            
            # Try to get the subscription confirmation message
            message = pubsub.get_message(timeout=1.0)
            if message:
                print(f"✅ Got subscription message: {message}")
                print(f"   Type: {message.get('type')}")
                print(f"   Channel: {message.get('channel')}")
                print(f"   Data: {message.get('data')}")
            else:
                print("❌ No subscription confirmation received")
                
        except IndexError as e:
            print(f"❌ IndexError during subscription: {e}")
            print("   This indicates RESP2 protocol format mismatch")
            return False
        except Exception as e:
            print(f"❌ Unexpected error: {e}")
            return False
        finally:
            try:
                pubsub.close()
            except:
                pass
                
        return True
        
    except redis.ConnectionError:
        print("❌ Cannot connect to Redis server")
        return False

def test_raw_subscribe_protocol():
    """Test raw SUBSCRIBE protocol to see exact response format"""
    print("\nTesting raw SUBSCRIBE protocol...")
    
    try:
        # Connect using raw socket
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.connect(('127.0.0.1', 6379))
        s.settimeout(1.0)
        
        # Send SUBSCRIBE command
        cmd = "*2\r\n$9\r\nSUBSCRIBE\r\n$12\r\ntest_channel\r\n"
        s.sendall(cmd.encode())
        
        # Read response
        response = s.recv(1024)
        print(f"Raw response bytes: {response}")
        print(f"Raw response repr: {repr(response)}")
        
        # Parse RESP
        if response.startswith(b'*'):
            # It's an array
            lines = response.decode('utf-8', errors='ignore').split('\r\n')
            array_size = int(lines[0][1:])
            print(f"Response is array of size: {array_size}")
            
            # Check if it's a nested array (incorrect for pub/sub)
            if lines[1].startswith('*'):
                print("❌ Response contains nested array - this is incorrect!")
                print("   Redis sends individual messages, not an array of arrays")
                return False
            else:
                print("✅ Response format looks correct")
                
        s.close()
        return True
        
    except Exception as e:
        print(f"❌ Error during raw protocol test: {e}")
        return False

def test_publish_subscribe_flow():
    """Test complete pub/sub flow with redis-py"""
    print("\nTesting complete publish/subscribe flow...")
    
    messages_received = []
    
    def subscriber_thread():
        try:
            r = redis.Redis(host='127.0.0.1', port=6379, decode_responses=False)
            pubsub = r.pubsub()
            
            # Subscribe
            pubsub.subscribe('test_channel')
            
            # Listen for messages
            for message in pubsub.listen():
                messages_received.append(message)
                if message['type'] == 'message':
                    break
                    
        except Exception as e:
            print(f"Subscriber error: {e}")
            
    # Start subscriber
    sub_thread = threading.Thread(target=subscriber_thread)
    sub_thread.start()
    
    # Wait for subscription
    time.sleep(0.5)
    
    # Publish message
    try:
        r = redis.Redis(host='127.0.0.1', port=6379)
        num_receivers = r.publish('test_channel', 'test message')
        print(f"Published to {num_receivers} subscribers")
    except Exception as e:
        print(f"Publish error: {e}")
        
    # Wait for subscriber to finish
    sub_thread.join(timeout=2.0)
    
    # Check results
    if len(messages_received) >= 2:
        print(f"✅ Received {len(messages_received)} messages")
        for i, msg in enumerate(messages_received):
            print(f"   Message {i}: type={msg.get('type')}, channel={msg.get('channel')}")
        return True
    else:
        print(f"❌ Only received {len(messages_received)} messages")
        return False

def test_multiple_channel_subscribe():
    """Test subscribing to multiple channels"""
    print("\nTesting multiple channel subscription...")
    
    try:
        r = redis.Redis(host='127.0.0.1', port=6379, decode_responses=False)
        pubsub = r.pubsub()
        
        # Subscribe to multiple channels
        channels = ['channel1', 'channel2', 'channel3']
        pubsub.subscribe(*channels)
        
        # Get all subscription confirmations
        confirmations = []
        for _ in range(len(channels)):
            msg = pubsub.get_message(timeout=1.0)
            if msg:
                confirmations.append(msg)
            else:
                break
                
        if len(confirmations) == len(channels):
            print(f"✅ Got all {len(channels)} subscription confirmations")
            return True
        else:
            print(f"❌ Only got {len(confirmations)} out of {len(channels)} confirmations")
            return False
            
    except Exception as e:
        print(f"❌ Error: {e}")
        return False
    finally:
        try:
            pubsub.close()
        except:
            pass

def main():
    print("=" * 60)
    print("FERROUS PUB/SUB PROTOCOL VALIDATION TEST")
    print("=" * 60)
    
    # Check if server is running
    try:
        r = redis.Redis(host='127.0.0.1', port=6379)
        r.ping()
        print("✅ Server connection verified\n")
    except:
        print("❌ Cannot connect to server on port 6379")
        return
    
    # Run tests
    results = []
    
    # Test 1: Basic subscribe with redis-py
    results.append(('SUBSCRIBE with redis-py', test_subscribe_protocol_with_redis_py()))
    
    # Test 2: Raw protocol check
    results.append(('Raw SUBSCRIBE protocol', test_raw_subscribe_protocol()))
    
    # Test 3: Full pub/sub flow
    results.append(('Complete pub/sub flow', test_publish_subscribe_flow()))
    
    # Test 4: Multiple channel subscription
    results.append(('Multiple channel subscribe', test_multiple_channel_subscribe()))
    
    # Summary
    print("\n" + "=" * 60)
    print("TEST SUMMARY")
    print("=" * 60)
    
    passed = sum(1 for _, result in results if result)
    total = len(results)
    
    for test_name, result in results:
        status = "✅ PASS" if result else "❌ FAIL"
        print(f"{test_name}: {status}")
    
    print(f"\nTotal: {passed}/{total} tests passed")
    
    if passed < total:
        print("\n⚠️ PUB/SUB PROTOCOL ISSUES DETECTED")
        print("The server is likely returning subscription confirmations in an incorrect format.")
        print("Redis-py expects individual messages, not an array of messages.")

if __name__ == "__main__":
    main()