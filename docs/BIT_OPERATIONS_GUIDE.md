# Ferrous Bit Operations Usage Guide

## Introduction

Bit operations in Ferrous enable memory-efficient storage and manipulation of boolean values. This guide demonstrates practical use cases for SETBIT, GETBIT, BITCOUNT, BITOP, and BITPOS commands.

## Use Cases

### 1. User Activity Tracking

Track which users were active on specific days using minimal memory:

```bash
# Mark user IDs as active (1 bit per user)
SETBIT users:2025-11-27 12345 1    # User 12345 active today
SETBIT users:2025-11-27 67890 1    # User 67890 active today
SETBIT users:2025-11-27 99999 1    # User 99999 active today

# Check if specific user was active
GETBIT users:2025-11-27 12345       # Returns 1 (yes)
GETBIT users:2025-11-27 54321       # Returns 0 (no)

# Count total active users
BITCOUNT users:2025-11-27           # Returns 3

# Memory efficiency: 1 million users = ~122 KB (vs ~8 MB for set of integers)
```

### 2. Multi-Day Activity Analysis

Find users active on multiple days using BITOP:

```bash
# Track activity across multiple days
SETBIT users:day1 100 1
SETBIT users:day1 200 1
SETBIT users:day1 300 1

SETBIT users:day2 200 1
SETBIT users:day2 300 1
SETBIT users:day2 400 1

# Find users active BOTH days (intersection)
BITOP AND users:both_days users:day1 users:day2
BITCOUNT users:both_days              # Returns 2 (users 200, 300)

# Find users active on EITHER day (union)
BITOP OR users:any_day users:day1 users:day2
BITCOUNT users:any_day                # Returns 4 (users 100, 200, 300, 400)

# Find users active on day1 but NOT day2 (difference)
BITOP NOT users:day2_inverted users:day2
BITOP AND users:only_day1 users:day1 users:day2_inverted
BITCOUNT users:only_day1              # Returns 1 (user 100)
```

### 3. Feature Flags and A/B Testing

Manage feature flags for millions of users:

```bash
# Enable feature X for specific users
SETBIT feature:x:enabled 12345 1
SETBIT feature:x:enabled 67890 1

# Check if user has feature
GETBIT feature:x:enabled 12345        # Returns 1 (enabled)

# Count users with feature enabled
BITCOUNT feature:x:enabled            # Returns 2

# Move user from group A to group B
SETBIT experiment:group_a 12345 0
SETBIT experiment:group_b 12345 1
```

### 4. Real-Time Analytics

Track events with timestamp-based bit positions:

```bash
# Track hourly activity (24 hours = 24 bits)
# Bit position represents hour of day (0-23)
SETBIT activity:2025-11-27 9 1        # Active at 9 AM
SETBIT activity:2025-11-27 14 1       # Active at 2 PM
SETBIT activity:2025-11-27 18 1       # Active at 6 PM

# Check activity during business hours (9-17)
BITCOUNT activity:2025-11-27 1 2      # Count bits in bytes representing hours 8-23

# Find first active hour
BITPOS activity:2025-11-27 1          # Returns 9 (first 1-bit)
```

### 5. Simple Bloom Filter

Implement a basic membership test structure:

```bash
# Add items by setting bits at hash positions
# (In real implementation, use multiple hash functions)
SETBIT bloom:filter 42 1
SETBIT bloom:filter 137 1
SETBIT bloom:filter 891 1

# Test membership (may have false positives, never false negatives)
GETBIT bloom:filter 42                # Returns 1 (probably in set)
GETBIT bloom:filter 999               # Returns 0 (definitely not in set)
```

### 6. Permission Bits

Store user permissions compactly:

```bash
# Permission bits: READ=0, WRITE=1, EXECUTE=2, ADMIN=3
SETBIT user:12345:perms 0 1           # READ permission
SETBIT user:12345:perms 1 1           # WRITE permission
SETBIT user:12345:perms 3 1           # ADMIN permission

# Check permissions
GETBIT user:12345:perms 2             # Returns 0 (no EXECUTE)
GETBIT user:12345:perms 3             # Returns 1 (has ADMIN)

# Count granted permissions
BITCOUNT user:12345:perms             # Returns 3
```

## Performance Characteristics

| Operation | Time Complexity | Notes |
|-----------|----------------|-------|
| SETBIT | O(1) | Direct byte access, may grow string |
| GETBIT | O(1) | Direct byte access |
| BITCOUNT | O(N) | N = bytes examined, uses CPU intrinsics |
| BITOP | O(N) | N = length of longest string |
| BITPOS | O(N) | N = bytes examined, early termination |

## Memory Efficiency

Bit operations offer exceptional memory efficiency for boolean data:

| Data Structure | Memory per Item | 1M Items | 1B Items |
|----------------|----------------|----------|----------|
| Set of Integers | ~8-16 bytes | ~8-16 MB | ~8-16 GB |
| Bitmap (SETBIT) | 1 bit | ~122 KB | ~119 MB |
| **Savings** | **64-128x** | **64-128x** | **64-128x** |

## Best Practices

1. **Use consistent key naming**: `prefix:date` or `prefix:entity:date` patterns
2. **Set expiration on time-series data**: `EXPIRE users:2025-11-27 86400`
3. **Batch operations**: Use BITOP instead of multiple GET operations
4. **Range queries**: Use BITCOUNT with ranges to analyze specific time windows
5. **Combine with other data types**: Use bitmaps for fast filtering, then fetch full data from other structures

## Integration Examples

### Combined with Sets

```bash
# Bitmap for fast existence check
SETBIT active:quick 12345 1

# Set for full data
SADD active:full "user:12345:john"

# Fast check + detailed lookup
GETBIT active:quick 12345             # Fast existence check
SMEMBERS active:full                  # Full data retrieval
```

### Combined with Streams

```bash
# Track which events were processed
XADD events * user 12345 action login
SETBIT processed:events 12345 1      # Mark as processed

# Later: check if event was processed
GETBIT processed:events 12345         # Quick lookup
```

## Common Patterns

### Pattern 1: Time-Series Presence

```bash
# One key per day, bit offset = user ID
SETBIT active:2025-11-27 <user_id> 1

# Analyze weekly active users
BITOP OR active:week users:day1 users:day2 ... users:day7
BITCOUNT active:week
```

### Pattern 2: Tag-Based Filtering

```bash
# Each tag gets a bitmap
SETBIT tag:premium <user_id> 1
SETBIT tag:verified <user_id> 1

# Find premium AND verified users
BITOP AND users:premium_verified tag:premium tag:verified
```

### Pattern 3: Rate Limiting

```bash
# Track API calls in current minute (60 bits for 60 seconds)
SETBIT ratelimit:user123:minute <second> 1

# Count calls in this minute
BITCOUNT ratelimit:user123:minute

# Reset for new minute
DEL ratelimit:user123:minute
```

## Comparison with Other Approaches

| Scenario | Bitmap Approach | Alternative | Advantage |
|----------|----------------|-------------|-----------|
| 1B user activity tracking | ~119 MB | ~8-16 GB (Set) | 64-128x less memory |
| Daily active user count | Single BITCOUNT | Count set members | Faster, no iteration |
| Multi-day intersection | Single BITOP AND | Multiple SINTER | Single atomic operation |
| Permission checking | Single GETBIT O(1) | Hash lookup O(1) | More compact storage |

## See Also

- [BIT_OPERATIONS_SPEC.md](BIT_OPERATIONS_SPEC.md) - Technical specification
- [Redis Bit Operations](https://redis.io/docs/data-types/bitmaps/) - Redis documentation