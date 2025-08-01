#!/bin/bash

# PASSWORD="mysecretpassword"

# Test basic commands
echo "Testing basic commands..."
redis-cli -p 6379 SET mykey "myvalue"

echo "Testing LIST commands:"
redis-cli -h 127.0.0.1 -p 6379 LPUSH mylist "item1" "item2" "item3"
redis-cli -h 127.0.0.1 -p 6379 LRANGE mylist 0 -1

echo -e "\nTesting SET commands:"
redis-cli -h 127.0.0.1 -p 6379 SADD myset "member1" "member2" "member3"
redis-cli -h 127.0.0.1 -p 6379 SMEMBERS myset

echo -e "\nTesting HASH commands:"
redis-cli -h 127.0.0.1 -p 6379 HSET myhash field1 "value1" field2 "value2"
redis-cli -h 127.0.0.1 -p 6379 HGETALL myhash

echo -e "\nTesting TRANSACTIONS:"
# Run as a single transaction in a single connection using redis-cli interactive mode
redis-cli -h 127.0.0.1 -p 6379 << EOF
MULTI
SET key1 "transaction value1"
INCR counter
EXEC
EOF

echo -e "\nTesting PUB/SUB:"
redis-cli -h 127.0.0.1 -p 6379 PUBLISH mychannel "Hello, subscribers!"

echo -e "\nTesting SORTED SET commands:"
redis-cli -h 127.0.0.1 -p 6379 ZADD myzset 1 "one" 2 "two" 3 "three"
redis-cli -h 127.0.0.1 -p 6379 ZRANGE myzset 0 -1 WITHSCORES
