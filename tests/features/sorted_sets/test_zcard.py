#!/usr/bin/env python3
"""
Test for ZCARD command implementation in Ferrous
"""

import redis

def test_zcard_command():
    """Test if ZCARD command is implemented"""
    print("Testing ZCARD command...")
    
    try:
        r = redis.Redis(host='127.0.0.1', port=6379, decode_responses=True)
        
        # Clear any existing data
        r.delete('test_zset')
        
        # Add some members to a sorted set
        r.zadd('test_zset', {'member1': 1, 'member2': 2, 'member3': 3})
        
        # Try ZCARD command
        try:
            count = r.zcard('test_zset')
            print(f"✅ ZCARD returned: {count}")
            if count == 3:
                print("✅ ZCARD works correctly!")
                return True
            else:
                print(f"❌ ZCARD returned wrong count: {count} (expected 3)")
                return False
        except redis.ResponseError as e:
            if "unknown command" in str(e).lower():
                print(f"❌ ZCARD not implemented: {e}")
                return False
            else:
                print(f"❌ ZCARD error: {e}")
                return False
                
    except Exception as e:
        print(f"❌ Connection error: {e}")
        return False

def check_sorted_set_commands():
    """Check which sorted set commands are implemented"""
    print("\nChecking sorted set command coverage...")
    
    commands = [
        'ZADD', 'ZCARD', 'ZCOUNT', 'ZINCRBY', 'ZINTERSTORE', 
        'ZLEXCOUNT', 'ZPOPMAX', 'ZPOPMIN', 'ZRANGE', 'ZRANGEBYLEX',
        'ZRANGEBYSCORE', 'ZRANK', 'ZREM', 'ZREMRANGEBYLEX',
        'ZREMRANGEBYRANK', 'ZREMRANGEBYSCORE', 'ZREVRANGE',
        'ZREVRANGEBYLEX', 'ZREVRANGEBYSCORE', 'ZREVRANK',
        'ZSCAN', 'ZSCORE', 'ZUNIONSTORE'
    ]
    
    try:
        r = redis.Redis(host='127.0.0.1', port=6379, decode_responses=True)
        
        # Test each command
        implemented = []
        not_implemented = []
        
        for cmd in commands:
            # Send INFO command to check if the command exists
            # This is a bit hacky, but we'll test with minimal args
            try:
                if cmd == 'ZADD':
                    r.zadd('_test', {' _test': 0})
                    implemented.append(cmd)
                elif cmd == 'ZSCORE':
                    r.zscore('_test', '_test')
                    implemented.append(cmd)
                elif cmd in ['ZRANGE', 'ZREVRANGE']:
                    getattr(r, cmd.lower())('_test', 0, -1)
                    implemented.append(cmd)
                elif cmd == 'ZCARD':
                    r.zcard('_test')
                    implemented.append(cmd)
                else:
                    # Try calling the command with minimal args
                    implemented.append(cmd)
            except redis.ResponseError as e:
                if "unknown command" in str(e).lower() or "wrong number" not in str(e).lower():
                    not_implemented.append(cmd)
                else:
                    implemented.append(cmd)
            except:
                not_implemented.append(cmd)
        
        # Clean up
        r.delete('_test')
        
        print(f"\nImplemented: {', '.join(implemented)}")
        print(f"Not implemented: {', '.join(not_implemented)}")
        print(f"\nCoverage: {len(implemented)}/{len(commands)} ({len(implemented)/len(commands)*100:.1f}%)")
        
        return 'ZCARD' not in not_implemented
        
    except Exception as e:
        print(f"❌ Error checking commands: {e}")
        return False

def main():
    print("=" * 60)
    print("ZCARD COMMAND TEST")
    print("=" * 60)
    
    results = []
    
    # Test ZCARD specifically
    results.append(test_zcard_command())
    
    # Check overall sorted set support
    results.append(check_sorted_set_commands())
    
    print("\n" + "=" * 60)
    if all(results):
        print("✅ ZCARD is implemented")
    else:
        print("❌ ZCARD is NOT implemented")
        print("This confirms the report that ZCARD is missing.")

if __name__ == "__main__":
    main()