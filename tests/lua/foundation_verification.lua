-- Foundation verification test
-- Tests the critical features of string interning and table operations

print("Starting foundation verification test...")

-- Test 1: String interning with literals and concatenation
local s1 = "test"
local s2 = "test"
local s3 = "te".."st"

print("String interning test:")
print("s1 == s2:", s1 == s2)
print("s1 == s3:", s1 == s3)
local string_test_passed = (s1 == s2) and (s1 == s3)
print("String test passed:", string_test_passed)

-- Test 2: Table field access with string keys
local t = {}
t[s1] = "value1"
print("\nTable string keys test:")
print("t[s1] = ", t[s1])
print("t[s2] = ", t[s2])
print("t[s3] = ", t[s3])
local table_test_passed = (t[s1] == "value1") and (t[s2] == "value1") and (t[s3] == "value1")
print("Table test passed:", table_test_passed)

-- Test 3: Globals access with string literals
print("\nGlobal table access test:")
local print_fn = _G["print"]
print("Global lookup success:", print_fn == print)
local global_test_passed = (print_fn == print)
print("Global test passed:", global_test_passed)

-- Return overall success status
return string_test_passed and table_test_passed and global_test_passed
