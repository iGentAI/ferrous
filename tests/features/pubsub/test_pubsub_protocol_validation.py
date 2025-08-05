#!/usr/bin/env python3
"""
Protocol-level validation tests for Ferrous Pub/Sub implementation
Tests RESP2 format compliance and redis-py client compatibility
"""

import socket
import time
import threading
import redis
import sys
import struct

class PubSubProtocolTester:
    def __init__(self, host='127.0.0.1', port=6379):
        self.host = host
        self.port = port
        
    def parse_resp(self, data):
        """Parse RESP protocol response"""
        if not data:
            return None
            
        type_char = data[0:1]
        if type_char == b'+':  # Simple string
            end = data.find(b'\r\n')
            return data[1:end].decode()
        elif type_char == b'-':  # Error
            end = data.find(b'\r\n')
            return f"ERROR: {data[1:end].decode()}"
        elif type_char == b':':  # Integer
            end = data.find(b'\r\n')
            return int(data[1:end])
        elif type_char == b'$':  # Bulk string
            end = data.find(b'\r\n')
            length = int(data[1:end])
            if length == -1:
                return None
            start = end + 2
            return data[start:start + length]
        elif type_char == b'*':  # Array
            end = data.find(b'\r\n')
            count = int(data[1:end])
            elements = []
            pos = end + 2
            for i in range(count):
                # Parse each element recursively
                if pos >= len(data):
                    return None  # Incomplete response
                elem_end = self._find_element_end(data[pos:])
                if elem_end == -1:
                    return None
                elem_data = data[pos:pos + elem_end]
                elem = self.parse_resp(elem_data)
                elements.append(elem)
                pos += elem_end
            return elements
        return None
        
    def _find_element_end(self, data):
        """Find the end of a RESP element"""
        if not data:
            return -1
            
        type_char = data[0:1]
        if type_char in [b'+', b'-', b':']:
            # Simple types end with \r\n
            end = data.find(b'\r\n')
            return end + 2 if end != -1 else -1
        elif type_char == b'$':
            # Bulk string
            end = data.find(b'\r\n')
            if end == -1:
                return -1
            length = int(data[1:end])
            if length == -1:
                return end + 2
            return end + 2 + length + 2  # length marker + \r\n + data + \r\n
        elif type_char == b'*':
            # Array - need to parse recursively
            end = data.find(b'\r\n')
            if end == -1:
                return -1
            count = int(data[1:end])
            pos = end + 2
            for i in range(count):
                elem_end = self._find_element_end(data[pos:])
                if elem_end == -1:
                    return -1
                pos += elem_end
            return pos
        return -1
        
    def test_subscribe_response_format(self):
        """Test that SUBSCRIBE returns correct RESP2 format"""
        print("Testing SUBSCRIBE response format...")
        
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.connect((self.host, self.port))
        s.settimeout(2.0)
        
        try:
            # Send SUBSCRIBE command
            cmd = "*2\r\n$9\r\nSUBSCRIBE\r\n$12\r\ntest_channel\r\n"
            s.sendall(cmd.encode())
            
            # Read response
            data = s.recv(1024)
            print(f"Raw response: {repr(data)}")
            
            # Parse response
            parsed = self.parse_resp(data)
            print(f"Parsed response: {parsed}")
            
            # Validate RESP2 format for subscription confirmation
            # Expected: ["subscribe", "test_channel", 1]
            if parsed is None:
                print("❌ Failed to parse response")
                return False
                
            if not isinstance(parsed, list):
                print(f"❌ Response is not an array: {type(parsed)}")
                return False
                
            if len(parsed) < 3:
                print(f"❌ Response array too short: {len(parsed)} elements (expected 3)")
                return False
                
            if parsed[0] != b'subscribe' and parsed[0] != 'subscribe':
                print(f"❌ First element is not 'subscribe': {parsed[0]}")
                return False
                
            if parsed[1] != b'test_channel' and parsed[1] != 'test_channel':
                print(f"❌ Second element is not channel name: {parsed[1]}")
                return False
                
            if isinstance(parsed[2], int) and parsed[2] >= 1:
                print("✅ SUBSCRIBE response format is correct")
                return True
            else:
                print(f"❌ Third element is not a positive integer: {parsed[2]}")
                return False
                
        except Exception as e:
            print(f"❌ Exception during test: {e}")
            return False
        finally:
            s.close()
            
    def test_publish_message_format(self):
        """Test that published messages arrive in correct RESP2 format"""
        print("\nTesting PUBLISH message format...")
        
        # Use a dedicated connection for subscription
        sub_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sub_socket.connect((self.host, self.port))
        sub_socket.settimeout(3.0)
        
        try:
            # Subscribe first
            sub_cmd = "*2\r\n$9\r\nSUBSCRIBE\r\n$11\r\npubsub_test\r\n"
            sub_socket.sendall(sub_cmd.encode())
            
            # Read subscription confirmation
            confirm_data = sub_socket.recv(1024)
            print(f"Subscription confirmation: {repr(confirm_data)}")
            
            # Give it time to establish
            time.sleep(0.5)
            
            # Publish from a different connection
            pub_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            pub_socket.connect((self.host, self.port))
            
            pub_cmd = "*3\r\n$7\r\nPUBLISH\r\n$11\r\npubsub_test\r\n$10\r\ntest_value\r\n"
            pub_socket.sendall(pub_cmd.encode())
            pub_resp = pub_socket.recv(1024)
            pub_socket.close()
            
            # Read published message on subscriber
            msg_data = sub_socket.recv(1024)
            print(f"Message data: {repr(msg_data)}")
            
            # Parse the message
            parsed_msg = self.parse_resp(msg_data)
            print(f"Parsed message: {parsed_msg}")
            
            # Validate message format
            # Expected: ["message", "pubsub_test", "test_value"]
            if parsed_msg is None:
                print("❌ Failed to parse message")
                return False
                
            if not isinstance(parsed_msg, list):
                print(f"❌ Message is not an array: {type(parsed_msg)}")
                return False
                
            if len(parsed_msg) < 3:
                print(f"❌ Message array too short: {len(parsed_msg)} elements (expected 3)")
                return False
                
            if parsed_msg[0] != b'message' and parsed_msg[0] != 'message':
                print(f"❌ First element is not 'message': {parsed_msg[0]}")
                return False
                
            if parsed_msg[1] != b'pubsub_test' and parsed_msg[1] != 'pubsub_test':
                print(f"❌ Second element is not channel name: {parsed_msg[1]}")
                return False
                
            if parsed_msg[2] != b'test_value' and parsed_msg[2] != 'test_value':
                print(f"❌ Third element is not message content: {parsed_msg[2]}")
                return False
                
            print("✅ PUBLISH message format is correct")
            return True
            
        except socket.timeout:
            print("❌ Timeout waiting for published message")
            return False
        except Exception as e:
            print(f"❌ Exception during test: {e}")
            return False
        finally:
            sub_socket.close()
            
    def test_redis_py_compatibility(self):
        """Test compatibility with redis-py client"""
        print("\nTesting redis-py client compatibility...")
        
        try:
            # Create redis-py client
            r = redis.Redis(host=self.host, port=self.port, decode_responses=True)
            
            # Create pubsub object
            pubsub = r.pubsub()
            
            # Subscribe to channel
            try:
                pubsub.subscribe('redis_py_test')
                print("✅ redis-py subscribe succeeded")
            except Exception as e:
                print(f"❌ redis-py subscribe failed: {e}")
                if "list index out of range" in str(e):
                    print("  ⚠️  This is the IndexError mentioned in the report!")
                return False
                
            # Test getting messages
            try:
                # Get subscription confirmation
                msg = pubsub.get_message(timeout=2.0)
                print(f"Subscription message: {msg}")
                
                if msg is None:
                    print("❌ No subscription confirmation received")
                    return False
                    
                if msg.get('type') != 'subscribe':
                    print(f"❌ Wrong message type: {msg.get('type')}")
                    return False
                    
            except Exception as e:
                print(f"❌ Failed to get subscription message: {e}")
                if "response[1]" in str(e) or "list index out of range" in str(e):
                    print("  ⚠️  RESP2 protocol violation detected!")
                return False
                
            # Publish a message
            r.publish('redis_py_test', 'Hello from redis-py')
            
            # Try to receive the message
            try:
                msg = pubsub.get_message(timeout=2.0)
                print(f"Published message: {msg}")
                
                if msg and msg.get('type') == 'message':
                    print("✅ redis-py pub/sub working correctly")
                    return True
                else:
                    print("❌ Did not receive published message correctly")
                    return False
                    
            except Exception as e:
                print(f"❌ Failed to get published message: {e}")
                return False
                
        except Exception as e:
            print(f"❌ redis-py test failed: {e}")
            return False
        finally:
            try:
                pubsub.close()
            except:
                pass
                
    def test_pattern_subscribe_format(self):
        """Test PSUBSCRIBE response format"""
        print("\nTesting PSUBSCRIBE response format...")
        
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.connect((self.host, self.port))
        s.settimeout(2.0)
        
        try:
            # Send PSUBSCRIBE command
            cmd = "*2\r\n$10\r\nPSUBSCRIBE\r\n$7\r\ntest:*\r\n"
            s.sendall(cmd.encode())
            
            # Read response
            data = s.recv(1024)
            print(f"Raw response: {repr(data)}")
            
            # Parse response
            parsed = self.parse_resp(data)
            print(f"Parsed response: {parsed}")
            
            # Expected: ["psubscribe", "test:*", 1]
            if parsed and isinstance(parsed, list) and len(parsed) >= 3:
                if (parsed[0] == b'psubscribe' or parsed[0] == 'psubscribe') and isinstance(parsed[2], int):
                    print("✅ PSUBSCRIBE response format is correct")
                    return True
                    
            print("❌ PSUBSCRIBE response format is incorrect")
            return False
            
        except Exception as e:
            print(f"❌ Exception during test: {e}")
            return False
        finally:
            s.close()

def main():
    print("=" * 70)
    print("FERROUS PUB/SUB PROTOCOL VALIDATION TEST SUITE")
    print("=" * 70)
    
    # Check if server is running
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.connect(('127.0.0.1', 6379))
        s.sendall(b"*1\r\n$4\r\nPING\r\n")
        resp = s.recv(1024)
        s.close()
        
        if b"PONG" not in resp:
            print("❌ Server not responding correctly")
            sys.exit(1)
    except:
        print("❌ Cannot connect to server")
        sys.exit(1)
        
    print("✅ Server connection verified")
    print()
    
    tester = PubSubProtocolTester()
    
    # Run tests
    results = []
    results.append(tester.test_subscribe_response_format())
    results.append(tester.test_publish_message_format())
    results.append(tester.test_redis_py_compatibility())
    results.append(tester.test_pattern_subscribe_format())
    
    # Summary
    passed = sum(results)
    total = len(results)
    
    print()
    print("=" * 70)
    print(f"PUB/SUB PROTOCOL TEST RESULTS: {passed}/{total} PASSED")
    print("=" * 70)
    
    if passed == total:
        print("🎉 All pub/sub protocol tests passed!")
        sys.exit(0)
    else:
        print("❌ Some pub/sub protocol tests failed")
        print("   This confirms the reported RESP2 protocol issues")
        sys.exit(1)

if __name__ == "__main__":
    main()