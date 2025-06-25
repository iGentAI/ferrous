#!/bin/bash
# Basic test script for Ferrous server

echo "Testing Ferrous with redis-cli..."

# Set password for authentication
PASSWORD="mysecretpassword"

# Test PING command
echo "Testing PING..."
redis-cli -a $PASSWORD -p 6379 PING

# Test ECHO command
echo "Testing ECHO..."
redis-cli -a $PASSWORD -p 6379 ECHO "Hello Ferrous"

# Test SET and GET (basic)
echo "Testing SET/GET..."
redis-cli -a $PASSWORD -p 6379 SET test "value"
redis-cli -a $PASSWORD -p 6379 GET test

# Test QUIT
echo "Testing QUIT..."
echo "QUIT" | redis-cli -a $PASSWORD -p 6379

echo "Basic tests completed!"