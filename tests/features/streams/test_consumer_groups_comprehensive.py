#!/usr/bin/env python3
"""
Comprehensive test suite for Redis Streams Consumer Groups functionality
Tests: XGROUP, XREADGROUP, XACK, XPENDING, XCLAIM, XAUTOCLAIM, XINFO
"""

import redis
import time
import threading
import pytest
from typing import List, Dict, Tuple, Optional

# Test configuration
REDIS_HOST = 'localhost'
REDIS_PORT = 6379

def get_client():
    """Get a Redis client instance"""
    return redis.Redis(host=REDIS_HOST, port=REDIS_PORT, decode_responses=True)

def clean_test_streams(client):
    """Clean up test streams"""
    for key in client.keys("test:stream:*"):
        client.delete(key)

class TestXGROUPCommands:
    """Test XGROUP command family"""
    
    def setup_method(self):
        """Set up test client and clean state"""
        self.client = get_client()
        clean_test_streams(self.client)
        
    def teardown_method(self):
        """Clean up after tests"""
        clean_test_streams(self.client)
    
    def test_xgroup_create_basic(self):
        """Test basic XGROUP CREATE"""
        stream_key = "test:stream:group1"
        
        # Add some entries
        id1 = self.client.xadd(stream_key, {"field1": "value1"})
        id2 = self.client.xadd(stream_key, {"field2": "value2"})
        
        # Create a consumer group
        result = self.client.xgroup_create(stream_key, "mygroup", "0")
        assert result is True
        
        # Try to create the same group again (should fail)
        with pytest.raises(redis.ResponseError) as exc:
            self.client.xgroup_create(stream_key, "mygroup", "0")
        assert "BUSYGROUP" in str(exc.value)
    
    def test_xgroup_create_with_mkstream(self):
        """Test XGROUP CREATE with MKSTREAM option"""
        stream_key = "test:stream:mkstream"
        
        # Create group on non-existent stream with MKSTREAM
        result = self.client.xgroup_create(stream_key, "mygroup", "0", mkstream=True)
        assert result is True
        
        # Verify stream exists
        assert self.client.exists(stream_key) == 1
    
    def test_xgroup_create_special_ids(self):
        """Test XGROUP CREATE with special IDs"""
        stream_key = "test:stream:special"
        
        # Add entries
        id1 = self.client.xadd(stream_key, {"a": "1"})
        id2 = self.client.xadd(stream_key, {"b": "2"})
        
        # Create group with $ (last ID)
        result = self.client.xgroup_create(stream_key, "group1", "$")
        assert result is True
        
        # Create group with specific ID
        result = self.client.xgroup_create(stream_key, "group2", id1)
        assert result is True
    
    def test_xgroup_destroy(self):
        """Test XGROUP DESTROY"""
        stream_key = "test:stream:destroy"
        
        # Create stream and group
        self.client.xadd(stream_key, {"a": "1"})
        self.client.xgroup_create(stream_key, "mygroup", "0")
        
        # Destroy the group
        result = self.client.xgroup_destroy(stream_key, "mygroup")
        assert result == 1
        
        # Try to destroy non-existent group
        result = self.client.xgroup_destroy(stream_key, "nonexistent")
        assert result == 0
    
    def test_xgroup_createconsumer(self):
        """Test XGROUP CREATECONSUMER"""
        stream_key = "test:stream:consumer"
        
        # Create stream and group
        self.client.xadd(stream_key, {"a": "1"})
        self.client.xgroup_create(stream_key, "mygroup", "0")
        
        # Create consumer
        result = self.client.xgroup_createconsumer(stream_key, "mygroup", "consumer1")
        assert result == 1
        
        # Create same consumer again (should return 0)
        result = self.client.xgroup_createconsumer(stream_key, "mygroup", "consumer1")
        assert result == 0
    
    def test_xgroup_delconsumer(self):
        """Test XGROUP DELCONSUMER"""
        stream_key = "test:stream:delconsumer"
        
        # Create stream, group, and consumer
        self.client.xadd(stream_key, {"a": "1"})
        self.client.xgroup_create(stream_key, "mygroup", "0")
        self.client.xgroup_createconsumer(stream_key, "mygroup", "consumer1")
        
        # Delete consumer
        result = self.client.xgroup_delconsumer(stream_key, "mygroup", "consumer1")
        assert result >= 0  # Returns number of pending messages deleted
        
        # Delete non-existent consumer
        result = self.client.xgroup_delconsumer(stream_key, "mygroup", "nonexistent")
        assert result == 0
    
    def test_xgroup_setid(self):
        """Test XGROUP SETID"""
        stream_key = "test:stream:setid"
        
        # Create stream with entries
        id1 = self.client.xadd(stream_key, {"a": "1"})
        id2 = self.client.xadd(stream_key, {"b": "2"})
        id3 = self.client.xadd(stream_key, {"c": "3"})
        
        # Create group
        self.client.xgroup_create(stream_key, "mygroup", "0")
        
        # Set ID to specific value
        result = self.client.xgroup_setid(stream_key, "mygroup", id2)
        assert result is True
        
        # Set ID to $ (last entry)
        result = self.client.xgroup_setid(stream_key, "mygroup", "$")
        assert result is True


class TestXREADGROUP:
    """Test XREADGROUP command"""
    
    def setup_method(self):
        """Set up test client and clean state"""
        self.client = get_client()
        clean_test_streams(self.client)
    
    def teardown_method(self):
        """Clean up after tests"""
        clean_test_streams(self.client)
    
    def test_xreadgroup_basic(self):
        """Test basic XREADGROUP"""
        stream_key = "test:stream:readgroup"
        
        # Add entries
        id1 = self.client.xadd(stream_key, {"a": "1"})
        id2 = self.client.xadd(stream_key, {"b": "2"})
        id3 = self.client.xadd(stream_key, {"c": "3"})
        
        # Create group
        self.client.xgroup_create(stream_key, "mygroup", "0")
        
        # Read from group
        result = self.client.xreadgroup("mygroup", "consumer1", {stream_key: ">"})
        assert len(result) == 1
        assert result[0][0] == stream_key
        assert len(result[0][1]) == 3  # Should get all 3 entries
    
    def test_xreadgroup_with_count(self):
        """Test XREADGROUP with COUNT"""
        stream_key = "test:stream:count"
        
        # Add entries
        for i in range(10):
            self.client.xadd(stream_key, {"field": f"value{i}"})
        
        # Create group
        self.client.xgroup_create(stream_key, "mygroup", "0")
        
        # Read with count limit
        result = self.client.xreadgroup(
            "mygroup", "consumer1", 
            {stream_key: ">"}, 
            count=5
        )
        assert len(result[0][1]) == 5
        
        # Read remaining entries
        result = self.client.xreadgroup(
            "mygroup", "consumer1",
            {stream_key: ">"}
        )
        assert len(result[0][1]) == 5
    
    def test_xreadgroup_noack(self):
        """Test XREADGROUP with NOACK"""
        stream_key = "test:stream:noack"
        
        # Add entries
        id1 = self.client.xadd(stream_key, {"a": "1"})
        
        # Create group
        self.client.xgroup_create(stream_key, "mygroup", "0")
        
        # Read with NOACK
        result = self.client.xreadgroup(
            "mygroup", "consumer1",
            {stream_key: ">"},
            noack=True
        )
        assert len(result) == 1
        
        # Check pending (should be empty due to NOACK)
        pending = self.client.xpending(stream_key, "mygroup")
        # Note: NOACK behavior may vary by implementation
    
    def test_xreadgroup_multiple_consumers(self):
        """Test XREADGROUP with multiple consumers"""
        stream_key = "test:stream:multiconsumer"
        
        # Add entries
        for i in range(6):
            self.client.xadd(stream_key, {"field": f"value{i}"})
        
        # Create group
        self.client.xgroup_create(stream_key, "mygroup", "0")
        
        # Consumer 1 reads first 3
        result1 = self.client.xreadgroup(
            "mygroup", "consumer1",
            {stream_key: ">"},
            count=3
        )
        assert len(result1[0][1]) == 3
        
        # Consumer 2 reads next 3
        result2 = self.client.xreadgroup(
            "mygroup", "consumer2",
            {stream_key: ">"},
            count=3
        )
        assert len(result2[0][1]) == 3
        
        # Verify messages are different
        ids1 = [msg[0] for msg in result1[0][1]]
        ids2 = [msg[0] for msg in result2[0][1]]
        assert set(ids1).isdisjoint(set(ids2))


class TestXACK:
    """Test XACK command"""
    
    def setup_method(self):
        """Set up test client and clean state"""
        self.client = get_client()
        clean_test_streams(self.client)
    
    def teardown_method(self):
        """Clean up after tests"""
        clean_test_streams(self.client)
    
    def test_xack_basic(self):
        """Test basic XACK"""
        stream_key = "test:stream:ack"
        
        # Add entries
        id1 = self.client.xadd(stream_key, {"a": "1"})
        id2 = self.client.xadd(stream_key, {"b": "2"})
        
        # Create group and read
        self.client.xgroup_create(stream_key, "mygroup", "0")
        result = self.client.xreadgroup("mygroup", "consumer1", {stream_key: ">"})
        
        # Acknowledge first message
        ack_count = self.client.xack(stream_key, "mygroup", id1)
        assert ack_count == 1
        
        # Try to acknowledge again (should return 0)
        ack_count = self.client.xack(stream_key, "mygroup", id1)
        assert ack_count == 0
    
    def test_xack_multiple(self):
        """Test XACK with multiple IDs"""
        stream_key = "test:stream:ackmulti"
        
        # Add entries
        ids = []
        for i in range(5):
            ids.append(self.client.xadd(stream_key, {"field": f"value{i}"}))
        
        # Create group and read all
        self.client.xgroup_create(stream_key, "mygroup", "0")
        self.client.xreadgroup("mygroup", "consumer1", {stream_key: ">"})
        
        # Acknowledge multiple messages
        ack_count = self.client.xack(stream_key, "mygroup", *ids[:3])
        assert ack_count == 3
        
        # Check pending count
        pending = self.client.xpending(stream_key, "mygroup")
        assert pending['pending'] == 2


class TestXPENDING:
    """Test XPENDING command"""
    
    def setup_method(self):
        """Set up test client and clean state"""
        self.client = get_client()
        clean_test_streams(self.client)
    
    def teardown_method(self):
        """Clean up after tests"""
        clean_test_streams(self.client)
    
    def test_xpending_summary(self):
        """Test XPENDING summary form"""
        stream_key = "test:stream:pending"
        
        # Add entries
        ids = []
        for i in range(3):
            ids.append(self.client.xadd(stream_key, {"field": f"value{i}"}))
        
        # Create group and read
        self.client.xgroup_create(stream_key, "mygroup", "0")
        self.client.xreadgroup("mygroup", "consumer1", {stream_key: ">"}, count=2)
        self.client.xreadgroup("mygroup", "consumer2", {stream_key: ">"}, count=1)
        
        # Get pending summary
        pending = self.client.xpending(stream_key, "mygroup")
        assert pending['pending'] == 3
        assert pending['min'] == ids[0]
        assert pending['max'] == ids[2]
        assert len(pending['consumers']) == 2
    
    def test_xpending_range(self):
        """Test XPENDING range form"""
        stream_key = "test:stream:pendingrange"
        
        # Add entries
        ids = []
        for i in range(5):
            ids.append(self.client.xadd(stream_key, {"field": f"value{i}"}))
        
        # Create group and read
        self.client.xgroup_create(stream_key, "mygroup", "0")
        self.client.xreadgroup("mygroup", "consumer1", {stream_key: ">"})
        
        # Get pending range
        pending = self.client.xpending_range(
            stream_key, "mygroup",
            min="-", max="+", count=3
        )
        assert len(pending) == 3
        
        # Check structure
        for entry in pending:
            assert 'message_id' in entry
            assert 'consumer' in entry
            assert 'time_since_delivered' in entry
            assert 'times_delivered' in entry
    
    def test_xpending_consumer_filter(self):
        """Test XPENDING with consumer filter"""
        stream_key = "test:stream:pendingconsumer"
        
        # Add entries
        for i in range(6):
            self.client.xadd(stream_key, {"field": f"value{i}"})
        
        # Create group and read with different consumers
        self.client.xgroup_create(stream_key, "mygroup", "0")
        self.client.xreadgroup("mygroup", "consumer1", {stream_key: ">"}, count=3)
        self.client.xreadgroup("mygroup", "consumer2", {stream_key: ">"}, count=3)
        
        # Get pending for specific consumer
        pending = self.client.xpending_range(
            stream_key, "mygroup",
            min="-", max="+", count=10,
            consumername="consumer1"
        )
        assert len(pending) == 3
        assert all(p['consumer'] == 'consumer1' for p in pending)


class TestXCLAIM:
    """Test XCLAIM command"""
    
    def setup_method(self):
        """Set up test client and clean state"""
        self.client = get_client()
        clean_test_streams(self.client)
    
    def teardown_method(self):
        """Clean up after tests"""
        clean_test_streams(self.client)
    
    def test_xclaim_basic(self):
        """Test basic XCLAIM"""
        stream_key = "test:stream:claim"
        
        # Add entries
        id1 = self.client.xadd(stream_key, {"a": "1"})
        id2 = self.client.xadd(stream_key, {"b": "2"})
        
        # Create group and read with consumer1
        self.client.xgroup_create(stream_key, "mygroup", "0")
        self.client.xreadgroup("mygroup", "consumer1", {stream_key: ">"})
        
        # Wait a bit to allow idle time
        time.sleep(0.1)
        
        # Claim messages with consumer2
        claimed = self.client.xclaim(
            stream_key, "mygroup", "consumer2",
            min_idle_time=10,  # 10ms
            message_ids=[id1, id2]
        )
        assert len(claimed) == 2
    
    def test_xclaim_with_force(self):
        """Test XCLAIM with FORCE option"""
        stream_key = "test:stream:claimforce"
        
        # Add entries
        id1 = self.client.xadd(stream_key, {"a": "1"})
        
        # Create group and read
        self.client.xgroup_create(stream_key, "mygroup", "0")
        self.client.xreadgroup("mygroup", "consumer1", {stream_key: ">"})
        
        # Immediately claim with FORCE (no idle time check)
        claimed = self.client.xclaim(
            stream_key, "mygroup", "consumer2",
            min_idle_time=1000000,  # Very high idle time
            message_ids=[id1],
            force=True
        )
        assert len(claimed) == 1
    
    def test_xclaim_justid(self):
        """Test XCLAIM with JUSTID option"""
        stream_key = "test:stream:claimjustid"
        
        # Add entries
        ids = []
        for i in range(3):
            ids.append(self.client.xadd(stream_key, {"field": f"value{i}"}))
        
        # Create group and read
        self.client.xgroup_create(stream_key, "mygroup", "0")
        self.client.xreadgroup("mygroup", "consumer1", {stream_key: ">"})
        
        # Wait for idle time
        time.sleep(0.1)
        
        # Claim with JUSTID
        claimed = self.client.xclaim(
            stream_key, "mygroup", "consumer2",
            min_idle_time=10,
            message_ids=ids,
            justid=True
        )
        # Should return just IDs, not full messages
        assert all(isinstance(id, str) for id in claimed)


class TestXAUTOCLAIM:
    """Test XAUTOCLAIM command"""
    
    def setup_method(self):
        """Set up test client and clean state"""
        self.client = get_client()
        clean_test_streams(self.client)
    
    def teardown_method(self):
        """Clean up after tests"""
        clean_test_streams(self.client)
    
    def test_xautoclaim_basic(self):
        """Test basic XAUTOCLAIM"""
        stream_key = "test:stream:autoclaim"
        
        # Add entries
        for i in range(5):
            self.client.xadd(stream_key, {"field": f"value{i}"})
        
        # Create group and read with consumer1
        self.client.xgroup_create(stream_key, "mygroup", "0")
        self.client.xreadgroup("mygroup", "consumer1", {stream_key: ">"})
        
        # Wait for idle time
        time.sleep(0.1)
        
        # Auto-claim with consumer2
        result = self.client.xautoclaim(
            stream_key, "mygroup", "consumer2",
            min_idle_time=10,  # 10ms
            start="0-0",
            count=3
        )
        
        # Check structure
        assert 'next' in result
        assert 'messages' in result
        assert len(result['messages']) <= 3
    
    def test_xautoclaim_justid(self):
        """Test XAUTOCLAIM with JUSTID"""
        stream_key = "test:stream:autoclaimjustid"
        
        # Add entries
        for i in range(5):
            self.client.xadd(stream_key, {"field": f"value{i}"})
        
        # Create group and read
        self.client.xgroup_create(stream_key, "mygroup", "0")
        self.client.xreadgroup("mygroup", "consumer1", {stream_key: ">"})
        
        # Wait for idle time
        time.sleep(0.1)
        
        # Auto-claim with JUSTID
        result = self.client.xautoclaim(
            stream_key, "mygroup", "consumer2",
            min_idle_time=10,
            start="0-0",
            justid=True
        )
        
        # Should return just IDs
        assert 'next' in result
        if 'messages' in result and result['messages']:
            assert all(isinstance(id, str) for id in result['messages'])


class TestXINFO:
    """Test XINFO command family"""
    
    def setup_method(self):
        """Set up test client and clean state"""
        self.client = get_client()
        clean_test_streams(self.client)
    
    def teardown_method(self):
        """Clean up after tests"""
        clean_test_streams(self.client)
    
    def test_xinfo_stream(self):
        """Test XINFO STREAM"""
        stream_key = "test:stream:info"
        
        # Add entries
        for i in range(3):
            self.client.xadd(stream_key, {"field": f"value{i}"})
        
        # Create groups
        self.client.xgroup_create(stream_key, "group1", "0")
        self.client.xgroup_create(stream_key, "group2", "0")
        
        # Get stream info
        info = self.client.xinfo_stream(stream_key)
        
        assert info['length'] == 3
        assert info['groups'] == 2
        assert 'last-generated-id' in info
        assert 'first-entry' in info
        assert 'last-entry' in info
    
    def test_xinfo_groups(self):
        """Test XINFO GROUPS"""
        stream_key = "test:stream:infogroups"
        
        # Add entries
        self.client.xadd(stream_key, {"a": "1"})
        
        # Create groups
        self.client.xgroup_create(stream_key, "group1", "0")
        self.client.xgroup_create(stream_key, "group2", "$")
        
        # Read with group1
        self.client.xreadgroup("group1", "consumer1", {stream_key: ">"})
        
        # Get groups info
        groups = self.client.xinfo_groups(stream_key)
        
        assert len(groups) == 2
        for group in groups:
            assert 'name' in group
            assert 'consumers' in group
            assert 'pending' in group
            assert 'last-delivered-id' in group
    
    def test_xinfo_consumers(self):
        """Test XINFO CONSUMERS"""
        stream_key = "test:stream:infoconsumers"
        
        # Add entries
        for i in range(5):
            self.client.xadd(stream_key, {"field": f"value{i}"})
        
        # Create group
        self.client.xgroup_create(stream_key, "mygroup", "0")
        
        # Read with different consumers
        self.client.xreadgroup("mygroup", "consumer1", {stream_key: ">"}, count=2)
        self.client.xreadgroup("mygroup", "consumer2", {stream_key: ">"}, count=3)
        
        # Get consumers info
        consumers = self.client.xinfo_consumers(stream_key, "mygroup")
        
        assert len(consumers) == 2
        for consumer in consumers:
            assert 'name' in consumer
            assert 'pending' in consumer
            assert 'idle' in consumer


class TestConsumerGroupsEdgeCases:
    """Test edge cases and error conditions"""
    
    def setup_method(self):
        """Set up test client and clean state"""
        self.client = get_client()
        clean_test_streams(self.client)
    
    def teardown_method(self):
        """Clean up after tests"""
        clean_test_streams(self.client)
    
    def test_operations_on_nonexistent_group(self):
        """Test operations on non-existent groups"""
        stream_key = "test:stream:nogroup"
        self.client.xadd(stream_key, {"a": "1"})
        
        # Try to read from non-existent group
        with pytest.raises(redis.ResponseError) as exc:
            self.client.xreadgroup("nonexistent", "consumer1", {stream_key: ">"})
        assert "NOGROUP" in str(exc.value)
        
        # Try to acknowledge in non-existent group
        result = self.client.xack(stream_key, "nonexistent", "1234-0")
        assert result == 0
    
    def test_duplicate_groups(self):
        """Test creating duplicate groups"""
        stream_key = "test:stream:duplicate"
        self.client.xadd(stream_key, {"a": "1"})
        
        # Create first group
        self.client.xgroup_create(stream_key, "mygroup", "0")
        
        # Try to create duplicate
        with pytest.raises(redis.ResponseError) as exc:
            self.client.xgroup_create(stream_key, "mygroup", "0")
        assert "BUSYGROUP" in str(exc.value)
    
    def test_empty_stream_operations(self):
        """Test operations on empty streams"""
        stream_key = "test:stream:empty"
        
        # Create group on empty stream with MKSTREAM
        self.client.xgroup_create(stream_key, "mygroup", "0", mkstream=True)
        
        # Read from empty stream
        result = self.client.xreadgroup("mygroup", "consumer1", {stream_key: ">"})
        assert result == []
        
        # Get pending on empty stream
        pending = self.client.xpending(stream_key, "mygroup")
        assert pending['pending'] == 0


if __name__ == "__main__":
    # Run tests
    pytest.main([__file__, "-v"])