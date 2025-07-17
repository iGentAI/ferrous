-- Test script for SETLIST opcode implementation
-- This script tests table array initialization with various patterns

-- Basic array initialization
local t1 = {10, 20, 30, 40, 50}
print("Basic array test:")
for i=1, #t1 do
    print(string.format("  t1[%d] = %s", i, t1[i]))
end
assert(#t1 == 5, "t1 should have 5 elements")
assert(t1[1] == 10, "t1[1] should be 10")
assert(t1[5] == 50, "t1[5] should be 50")
assert(t1[6] == nil, "t1[6] should be nil")

-- Mixed table with array and hash parts
local t2 = {
    5, 10, 15,    -- Array part
    x = 100,      -- Hash part
    20, 25,       -- More array
    y = 200,      -- More hash
}

print("\nMixed table test:")
print(string.format("  Array length: %d", #t2))
for i=1, #t2 do
    print(string.format("  t2[%d] = %s", i, t2[i]))
end
print(string.format("  t2.x = %s", t2.x))
print(string.format("  t2.y = %s", t2.y))

assert(#t2 == 5, "t2 should have 5 array elements")
assert(t2[1] == 5, "t2[1] should be 5")
assert(t2[5] == 25, "t2[5] should be 25")
assert(t2.x == 100, "t2.x should be 100")
assert(t2.y == 200, "t2.y should be 200")

-- Larger array initialization test (might trigger multiple SETLIST operations)
-- Lua 5.1 constructs arrays with SETLIST in batches of 50 elements
local t3 = {}
for i=1, 60 do
    t3[i] = i * 10
end

print("\nLarger array test:")
print(string.format("  Array length: %d", #t3))
print(string.format("  First element: t3[1] = %s", t3[1]))
print(string.format("  Element 50: t3[50] = %s", t3[50]))
print(string.format("  Element 51: t3[51] = %s", t3[51]))
print(string.format("  Last element: t3[60] = %s", t3[60]))

assert(#t3 == 60, "t3 should have 60 elements")
assert(t3[1] == 10, "t3[1] should be 10")
assert(t3[50] == 500, "t3[50] should be 500")
assert(t3[51] == 510, "t3[51] should be 510")  -- This likely goes into a second SETLIST batch
assert(t3[60] == 600, "t3[60] should be 600")

-- Dynamic array creation using sequential insertion 
-- (tests the VM's handling of dynamically growing arrays)
local function build_array(n)
    local result = {}
    for i = 1, n do
        result[i] = i * i
    end
    return result
end

local t4 = build_array(10)
print("\nDynamic array test:")
print(string.format("  Array length: %d", #t4))
print(string.format("  t4[1] = %s", t4[1]))
print(string.format("  t4[10] = %s", t4[10]))

assert(#t4 == 10, "t4 should have 10 elements")
assert(t4[1] == 1, "t4[1] should be 1")
assert(t4[10] == 100, "t4[10] should be 100")

print("\nAll SETLIST tests passed!")
return {
    basic_array_test = true,
    mixed_table_test = true,
    large_array_test = true,
    dynamic_array_test = true,
    success = true
}