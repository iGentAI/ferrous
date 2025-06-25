#!/bin/bash

# Set password for authentication
PASSWORD="mysecretpassword"

echo "Testing LIST commands:"
redis-cli -h 127.0.0.1 -p 6379 -a $PASSWORD LPUSH mylist "item1" "item2" "item3"
redis-cli -h 127.0.0.1 -p 6379 -a $PASSWORD LRANGE mylist 0 -1

echo -e "\nTesting SET commands:"
redis-cli -h 127.0.0.1 -p 6379 -a $PASSWORD SADD myset "member1" "member2" "member3"
redis-cli -h 127.0.0.1 -p 6379 -a $PASSWORD SMEMBERS myset

echo -e "\nTesting HASH commands:"
redis-cli -h 127.0.0.1 -p 6379 -a $PASSWORD HSET myhash field1 "value1" field2 "value2"
redis-cli -h 127.0.0.1 -p 6379 -a $PASSWORD HGETALL myhash

echo -e "\nTesting TRANSACTIONS:"
# Run as a single transaction in a single connection using redis-cli interactive mode
redis-cli -h 127.0.0.1 -p 6379 -a $PASSWORD << EOF
MULTI
SET key1 "transaction value1"
INCR counter
EXEC
EOF

echo -e "\nTesting PUB/SUB:"
redis-cli -h 127.0.0.1 -p 6379 -a $PASSWORD PUBLISH mychannel "Hello, subscribers!"

echo -e "\nTesting SORTED SET commands:"
redis-cli -h 127.0.0.1 -p 6379 -a $PASSWORD ZADD myzset 1 "one" 2 "two" 3 "three"
redis-cli -h 127.0.0.1 -p 6379 -a $PASSWORD ZRANGE myzset 0 -1 WITHSCORES
