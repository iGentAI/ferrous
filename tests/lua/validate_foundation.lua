-- Validation test for string interning and table operations
print("Starting foundation validation test...")

-- Test string interning with table keys
local t = {}

-- Create key by concatenation
local key1 = "test".."-".."key"
print("key1 =", key1)

-- Create same key directly
local key2 = "test-key"
print("key2 =", key2)

-- Verify they're equal
print("key1 == key2:", key1 == key2)

-- Set value with first key
t[key1] = "value1"
print("Set t[key1] = 'value1'")

-- Get value with second key
print("t[key2] =", t[key2])

-- Test table references
local t1 = {}
t1.value = "inner value"

local t2 = {}
t2.ref = t1

-- Access through reference
print("t2.ref.value =", t2.ref.value)

-- Test _G access
local print_func = _G.print
print_func("Using print function from _G")

return t2.ref == t1 and t[key1] == t[key2] and key1 == key2
