#!/usr/bin/env python3
"""
Ferrous Lua VM Table Field Concatenation Diagnostics

This script tests various table field access and concatenation patterns to pinpoint
specific limitations in the current Lua VM implementation. The diagnostic results
help identify which patterns work and which need improvement in the compiler
or VM implementation.

Summary of Current Behavior:
1. Simple table access (t.field) works correctly
2. Simple string field concatenation (t.str .. " world") works
3. Using local variables as intermediaries sometimes works
4. Multiple field access in a single expression fails
5. Direct number field concatenation fails

This script serves as both a test and diagnostic tool, as well as
documentation of the current VM limitations.
"""

import socket
import time

def encode_eval(script):
    """Encode a Lua script as a proper RESP EVAL command"""
    script_bytes = script.encode('utf-8')
    return f"*3\r\n$4\r\nEVAL\r\n${len(script_bytes)}\r\n{script}\r\n$1\r\n0\r\n".encode('utf-8')

def send_command(cmd):
    """Send a command to the Redis server and get the response"""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(2)
    s.connect(('localhost', 6379))
    s.sendall(cmd)
    resp = b""
    start = time.time()
    while time.time() - start < 1.0:
        try:
            chunk = s.recv(1024)
            if not chunk:
                break
            resp += chunk
            if resp.endswith(b'\r\n'):
                break
        except socket.timeout:
            break
    s.close()
    return resp

def test_script(script, desc):
    """Test a Lua script and print the results with description"""
    print(f"\nTesting {desc}:\n{script}")
    cmd = encode_eval(script)
    resp = send_command(cmd)
    print(f"Response: {resp}")
    return resp

print("===== Table Field Concatenation Debug =====\n")
print("This test script documents the current limitations of table field handling in Ferrous Lua VM")
print("The results help pinpoint which areas of the VM or compiler need further improvement")

# Test simple number access - WORKS
test_script(
    "local t = {num=42}; return t.num",
    "Simple number field access"
)

# Test simple string+string concatenation - WORKS
test_script(
    "return 'Value: ' .. 'hello'", 
    "String + string concat"
)

# Test string+number concatenation - WORKS
test_script(
    "return 'Number: ' .. 42",
    "String + number concat"
)

# Test number field without concatenation - WORKS
test_script(
    "local t = {num=42}; local n = t.num; return n",
    "Just number field to local"
)

# Test string field without concatenation - WORKS
test_script(
    "local t = {str='hello'}; local s = t.str; return s",
    "Just string field to local"
)

# Test string field + literal via local - SHOULD WORK 
test_script(
    "local t = {str='hello'}; local s = t.str; return s .. ' world'",
    "String field + literal via local"
)

# Test direct string field concatenation - WORKS
test_script(
    "local t = {str='hello'}; return t.str .. ' world'",
    "Direct string field concat"
)

# Number field + literal concatenation via local - WORKS
test_script(
    "local t = {num=42}; local n = t.num; return 'Number: ' .. n",
    "Number field + literal via local"
)

# Direct number field concatenation - FAILS
# Issue: Type error when trying to concatenate number fields directly
test_script(
    "local t = {num=42}; return 'Number: ' .. t.num",
    "Direct number field concat"
)

print("\n===== Multiple Field Tests =====\n")
print("These tests identify issues with multiple table field access in concatenation expressions")

# Multiple fields via locals - FAILS
# Issue: The table reference is lost after the first field access
test_script(
    "local t = {a='hello', b='world'}; local x = t.a; local y = t.b; return x .. ' ' .. y",
    "Multi field concat via locals"
)

# Direct multiple field concatenation - FAILS
# Issue: Table reference is lost during multiple concatenation operations
test_script(
    "local t = {a='hello', b='world'}; return t.a .. ' ' .. t.b",
    "Direct multi field concat"
)

# Mixed string and number fields - FAILS
# Issue: Combines both table reference loss and number field concatenation issue
test_script(
    "local t = {str='hello', num=42}; return t.str .. ' ' .. t.num",
    "String field + number field"
)

# Case with temp variables - FAILS
# Issue: This is the specific use case we're trying to fix - Problem case from the original test
test_script(
    "local t = {foo='bar', baz=42}; local s1 = t.foo; local s2 = t.baz; return s1 .. ' ' .. s2",
    "Test case with temp variables"
)

print("\n===== Implementation Notes =====")
print("To fix these issues, focus on these areas in the VM and compiler:")
print("1. Ensure table references are preserved throughout concatenation operations")
print("2. Improve register handling to prevent losing table context")
print("3. Fix number field concatenation type handling in the VM")
print("4. Ensure the VM can handle multiple field access from the same table")
print("5. Consider a two-phase approach: first extract all needed values, then concatenate")