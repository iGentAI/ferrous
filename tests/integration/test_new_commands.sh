#!/bin/bash
# Test script for newly implemented Redis commands in Ferrous

echo "=========================================="
echo "TESTING NEWLY IMPLEMENTED COMMANDS"
echo "=========================================="

# Test database selection and DBSIZE
echo "Testing SELECT and DBSIZE commands..."

# Set up data in different databases
redis-cli -p 6379 SELECT 0
redis-cli -p 6379 SET db0_key1 "value1"
redis-cli -p 6379 SET db0_key2 "value2"

redis-cli -p 6379 SELECT 1
redis-cli -p 6379 SET db1_key1 "value1"

redis-cli -p 6379 SELECT 2
redis-cli -p 6379 SET db2_key1 "value1"
redis-cli -p 6379 SET db2_key2 "value2" 
redis-cli -p 6379 SET db2_key3 "value3"

# Test DBSIZE in each database
echo "Database sizes:"
redis-cli -p 6379 SELECT 0
echo -n "DB 0: "
redis-cli -p 6379 DBSIZE
redis-cli -p 6379 SELECT 1
echo -n "DB 1: "
redis-cli -p 6379 DBSIZE
redis-cli -p 6379 SELECT 2  
echo -n "DB 2: "
redis-cli -p 6379 DBSIZE

echo ""
echo "Testing SETNX command..."
redis-cli -p 6379 SELECT 0

# Test SETNX on new key (should succeed)
result1=$(redis-cli -p 6379 SETNX new_key "new_value")
echo "SETNX on new key: $result1"

# Test SETNX on existing key (should fail)
result2=$(redis-cli -p 6379 SETNX db0_key1 "different_value")
echo "SETNX on existing key: $result2"

# Verify original value unchanged
value=$(redis-cli -p 6379 GET db0_key1)
echo "Original value preserved: $value"

echo ""
echo "Testing SETEX command..."
redis-cli -p 6379 SETEX expire_key 2 "expiring_value"
echo "Set key with 2 second expiration"

echo "Value before expiration:"
redis-cli -p 6379 GET expire_key

echo "TTL:"
redis-cli -p 6379 TTL expire_key

echo ""
echo "Testing PSETEX command..."
redis-cli -p 6379 PSETEX ms_expire_key 1500 "ms_expiring_value"
echo "Set key with 1500 ms expiration"

echo "Value before expiration:"
redis-cli -p 6379 GET ms_expire_key

echo "TTL in milliseconds:"
redis-cli -p 6379 PTTL ms_expire_key

echo ""
echo "Testing DECRBY command..."
redis-cli -p 6379 SET counter "10"
echo "Initial counter value:"
redis-cli -p 6379 GET counter

echo "After DECRBY 3:"
redis-cli -p 6379 DECRBY counter 3

echo "After DECRBY 5:"  
redis-cli -p 6379 DECRBY counter 5

echo ""
echo "Testing FLUSHDB command..."
echo "Before FLUSHDB in database 2:"
redis-cli -p 6379 SELECT 2
redis-cli -p 6379 DBSIZE

redis-cli -p 6379 FLUSHDB
echo "After FLUSHDB in database 2:"
redis-cli -p 6379 DBSIZE

echo "Database 0 should be unaffected:"
redis-cli -p 6379 SELECT 0
redis-cli -p 6379 DBSIZE

echo ""
echo "Testing FLUSHALL command..."
echo "Before FLUSHALL - Total keys across all DBs:"
redis-cli -p 6379 SELECT 0
db0_size=$(redis-cli -p 6379 DBSIZE)
redis-cli -p 6379 SELECT 1
db1_size=$(redis-cli -p 6379 DBSIZE)
redis-cli -p 6379 SELECT 2
db2_size=$(redis-cli -p 6379 DBSIZE)
echo "DB0: $db0_size, DB1: $db1_size, DB2: $db2_size"

redis-cli -p 6379 FLUSHALL
echo "After FLUSHALL - All databases should be empty:"
redis-cli -p 6379 SELECT 0
db0_size=$(redis-cli -p 6379 DBSIZE)
redis-cli -p 6379 SELECT 1  
db1_size=$(redis-cli -p 6379 DBSIZE)
redis-cli -p 6379 SELECT 2
db2_size=$(redis-cli -p 6379 DBSIZE)
echo "DB0: $db0_size, DB1: $db1_size, DB2: $db2_size"

echo ""
echo "Testing RENAMENX command..."
redis-cli -p 6379 SELECT 0
redis-cli -p 6379 SET rename_test1 "value1"
redis-cli -p 6379 SET rename_test2 "value2"

# Test successful rename (new key doesn't exist)
result1=$(redis-cli -p 6379 RENAMENX rename_test1 rename_success)
echo "RENAMENX to non-existing key: $result1"

# Verify the rename worked
value1=$(redis-cli -p 6379 GET rename_test1)
value2=$(redis-cli -p 6379 GET rename_success)
echo "Original key after rename: $value1 (should be nil)"
echo "New key after rename: $value2 (should be value1)"

# Test failed rename (new key already exists) 
result2=$(redis-cli -p 6379 RENAMENX rename_test2 rename_success)
echo "RENAMENX to existing key: $result2"

# Verify nothing changed
value3=$(redis-cli -p 6379 GET rename_test2)
value4=$(redis-cli -p 6379 GET rename_success)
echo "Source key unchanged: $value3 (should be value2)"
echo "Target key unchanged: $value4 (should be value1)"

echo ""
echo "Testing RANDOMKEY command..."
redis-cli -p 6379 FLUSHDB
echo "RANDOMKEY on empty database:"
empty_result=$(redis-cli -p 6379 RANDOMKEY)
echo "Empty DB result: '$empty_result' (should be nil)"

# Add some keys for RANDOMKEY testing
redis-cli -p 6379 SET random_key1 "value1" > /dev/null
redis-cli -p 6379 SET random_key2 "value2" > /dev/null  
redis-cli -p 6379 SET random_key3 "value3" > /dev/null

echo "RANDOMKEY with 3 keys:"
random_result=$(redis-cli -p 6379 RANDOMKEY)
echo "Random key result: $random_result (should be one of random_key1, random_key2, random_key3)"

# Verify the returned key exists
key_exists=$(redis-cli -p 6379 EXISTS "$random_result")
echo "Returned key exists: $key_exists (should be 1)"

echo ""
echo "=========================================="
echo "NEW COMMANDS TEST COMPLETED"
echo "=========================================="