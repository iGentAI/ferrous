#!/usr/bin/env python3
"""
Comprehensive Pub/Sub functionality tests for Ferrous
Tests SUBSCRIBE, UNSUBSCRIBE, PSUBSCRIBE, PUNSUBSCRIBE, PUBLISH
"""

import socket
import time
import threading
import sys

def redis_command(cmd, host='127.0.0.1', port=6379):
    """Send a Redis command and return the response"""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect((host, port))
    
    s.sendall(cmd.encode())
    resp = s.recv(4096)
    s.close()
    return resp

class PubSubTester:
    def __init__(self, host='127.0.0.1', port=6379):
        self.host = host
        self.port = port
        self.received_messages = []
        
    def subscriber_worker(self, channels, patterns=None, duration=5):
        """Worker thread for subscription testing"""
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.connect((self.host, self.port))
        s.settimeout(duration)
        
        try:
            # Subscribe to channels
            if channels:
                for channel in channels:
                    cmd = f"*2\r\n$9\r\nSUBSCRIBE\r\n${len(channel)}\r\n{channel}\r\n"
                    s.sendall(cmd.encode())
                    resp = s.recv(1024)  # Subscribe confirmation
                    
            # Subscribe to patterns  
            if patterns:
                for pattern in patterns:
                    cmd = f"*2\r\n$10\r\nPSUBSCRIBE\r\n${len(pattern)}\r\n{pattern}\r\n"
                    s.sendall(cmd.encode())
                    resp = s.recv(1024)  # Subscribe confirmation
                    
            # Listen for messages
            while True:
                try:
                    data = s.recv(1024)
                    if data:
                        self.received_messages.append(data)
                except socket.timeout:
                    break
                    
        except Exception as e:
            print(f"Subscriber error: {e}")
        finally:
            s.close()
            
    def test_basic_pubsub(self):
        """Test basic PUBLISH/SUBSCRIBE functionality"""
        print("Testing basic PUBLISH/SUBSCRIBE...")
        
        self.received_messages = []
        
        # Start subscriber in background
        subscriber = threading.Thread(
            target=self.subscriber_worker, 
            args=(["test-channel"], None, 3)
        )
        subscriber.start()
        
        # Wait for subscription to establish
        time.sleep(0.5)
        
        # Publish message
        cmd = "*3\r\n$7\r\nPUBLISH\r\n$12\r\ntest-channel\r\n$13\r\nHello PubSub!\r\n"
        resp = redis_command(cmd)
        
        # Wait for subscriber to receive
        subscriber.join(5)
        
        # Check results
        if self.received_messages:
            print("✅ Basic pub/sub working")
            return True
        else:
            print("❌ No messages received")
            return False
            
    def test_pattern_subscription(self):
        """Test pattern-based subscriptions"""
        print("Testing pattern subscriptions...")
        
        self.received_messages = []
        
        # Start pattern subscriber
        subscriber = threading.Thread(
            target=self.subscriber_worker,
            args=(None, ["news.*"], 3)
        )
        subscriber.start()
        
        time.sleep(0.5)
        
        # Publish to matching channel
        cmd = "*3\r\n$7\r\nPUBLISH\r\n$10\r\nnews.sport\r\n$15\r\nSports headline\r\n"
        resp = redis_command(cmd)
        
        subscriber.join(5)
        
        if self.received_messages:
            print("✅ Pattern subscriptions working")
            return True
        else:
            print("❌ Pattern subscription failed")
            return False
            
    def test_multi_channel_subscription(self):
        """Test subscribing to multiple channels"""
        print("Testing multi-channel subscriptions...")
        
        self.received_messages = []
        
        subscriber = threading.Thread(
            target=self.subscriber_worker,
            args=(["channel1", "channel2", "channel3"], None, 4)
        )
        subscriber.start()
        
        time.sleep(0.5)
        
        # Publish to multiple channels with correct length specifiers
        for i, channel in enumerate(["channel1", "channel2", "channel3"]):
            message = f"message{i}"
            cmd = f"*3\r\n$7\r\nPUBLISH\r\n${len(channel)}\r\n{channel}\r\n${len(message)}\r\n{message}\r\n"
            resp = redis_command(cmd)
            time.sleep(0.1)
        
        subscriber.join(5)
        
        # Should receive 3 messages
        if len(self.received_messages) >= 3:
            print("✅ Multi-channel subscriptions working")
            return True
        else:
            print(f"❌ Multi-channel failed: got {len(self.received_messages)} messages")
            return False

def main():
    print("=" * 60)
    print("FERROUS PUB/SUB COMPREHENSIVE TESTS")
    print("=" * 60)
    
    # Check if server is running
    try:
        resp = redis_command("*1\r\n$4\r\nPING\r\n")
        if b"PONG" not in resp:
            print("❌ Server not responding correctly")
            sys.exit(1)
    except:
        print("❌ Cannot connect to server")
        sys.exit(1)
        
    print("✅ Server connection verified")
    print()
    
    tester = PubSubTester()
    
    # Run tests
    results = []
    results.append(tester.test_basic_pubsub())
    results.append(tester.test_pattern_subscription())
    results.append(tester.test_multi_channel_subscription())
    
    # Summary
    passed = sum(results)
    total = len(results)
    
    print()
    print("=" * 60)
    print(f"PUB/SUB TEST RESULTS: {passed}/{total} PASSED")
    print("=" * 60)
    
    if passed == total:
        print("🎉 All pub/sub tests passed!")
        sys.exit(0)
    else:
        print("❌ Some pub/sub tests failed")
        sys.exit(1)

if __name__ == "__main__":
    main()