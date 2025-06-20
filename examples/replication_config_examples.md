# Replication Configuration Examples

This document provides various example configurations for setting up replication with Ferrous. Each example includes configuration files and matching commands to help you understand different replication scenarios.

## Basic Master-Slave Setup

### Master Configuration (master.conf)

```
# Network settings
bind 127.0.0.1
port 6379
tcp-backlog 511
timeout 0
tcp-keepalive 300

# Authentication
requirepass mysecretpassword

# General settings
databases 16
loglevel notice
daemonize no

# RDB persistence
save 900 1
save 300 10
save 60 10000
dbfilename dump_master.rdb
dir ./data/master
```

### Replica Configuration (replica.conf)

```
# Network settings
bind 127.0.0.1
port 6380
tcp-backlog 511
timeout 0
tcp-keepalive 300

# Authentication
requirepass mysecretpassword

# General settings
databases 16
loglevel notice
daemonize no

# RDB persistence
save 900 1
save 300 10
save 60 10000
dbfilename dump_replica.rdb
dir ./data/replica

# Replication settings
replicaof 127.0.0.1 6379
```

### Starting the Servers

```bash
# Create data directories
mkdir -p data/master data/replica

# Start the master
./target/release/ferrous master.conf

# Start the replica (in a separate terminal)
./target/release/ferrous replica.conf
```

### Verifying Replication

```bash
# Check master status
redis-cli -h 127.0.0.1 -p 6379 -a mysecretpassword INFO replication

# Check replica status
redis-cli -h 127.0.0.1 -p 6380 -a mysecretpassword INFO replication
```

## Multi-Replica Setup

For multiple replicas, create additional configuration files with unique ports:

### Second Replica Configuration (replica2.conf)

```
# Network settings
bind 127.0.0.1
port 6381
tcp-backlog 511
timeout 0
tcp-keepalive 300

# Authentication
requirepass mysecretpassword

# General settings
databases 16
loglevel notice
daemonize no

# RDB persistence
save 900 1
save 300 10
save 60 10000
dbfilename dump_replica2.rdb
dir ./data/replica2

# Replication settings
replicaof 127.0.0.1 6379
```

### Starting Multiple Replicas

```bash
mkdir -p data/replica2

# Start the second replica
./target/release/ferrous replica2.conf
```

## Dynamic Configuration

You can also set up replication dynamically using the REPLICAOF command:

```bash
# Start two instances without replication
./target/release/ferrous --port 6379 &
./target/release/ferrous --port 6380 &

# Configure the second instance as a replica
redis-cli -h 127.0.0.1 -p 6380 REPLICAOF 127.0.0.1 6379
```

## Testing Replication

### Simple Test

```bash
# Set a key on the master
redis-cli -h 127.0.0.1 -p 6379 -a mysecretpassword SET testkey "Replication test"

# Check if it appears on the replica (may take a moment)
redis-cli -h 127.0.0.1 -p 6380 -a mysecretpassword GET testkey
```

### Automated Testing

```bash
# Run the included replication test script
./test_replication.sh
```

The test script performs:
1. Starting master and replica servers
2. Verifying replication connection
3. Testing data propagation
4. Testing role transitions (promotion/demotion)
5. Cleaning up processes

## Failover Scenario

To simulate a failover scenario:

```bash
# Start master and replica as usual

# Set some data on the master
redis-cli -h 127.0.0.1 -p 6379 -a mysecretpassword SET important_data "This data must survive failover"

# Verify replication
redis-cli -h 127.0.0.1 -p 6380 -a mysecretpassword GET important_data

# Simulate master failure
pkill -f "ferrous master.conf"

# Promote the replica to master
redis-cli -h 127.0.0.1 -p 6380 -a mysecretpassword REPLICAOF NO ONE

# Verify data is still available on the new master
redis-cli -h 127.0.0.1 -p 6380 -a mysecretpassword GET important_data

# Once the original master is restored, make it a replica of the new master
./target/release/ferrous master.conf &
redis-cli -h 127.0.0.1 -p 6379 -a mysecretpassword REPLICAOF 127.0.0.1 6380
```

## Authentication Notes

Both master and replica should have the same password set for successful authentication during replication. The replica will automatically use this password when connecting to the master.

## Troubleshooting

### Common Issues

1. **Connection Refused**: Check that the master is running and accessible on the specified port.
2. **Authentication Failed**: Ensure both master and replica have the same password.
3. **Synchronization Issues**: Check logs for any error messages during initial sync.

### Checking Logs

Run the servers with output directed to log files for better troubleshooting:

```bash
./target/release/ferrous master.conf > master.log 2>&1 &
./target/release/ferrous replica.conf > replica.log 2>&1 &
```

Then monitor the logs for any errors:

```bash
tail -f master.log
tail -f replica.log
```

## Conclusion

These examples should help you get started with replication in Ferrous. For more detailed information, refer to the [Replication Guide](REPLICATION_GUIDE.md) and the main documentation.