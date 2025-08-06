#!/usr/bin/env python3
"""
Ferrous Pub/Sub protocol validation test suite.

This script performs protocol-level tests against a Ferrous (Redis-compatible)
server to ensure that its PUB/SUB implementation conforms to the RESP2 wire
format and is compatible with the official `redis-py` client.

It covers:

* Low-level RESP2 validation of SUBSCRIBE, PSUBSCRIBE and published messages
* Publish/subscribe round-trip checks using raw sockets
* Functional tests using the redis-py high-level client
* Multi-channel subscription confirmation

Exit status is **0** when all tests pass, otherwise **1**.
"""

import socket
import sys
import time
import threading
from typing import Any, List

import redis


# ────────────────────────── RESP2 helpers ──────────────────────────
class PubSubProtocolTester:
    def __init__(self, host: str = "127.0.0.1", port: int = 6379) -> None:
        self.host = host
        self.port = port

    # ------------------------ low-level parser ------------------------
    def parse_resp(self, data: bytes) -> Any:
        """Parse a single RESP2 value from *data* or return ``None`` if
        the buffer is incomplete."""
        if not data:
            return None

        kind = data[0:1]

        # Simple string / Error / Integer
        if kind in (b"+", b"-", b":"):
            end = data.find(b"\r\n")
            if end == -1:
                return None
            payload = data[1:end]
            if kind == b"+":
                return payload.decode()
            if kind == b"-":
                return f"ERROR: {payload.decode()}"
            return int(payload)

        # Bulk string
        if kind == b"$":
            end = data.find(b"\r\n")
            if end == -1:
                return None
            length = int(data[1:end])
            if length == -1:
                return None  # NULL bulk string
            start = end + 2
            end_of_payload = start + length
            if len(data) < end_of_payload + 2:
                return None
            return data[start:end_of_payload]

        # Array
        if kind == b"*":
            end = data.find(b"\r\n")
            if end == -1:
                return None
            count = int(data[1:end])
            pos = end + 2
            items: List[Any] = []
            for _ in range(count):
                elem_len = self._element_length(data[pos:])
                if elem_len == -1:
                    return None
                items.append(self.parse_resp(data[pos : pos + elem_len]))
                pos += elem_len
            return items

        return None

    def _element_length(self, data: bytes) -> int:
        """Return the byte length of the first RESP element in *data* (including
        its terminator) or -1 if incomplete."""
        if not data:
            return -1

        kind = data[0:1]

        if kind in (b"+", b"-", b":"):
            end = data.find(b"\r\n")
            return -1 if end == -1 else end + 2

        if kind == b"$":
            end = data.find(b"\r\n")
            if end == -1:
                return -1
            length = int(data[1:end])
            if length == -1:
                return end + 2
            return end + 2 + length + 2

        if kind == b"*":
            end = data.find(b"\r\n")
            if end == -1:
                return -1
            count = int(data[1:end])
            pos = end + 2
            for _ in range(count):
                elem_len = self._element_length(data[pos:])
                if elem_len == -1:
                    return -1
                pos += elem_len
            return pos

        return -1

    # ----------------------------- tests -----------------------------
    def test_subscribe_response_format(self) -> bool:
        """SUBSCRIBE confirmation must be: ['subscribe', channel, <n>]"""
        print("Testing SUBSCRIBE response format …")
        with socket.create_connection((self.host, self.port), timeout=2) as s:
            s.sendall(b"*2\r\n$9\r\nSUBSCRIBE\r\n$12\r\ntest_channel\r\n")
            resp = s.recv(1024)
            parsed = self.parse_resp(resp)
            print("Parsed:", parsed)
            ok = (
                isinstance(parsed, list)
                and len(parsed) >= 3
                and parsed[0] in (b"subscribe", "subscribe")
                and parsed[1] in (b"test_channel", "test_channel")
                and isinstance(parsed[2], int)
                and parsed[2] >= 1
            )
            print("✅" if ok else "❌", "SUBSCRIBE response format")
            return ok

    def test_publish_message_format(self) -> bool:
        """Published payload must be: ['message', channel, data]"""
        print("\nTesting PUBLISH message format …")
        with socket.create_connection((self.host, self.port), timeout=3) as sub:
            sub.sendall(b"*2\r\n$9\r\nSUBSCRIBE\r\n$11\r\npubsub_test\r\n")
            _ = sub.recv(1024)  # ignore confirmation
            time.sleep(0.3)

            with socket.create_connection((self.host, self.port)) as pub:
                pub.sendall(
                    b"*3\r\n$7\r\nPUBLISH\r\n$11\r\npubsub_test\r\n$10\r\ntest_value\r\n"
                )
                _ = pub.recv(64)

            msg = sub.recv(1024)
            parsed = self.parse_resp(msg)
            print("Parsed:", parsed)
            ok = (
                isinstance(parsed, list)
                and len(parsed) >= 3
                and parsed[0] in (b"message", "message")
                and parsed[1] in (b"pubsub_test", "pubsub_test")
                and parsed[2] in (b"test_value", "test_value")
            )
            print("✅" if ok else "❌", "PUBLISH message format")
            return ok

    def test_pattern_subscribe_format(self) -> bool:
        """PSUBSCRIBE confirmation must be: ['psubscribe', pattern, <n>]"""
        print("\nTesting PSUBSCRIBE response format …")
        with socket.create_connection((self.host, self.port), timeout=2) as s:
            s.sendall(b"*2\r\n$10\r\nPSUBSCRIBE\r\n$7\r\ntest:*\r\n")
            parsed = self.parse_resp(s.recv(1024))
            print("Parsed:", parsed)
            ok = (
                isinstance(parsed, list)
                and len(parsed) >= 3
                and parsed[0] in (b"psubscribe", "psubscribe")
                and parsed[1] in (b"test:*", "test:*")
                and isinstance(parsed[2], int)
            )
            print("✅" if ok else "❌", "PSUBSCRIBE response format")
            return ok

    def test_redis_py_compatibility(self) -> bool:
        """High-level redis-py client must work end-to-end."""
        print("\nTesting redis-py client compatibility …")
        try:
            r = redis.Redis(host=self.host, port=self.port, decode_responses=True)
            pubsub = r.pubsub()
            pubsub.subscribe("redis_py_test")

            sub_msg = pubsub.get_message(timeout=2)
            if not sub_msg or sub_msg.get("type") != "subscribe":
                print("❌ No valid subscription confirmation")
                return False

            r.publish("redis_py_test", "hello from redis-py")
            pub_msg = pubsub.get_message(timeout=2)
            ok = pub_msg and pub_msg.get("type") == "message"
            print("✅" if ok else "❌", "redis-py round-trip")
            return ok
        except Exception as exc:
            print("❌ redis-py error:", exc)
            return False
        finally:
            try:
                pubsub.close()
            except Exception:
                pass

    # ---------------------- run whole suite -------------------------
    def run_all(self) -> List[bool]:
        tests = [
            self.test_subscribe_response_format,
            self.test_publish_message_format,
            self.test_pattern_subscribe_format,
            self.test_redis_py_compatibility,
        ]
        return [t() for t in tests]


# ───────────────────────── extra cross-check ─────────────────────────
def test_multiple_channel_subscribe(
    host: str = "127.0.0.1", port: int = 6379
) -> bool:
    """redis-py should return one confirmation per channel."""
    print("\nTesting multi-channel subscribe …")
    channels = ["ch1", "ch2", "ch3"]

    try:
        r = redis.Redis(host=host, port=port, decode_responses=False)
        pubsub = r.pubsub()
        pubsub.subscribe(*channels)

        confirmations = [pubsub.get_message(timeout=1) for _ in channels]
        confirmations = [m for m in confirmations if m]

        ok = len(confirmations) == len(channels)
        print("✅" if ok else "❌", f"{len(confirmations)}/{len(channels)} confirmations")
        return ok
    except Exception as exc:
        print("❌ multi-channel error:", exc)
        return False
    finally:
        try:
            pubsub.close()
        except Exception:
            pass


# ──────────────────────────────── main ────────────────────────────────
def main() -> None:
    print("=" * 70)
    print("FERROUS PUB/SUB PROTOCOL VALIDATION TEST SUITE")
    print("=" * 70)

    # Quick connectivity ping
    try:
        with socket.create_connection(("127.0.0.1", 6379), timeout=1) as s:
            s.sendall(b"*1\r\n$4\r\nPING\r\n")
            if b"PONG" not in s.recv(16):
                raise RuntimeError("PING failed")
        print("✅ Server connection verified\n")
    except Exception as exc:
        print("❌ Cannot connect to server:", exc)
        sys.exit(1)

    tester = PubSubProtocolTester()
    results = tester.run_all()
    results.append(test_multiple_channel_subscribe())

    passed = sum(results)
    total = len(results)

    print("\n" + "=" * 70)
    print(f"RESULT: {passed}/{total} tests passed")
    print("=" * 70)
    sys.exit(0 if passed == total else 1)


if __name__ == "__main__":
    main()
