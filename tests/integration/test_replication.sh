#!/bin/bash

# Test script for Ferrous replication
# Tests master-slave replication functionality

set -e

# Set password for authentication
PASSWORD="mysecretpassword"

# Clean up on exit
function cleanup() {
    echo "Cleaning up..."
    pkill -f "ferrous master.conf" || true
    pkill -f "ferrous replica.conf" || true
    echo "Done."
}

trap cleanup EXIT

# Ensure the data directories exist
mkdir -p data/master data/replica

# Start the master server
echo "Starting master server..."
./target/release/ferrous master.conf > master.log 2>&1 &
MASTER_PID=$!

# Wait for the master to start
sleep 2

# Start the replica server
echo "Starting replica server..."
./target/release/ferrous replica.conf > replica.log 2>&1 &
REPLICA_PID=$!

# Wait for the replica to start and connect to master
sleep 5

# Check replication status
echo "Checking master replication status..."
MASTER_STATUS=$(redis-cli -h 127.0.0.1 -p 6379 -a $PASSWORD INFO replication)
echo "$MASTER_STATUS"

echo "Checking replica replication status..."
REPLICA_STATUS=$(redis-cli -h 127.0.0.1 -p 6380 -a $PASSWORD INFO replication)
echo "$REPLICA_STATUS"

# Test replication by setting a key on the master and checking if it appears on the replica
echo "Testing replication..."
redis-cli -h 127.0.0.1 -p 6379 -a $PASSWORD SET replication_test "Replication is working!"

# Wait for replication to propagate
echo "Waiting for replication to propagate..."
sleep 2

# Check if the key exists on the replica
RESULT=$(redis-cli -h 127.0.0.1 -p 6380 -a $PASSWORD GET replication_test)
if [ "$RESULT" == "Replication is working!" ]; then
    echo "✅ Replication test passed!"
else
    echo "❌ Replication test failed: Key not replicated. Got: '$RESULT'"
    exit 1
fi

# Test role change by promoting the replica to a master
echo "Testing promotion of replica to master..."
redis-cli -h 127.0.0.1 -p 6380 -a $PASSWORD REPLICAOF NO ONE

sleep 2

# Check that the former replica is now a master
NEW_MASTER_STATUS=$(redis-cli -h 127.0.0.1 -p 6380 -a $PASSWORD INFO replication)
echo "$NEW_MASTER_STATUS"

if [[ "$NEW_MASTER_STATUS" == *"role:master"* ]]; then
    echo "✅ Promotion test passed!"
else
    echo "❌ Promotion test failed: Replica was not promoted to master."
    exit 1
fi

# Test making the new master a replica of the original master
echo "Testing role change back to replica..."
redis-cli -h 127.0.0.1 -p 6380 -a $PASSWORD REPLICAOF 127.0.0.1 6379

sleep 2

# Check that the server is now a replica again
BACK_TO_REPLICA_STATUS=$(redis-cli -h 127.0.0.1 -p 6380 -a $PASSWORD INFO replication)
echo "$BACK_TO_REPLICA_STATUS"

if [[ "$BACK_TO_REPLICA_STATUS" == *"role:slave"* ]]; then
    echo "✅ Role change test passed!"
else
    echo "❌ Role change test failed: Master was not demoted to replica."
    exit 1
fi

echo "All tests passed successfully!"
exit 0