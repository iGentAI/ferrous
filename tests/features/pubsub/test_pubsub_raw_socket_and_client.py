#!/usr/bin/env python3
"""
Comprehensive Pub/Sub Test for Raw Socket and Redis Client Library
Tests both SUBSCRIBE/PSUBSCRIBE functionality with raw RESP protocol and redis-py client
"""

import redis
import socket
import threading
import time
import sys

class ComprehensivePubSubTester:
    def __init__(self, host='127.0.0.1', port=6379):
        self.host = host
        self.port = port
        
    def test_raw_socket_subscribe(self):
        """Test SUBSCRIBE with raw socket RESP protocol"""
        print("Testing raw socket SUBSCRIBE...")
        
        try:
            # Test raw socket SUBSCRIBE
            s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            s.settimeout(5)  # 5 second timeout
            s.connect((self.host, self.port))
            
            # Send SUBSCRIBE command
            s.sendall(b'*2\r\n$9\r\nSUBSCRIBE\r\n$12\r\ntest_channel\r\n')
            
            # Should get immediate confirmation
            resp = s.recv(1024)
            if resp and b'subscribe' in resp.lower():
                print("  ✅ Raw socket SUBSCRIBE: Immediate confirmation received")
                s.close()
                return True
            else:
                print(f"  ❌ Raw socket SUBSCRIBE: Unexpected response: {resp}")
                s.close()
                return False
                
        except socket.timeout:
            print("  ❌ Raw socket SUBSCRIBE: TIMEOUT - confirmation not sent")
            return False
        except Exception as e:
            print(f"  ❌ Raw socket SUBSCRIBE failed: {e}")
            return False
    
    def test_raw_socket_psubscribe(self):
        """Test PSUBSCRIBE with raw socket RESP protocol - THE CRITICAL TEST"""
        print("Testing raw socket PSUBSCRIBE...")
        
        try:
            # Test raw socket PSUBSCRIBE
            s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            s.settimeout(5)  # 5 second timeout
            s.connect((self.host, self.port))
            
            # Send PSUBSCRIBE command
            s.sendall(b'*2\r\n$10\r\nPSUBSCRIBE\r\n$7\r\ntest:*\r\n')
            
            # Should get immediate confirmation
            resp = s.recv(1024)
            if resp and b'psubscribe' in resp.lower():
                print("  ✅ Raw socket PSUBSCRIBE: Immediate confirmation received")
                s.close()
                return True
            else:
                print(f"  ❌ Raw socket PSUBSCRIBE: Unexpected response: {resp}")
                s.close()
                return False
                
        except socket.timeout:
            print("  ❌ Raw socket PSUBSCRIBE: TIMEOUT - confirmation not sent (BUG)")
            return False
        except Exception as e:
            print(f"  ❌ Raw socket PSUBSCRIBE failed: {e}")
            return False
    
    def test_redis_client_subscribe(self):
        """Test SUBSCRIBE with redis-py client library"""
        print("Testing redis-py client SUBSCRIBE...")
        
        try:
            r = redis.Redis(host=self.host, port=self.port, decode_responses=False)
            pubsub = r.pubsub()
            
            # Subscribe to channel
            pubsub.subscribe('client_test_channel')
            
            # Get subscription confirmation
            message = pubsub.get_message(timeout=5.0)
            if message and message['type'] == 'subscribe':
                print("  ✅ Redis-py SUBSCRIBE: Client library confirmation received")
                pubsub.close()
                return True
            else:
                print(f"  ❌ Redis-py SUBSCRIBE: Unexpected message: {message}")
                pubsub.close()
                return False
                
        except Exception as e:
            print(f"  ❌ Redis-py SUBSCRIBE failed: {e}")
            return False
    
    def test_redis_client_psubscribe(self):
        """Test PSUBSCRIBE with redis-py client library"""
        print("Testing redis-py client PSUBSCRIBE...")
        
        try:
            r = redis.Redis(host=self.host, port=self.port, decode_responses=False)
            pubsub = r.pubsub()
            
            # Pattern subscribe
            pubsub.psubscribe('client:*')
            
            # Get subscription confirmation
            message = pubsub.get_message(timeout=5.0)
            if message and message['type'] == 'psubscribe':
                print("  ✅ Redis-py PSUBSCRIBE: Client library confirmation received")
                pubsub.close()
                return True
            else:
                print(f"  ❌ Redis-py PSUBSCRIBE: Unexpected message: {message}")
                pubsub.close()
                return False
                
        except Exception as e:
            print(f"  ❌ Redis-py PSUBSCRIBE failed: {e}")
            return False
    
    def test_pub_sub_integration(self):
        """Test full pub/sub integration with both patterns"""
        print("Testing pub/sub integration...")
        
        try:
            # Setup publisher
            pub_client = redis.Redis(host=self.host, port=self.port, decode_responses=True)
            
            # Setup subscriber for both channel and pattern
            sub_client = redis.Redis(host=self.host, port=self.port, decode_responses=False)
            pubsub = sub_client.pubsub()
            
            # Subscribe to both channel and pattern
            pubsub.subscribe('integration_test')
            pubsub.psubscribe('integration:*')
            
            # Get confirmations
            sub_confirm = pubsub.get_message(timeout=3.0)
            psub_confirm = pubsub.get_message(timeout=3.0)
            
            if not (sub_confirm and psub_confirm):
                print("  ❌ Integration: Subscription confirmations failed")
                return False
            
            # Publish message that matches both
            pub_client.publish('integration_test', 'direct_message')
            pub_client.publish('integration:match', 'pattern_message')
            
            # Should get messages
            msg1 = pubsub.get_message(timeout=3.0)
            msg2 = pubsub.get_message(timeout=3.0)
            
            messages_received = 2 if (msg1 and msg2) else (1 if (msg1 or msg2) else 0)
            
            if messages_received >= 2:
                print(f"  ✅ Integration: Received {messages_received} messages correctly")
                pubsub.close()
                return True
            else:
                print(f"  ❌ Integration: Only received {messages_received}/2 expected messages")
                pubsub.close()
                return False
                
        except Exception as e:
            print(f"  ❌ Integration test failed: {e}")
            return False

def main():
    print("=" * 80)
    print("FERROUS COMPREHENSIVE PUB/SUB VALIDATION")
    print("Testing both raw socket RESP protocol and redis-py client library")
    print("=" * 80)
    
    # Verify server connection
    try:
        r = redis.Redis(host='127.0.0.1', port=6379)
        r.ping()
        print("✅ Server connection verified")
    except Exception as e:
        print(f"❌ Cannot connect to server: {e}")
        sys.exit(1)
        
    print()
    
    tester = ComprehensivePubSubTester()
    
    # Run comprehensive pub/sub validation
    results = []
    results.append(tester.test_raw_socket_subscribe())
    results.append(tester.test_raw_socket_psubscribe())  # The critical test that was failing
    results.append(tester.test_redis_client_subscribe())
    results.append(tester.test_redis_client_psubscribe())
    results.append(tester.test_pub_sub_integration())
    
    # Summary
    passed = sum(results)
    total = len(results)
    
    print("\n" + "=" * 80)
    print(f"COMPREHENSIVE PUB/SUB RESULTS: {passed}/{total} PASSED")
    print("=" * 80)
    
    if passed == total:
        print("🎉 ALL PUB/SUB TESTS PASSED!")
        print("✅ Both raw socket and client library interfaces working")
        print("✅ PSUBSCRIBE protocol compliance validated")
        print("✅ No hanging or timeout issues")
        sys.exit(0)
    else:
        print(f"❌ PUB/SUB ISSUES: {total - passed} tests failed")
        print("⚠️ PSUBSCRIBE protocol compliance needs attention")
        sys.exit(1)

if __name__ == "__main__":
    main()