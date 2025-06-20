# Ferrous Replication Guide

This guide provides instructions for setting up and managing master-slave replication in Ferrous, the Redis-compatible server written in Rust.

## Overview

Ferrous supports master-slave replication, allowing you to create a read-scalable and highly available Redis-compatible deployment. Replication provides the following benefits:

- **Data Redundancy**: Replicas maintain a copy of the master's data
- **Read Scaling**: Distribute read operations across replicas
- **High Availability**: Promote a replica to master in case of master failure
- **Data Security**: Create offline backups using replicas without impacting the master

## Prerequisites

- Ferrous v0.1.0 or higher
- Network connectivity between master and replica instances
- Sufficient disk space for RDB files
- Separate data directories for master and replica instances

## Basic Setup

### Method 1: Using Configuration Files

1. **Create data directories**:
   ```bash
   mkdir -p data/master data/replica
   ```

2. **Create configuration files**:

   **master.conf**:
   ```
   bind 127.0.0.1
   port 6379
   requirepass mysecretpassword
   dir ./data/master
   dbfilename dump_master.rdb
   ```

   **replica.conf**:
   ```
   bind 127.0.0.1
   port 6380
   requirepass mysecretpassword
   dir ./data/replica
   dbfilename dump_replica.rdb
   replicaof 127.0.0.1 6379
   ```

3. **Start the master server**:
   ```bash
   ./target/release/ferrous master.conf
   ```

4. **Start the replica server**:
   ```bash
   ./target/release/ferrous replica.conf
   ```

### Method 2: Using Dynamic Configuration

1. **Start two Ferrous instances on different ports**:
   ```bash
   ./target/release/ferrous --port 6379 &
   ./target/release/ferrous --port 6380 &
   ```

2. **Configure replication with the REPLICAOF command**:
   ```bash
   redis-cli -h 127.0.0.1 -p 6380 REPLICAOF 127.0.0.1 6379
   ```

## Monitoring Replication Status

Check replication status using the INFO command:

```bash
redis-cli -h 127.0.0.1 -p 6379 INFO replication  # Master status
redis-cli -h 127.0.0.1 -p 6380 INFO replication  # Replica status
```

### Master Status Example
```
# Replication
role:master
connected_slaves:1
slave0:ip=127.0.0.1,port=6380,state=online,offset=42,lag=0
repl_id:8c13b54aeabd5c371235d10d4f41d71c80836994
repl_offset:42
```

### Replica Status Example
```
# Replication
role:slave
master_host:127.0.0.1
master_port:6379
master_link_status:up
slave_repl_offset:42
master_repl_id:8c13b54aeabd5c371235d10d4f41d71c80836994
```

## Role Transitions

### Promoting a Replica to Master

To promote a replica to master (for instance, during maintenance or failover):

```bash
redis-cli -h 127.0.0.1 -p 6380 REPLICAOF NO ONE
```

This disconnects the replica from its master and promotes it to operate as an independent master.

### Converting a Master to Replica

To demote a master to become a replica of another instance:

```bash
redis-cli -h 127.0.0.1 -p 6379 REPLICAOF 192.168.1.100 6379
```

## Authentication

Ferrous replication supports authentication to secure the replication link. When a replica connects to a master that requires authentication, the replica will automatically send an AUTH command with the password configured in the replica.

In the configuration file, specify the same password for both master and replica:

```
# In both master.conf and replica.conf
requirepass mysecretpassword
```

## Testing Replication

Ferrous includes a replication test script that validates basic functionality:

```bash
./test_replication.sh
```

This test script performs:
1. Master and replica startup
2. Replication connection verification
3. Data propagation test
4. Role transition tests
5. Clean shutdown

## Troubleshooting

### Connection Issues
- Verify network connectivity between instances
- Check that the master's port is accessible from the replica
- Ensure that firewall rules allow the connection

### Authentication Problems
- Verify that the password is correctly set on both master and replica
- Check authentication status in logs

### Synchronization Issues
- Check the master_link_status in the replica's INFO output
- Examine server logs for RDB transfer errors
- Verify that data directories are writable

## Best Practices

1. **Network Security**:
   - Use authentication with strong passwords
   - Run instances on private networks when possible
   - Consider using SSH tunnels for remote replication

2. **Resource Management**:
   - Allocate sufficient memory to both master and replicas
   - Ensure adequate disk space for RDB files
   - Monitor network bandwidth for replication traffic

3. **Monitoring**:
   - Regularly check replication status using INFO
   - Monitor replication lag (shown in INFO output)
   - Set up alerts for master_link_status changes

4. **Backup Strategy**:
   - Perform backups from replicas to avoid impacting the master
   - Use BGSAVE on replicas for point-in-time snapshots
   - Test backups regularly

## Limitations

The current implementation has some limitations:

1. **Partial Synchronization**: Not yet implemented, full RDB transfer happens on reconnection
2. **Diskless Replication**: Not yet implemented, all transfers use disk
3. **SSL/TLS**: Not yet supported for replication connections
4. **Replica Chain**: Replicas cannot have their own replicas yet

## Conclusion

Ferrous replication provides a robust way to create highly available Redis-compatible deployments. By following this guide, you can set up and manage master-slave replication for your Ferrous instances, enabling more resilient and scalable applications.

For more information about other features, refer to the main documentation and the ARCHITECTURE.md file.