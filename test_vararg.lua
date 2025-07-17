-- Test script for VARARG opcode implementation
-- This script tests different patterns of accessing variable arguments

-- Basic vararg functionality
function test1(a, b, ...)
    local x, y, z = ...
    return a, b, x, y, z
end

-- Test with exact match (3 varargs)
local a, b, x, y, z = test1(1, 2, 3, 4, 5)
print("test1(1, 2, 3, 4, 5) returns:", a, b, x, y, z)
assert(a == 1, "Expected a=1")
assert(b == 2, "Expected b=2")
assert(x == 3, "Expected x=3")
assert(y == 4, "Expected y=4")
assert(z == 5, "Expected z=5")

-- Test with fewer varargs than requested
local a, b, x, y, z = test1(1, 2, 3)
print("test1(1, 2, 3) returns:", a, b, x, y, z)
assert(a == 1, "Expected a=1")
assert(b == 2, "Expected b=2")
assert(x == 3, "Expected x=3")
assert(y == nil, "Expected y=nil")
assert(z == nil, "Expected z=nil")

-- Test with no varargs
local a, b, x, y, z = test1(1, 2)
print("test1(1, 2) returns:", a, b, x, y, z)
assert(a == 1, "Expected a=1")
assert(b == 2, "Expected b=2")
assert(x == nil, "Expected x=nil")
assert(y == nil, "Expected y=nil")
assert(z == nil, "Expected z=nil")

-- Test with extra varargs (should be ignored)
local a, b, x, y, z = test1(1, 2, 3, 4, 5, 6, 7)
print("test1(1, 2, 3, 4, 5, 6, 7) returns:", a, b, x, y, z)
assert(a == 1, "Expected a=1")
assert(b == 2, "Expected b=2")
assert(x == 3, "Expected x=3")
assert(y == 4, "Expected y=4")
assert(z == 5, "Expected z=5")

-- Test collecting all varargs (uses VARARG with B=0)
function test2(...)
    return ...
end

local results = {test2(1, 2, 3, 4, 5)}
print("test2(1, 2, 3, 4, 5) returned " .. #results .. " values:", unpack(results))
assert(#results == 5, "Expected 5 results")
assert(results[1] == 1, "Expected results[1]=1")
assert(results[2] == 2, "Expected results[2]=2")
assert(results[3] == 3, "Expected results[3]=3")
assert(results[4] == 4, "Expected results[4]=4")
assert(results[5] == 5, "Expected results[5]=5")

-- Test with no arguments (should return nothing)
local empty_results = {test2()}
print("test2() returned " .. #empty_results .. " values")
assert(#empty_results == 0, "Expected 0 results")

-- Test table construction with varargs (common pattern)
function test3(...)
    local args = {...}
    return args
end

local t = test3(10, 20, 30)
print("test3(10, 20, 30) table:", t[1], t[2], t[3])
assert(#t == 3, "Expected table of size 3")
assert(t[1] == 10, "Expected t[1]=10")
assert(t[2] == 20, "Expected t[2]=20")
assert(t[3] == 30, "Expected t[3]=30")

-- Test empty table from no varargs
local empty_t = test3()
print("test3() table size:", #empty_t)
assert(#empty_t == 0, "Expected empty table")

-- Test varargs in nested functions
function test4(...)
    local function inner()
        return ...  -- Should access outer varargs via upvalue
    end
    return inner()
end

local v1, v2 = test4("hello", "world")
print("test4('hello', 'world') returns:", v1, v2)
assert(v1 == "hello", "Expected v1='hello'")
assert(v2 == "world", "Expected v2='world'")

print("All vararg tests passed!")
return {
    success = true,
    total_tests_passed = 11
}