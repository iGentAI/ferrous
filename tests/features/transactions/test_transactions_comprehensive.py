#!/usr/bin/env python3
"""
Comprehensive transaction testing for Ferrous
Tests MULTI/EXEC/DISCARD/WATCH including edge cases and error scenarios
"""

import socket
import time
import threading
import sys

class TransactionTester:
    def __init__(self, host='127.0.0.1', port=6379):
        self.host = host
        self.port = port
        
    def send_commands_single_connection(self, commands):
        """Send multiple commands on a single connection and return all responses"""
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.connect((self.host, self.port))
        s.settimeout(5.0)
        
        responses = []
        try:
            for cmd in commands:
                s.sendall(cmd)
                resp = s.recv(4096)
                responses.append(resp)
        finally:
            s.close()
        
        return responses

def test_basic_transaction():
    """Test basic MULTI/EXEC functionality"""
    print("Testing basic transactions...")
    
    tester = TransactionTester()
    commands = [
        b"*1\r\n$5\r\nMULTI\r\n",  # Start transaction
        b"*3\r\n$3\r\nSET\r\n$3\r\ntx1\r\n$6\r\nvalue1\r\n",  # Queue SET
        b"*3\r\n$3\r\nSET\r\n$3\r\ntx2\r\n$6\r\nvalue2\r\n",  # Queue SET  
        b"*1\r\n$4\r\nEXEC\r\n",  # Execute
    ]
    
    responses = tester.send_commands_single_connection(commands)
    
    # Check responses: OK, QUEUED, QUEUED, [OK, OK]
    if (b"+OK" in responses[0] and 
        b"QUEUED" in responses[1] and 
        b"QUEUED" in responses[2] and
        b"*" in responses[3]):  # Array response from EXEC
        print("✅ Basic transaction working")
        return True
    else:
        print(f"❌ Transaction failed. Responses: {responses}")
        return False

def test_watch_violation():
    """Test WATCH key violations causing transaction abort"""
    print("Testing WATCH key violations...")
    
    tester = TransactionTester()
    
    # Clear any stale test state
    tester.send_commands_single_connection([
        b"*2\r\n$3\r\nDEL\r\n$7\r\nwatched\r\n"
    ])
    
    # Use single connection for entire WATCH sequence to preserve state
    watch_commands = [
        b"*3\r\n$3\r\nSET\r\n$7\r\nwatched\r\n$7\r\ninitial\r\n",  # Set initial value
        b"*2\r\n$5\r\nWATCH\r\n$7\r\nwatched\r\n",  # Start watching
        b"*3\r\n$3\r\nSET\r\n$7\r\nwatched\r\n$8\r\nmodified\r\n",  # External modification
        b"*1\r\n$5\r\nMULTI\r\n",  # Start transaction  
        b"*3\r\n$3\r\nSET\r\n$7\r\nwatched\r\n$11\r\ntransaction\r\n",  # Queue SET
        b"*1\r\n$4\r\nEXEC\r\n",  # Execute - should be aborted
        b"*2\r\n$3\r\nGET\r\n$7\r\nwatched\r\n"  # Check final value
    ]
    
    responses = tester.send_commands_single_connection(watch_commands)
    
    # The EXEC response should be null array (aborted) and GET should show "modified"
    exec_response = responses[-2]  # EXEC response
    get_response = responses[-1]   # GET response
    
    if exec_response == b"*-1\r\n":  # Null array means transaction aborted
        print("✅ WATCH violation correctly aborted transaction")
        return True
    elif b"modified" in get_response and b"transaction" not in get_response:
        print("✅ WATCH violation correctly aborted transaction (verified by key value)")
        return True
    else:
        print(f"❌ WATCH violation test failed. EXEC response: {exec_response}, GET response: {get_response}")
        return False

def test_discard():
    """Test DISCARD command"""
    print("Testing DISCARD functionality...")
    
    tester = TransactionTester()
    
    # First, clear any existing test keys to ensure clean state
    tester.send_commands_single_connection([
        b"*3\r\n$3\r\nDEL\r\n$8\r\ndiscard1\r\n$8\r\ndiscard2\r\n"
    ])
    
    commands = [
        b"*1\r\n$5\r\nMULTI\r\n",  # Start transaction
        b"*3\r\n$3\r\nSET\r\n$8\r\ndiscard1\r\n$6\r\nvalue1\r\n",  # Queue SET
        b"*3\r\n$3\r\nSET\r\n$8\r\ndiscard2\r\n$6\r\nvalue2\r\n",  # Queue SET
        b"*1\r\n$7\r\nDISCARD\r\n",  # Discard transaction
        b"*1\r\n$4\r\nEXEC\r\n",  # This should fail with "ERR EXEC without MULTI"
    ]
    
    responses = tester.send_commands_single_connection(commands)
    
    # Check that DISCARD succeeded and EXEC properly failed
    if (b"+OK" in responses[3] and  # DISCARD returns OK
        b"ERR EXEC without MULTI" in responses[4]):  # EXEC correctly fails
        # Verify keys were not set by checking one of them on same connection
        verify_resp = tester.send_commands_single_connection([
            b"*2\r\n$3\r\nGET\r\n$8\r\ndiscard1\r\n"
        ])
        
        if b"$-1" in verify_resp[0]:  # Null response
            print("✅ DISCARD correctly cancelled transaction")
            return True
        else:
            print(f"❌ DISCARD failed - key exists: {verify_resp[0]}")
            return False
    else:
        print(f"❌ DISCARD test failed. DISCARD response: {responses[3]}, EXEC response: {responses[4]}")
        return False

def test_transaction_errors():
    """Test error handling within transactions"""
    print("Testing transaction error handling...")
    
    tester = TransactionTester()
    commands = [
        b"*1\r\n$5\r\nMULTI\r\n",  # Start transaction
        b"*3\r\n$3\r\nSET\r\n$5\r\nerror\r\n$4\r\ntest\r\n",  # Valid command
        b"*2\r\n$3\r\nSET\r\n$3\r\nkey\r\n",  # Invalid command (missing value)
        b"*2\r\n$4\r\nINCR\r\n$7\r\ncounter\r\n",  # Valid command
        b"*1\r\n$4\r\nEXEC\r\n",  # Execute
    ]
    
    responses = tester.send_commands_single_connection(commands)
    
    # Should get MULTI OK, QUEUED, QUEUED, QUEUED, then EXEC array response
    exec_resp = responses[-1]
    
    # EXEC should return array with mixed success/error results
    if (b"*" in exec_resp and     # Array response
        b"+OK" in exec_resp and   # Success result
        b"-ERR" in exec_resp):    # Error result
        print("✅ Transaction error handling working")
        return True
    else:
        print(f"❌ Transaction error handling failed. EXEC response: {exec_resp}")
        return False

def main():
    print("=" * 60)
    print("FERROUS COMPREHENSIVE TRANSACTION TESTS")
    print("=" * 60)
    
    # Verify server connection
    try:
        tester = TransactionTester()
        responses = tester.send_commands_single_connection([b"*1\r\n$4\r\nPING\r\n"])
        if b"PONG" not in responses[0]:
            print("❌ Server not responding")
            sys.exit(1)
    except:
        print("❌ Cannot connect to server")
        sys.exit(1)
    
    print("✅ Server connection verified")
    print()
    
    # Run tests
    results = []
    results.append(test_basic_transaction())
    results.append(test_watch_violation())
    results.append(test_discard())
    results.append(test_transaction_errors())
    
    # Summary
    passed = sum(results)
    total = len(results)
    
    print()
    print("=" * 60)
    print(f"TRANSACTION TEST RESULTS: {passed}/{total} PASSED")
    print("=" * 60)
    
    if passed == total:
        print("🎉 All transaction tests passed!")
        sys.exit(0)
    else:
        print("❌ Some transaction tests failed")
        sys.exit(1)

if __name__ == "__main__":
    main()