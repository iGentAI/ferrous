# Ferrous Bit Operations Specification

## Overview

This document specifies the implementation of Redis-compatible bit operations for Ferrous. Bit operations treat Redis strings as arrays of bits, enabling memory-efficient storage of boolean values and efficient bitwise computations.

## Design Goals

1. **Full Redis Compatibility**: Match Redis bit operation semantics exactly
2. **Memory Efficiency**: Support billions of bits with minimal memory overhead
3. **Performance**: O(1) for single-bit operations, O(N) for bulk operations
4. **Thread Safety**: Integrate with Ferrous's sharded storage architecture

## Bit Addressing Scheme

Redis strings are treated as bit arrays with the following conventions:

- **Byte Order**: Strings are indexed from byte 0 (leftmost) to byte N-1 (rightmost)
- **Bit Order within Byte**: Bit 0 is the MSB (most significant bit), Bit 7 is the LSB
- **Overall Bit Indexing**: Bit offset 0 is MSB of byte 0; bit offset 7 is LSB of byte 0; bit offset 8 is MSB of byte 1

```
Byte:     [  Byte 0  ] [  Byte 1  ] [  Byte 2  ]
Bits:      0 1 2 3 4 5 6 7  8 9 ...
Example:   1 0 0 0 0 0 0 1  0 1 ...
           ↑               ↑
           bit 0 (MSB)     bit 8 (MSB of byte 1)
```

## Commands

### SETBIT

**Syntax**: `SETBIT key offset value`

**Description**: Sets or clears the bit at offset in the string value stored at key.

**Parameters**:
- `key`: The key name
- `offset`: Bit offset (0 to 2^32-1). Maximum offset is 4294967295 (512MB string limit)
- `value`: 0 or 1

**Behavior**:
- If key does not exist, a new string value is created
- String is grown with zero bytes to accommodate the bit offset
- Returns the original bit value at the offset (0 or 1)
- Time complexity: O(1)

**Return Value**: Integer - the original bit value (0 or 1)

**Errors**:
- `ERR bit offset is not an integer or out of range` - if offset is negative or not an integer
- `ERR bit is not an integer or out of range` - if value is not 0 or 1
- `WRONGTYPE Operation against a key holding the wrong kind of value` - if key holds non-string

**Example**:
```
SETBIT mykey 7 1      # Returns 0 (previous value), sets LSB of byte 0
SETBIT mykey 0 1      # Returns 0, sets MSB of byte 0
GET mykey             # Returns "\x81" (binary 10000001)
```

### GETBIT

**Syntax**: `GETBIT key offset`

**Description**: Returns the bit value at offset in the string value stored at key.

**Parameters**:
- `key`: The key name
- `offset`: Bit offset (0 to 2^32-1)

**Behavior**:
- If key does not exist, returns 0
- If offset is beyond string length, returns 0 (out-of-range bits are always 0)
- Time complexity: O(1)

**Return Value**: Integer - 0 or 1

**Errors**:
- `ERR bit offset is not an integer or out of range` - if offset is negative or not an integer
- `WRONGTYPE Operation against a key holding the wrong kind of value` - if key holds non-string

**Example**:
```
SETBIT mykey 7 1
GETBIT mykey 0        # Returns 0
GETBIT mykey 7        # Returns 1
GETBIT mykey 100      # Returns 0 (beyond string length)
GETBIT nonexistent 0  # Returns 0
```

### BITCOUNT

**Syntax**: `BITCOUNT key [start end [BYTE|BIT]]`

**Description**: Counts the number of set bits (population count) in a string.

**Parameters**:
- `key`: The key name
- `start` (optional): Start position (default: 0)
- `end` (optional): End position (default: -1, meaning end of string)
- `BYTE|BIT` (optional, Redis 7.0+): Unit for start/end - BYTE (default) or BIT

**Behavior**:
- Without range: counts all set bits in the string
- With range: counts set bits in specified byte or bit range
- Negative indices count from end (-1 = last byte/bit)
- If key doesn't exist, returns 0
- Time complexity: O(N) where N is the number of bytes examined

**Return Value**: Integer - count of set bits

**Errors**:
- `ERR value is not an integer or out of range` - if start/end not integers
- `ERR syntax error` - if invalid unit specified
- `WRONGTYPE Operation against a key holding the wrong kind of value` - if key holds non-string

**Example**:
```
SET mykey "\xff\xf0\x00"    # 11111111 11110000 00000000
BITCOUNT mykey             # Returns 12
BITCOUNT mykey 0 0         # Returns 8 (first byte only)
BITCOUNT mykey 1 1         # Returns 4 (second byte only)
BITCOUNT mykey 0 -1        # Returns 12 (all bytes)
BITCOUNT mykey 0 7 BIT     # Returns 8 (first 8 bits)
```

### BITOP

**Syntax**: `BITOP operation destkey key [key ...]`

**Description**: Performs bitwise operations between strings and stores the result.

**Parameters**:
- `operation`: AND, OR, XOR, or NOT
- `destkey`: Destination key for the result
- `key [key ...]`: Source key(s)

**Operations**:
- `AND`: Bitwise AND of all input strings
- `OR`: Bitwise OR of all input strings
- `XOR`: Bitwise XOR of all input strings
- `NOT`: Bitwise NOT of a single input string (only one key allowed)

**Behavior**:
- Result string length equals the longest input string
- Missing keys are treated as zero-filled strings
- Shorter strings are zero-padded on the right for operations
- Time complexity: O(N) where N is total size of all strings

**Return Value**: Integer - the length of the result string in bytes

**Errors**:
- `ERR BITOP NOT requires one and only one key` - if NOT used with != 1 key
- `ERR syntax error` - if invalid operation
- `WRONGTYPE Operation against a key holding the wrong kind of value` - if any key holds non-string

**Example**:
```
SET key1 "\xff\x0f"    # 11111111 00001111
SET key2 "\x0f\xff"    # 00001111 11111111
BITOP AND destkey key1 key2   # "\x0f\x0f", returns 2
BITOP OR destkey key1 key2    # "\xff\xff", returns 2
BITOP XOR destkey key1 key2   # "\xf0\xf0", returns 2
BITOP NOT destkey key1        # "\x00\xf0", returns 2
```

### BITPOS

**Syntax**: `BITPOS key bit [start [end [BYTE|BIT]]]`

**Description**: Returns the position of the first bit set to 0 or 1 in a string.

**Parameters**:
- `key`: The key name
- `bit`: 0 or 1 - the bit value to search for
- `start` (optional): Start byte/bit position (default: 0)
- `end` (optional): End byte/bit position (default: -1)
- `BYTE|BIT` (optional): Unit for start/end - BYTE (default) or BIT

**Behavior**:
- Returns position of first bit matching the search value
- If key doesn't exist: returns 0 for bit=0, -1 for bit=1
- If no matching bit found in range: returns -1
- Empty string: returns -1 for both bit values
- Time complexity: O(N)

**Return Value**: Integer - bit position or -1 if not found

**Errors**:
- `ERR bit is not an integer or out of range` - if bit not 0 or 1
- `ERR value is not an integer or out of range` - if start/end not integers
- `WRONGTYPE Operation against a key holding the wrong kind of value` - if key holds non-string

**Example**:
```
SET mykey "\x00\xff\xf0"   # 00000000 11111111 11110000
BITPOS mykey 1             # Returns 8 (first 1-bit)
BITPOS mykey 0             # Returns 0 (first 0-bit)
BITPOS mykey 1 1 1         # Returns 8 (search byte 1 only)
BITPOS mykey 0 2 -1        # Returns 20 (first 0 in last byte)
```

### BITFIELD

**Syntax**: `BITFIELD key [GET encoding offset] [SET encoding offset value] [INCRBY encoding offset increment] [OVERFLOW WRAP|SAT|FAIL] ...`

**Description**: Operates on multiple bit fields within a string in a single atomic operation.

**Parameters**:
- `key`: The key name
- Sub-commands (can be chained):
  - `GET encoding offset`: Read a field
  - `SET encoding offset value`: Set a field
  - `INCRBY encoding offset increment`: Increment a field
  - `OVERFLOW WRAP|SAT|FAIL`: Set overflow behavior for subsequent operations

**Encodings**:
- `i<bits>`: Signed integer (e.g., i8, i16, i32, i64)
- `u<bits>`: Unsigned integer (e.g., u8, u16, u32, u63)
- Maximum bits: 64 for signed, 63 for unsigned

**Offset Types**:
- Plain number: bit offset from start
- `#<index>`: Field index (offset = index * encoding_bits)

**Overflow Behaviors**:
- `WRAP` (default): Wrap around on overflow
- `SAT`: Saturate at min/max value
- `FAIL`: Return nil on overflow, don't perform operation

**Behavior**:
- Multiple operations execute atomically
- Each operation returns its result in order
- String is grown as needed to accommodate offsets
- Time complexity: O(N) where N is number of sub-commands

**Return Value**: Array of integers (one per GET/SET/INCRBY operation)

**Example**:
```
BITFIELD mykey SET u8 0 200                    # [200] - set byte 0 to 200
BITFIELD mykey GET u8 0                        # [200] - read byte 0
BITFIELD mykey INCRBY u8 0 100 OVERFLOW WRAP   # [44] - 200+100 wraps to 44
BITFIELD mykey SET i8 0 -100 GET i8 0          # [-100, -100]
BITFIELD mykey SET u8 #0 10 SET u8 #1 20       # [10, 20] - set field 0 and 1
```

## Implementation Notes

### Storage Integration

Bit operations work on the `Value::String(Vec<u8>)` type in Ferrous. Key implementation details:

1. **String Growth**: SETBIT must grow strings with zero bytes when offset exceeds current length
2. **Byte Calculation**: `byte_index = offset / 8`, `bit_index = 7 - (offset % 8)` (MSB-first)
3. **Thread Safety**: All operations use the sharded storage engine's locking mechanisms
4. **Memory Tracking**: String growth must update memory accounting

### Error Handling

All bit operations must validate:
1. Key type (must be string or non-existent)
2. Offset/range bounds
3. Bit values (0 or 1 for SETBIT/GETBIT/BITPOS)
4. Operation validity (BITOP)

### Performance Considerations

1. **SETBIT/GETBIT**: O(1) - direct byte access
2. **BITCOUNT**: O(N) - use population count intrinsics where available
3. **BITOP**: O(N) - process in 8-byte chunks for efficiency
4. **BITPOS**: O(N) - early termination when bit found

### Testing Requirements

1. **Unit Tests**: Each command with various inputs
2. **Edge Cases**:
   - Empty strings
   - Non-existent keys
   - Maximum offset (2^32-1)
   - Negative indices
   - Type errors
3. **Integration Tests**: Multi-command sequences
4. **Compatibility Tests**: Validate against Redis behavior

## Implementation Priority

1. **Phase 1 (Core)**: SETBIT, GETBIT, BITCOUNT, BITOP
2. **Phase 2 (Extended)**: BITPOS, BITFIELD (basic GET/SET)
3. **Phase 3 (Advanced)**: BITFIELD INCRBY with overflow modes

## References

- [Redis SETBIT](https://redis.io/commands/setbit/)
- [Redis GETBIT](https://redis.io/commands/getbit/)
- [Redis BITCOUNT](https://redis.io/commands/bitcount/)
- [Redis BITOP](https://redis.io/commands/bitop/)
- [Redis BITPOS](https://redis.io/commands/bitpos/)
- [Redis BITFIELD](https://redis.io/commands/bitfield/)